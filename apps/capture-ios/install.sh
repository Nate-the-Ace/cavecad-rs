#!/usr/bin/env bash
# Builds CaveCAD Capture, signs it with your Apple team, installs it on a connected iPhone and launches it.
#   apps/capture-ios/install.sh <apple-team-id> <device-udid>
# The device udid: xcrun devicectl list devices. The phone must be unlocked, cabled (or paired) and trust this Mac.
set -euo pipefail
TEAM=${1:?usage: install.sh <apple-team-id> <device-udid>}
UDID=${2:?usage: install.sh <apple-team-id> <device-udid>}
cd "$(dirname "$0")"
./rust/build-xcframework.sh
xcodegen generate
xcodebuild -scheme CaveCADCapture -configuration Debug -destination "id=$UDID" \
  -allowProvisioningUpdates -allowProvisioningDeviceRegistration DEVELOPMENT_TEAM="$TEAM" \
  -derivedDataPath build build | tail -1
APP="build/Build/Products/Debug-iphoneos/CaveCAD Capture.app"
xcrun devicectl device install app --device "$UDID" "$APP"
xcrun devicectl device process launch --device "$UDID" --terminate-existing app.cavecad.capture
echo "installed and launched on $UDID"
