# Gimbal Capture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers-extended-cc:subagent-driven-development (recommended) or superpowers-extended-cc:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A gimbal pass in the iPhone capture app: the Osmo Mobile 7 sweeps the phone slowly through its whole range while ARKit scans, then aims at the holes it can reach, returns home, saves the pass and chimes; it holds for poor tracking or light and saves what it has on failure.

**Architecture:** Merge branch `ble-bridge` (tools/cave-osmo + patched btleplug) into `lidar-capture`. Pure camera-sweep and fill planning go in `cave-osmo` (`camera` module). The capture app's Rust library links `cave-osmo` and runs one gimbal "runner" (tokio runtime on its own thread) behind plain C doors; Swift polls its status. A Swift `GimbalRun` state machine (tested against a fake gimbal) drives checks, sweep, fill, holds and failures and plugs into the existing `ShotRecorder` pass flow.

**Tech Stack:** Rust 2024 (tokio, btleplug 0.11.8 patched, serde_json), C FFI + xcframework, Swift 6 / SwiftUI / ARKit / CoreBluetooth (through btleplug), XcodeGen, XCTest.

**User decisions (already made):**
- "Hands-free while I work" — and also repeatable full coverage and steadiness ("all of the above").
- "Pattern, fill, stop": sweep, then patch holes, then home, save, chime/vibrate.
- "the lidar is continuous, so we just need to sweep the full range of the gimbal slowly and then patch any holes" — one continuous slow sweep, no dwell stops.
- "Area lights I set up": app checks light, holds when too dark.
- "Whole cave-osmo in the app": btleplug + tokio inside the app's Rust library.
- "Portrait, camera out" mount.
- Spec: `docs/superpowers/specs/2026-11-20-gimbal-capture-design.md` (approved).

**Facts the engineer needs (from `tools/cave-osmo/FINDINGS.md` and memory, branch ble-bridge):**
- Target mode (DJI waypoints) is the reliable way to move: list of ≤ 15 targets (yaw, roll, pitch in degrees relative to home, plus move seconds), each list must end at home (entering target mode sends the gimbal home). ~1 s accurate moves, no roll flips. Target mode **times out ~9 s after the last command**: split long moves into targets of ≤ 8 s each.
- Target-mode yaw range about −200…+95 relative to home; portrait joint windows `OM7_PORTRAIT` pan −198…84, tilt −11…31, roll 75…205. With the phone portrait, camera out: target **yaw pans the camera, pitch tilts it**. The sign of yaw/pitch vs camera direction is confirmed on the bench (Task 8) and kept as a setting.
- The gimbal reports attitude ~1/s after the handshake; `Osmo::find` finds a gimbal iOS has bonded (patched btleplug).
- iOS needs `NSBluetoothAlwaysUsageDescription` in Info.plist.

**Repo rules:** read `CLAUDE.md`. Core crates pure and wasm32-safe (`tools/cave-osmo` is a tool, outside the engine workspace — Bluetooth allowed there). `///` doc on every public item; `python3 tools/make_code_index.py` after public items change; `cargo fmt -p <crate>` only, never the whole workspace; `export PATH=/opt/homebrew/opt/rustup/bin:$PATH`.

**Commands:**
- cave-osmo: `cd tools/cave-osmo && cargo test --lib && cargo clippy --lib --tests -- -D warnings`
- capture FFI: `cd apps/capture-ios/rust && cargo test && cargo clippy --all-targets -- -D warnings`
- App: `cd apps/capture-ios && ./rust/build-xcframework.sh && xcodegen generate -q && xcodebuild test -scheme CaveCADCapture -destination 'platform=iOS Simulator,name=iPhone 18 Pro' CODE_SIGNING_ALLOWED=NO 2>&1 | grep -E "error:|Executed|TEST (SUCCEEDED|FAILED)"`; device build `xcodebuild -scheme CaveCADCapture -destination 'generic/platform=iOS' CODE_SIGNING_ALLOWED=NO build 2>&1 | tail -1`
- Phone: `apps/capture-ios/install.sh 2ANLK72UUS 00008150-0011281122D2401C` ("Locked" launch error = installed); log `apps/capture-ios/pull-log.sh 00008150-0011281122D2401C`

## File map

| File | Responsibility |
|---|---|
| `tools/cave-osmo/src/camera.rs` (new; `pub mod camera;` in lib.rs) | camera window, `sweep`, `fill` target lists |
| `apps/capture-ios/rust/Cargo.toml` | depend on cave-osmo, tokio; btleplug patch |
| `apps/capture-ios/rust/src/gimbal.rs` (new) | runner thread, status, run/hold/stop logic |
| `apps/capture-ios/rust/src/lib.rs`, `include/capture_core.h` | `cc_gimbal_*` doors |
| `apps/capture-ios/Sources/Core.swift` | Swift wrappers |
| `apps/capture-ios/Sources/Gimbal.swift` (new) | `GimbalDriver` protocol, `CoreGimbal`, `GimbalStatus` |
| `apps/capture-ios/Sources/GimbalRun.swift` (new) | state machine: checks, sweep, fill, holds, failures |
| `apps/capture-ios/Sources/Coverage.swift` (new) | missing directions around the pivot from the mesh |
| `crates/cave-capture/src/passes.rs` | `Pass.flags` |
| `apps/capture-ios/Sources/ShotRecorder.swift`, `CaptureView.swift` | wiring, Gimbal switch, chime |
| `apps/capture-ios/Resources/Info.plist` | Bluetooth usage string |

---

### Task 1: Merge `ble-bridge` into `lidar-capture`

**Goal:** `tools/cave-osmo` and the patched btleplug are in this branch; everything still builds and tests.

**Files:** merge commit; `docs/CODE-INDEX.md` regenerated.

**Acceptance Criteria:**
- [ ] `git merge ble-bridge` completes with no conflicts; `tools/cave-osmo/src/lib.rs` and `apps/desktop/src-tauri/vendor/btleplug-0.11.8` exist on the branch.
- [ ] `cargo test -p cave-capture` passes; `cd tools/cave-osmo && cargo test --lib` passes; `python3 tools/make_code_index.py --check` passes (after regenerating).
- [ ] App simulator tests still `** TEST SUCCEEDED **`.

**Verify:** `git log --oneline -1` shows the merge; the commands above pass.

