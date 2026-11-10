#!/usr/bin/env python3
"""Writes the desktop app's update list (latest.json) for one version from the signature files.

The desktop workflow builds macOS, Linux and Windows side by side; when each build wrote latest.json itself,
the last one to finish replaced the others' entries (0.1.27-0.1.29 came out with one platform only, so the
other platforms were never offered the update). The builds no longer write it: this runs once after all of
them, in a folder holding the release's .sig files, and lists every platform.

    make-latest-json.py <version> <notes> <base url> > latest.json
"""
import datetime
import json
import sys

version, notes, base = sys.argv[1], sys.argv[2], sys.argv[3].rstrip("/")


def entry(name):
    with open(name + ".sig", encoding="utf-8") as f:
        return {"url": f"{base}/{name}", "signature": f.read().strip()}


mac = entry("CaveCAD_universal.app.tar.gz")
appimage = entry(f"CaveCAD_{version}_amd64.AppImage")
exe = entry(f"CaveCAD_{version}_x64-setup.exe")
platforms = {k: mac for k in ("darwin-aarch64", "darwin-aarch64-app", "darwin-x86_64", "darwin-x86_64-app")}
platforms.update(
    {
        "linux-x86_64": appimage,
        "linux-x86_64-appimage": appimage,
        "linux-x86_64-deb": entry(f"CaveCAD_{version}_amd64.deb"),
        "linux-x86_64-rpm": entry(f"CaveCAD-{version}-1.x86_64.rpm"),
        # Windows: the setup .exe is the one most people install, so an update replaces like with like
        "windows-x86_64": exe,
        "windows-x86_64-nsis": exe,
        "windows-x86_64-msi": entry(f"CaveCAD_{version}_x64_en-US.msi"),
    }
)
now = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.000Z")
json.dump({"version": version, "notes": notes, "pub_date": now, "platforms": platforms}, sys.stdout, indent=2)
print()
