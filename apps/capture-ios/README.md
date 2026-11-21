# CaveCAD Capture (iPhone)

The LiDAR room capture app (spec: `docs/superpowers/specs/2026-11-17-lidar-room-capture-design.md`).
Needs an iPhone with LiDAR (Pro models), Xcode, XcodeGen (`brew install xcodegen`) and rustup's toolchain.

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

`test-cmd.sh` is `xcrun devicectl device process launch --device <udid> --payload-url 'cavecadcapture://test?<query>'
app.cavecad.capture`: without `--terminate-existing` the running app is not restarted, it just opens the URL.
While a gimbal pass runs, a `[test] pass: …` status line is logged every 5 s. `live-log.sh <udid> all` shows
every line of the app's process, system frameworks included.
