#!/usr/bin/env bash
dd if=/dev/urandom of="$1/file" bs=1024 count=131073
