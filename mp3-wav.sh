#!/bin/bash

# Check if ffmpeg is installed
if ! command -v ffmpeg &> /dev/null
then
    echo "ffmpeg is required but not installed. Install it first."
    exit 1
fi

# Check if input is provided
if [ -z "$1" ]; then
    echo "Usage: $0 <input_file.mp3>"
    exit 1
fi

INPUT="$1"

# Check if file exists
if [ ! -f "$INPUT" ]; then
    echo "File '$INPUT' not found!"
    exit 1
fi

# Generate output filename by replacing .mp3 with .wav
OUTPUT="${INPUT%.mp3}.wav"

# Convert mp3 to wav
ffmpeg -i "$INPUT" -ar 44100 -ac 2 -sample_fmt s16 "$OUTPUT"

echo "Conversion complete: $OUTPUT"
