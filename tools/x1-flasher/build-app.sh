#!/bin/sh
# Builds "X1 Flasher.app": the SwiftUI window, the x1flash helper (Rust, tools/cave-x1) inside it, and the
# recovery card. Installs it to ~/Applications.
#   tools/x1-flasher/build-app.sh
set -e
cd "$(dirname "$0")"
here=$(pwd)
(cd ../cave-x1 && cargo build --release --bin x1flash)
swift build -c release
app="build/X1 Flasher.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp .build/release/X1Flasher "$app/Contents/MacOS/X1Flasher"
cp ../cave-x1/target/release/x1flash "$app/Contents/MacOS/x1flash"
cp ../cave-x1/recovery.html "$app/Contents/Resources/recovery.html"
# icon
icon=build/icon.iconset; rm -rf "$icon"; mkdir -p "$icon"
swift make-icon.swift build/icon-1024.png 1024
for s in 16 32 128 256 512; do
  sips -z $s $s build/icon-1024.png --out "$icon/icon_${s}x${s}.png" >/dev/null
  d=$((s*2)); sips -z $d $d build/icon-1024.png --out "$icon/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$icon" -o "$app/Contents/Resources/AppIcon.icns"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleIdentifier</key><string>app.cavecad.tools.x1-flasher</string>
  <key>CFBundleName</key><string>X1 Flasher</string>
  <key>CFBundleDisplayName</key><string>X1 Flasher</string>
  <key>CFBundleExecutable</key><string>X1Flasher</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSBluetoothAlwaysUsageDescription</key><string>Finds the Cavway X1 over Bluetooth to flash its firmware.</string>
</dict></plist>
PLIST
codesign --force --deep --sign - "$app"
mkdir -p "$HOME/Applications"
rm -rf "$HOME/Applications/X1 Flasher.app"
cp -R "$app" "$HOME/Applications/"
echo "Installed: $HOME/Applications/X1 Flasher.app"
