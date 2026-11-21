# CaveCAD Capture (iPhone)

The LiDAR room capture app (spec: `docs/superpowers/specs/2026-11-17-lidar-room-capture-design.md`).
Capturing needs an iPhone or iPad with LiDAR (Pro models); on any other iPhone or iPad the app is a viewer
(saved shots, shot viewer, Overall, chains, Station cards; no station picker, Capture or gimbal), decided by
`ARWorldTrackingConfiguration.supportsSceneReconstruction(.mesh)` (`DeviceMode`). Building needs Xcode,
XcodeGen (`brew install xcodegen`) and rustup's toolchain.

    apps/capture-ios/install.sh <team-id> <device-udid>     # build, install, launch
    cd apps/capture-ios/rust && cargo test                   # Rust core tests
    cd apps/capture-ios && ./rust/build-xcframework.sh && xcodegen generate && \
        xcodebuild test -scheme CaveCADCapture -destination 'platform=iOS Simulator,name=iPhone 18 Pro' CODE_SIGNING_ALLOWED=NO

Shots are kept on the phone in Documents/shots/<cave>/<shot>.cvcap (format: `crates/cave-capture/src/bundle.rs`).
Saved shots are placed together each time the list refreshes (`Core.chains` → `cc_chains`, headers only): shots at
neighbouring stations are chained through their station cards ("chained to A4"); a chain whose stations the cave lacks
floats ("chain A7–A9 · waiting for station") and drops into place when one arrives. The log gets
`[shots] chains: n shots, k chains, p placed by chain, m flagged` on every refresh.
Caves downloaded for offline use are in Application Support/caves (deleted on sign-out; shots are not).
The phone registers itself as a device on the CaveCAD account the first time it lists caves.

## CaveCAD on the same phone, offline (`Sources/SharedStations.swift`)

CaveCAD on this phone shares each cave's stations through the keychain (generic password, service
`cavecad.stations`, account = cave id, group `<team>.app.cavecad.shared`; format in CAVECAD-CONTRACT.md). Capture
merges them into the kept copy's stations when a cave opens and again each time the app comes back to the front
(not while the capture screen is up):

- Same station id: CaveCAD's position when it changed after the copy was kept, else the kept one.
- Stations only CaveCAD has are added after the kept ones and shown as "A9 · from CaveCAD on this phone", with
  a footer. They are real stations: picking one, typing its label, the test command `action=station&label=…`,
  waiting shots and chains all use the merged list.
- Caves only CaveCAD shares (made there, never downloaded here) are listed too ("from CaveCAD on this phone"),
  offline and online, and open as an empty copy whose stations all come from CaveCAD. "Get latest" keeps the
  real copy once the server has the cave. Until then a 404 on its captures list means "cave not on the server
  yet": its shots wait here and are tried again on the next sync (opening it, coming back to the app).
- Log: `[stations] N kept + M from CaveCAD on this phone` on opening and whenever the merge changes.
- Keychain groups (`Resources/CaveCADCapture.entitlements`): the app's own `<team>.app.cavecad.capture` first,
  then the shared one. `Keychain.save` names the own group, so the token and device key never land in the
  shared group; `load` does not filter by group, so items saved before the groups existed still load.
  An unsigned simulator build has no keychain, so those paths are only tested on a device.

Documents is shown in Files ("On My iPhone › Capture": `shots/` and `capture.log`), `UIFileSharingEnabled` and
`LSSupportsOpeningDocumentsInPlace` in Info.plist. CaveCAD on this phone links that folder to read the shots;
keep the `shots/<cave>/<shot>.cvcap` layout (CAVECAD-CONTRACT.md, "What CaveCAD relies on in Capture").

## Shot sync (`Sources/ShotSync.swift`, server side `apps/server/CAPTURES.md`)

Every saved shot of a cave goes to the server and every shot the server has comes down, one cave and one
upload at a time, from one `ShotSync` that lives for the app's run. It runs at launch, sign-in, coming back to
the app, opening a cave, "Get latest", pull to refresh on the station screen, and after every save or pass.

- Upload: 4 MiB chunks; asks `.../:sha/upload` for `have` first and resumes from it (also after a relaunch);
  409 → carry on from the server's `have`. A pass rewrites the file, so its new SHA-256 is uploaded and replaces
  the old one (same shot id). A version the server refuses (422/413) is not sent again until the file changes.
  No connection → retry after 5, 10, 20 … 300 s.
- Download: shots this device lacks (or holds unchanged since the last sync while the server's changed) come
  down in 4 MiB `Range` pieces into `<shot>.cvcap.part`, are checked against their SHA-256 and moved into place.
  A local shot the server lacks is uploaded, never deleted.
- Delete: deleting a shot here deletes it on the server (`Content-Type: application/json`), queued while offline.
- State per cave: Application Support/shot-sync/<cave>.json (each shot's SHA, hashed size/mtime, synced SHA,
  bytes uploaded, pending deletes).
- 404 on the captures list = the server keeps no captures (not deployed): logged once, "Sync not available on
  the server yet" under the saved shots, everything stays local, no retries until the next sync is asked for.
  For a cave not kept here (only in CaveCAD on this phone) a 404 means the server lacks that cave instead:
  "cave not on the server yet", the other caves keep syncing.
- Log area `sync`: list counts, each upload start/resume/409/done/failure (status code only), each download.

## Driving a test run from the Mac (Debug builds only)

Debug builds register the URL scheme `cavecadcapture` and take test commands over the cable; Release builds
register no scheme and compile no handler (`Sources/TestCommand.swift`, `Resources/Info.plist` under
`CAPTURE_TEST_URLS`, set for Debug in `project.yml`). Each command goes through the same functions the screen's
buttons call, under the same enable rules, and is logged as `[test] got …` and then `[test] … ok` or
`[test] … refused: <why>`.

    apps/capture-ios/live-log.sh <udid>                          # the app's own log lines, live (idevicesyslog)
    apps/capture-ios/test-cmd.sh <udid> 'action=status'          # one line: screen, cave, station, phase, gimbal, tracking, lux
    apps/capture-ios/test-cmd.sh <udid> 'action=open&cave=Truitt Cave'   # cave id or name; from the cave list
    apps/capture-ios/test-cmd.sh <udid> 'action=station&label=A3'        # pick the station and go to Capture
    apps/capture-ios/test-cmd.sh <udid> 'action=station&label=A9&other=1' # as "Other station…": a label the cave lacks is taken as waiting
    apps/capture-ios/test-cmd.sh <udid> 'action=gimbal&on=1'             # the Gimbal switch (0 turns it off)
    apps/capture-ios/test-cmd.sh <udid> 'action=start'                   # also pause, resume, stop
    apps/capture-ios/test-cmd.sh <udid> 'action=back'                    # leave the capture screen (from stations: the cave list)
    apps/capture-ios/test-cmd.sh <udid> 'action=sync'                    # shot sync now (open cave, else all); status has sync=… counts

`test-cmd.sh` is `xcrun devicectl device process launch --device <udid> --payload-url 'cavecadcapture://test?<query>'
app.cavecad.capture`: without `--terminate-existing` the running app is not restarted, it just opens the URL.
While a gimbal pass runs, a `[test] pass: …` status line is logged every 5 s. `live-log.sh <udid> all` shows
every line of the app's process, system frameworks included.
