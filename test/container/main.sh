#!/usr/bin/env bash
# To avoid showing warnings multiple times, we redirect stderr to /dev/null every time we run `cargo` except when we build the tests
export CARGO_TARGET_DIR=/target
cd /project_root
printf     "########################### BUILD ###########################\n\n"
cargo test --no-run || exit 1
cargo build 2>/dev/null || exit 1
printf "\n\n######################## UNIT TESTS #########################\n\n"
cargo test 2>/dev/null || TEST_FAILED=true
printf "\n\n##################### UNIT TESTS (SUDO) #####################\n\n"
cargo test-sudo 2>/dev/null || TEST_FAILED=true
printf "\n\n##################### INTEGRATION TESTS #####################\n\n"
cd /test
python3 run.py "$CARGO_TARGET_DIR/debug/ofs-convert-rs" tests || TEST_FAILED=true

if [ $TEST_FAILED ]; then
	exit 1
fi