**Steps:**
- [ ] **Step 1:** Make sure the ble-bridge worktree has nothing uncommitted: `git -C ../cavecad-rs-ble status --short` → empty. If not empty, stop and report (do not commit someone else's work).
- [ ] **Step 2:** `git merge --no-ff ble-bridge -m "Merge ble-bridge (cave-osmo gimbal library, patched btleplug) into lidar-capture"`. Expected: no conflicts (checked with `git merge-tree` on 2026-11-20).
- [ ] **Step 3:** `python3 tools/make_code_index.py` then `git add docs/CODE-INDEX.md && git commit -m "Code index after merging ble-bridge"` if it changed.
- [ ] **Step 4:** Run the cave-capture, cave-osmo lib and app test commands; all pass. Do not run the whole workspace `cargo fmt`.

---

### Task 2: `cave-osmo::camera` — sweep and fill target lists

**Goal:** Pure functions turning a camera window and a speed into target-mode lists: the continuous sweep, and fill lists from missing directions.

**Files:**
- Create: `tools/cave-osmo/src/camera.rs`
- Modify: `tools/cave-osmo/src/lib.rs` (add `pub mod camera;`)

**Acceptance Criteria:**
- [ ] `sweep(OM7_CAMERA, 10.0)`: every list has 1..=15 targets, every list's last target is home (0,0,0), no target leaves the window, no target's move is longer than `MAX_MOVE_S` (8 s), and the yaw path covers both window ends at both pitch rows.
- [ ] Move seconds = angular distance / speed (at least 0.6 s).
- [ ] `fill(dirs, OM7_CAMERA, 10.0)`: directions outside the window are dropped, directions within 12° of an earlier kept one are merged, lists ≤ 15 each ending at home, nearest-next order from home.
- [ ] Empty input → empty fill; speed ≤ 0 → empty plan.

**Verify:** `cd tools/cave-osmo && cargo test --lib camera` → `test result: ok`

**Steps:**
- [ ] **Step 1: Tests first** — append to `camera.rs` (written with the module below; run them before filling in the bodies by stubbing `sweep`/`fill` to `vec![]` first):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn all(lists: &[Vec<Target>]) -> Vec<Target> { lists.iter().flatten().copied().collect() }

    #[test]
    fn sweep_stays_inside_and_ends_each_list_home() {
        let lists = sweep(OM7_CAMERA, 10.0);
        assert!(!lists.is_empty());
        for l in &lists {
            assert!((1..=crate::targets::LIST_MAX).contains(&l.len()), "{}", l.len());
            let last = l.last().unwrap();
            assert_eq!((last.yaw, last.roll, last.pitch), (0.0, 0.0, 0.0));
            for t in l {
                assert!(t.yaw >= OM7_CAMERA.yaw.0 - 1e-9 && t.yaw <= OM7_CAMERA.yaw.1 + 1e-9, "{t:?}");
                assert!(t.pitch >= OM7_CAMERA.pitch.0 - 1e-9 && t.pitch <= OM7_CAMERA.pitch.1 + 1e-9, "{t:?}");
                assert!(t.seconds <= MAX_MOVE_S + 1e-9 && t.seconds >= 0.6 - 1e-9, "{t:?}");
            }
        }
        let ts = all(&lists);
        for (yaw, pitch) in [(OM7_CAMERA.yaw.0, OM7_CAMERA.pitch.0), (OM7_CAMERA.yaw.1, OM7_CAMERA.pitch.0),
                             (OM7_CAMERA.yaw.0, OM7_CAMERA.pitch.1), (OM7_CAMERA.yaw.1, OM7_CAMERA.pitch.1)] {
            assert!(ts.iter().any(|t| (t.yaw - yaw).abs() < 1e-9 && (t.pitch - pitch).abs() < 1e-9), "{yaw},{pitch}");
        }
    }

    #[test]
    fn move_time_is_distance_over_speed() {
        let lists = sweep(Window { yaw: (-20.0, 20.0), pitch: (0.0, 0.0) }, 10.0);
        let ts = all(&lists);
        // first move: home (0,0) to (20,0) = 20 deg at 10 deg/s
        assert!((ts[0].seconds - 2.0).abs() < 1e-9, "{:?}", ts[0]);
        assert!(sweep(OM7_CAMERA, 0.0).is_empty());
    }

    #[test]
    fn fill_keeps_reachable_merges_close_and_goes_home() {
        let dirs = [(10.0, 5.0), (12.0, 6.0), (300.0, 0.0), (-150.0, 20.0), (0.0, 80.0)];
        let lists = fill(&dirs, OM7_CAMERA, 10.0);
        let ts = all(&lists);
        let aims: Vec<_> = ts.iter().filter(|t| !(t.yaw == 0.0 && t.pitch == 0.0)).collect();
        assert_eq!(aims.len(), 2, "{aims:?}"); // (12,6) merged into (10,5); 300 and pitch 80 unreachable
        assert!((aims[0].yaw - 10.0).abs() < 1e-9); // nearest first
        for l in &lists { let last = l.last().unwrap(); assert_eq!((last.yaw, last.pitch), (0.0, 0.0)); }
        assert!(fill(&[], OM7_CAMERA, 10.0).is_empty());
    }
}
```

- [ ] **Step 2:** Run `cargo test --lib camera` → fails (stubs).
- [ ] **Step 3: Implement** `tools/cave-osmo/src/camera.rs`:

```rust
//! Sweeping a phone's camera through the room with the gimbal: target-mode lists for the phone held portrait,
//! camera out (target yaw pans the camera, pitch tilts it). Pure: no Bluetooth.

use crate::targets::{LIST_MAX, Target};

/// Where the camera can be pointed, degrees relative to home (target-mode yaw and pitch).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Window {
    /// Yaw (pan) range, low to high.
    pub yaw: (f64, f64),
    /// Pitch (tilt) range, low to high.
    pub pitch: (f64, f64),
}

/// The OM7 holding the phone portrait, camera out, inside its joint windows with a margin (FINDINGS; bench-tuned).
pub const OM7_CAMERA: Window = Window { yaw: (-195.0, 80.0), pitch: (-10.0, 30.0) };

/// Longest single move, seconds: target mode drops out ~9 s after the last command, so long sweeps are split.
pub const MAX_MOVE_S: f64 = 8.0;

const HOME: (f64, f64) = (0.0, 0.0);

/// Targets moving from `from` to `to` at `dps`, split so no move is longer than [`MAX_MOVE_S`].
fn segment(from: (f64, f64), to: (f64, f64), dps: f64) -> Vec<Target> {
    let d = ((to.0 - from.0).powi(2) + (to.1 - from.1).powi(2)).sqrt();
    let n = ((d / dps) / MAX_MOVE_S).ceil().max(1.0) as usize;
    (1..=n)
        .map(|k| {
            let f = k as f64 / n as f64;
            Target {
                yaw: from.0 + (to.0 - from.0) * f,
                roll: 0.0,
                pitch: from.1 + (to.1 - from.1) * f,
                seconds: (d / n as f64 / dps).max(0.6),
            }
        })
        .collect()
}

/// Cuts a path (starting and ending at home) into lists of at most [`LIST_MAX`] targets, each ending at home: a
/// list that would overflow is closed with a move home, and the next one starts from home again.
fn lists_of(points: &[(f64, f64)], dps: f64) -> Vec<Vec<Target>> {
    let mut lists = Vec::new();
    let mut cur: Vec<Target> = Vec::new();
    let mut at = HOME;
    let mut i = 0;
    while i < points.len() {
        let p = points[i];
        let seg = segment(at, p, dps);
        let back = segment(p, HOME, dps);
        if cur.len() + seg.len() + back.len() > LIST_MAX && !cur.is_empty() {
            cur.extend(segment(at, HOME, dps));
            lists.push(std::mem::take(&mut cur));
            at = HOME;
            continue;
        }
        cur.extend(seg);
        at = p;
        i += 1;
    }
    if !cur.is_empty() || at != HOME {
        cur.extend(segment(at, HOME, dps));
    }
    if !cur.is_empty() {
        lists.push(cur);
    }
    lists.retain(|l| !l.is_empty());
    lists
}

/// One continuous slow sweep of the whole window: pitch at its bottom, pan to the high end, pan across to the low
/// end, tilt to the top, pan back to the high end, home. Empty for a non-positive speed.
pub fn sweep(w: Window, dps: f64) -> Vec<Vec<Target>> {
    if !(dps > 0.0) {
        return vec![];
    }
    let path = [(w.yaw.1, w.pitch.0), (w.yaw.0, w.pitch.0), (w.yaw.0, w.pitch.1), (w.yaw.1, w.pitch.1)];
    lists_of(&path, dps)
}

