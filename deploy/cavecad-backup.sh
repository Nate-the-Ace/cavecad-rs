#!/bin/bash
# Nightly WAL-safe copy of the CaveCAD database, keeping the last 14. Run by cavecad-backup.timer.
# The copy is written to a .tmp beside it, must pass PRAGMA integrity_check, and only then is moved into place;
# any failure exits non-zero (the unit shows failed) and leaves no .tmp behind. Copies are readable by the owner only.
set -euo pipefail
umask 077
DB=${CAVECAD_DB:-/var/lib/cavecad/cavecad.sqlite}
DIR=${CAVECAD_BACKUP_DIR:-/var/lib/cavecad/backups}
if [ ! -f "$DB" ]; then
  echo "cavecad-backup: no database at $DB" >&2
  exit 1
fi
mkdir -p "$DIR"
OUT="$DIR/cavecad-$(date +%F).sqlite"
TMP="$OUT.tmp"
trap 'rm -f "$TMP" "$TMP-wal" "$TMP-shm"' EXIT
rm -f "$TMP"
sqlite3 "$DB" ".backup '$TMP'"
# The copy keeps the source's WAL mode; a plain rollback journal makes it one self-contained file.
sqlite3 "$TMP" 'PRAGMA journal_mode=DELETE;' >/dev/null
CHECK=$(sqlite3 "$TMP" 'PRAGMA integrity_check;')
if [ "$CHECK" != "ok" ]; then
  echo "cavecad-backup: the copy failed its integrity check: $CHECK" >&2
  exit 1
fi
mv -f "$TMP" "$OUT"
# keep the newest 14 (names sort by date)
ls -1 "$DIR"/cavecad-*.sqlite | sort -r | tail -n +15 | xargs -r rm -f --
