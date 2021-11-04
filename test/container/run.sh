#!/usr/bin/env bash
PROJECT_ROOT=$(realpath "$(dirname "$0")/../..")
TEST_DIR=$PROJECT_ROOT/test
docker run --rm \
           --tty \
           --privileged \
           -v /dev:/dev \
           -v "$PROJECT_ROOT":/project_root:ro \
           -v "$TEST_DIR":/test \
           -e OFS_CONVERT_TOOL_TIMEOUT="$OFS_CONVERT_TOOL_TIMEOUT" \
           ofs-convert-testing