/// Aims at missing directions (yaw, pitch relative to home): keeps those inside the window, merges any within 12°
/// of one already kept, visits them nearest-first from home, every list ending home.
pub fn fill(dirs: &[(f64, f64)], w: Window, dps: f64) -> Vec<Vec<Target>> {
    if !(dps > 0.0) {
        return vec![];
    }
    let inside = |d: &(f64, f64)| d.0 >= w.yaw.0 && d.0 <= w.yaw.1 && d.1 >= w.pitch.0 && d.1 <= w.pitch.1;
    let dist = |a: (f64, f64), b: (f64, f64)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
    let mut kept: Vec<(f64, f64)> = Vec::new();
    for d in dirs.iter().filter(|d| inside(d)) {
        if !kept.iter().any(|k| dist(*k, *d) < 12.0) {
            kept.push(*d);
        }
    }
    let mut order = Vec::new();
    let mut at = HOME;
    while !kept.is_empty() {
        let (i, _) = kept.iter().enumerate().min_by(|a, b| dist(at, *a.1).total_cmp(&dist(at, *b.1))).unwrap();
        at = kept.remove(i);
        order.push(at);
    }
    if order.is_empty() {
        return vec![];
    }
    lists_of(&order, dps)
}
```

Add `pub mod camera;` to `tools/cave-osmo/src/lib.rs` near the other modules.
- [ ] **Step 4:** `cargo test --lib camera` passes; `cargo clippy --lib --tests -- -D warnings` clean; `cargo fmt` inside `tools/cave-osmo` (it is its own workspace).
- [ ] **Step 5:** Commit: `git add tools/cave-osmo/src && git commit -m "cave-osmo: camera sweep and fill target lists (phone portrait, camera out)"`

---

### Task 3: Gimbal runner and `cc_gimbal_*` doors

**Goal:** The app's Rust library runs one gimbal runner (tokio on its own thread) with plain C doors to connect, run lists, hold, go on, stop home, disconnect and read status.

**Files:**
- Modify: `apps/capture-ios/rust/Cargo.toml`
- Create: `apps/capture-ios/rust/src/gimbal.rs`
- Modify: `apps/capture-ios/rust/src/lib.rs` (`mod gimbal;` + doors), `apps/capture-ios/rust/include/capture_core.h`
- Modify: `apps/capture-ios/Sources/Core.swift` (wrappers)
- Modify: `apps/capture-ios/Resources/Info.plist` (`NSBluetoothAlwaysUsageDescription`: "CaveCAD Capture drives your gimbal to sweep the phone while scanning.")

**Acceptance Criteria:**
- [ ] `capture-ffi` builds for `aarch64-apple-ios` and the simulator via `./rust/build-xcframework.sh` (btleplug patched path resolves).
- [ ] Doors: `cc_gimbal_connect(timeout_ms: u32)`, `cc_gimbal_run(plan_json)` (JSON array of lists of `{yaw,roll,pitch,seconds}`), `cc_gimbal_hold()`, `cc_gimbal_go_on()`, `cc_gimbal_stop_home()`, `cc_gimbal_disconnect()` return nothing; `cc_gimbal_status()` returns JSON `{"connected","state","step","steps","mount_ok","joints":[3]|null,"error":string|null}` with `state` one of `idle, connecting, moving, holding, homing, done, error`.
- [ ] `mount_ok` = joints known and roll joint within 80…112 (portrait upright, as the Tauri app's homeFrom rule).
- [ ] Rust unit tests: plan JSON parsing (good, empty, >15 refused), status JSON shape, `mount_ok` rule.
- [ ] On the simulator `cc_gimbal_connect` ends in `state:"error"` with a readable error, not a crash.

**Verify:** `cd apps/capture-ios/rust && cargo test` → ok; app tests `** TEST SUCCEEDED **` (a Swift test calls `Core.gimbalStatus()` and gets `idle`).

**Steps:**
- [ ] **Step 1: Cargo.toml** — add:

```toml
cave-osmo = { path = "../../../tools/cave-osmo" }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "time", "sync"] }

[patch.crates-io]
btleplug = { path = "../../desktop/src-tauri/vendor/btleplug-0.11.8" }
```

(Check the `cave-osmo` package name in `tools/cave-osmo/Cargo.toml`; use it as the dependency key.)
- [ ] **Step 2: Tests first** in `gimbal.rs` `#[cfg(test)] mod tests`:

```rust
#[test]
fn plan_json_parses_and_refuses_bad_lists() {
    let ok = parse_plan(r#"[[{"yaw":10,"roll":0,"pitch":0,"seconds":1},{"yaw":0,"roll":0,"pitch":0,"seconds":1}]]"#).unwrap();
    assert_eq!(ok.len(), 1);
    assert_eq!(ok[0].len(), 2);
    assert!(parse_plan("[]").is_err());
    let big: Vec<_> = (0..16).map(|_| serde_json::json!({"yaw":0,"roll":0,"pitch":0,"seconds":1})).collect();
    assert!(parse_plan(&serde_json::json!([big]).to_string()).is_err());
}

#[test]
fn status_json_and_mount_rule() {
    let mut s = Status::default();
    let j: serde_json::Value = serde_json::from_str(&s.json()).unwrap();
    assert_eq!(j["state"], "idle");
    assert_eq!(j["connected"], false);
    assert!(j["joints"].is_null());
    assert!(!mount_ok(None));
    assert!(mount_ok(Some([0.0, 0.0, 95.0])));
    assert!(!mount_ok(Some([0.0, 0.0, 150.0])));
    s.state = State::Moving; s.step = 3; s.steps = 11;
    let j: serde_json::Value = serde_json::from_str(&s.json()).unwrap();
    assert_eq!((j["state"].as_str(), j["step"].as_u64(), j["steps"].as_u64()), (Some("moving"), Some(3), Some(11)));
}
```

- [ ] **Step 3: Implement `gimbal.rs`:**

