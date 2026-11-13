#!/usr/bin/env bash
# Copies the newest desktop build (GitHub release desktop-latest) to the CaveCAD server's downloads folder:
# the installers (downloads.json) and the desktop app's own updates (update.json, from the release's
# latest.json: per platform the update file and its signature, which the app checks before installing).
# Run on the Mac after desktop.yml succeeds: deploy/update-downloads.sh
# Away from home, reach the server through Tailscale: CAVECAD_SSH_JUMP=nathan@media-vm deploy/update-downloads.sh
# Test builds (desktop.yml run with channel=dev, release desktop-dev): deploy/update-downloads.sh --dev puts them in
# /var/lib/cavecad/downloads-dev, which only admins can download from or update to.
set -euo pipefail
TAG=desktop-latest
DEST=/var/lib/cavecad/downloads
if [ "${1:-}" = "--dev" ]; then TAG=desktop-dev; DEST=/var/lib/cavecad/downloads-dev; fi
JUMP=(${CAVECAD_SSH_JUMP:+-J "$CAVECAD_SSH_JUMP"})
REPO=Nate-the-Ace/cavecad-rs
WORK=$(mktemp -d)
gh release download "$TAG" -R "$REPO" -D "$WORK" --pattern '*.dmg' --pattern '*-setup.exe' --pattern '*.msi' --pattern '*.AppImage' --pattern '*.deb' --pattern '*.rpm' --pattern '*.app.tar.gz' --pattern 'latest.json'
cd "$WORK"
if [ -f latest.json ]; then
  python3 - <<'PY'
import json, os, urllib.parse
latest = json.load(open('latest.json'))
platforms = {}
for key, p in latest.get('platforms', {}).items():
    name = urllib.parse.unquote(p['url'].rsplit('/', 1)[-1])
    if os.path.isfile(name) and p.get('signature'):
        platforms[key] = {'file': name, 'signature': p['signature']}
# Windows: the setup .exe is the installer most people use (the .msi is for managed PCs), so an update replaces
# like with like; Linux: only the AppImage can replace itself
for plain, preferred in (('windows-x86_64', 'windows-x86_64-nsis'), ('linux-x86_64', 'linux-x86_64-appimage')):
    if preferred in platforms:
        platforms[plain] = platforms[preferred]
json.dump({'version': latest['version'], 'notes': latest.get('notes', ''), 'pubDate': latest.get('pub_date', ''), 'platforms': platforms}, open('update.json', 'w'))
print('update:', latest['version'], sorted(platforms))
PY
  rm latest.json
else
  echo 'no latest.json on the release: no update offered (was the build signed?)'
fi
# the newest version of each kind: the release keeps older builds' files beside the new ones
pick() { ls $1 2>/dev/null | sort -V | tail -1; }
VERSION=$(pick '*.dmg' | sed -E 's/^CaveCAD_([^_]+)_.*/\1/')
cat > downloads.json <<JSON
{"version":"$VERSION","builtAt":"$(date -u +%Y-%m-%dT%H:%M:%SZ)","files":{"mac":"$(pick '*.dmg')","windows":"$(pick '*-setup.exe')","windowsMsi":"$(pick '*.msi')","linuxAppImage":"$(pick '*.AppImage')","linuxDeb":"$(pick '*.deb')","linuxRpm":"$(pick '*.rpm')"}}
JSON
COPYFILE_DISABLE=1 tar -czf /tmp/cavecad-downloads.tgz .
scp -q ${JUMP[@]+"${JUMP[@]}"} /tmp/cavecad-downloads.tgz root@192.168.86.10:/tmp/
ssh ${JUMP[@]+"${JUMP[@]}"} root@192.168.86.10 "pct push 209 /tmp/cavecad-downloads.tgz /tmp/d.tgz && rm /tmp/cavecad-downloads.tgz && pct exec 209 -- bash -c 'D=$DEST && mkdir -p \$D.new && tar -xzf /tmp/d.tgz -C \$D.new 2>/dev/null && rm /tmp/d.tgz && rm -rf \$D.old && { [ -d \$D ] && mv \$D \$D.old || true; } && mv \$D.new \$D && chown -R cavecad:cavecad \$D'"
rm -rf "$WORK" /tmp/cavecad-downloads.tgz
echo "downloads ($TAG -> $DEST): $VERSION"
