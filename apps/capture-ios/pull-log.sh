#!/usr/bin/env bash
# Copies the Capture app's log (Documents/capture.log) and a screenshot off the iPhone, prints the log's tail.
#   apps/capture-ios/pull-log.sh <device-udid> [out-dir] [lines]
set -euo pipefail
UDID=${1:?usage: pull-log.sh <device-udid> [out-dir] [lines]}
OUT=${2:-$(dirname "$0")/build/device}
mkdir -p "$OUT"
xcrun devicectl device copy from --device "$UDID" --domain-type appDataContainer \
  --domain-identifier app.cavecad.capture --source Documents/capture.log --destination "$OUT/capture.log" >/dev/null
xcrun devicectl device capture screenshot --device "$UDID" --destination "$OUT/screen.png" >/dev/null || true
echo "log: $OUT/capture.log  screenshot: $OUT/screen.png"
tail -n "${3:-60}" "$OUT/capture.log"