```rust
//! One gimbal at a time, driven from a tokio runtime on its own thread. The C doors only flip flags and send
//! commands; the phone polls `status`.

use cave_osmo::targets::{LIST_MAX, Target};
use cave_osmo::Osmo;
use serde::Deserialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State { #[default] Idle, Connecting, Moving, Holding, Homing, Done, Error }

impl State {
    fn name(self) -> &'static str {
        match self { State::Idle => "idle", State::Connecting => "connecting", State::Moving => "moving",
            State::Holding => "holding", State::Homing => "homing", State::Done => "done", State::Error => "error" }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Status {
    pub connected: bool,
    pub state: State,
    pub step: u32,
    pub steps: u32,
    pub joints: Option<[f64; 3]>,
    pub error: Option<String>,
}

impl Status {
    pub fn json(&self) -> String {
        serde_json::json!({
            "connected": self.connected, "state": self.state.name(), "step": self.step, "steps": self.steps,
            "mount_ok": mount_ok(self.joints), "joints": self.joints, "error": self.error,
        }).to_string()
    }
}

/// Portrait upright: the roll joint at home sits within 80…112 (the Tauri app's homeFrom rule).
pub fn mount_ok(joints: Option<[f64; 3]>) -> bool {
    joints.is_some_and(|j| (80.0..=112.0).contains(&j[2]))
}

#[derive(Deserialize)]
struct TargetIn { yaw: f64, roll: f64, pitch: f64, seconds: f64 }

/// A plan: lists of target-mode targets, each 1..=15.
pub fn parse_plan(json: &str) -> Result<Vec<Vec<Target>>, String> {
    let lists: Vec<Vec<TargetIn>> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if lists.is_empty() || lists.iter().any(|l| l.is_empty() || l.len() > LIST_MAX) {
        return Err("each list must hold 1 to 15 targets".into());
    }
    Ok(lists.into_iter().map(|l| l.into_iter().map(|t| Target { yaw: t.yaw, roll: t.roll, pitch: t.pitch, seconds: t.seconds }).collect()).collect())
}

enum Cmd { Connect(Duration), Run(Vec<Vec<Target>>), StopHome, Disconnect }

struct Runner {
    _rt: tokio::runtime::Runtime,
    tx: mpsc::UnboundedSender<Cmd>,
    status: Arc<Mutex<Status>>,
    hold: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
}

fn runner() -> &'static Runner {
    static R: OnceLock<Runner> = OnceLock::new();
    R.get_or_init(|| {
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().expect("tokio");
        let (tx, rx) = mpsc::unbounded_channel();
        let status = Arc::new(Mutex::new(Status::default()));
        let hold = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        rt.spawn(drive(rx, status.clone(), hold.clone(), stop.clone()));
        Runner { _rt: rt, tx, status, hold, stop }
    })
}

fn set(status: &Mutex<Status>, f: impl FnOnce(&mut Status)) { f(&mut status.lock().unwrap()) }

async fn drive(mut rx: mpsc::UnboundedReceiver<Cmd>, status: Arc<Mutex<Status>>, hold: Arc<AtomicBool>, stop: Arc<AtomicBool>) {
    let mut osmo: Option<Osmo> = None;
    while let Some(cmd) = rx.recv().await {
        match cmd {
            Cmd::Connect(wait) => {
                set(&status, |s| { s.state = State::Connecting; s.error = None; });
                match Osmo::find(wait).await {
                    Ok(o) => {
                        let joints = o.attitude().map(|(_, a)| a.joints());
                        osmo = Some(o);
                        set(&status, |s| { s.connected = true; s.state = State::Idle; s.joints = joints; });
                    }
                    Err(e) => set(&status, |s| { s.connected = false; s.state = State::Error; s.error = Some(format!("{e:?}")); }),
                }
            }
            Cmd::Run(lists) => {
                let Some(o) = osmo.as_mut() else { set(&status, |s| { s.state = State::Error; s.error = Some("no gimbal".into()); }); continue };
                stop.store(false, Ordering::SeqCst);
                let r = run(o, &lists, &status, &hold, &stop).await;
                set(&status, |s| match r {
                    Ok(()) => s.state = State::Done,
                    Err(e) => { s.state = State::Error; s.error = Some(e); }
                });
            }
            Cmd::StopHome => { /* handled inside run via the stop flag; idle here */ }
            Cmd::Disconnect => {
                if let Some(o) = osmo.take() { o.disconnect().await; }
                set(&status, |s| *s = Status::default());
            }
        }
        if let Some(o) = osmo.as_ref() {
            let joints = o.attitude().map(|(_, a)| a.joints());
            set(&status, |s| s.joints = joints.or(s.joints));
        }
    }
}

/// Runs each list: enter (sends home), upload, step through; hold waits between targets keeping target mode
/// alive; stop goes home and ends.
async fn run(o: &mut Osmo, lists: &[Vec<Target>], status: &Mutex<Status>, hold: &AtomicBool, stop: &AtomicBool) -> Result<(), String> {
    let steps: u32 = lists.iter().map(|l| l.len() as u32).sum();
    set(status, |s| { s.steps = steps; s.step = 0; s.state = State::Moving; });
    o.targets_enter(false).await.map_err(|e| format!("{e:?}"))?;
    tokio::time::sleep(Duration::from_millis(2500)).await;
    let mut done = 0u32;
    for (n, list) in lists.iter().enumerate() {
        let id = (n % 200) as u8 + 1;
        if n > 0 {
            o.targets_leave().await.map_err(|e| format!("{e:?}"))?;
            o.targets_enter(true).await.map_err(|e| format!("{e:?}"))?;
            tokio::time::sleep(Duration::from_millis(2500)).await;
        }
        o.targets_upload(list, id).await.map_err(|e| format!("{e:?}"))?;
        for (i, t) in list.iter().enumerate() {
            if i > 0 { o.targets_go(i as u8).await.map_err(|e| format!("{e:?}"))?; }
            let arrived = o.targets_arrived(i as u8, id, Duration::from_secs_f64(t.seconds + 5.0)).await.map_err(|e| format!("{e:?}"))?;
            if !arrived { return Err(format!("target {} of list {} not reached", i, n + 1)); }
            done += 1;
            let joints = o.attitude().map(|(_, a)| a.joints());
            set(status, |s| { s.step = done; s.joints = joints.or(s.joints); });
            // hold: wait here, re-sending "go i" every 5 s so target mode does not drop out
            let mut last = std::time::Instant::now();
            while hold.load(Ordering::SeqCst) && !stop.load(Ordering::SeqCst) {
                set(status, |s| s.state = State::Holding);
                if last.elapsed() > Duration::from_secs(5) { let _ = o.targets_go(i as u8).await; last = std::time::Instant::now(); }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            set(status, |s| s.state = State::Moving);
            if stop.load(Ordering::SeqCst) {
                set(status, |s| s.state = State::Homing);
                let _ = o.targets_leave().await;
                let _ = o.targets_enter(true).await; // entering sends it home
                tokio::time::sleep(Duration::from_millis(2500)).await;
                let _ = o.targets_exit().await;
                return Ok(());
            }
        }
    }
    let _ = o.targets_exit().await;
    Ok(())
}

pub fn connect(ms: u32) { let _ = runner().tx.send(Cmd::Connect(Duration::from_millis(ms as u64))); }
pub fn run_plan(json: &str) -> Result<(), String> { let p = parse_plan(json)?; runner().hold.store(false, Ordering::SeqCst); runner().tx.send(Cmd::Run(p)).map_err(|e| e.to_string()) }
pub fn hold() { runner().hold.store(true, Ordering::SeqCst); }
pub fn go_on() { runner().hold.store(false, Ordering::SeqCst); }
pub fn stop_home() { runner().stop.store(true, Ordering::SeqCst); runner().hold.store(false, Ordering::SeqCst); let _ = runner().tx.send(Cmd::StopHome); }
pub fn disconnect() { let _ = runner().tx.send(Cmd::Disconnect); }
pub fn status() -> String { runner().status.lock().unwrap().json() }
```

(Check `Osmo::find`, `attitude`, `Attitude::joints`, `targets_*`, `disconnect` signatures in `tools/cave-osmo/src/lib.rs` and adapt; an `Err(Error)` debug-formats. If `Osmo` is not `Send`, keep it inside the single runtime task as shown — never move it across threads.)
- [ ] **Step 4: Doors** in `lib.rs`: `mod gimbal;` and

