#!/usr/bin/env bash
for i in $(seq 1 501); do
	touch "$1/$i"
done
