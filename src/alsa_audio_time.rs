#[macro_use]
extern crate serde_derive;
extern crate docopt;
extern crate alsa;
extern crate alsa_sys;
extern crate time;
extern crate thread_priority;
extern crate libc;

mod realtime_priority;

use std::process;
use docopt::Docopt;
use alsa::{Direction, ValueOr};
use alsa::pcm::{PCM, HwParams, Format, Access, Status};
use libc::timespec;
use std::fs::File;
use std::io::prelude::*;

const USAGE: &str = "
ALSA audio_time in Rust

Usage:
  alsa-audio-time [-p -c -D <device> -t <type> -r <Hz> -s <frames> -o <periods> -w <fname>]
  alsa-audio-time (-h | --help)

Options:
  -h --help                     Show this screen.
  -p --playback                 Playback tstamps
  -c --capture                  Capture tstamps.
  -D --device=<device>          Select ALSA device [default: hw:0,0].
  -d --delay=<enable>           Enable delay compensation [default: false]
  -t --ts-type=<type>           Default(0),link(1),link_estimated(2),synchronized(3) [default: 0].
  -s --period-size=<frames>     Period size in frames [default: 256].
  -o --periods=<count>          Periods [default: 4].
  -r --sample-rate=<Hz>         Recording sample rate [default: 48000].
  -w --write-to-file=<fname>    Write timestamps to file.
";

const CHANNELS: u32 = 2;
const PCM_LINK: bool = false;
const PRE_FILL_P: bool = false;

#[derive(Debug, Deserialize)]
struct Args {
    flag_playback: bool,
    flag_capture: bool,
    flag_device: String,
    flag_ts_type: u32,
    flag_period_size: u32,
    flag_periods: u32,
    flag_delay: bool,
    flag_sample_rate: u32,
    flag_write_to_file: Option<String>,
}

#[derive(Debug)]
enum TimeStampType {
    Default,
    Link,
    LinkEstimated,
    Synchronized,
}

struct PreviousStatus {
    audio_htstamp: timespec,
    htstamp: timespec,
    captured_frames: u64,
}

