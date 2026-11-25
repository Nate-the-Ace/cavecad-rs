#!/usr/bin/env bash
# Builds CaveCAD for a real Android phone or tablet (the same Tauri app and web interface as the iPhone and iPad),
# installs it over USB and launches it. The cave data (the web view's storage) stays across reinstalls.
#   tools/android-install.sh [adb-serial]
# The serial: adb devices (default: the first device listed). A phone gets the phone layout (width <= 600).
# Needs ~/android-env.sh (JDK 17, ANDROID_HOME, NDK_HOME), the Rust Android targets and gh signed in: docs/ANDROID-TAURI.md.
# Build settings are the same as the desktop test builds (desktop.yml, channel=dev): without the account address the
# sign-in can never finish, and without the dev channel the Survey tools are hidden.
set -euo pipefail
# shellcheck disable=SC1090
source ~/android-env.sh
SERIAL=${1:-$(adb devices | awk 'NR>1 && $2=="device" {print $1; exit}')}
[ -n "$SERIAL" ] || { echo "no Android device connected (plug it in, allow USB debugging)"; exit 1; }
cd "$(dirname "$0")/../apps/desktop"
export LANG=en_US.UTF-8 PATH=/opt/homebrew/opt/rustup/bin:$PATH
export VITE_ACCOUNT_BASE=$(gh variable get ACCOUNT_BASE -R Nate-the-Ace/cavecad-rs)
export VITE_SIGNIN_URL=$(gh variable get SIGNIN_URL -R Nate-the-Ace/cavecad-rs)
export VITE_PASS_PUBLIC_KEY=$(gh variable get PASS_PUBLIC_KEY -R Nate-the-Ace/cavecad-rs)
export VITE_UPDATE_CHANNEL=dev VITE_SIGNOUT_URL=
npx tauri android build --debug --target aarch64 --apk
APK=src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
adb -s "$SERIAL" install -r "$APK"
adb -s "$SERIAL" shell am start -n app.cavecad.desktop/.MainActivity
echo "installed and launched on $SERIAL"
