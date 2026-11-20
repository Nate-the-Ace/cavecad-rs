# CaveCAD Capture (iPhone)

The LiDAR room capture app (spec: `docs/superpowers/specs/2026-11-17-lidar-room-capture-design.md`).
Needs an iPhone with LiDAR (Pro models), Xcode, XcodeGen (`brew install xcodegen`) and rustup's toolchain.

    apps/capture-ios/install.sh <team-id> <device-udid>     # build, install, launch
    cd apps/capture-ios/rust && cargo test                   # Rust core tests
    cd apps/capture-ios && ./rust/build-xcframework.sh && xcodegen generate && \
        xcodebuild test -scheme CaveCADCapture -destination 'platform=iOS Simulator,name=iPhone 18 Pro' CODE_SIGNING_ALLOWED=NO

Shots are kept on the phone in Documents/shots/<cave>/<shot>.cvcap (format: `crates/cave-capture/src/bundle.rs`).
Caves downloaded for offline use are in Application Support/caves (deleted on sign-out; shots are not).
The phone registers itself as a device on the CaveCAD account the first time it lists caves.
