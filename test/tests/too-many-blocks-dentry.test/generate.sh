#!/usr/bin/env bash

# Create a situation in which adding the dentry for the last file "x" is the tipping point between the conversion succeeding and failing (i.e. adding the last dentry requires the allocation of a block that is not available). This is tricky to accomplish because more files/longer filenames also require more blocks for serialization, in which case the conversion would fail before adding the dentry. The filename lengths are chosen exactly to prevent this, which means that the constants are highly implementation-dependent and may need to be adjusted if the implementation changes.

FILE_COUNT=90
LONG_FILENAME_PREFIX_LEN=244
SHORT_FILENAME_LEN=230

function string_with_len() {
	LEN=$1
	STRING=""
	for i in $(seq 1 $LEN); do
		STRING="${STRING}a"
	done
	echo $STRING
}

for i in $(seq 1 $FILE_COUNT); do
	dd if=/dev/urandom of="$1/$(string_with_len $LONG_FILENAME_PREFIX_LEN)$i" bs=1024 count=1
done
dd if=/dev/urandom of="$1/$(string_with_len $SHORT_FILENAME_LEN)" bs=1024 count=1
dd if=/dev/urandom of="$1/a" bs=1024 count=1
dd if=/dev/urandom of="$1/x" bs=1024 count=1
