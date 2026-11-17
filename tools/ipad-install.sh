#!/usr/bin/env bash
# Builds CaveCAD for a real iPad or iPhone, signs it with your Apple team, installs it over a cable and launches it.
# Free signing ("Personal Team") lasts 7 days: run this again to reinstall (the cave data stays).
#   tools/ipad-install.sh <apple-team-id> [device-udid]
# The team id is the OU of your "Apple Development" certificate (Keychain Access, or:
#   security find-certificate -c "Apple Development" -p | openssl x509 -noout -subject).
# The device udid: xcrun devicectl list devices (default: the first connected iPad). An iPhone gets the phone layout.
# Build settings are the same as the desktop test builds (desktop.yml, channel=dev): without the account address the
# sign-in can never finish, and without the dev channel the Survey tools are hidden.
set -euo pipefail
TEAM=${1:?usage: tools/ipad-install.sh <apple-team-id> [device-udid]}
UDID=${2:-$(xcrun devicectl list devices 2>/dev/null | awk '/available/ && /iPad/ && !/simulated/ {for (i=1;i<=NF;i++) if ($i ~ /^[0-9A-F]{8}-[0-9A-F]{16}$/) {print $i; exit}}')}
[ -n "$UDID" ] || { echo "no iPad connected (plug it in, unlock it, trust this Mac)"; exit 1; }
cd "$(dirname "$0")/../apps/desktop"
export LANG=en_US.UTF-8 PATH=/opt/homebrew/opt/rustup/bin:$PATH APPLE_DEVELOPMENT_TEAM=$TEAM
export VITE_ACCOUNT_BASE=$(gh variable get ACCOUNT_BASE -R Nate-the-Ace/cavecad-rs)
export VITE_SIGNIN_URL=$(gh variable get SIGNIN_URL -R Nate-the-Ace/cavecad-rs)
export VITE_PASS_PUBLIC_KEY=$(gh variable get PASS_PUBLIC_KEY -R Nate-the-Ace/cavecad-rs)
export VITE_UPDATE_CHANNEL=dev VITE_SIGNOUT_URL=
# a device new to the team must be registered before Apple will make a profile that covers it: an Xcode build aimed
# at the device does that (it then stops at the Rust step, which only runs under the Tauri CLI: that is expected)
(cd src-tauri/gen/apple && xcodebuild -scheme cavecad-desktop_iOS -configuration debug -destination "id=$UDID" \
  -allowProvisioningUpdates -allowProvisioningDeviceRegistration DEVELOPMENT_TEAM="$TEAM" -derivedDataPath build/register build \
  >/dev/null 2>&1 || true)
npx tauri ios build --debug --target aarch64
APP=$(ls -dt ~/Library/Developer/Xcode/DerivedData/cavecad-desktop-*/Build/Products/debug-iphoneos/CaveCAD.app | head -1)
xcrun devicectl device install app --device "$UDID" "$APP"
xcrun devicectl device process launch --device "$UDID" --terminate-existing app.cavecad.desktop
echo "installed and launched on $UDID"
