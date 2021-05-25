#!/usr/bin/env bash
CARGO_TARGET_DIR=/target
rsync -r /dependencies/target/ "$CARGO_TARGET_DIR"
cd /project_root
cargo build --target-dir "$CARGO_TARGET_DIR"
cd /test
python3 run.py /target/debug/ofs-convert-rs tests
