#[macro_use]
extern crate serde_derive;
extern crate docopt;
extern crate rustfft;

mod dsp;

use docopt::Docopt;
use rustfft::FFTplanner;
use rustfft::num_complex::Complex;
use rustfft::num_traits::Zero;
use std::fs::File;
use std::io::BufReader;
use std::io::prelude::*;

use dsp::Biquad;
use dsp::iir;


const USAGE: &str = "
ALSA results analysis

Usage:
  analysis <input> <filtered-1> <filtered-2> <fft> <filtered-fft-1> <filtered-fft-2>
  analysis (-h | --help)

Options:
  -h --help         Show this screen.
  <input>           Input data file.
  <filtered-1>      Filtered input data.
  <filtered-2>      Filtered input data.
  <fft>             Output FFT from input data.
  <filtered-fft-1>  Output FFT from filtered data.
  <filtered-fft-2>  Output FFT from filtered data.
";


#[derive(Debug, Deserialize)]
struct Args {
    arg_input: String,
    arg_filtered_1: String,
    arg_filtered_2: String,
    arg_fft: String,
    arg_filtered_fft_1: String,
    arg_filtered_fft_2: String,
}

fn get_fft(data: &[f64]) -> Vec<f64> {
    let mut fft_in: Vec<Complex<f64>> = data
        .iter()
        .map(|value| Complex::new(*value, 0.0))
        .collect();

    let mut fft_out: Vec<Complex<f64>> = vec![Complex::zero(); data.len()];

    let mut planner = FFTplanner::new(false);
    let fft = planner.plan_fft(data.len());
    fft.process(&mut fft_in, &mut fft_out);

    let mut fft_vals = fft_out
        .iter()
        .map(|c| c.re.abs())
        .collect::<Vec<f64>>();

    fft_vals.truncate(data.len() / 2);
    fft_vals
}

fn write_data(file_name: &str, data: &[f64], period_time: f64) {
    eprintln!("{} average: {}", file_name, data.iter().sum::<f64>() / data.len() as f64);
    let mut file = File::create(file_name).unwrap();
    let mut time = 0.0;
    for i in 0..data.len() {
        writeln!(file, "{} {}", time, data[i]).unwrap();
        time += period_time;
    }
}

fn write_fft(file_name: &str, fft_data: &[f64]) {
    let mut file = File::create(file_name).unwrap();
    for i in 0..fft_data.len() {
        writeln!(file, "{} {}", i as f64 / (fft_data.len() - 1) as f64, fft_data[i]).unwrap();
    }
}

fn get_biquad(magic: f64, q: f64) -> Biquad {
    let omega = 2.0 * std::f64::consts::PI * magic;
    let cos_omega = omega.cos();
    let alpha = omega.sin() / (2.0 * q);

    let b0 = (1.0 - cos_omega) / 2.0;
    let b1 = 1.0 - cos_omega;
    let b2 = (1.0 - cos_omega) / 2.0;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cos_omega;
    let a2 = 1.0 - alpha;

    let mut bq = Biquad::default();

    bq.b0 = b0 / a0;
    bq.b1 = b1 / a0;
    bq.b2 = b2 / a0;
    bq.a1 = -a1 / a0;
    bq.a2 = -a2 / a0;

    // bq.print();

    bq
}

fn main() {
    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());


    let s = &args.arg_input.split(".dat")
        .collect::<Vec<&str>>()[0]
        .split("_")
        .collect::<Vec<&str>>();
    let sample_rate: u32 = s[s.len() - 3].parse().unwrap();
    let period_size: u32 = s[s.len() - 2].parse().unwrap();
    let period_count: u32 = s[s.len() - 1].parse().unwrap();
    let period_time = 1.0 / sample_rate as f64 * period_size as f64;

    eprintln!("period_size: {}, period_count: {}, period_time {}",
              period_size,
              period_count,
              period_time);

    let file = File::open(&args.arg_input).unwrap();
    let mut buf_reader = BufReader::new(file);
    let mut contents = String::new();
    buf_reader.read_to_string(&mut contents).unwrap();

    let skip_seconds = 0.5;
    let fade_seconds = 1.0;

    let skip = (sample_rate as f64 * skip_seconds / period_size as f64) as usize;
    let fade = (sample_rate as f64 * fade_seconds / period_size as f64) as usize;

    let mut data: Vec<f64> = contents
        .lines()
        .map(|l| l.parse().unwrap())
        .skip(skip)
        .collect();

    // fade in the data
    for i in 0..fade {
        let coef = i as f64 / (fade - 1) as f64;
        data[i] = data[i] * coef;
    }

    write_data("/tmp/data.dat", &data, period_time);
    let fft = get_fft(&data);
    write_fft(&args.arg_fft, &fft);


    let mut filtered_data = vec![0.0; data.len()];

    /*
     * Biquad 1
     */
    let magic = period_time / 20.0;
    // let magic =  0.00004;
    // let magic = 40.0 / 1e6;
    println!("magic * 48000 : {}", magic * 48000.0);
    let q = std::f64::consts::FRAC_1_SQRT_2;

    let mut bq = get_biquad(magic, q);

    iir(&data.clone(), &mut filtered_data, &mut bq);
    let fft = get_fft(&filtered_data);
    write_data(&args.arg_filtered_1, &filtered_data, period_time);
    write_fft(&args.arg_filtered_fft_1, &fft);

    /*
     * Biquad 2
     */

    /*
        let magic = period_size as f64 / sample_rate as f64 / 10.0;
        let q = std::f64::consts::FRAC_1_SQRT_2;

        let mut bq = get_biquad(magic, q);

        dsp::iir(&filtered_data.clone(), &mut filtered_data, &mut bq);
        let fft = get_fft(&filtered_data);
        write_data(&args.arg_filtered_2, &filtered_data, period_time);
        write_fft(&args.arg_filtered_fft_2, &fft);
    */
}