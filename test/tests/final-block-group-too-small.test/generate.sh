#!/usr/bin/env bash
dd if=/dev/urandom of="$1/file1" bs=1024 count=100
dd if=/dev/urandom of="$1/file2" bs=1024 count=8027  # fill every data cluster, including the end of the filesystem
rm "$1/file1"  # free up some space at the start to allow conversion
