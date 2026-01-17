#!/bin/sh
cargo run -p rhythm-pi-client --release
pkill rhythm-pi-client || true