```rust
#[unsafe(no_mangle)] pub extern "C" fn cc_gimbal_connect(timeout_ms: u32) { gimbal::connect(timeout_ms) }
/// # Safety
/// `plan_json` NUL-terminated UTF-8 or null.
#[unsafe(no_mangle)] pub unsafe extern "C" fn cc_gimbal_run(plan_json: *const c_char) -> CcBuf {
    guard(|| match unsafe { text(plan_json) } {
        Ok(Some(t)) => match gimbal::run_plan(t) { Ok(()) => give_json(serde_json::json!({"ok": true})), Err(e) => err(&e) },
        _ => err("plan is not text"),
    }, internal)
}
#[unsafe(no_mangle)] pub extern "C" fn cc_gimbal_hold() { gimbal::hold() }
#[unsafe(no_mangle)] pub extern "C" fn cc_gimbal_go_on() { gimbal::go_on() }
#[unsafe(no_mangle)] pub extern "C" fn cc_gimbal_stop_home() { gimbal::stop_home() }
#[unsafe(no_mangle)] pub extern "C" fn cc_gimbal_disconnect() { gimbal::disconnect() }
#[unsafe(no_mangle)] pub extern "C" fn cc_gimbal_status() -> CcBuf { give(gimbal::status().into_bytes()) }
```

Document each in `capture_core.h` (status JSON fields and states as above).
- [ ] **Step 5: Swift wrappers** in `Core.swift`:

```swift
    static func gimbalConnect(timeoutMs: UInt32 = 15000) { cc_gimbal_connect(timeoutMs) }
    /// nil on success, else the core's error text.
    static func gimbalRun(_ lists: [[[String: Double]]]) -> String? {
        guard let d = try? JSONSerialization.data(withJSONObject: lists), let t = String(data: d, encoding: .utf8) else { return "plan" }
        let out = t.withCString { take(cc_gimbal_run($0)) }
        let j = (try? JSONSerialization.jsonObject(with: out) as? [String: Any]) ?? [:]
        return j["ok"] as? Bool == true ? nil : (j["error"] as? String ?? "gimbal")
    }
    static func gimbalHold() { cc_gimbal_hold() }
    static func gimbalGoOn() { cc_gimbal_go_on() }
    static func gimbalStopHome() { cc_gimbal_stop_home() }
    static func gimbalDisconnect() { cc_gimbal_disconnect() }
    static func gimbalStatus() -> [String: Any] {
        (try? JSONSerialization.jsonObject(with: take(cc_gimbal_status())) as? [String: Any]) ?? [:]
    }
```

- [ ] **Step 6:** Swift test (in a new `Tests/GimbalTests.swift`): `XCTAssertEqual(Core.gimbalStatus()["state"] as? String, "idle")`.
- [ ] **Step 7:** Build xcframework, run Rust + app tests and the device build; commit `apps/capture-ios/rust apps/capture-ios/Sources/Core.swift apps/capture-ios/Resources/Info.plist apps/capture-ios/Tests` with message "capture-ios: gimbal runner (cave-osmo in the app) behind cc_gimbal_* doors".

---

### Task 4: `GimbalDriver`, fake gimbal, and the `GimbalRun` state machine

**Goal:** A tested Swift state machine that runs a gimbal pass: checks, sweep, fill, home, done; holds for tracking/light; ends on lost tracking, gimbal loss or low battery — independent of ARKit and Bluetooth (inputs are plain values).

**Files:**
- Create: `apps/capture-ios/Sources/Gimbal.swift`, `apps/capture-ios/Sources/GimbalRun.swift`
- Test: `apps/capture-ios/Tests/GimbalTests.swift`

**Acceptance Criteria:**
- [ ] `GimbalRun.check(...)` refuses with a specific message for: not connected, mount not portrait, tracking not normal, too dark (light below `minLux`, default 400).
- [ ] With a `FakeGimbal`, `tick` advances sweep → (fill planned from `missing`) → fill → done, calling `run` once per stage, and reports `.finished(flags: [])`.
- [ ] Tracking limited → `hold()`; normal again → `goOn()`. Lost > 10 s → `stopHome()` and `.finished(flags: ["tracking lost"])`.
- [ ] Too dark > 3 s → hold with message "Too dark: check the lights"; bright again → go on.
- [ ] Gimbal state `error` or `connected == false` mid-run → `.finished(flags: ["gimbal lost"])`.
- [ ] Battery < 0.15 → `stopHome()` → `.finished(flags: ["low battery"])` once the gimbal reports `done`.
- [ ] Fill skipped (straight to done) when no reachable missing directions; fill time capped at 120 s (then `stopHome`).

**Verify:** app tests `** TEST SUCCEEDED **` including `GimbalTests`.

**Steps:**
- [ ] **Step 1: `Gimbal.swift`:**

```swift
import Foundation

/// The gimbal as the capture app sees it (the real one in the Rust core, or a fake in tests).
protocol GimbalDriver: AnyObject {
    func connect()
    /// nil when started, else why not.
    func run(_ lists: [[[String: Double]]]) -> String?
    func hold()
    func goOn()
    func stopHome()
    func status() -> GimbalStatus
}

struct GimbalStatus: Equatable, Sendable {
    var connected = false
    var state = "idle"   // idle, connecting, moving, holding, homing, done, error
    var step = 0, steps = 0
    var mountOK = false
    var error: String?

    init(connected: Bool = false, state: String = "idle", step: Int = 0, steps: Int = 0, mountOK: Bool = false, error: String? = nil) {
        self.connected = connected; self.state = state; self.step = step; self.steps = steps; self.mountOK = mountOK; self.error = error
    }

    init(_ d: [String: Any]) {
        connected = d["connected"] as? Bool ?? false
        state = d["state"] as? String ?? "idle"
        step = (d["step"] as? NSNumber)?.intValue ?? 0
        steps = (d["steps"] as? NSNumber)?.intValue ?? 0
        mountOK = d["mount_ok"] as? Bool ?? false
        error = d["error"] as? String
    }
}

/// The Osmo through the Rust core's runner.
final class CoreGimbal: GimbalDriver {
    func connect() { Core.gimbalConnect() }
    func run(_ lists: [[[String: Double]]]) -> String? { Core.gimbalRun(lists) }
    func hold() { Core.gimbalHold() }
    func goOn() { Core.gimbalGoOn() }
    func stopHome() { Core.gimbalStopHome() }
    func status() -> GimbalStatus { GimbalStatus(Core.gimbalStatus()) }
}
```

- [ ] **Step 2: Tests first** (`GimbalTests.swift`) with a `FakeGimbal` recording calls:

