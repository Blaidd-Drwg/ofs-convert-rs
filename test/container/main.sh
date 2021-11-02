#!/usr/bin/env bash
export CARGO_TARGET_DIR=/target
rsync -r /dependencies/target/ "$CARGO_TARGET_DIR"
cd /project_root
cargo build || exit 1
cargo test --no-run || exit 1
printf "\n\n######################## UNIT TESTS #########################\n\n"
cargo test
printf "\n\n##################### UNIT TESTS (SUDO) #####################\n\n"
cargo test-sudo
printf "\n\n##################### INTEGRATION TESTS #####################\n\n"
cd /test
python3 run.py "$CARGO_TARGET_DIR/debug/ofs-convert-rs" tests
