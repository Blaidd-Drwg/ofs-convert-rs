#!/usr/bin/env bash
cd /build
cmake /src -DCMAKE_BUILD_TYPE=$CMAKE_CONFIGURATION
cmake --build .
cd /test
python3 run.py /build/ofs-convert tests
