#[macro_use]
extern crate serde_derive;
extern crate docopt;
extern crate alsa;
extern crate alsa_sys;
extern crate time;
extern crate thread_priority;
extern crate libc;

mod realtime_priority;

use std::thread;
use std::time::Duration;
use std::process;
use docopt::Docopt;
use alsa::{Direction, ValueOr};
use alsa::pcm::{PCM, HwParams, Format, Access, Status};
use libc::timespec;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

const USAGE: &str = "
ALSA audio_time in Rust

Usage:
  alsa-audio-time [-p -c -D <device> -t <type> -r <Hz> -s <frames> -o <periods> -f <Hz>]
  alsa-audio-time (-h | --help)

Options:
  -h --help                     Show this screen.
  -p --playback                 Playback tstamps
  -c --capture                  Capture tstamps.
  -D --device=<device>          Select ALSA device [default: hw:0,0].
  -d --delay=<enable>           Enable delay compensation [default: false]
  -t --ts-type=<type>           Default(0),link(1),link_estimated(2),synchronized(3) [default: 0].
  -f --ts-freq=<Hz>             Timestamp Frequency [default: 0].
  -s --period-size=<frames>     Period size in frames [default: 256].
  -o --periods=<count>          Periods [default: 4].
  -r --sample-rate=<Hz>         Recording sample rate [default: 48000].
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
    flag_ts_freq: f64,
    flag_period_size: u32,
    flag_periods: u32,
    flag_delay: bool,
    flag_sample_rate: u32,
}

#[derive(Debug)]
enum TimeStampType {
    Default,
    Link,
    LinkEstimated,
    Synchronized,
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

    let mut handle_p: Option<Arc<PCM>> = None;
    let mut handle_c: Option<Arc<PCM>> = None;
    let mut buffer_c = vec![0i16; (period_size * periods * CHANNELS) as usize];
    let buffer_p = vec![0i16; (period_size * periods * CHANNELS) as usize];
    let xruns_p = Arc::new(AtomicUsize::new(0));
    let xruns_c = Arc::new(AtomicUsize::new(0));
    let frames_count_p: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));
    let frames_count_c: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));

    // let q : () = handle_p;

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

        handle_p = Some(Arc::new(pcm));
    }

    if args.flag_capture {
        let mut pcm = PCM::new(&args.flag_device, Direction::Capture, false).unwrap();
        set_params(&mut pcm, args.flag_sample_rate, period_size, periods);
        handle_c = Some(Arc::new(pcm));
    }


    if args.flag_ts_freq != 0.0 {
        let timestamp_freq = args.flag_ts_freq;
        let handle_p_clone = handle_p.clone();
        let handle_c_clone = handle_c.clone();
        let frames_count_p_clone = frames_count_p.clone();
        let frames_count_c_clone = frames_count_c.clone();
        let xruns_p_clone = xruns_p.clone();
        let xruns_c_clone = xruns_c.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(1));
            let sleep_duration = Duration::new(0, (1e9 / timestamp_freq) as u32);
            loop {
                thread::sleep(sleep_duration);
                if let Some(pcm_c) = handle_c_clone.as_ref() {
                    eprint!("Capture   xruns: {}  ", xruns_c_clone.load(Ordering::Relaxed));
                    let frames_count_lock = frames_count_c_clone.lock().unwrap();
                    let status = pcm_c.status().unwrap();
                    print_timestamp(&status, *frames_count_lock);
                }
                if let Some(pcm_p) = handle_p_clone.as_ref() {
                    eprint!("Playback  xruns: {}  ", xruns_p_clone.load(Ordering::Relaxed));
                    let frames_count_lock = frames_count_p_clone.lock().unwrap();
                    let status = pcm_p.status().unwrap();
                    print_timestamp(&status, *frames_count_lock);
                }
            };
        });
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
                *frames_count_p.lock().unwrap() += frames;
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
            pcm_c.wait(None).unwrap();

            let io = pcm_c.io_i16().unwrap();
            let res = io.readi(&mut buffer_c);

            match res {
                Ok(len) => {
                    *frames_count_p.lock().unwrap() += len as u64;
                    if args.flag_ts_freq == 0.0 {
                        eprint!("Capture   xruns: {}  ", xruns_c.load(Ordering::SeqCst));
                        let status = pcm_c.status().unwrap();
                        print_timestamp(&status, *frames_count_p.lock().unwrap());
                    }
                }
                Err(e) => {
                    eprintln!("Recovering from Capture error");
                    pcm_c.try_recover(e, false).unwrap();
                    pcm_c.start().unwrap();
                    xruns_c.fetch_add(1, Ordering::SeqCst);
                }
            }
        }

        if let Some(pcm_p) = handle_p.as_ref() {
            let io = pcm_p.io_i16().unwrap();

            let res = io.writei(&buffer_p);
            match res {
                Ok(len) => {
                    *frames_count_p.lock().unwrap() += len as u64;
                    if args.flag_ts_freq == 0.0 {
                        eprint!("Playback  xruns: {}  ", xruns_p.load(Ordering::SeqCst));
                        let status = pcm_p.status().unwrap();
                        print_timestamp(&status, *frames_count_p.lock().unwrap());
                    }
                }
                Err(e) => {
                    eprintln!("Recovered from Playback error");
                    pcm_p.try_recover(e, false).unwrap();
                    xruns_p.fetch_add(1, Ordering::SeqCst);
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
    eprint!("audio_htstamp: {}  ", format_timespec(status.get_audio_htstamp()));
    eprintln!("htstamp: {}", format_timespec(status.get_htstamp()));
}

fn format_timespec(ts: timespec) -> String {
    format!("{}.{:<9}", ts.tv_sec as u64, ts.tv_nsec as u64)
}