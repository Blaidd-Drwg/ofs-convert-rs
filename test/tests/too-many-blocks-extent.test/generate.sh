#!/usr/bin/env bash

BLOCK_COUNT=1734

# Create a situation in which creating the extents for the file "bigfile" is the tipping point between the conversion succeeding and failing (i.e. adding the last extent to its extent tree requires the allocation of a block that is not available). We cause external fragmentation (which increases the number of extents for bigfile) by
#   - filling the disk with small files
#   - deleting every second file
#   - filling the space in between with one big file
#   - deleting the remaining small files
#   - filling the rest of the space with the big file

for i in $(seq 1 $BLOCK_COUNT); do
	dd if=/dev/urandom of="$1/smallfile$i" bs=1024 count=1
done

for i in $(seq 1 2 $BLOCK_COUNT); do
	rm "$1/smallfile$i"
done

dd if=/dev/urandom of="$1/bigfile" bs=1024 count=$(($BLOCK_COUNT / 2))

for i in $(seq 2 2 $BLOCK_COUNT); do
	rm "$1/smallfile$i"
done

dd if=/dev/urandom of="$1/bigfile" bs=1024 count=$(($BLOCK_COUNT - $BLOCK_COUNT / 2)) oflag=append conv=notrunc
