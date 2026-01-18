#!/bin/sh
cargo run -p rhythm-pi-charter --release -- "$@"
pkill rhythm-pi-charter || true