```swift
import XCTest
@testable import CaveCAD_Capture

final class FakeGimbal: GimbalDriver {
    var st = GimbalStatus(connected: true, state: "idle", mountOK: true)
    var runs: [[[[String: Double]]]] = [], holds = 0, goOns = 0, stops = 0
    func connect() {}
    func run(_ l: [[[String: Double]]]) -> String? { runs.append(l); st.state = "moving"; return nil }
    func hold() { holds += 1 }
    func goOn() { goOns += 1 }
    func stopHome() { stops += 1; st.state = "homing" }
    func status() -> GimbalStatus { st }
}

final class GimbalTests: XCTestCase {
    func inputs(t: Double, tracking: GimbalRun.Tracking = .normal, lux: Double = 1000, battery: Double = 0.9,
                missing: [(Double, Double)] = []) -> GimbalRun.Inputs {
        GimbalRun.Inputs(time: t, tracking: tracking, lux: lux, battery: battery, missing: missing)
    }

    func testChecksRefuseWithReasons() {
        let g = FakeGimbal()
        XCTAssertNil(GimbalRun.check(g.status(), tracking: .normal, lux: 1000))
        g.st.connected = false
        XCTAssertEqual(GimbalRun.check(g.status(), tracking: .normal, lux: 1000), "Gimbal not connected.")
        g.st.connected = true; g.st.mountOK = false
        XCTAssertEqual(GimbalRun.check(g.status(), tracking: .normal, lux: 1000), "Put the phone upright (portrait) in the gimbal.")
        g.st.mountOK = true
        XCTAssertEqual(GimbalRun.check(g.status(), tracking: .limited, lux: 1000), "Hold still until tracking settles.")
        XCTAssertEqual(GimbalRun.check(g.status(), tracking: .normal, lux: 100), "Too dark: check the lights.")
    }

    func testSweepThenFillThenDone() {
        let g = FakeGimbal(), r = GimbalRun(gimbal: g)
        XCTAssertEqual(r.start(at: 0), nil)
        XCTAssertEqual(g.runs.count, 1)                  // sweep
        g.st.state = "done"
        _ = r.tick(inputs(t: 60, missing: [(10, 5)]))  // sweep done: fill planned and started
        XCTAssertEqual(g.runs.count, 2)
        g.st.state = "done"
        XCTAssertEqual(r.tick(inputs(t: 90)), .finished(flags: []))
    }

    func testNoReachableHolesSkipsFill() {
        let g = FakeGimbal(), r = GimbalRun(gimbal: g)
        _ = r.start(at: 0); g.st.state = "done"
        XCTAssertEqual(r.tick(inputs(t: 60, missing: [(0, 85)])), .finished(flags: []))
        XCTAssertEqual(g.runs.count, 1)
    }

    func testTrackingHoldsAndLostEnds() {
        let g = FakeGimbal(), r = GimbalRun(gimbal: g)
        _ = r.start(at: 0)
        _ = r.tick(inputs(t: 1, tracking: .limited)); XCTAssertEqual(g.holds, 1)
        _ = r.tick(inputs(t: 2, tracking: .normal)); XCTAssertEqual(g.goOns, 1)
        _ = r.tick(inputs(t: 3, tracking: .lost))
        XCTAssertEqual(r.tick(inputs(t: 9, tracking: .lost)), .running)
        _ = r.tick(inputs(t: 14, tracking: .lost))
        XCTAssertEqual(g.stops, 1)
        g.st.state = "done"
        XCTAssertEqual(r.tick(inputs(t: 20, tracking: .lost)), .finished(flags: ["tracking lost"]))
    }

    func testDarkHoldsAfterThreeSeconds() {
        let g = FakeGimbal(), r = GimbalRun(gimbal: g)
        _ = r.start(at: 0)
        _ = r.tick(inputs(t: 1, lux: 50)); XCTAssertEqual(g.holds, 0)
        _ = r.tick(inputs(t: 4.5, lux: 50)); XCTAssertEqual(g.holds, 1)
        XCTAssertEqual(r.message, "Too dark: check the lights")
        _ = r.tick(inputs(t: 5, lux: 900)); XCTAssertEqual(g.goOns, 1)
    }

    func testGimbalLostAndLowBattery() {
        var g = FakeGimbal(), r = GimbalRun(gimbal: g)
        _ = r.start(at: 0); g.st.connected = false
        XCTAssertEqual(r.tick(inputs(t: 1)), .finished(flags: ["gimbal lost"]))
        g = FakeGimbal(); r = GimbalRun(gimbal: g)
        _ = r.start(at: 0)
        _ = r.tick(inputs(t: 1, battery: 0.1)); XCTAssertEqual(g.stops, 1)
        g.st.state = "done"
        XCTAssertEqual(r.tick(inputs(t: 2, battery: 0.1)), .finished(flags: ["low battery"]))
    }
}
```

- [ ] **Step 3: `GimbalRun.swift`:**

```swift
import Foundation

/// One gimbal pass: sweep, then fill the reachable holes, then done. Pure decisions from plain inputs, so it is
/// tested without ARKit or Bluetooth; `ShotRecorder` feeds it once a frame-ish (a few times a second).
@MainActor
final class GimbalRun {
    enum Tracking: Equatable { case normal, limited, lost }
    enum Outcome: Equatable { case running, finished(flags: [String]) }
    struct Inputs { var time: Double; var tracking: Tracking; var lux: Double; var battery: Double; var missing: [(Double, Double)] }

    static let minLux = 400.0
    static let darkAfter = 3.0, lostAfter = 10.0, fillMax = 120.0, lowBattery = 0.15
    /// Sweep speed, degrees per second (bench-tuned).
    static var speed = 10.0

    private enum Stage { case sweep, fill, ending }
    private let gimbal: GimbalDriver
    private var stage = Stage.sweep
    private var holding = false
    private var darkSince: Double?, lostSince: Double?, fillStart: Double?
    private var flags: [String] = []
    private(set) var message = ""

    init(gimbal: GimbalDriver) { self.gimbal = gimbal }

    /// Why a pass cannot start now, or nil.
    static func check(_ s: GimbalStatus, tracking: Tracking, lux: Double) -> String? {
        if !s.connected { return "Gimbal not connected." }
        if !s.mountOK { return "Put the phone upright (portrait) in the gimbal." }
        if tracking != .normal { return "Hold still until tracking settles." }
        if lux < minLux { return "Too dark: check the lights." }
        return nil
    }

    /// Starts the sweep; nil when started, else why not.
    func start(at t: Double) -> String? {
        let lists = Core.cameraSweep(speed: Self.speed)
        if let e = gimbal.run(lists) { return e }
        stage = .sweep; message = "Sweeping…"
        return nil
    }

    func tick(_ i: Inputs) -> Outcome {
        let s = gimbal.status()
        if !s.connected || s.state == "error" { return .finished(flags: flags + ["gimbal lost"]) }
        if stage == .ending { return s.state == "done" || s.state == "idle" ? .finished(flags: flags) : .running }
        // stops that end the pass
        if i.tracking == .lost {
            lostSince = lostSince ?? i.time
            if i.time - lostSince! > Self.lostAfter { return end("tracking lost") }
        } else { lostSince = nil }
        if i.battery < Self.lowBattery { return end("low battery") }
        if stage == .fill, let f = fillStart, i.time - f > Self.fillMax { return end(nil) }
        // holds
        let dark = i.lux < Self.minLux
        darkSince = dark ? (darkSince ?? i.time) : nil
        let tooDark = darkSince.map { i.time - $0 > Self.darkAfter } ?? false
        let shouldHold = i.tracking != .normal || tooDark
        if shouldHold != holding {
            holding = shouldHold
            if holding { gimbal.hold() } else { gimbal.goOn() }
        }
        message = tooDark ? "Too dark: check the lights" : (i.tracking != .normal ? "Holding: tracking" : (stage == .sweep ? "Sweeping…" : "Filling holes…"))
        // stage changes
        if s.state == "done" {
            switch stage {
            case .sweep:
                let lists = Core.cameraFill(missing: i.missing, speed: Self.speed)
                if lists.isEmpty { return .finished(flags: flags) }
                if let e = gimbal.run(lists) { return .finished(flags: flags + ["gimbal lost: \(e)"]) }
                stage = .fill; fillStart = i.time
            case .fill: return .finished(flags: flags)
            case .ending: break
            }
        }
        return .running
    }

    private func end(_ flag: String?) -> Outcome {
        if let flag { flags.append(flag) }
        gimbal.stopHome(); stage = .ending; message = "Going home…"
        return .running
    }
}
```

