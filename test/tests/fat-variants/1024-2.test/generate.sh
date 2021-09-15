#!/usr/bin/env bash
mkdir -p "$1/dir/dir2"
dd if=/dev/urandom bs=1024 count=1 > "$1/small_file"
dd if=/dev/urandom bs=1024 count=3 > "$1/dir/file"
dd if=/dev/urandom bs=2048 count=16385 > "$1/dir/dir2/large_file"
