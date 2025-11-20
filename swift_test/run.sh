#!/bin/bash
set -e

cd "$(dirname "$0")"

echo "Building Rust library..."
(cd .. && cargo build --release)

echo "Copying header..."
cp ../target/release/build/riff_core-*/out/riff_core.h .

echo "Compiling Swift test..."
swiftc -import-objc-header riff_core.h -L../target/release -lriff_core -whole-module-optimization Generated.swift test.swift -o test_ffi

echo "Running test..."
DYLD_LIBRARY_PATH=../target/release ./test_ffi