`Core.cameraSweep(speed:)` and `Core.cameraFill(missing:speed:)` are Swift wrappers over two more doors added in this task: `cc_camera_sweep(speed: f64) -> CcBuf` and `cc_camera_fill(missing_json, speed) -> CcBuf`, returning the lists JSON (`[[{"yaw","roll","pitch","seconds"}]]`) from `cave_osmo::camera::sweep/fill` with `OM7_CAMERA`. Add them to `lib.rs`, `capture_core.h` and `Core.swift` (returning `[[[String: Double]]]`), with a Rust test that the sweep JSON round-trips through `gimbal::parse_plan`.
- [ ] **Step 4:** Run app tests until green; commit "capture-ios: GimbalRun state machine (sweep, fill, holds, failures) with a fake gimbal".

---

### Task 5: Coverage — missing directions around the pivot

**Goal:** From the current mesh and the pivot (camera position at home) and home heading, list the missing 10° cells as (yaw, pitch) relative to home for `GimbalRun.Inputs.missing`.

**Files:**
- Create: `apps/capture-ios/Sources/Coverage.swift`
- Test: `apps/capture-ios/Tests/GimbalTests.swift` (append)

**Acceptance Criteria:**
- [ ] A closed box mesh around the pivot (walls 2 m away) → no missing cells.
- [ ] The same box with one wall removed → missing cells only toward that wall, converted to yaw relative to the home heading (yaw sign setting `Coverage.yawSign`, default +1, flips them).
- [ ] Cells are 10° in azimuth over 360 and 10° in elevation over −45…+65; a cell is missing when the sight line from the pivot hits nothing within 5 m.
- [ ] Runs off the main thread (pure function on a `MeshSnapshot`).

**Verify:** app tests `** TEST SUCCEEDED **`.

**Steps:**
- [ ] **Step 1: Tests** — build `MeshSnapshot` boxes with `MeshPart` (identity transform; two triangles per wall), pivot at origin, home heading −z (ARKit camera forward):

```swift
    func box(without missingWall: Int? = nil) -> MeshSnapshot {
        // walls at x=±2, z=±2, floor y=-1.5, ceiling y=2
        let quads: [[SIMD3<Float>]] = [
            [SIMD3(-2,-1.5,-2), SIMD3(2,-1.5,-2), SIMD3(2,2,-2), SIMD3(-2,2,-2)],   // 0: front (-z)
            [SIMD3(-2,-1.5, 2), SIMD3(2,-1.5, 2), SIMD3(2,2, 2), SIMD3(-2,2, 2)],   // 1: back (+z)
            [SIMD3( 2,-1.5,-2), SIMD3(2,-1.5, 2), SIMD3(2,2, 2), SIMD3(2,2,-2)],    // 2: right (+x)
            [SIMD3(-2,-1.5,-2), SIMD3(-2,-1.5,2), SIMD3(-2,2,2), SIMD3(-2,2,-2)],   // 3: left (-x)
            [SIMD3(-2,-1.5,-2), SIMD3(2,-1.5,-2), SIMD3(2,-1.5,2), SIMD3(-2,-1.5,2)], // 4: floor
            [SIMD3(-2, 2,-2), SIMD3(2, 2,-2), SIMD3(2, 2,2), SIMD3(-2, 2,2)],       // 5: ceiling
        ]
        let parts = quads.enumerated().filter { $0.offset != missingWall }.map {
            MeshPart(positions: $0.element, normals: [], indices: [0, 1, 2, 0, 2, 3], transform: matrix_identity_float4x4)
        }
        return MeshSnapshot(parts)
    }

    func testClosedBoxHasNoHoles() {
        XCTAssertTrue(Coverage.missing(mesh: box(), pivot: .zero, homeForward: SIMD3(0, 0, -1)).isEmpty)
    }

    func testMissingWallShowsUpRelativeToHome() {
        let m = Coverage.missing(mesh: box(without: 2), pivot: .zero, homeForward: SIMD3(0, 0, -1))  // right wall (+x) gone
        XCTAssertFalse(m.isEmpty)
        XCTAssertTrue(m.allSatisfy { $0.yaw > 30 && $0.yaw < 150 }, "\(m)")   // to the right of home: +yaw with yawSign +1
    }
```

- [ ] **Step 2: Implement `Coverage.swift`:**

```swift
import simd

/// Which directions around the gimbal's pivot still see no surface, as yaw/pitch relative to home.
enum Coverage {
    /// +1 when target yaw turns the camera clockwise seen from above (right); the bench test sets it.
    nonisolated(unsafe) static var yawSign = 1.0
    static let cell = 10.0, reach = 5.0, elevations = (-45.0, 65.0)

    struct Direction: Equatable { let yaw: Double; let pitch: Double }

    /// Cell centres with no mesh hit within `reach` from `pivot`. `homeForward` is the camera's forward at home
    /// (ARKit frame, y up).
    static func missing(mesh: MeshSnapshot, pivot: SIMD3<Double>, homeForward: SIMD3<Double>) -> [Direction] {
        let f = simd_normalize(SIMD3(homeForward.x, 0, homeForward.z))
        let homeAz = atan2(f.x, -f.z) * 180 / .pi           // clockwise from -z
        var out: [Direction] = []
        var el = elevations.0 + cell / 2
        while el < elevations.1 {
            var az = cell / 2
            while az < 360 {
                let a = az * .pi / 180, e = el * .pi / 180
                let dir = SIMD3(sin(a) * cos(e), sin(e), -cos(a) * cos(e))
                let hit = mesh.hits(origin: pivot, dir: dir).first.map { $0 <= reach } ?? false
                if !hit {
                    var yaw = az - homeAz
                    yaw = (yaw + 540).truncatingRemainder(dividingBy: 360) - 180
                    out.append(Direction(yaw: yaw * yawSign, pitch: el))
                }
                az += cell
            }
            el += cell
        }
        return out
    }
}
```

- [ ] **Step 3:** Run tests, commit "capture-ios: coverage of directions around the gimbal pivot".

---

### Task 6: Pass flags in the core

**Goal:** A pass records why it ended early (`tracking lost`, `gimbal lost`, `low battery`), and whether it was a gimbal pass.

**Files:** `crates/cave-capture/src/passes.rs`, `crates/cave-capture/tests/passes.rs`, `apps/capture-ios/rust/src/lib.rs` (`PassIn.flags`), `apps/capture-ios/Sources/ShotRecorder.swift` (`ShotJob` passes flags).

