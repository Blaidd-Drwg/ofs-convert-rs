#!/usr/bin/env bash
dd if=/dev/zero of="$1/file1" bs=1024 count=1
mkdir "$1/dir"
dd if=/dev/zero of="$1/file2" bs=1024 count=1

dd if=/dev/zero of="$1/dir/file1" bs=1024 count=1
mkdir "$1/dir/dir"
dd if=/dev/zero of="$1/dir/file2" bs=1024 count=1

dd if=/dev/zero of="$1/dir/dir/file1" bs=1024 count=1
mkdir "$1/dir/dir/dir"
dd if=/dev/zero of="$1/dir/dir/file2" bs=1024 count=1

dd if=/dev/zero of="$1/dir/dir/dir/file" bs=1024 count=1
