#!/bin/bash
# Generate charts for all instruments of a song

set -e

# Configuration
AUDIO_FILE="${1:?Usage: $0 <audio.wav> <song_id> [output_dir]}"
SONG_ID="${2:?Usage: $0 <audio.wav> <song_id> [output_dir]}"
OUTPUT_DIR="${3:-.}"

CHARTER="./target/release/rhythm-pi-charter"

echo "Building charter..."
cargo build -p rhythm-pi-charter --release

echo "Generating charts for: $SONG_ID"
echo "Audio: $AUDIO_FILE"
echo "Output: $OUTPUT_DIR"
echo ""

INSTRUMENTS=("vocals" "bass" "drums" "lead")
TOTAL=$((${#INSTRUMENTS[@]} * 4)) # 4 difficulties each
CURRENT=0

for INSTRUMENT in "${INSTRUMENTS[@]}"; do
    echo "Generating $INSTRUMENT charts..."
    
    $CHARTER \
        --audio "$AUDIO_FILE" \
        --song-id "$SONG_ID" \
        --instrument "$INSTRUMENT" \
        --output "$OUTPUT_DIR" \
        --verbose \
        2>&1 | grep -E "Saved|Generated|Summary" || true
    
    echo ""
done

echo "All charts generated!"
echo ""
echo "Generated files:"
ls -lh "$OUTPUT_DIR"/${SONG_ID// /_}_*.json 2>/dev/null | head -20

echo ""
echo "Summary:"
for INSTRUMENT in "${INSTRUMENTS[@]}"; do
    COUNT=$(ls "$OUTPUT_DIR"/${SONG_ID// /_}_${INSTRUMENT}_*.json 2>/dev/null | wc -l)
    if [ "$COUNT" -eq 4 ]; then
        echo "  ✓ $INSTRUMENT: 4 difficulties"
    fi
done

