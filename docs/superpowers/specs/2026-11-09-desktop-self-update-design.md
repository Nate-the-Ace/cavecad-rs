# Desktop self-update — design

2026-11-09. Approved by Nathan in conversation ("Ask, then restart"; "Yes, build it").

## Why

The desktop app carries its own copy of the web app, built when the installer was. It has no updater and
every build says version 0.0.1, so a desktop user runs whatever they last installed while cavecad.com moves
on. Nathan: "the desktop app version needs to be able to self update."

## What the person sees

- On start, and every 6 hours while open, the app asks the CaveCAD server whether a newer version exists. If
  one does, it downloads it in the background, checks its signature, then shows a bar:
  **CaveCAD 0.1.N is ready.** *Restart now* · *Later*.
- Nothing installs until *Restart now*. The open cave is kept first (as every change already is), then the
  update installs and the app restarts. *Later* hides the bar until the next start.
- Linux installed from .deb or .rpm cannot replace itself: the bar says **CaveCAD 0.1.N is out** with a link
  to the Downloads list instead. macOS, Windows and the Linux AppImage update themselves.
- Signed out, or a non-member: no check (updates are members-only, as the installers are).
- The app installed today has no updater: everyone reinstalls once by hand; from then on updates come by the
  bar.

## Pieces

1. **Version.** `desktop.yml` stamps `0.1.<run number>` into `tauri.conf.json` before building. Local builds
   stay 0.0.1 and so are always older than any release.
2. **Signing.** A Tauri updater key pair (minisign). The public key lives in `tauri.conf.json`
   (`plugins.updater.pubkey`); the private key and its password are GitHub secrets
   `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, read by tauri-action, which then
   writes a `.sig` beside each update file and a `latest.json` on the release
   (`bundle.createUpdaterArtifacts: true`). The app refuses any update not signed by that key. Nathan keeps a
   copy of the private key and password in his password manager: if lost, a new key means one more manual
   reinstall for everyone.
3. **Publishing.** `deploy/update-downloads.sh` also fetches `latest.json`, the `.app.tar.gz` and the `.sig`
   files, and writes `update.json` into the downloads folder: version, notes, date, and per platform the file
   name and signature.
4. **Server.** `GET /api/desktop-update/<os>/<arch>/<current>` (the updater's own URL shape). Signed-in member
   only (the desktop app's bearer token). `204` when `<current>` is not older than `update.json`'s version or
   there is no build for that platform; otherwise the updater's JSON: `version`, `notes`, `pub_date`,
   `signature`, and `url` = this host's `/downloads/<file>`. `/downloads/` serves the update files too (the
   names in `update.json`), still members-only.
5. **App, Rust side.** `tauri-plugin-updater`. Two commands: `update_prepare(base, token)` checks
   `<base>/api/desktop-update/{{target}}/{{arch}}/{{current_version}}` with the bearer header, downloads and
   verifies, and keeps the bytes; it answers the version, and whether it can install (false on Linux outside
   an AppImage, where it does not download). `update_install()` installs the kept bytes and restarts.
6. **App, web side.** `lib/desktop-update.svelte.ts` runs the check (desktop, signed in) and holds the state;
   `components/UpdateBar.svelte` shows the bar.

## Not in scope

Code signing by Apple or Microsoft (the first-run warning stays); a release channel choice; showing release
notes beyond one line.

## Testing

Server: unit test of the endpoint (older, same, newer, missing platform, signed out). Version compare and the
`latest.json` → `update.json` step: unit tests. End to end: two `desktop.yml` builds; install the first, run
`update-downloads.sh` for the second, see the bar, restart into the second.
