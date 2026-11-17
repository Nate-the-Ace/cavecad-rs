#!/bin/sh
# Runs a cave-x1 tool on a Mac. macOS only lets a program use Bluetooth if it says why (a usage string in an app's
# Info.plist), and a plain command-line program cannot, so the tool is wrapped in a tiny app and launched as one.
# The first run asks to allow Bluetooth. Output goes to the terminal when the tool finishes.
#   tools/cave-osmo/run-mac.sh osmo-probe [args…]
set -e
cd "$(dirname "$0")"
bin=${1:-osmo-probe}; shift || true
cargo build --release --bin "$bin" >/dev/null
app="target/$bin.app"
mkdir -p "$app/Contents/MacOS"
cp "target/release/$bin" "$app/Contents/MacOS/$bin"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleIdentifier</key><string>app.cavecad.tools.$bin</string>
  <key>CFBundleName</key><string>$bin</string>
  <key>CFBundleExecutable</key><string>$bin</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSUIElement</key><true/>
  <key>NSBluetoothAlwaysUsageDescription</key><string>Talks to a DJI Osmo Mobile gimbal to turn it.</string>
</dict></plist>
PLIST
codesign --force --sign - "$app" 2>/dev/null
out=$(mktemp -t "$bin")
open -W --stdout "$out" --stderr "$out" "$app" --args "$@"
cat "$out"
