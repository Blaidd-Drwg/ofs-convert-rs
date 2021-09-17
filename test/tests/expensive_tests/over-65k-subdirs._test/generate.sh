#!/usr/bin/env bash
# create 65000 + 3 ('.', '..', 'lost+found') links to the root directory
for i in $(seq 1 65000); do
	mkdir "$1/$i"  # name must fit into a FAT 8.3 short name
done
