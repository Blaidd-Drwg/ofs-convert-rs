#!/usr/bin/env bash
mkdir -p "$1/dir/dir2"
dd if=/dev/zero bs=512 count=1 > "$1/small_file"
dd if=/dev/zero bs=512 count=3 > "$1/dir/file"
dd if=/dev/zero bs=1024 count=8193 > "$1/dir/dir2/large_file"
