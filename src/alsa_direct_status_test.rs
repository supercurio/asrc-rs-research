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
use alsa::pcm::{PCM, HwParams, Format, Access};
use alsa::direct::pcm::Status;
use alsa::direct::pcm::SyncPtrStatus;
use libc::timespec;

const USAGE: &str = "
alsa-direct-status-test

Usage:
  alsa-audio-time [-p -c -D <device> -r <Hz> -s <frames> -o <periods> -f <Hz>]
  alsa-audio-time (-h | --help)

Options:
  -h --help                     Show this screen.
  -p --playback                 Playback tstamps
  -c --capture                  Capture tstamps.
  -D --device=<device>          Select ALSA device [default: hw:0,0].
  -s --period-size=<frames>     Period size in frames [default: 256].
  -o --periods=<count>          Periods [default: 4].
  -r --sample-rate=<Hz>         Recording sample rate [default: 48000].
  -f --status-freq=<Hz>         Status Frequency [default: 10].
";

const CHANNELS: u32 = 2;

#[derive(Debug, Deserialize)]
struct Args {
    flag_playback: bool,
    flag_capture: bool,
    flag_device: String,
    flag_period_size: u32,
    flag_periods: u32,
    flag_sample_rate: u32,
    flag_status_freq: f64,
}

fn main() {
    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());

    if !args.flag_capture && !args.flag_playback {
        eprintln!("{}", USAGE);
        eprintln!("Error: please enable capture, playback or both.");
        process::exit(1);
    }

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
    let mut sync_status_p: Option<SyncPtrStatus> = None;
    let mut sync_status_c: Option<SyncPtrStatus> = None;
    let mut status_p: Option<Status> = None;
    let mut status_c: Option<Status> = None;
    let mut buffer_c = vec![0i16; (period_size * periods * CHANNELS) as usize];
    let buffer_p = vec![0i16; (period_size * periods * CHANNELS) as usize];

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

        sync_status_p = Some(unsafe {
            SyncPtrStatus::sync_ptr(
                alsa::direct::pcm::pcm_to_fd(&pcm).unwrap(),
                false,
                None,
                None).unwrap()
        });
        if cfg!(target_arch = "x86_64" ) {
            status_p = Some(Status::new(&pcm).unwrap());
        }
        handle_p = Some(pcm);
    }

    if args.flag_capture {
        let mut pcm = PCM::new(&args.flag_device, Direction::Capture, false).unwrap();
        set_params(&mut pcm, args.flag_sample_rate, period_size, periods);

        sync_status_c = Some(unsafe {
            SyncPtrStatus::sync_ptr(
                alsa::direct::pcm::pcm_to_fd(&pcm).unwrap(),
                false,
                None,
                None).unwrap()
        });

        if cfg!(target_arch = "x86_64" ) {
            status_c = Some(Status::new(&pcm).unwrap());
        }
        handle_c = Some(pcm);
    }


    let status_freq = args.flag_status_freq;

    thread::spawn(move || {
        thread::sleep(Duration::from_secs(1));
        let sleep_duration = Duration::new(0, (1e9 / status_freq) as u32);
        loop {
            thread::sleep(sleep_duration);

            if let Some(status) = status_c.as_ref() {
                print_status(&status);
            }
            if let Some(status) = sync_status_c.as_ref() {
                print_sync_status(&status);
            }
            if let Some(status) = status_p.as_ref() {
                print_status(&status);
            }
            if let Some(status) = sync_status_p.as_ref() {
                print_sync_status(&status);
            }
        };
    });

    realtime_priority::get_realtime_priority();

    if let Some(pcm_c) = handle_c.as_ref() {
        pcm_c.start().unwrap();
    }


    loop {
        if let Some(pcm_c) = handle_c.as_ref() {
            if let Err(e) = pcm_c.wait(None) {
                eprintln!("Recovering from Capture wait error");
                pcm_c.try_recover(e, false).unwrap();
                pcm_c.start().unwrap();
            }

            let io = pcm_c.io_i16().unwrap();

            if let Err(e) = io.readi(&mut buffer_c) {
                eprintln!("Recovering from Capture error");
                pcm_c.try_recover(e, false).unwrap();
                pcm_c.start().unwrap();
            }
        }

        if let Some(pcm_p) = handle_p.as_ref() {
            let io = pcm_p.io_i16().unwrap();

            if let Err(e) = io.writei(&buffer_p) {
                eprintln!("Recovered from Playback error");
                pcm_p.try_recover(e, false).unwrap();
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

fn print_sync_status(status: &SyncPtrStatus) {
    eprint!("Sync Status: state: {:?}  ", status.state());
    eprintln!("htstamp: {:<18}  ", timespec_f64(status.htstamp()));
}

fn print_status(status: &Status) {
    eprint!("Status:      state: {:?}  ", status.state());
    eprint!("htstamp: {:<18}  ", timespec_f64(status.htstamp()));
    eprintln!("audio_htstamp: {:<18}  ", timespec_f64(status.audio_htstamp()));
}

fn timespec_f64(ts: timespec) -> f64 {
    ts.tv_sec as f64 + (ts.tv_nsec as f64) / 1e9
}