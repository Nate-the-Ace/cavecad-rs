#!/bin/sh
# Builds and runs the Swift probe (uses the Mac's existing link to the Osmo), wrapped in an app so macOS allows Bluetooth.
#   tools/cave-osmo/run-swift.sh steps wait:3000 speed:300:0:2000 abs:450:0:20
set -e
cd "$(dirname "$0")"
app=target/osmo-probe-swift.app
mkdir -p "$app/Contents/MacOS"
# rebuild only when the source changed: every rebuild is a new binary, and macOS asks for Bluetooth again
if [ ! -x "$app/Contents/MacOS/osmo-probe-swift" ] || [ swift/osmo-probe.swift -nt "$app/Contents/MacOS/osmo-probe-swift" ]; then
  swiftc -O swift/osmo-probe.swift -o "$app/Contents/MacOS/osmo-probe-swift"
  rebuilt=1
fi
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
# a stable identity (Apple Development, if there is one) so macOS remembers the Bluetooth permission across rebuilds;
# otherwise ad hoc, which macOS treats as a new app after each rebuild
if [ -n "$rebuilt" ]; then
  id=$(security find-identity -v -p codesigning 2>/dev/null | sed -n 's/.*"\(Apple Development[^"]*\)".*/\1/p' | head -1)
  codesign --force --sign "${id:--}" --identifier app.cavecad.tools.osmo-probe-swift "$app" 2>/dev/null || codesign --force --sign - "$app"
fi
out=$(mktemp -t osmo-probe-swift)
open -W --stdout "$out" --stderr "$out" "$app" --args "$@"
cat "$out"
