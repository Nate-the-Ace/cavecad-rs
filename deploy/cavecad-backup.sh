#!/bin/bash
# Nightly WAL-safe copy of the CaveCAD database, keeping the last 14. Run by cavecad-backup.timer.
set -euo pipefail
DB=/var/lib/cavecad/cavecad.sqlite
DIR=/var/lib/cavecad/backups
mkdir -p "$DIR"
sqlite3 "$DB" ".backup '$DIR/cavecad-$(date +%F).sqlite'"
# keep the newest 14 (names sort by date)
ls -1 "$DIR"/cavecad-*.sqlite | sort | head -n -14 | xargs -r rm -f --
