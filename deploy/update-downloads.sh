#!/usr/bin/env bash
# Copies the newest desktop build (GitHub release desktop-latest) to the CaveCAD server's downloads folder.
# Run on the Mac after desktop.yml succeeds: deploy/update-downloads.sh
set -euo pipefail
REPO=Nate-the-Ace/cavecad-rs
WORK=$(mktemp -d)
gh release download desktop-latest -R "$REPO" -D "$WORK" --pattern '*.dmg' --pattern '*-setup.exe' --pattern '*.msi' --pattern '*.AppImage' --pattern '*.deb' --pattern '*.rpm'
cd "$WORK"
pick() { ls $1 2>/dev/null | head -1; }
VERSION=$(pick '*.dmg' | sed -E 's/^CaveCAD_([^_]+)_.*/\1/')
cat > downloads.json <<JSON
{"version":"$VERSION","builtAt":"$(date -u +%Y-%m-%dT%H:%M:%SZ)","files":{"mac":"$(pick '*.dmg')","windows":"$(pick '*-setup.exe')","windowsMsi":"$(pick '*.msi')","linuxAppImage":"$(pick '*.AppImage')","linuxDeb":"$(pick '*.deb')","linuxRpm":"$(pick '*.rpm')"}}
JSON
COPYFILE_DISABLE=1 tar -czf /tmp/cavecad-downloads.tgz .
scp -q /tmp/cavecad-downloads.tgz root@192.168.86.10:/tmp/
ssh root@192.168.86.10 'pct push 209 /tmp/cavecad-downloads.tgz /tmp/d.tgz && rm /tmp/cavecad-downloads.tgz && pct exec 209 -- bash -c "mkdir -p /var/lib/cavecad/downloads.new && tar -xzf /tmp/d.tgz -C /var/lib/cavecad/downloads.new 2>/dev/null && rm /tmp/d.tgz && rm -rf /var/lib/cavecad/downloads.old && { [ -d /var/lib/cavecad/downloads ] && mv /var/lib/cavecad/downloads /var/lib/cavecad/downloads.old || true; } && mv /var/lib/cavecad/downloads.new /var/lib/cavecad/downloads && chown -R cavecad:cavecad /var/lib/cavecad/downloads"'
rm -rf "$WORK" /tmp/cavecad-downloads.tgz
echo "downloads: $VERSION"
