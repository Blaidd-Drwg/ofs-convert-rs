#!/usr/bin/env bash
mkdir -p build
docker run --rm \
           --privileged \
           -v $(realpath ../..):/src:ro \
           -v $(realpath ./build):/build \
           -v $(realpath ..):/test \
           -e OFS_CONVERT_TOOL_TIMEOUT="$OFS_CONVERT_TOOL_TIMEOUT" \
           -e CMAKE_CONFIGURATION="$CMAKE_CONFIGURATION" \
           ofs-convert-testing
