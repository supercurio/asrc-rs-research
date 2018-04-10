#[macro_use]
extern crate serde_derive;
extern crate docopt;
extern crate alsa;
extern crate thread_priority;

mod realtime_priority;

use docopt::Docopt;
use alsa::{Direction, ValueOr};
use alsa::pcm::{PCM, HwParams, Format, Access, State};

const USAGE: &str = "
ALSA simple loopback

Usage:
  alsa-simple-loopback [--capture-device=<alsa-device> --playback-device=<alsa-device> --channels=<nr> --capture-period-size=<frames> --capture-periods=<count> --playback-period-size=<frames> --playback-periods=<count> --capture-sample-rate=<Hz> --playback-sample-rate=<Hz>]
  alsa-simple-loopback (-h | --help)

Options:
  -h --help                         Show this screen.
  --capture-device=<alsa-device>    ALSA device to record from [default: default]
  --playback-device=<alsa-device>   ALSA device to playback to [default: default]
  --channels=<nr>                   Channels to capture and play [default: 2]
  --capture-period-size=<frames>    Size of capture frames [default: 32].
  --capture-periods=<count>         Amount of recording periods [default: 2].
  --playback-period-size=<frames>   Size of playback frames [default: 32].
  --playback-periods=<count>        Amount of playback periods [default: 2].
  --capture-sample-rate=<Hz>        Recording sample rate [default: 48000].
  --playback-sample-rate=<Hz>       Playback sample rate [default: 48000].
";


#[derive(Debug, Deserialize)]
struct Args {
    flag_capture_device: String,
    flag_playback_device: String,
    flag_channels: u32,
    flag_capture_period_size: usize,
    flag_capture_periods: u32,
    flag_playback_period_size: usize,
    flag_playback_periods: u32,
    flag_capture_sample_rate: u32,
    flag_playback_sample_rate: u32,
}

fn main() {
    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());

    eprintln!("Capture\n  card:    {}\n  rate:    {}\n  period:  {}\n  periods: {}",
              args.flag_capture_device,
              args.flag_capture_sample_rate,
              args.flag_capture_period_size,
              args.flag_capture_periods);
    eprintln!("Playback\n  card:    {}\n  rate:    {}\n  period:  {}\n  periods: {}",
              args.flag_playback_device,
              args.flag_playback_sample_rate,
              args.flag_playback_period_size,
              args.flag_playback_periods);

    let pcm_capture =
        setup_card(Direction::Capture,
                   args.flag_capture_device,
                   args.flag_channels,
                   args.flag_capture_sample_rate,
                   args.flag_capture_period_size,
                   args.flag_capture_periods);

    let pcm_playback =
        setup_card(Direction::Playback,
                   args.flag_playback_device,
                   args.flag_channels,
                   args.flag_playback_sample_rate,
                   args.flag_playback_period_size,
                   args.flag_playback_periods);

    let hwp = pcm_playback.hw_params_current().unwrap();
    let swp = pcm_playback.sw_params_current().unwrap();
    let start_threshold = hwp.get_buffer_size().unwrap() - hwp.get_period_size().unwrap();
    eprintln!("Playback start threshold: {}", start_threshold);
    swp.set_start_threshold(start_threshold).unwrap();
    pcm_playback.sw_params(&swp).unwrap();

    let io_capture = pcm_capture.io_i32().unwrap();
    let io_playback = pcm_playback.io_i32().unwrap();

    let period_buffer_size = get_period_buffer_size(&pcm_playback);
    eprintln!("IO buffer size: {}", period_buffer_size);

    let mut buf = vec![0; period_buffer_size];

    realtime_priority::get_realtime_priority();

    loop {
        let capture_state = pcm_capture.state();
        if capture_state != State::Running { eprintln!("Capture state: {:?}", capture_state); }
        if capture_state == State::XRun {
            eprintln!("Prepare capture");
            pcm_capture.prepare().unwrap();
        }

        if let Err(_) = io_capture.readi(&mut buf) {
            pcm_capture.prepare().unwrap();
        }

        let playback_state = pcm_playback.state();
        if playback_state != State::Running { eprintln!("Playback state: {:?}", playback_state); }
        if playback_state == State::XRun {
            eprintln!("Prepare playback");
            pcm_playback.prepare().unwrap();
        }

        if let Err(_) = io_playback.writei(&buf) {
            pcm_playback.prepare().unwrap();
        }
    }
}

fn setup_card(direction: Direction,
              device: String,
              channels: u32,
              sample_rate: u32,
              period_size: usize,
              periods: u32) -> PCM {
    let pcm = PCM::new(&device, direction, false).unwrap();
    {
        let hwp = HwParams::any(&pcm).unwrap();
        hwp.set_channels(channels).unwrap();
        hwp.set_rate(sample_rate, ValueOr::Nearest).unwrap();
        hwp.set_format(Format::s32()).unwrap();
        hwp.set_access(Access::RWInterleaved).unwrap();
        #[cfg(target_pointer_width = "32")]
        hwp.set_period_size(period_size as i32, ValueOr::Nearest).unwrap();
        #[cfg(target_pointer_width = "64")]
        hwp.set_period_size(period_size as i64, ValueOr::Nearest).unwrap();
        hwp.set_periods(periods, ValueOr::Nearest).unwrap();
        pcm.hw_params(&hwp).unwrap();
        let hwp = pcm.hw_params_current().unwrap();
        let period_size = hwp.get_period_size().unwrap() as usize;
        let buffer_size = hwp.get_buffer_size().unwrap() as usize;
        eprintln!("Card period size: {}, HW buffer size: {}", period_size, buffer_size);
    }

    pcm
}

fn get_period_buffer_size(pcm: &alsa::pcm::PCM) -> usize {
    let hwp = pcm.hw_params_current().unwrap();
    hwp.get_period_size().unwrap() as usize * hwp.get_channels().unwrap() as usize
}