fn main() {
    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());

    let ts_type = match args.flag_ts_type {
        1 => TimeStampType::Link,
        2 => TimeStampType::LinkEstimated,
        3 => TimeStampType::Synchronized,
        _ => TimeStampType::Default,
    };

    if !args.flag_capture && !args.flag_playback {
        eprintln!("{}", USAGE);
        eprintln!("Error: please enable capture, playback or both.");
        process::exit(1);
    }

    eprintln!("Timestamp type: {:?}", ts_type);
    if args.flag_capture {
        eprintln!("Capture from:   {}", args.flag_device);
    }
    if args.flag_playback {
        eprintln!("Playback from:  {}", args.flag_device);
    }

    let period_size = args.flag_period_size;
    let periods = args.flag_periods;
    eprintln!("Period size:    {}", period_size);
    eprintln!("Periods:        {}", periods);
    eprintln!("Sample rate:    {}", args.flag_sample_rate);

    let mut handle_p: Option<PCM> = None;
    let mut handle_c: Option<PCM> = None;
    let mut buffer_c = vec![0i16; (period_size * periods * CHANNELS) as usize];
    let buffer_p = vec![0i16; (period_size * periods * CHANNELS) as usize];
    let mut xruns_p = 0;
    let mut xruns_c = 0;
    let mut frames_count_p: u64 = 0;
    let mut frames_count_c: u64 = 0;
    let mut last_status_c: Option<PreviousStatus> = None;
    let mut last_status_p: Option<PreviousStatus> = None;

    let mut out_file = args.flag_write_to_file.map(|f| File::create(f).unwrap());

    if args.flag_playback {
        let mut pcm = PCM::new(&args.flag_device, Direction::Playback, false).unwrap();
        set_params(&mut pcm, args.flag_sample_rate, period_size, periods);
        {
            let hwp = pcm.hw_params_current().unwrap();
            let start_threshold = hwp.get_buffer_size().unwrap() - hwp.get_period_size().unwrap();

            let swp = pcm.sw_params_current().unwrap();
            swp.set_start_threshold(start_threshold).unwrap();
            pcm.sw_params(&swp).unwrap();
        }

        handle_p = Some(pcm);
    }

    if args.flag_capture {
        let mut pcm = PCM::new(&args.flag_device, Direction::Capture, false).unwrap();
        set_params(&mut pcm, args.flag_sample_rate, period_size, periods);
        handle_c = Some(pcm);
    }

    if PCM_LINK {
        if let (Some(_pcm_p), Some(_pcm_c)) = (handle_p.as_ref(),
                                               handle_c.as_ref()) {
            // TODO: link both capture and playback PCM
        }
    }

    // fill playback buffer with zeroes to start
    if PRE_FILL_P {
        if let Some(pcm_p) = handle_p.as_ref() {
            let io = pcm_p.io_i16().unwrap();
            for _ in 0..periods {
                let frames = io.writei(&buffer_p).unwrap() as u64;
                frames_count_p += frames;
            }
        }
    }

    if let Some(pcm_c) = handle_c.as_ref() {
        if !PCM_LINK || PCM_LINK && !args.flag_playback {
            // need to start capture explicitly
            pcm_c.start().unwrap();
        }
    }

    realtime_priority::get_realtime_priority();

    loop {
        if let Some(pcm_c) = handle_c.as_ref() {
            if let Err(e) = pcm_c.wait(None) {
                eprintln!("Recovering from Capture wait error");
                pcm_c.try_recover(e, false).unwrap();
                pcm_c.start().unwrap();
                xruns_c += 1;
                frames_count_c = 0;
                last_status_c = None;
            }

            let io = pcm_c.io_i16().unwrap();

            match io.readi(&mut buffer_c) {
                Ok(len) => {
                    frames_count_c += len as u64;
                    eprint!("Capture   xruns: {}  ", xruns_c);
                    let status = pcm_c.status().unwrap();
                    if let Some(file) = out_file.as_mut() {
                        write_timestamp_capture(file, &status, &mut last_status_c, frames_count_c);
                    }
                    print_timestamp(&status, frames_count_c);
                }
                Err(e) => {
                    eprintln!("Recovering from Capture error");
                    pcm_c.try_recover(e, false).unwrap();
                    pcm_c.start().unwrap();
                    xruns_c += 1;
                    frames_count_c = 0;
                    last_status_c = None;
                }
            }
        }

        if let Some(pcm_p) = handle_p.as_ref() {
            let io = pcm_p.io_i16().unwrap();

            match io.writei(&buffer_p) {
                Ok(len) => {
                    frames_count_p += len as u64;
                    eprint!("Playback  xruns: {}  ", xruns_p);
                    let status = pcm_p.status().unwrap();
                    if let Some(file) = out_file.as_mut() {
                        write_timestamp_playback(file, &status, frames_count_p);
                    }
                    print_timestamp(&status, frames_count_p);
                }
                Err(e) => {
                    eprintln!("Recovered from Playback error");
                    pcm_p.try_recover(e, false).unwrap();
                    xruns_p += 1;
                    frames_count_p = 0;
                    last_status_p = None;
                }
            }
        }
    }
}


fn set_params(pcm: &mut PCM, sample_rate: u32, period_size: u32, periods: u32) {
    let hwp = HwParams::any(&pcm).unwrap();
    hwp.set_channels(2).unwrap();
    hwp.set_rate(sample_rate, ValueOr::Nearest).unwrap();
    hwp.set_format(Format::s16()).unwrap();
    hwp.set_access(Access::RWInterleaved).unwrap();
    #[cfg(target_pointer_width = "32")]
        hwp.set_period_size(period_size as i32, ValueOr::Nearest).unwrap();
    #[cfg(target_pointer_width = "64")]
        hwp.set_period_size(period_size as i64, ValueOr::Nearest).unwrap();
    hwp.set_periods(periods, ValueOr::Nearest).unwrap();
    pcm.hw_params(&hwp).unwrap();

    let swp = pcm.sw_params_current().unwrap();
    swp.set_tstamp_mode(true).unwrap();
// TODO: also set timestamp type
    pcm.sw_params(&swp).unwrap();
}

