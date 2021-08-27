#!/usr/bin/env bash

BLOCK_COUNT=1733

# Cause external fragmentation by
#   - filling the disk with small files
#   - deleting every second file
#   - filling the space in between with one big file
#   - deleting the remaining small files
#   - filling the rest of the space with the big file

for i in $(seq 1 $BLOCK_COUNT); do
	dd if=/dev/zero of="$1/smallfile$i" bs=1024 count=1
done

for i in $(seq 1 2 $BLOCK_COUNT); do
	rm "$1/smallfile$i"
done

dd if=/dev/zero of="$1/bigfile" bs=1024 count=$(($BLOCK_COUNT / 2))

for i in $(seq 2 2 $BLOCK_COUNT); do
	rm "$1/smallfile$i"
done

dd if=/dev/zero of="$1/bigfile" bs=1024 count=$(($BLOCK_COUNT - $BLOCK_COUNT / 2)) oflag=append conv=notrunc
