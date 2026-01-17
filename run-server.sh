#!/bin/sh
cargo run -p rhythm-pi-server --bin rhythm-pi-server --release
pkill rhythm-pi-server || true