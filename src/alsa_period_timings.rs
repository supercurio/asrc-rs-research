#[macro_use]
extern crate serde_derive;
extern crate docopt;
extern crate alsa;
extern crate time;
extern crate thread_priority;

mod realtime_priority;

use std::process;

use docopt::Docopt;
use alsa::{Direction, ValueOr};
use alsa::pcm::{PCM, HwParams, Format, Access, IO};

const USAGE: &str = "
ALSA capture and playback period timer

Usage:
  alsa-period-timing <mode> [--duration=<seconds> --capture-device=<alsa-device> --playback-device=<alsa-device> --capture-buffer-size=<frames> --channels=<nr> --capture-period-size=<frames> --capture-periods=<count> --playback-period-size=<frames> --playback-periods=<count> --sample-rate=<Hz>]
  alsa-period-timing (-h | --help)

Options:
  -h --help                         Show this screen.
  <mode>                            Mode: capture, playback or capture_playback
  --duration=<seconds>              Record duration in seconds [default: 5]
  --capture-device=<alsa-device>    ALSA device to record from [default: default]
  --playback-device=<alsa-device>   ALSA device to playback to [default: default]
  --channels=<nr>                   Channels to capture and play [default: 2]
  --capture-period-size=<frames>    Size of capture frames [default: 128].
  --playback-period-size=<frames>   Size of playback frames [default: 128].
  --capture-periods=<count>         Amount of recording periods [default: 2].
  --playback-periods=<count>        Amount of playback periods [default: 2].
  --sample-rate=<Hz>                Recording sample rate [default: 48000].
";

#[derive(Debug, Deserialize)]
struct Args {
    arg_mode: String,
    flag_duration: u64,
    flag_capture_device: String,
    flag_playback_device: String,
    flag_channels: u32,
    flag_capture_period_size: usize,
    flag_capture_periods: u32,
    flag_playback_period_size: usize,
    flag_playback_periods: u32,
    flag_sample_rate: u32,
}

fn main() {
    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());

    match args.arg_mode.as_ref() {
        "capture" => {
            let device = args.flag_capture_device;
            eprintln!("Capture  {}, {} Hz, {} frames * {}",
                      device, args.flag_sample_rate, args.flag_capture_period_size,
                      args.flag_capture_periods);

            let pcm = PCM::new(&device, Direction::Capture, false).unwrap();
            let hwp = HwParams::any(&pcm).unwrap();
            hwp.set_channels(args.flag_channels).unwrap();
            hwp.set_rate(args.flag_sample_rate, ValueOr::Nearest).unwrap();
            hwp.set_format(Format::s16()).unwrap();
            hwp.set_access(Access::RWInterleaved).unwrap();
            #[cfg(target_pointer_width = "32")]
            hwp.set_period_size(args.flag_capture_period_size as i32, ValueOr::Nearest).unwrap();
            #[cfg(target_pointer_width = "64")]
            hwp.set_period_size(args.flag_capture_period_size as i64, ValueOr::Nearest).unwrap();
            hwp.set_periods(args.flag_capture_periods, ValueOr::Nearest).unwrap();
            pcm.hw_params(&hwp).unwrap();
            let io = pcm.io_i16().unwrap();

            let hwp = pcm.hw_params_current().unwrap();
            let period_size = hwp.get_period_size().unwrap() as usize;
            let buffer_size = hwp.get_buffer_size().unwrap() as usize;
            eprintln!("Capture period size: {}, HW buffer size: {}", period_size, buffer_size);

            let buf = vec![0; period_size * args.flag_channels as usize];
            card_vs_systime(buf,
                            io,
                            Direction::Capture,
                            args.flag_sample_rate,
                            args.flag_duration);
        }

        "playback" => {
            let device = args.flag_playback_device;
            eprintln!("Playback {}, {} Hz, {} frames * {}",
                      device, args.flag_sample_rate, args.flag_playback_period_size,
                      args.flag_playback_periods);

            let pcm = PCM::new(&device, Direction::Playback, false).unwrap();
            let hwp = HwParams::any(&pcm).unwrap();
            hwp.set_channels(args.flag_channels).unwrap();
            hwp.set_rate(args.flag_sample_rate, ValueOr::Nearest).unwrap();
            hwp.set_format(Format::s16()).unwrap();
            hwp.set_access(Access::RWInterleaved).unwrap();
            #[cfg(target_pointer_width = "32")]
            hwp.set_period_size(args.flag_playback_period_size as i32, ValueOr::Nearest).unwrap();
            #[cfg(target_pointer_width = "64")]
            hwp.set_period_size(args.flag_playback_period_size as i64, ValueOr::Nearest).unwrap();
            hwp.set_periods(args.flag_playback_periods, ValueOr::Nearest).unwrap();
            pcm.hw_params(&hwp).unwrap();
            let io = pcm.io_i16().unwrap();

            let hwp = pcm.hw_params_current().unwrap();
            let period_size = hwp.get_period_size().unwrap() as usize;
            let buffer_size = hwp.get_buffer_size().unwrap() as usize;
            eprintln!("Playback period size: {}, HW buffer size: {}", period_size, buffer_size);

            let buf = vec![0; period_size * args.flag_channels as usize];
            card_vs_systime(buf,
                            io,
                            Direction::Playback,
                            args.flag_sample_rate,
                            args.flag_duration);
        }
        _ => {
            eprintln!("No valid mode specified: {}", args.arg_mode);
            process::exit(2);
        }
    }
}

fn card_vs_systime(mut rec_buf: Vec<i16>,
                   io: IO<i16>,
                   direction: Direction,
                   sample_rate: u32,
                   duration_s: u64) {
    realtime_priority::get_realtime_priority();

    let start_ns = time::precise_time_ns();
    let mut time_ns = start_ns;
    loop {
        let read = match direction {
            Direction::Capture => io.readi(&mut rec_buf),
            Direction::Playback => io.writei(&rec_buf),
        };
        let now_ns = time::precise_time_ns();
        let elapsed_ns = now_ns - time_ns;
        time_ns = now_ns;
        match read {
            Ok(frames) => {
                let period_time_reference = frames as f64 / sample_rate as f64 * 1e6;
                println!("{}", elapsed_ns as f64 / 1e3 - period_time_reference);
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                process::exit(1);
            }
        };
        if now_ns - start_ns > duration_s * 1_000_000_000 {
            break;
        }
    }
}

fn time_average(mut rec_buf: Vec<i16>, io: IO<i16>, sample_rate: u32) {
    let mut captured_sample_count: u64 = 0;
    let mut start_ns = 0;

    loop {
        let read = io.readi(rec_buf.as_mut_slice());
        match read {
            Ok(size) => {
                if start_ns == 0 {
                    start_ns = time::precise_time_ns();
                    captured_sample_count = 0;
                    continue;
                }

                let time_ref = time::precise_time_ns() - start_ns;
                captured_sample_count += size as u64;

                let time_ref_sample_count = time_ref as f64 * 1e-9 * sample_rate as f64;
                let ratio = captured_sample_count as f64 / time_ref_sample_count;

// println!("{} {}", time_ref_sample_count, captured_sample_count);
                println!("{} 1", ratio);
            }
            Err(e) => eprintln!("Error: {}", e),
        };
    }
}
