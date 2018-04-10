#!/bin/sh

MODE=$1
CARD_NAME=$2
CARD=$3
RATE=$4
CHANNELS=$5
cargo build --release --bin alsa-period-timings || exit

do_alsa() {
    FRAME_SIZE=$1
    FRAMES=$2

    OUT_FILE="/tmp/data_${CARD_NAME}_${RATE}_${FRAME_SIZE}_${FRAMES}.dat"

    if [ "$MODE" = "capture" ]; then
        echo "\nRecording to $OUT_FILE:\n"

        sudo nice -n -20 ./target/release/alsa-period-timings "$MODE" \
            --duration=300 \
            --sample-rate="$RATE" \
            --channels="$CHANNELS" \
            --capture-device="$CARD" \
            --capture-period-size="$FRAME_SIZE" \
            --capture-periods="$FRAMES" > "$OUT_FILE"
    elif [ "$MODE" = "playback" ]; then
        echo "\nPlaying to $OUT_FILE:\n"

        sudo nice -n -20 ./target/release/alsa-period-timings "$MODE" \
            --duration=300 \
            --sample-rate="$RATE" \
            --channels="$CHANNELS" \
            --playback-device="$CARD" \
            --playback-period-size="$FRAME_SIZE" \
            --playback-periods="$FRAMES" > "$OUT_FILE"
    fi
}

for x in 8 16 32 48 64 128 256 512 1024 2048 4096; do
    do_alsa "$x" 2
done

for x in 8 16 32 48 64 128 256 512 1024; do
    do_alsa "$x" 3
    do_alsa "$x" 4
done


