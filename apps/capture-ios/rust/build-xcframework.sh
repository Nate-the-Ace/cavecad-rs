#!/usr/bin/env bash
# Builds the Rust core for the iPhone and the simulator and wraps it as CaptureCore.xcframework.
set -euo pipefail
cd "$(dirname "$0")"
export PATH=/opt/homebrew/opt/rustup/bin:$PATH
rustup target add aarch64-apple-ios aarch64-apple-ios-sim >/dev/null
cargo build --release --lib --target aarch64-apple-ios
cargo build --release --lib --target aarch64-apple-ios-sim
rm -rf ../CaptureCore.xcframework
xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/libcapture_ffi.a -headers include \
  -library target/aarch64-apple-ios-sim/release/libcapture_ffi.a -headers include \
  -output ../CaptureCore.xcframework
echo "built apps/capture-ios/CaptureCore.xcframework"
