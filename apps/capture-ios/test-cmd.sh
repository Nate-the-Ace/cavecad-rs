#!/usr/bin/env bash
# Sends one test command to the Capture app running on a cabled iPhone (Debug builds only; see TestCommand.swift).
#   apps/capture-ios/test-cmd.sh <device-udid> '<query>'
#   apps/capture-ios/test-cmd.sh 00008150-... 'action=start'
#   apps/capture-ios/test-cmd.sh 00008150-... 'action=open&cave=Truitt Cave'     # spaces are encoded here
# The app is not restarted: devicectl hands the URL to the running app (or launches it first). The result is a
# [test] line in the app's log: watch it with live-log.sh, or read it later with pull-log.sh.
set -euo pipefail
UDID=${1:?usage: test-cmd.sh <device-udid> '<query, e.g. action=status>'}
QUERY=${2:?usage: test-cmd.sh <device-udid> '<query, e.g. action=status>'}
QUERY=${QUERY// /%20}
xcrun devicectl device process launch --device "$UDID" --quiet \
  --payload-url "cavecadcapture://test?$QUERY" app.cavecad.capture
echo "sent: cavecadcapture://test?$QUERY"