fn print_timestamp(status: &Status, frames_count: u64) {
    eprint!("delay: {:5}  ", status.get_delay());
    eprint!("avail: {:5}  ", status.get_avail());
    eprint!("avail_max: {:5}  ", status.get_avail_max());
    eprint!("frames: {}  ", frames_count);

    let audio_htstamp = timespec_f64(status.get_audio_htstamp());
    let trigger_htstamp = timespec_f64(status.get_trigger_htstamp());
    let htstamp = timespec_f64(status.get_htstamp());
    let drift = htstamp - trigger_htstamp - audio_htstamp;

    eprint!("audio_htstamp: {:<18}  ", audio_htstamp);
    eprint!("trigger_htstamp: {:<18}  ", trigger_htstamp);
    eprint!("htstamp: {:<18}", htstamp);
    eprintln!("drift: {:<18}", drift);
}

fn write_timestamp_capture(file: &mut File,
                           status: &Status,
                           last_status: &mut Option<PreviousStatus>,
                           frames_count: u64) {
    let audio_elapsed = timespec_f64(status.get_audio_htstamp());
    let trigger_tstamp = timespec_f64(status.get_trigger_htstamp());
    let system_tstamp = timespec_f64(status.get_htstamp());

    let system_elapsed = system_tstamp - trigger_tstamp;
    let captured_frames = frames_count + status.get_delay() as u64;

    if let Some(last_status) = last_status.as_ref() {
        let captured_frames_from_last = captured_frames - last_status.captured_frames;
        let system_elapsed_from_last = system_tstamp - timespec_f64(last_status.htstamp);
        let audio_elapsed_from_last = audio_elapsed - timespec_f64(last_status.audio_htstamp);

        let audio_rate_instant = captured_frames_from_last as f64 / audio_elapsed_from_last;
        let system_rate_instant = captured_frames_from_last as f64 / system_elapsed_from_last;

        let drift = system_tstamp - trigger_tstamp - audio_elapsed;

        // write!(file, "{} ", audio_elapsed).unwrap();
        // write!(file, "{} ", captured_frames).unwrap();
        // write!(file, "{} ", audio_rate_instant).unwrap();
        // write!(file, "{} ", system_rate_instant).unwrap();
        writeln!(file, "{}", system_rate_instant).unwrap();
        // write!(file, "{} ", audio_rate_instant / system_rate_instant).unwrap();
        // write!(file, "{} ", 48000.0 / system_rate_instant).unwrap();
        //writeln!(file, "{}", drift).unwrap();
    }

    let saved_status = PreviousStatus {
        audio_htstamp: status.get_audio_htstamp(),
        htstamp: status.get_htstamp(),
        captured_frames,
    };
    *last_status = Some(saved_status);
}

fn write_timestamp_playback(file: &mut File, status: &Status, frames_count: u64) {
    let audio_elapsed = timespec_f64(status.get_audio_htstamp());
    let trigger_tstamp = timespec_f64(status.get_trigger_htstamp());
    let system_tstamp = timespec_f64(status.get_htstamp());

    let system_elapsed = system_tstamp - trigger_tstamp;
    let played_frames = audio_elapsed * 48000.0;

    let audio_rate = played_frames as f64 / audio_elapsed;
    let system_rate = played_frames as f64 / system_elapsed;

    let drift = system_tstamp - trigger_tstamp - audio_elapsed;

    write!(file, "{} ", system_elapsed).unwrap();
    write!(file, "{} ", played_frames).unwrap();
    write!(file, "{} ", audio_rate).unwrap();
    write!(file, "{} ", system_rate).unwrap();
    writeln!(file, "{}", drift).unwrap();
}

fn timespec_f64(ts: timespec) -> f64 {
    ts.tv_sec as f64 + (ts.tv_nsec as f64) / 1e9
}