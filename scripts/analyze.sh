#!/bin/sh

cargo build --release --bin analysis
./target/release/analysis $1 /tmp/filtered-1.dat /tmp/filtered-2.dat /tmp/fft.dat /tmp/filtered-fft-1.dat /tmp/filtered-fft-2.dat
