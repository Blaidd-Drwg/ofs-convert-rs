#!/usr/bin/env bash

PROJECT_ROOT=$(realpath "$(dirname "$0")/../..")
DOCKER_DIR=test/container  # relative to PROJECT_ROOT
docker build -t ofs-convert-testing -f "$PROJECT_ROOT/$DOCKER_DIR/Dockerfile" --build-arg docker_dir="$DOCKER_DIR" "$PROJECT_ROOT"
