# CaveCAD Capture: gimbal-driven passes

Status: design approved by Nathan, 2026-11-20. Build step 3 of
`2026-11-17-lidar-room-capture-design.md`, revised after hand scans, station cards and supplemental passes
shipped (`2026-11-20-supplemental-scans-design.md`). Branch `lidar-capture`, app `apps/capture-ios`.

## Goal

Hands-free, complete and steady scans: set the phone in the DJI Osmo Mobile 7 on a tripod over the station,
press Start, and do other survey work while the gimbal turns the phone through the room. Come back to a
saved pass.

## Decisions (Nathan, 2026-11-20)

- Purpose: hands-free while he works, repeatable full coverage, and steadier/reach. All three.
- Finish: **pattern, then fill, then stop** (save, chime, vibrate).
- Light: **area lights** he sets up; the app checks there is enough and holds when there isn't.
- Driver: **the whole `cave-osmo` in the app** (btleplug + tokio inside the app's Rust library), one
  implementation shared with the Mac tools. The same patched btleplug already runs on this iPhone in the
  Tauri app.
- Mount: **portrait, back camera out** of the clamp.

## Bringing the gimbal code in

Merge branch `ble-bridge` (167 commits; `git merge-tree` shows no conflicts as of 2026-11-20) into
`lidar-capture`, then regenerate `docs/CODE-INDEX.md`. `tools/cave-osmo` stays outside the engine workspace
(it needs native Bluetooth). The capture app's Rust library (`apps/capture-ios/rust`) depends on it by path,
with the same `[patch.crates-io] btleplug` path, and builds it for `aarch64-apple-ios` and the simulator
(on the simulator Bluetooth is absent: connect fails cleanly).

## The gimbal door (Rust FFI, `apps/capture-ios/rust`)

A gimbal runner owns a tokio runtime on its own thread and one `cave_osmo::Osmo`. The C doors are plain,
non-async calls:

- `cc_gimbal_connect(timeout_ms)` -> starts finding and connecting (finds a gimbal iOS has bonded).
- `cc_gimbal_status()` -> JSON: `connected`, `attitude` (quaternion), `joints`, `mount_ok` (portrait from
  the joints), `state` (`idle`, `moving`, `holding`, `home`, `error`), `step` (n of m), `last_error`.
- `cc_gimbal_run(plan_json)` -> runs a list of target-mode lists (each home to home, <= 15 targets).
- `cc_gimbal_hold()` / `cc_gimbal_go_on()` -> finish the current move, wait / continue.
- `cc_gimbal_stop_home()` -> abort, return home.
- `cc_gimbal_disconnect()`.

Status is polled by Swift (about 4 times a second, cheap). No callbacks into Swift.

## Patterns (pure Rust in `cave-osmo`, tested)

OM7 portrait windows (FINDINGS, target mode): yaw about -198..+84 (about 280 deg; a wedge of about 80 deg
behind cannot be reached), tilt -11..+31. The 4K camera's vertical view (about 65 deg) widens coverage to
roughly -45..+65 deg elevation; straight up and down are not reached.

- **Sweep:** two tilt rows (-10 and +30 deg), each panning the full yaw window in steps of about 30 deg,
  slow moves (about 15 deg/s) with a dwell at each step (default 1.5 s) so frames are sharp and LiDAR fills.
  About 3-4 minutes. Expressed as target-mode lists, each returning home. Rows, step, speed and dwell are
  parameters; defaults tuned on the bench before the cave.
- **Facing:** the rig's forward faces a wall; the unreachable wedge goes where the person came in from.
- **Fill:** after the sweep, directions around the pivot are binned into 10 deg cells; a cell is missing
  when no mesh is hit within 5 m along that direction from the pivot (the existing `MeshSnapshot` sight-line
  test, off the main thread). Reachable missing cells are clustered; the gimbal aims at the biggest clusters
  (`cave_osmo::aim`), up to 2 minutes, or until a fill step adds no covered cells.
- Unreachable gaps (up, down, the back wedge) stay as red hole outlines in the ghost for a hand Resume.

## Flow in the app

- A **Gimbal** switch on the capture screen. On: Start runs a gimbal pass; everything else is unchanged
  (cards, passes in one file, ghost, Pause, Resume, Stop). Hand and gimbal passes mix freely.
- Before moving: gimbal connected and `mount_ok`; ARKit tracking normal; light estimate above a threshold
  (`ambientIntensity`, default 400 lm, bench-tuned). Any failing check says which and does not start.
- Sweep -> Fill -> home -> save the pass -> chime and vibrate. The screen stays awake throughout (ARKit
  needs the app in front).
- **Pause** holds the gimbal where it is and saves the pass (as now). **Stop** returns home, saves, opens
  the viewer.

## Holds and failures

- Tracking limited: hold (finish the current move, start no next one) until normal. Lost > 10 s: end the
  pass, save what it has, flag "tracking lost".
- Too dark (light estimate under the threshold for > 3 s): hold, "Too dark: check the lights", continue when
  bright again.
- Gimbal disconnects: save what the pass has, flag "gimbal lost", try one reconnect.
- Phone battery under 15 %: finish the move, home, save.
- Flags go in the pass record (`Pass` gains `#[serde(default)] flags: Vec<String>`) and the log.

## Keeping it light

Bluetooth and moves on the Rust runner thread; attitude at 1 Hz from the gimbal; Swift polls status
cheaply; card detection and its backoff unchanged; fill analysis every few seconds off the main thread.

## Log

Every gimbal step, hold and reason; coverage (cells covered / reachable) before and after Fill; run time;
connect time.

## Testing

- `cave-osmo`: sweep lists within joint windows, <= 15 targets, each home to home; durations; fill
  clustering and aim choices from a synthetic coverage grid.
- App: the run's state machine (checks, sweep, fill, holds, failures) against a fake gimbal in simulator
  tests; status JSON parsing.
- Bench with Nathan: mount check, sweep timing and sharpness, light threshold, fill improvement.
- Then the cave.

## Not in this version

Landscape mount; roll-based vertical reach; a second tripod position to cover the back wedge
automatically; driving the X1 laser during a LiDAR pass.
