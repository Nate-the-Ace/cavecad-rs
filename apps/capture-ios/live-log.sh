#!/usr/bin/env bash
# Streams the Capture app's own log lines live from a cabled iPhone (needs libimobiledevice: brew install libimobiledevice).
#   apps/capture-ios/live-log.sh <device-udid>          # only the app's "[area] text" lines (Log.write)
#   apps/capture-ios/live-log.sh <device-udid> all      # every line the app's process logs (system frameworks too)
# idevicesyslog prints "Nov 20 23:27:54.285941 CaveCAD Capture(<image>)[pid] <Notice>: [area] text"; Log mirrors
# each line to os Logger as "[area] text", so the filter keeps that process's lines whose message starts with a
# lower-case [area], and prints them as "time [area] text". Ctrl-C to stop.
set -euo pipefail
UDID=${1:?usage: live-log.sh <device-udid> [all]}
PROC='CaveCAD Capture(\([^)]*\))?\[[0-9]+\] <[A-Za-z]+>: '
if [ "${2:-}" = all ]; then
  idevicesyslog -n -u "$UDID" --no-colors | grep --line-buffered -E "$PROC"
else
  idevicesyslog -n -u "$UDID" --no-colors \
    | sed -l -n -E "s/^([A-Z][a-z]{2} +[0-9]+ [0-9:.]+) ${PROC}(\[[a-z]+\] .*)$/\1 \3/p"
fi
