#!/bin/bash
# Development run script

cd "$(dirname "$0")"

# Enable logging
export RUST_LOG=info

echo "Starting Rhythm PI Client (Development Mode)..."
echo ""

# Check if we need to build first
if [ ! -f "target/debug/rhythm-pi-client" ]; then
    echo "Building client..."
    cargo build
    echo ""
fi

# Run with debug info
exec cargo run
