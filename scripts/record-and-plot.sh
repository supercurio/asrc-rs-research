#!/bin/sh

CARD=$1
FRAME_SIZE=$2
FRAMES=$3

CAPTURE_OUT="/tmp/data-$CARD-$FRAME_SIZE-$FRAMES".dat

cargo build --release || exit

./target/release/alsa-capture-playback \
	--capture-device="$CARD" \
	--capture-period-size="$FRAME_SIZE" \
	--capture-periods="$FRAMES" \
	--channels=2 > "$CAPTURE_OUT"

./target/release/analysis "$CAPTURE_OUT" \
    "/tmp/filtered-1.dat" \
    "/tmp/filtered-2.dat" \
    "/tmp/fft.dat" \
    "/tmp/filtered-fft-1.dat" \
    "/tmp/filtered-fft-2.dat"

./scripts/plot-filtered-fft.sh
