#!/usr/bin/env bash
dd if=/dev/zero of="$1/file" bs=1024 count=6