**Acceptance Criteria:**
- [ ] `Pass` gains `#[serde(default)] pub flags: Vec<String>`; `NewPass` gains `flags: Vec<String>`; `add_pass` copies them; an old file without flags reads with `[]`.
- [ ] `ShotHeader` gains `#[serde(default)] pub flags: Vec<String>` (the flags of a shot's first pass); `add_pass` copies them into the synthesised first pass record.
- [ ] `cc_shot_add_pass` accepts `"flags"`; `ShotJob` gains `flags: [String]` and writes them (header `flags` for a new shot, pass JSON `flags` for a pass). Every gimbal pass gets the flag `gimbal`.

**Verify:** `cargo test -p cave-capture`, `cd apps/capture-ios/rust && cargo test`, app tests.

**Steps:**
- [ ] **Step 1:** Tests in `crates/cave-capture/tests/passes.rs`: flags round-trip through `add_pass` + encode/decode; header flags become the synthesised first pass's flags; old JSON without flags reads `[]`.
- [ ] **Step 2:** Implement (fields with `///` docs, `#[serde(default)]`), update every `ShotHeader { .. }` and `NewPass { .. }` literal, `PassIn` and the FFI test, `ShotJob` init parameter `flags: [String] = []` and its header/pass JSON.
- [ ] **Step 3:** Code index, fmt (crate only), clippy, tests; commit "cave-capture: pass flags (gimbal, tracking lost, gimbal lost, low battery)".

---

### Task 7: Wire it into the capture screen

**Goal:** A Gimbal switch on the capture screen runs `GimbalRun` for a pass; Pause/Stop work as now; the pass saves with flags and the phone chimes and vibrates when the gimbal pass finishes.

**Files:** `apps/capture-ios/Sources/ShotRecorder.swift`, `apps/capture-ios/Sources/CaptureView.swift`, `apps/capture-ios/Tests/ShotRecorderTests.swift`

**Acceptance Criteria:**
- [ ] `ShotRecorder` has `@Published var gimbalMode = false`, `@Published var gimbalStatus: GimbalStatus`, `var gimbal: GimbalDriver = CoreGimbal()` (injectable), and a 4 Hz timer while `gimbalMode` polls status and, while capturing a gimbal pass, calls `run.tick(inputs)`.
- [ ] Turning the switch on calls `gimbal.connect()` and shows "Gimbal: connecting… / connected / not found (reason)"; the switch stays on.
- [ ] Start in gimbal mode: `GimbalRun.check(...)` first (tracking from the current frame, lux from `frame.lightEstimate?.ambientIntensity`, battery from `UIDevice.current.batteryLevel` with monitoring enabled); a refusal shows its message and doesn't start. Otherwise the normal `start()` then `run.start`.
- [ ] `Inputs.missing` is recomputed every 5 s off the main thread (`Coverage.missing` on a snapshot of mesh anchors near the pivot = the camera position at Start; `homeForward` = camera −z at Start).
- [ ] `.finished(flags)` → `pause(cave:over:)` with flags `["gimbal"] + flags` → on save: chime (`AudioServicesPlaySystemSound(1025)`) and success haptic (`UINotificationFeedbackGenerator().notificationOccurred(.success)`).
- [ ] Pause during a gimbal pass: `gimbal.hold()` then the normal Pause save (gimbal stays where it is). Stop: `gimbal.stopHome()` then the normal Stop.
- [ ] `UIApplication.shared.isIdleTimerDisabled = true` for the whole gimbal pass (already true while capturing).
- [ ] Log lines: `gimbal: connected / not found: …`, `gimbal: sweep N steps`, `gimbal: hold (reason)`, `gimbal: go on`, `gimbal: fill N targets (coverage a/b cells)`, `gimbal: finished [flags] in S s`.
- [ ] Test with `FakeGimbal` injected: gimbal mode Start refuses when the fake isn't connected (message shown, phase stays ready).

**Verify:** app tests `** TEST SUCCEEDED **`; device build `** BUILD SUCCEEDED **`; installed on the phone.

**Steps:**
- [ ] **Step 1:** Write the refusal test (fake gimbal, not connected → `rec.message == "Gimbal not connected."`, phase `.ready`).
- [ ] **Step 2:** Implement in `ShotRecorder` (read the file first: `start()`, `pause(cave:over:)`, `finish(cave:over:)`, `session(_:didUpdate:)`), keeping heavy work off the main actor exactly as the card mesh passes do (`Self.meshQueue`).
- [ ] **Step 3:** `CaptureView`: a `Toggle("Gimbal", isOn: $rec.gimbalMode)` styled to match the big controls panel (white text, large), plus a one-line gimbal status above the buttons when on ("Gimbal connected", "Sweeping 4/11", "Holding: too dark", …).
- [ ] **Step 4:** Tests, device build, install; commit "capture-ios: gimbal passes on the capture screen (switch, checks, sweep, fill, holds, chime)".

---

### Task 8: Bench test with Nathan

**Goal:** On the real OM7 and iPhone: the sweep covers the room, signs are right, speed keeps frames sharp, light threshold is sensible, fill improves coverage.

**Files:** `tools/cave-osmo/FINDINGS.md` (bench results), constants in `camera.rs` (`OM7_CAMERA`), `GimbalRun.speed`, `GimbalRun.minLux`, `Coverage.yawSign` if the bench says so.

**Acceptance Criteria:**
- [ ] The log shows `gimbal: connected`, a sweep of N steps finishing, `gimbal: fill … (coverage a/b cells)` with b-a improving after fill, and `gimbal: finished [gimbal]`.
- [ ] Nathan confirms on the phone: the camera swept the full pan both rows, no flips, and the saved pass looks complete in the viewer.
- [ ] Yaw sign confirmed (fill aimed at the real hole); speed and light threshold recorded in FINDINGS.

**Verify:** `apps/capture-ios/pull-log.sh 00008150-0011281122D2401C` → the log lines above.

**Steps:**
- [ ] **Step 1:** Install. Nathan: Bluetooth on, Mimo closed, gimbal powered, phone portrait in the clamp, area lights on, tripod over a station, cards out.
- [ ] **Step 2:** Gimbal switch on → "connected"; Start. Watch the sweep; afterwards read the log.
- [ ] **Step 3:** If fill aimed the wrong way, flip `Coverage.yawSign`; tune `GimbalRun.speed` if frames smeared or the mesh lagged; set `minLux` from the log's light values. Record in FINDINGS; commit "Bench: gimbal capture tuned (speed, light, yaw sign)".

---

## Self-review notes

- Spec coverage: merge (T1), door + runner (T3), patterns sweep/fill (T2, T4 wrappers), coverage (T5), flow + checks + chime + Pause/Stop (T7), holds/failures (T4, T7), flags in pass record (T6), lightweight (runner thread, 4 Hz poll, coverage every 5 s off main: T3/T7), log (T7), testing (T2-T7, bench T8). Not built per spec: landscape mount, roll reach, second tripod position, X1 during LiDAR.
- Names used across tasks: `cave_osmo::camera::{Window, OM7_CAMERA, MAX_MOVE_S, sweep, fill}`; `gimbal::{State, Status, mount_ok, parse_plan, connect, run_plan, hold, go_on, stop_home, disconnect, status}`; doors `cc_gimbal_{connect,run,hold,go_on,stop_home,disconnect,status}`, `cc_camera_{sweep,fill}`; Swift `Core.gimbal{Connect,Run,Hold,GoOn,StopHome,Disconnect,Status}`, `Core.camera{Sweep,Fill}`; `GimbalDriver`, `GimbalStatus`, `CoreGimbal`, `FakeGimbal` (tests), `GimbalRun` (`check`, `start`, `tick`, `Inputs`, `Outcome`, `Tracking`, `message`), `Coverage.missing`, `Coverage.Direction`, `Coverage.yawSign`.
- Type check: `Coverage.missing` returns `[Coverage.Direction]`; `GimbalRun.Inputs.missing` is `[(Double, Double)]` — Task 7 maps `Direction` to `(yaw, pitch)` when filling `Inputs`.
- Flags: header `flags` = the first pass's flags; pass JSON `flags` for later passes (Task 6).
