#!/usr/bin/env bash
FILENAME=ä
for i in $(seq 1 254); do
	FILENAME=a$FILENAME
done
# FILENAME: 2 + (254 * 1) bytes (assuming UTF-8 encoding)
touch "$1/$FILENAME"
