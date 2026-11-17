#!/bin/sh
# Builds and runs the Swift probe (uses the Mac's existing link to the Osmo), wrapped in an app so macOS allows Bluetooth.
#   tools/cave-osmo/run-swift.sh steps wait:3000 speed:300:0:2000 abs:450:0:20
set -e
cd "$(dirname "$0")"
app=target/osmo-probe-swift.app
mkdir -p "$app/Contents/MacOS"
swiftc -O swift/osmo-probe.swift -o "$app/Contents/MacOS/osmo-probe-swift"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleIdentifier</key><string>app.cavecad.tools.osmo-probe-swift</string>
  <key>CFBundleName</key><string>osmo-probe-swift</string>
  <key>CFBundleExecutable</key><string>osmo-probe-swift</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSUIElement</key><true/>
  <key>NSBluetoothAlwaysUsageDescription</key><string>Talks to a DJI Osmo Mobile gimbal to turn it.</string>
</dict></plist>
PLIST
codesign --force --sign - "$app" 2>/dev/null
out=$(mktemp -t osmo-probe-swift)
open -W --stdout "$out" --stderr "$out" "$app" --args "$@"
cat "$out"
