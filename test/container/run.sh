#!/usr/bin/env bash
PROJECT_ROOT=$(realpath "$(dirname "$0")/../..")
TEST_DIR=$PROJECT_ROOT/test
TARGET_DIR=$TEST_DIR/container/target

mkdir -p "$TARGET_DIR"
docker run --rm \
           --privileged \
           -v "$PROJECT_ROOT":/project_root:ro \
           -v "$TEST_DIR":/test \
           -v "$TARGET_DIR":/target \
           -e OFS_CONVERT_TOOL_TIMEOUT="$OFS_CONVERT_TOOL_TIMEOUT" \
           -e CMAKE_CONFIGURATION="$CMAKE_CONFIGURATION" \
           ofs-convert-testing
