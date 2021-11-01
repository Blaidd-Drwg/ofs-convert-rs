#!/usr/bin/env bash
# Write a file that fills the entire filesystem so there are no blocks left for conversion. Then increase the partition's size so that if the filesystem filled the entire partition, the conversion could succeed. Expect failure.
dd if=/dev/urandom of="$1/file" bs=1024 count=10
dd if=/dev/zero bs=1024 count=7 >>"$2"
