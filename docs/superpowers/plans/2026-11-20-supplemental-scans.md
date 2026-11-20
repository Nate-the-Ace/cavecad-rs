# Supplemental Scans Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers-extended-cc:subagent-driven-development (recommended) or superpowers-extended-cc:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Scan more at a station on the same visit: the earlier scan shows as a grey low-poly ghost with red hole edges, the new pass is added into the station's one shot file, lined up by kept ARKit tracking or, after a restart, by the station cards; card work backs off once cards settle.

**Architecture:** Pure geometry in the Rust core `crates/cave-capture` (new module `passes.rs`: pass record, `add_pass`, `ghost`, `cards_fit`), exposed through three new FFI doors in `apps/capture-ios/rust` and `Core.swift`. The iPhone app keeps its `ARSession` across passes, finds the station's parent shot, draws the ghost in the AR view, and saves each pass by rewriting the file atomically.

**Tech Stack:** Rust 2024 (serde, serde_json), C FFI + xcframework, Swift 6 / SwiftUI / ARKit / RealityKit, XcodeGen, XCTest.

**User decisions (already made):**
- "Same visit, minutes later": supplemental scans happen right after the first, cards still in place.
- "Added into the first file": one file per station; each pass is added into it.
- "Grey ghost + red hole edges": the earlier scan as a see-through low-poly shell with red hole outlines.
- "Back off once settled": card detection slows once cards settle; live markers stay.
- "A, with C as backup": keep ARKit tracking between passes; after a restart line up from the cards; no saved world map.
- Hand scans are normal; on-station gimbal scans stay supported.
- Spec: `docs/superpowers/specs/2026-11-20-supplemental-scans-design.md` (approved).

**Repo rules that bind every task:** read `CLAUDE.md` (root). Core crates stay pure (no files, clock, threads) and wasm32-safe; a `///` doc on every public item; after adding public items run `python3 tools/make_code_index.py` and commit `docs/CODE-INDEX.md`; format only the crate you touched (`cargo fmt -p cave-capture`); use rustup's toolchain: `export PATH=/opt/homebrew/opt/rustup/bin:$PATH`.

**Commands:**
- Core: `cargo test -p cave-capture` and `cargo clippy -p cave-capture --all-targets -- -D warnings`
- FFI: `cd apps/capture-ios/rust && cargo test && cargo clippy --all-targets -- -D warnings`
- App: `cd apps/capture-ios && ./rust/build-xcframework.sh && xcodegen generate && xcodebuild test -scheme CaveCADCapture -destination 'platform=iOS Simulator,name=iPhone 18 Pro' CODE_SIGNING_ALLOWED=NO 2>&1 | grep -E "error:|Executed|TEST (SUCCEEDED|FAILED)"`
- Phone: `apps/capture-ios/install.sh 2ANLK72UUS 00008150-0011281122D2401C`, log: `apps/capture-ios/pull-log.sh 00008150-0011281122D2401C`

## File map

| File | Responsibility |
|---|---|
| `crates/cave-capture/src/passes.rs` (new) | `Pass`, `NewPass`, `PassMesh`, `add_pass`, `Ghost`, `ghost`, `CardFit`, `cards_fit`, transform helpers |
| `crates/cave-capture/src/bundle.rs` | `ShotHeader.passes` field |
| `crates/cave-capture/src/lib.rs` | re-exports |
| `crates/cave-capture/tests/passes.rs` (new) | core tests |
| `apps/capture-ios/rust/src/lib.rs` + `include/capture_core.h` | `cc_shot_add_pass`, `cc_shot_ghost`, `cc_cards_fit` |
| `apps/capture-ios/Sources/Core.swift` | Swift wrappers |
| `apps/capture-ios/Sources/CardTracking.swift` | `settled`, settled-aware mesh batch |
| `apps/capture-ios/Sources/CardDetector.swift` | per-run decimate/threads |
| `apps/capture-ios/Sources/ShotFiles.swift` | parent lookup, atomic replace |
| `apps/capture-ios/Sources/ShotRecorder.swift` | kept session, passes, backoff, card fit/check, save |
| `apps/capture-ios/Sources/GhostView.swift` (new) | ghost + outline RealityKit entities |
| `apps/capture-ios/Sources/CaptureView.swift` | Scan more, ghost, warnings |

---

### Task 1: Pass record and `add_pass` in the core

**Goal:** A shot file can hold several passes: `ShotHeader.passes`, and `add_pass(old, new)` returns the combined shot (same-session passes replace the mesh; card-lined passes are transformed and added).

**Files:**
- Create: `crates/cave-capture/src/passes.rs`
- Modify: `crates/cave-capture/src/bundle.rs` (ShotHeader, after `cards`), `crates/cave-capture/src/lib.rs`
- Modify: every `ShotHeader { ... }` literal: `crates/cave-capture/src/bundle.rs`, `crates/cave-capture/tests/bundle.rs` (add `passes: vec![]`)
- Test: `crates/cave-capture/tests/passes.rs`

**Acceptance Criteria:**
- [ ] An old header JSON with no `passes` decodes with `passes == []`.
- [ ] `add_pass` on a shot with no passes records two passes: `first` (identity, keyframes 0..n) and the new one.
- [ ] `PassMesh::Replace` keeps only the new mesh; `PassMesh::Add` appends it with indices offset.
- [ ] A `cards` pass's positions, normals, keyframe poses and cards are moved by its transform; old keyframe JPEG bytes are kept byte-for-byte.
- [ ] Top-level `cards`: a card in the new pass replaces the same id; others are kept.

**Verify:** `cargo test -p cave-capture --test passes` → `test result: ok`

**Steps:**

- [ ] **Step 1: Write the failing tests** in `crates/cave-capture/tests/passes.rs`:

```rust
use cave_capture::{
    CardSeen, Keyframe, NewPass, PassMesh, Shot, ShotHeader, add_pass, decode, encode,
};

fn card(id: u32, p: [f64; 3]) -> CardSeen {
    CardSeen { id, word: format!("#{id}"), point: p, corners: vec![], sightings: 9, spread_m: 0.005 }
}

fn header() -> ShotHeader {
    ShotHeader {
        shot_id: "s".into(), cave_id: "c".into(), station_id: "a".into(), station_label: "A".into(),
        captured_at_ms: 1000, device: "t".into(), instrument_height_m: 0.3, start_camera: [0.0; 3],
        taps: vec![], tracking: "normal".into(), arkit_version: "27".into(), gravity: [0.0, -1.0, 0.0],
        compass: None, cards: vec![card(2, [0.0, -1.0, -1.0]), card(12, [0.0, -1.0, -4.0])], passes: vec![],
    }
}

fn kf(tag: u8) -> Keyframe {
    let mut pose = [0f32; 16];
    pose[0] = 1.0; pose[5] = 1.0; pose[10] = 1.0; pose[15] = 1.0;
    Keyframe { pose, intrinsics: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], width: 2, height: 2, jpeg: vec![tag; 5] }
}

fn shot() -> Shot {
    Shot {
        header: header(),
        positions: vec![0.0, 0.0, -1.0, 1.0, 0.0, -1.0, 0.0, 1.0, -1.0],
        normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
        indices: vec![0, 1, 2],
        keyframes: vec![kf(1), kf(2)],
    }
}

/// Column-major: turn `yaw` about +y, then shift.
fn yaw_shift(yaw: f64, t: [f64; 3]) -> [f64; 16] {
    let (s, c) = yaw.sin_cos();
    [c, 0.0, -s, 0.0, 0.0, 1.0, 0.0, 0.0, s, 0.0, c, 0.0, t[0], t[1], t[2], 1.0]
}

fn identity() -> [f64; 16] { yaw_shift(0.0, [0.0; 3]) }

fn new_pass(aligned_by: &str, transform: [f64; 16], mesh: PassMesh) -> NewPass {
    NewPass {
        captured_at_ms: 2000, aligned_by: aligned_by.into(), transform, mesh,
        positions: vec![0.0, 0.0, -2.0, 1.0, 0.0, -2.0, 0.0, 1.0, -2.0],
        normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
        indices: vec![0, 1, 2],
        keyframes: vec![kf(9)],
        cards: vec![card(12, [0.0, -1.0, -4.1]), card(26, [0.0, -1.0, 2.0])],
        tracking: "limited".into(),
    }
}

#[test]
fn old_header_without_passes_still_reads() {
    let bytes = encode(&shot()).unwrap();
    assert!(decode(&bytes).unwrap().header.passes.is_empty());
}

#[test]
fn same_session_pass_replaces_the_mesh_and_appends_photos() {
    let out = add_pass(&shot(), new_pass("same_session", identity(), PassMesh::Replace)).unwrap();
    assert_eq!(out.indices, vec![0, 1, 2]);
    assert_eq!(out.positions[2], -2.0);
    assert_eq!(out.keyframes.len(), 3);
    assert_eq!(out.keyframes[0].jpeg, vec![1; 5]);
    assert_eq!(out.keyframes[2].jpeg, vec![9; 5]);
    let p = &out.header.passes;
    assert_eq!(p.len(), 2);
    assert_eq!((p[0].aligned_by.as_str(), p[0].first_keyframe, p[0].keyframe_count), ("first", 0, 2));
    assert_eq!((p[1].aligned_by.as_str(), p[1].first_keyframe, p[1].keyframe_count), ("same_session", 2, 1));
    assert_eq!(p[0].cards.len(), 2);
    assert_eq!(out.header.tracking, "limited");
    // cards: 12 replaced by the new pass, 2 kept, 26 added; in id order
    let ids: Vec<u32> = out.header.cards.iter().map(|c| c.id).collect();
    assert_eq!(ids, vec![2, 12, 26]);
    assert!((out.header.cards[1].point[2] + 4.1).abs() < 1e-12);
    // and it round-trips through the file format
    assert_eq!(decode(&encode(&out).unwrap()).unwrap(), out);
}

#[test]
fn card_pass_is_moved_and_added() {
    let t = yaw_shift(std::f64::consts::FRAC_PI_2, [1.0, 0.0, 0.0]);
    let out = add_pass(&shot(), new_pass("cards", t, PassMesh::Add)).unwrap();
    assert_eq!(out.indices, vec![0, 1, 2, 3, 4, 5]);
    // new vertex (0,0,-2) turned 90 deg about +y -> (-2,0,0), shifted -> (-1,0,0)
    assert!((out.positions[9] + 1.0).abs() < 1e-6 && out.positions[11].abs() < 1e-6);
    // its normal (0,0,1) turns to (1,0,0)
    assert!((out.normals[9] - 1.0).abs() < 1e-6);
    // the new photo's pose is moved too: translation column becomes (1,0,0)
    assert!((out.keyframes[2].pose[12] - 1.0).abs() < 1e-6);
    // the new pass's card 26 at (0,-1,2) -> (2,-1,0) + (1,0,0) = (3,-1,0)
    let c26 = out.header.cards.iter().find(|c| c.id == 26).unwrap();
    assert!((c26.point[0] - 3.0).abs() < 1e-9 && c26.point[2].abs() < 1e-9);
    assert_eq!(out.header.passes[1].transform, t);
}

#[test]
fn third_pass_extends_the_list() {
    let two = add_pass(&shot(), new_pass("same_session", identity(), PassMesh::Replace)).unwrap();
    let three = add_pass(&two, new_pass("same_session", identity(), PassMesh::Replace)).unwrap();
    assert_eq!(three.header.passes.len(), 3);
    assert_eq!(three.header.passes[2].first_keyframe, 3);
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p cave-capture --test passes`
Expected: FAIL to compile (`NewPass`, `PassMesh`, `add_pass`, `passes` not found).

- [ ] **Step 3: Add the field** in `crates/cave-capture/src/bundle.rs`, after `pub cards: Vec<CardSeen>,`:

```rust
    /// The scanning passes this shot was built from, first one first; absent (empty) in a shot of
    /// one pass written before passes existed. See [`crate::passes`].
    #[serde(default)]
    pub passes: Vec<crate::passes::Pass>,
```

Add `passes: vec![],` to every `ShotHeader { .. }` literal (`grep -rn "ShotHeader {" crates apps/capture-ios/rust/src`).

- [ ] **Step 4: Write `crates/cave-capture/src/passes.rs`** (part 1; Tasks 2 and 3 append to it):

```rust
//! Several scanning passes in one shot: the pass record, adding a pass, the ghost of a scan, and
//! lining a pass up from the station cards. Pure: bytes and numbers in, data out.

use crate::bundle::{BundleError, CardSeen, Keyframe, Shot};
use serde::{Deserialize, Serialize};

/// One scanning pass of a shot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pass {
    /// When the pass was captured (milliseconds since 1970, from the phone).
    pub captured_at_ms: i64,
    /// How it was lined up with the first pass: `first`, `same_session` or `cards`.
    pub aligned_by: String,
    /// Column-major 4x4 that moved this pass's data into the shot's frame (identity unless `cards`).
    pub transform: [f64; 16],
    /// Index of this pass's first keyframe in the shot.
    pub first_keyframe: u32,
    /// How many keyframes this pass added.
    pub keyframe_count: u32,
    /// The cards as this pass placed them, in the shot's frame.
    #[serde(default)]
    pub cards: Vec<CardSeen>,
}

/// What a new pass does to the shot's mesh.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassMesh {
    /// The pass's mesh is the whole room as ARKit now holds it (same session): it replaces the old.
    Replace,
    /// The pass's mesh is new geometry only (tracking was lost): it is added to the old.
    Add,
}

/// A pass to add to a shot, in its own capture frame.
#[derive(Clone, Debug, PartialEq)]
pub struct NewPass {
    /// When the pass was captured (milliseconds since 1970).
    pub captured_at_ms: i64,
    /// `same_session` or `cards`.
    pub aligned_by: String,
    /// Column-major 4x4 from this pass's frame to the shot's frame.
    pub transform: [f64; 16],
    /// Replace or add the mesh.
    pub mesh: PassMesh,
    /// Vertex positions, xyz per vertex.
    pub positions: Vec<f32>,
    /// Vertex normals, xyz per vertex.
    pub normals: Vec<f32>,
    /// Triangle indices, three per triangle.
    pub indices: Vec<u32>,
    /// The pass's kept photos.
    pub keyframes: Vec<Keyframe>,
    /// The cards the pass placed.
    pub cards: Vec<CardSeen>,
    /// How tracking went in this pass: `normal`, `limited` or `lost`.
    pub tracking: String,
}

/// The 4x4 identity, column-major.
pub const IDENTITY: [f64; 16] = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.];

/// A point moved by a column-major 4x4.
pub fn apply_point(m: &[f64; 16], p: [f64; 3]) -> [f64; 3] {
    [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
    ]
}

/// A direction turned by a column-major 4x4 (no shift).
pub fn apply_dir(m: &[f64; 16], v: [f64; 3]) -> [f64; 3] {
    [
        m[0] * v[0] + m[4] * v[1] + m[8] * v[2],
        m[1] * v[0] + m[5] * v[1] + m[9] * v[2],
        m[2] * v[0] + m[6] * v[1] + m[10] * v[2],
    ]
}

/// a * b, column-major 4x4.
pub fn mul(a: &[f64; 16], b: &[f64; 16]) -> [f64; 16] {
    let mut o = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            o[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    o
}

fn move_card(m: &[f64; 16], c: &CardSeen) -> CardSeen {
    CardSeen {
        point: apply_point(m, c.point),
        corners: c.corners.iter().map(|p| apply_point(m, *p)).collect(),
        ..c.clone()
    }
}

fn worst(a: &str, b: &str) -> String {
    let rank = |t: &str| match t { "lost" => 2, "limited" => 1, _ => 0 };
    if rank(b) > rank(a) { b.into() } else { a.into() }
}

/// The shot with `p` added: photos appended, mesh replaced or added, cards merged (a card the new
/// pass placed replaces the same id), and the pass recorded. A shot with no pass list gets its
/// first pass recorded too. `BadMesh` for ragged new arrays or an index past the new vertices.
pub fn add_pass(old: &Shot, p: NewPass) -> Result<Shot, BundleError> {
    if p.positions.len() % 3 != 0 || p.normals.len() != p.positions.len() || p.indices.len() % 3 != 0 {
        return Err(BundleError::BadMesh);
    }
    let n_new = (p.positions.len() / 3) as u32;
    if p.indices.iter().any(|&i| i >= n_new) {
        return Err(BundleError::BadMesh);
    }
    let m = p.transform;
    let mut positions = Vec::with_capacity(p.positions.len());
    let mut normals = Vec::with_capacity(p.normals.len());
    for v in p.positions.chunks_exact(3) {
        let q = apply_point(&m, [v[0] as f64, v[1] as f64, v[2] as f64]);
        positions.extend(q.iter().map(|x| *x as f32));
    }
    for v in p.normals.chunks_exact(3) {
        let q = apply_dir(&m, [v[0] as f64, v[1] as f64, v[2] as f64]);
        normals.extend(q.iter().map(|x| *x as f32));
    }
    let mut out = old.clone();
    if out.header.passes.is_empty() {
        out.header.passes.push(Pass {
            captured_at_ms: old.header.captured_at_ms,
            aligned_by: "first".into(),
            transform: IDENTITY,
            first_keyframe: 0,
            keyframe_count: old.keyframes.len() as u32,
            cards: old.header.cards.clone(),
        });
    }
    match p.mesh {
        PassMesh::Replace => {
            out.positions = positions;
            out.normals = normals;
            out.indices = p.indices.clone();
        }
        PassMesh::Add => {
            let base = (out.positions.len() / 3) as u32;
            out.positions.extend(positions);
            out.normals.extend(normals);
            out.indices.extend(p.indices.iter().map(|i| i + base));
        }
    }
    let first_keyframe = out.keyframes.len() as u32;
    for k in &p.keyframes {
        let mut pose64 = [0.0; 16];
        for (d, s) in pose64.iter_mut().zip(k.pose.iter()) {
            *d = *s as f64;
        }
        let moved = mul(&m, &pose64);
        let mut pose = [0f32; 16];
        for (d, s) in pose.iter_mut().zip(moved.iter()) {
            *d = *s as f32;
        }
        out.keyframes.push(Keyframe { pose, ..k.clone() });
    }
    let cards: Vec<CardSeen> = p.cards.iter().map(|c| move_card(&m, c)).collect();
    let mut merged: Vec<CardSeen> =
        out.header.cards.iter().filter(|c| !cards.iter().any(|n| n.id == c.id)).cloned().collect();
    merged.extend(cards.iter().cloned());
    merged.sort_by_key(|c| c.id);
    out.header.cards = merged;
    out.header.tracking = worst(&out.header.tracking, &p.tracking);
    out.header.passes.push(Pass {
        captured_at_ms: p.captured_at_ms,
        aligned_by: p.aligned_by,
        transform: m,
        first_keyframe,
        keyframe_count: p.keyframes.len() as u32,
        cards,
    });
    Ok(out)
}
```

In `crates/cave-capture/src/lib.rs` add `pub mod passes;` and
`pub use passes::{IDENTITY, NewPass, Pass, PassMesh, add_pass};`.

- [ ] **Step 5: Run tests and checks**

Run: `cargo test -p cave-capture && cargo clippy -p cave-capture --all-targets -- -D warnings && cargo fmt -p cave-capture`
Expected: all pass, no warnings.

- [ ] **Step 6: Index and commit**

```bash
python3 tools/make_code_index.py
git add crates/cave-capture docs/CODE-INDEX.md
git commit -m "cave-capture: passes in a shot and add_pass (replace or add the mesh, move a card-lined pass)"
```

---

### Task 2: `ghost`: low-poly shell and hole outlines

**Goal:** `ghost(positions, indices, cell_m, min_outline_m)` returns a vertex-clustered mesh and the open-edge outlines of that mesh, dropping outlines shorter than `min_outline_m`.

**Files:**
- Modify: `crates/cave-capture/src/passes.rs` (append), `crates/cave-capture/src/lib.rs` (re-export `Ghost, ghost`)
- Test: `crates/cave-capture/tests/passes.rs` (append)

**Acceptance Criteria:**
- [ ] A 1 m square sheet of 1 cm triangles with a 0.3 m square hole gives exactly 2 outlines: about 4.0 m and about 1.2 m long (within 0.15 m).
- [ ] A closed box gives 0 outlines.
- [ ] The sheet at `cell_m` 0.05 has at most 2 x 21 x 21 triangles.
- [ ] Seams between separately-indexed patches (ARKit anchors) disappear: two halves of a sheet with duplicated edge vertices give only the outer outline.

**Verify:** `cargo test -p cave-capture --test passes ghost` → `test result: ok`

**Steps:**

- [ ] **Step 1: Write the failing tests** (append to `tests/passes.rs`):

```rust
use cave_capture::ghost;

/// A flat sheet in x/z at y = 0, `n` x `n` cells of `step` m, skipping cells whose centre falls in
/// the square hole `[h0, h1)` on both axes. Starts at `x0`.
fn sheet(n: usize, step: f32, x0: f32, hole: Option<(f32, f32)>) -> (Vec<f32>, Vec<u32>) {
    let mut pos = Vec::new();
    for j in 0..=n { for i in 0..=n { pos.extend([x0 + i as f32 * step, 0.0, j as f32 * step]); } }
    let mut idx = Vec::new();
    let w = (n + 1) as u32;
    for j in 0..n { for i in 0..n {
        let (cx, cz) = (x0 + (i as f32 + 0.5) * step, (j as f32 + 0.5) * step);
        if let Some((a, b)) = hole { if cx >= a && cx < b && cz >= a && cz < b { continue; } }
        let v = j as u32 * w + i as u32;
        idx.extend([v, v + w, v + 1, v + 1, v + w, v + w + 1]);
    } }
    (pos, idx)
}

fn length(o: &[[f32; 3]]) -> f32 {
    o.windows(2).map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2) + (w[1][2] - w[0][2]).powi(2)).sqrt()).sum()
}

#[test]
fn ghost_outlines_the_hole_and_the_edge() {
    let (p, i) = sheet(100, 0.01, 0.0, Some((0.35, 0.65)));
    let g = ghost(&p, &i, 0.05, 0.15);
    assert!(g.indices.len() / 3 <= 2 * 21 * 21, "{} triangles", g.indices.len() / 3);
    let mut lens: Vec<f32> = g.outlines.iter().map(|o| length(o)).collect();
    lens.sort_by(f32::total_cmp);
    assert_eq!(lens.len(), 2, "{lens:?}");
    assert!((lens[0] - 1.2).abs() < 0.15 && (lens[1] - 4.0).abs() < 0.15, "{lens:?}");
}

#[test]
fn ghost_welds_anchor_seams() {
    let (mut p, mut i) = sheet(50, 0.01, 0.0, None);
    let (p2, i2) = sheet(50, 0.01, 0.5, None);
    let base = (p.len() / 3) as u32;
    p.extend(p2);
    i.extend(i2.iter().map(|v| v + base));
    let g = ghost(&p, &i, 0.05, 0.15);
    assert_eq!(g.outlines.len(), 1);
    assert!((length(&g.outlines[0]) - 3.0).abs() < 0.15);
}

#[test]
fn closed_box_has_no_outline() {
    let p: Vec<f32> = [[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.], [0., 0., 1.], [1., 0., 1.], [1., 1., 1.], [0., 1., 1.]]
        .iter().flatten().copied().collect();
    let i = vec![0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0, 7, 3, 1, 2, 6, 1, 6, 5];
    assert!(ghost(&p, &i, 0.05, 0.15).outlines.is_empty());
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p cave-capture --test passes ghost`
Expected: FAIL to compile (`ghost` not found).

- [ ] **Step 3: Implement** (append to `passes.rs`):

```rust
use std::collections::{BTreeMap, HashMap};

/// A scan made light for drawing over a live view: a simplified mesh and its hole outlines.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Ghost {
    /// Vertex positions, xyz per vertex.
    pub positions: Vec<f32>,
    /// Triangle indices, three per triangle.
    pub indices: Vec<u32>,
    /// Open edges of the simplified mesh joined into polylines (a closed loop repeats its first
    /// point at the end); each at least `min_outline_m` long.
    pub outlines: Vec<Vec<[f32; 3]>>,
}

/// Simplifies a mesh by vertex clustering on a `cell_m` grid (each cell's vertices become their
/// mean; triangles that collapse or repeat are dropped), then outlines its holes: edges used by one
/// triangle only, chained, dropping chains shorter than `min_outline_m`. Clustering also welds the
/// seams between ARKit's mesh anchors, so only real gaps and the scan's edge are outlined.
pub fn ghost(positions: &[f32], indices: &[u32], cell_m: f32, min_outline_m: f32) -> Ghost {
    let n = positions.len() / 3;
    if cell_m <= 0.0 || n == 0 {
        return Ghost::default();
    }
    let mut cell_of: HashMap<(i32, i32, i32), u32> = HashMap::new();
    let mut sums: Vec<([f64; 3], u32)> = Vec::new();
    let mut map = vec![0u32; n];
    for (v, p) in positions.chunks_exact(3).enumerate() {
        let key = ((p[0] / cell_m).floor() as i32, (p[1] / cell_m).floor() as i32, (p[2] / cell_m).floor() as i32);
        let id = *cell_of.entry(key).or_insert_with(|| {
            sums.push(([0.0; 3], 0));
            (sums.len() - 1) as u32
        });
        let s = &mut sums[id as usize];
        for k in 0..3 { s.0[k] += p[k] as f64; }
        s.1 += 1;
        map[v] = id;
    }
    let mut out = Ghost::default();
    for (s, c) in &sums {
        out.positions.extend(s.iter().map(|x| (*x / *c as f64) as f32));
    }
    let mut seen = std::collections::HashSet::new();
    for t in indices.chunks_exact(3) {
        if t.iter().any(|&i| i as usize >= n) { continue; }
        let (a, b, c) = (map[t[0] as usize], map[t[1] as usize], map[t[2] as usize]);
        if a == b || b == c || a == c { continue; }
        let mut key = [a, b, c];
        key.sort_unstable();
        if !seen.insert(key) { continue; }
        out.indices.extend([a, b, c]);
    }
    // open edges: used by exactly one triangle
    let mut uses: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for t in out.indices.chunks_exact(3) {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            *uses.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }
    let mut next: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (&(a, b), &u) in &uses {
        if u == 1 {
            next.entry(a).or_default().push(b);
            next.entry(b).or_default().push(a);
        }
    }
    let at = |v: u32| { let i = v as usize * 3; [out.positions[i], out.positions[i + 1], out.positions[i + 2]] };
    let dist = |a: [f32; 3], b: [f32; 3]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
    let mut outlines = Vec::new();
    while let Some((&start, _)) = next.iter().find(|(_, v)| !v.is_empty()) {
        let mut line = vec![at(start)];
        let mut len = 0.0f32;
        let mut cur = start;
        while let Some(nb) = next.get_mut(&cur).and_then(|v| v.pop()) {
            if let Some(back) = next.get_mut(&nb) {
                if let Some(k) = back.iter().position(|&x| x == cur) { back.swap_remove(k); }
            }
            let p = at(nb);
            len += dist(*line.last().unwrap(), p);
            line.push(p);
            cur = nb;
            if cur == start { break; }
        }
        if len >= min_outline_m { outlines.push(line); }
    }
    out.outlines = outlines;
    out
}
```

Add `Ghost, ghost` to the `pub use passes::{...}` line in `lib.rs`.

- [ ] **Step 4: Run tests and checks**

Run: `cargo test -p cave-capture && cargo clippy -p cave-capture --all-targets -- -D warnings && cargo fmt -p cave-capture`
Expected: all pass. If the hole outline measures short (cluster means pull edges inward by up to half a cell), widen the tolerance to 0.2 m in the test only after printing the lengths and confirming both outlines exist; do not change the 2-outline count.

- [ ] **Step 5: Commit**

```bash
python3 tools/make_code_index.py
git add crates/cave-capture docs/CODE-INDEX.md
git commit -m "cave-capture: ghost (vertex-clustered shell and hole outlines)"
```

---

### Task 3: `cards_fit`: line a pass up from its cards

**Goal:** From the cards the shot saw and the cards a new pass saw, find the gravity-aligned turn and shift (yaw about +y, then shift) mapping the new frame onto the shot's, with each matched card's misfit; also report plain offsets when no transform is wanted (same session check).

**Files:**
- Modify: `crates/cave-capture/src/passes.rs` (append), `crates/cave-capture/src/lib.rs` (re-export `CardFit, cards_fit, card_offsets`)
- Test: `crates/cave-capture/tests/passes.rs` (append)

**Acceptance Criteria:**
- [ ] With 2 matched cards moved by a known yaw 0.7 rad and shift (1, -0.2, 3), the fit recovers yaw within 1e-9 and every misfit is under 1e-9 m.
- [ ] With 3 cards where one was moved 0.12 m, `worst_m` is above 0.05 and names that card.
- [ ] Fewer than 2 matched cards: `None`.
- [ ] `card_offsets` gives each matched card's plain distance (identity transform).

**Verify:** `cargo test -p cave-capture --test passes cards` → `test result: ok`

**Steps:**

- [ ] **Step 1: Write the failing tests** (append):

```rust
use cave_capture::{apply_point, card_offsets, cards_fit};

#[test]
fn cards_fit_recovers_a_known_turn_and_shift() {
    let old = vec![card(2, [0.3, -1.0, -1.0]), card(12, [1.5, -0.8, -4.0]), card(26, [-0.4, -1.1, 2.0])];
    let t = yaw_shift(0.7, [1.0, -0.2, 3.0]);
    // the new pass sees the same cards in its own frame: new = inverse(t) * old
    let inv = yaw_shift(-0.7, [0.0; 3]);
    let back = |p: [f64; 3]| apply_point(&inv, [p[0] - 1.0, p[1] + 0.2, p[2] - 3.0]);
    let new: Vec<_> = old.iter().take(2).map(|c| card(c.id, back(c.point))).collect();
    let f = cards_fit(&old, &new).unwrap();
    assert!((f.yaw_rad - 0.7).abs() < 1e-9, "{}", f.yaw_rad);
    assert!(f.worst_m < 1e-9);
    for (a, b) in f.transform.iter().zip(t.iter()) { assert!((a - b).abs() < 1e-9); }
    assert_eq!(f.misfits.len(), 2);
}

#[test]
fn cards_fit_names_a_moved_card() {
    let old = vec![card(2, [0.0, -1.0, -1.0]), card(12, [0.0, -1.0, -4.0]), card(26, [0.0, -1.0, 2.0])];
    let mut new = old.clone();
    new[2].point[0] += 0.12;
    let f = cards_fit(&old, &new).unwrap();
    assert!(f.worst_m > 0.05);
    let (id, m) = f.misfits.iter().copied().max_by(|a, b| a.1.total_cmp(&b.1)).unwrap();
    assert_eq!(id, 26);
    assert!(m > 0.05);
    assert!(cards_fit(&old, &new[..1]).is_none());
}

#[test]
fn card_offsets_are_plain_distances() {
    let old = vec![card(2, [0.0, 0.0, 0.0]), card(12, [1.0, 0.0, 0.0])];
    let new = vec![card(2, [0.07, 0.0, 0.0]), card(26, [5.0, 0.0, 0.0])];
    let o = card_offsets(&old, &new);
    assert_eq!(o.len(), 1);
    assert_eq!(o[0].0, 2);
    assert!((o[0].1 - 0.07).abs() < 1e-12);
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p cave-capture --test passes cards`
Expected: FAIL to compile.

- [ ] **Step 3: Implement** (append to `passes.rs`):

```rust
/// How a pass lines up from its cards.
#[derive(Clone, Debug, PartialEq)]
pub struct CardFit {
    /// Turn about +y (up), radians.
    pub yaw_rad: f64,
    /// Column-major 4x4 from the pass's frame to the shot's (turn, then shift).
    pub transform: [f64; 16],
    /// Each matched card's distance from where the shot has it after the fit: (id, metres).
    pub misfits: Vec<(u32, f64)>,
    /// The largest misfit, metres.
    pub worst_m: f64,
}

fn matched<'a>(old: &'a [CardSeen], new: &'a [CardSeen]) -> Vec<(&'a CardSeen, &'a CardSeen)> {
    new.iter().filter_map(|n| old.iter().find(|o| o.id == n.id).map(|o| (o, n))).collect()
}

/// Least-squares turn about +y and shift taking the new pass's cards onto the shot's (gravity is
/// shared, so 4 unknowns). Needs 2 matched cards; a third shows a moved card as a misfit.
pub fn cards_fit(old: &[CardSeen], new: &[CardSeen]) -> Option<CardFit> {
    let m = matched(old, new);
    if m.len() < 2 {
        return None;
    }
    let k = m.len() as f64;
    let mut co = [0.0; 3];
    let mut cn = [0.0; 3];
    for (o, n) in &m {
        for i in 0..3 { co[i] += o.point[i] / k; cn[i] += n.point[i] / k; }
    }
    // R_y(t) u = (c ux + s uz, uy, -s ux + c uz); maximise sum v . R u
    let (mut sx, mut sc) = (0.0, 0.0);
    for (o, n) in &m {
        let (ux, uz) = (n.point[0] - cn[0], n.point[2] - cn[2]);
        let (vx, vz) = (o.point[0] - co[0], o.point[2] - co[2]);
        sc += vx * ux + vz * uz;
        sx += vx * uz - vz * ux;
    }
    let yaw = sx.atan2(sc);
    let (s, c) = yaw.sin_cos();
    let rot = [c, 0.0, -s, 0.0, 0.0, 1.0, 0.0, 0.0, s, 0.0, c, 0.0, 0.0, 0.0, 0.0, 1.0];
    let rc = apply_dir(&rot, cn);
    let mut transform = rot;
    transform[12] = co[0] - rc[0];
    transform[13] = co[1] - rc[1];
    transform[14] = co[2] - rc[2];
    let misfits: Vec<(u32, f64)> = m
        .iter()
        .map(|(o, n)| {
            let p = apply_point(&transform, n.point);
            (n.id, ((p[0] - o.point[0]).powi(2) + (p[1] - o.point[1]).powi(2) + (p[2] - o.point[2]).powi(2)).sqrt())
        })
        .collect();
    let worst_m = misfits.iter().map(|x| x.1).fold(0.0, f64::max);
    Some(CardFit { yaw_rad: yaw, transform, misfits, worst_m })
}

/// Each card seen in both, with how far apart they are as they stand (no fit): the same-session
/// check that nothing moved.
pub fn card_offsets(old: &[CardSeen], new: &[CardSeen]) -> Vec<(u32, f64)> {
    matched(old, new)
        .iter()
        .map(|(o, n)| (n.id, ((n.point[0] - o.point[0]).powi(2) + (n.point[1] - o.point[1]).powi(2) + (n.point[2] - o.point[2]).powi(2)).sqrt()))
        .collect()
}
```

Re-export `apply_point, apply_dir, mul, CardFit, cards_fit, card_offsets` from `lib.rs`.

- [ ] **Step 4: Run tests and checks**

Run: `cargo test -p cave-capture && cargo clippy -p cave-capture --all-targets -- -D warnings && cargo fmt -p cave-capture`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
python3 tools/make_code_index.py
git add crates/cave-capture docs/CODE-INDEX.md
git commit -m "cave-capture: cards_fit and card_offsets (line a pass up from the station cards)"
```

---

### Task 4: FFI doors and Swift wrappers

**Goal:** The app can call `add_pass`, `ghost` and `cards_fit`/`card_offsets` through `capture_core.h` and `Core.swift`.

**Files:**
- Modify: `apps/capture-ios/rust/src/lib.rs`, `apps/capture-ios/rust/include/capture_core.h`, `apps/capture-ios/Sources/Core.swift`
- Test: `apps/capture-ios/rust/src/lib.rs` (`mod tests`), `apps/capture-ios/Tests/ShotRecorderTests.swift`

**Acceptance Criteria:**
- [ ] `cc_shot_add_pass(old, len, pass_json, positions, n, normals, n, indices, n, jpegs, n)` returns a bundle whose header has 2 passes; empty buffer on a bad old bundle or bad JSON.
- [ ] `cc_shot_ghost(bundle, len, cell_m, min_outline_m)` returns little-endian binary: `u32 vertex_floats, u32 index_count, u32 outline_count`, then the floats, the indices, then per outline `u32 point_count` and its xyz floats; empty on a bad bundle.
- [ ] `cc_cards_fit(bundle, len, cards_json, same_session)` returns JSON `{"offsets":[{"id","m"}]}` when `same_session` is 1, else `{"fit":{"transform":[16],"yaw_rad","worst_m","misfits":[{"id","m"}]}}` or `{"fit":null}`.
- [ ] Swift: `Core.addPass`, `Core.ghost`, `Core.cardsFit` round-trip in a simulator test.
- [ ] `cc_shot_summary` JSON gains `"passes"`: the header's pass count (0 for an old one-pass file).

**Verify:** `cd apps/capture-ios/rust && cargo test` → `test result: ok`; app test command → `** TEST SUCCEEDED **`

**Steps:**

- [ ] **Step 1: Rust test first** (in `mod tests` of `apps/capture-ios/rust/src/lib.rs`; reuse the file's `sample_bundle()` and `read()` helpers):

```rust
    #[test]
    fn add_pass_ghost_and_fit_through_the_doors() {
        let b = sample_bundle();
        let pass = CString::new(
            r#"{"captured_at_ms":5,"aligned_by":"same_session","transform":[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1],
               "replace":true,"tracking":"normal","cards":[],
               "keyframes":[{"pose":[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1],"intrinsics":[1,0,0,0,1,0,0,0,1],"width":1,"height":1,"jpeg_len":3}]}"#,
        ).unwrap();
        let pos = [0f32, 0.0, -1.0, 1.0, 0.0, -1.0, 0.0, 1.0, -1.0];
        let nor = [0f32; 9];
        let idx = [0u32, 1, 2];
        let jpg = [7u8, 7, 7];
        let out = read(unsafe { cc_shot_add_pass(b.as_ptr(), b.len(), pass.as_ptr(), pos.as_ptr(), 9, nor.as_ptr(), 9, idx.as_ptr(), 3, jpg.as_ptr(), 3) });
        let shot = decode(&out).unwrap();
        assert_eq!(shot.header.passes.len(), 2);
        assert_eq!(shot.keyframes.last().unwrap().jpeg, vec![7, 7, 7]);
        assert!(read(unsafe { cc_shot_add_pass(b.as_ptr(), 3, pass.as_ptr(), pos.as_ptr(), 9, nor.as_ptr(), 9, idx.as_ptr(), 3, jpg.as_ptr(), 3) }).is_empty());

        let g = read(unsafe { cc_shot_ghost(out.as_ptr(), out.len(), 0.05, 0.0) });
        let word = |i: usize| u32::from_le_bytes(g[i * 4..i * 4 + 4].try_into().unwrap());
        assert_eq!((word(0), word(1)), (9, 3));
        assert_eq!(word(2), 1); // the lone triangle's edge is one closed outline

        let cards = CString::new(r#"[{"id":2,"word":"CURRENT","point":[0,0,0],"corners":[],"sightings":9,"spread_m":0.0}]"#).unwrap();
        let j: serde_json::Value = serde_json::from_slice(&read(unsafe { cc_cards_fit(out.as_ptr(), out.len(), cards.as_ptr(), 0) })).unwrap();
        assert!(j["fit"].is_null());
    }
```

(`sample_bundle()` must have no cards, so the fit is null; if it has cards, assert on `offsets` instead with `same_session` 1.)

- [ ] **Step 2: Run to see it fail**

Run: `cd apps/capture-ios/rust && cargo test add_pass_ghost_and_fit`
Expected: FAIL to compile.

- [ ] **Step 3: Implement the doors** in `apps/capture-ios/rust/src/lib.rs` (import `add_pass, ghost, cards_fit, card_offsets, NewPass, PassMesh, Keyframe, CardSeen` from `cave_capture`):

```rust
#[derive(Deserialize)]
struct PassIn {
    captured_at_ms: i64,
    aligned_by: String,
    transform: Vec<f64>,
    replace: bool,
    tracking: String,
    #[serde(default)]
    cards: Vec<CardSeen>,
    keyframes: Vec<KeyIn>,
}

/// See `capture_core.h`.
///
/// # Safety
/// Every pointer must be null or valid for its stated length (strings NUL-terminated).
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn cc_shot_add_pass(
    bundle: *const u8, len: usize, pass_json: *const c_char,
    positions: *const f32, n_positions: usize, normals: *const f32, n_normals: usize,
    indices: *const u32, n_indices: usize, jpegs: *const u8, n_jpegs: usize,
) -> CcBuf {
    guard(
        || {
            if (positions.is_null() && n_positions > 0) || (normals.is_null() && n_normals > 0)
                || (indices.is_null() && n_indices > 0) || (jpegs.is_null() && n_jpegs > 0) {
                return empty();
            }
            let Ok(old) = decode(unsafe { slice(bundle, len) }) else { return empty() };
            let Ok(Some(t)) = (unsafe { text(pass_json) }) else { return empty() };
            let Ok(p) = serde_json::from_str::<PassIn>(t) else { return empty() };
            let Ok(transform) = <[f64; 16]>::try_from(p.transform.as_slice()) else { return empty() };
            let jpeg_bytes = unsafe { slice(jpegs, n_jpegs) };
            let mut at = 0usize;
            let mut keyframes = Vec::with_capacity(p.keyframes.len());
            for k in &p.keyframes {
                let (Ok(pose), Ok(intrinsics)) = (<[f32; 16]>::try_from(k.pose.as_slice()), <[f32; 9]>::try_from(k.intrinsics.as_slice())) else { return empty() };
                let Some(end) = at.checked_add(k.jpeg_len) else { return empty() };
                let Some(jpeg) = jpeg_bytes.get(at..end) else { return empty() };
                at = end;
                keyframes.push(Keyframe { pose, intrinsics, width: k.width, height: k.height, jpeg: jpeg.to_vec() });
            }
            let new = NewPass {
                captured_at_ms: p.captured_at_ms, aligned_by: p.aligned_by, transform,
                mesh: if p.replace { PassMesh::Replace } else { PassMesh::Add },
                positions: unsafe { slice(positions, n_positions) }.to_vec(),
                normals: unsafe { slice(normals, n_normals) }.to_vec(),
                indices: unsafe { slice(indices, n_indices) }.to_vec(),
                keyframes, cards: p.cards, tracking: p.tracking,
            };
            let Ok(shot) = add_pass(&old, new) else { return empty() };
            match encode(&shot) {
                Ok(b) if check_header(&b).is_ok() => give(b),
                _ => empty(),
            }
        },
        empty,
    )
}

/// See `capture_core.h`.
///
/// # Safety
/// `bundle` must be null or valid for `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cc_shot_ghost(bundle: *const u8, len: usize, cell_m: f32, min_outline_m: f32) -> CcBuf {
    guard(
        || {
            let Ok(shot) = decode(unsafe { slice(bundle, len) }) else { return empty() };
            let g = ghost(&shot.positions, &shot.indices, cell_m, min_outline_m);
            let mut out = Vec::new();
            for w in [g.positions.len() as u32, g.indices.len() as u32, g.outlines.len() as u32] {
                out.extend_from_slice(&w.to_le_bytes());
            }
            for f in &g.positions { out.extend_from_slice(&f.to_le_bytes()); }
            for i in &g.indices { out.extend_from_slice(&i.to_le_bytes()); }
            for o in &g.outlines {
                out.extend_from_slice(&(o.len() as u32).to_le_bytes());
                for p in o { for f in p { out.extend_from_slice(&f.to_le_bytes()); } }
            }
            give(out)
        },
        empty,
    )
}

/// See `capture_core.h`.
///
/// # Safety
/// `bundle` valid for `len` bytes; `cards_json` NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cc_cards_fit(bundle: *const u8, len: usize, cards_json: *const c_char, same_session: u8) -> CcBuf {
    guard(
        || {
            let Ok(shot) = decode(unsafe { slice(bundle, len) }) else { return err("unreadable shot") };
            let Ok(Some(t)) = (unsafe { text(cards_json) }) else { return err("cards are not text") };
            let Ok(new) = serde_json::from_str::<Vec<CardSeen>>(t) else { return err("cards are not JSON") };
            let pairs = |v: &[(u32, f64)]| v.iter().map(|(id, m)| serde_json::json!({"id": id, "m": m})).collect::<Vec<_>>();
            if same_session != 0 {
                return give_json(serde_json::json!({"offsets": pairs(&card_offsets(&shot.header.cards, &new))}));
            }
            give_json(serde_json::json!({"fit": cards_fit(&shot.header.cards, &new).map(|f| serde_json::json!({
                "transform": f.transform, "yaw_rad": f.yaw_rad, "worst_m": f.worst_m, "misfits": pairs(&f.misfits)
            }))}))
        },
        internal,
    )
}
```

Also in the existing `fn summary(...)`, right after the header is read, add
`out["passes"] = shot.header.passes.len().into();` (and in the test above assert
`summary(&out)["passes"] == 2` through `cc_shot_summary(out.as_ptr(), out.len(), std::ptr::null())`).
Document `"passes"` in the `cc_shot_summary` comment in `capture_core.h`.

Note `decode` already reads the whole bundle; `cc_shot_ghost` decoding the photos too is acceptable (once per pass, background).

- [ ] **Step 4: Header** — append to `apps/capture-ios/rust/include/capture_core.h` before `cc_free`:

```c
/// Adds a scanning pass to a .cvcap (bundle, len BYTES) and returns the new bundle, or empty on any
/// error. pass_json: {"captured_at_ms","aligned_by":"same_session"|"cards","transform":[16 column-major,
/// pass frame to shot frame],"replace":bool (true: the pass mesh replaces the shot's, same session;
/// false: added),"tracking","cards":[CardSeen],"keyframes":[as cc_shot_encode]}. Mesh and jpegs as
/// cc_shot_encode (counts are elements; jpegs bytes back to back).
CcBuf cc_shot_add_pass(const uint8_t *bundle, size_t len, const char *pass_json,
                       const float *positions, size_t n_positions,
                       const float *normals, size_t n_normals,
                       const uint32_t *indices, size_t n_indices,
                       const uint8_t *jpegs, size_t n_jpegs);

/// The shot's mesh simplified on a cell_m grid, with its hole outlines (shorter than min_outline_m
/// dropped). Little-endian binary: u32 vertex_floats, u32 index_count, u32 outline_count; then
/// vertex_floats f32 (xyz per vertex, capture frame); index_count u32; then per outline u32
/// point_count and point_count*3 f32. Empty on a bad bundle.
CcBuf cc_shot_ghost(const uint8_t *bundle, size_t len, float cell_m, float min_outline_m);

/// Station cards of a new pass (cards_json: [CardSeen], the pass's capture frame) against the shot's.
/// same_session != 0: {"offsets":[{"id","m"}]} (plain distances). Else {"fit":{"transform":[16],
/// "yaw_rad","worst_m","misfits":[{"id","m"}]}} or {"fit":null} with fewer than 2 matched cards.
/// {"error"} on a bad bundle or JSON.
CcBuf cc_cards_fit(const uint8_t *bundle, size_t len, const char *cards_json, uint8_t same_session);
```

- [ ] **Step 5: Swift wrappers** — append inside `enum Core` in `apps/capture-ios/Sources/Core.swift`:

```swift
    /// A shot's ghost: simplified mesh and hole outlines, capture frame.
    struct GhostMesh: Sendable { var positions: [SIMD3<Float>]; var indices: [UInt32]; var outlines: [[SIMD3<Float>]] }

    static func addPass(bundle: Data, pass: Data, positions: [Float], normals: [Float], indices: [UInt32], jpegs: Data) -> Data {
        guard let pj = String(data: pass, encoding: .utf8) else { return Data() }
        return bundle.withUnsafeBytes { b in pj.withCString { pp in
            positions.withUnsafeBufferPointer { p in normals.withUnsafeBufferPointer { n in
                indices.withUnsafeBufferPointer { i in jpegs.withUnsafeBytes { j in
                    take(cc_shot_add_pass(b.bindMemory(to: UInt8.self).baseAddress, b.count, pp,
                                          p.baseAddress, p.count, n.baseAddress, n.count, i.baseAddress, i.count,
                                          j.bindMemory(to: UInt8.self).baseAddress, j.count))
                } }
            } }
        } }
    }

    static func ghost(bundle: Data, cellM: Float = 0.05, minOutlineM: Float = 0.15) -> GhostMesh? {
        let d = bundle.withUnsafeBytes { b in take(cc_shot_ghost(b.bindMemory(to: UInt8.self).baseAddress, b.count, cellM, minOutlineM)) }
        guard d.count >= 12 else { return nil }
        var at = 0
        func u32() -> Int? { guard at + 4 <= d.count else { return nil }; defer { at += 4 }; return Int(d.subdata(in: at..<at + 4).withUnsafeBytes { $0.loadUnaligned(as: UInt32.self) }.littleEndian) }
        func f32() -> Float? { guard at + 4 <= d.count else { return nil }; defer { at += 4 }; return Float(bitPattern: d.subdata(in: at..<at + 4).withUnsafeBytes { $0.loadUnaligned(as: UInt32.self) }.littleEndian) }
        guard let nf = u32(), let ni = u32(), let no = u32() else { return nil }
        var g = GhostMesh(positions: [], indices: [], outlines: [])
        for _ in 0..<(nf / 3) { guard let x = f32(), let y = f32(), let z = f32() else { return nil }; g.positions.append(SIMD3(x, y, z)) }
        for _ in 0..<ni { guard let i = u32() else { return nil }; g.indices.append(UInt32(i)) }
        for _ in 0..<no {
            guard let n = u32() else { return nil }
            var line: [SIMD3<Float>] = []
            for _ in 0..<n { guard let x = f32(), let y = f32(), let z = f32() else { return nil }; line.append(SIMD3(x, y, z)) }
            g.outlines.append(line)
        }
        return g
    }

    /// The core's card comparison (`cc_cards_fit`) as parsed JSON.
    static func cardsFit(bundle: Data, cards: [CardSeenRecord], sameSession: Bool) -> [String: Any] {
        guard let j = try? JSONSerialization.data(withJSONObject: cards.map(\.json)), let t = String(data: j, encoding: .utf8) else { return ["error": "cards"] }
        let d = bundle.withUnsafeBytes { b in t.withCString { c in
            take(cc_cards_fit(b.bindMemory(to: UInt8.self).baseAddress, b.count, c, sameSession ? 1 : 0))
        } }
        return (try? JSONSerialization.jsonObject(with: d) as? [String: Any]) ?? ["error": "unreadable"]
    }
```

(Ghost parse uses `Data.subdata` per word; it runs once per pass on a background task. If profiling shows it slow, read with `withUnsafeBytes` in one pass.)

- [ ] **Step 6: Swift test** — append to `apps/capture-ios/Tests/ShotRecorderTests.swift` (inside the test class; `ShotRecorderTests.tri` and `ShotJob` already exist there):

```swift
    func testPassGhostAndFitThroughTheCore() throws {
        let cave = "test-cave-pass"
        defer { try? FileManager.default.removeItem(at: ShotFiles.folder(cave: cave)) }
        let over = Station(id: "a", label: "A1", x: 0, y: 0, z: 0)
        let job = ShotJob(caveId: cave, over: over, parts: [Self.tri], keyframes: [], startCamera: .zero, tracking: "normal",
                          systemVersion: "27", compass: CompassReading(trueNorth: true, accuracyDeg: 5, headingDeg: 0))
        guard case .success(let url) = job.write() else { return XCTFail("not written") }
        let old = try Data(contentsOf: url)
        let pass = try JSONSerialization.data(withJSONObject: [
            "captured_at_ms": 5, "aligned_by": "same_session", "transform": [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1],
            "replace": true, "tracking": "normal", "cards": [Any](), "keyframes": [Any](),
        ] as [String: Any])
        let out = Core.addPass(bundle: old, pass: pass, positions: [0, 0, -2, 1, 0, -2, 0, 1, -2], normals: [Float](repeating: 0, count: 9),
                               indices: [0, 1, 2], jpegs: Data())
        XCTAssertFalse(out.isEmpty)
        let g = try XCTUnwrap(Core.ghost(bundle: out, minOutlineM: 0))
        XCTAssertEqual(g.indices.count, 3)
        XCTAssertEqual(g.outlines.count, 1)
        XCTAssertNotNil(Core.cardsFit(bundle: out, cards: [], sameSession: true)["offsets"])
    }
```

- [ ] **Step 7: Build and run everything**

Run: `cd apps/capture-ios/rust && cargo test && cargo clippy --all-targets -- -D warnings && cd .. && ./rust/build-xcframework.sh && xcodegen generate && xcodebuild test -scheme CaveCADCapture -destination 'platform=iOS Simulator,name=iPhone 18 Pro' CODE_SIGNING_ALLOWED=NO 2>&1 | grep -E "error:|Executed|TEST (SUCCEEDED|FAILED)"`
Expected: Rust `test result: ok`; `** TEST SUCCEEDED **`.

- [ ] **Step 8: Commit**

```bash
git add apps/capture-ios/rust apps/capture-ios/Sources/Core.swift apps/capture-ios/Tests/ShotRecorderTests.swift apps/capture-ios/CaptureCore.xcframework
git commit -m "capture-ios: core doors for add_pass, ghost and cards_fit"
```

---

### Task 5: Settled cards and detection backoff

**Goal:** A card is settled at 8+ kept sightings with spread under 1 cm; when every seen card is settled, detection runs every 2 s at `quad_decimate` 2 on 1 thread, and mesh passes skip settled cards; the log shows state changes and ARKit's frame rate.

**Files:**
- Modify: `apps/capture-ios/Sources/CardTracking.swift` (`CardTrack.settled`, `CardTracker.allSettled`, `meshBatch(limit:skipSettled:)`)
- Modify: `apps/capture-ios/Sources/CardDetector.swift` (`detect(_:decimate:threads:done:)`)
- Modify: `apps/capture-ios/Sources/ShotRecorder.swift` (`detectCards`, `meshCards`, `logCards`, frame counter)
- Test: `apps/capture-ios/Tests/CardTrackingTests.swift`

**Acceptance Criteria:**
- [ ] `settled` false at 7 kept sightings, true at 8 with spread under 0.01 m, false at 8 with spread 0.02 m.
- [ ] `allSettled` false with no cards seen.
- [ ] `meshBatch(limit:skipSettled: true)` returns no sighting of a settled card.
- [ ] Log lines `cards: settled, backing off` and `cards: searching at full rate` appear on state change; each 5 s summary includes `ARKit %.0f fps`.

**Verify:** app test command → `** TEST SUCCEEDED **`

**Steps:**

- [ ] **Step 1: Failing test** (append to `CardTrackingTests`; uses the existing `sighting(...)` helper):

```swift
    func testSettledAfterEightTightLooks() {
        var t = CardTracker()
        for _ in 0..<7 { t.add(sighting(size: 2)) }
        XCTAssertFalse(t.tracks[.current]!.settled)
        XCTAssertFalse(t.allSettled)
        t.add(sighting(size: 2))
        XCTAssertTrue(t.tracks[.current]!.settled)
        XCTAssertTrue(t.allSettled)
        XCTAssertTrue(t.meshBatch(limit: 10, skipSettled: true).isEmpty)
        XCTAssertEqual(t.meshBatch(limit: 10).count, 8)
        var loose = CardTracker()
        for i in 0..<8 { loose.add(sighting(size: 2 + (i % 2 == 0 ? 0.02 : -0.02))) }
        XCTAssertFalse(loose.tracks[.current]!.settled)
        XCTAssertFalse(CardTracker().allSettled)
    }
```

- [ ] **Step 2: Run, expect compile failure** (app test command).

- [ ] **Step 3: Implement** in `CardTracking.swift`:

```swift
// in CardTrack
    /// Enough close agreement that more looks will not move it: 8 kept sightings within 1 cm (typical).
    var settled: Bool { estimate.map { $0.kept >= 8 && $0.spread < 0.01 } ?? false }

// in CardTracker
    /// Every card seen so far is settled (false before any is seen).
    var allSettled: Bool { let s = seen; return !s.isEmpty && s.allSatisfy { $0.track.settled } }
```

Change `meshBatch` to:

```swift
    func meshBatch(limit: Int, skipSettled: Bool = false) -> [CardSighting] {
        let pool = skipSettled ? tracks.values.filter { !$0.settled }.flatMap(\.sightings) : allSightings
        return Array(pool.sorted { ($0.meshedPass ?? -1, $0.seq) < ($1.meshedPass ?? -1, $1.seq) }.prefix(limit))
    }
```

In `CardDetector.swift`, give `detect` two parameters and set them on the queue before running:

```swift
    func detect(_ frame: CardFrame, decimate: Float = 1, threads: Int32 = 2, done: @escaping @Sendable (CardResult) -> Void) {
        queue.async { [self] in
            detector.pointee.quad_decimate = decimate
            detector.pointee.nthreads = threads
            // ... existing body unchanged ...
```

In `ShotRecorder.swift`:
- add `private var backedOff = false`, `private var frameCount = 0, lastFpsTime: TimeInterval = 0`, reset them in `start()`.
- in `session(_:didUpdate frame:)` inside the capturing branch: `frameCount += 1`.
- in `detectCards`, replace the rate guard with:

```swift
        let calm = cardTracker.allSettled
        if calm != backedOff {
            backedOff = calm
            Log.write("cards", calm ? "settled, backing off" : "searching at full rate")
        }
        guard !detectingCards, time - lastCardTime >= (calm ? 2.0 : Self.cardEvery) else { return }
```

  and call `cardDetector.detect(frame, decimate: calm ? 2 : 1, threads: calm ? 1 : 2) { ... }`.
- in `meshCards`: `cardTracker.meshBatch(limit: Self.meshBatchLimit, skipSettled: true)`.
- in `logCards`, first line:

```swift
        if let t = session.currentFrame?.timestamp, t > lastFpsTime, lastFpsTime > 0 {
            Log.write("cards", String(format: "%@: ARKit %.0f fps", when, Double(frameCount) / (t - lastFpsTime)))
        }
        lastFpsTime = session.currentFrame?.timestamp ?? 0; frameCount = 0
```

- [ ] **Step 4: Run app tests** → `** TEST SUCCEEDED **`

- [ ] **Step 5: Commit**

```bash
git add apps/capture-ios/Sources apps/capture-ios/Tests
git commit -m "capture-ios cards: settled cards back detection off (2 s, half resolution, 1 thread); log fps"
```

---

### Task 6: Kept session, parent shot, Scan more, saving a pass

**Goal:** Opening Capture over a station with a shot makes the next Start a pass added to that file; Stop saves without ending the ARKit session; "Scan more" starts another pass; a pass is saved through `Core.addPass` and an atomic replace.

**Files:**
- Modify: `apps/capture-ios/Sources/ShotFiles.swift` (`parent(cave:station:)`, `replace(_:with:)`)
- Modify: `apps/capture-ios/Sources/ShotRecorder.swift` (parent state, `prepare`, `stop`, `scanMore`, `leave`, `ShotJob.writePass`)
- Modify: `apps/capture-ios/Sources/CaptureView.swift` (title line, Start/Scan more labels)
- Test: `apps/capture-ios/Tests/ShotRecorderTests.swift`

**Acceptance Criteria:**
- [ ] `ShotFiles.parent(cave:station:)` returns the newest shot whose header `station_id` matches, nil when none.
- [ ] `ShotFiles.replace(url, with:)` leaves the old bytes when the write fails (test: replace into a path whose folder was removed returns false and the original file is unchanged).
- [ ] `ShotJob.writePass(into:sameSession:transform:)` rewrites the parent with 2 passes and both passes' photos.
- [ ] Stop no longer calls `session.pause()`; `leave()` still does.
- [ ] Capture screen shows "Adding to the scan from <time>" and the button reads "Scan more" when a parent exists.

**Verify:** app test command → `** TEST SUCCEEDED **`

**Steps:**

- [ ] **Step 1: Failing tests** (append to `ShotRecorderTests`):

```swift
    func testParentShotAndSafeReplace() throws {
        let cave = "test-cave-parent"
        defer { try? FileManager.default.removeItem(at: ShotFiles.folder(cave: cave)) }
        let a = Station(id: "a", label: "A1", x: 0, y: 0, z: 0), b = Station(id: "b", label: "B1", x: 0, y: 0, z: 0)
        func write(_ s: Station) -> URL {
            let job = ShotJob(caveId: cave, over: s, parts: [Self.tri], keyframes: [], startCamera: .zero, tracking: "normal",
                              systemVersion: "27", compass: CompassReading(trueNorth: true, accuracyDeg: 5, headingDeg: 0))
            guard case .success(let u) = job.write() else { XCTFail("not written"); return URL(fileURLWithPath: "/") }
            return u
        }
        XCTAssertNil(ShotFiles.parent(cave: cave, station: "a"))
        let ua = write(a)
        _ = write(b)
        XCTAssertEqual(ShotFiles.parent(cave: cave, station: "a"), ua)
        let before = try Data(contentsOf: ua)
        XCTAssertTrue(ShotFiles.replace(ua, with: Data([1, 2, 3])))
        XCTAssertEqual(try Data(contentsOf: ua), Data([1, 2, 3]))
        try before.write(to: ua)
        let gone = ShotFiles.folder(cave: "test-cave-missing").appendingPathComponent("x.cvcap")
        try? FileManager.default.removeItem(at: ShotFiles.folder(cave: "test-cave-missing"))
        XCTAssertFalse(ShotFiles.replace(gone, with: Data([9])))
        XCTAssertEqual(try Data(contentsOf: ua), before)

        // a same-session pass rewrites the parent with two passes
        let kf = ShotKeyframe(pose: matrix_identity_float4x4, intrinsics: matrix_identity_float3x3, w: 1, h: 1, jpeg: Data([5, 5]))
        let pass = ShotJob(caveId: cave, over: a, parts: [Self.tri], keyframes: [kf], startCamera: .zero, tracking: "normal",
                           systemVersion: "27", compass: CompassReading(trueNorth: true, accuracyDeg: 5, headingDeg: 0))
        guard case .success(let same) = pass.writePass(into: ua, sameSession: true, transform: nil) else { return XCTFail("pass not written") }
        XCTAssertEqual(same, ua)
        let h = try Self.header(of: try Data(contentsOf: ua))
        XCTAssertEqual((h["passes"] as? [[String: Any]])?.count, 2)
        XCTAssertEqual((h["keyframes"] as? [[String: Any]])?.count, 1)
    }
```

- [ ] **Step 2: Run, expect compile failure.**

- [ ] **Step 3: `ShotFiles`** — add:

```swift
    /// The newest shot of the cave taken over `station` (header only is read), or nil.
    static func parent(cave: String, station: String) -> URL? {
        list(cave: cave).first { u in
            guard let d = try? Data(contentsOf: u, options: .alwaysMapped) else { return false }
            return Core.summary(bundle: d, pair: nil)["station_id"] as? String == station
        }
    }

    /// Writes `data` beside `url`, then swaps it in; the old file stays if anything fails.
    @discardableResult
    static func replace(_ url: URL, with data: Data) -> Bool {
        let tmp = url.deletingLastPathComponent().appendingPathComponent(".\(UUID().uuidString).tmp")
        do {
            try data.write(to: tmp, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
            _ = try FileManager.default.replaceItemAt(url, withItemAt: tmp)
            return true
        } catch {
            try? FileManager.default.removeItem(at: tmp)
            return false
        }
    }
```

- [ ] **Step 4: `ShotJob.writePass`** — add to `ShotJob` (reuse its keyframe-joining code; extract the existing photo-join loop in `write()` into `private func joinedPhotos() -> (kfs: [[String: Any]], jpegs: Data, numbers: [Double])` and call it from both):

```swift
    /// Adds this job as a pass to the shot at `parent`. Same session: the mesh replaces the shot's
    /// (ARKit's whole current mesh) and the transform is identity. Otherwise `transform` (column-major,
    /// this pass's frame to the shot's, from the cards) moves it and the mesh is added.
    func writePass(into parent: URL, sameSession: Bool, transform: [Double]?) -> Result<URL, ShotFailure> {
        guard let old = try? Data(contentsOf: parent) else { return .failure(ShotFailure(message: "The earlier scan could not be read.")) }
        if !cards.allSightings.isEmpty {
            cards.applyMesh(CardTracker.meshRanges(cards.allSightings, MeshSnapshot(parts, near: cards.points)))
        }
        var mesh = ShotRecorder.mergeAnchors(parts)
        parts = []
        let (kfs, jpegs, numbers) = joinedPhotos()
        let identity: [Double] = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]
        let pass: [String: Any] = [
            "captured_at_ms": Int64(Date().timeIntervalSince1970 * 1000),
            "aligned_by": sameSession ? "same_session" : "cards",
            "transform": sameSession ? identity : (transform ?? identity),
            "replace": sameSession, "tracking": tracking, "cards": cards.records.map(\.json), "keyframes": kfs,
        ]
        guard numbers.allSatisfy(\.isFinite), let pj = try? JSONSerialization.data(withJSONObject: pass) else { return .failure(Self.unwritable) }
        let bundle = Core.addPass(bundle: old, pass: pj, positions: mesh.positions, normals: mesh.normals, indices: mesh.indices, jpegs: jpegs)
        mesh = MergedMesh(positions: [], normals: [], indices: [])
        guard !bundle.isEmpty else { return .failure(Self.unwritable) }
        return ShotFiles.replace(parent, with: bundle) ? .success(parent) : .failure(ShotFailure(message: "Saving the pass failed; the earlier scan is kept."))
    }
```

- [ ] **Step 5: Recorder** — in `ShotRecorder`:

```swift
    /// The station's shot this capture adds to (nil: the next Stop makes a new shot).
    @Published private(set) var parent: URL?
    /// When the parent was made, for the screen.
    @Published private(set) var parentMade: Date?
    /// The parent's earlier passes were scanned in this ARKit session (same frame).
    private(set) var parentInSession = false

    /// Called when the capture screen opens over a station.
    func prepare(cave: KeptCave, over: Station) {
        parent = ShotFiles.parent(cave: cave.id, station: over.id)
        parentMade = parent.flatMap { try? $0.resourceValues(forKeys: [.creationDateKey]).creationDate }
        parentInSession = false
        if parent != nil { Log.write("shot", "adding to the station's shot (another session)") }
    }
```

In `stop(...)`: delete `session.pause()` (keep `isIdleTimerDisabled = false`). Build the job as now, then:

```swift
        let into = parent, same = parentInSession, transform = cardTransform  // cardTransform: Task 7; until then nil
        Task {
            let result = await Task.detached { into.map { job.writePass(into: $0, sameSession: same, transform: transform) } ?? job.write() }.value
            switch result {
            case .success(let url):
                parent = url; parentInSession = true
                let n = (try? Data(contentsOf: url, options: .alwaysMapped)).map { Core.summary(bundle: $0, pair: nil)["passes"] as? Int ?? 0 } ?? 0
                Log.write("shot", "saved pass \(max(n, 1))")
                parentMade = parentMade ?? Date()
                phase = .saved(url); message = "Shot saved. Scan more to fill gaps."
            case .failure(let m): phase = .failed(m.message); message = m.message
            }
        }
```

Add `func scanMore()` that sets `phase = .ready`, `message = Self.readyMessage` and does NOT call `run()` (the session keeps running); keep `reset()` for the failure path. `leave()` keeps `session.pause()`.

Guard against a lost/cards pass being saved as same-session: if `parentInSession` is false and `transform` is nil when Stop is pressed on a parent, save the pass with `aligned_by` `cards` and identity only after Task 7; in this task, refuse with `message = "Look at two cards so this pass can be lined up."` and keep capturing.

- [ ] **Step 6: Screen** — in `CaptureView`:
- `.onAppear { rec.prepare(cave: cave, over: over) }` (before `rec.run()` if it is there).
- Under the "Over \(over.label)" capsule: `if let d = rec.parentMade { Text("Adding to the scan from \(d.formatted(date: .omitted, time: .shortened))").font(.caption).padding(6).background(.thinMaterial, in: .capsule) }`.
- Ready button label: `rec.parent == nil ? "Start" : "Scan more"`.
- Saved state: replace "New shot" with `Button("Scan more") { rec.scanMore() }`.

- [ ] **Step 7: Run app tests** → `** TEST SUCCEEDED **`

- [ ] **Step 8: Commit**

```bash
git add apps/capture-ios/Sources apps/capture-ios/Tests
git commit -m "capture-ios: scan more at a station; passes added into the station's shot, session kept between passes"
```

---

### Task 7: Ghost in the live view, card line-up and the check

**Goal:** During a pass over a parent, the parent's ghost (grey see-through mesh, red outline ribbons) shows in the AR view, at identity in the same session or once cards line it up; cards more than 5 cm off warn; a misfit over 5 cm refuses the line-up.

**Files:**
- Create: `apps/capture-ios/Sources/GhostView.swift`
- Modify: `apps/capture-ios/Sources/ShotRecorder.swift` (ghost load, `cardTransform`, check), `apps/capture-ios/Sources/CaptureView.swift` (pass ghost to `ARMeshView`)
- Test: `apps/capture-ios/Tests/ShotRecorderTests.swift`

**Acceptance Criteria:**
- [ ] `GhostView.ribbon(outlines:width:)` makes 4 triangles per outline segment (a vertical and a flat strip) and no NaN.
- [ ] `ShotRecorder.lineUp(fit:)` accepts a fit with `worst_m` 0.03 (returns the transform) and refuses 0.12 (returns nil and a message naming the worst card).
- [ ] In the same session, a settled card more than 0.05 m from the parent's card sets the warning "<WORD> is N cm from where it was: card moved, or tracking slipped?" once per card per pass.
- [ ] Log lines: `ghost: N triangles, M outlines, X ms`; `lined up by cards: yaw D deg, worst W mm` or `card line-up refused`.

**Verify:** app test command → `** TEST SUCCEEDED **`; then on the phone, Task 8.

**Steps:**

- [ ] **Step 1: Failing tests** (append to `ShotRecorderTests`):

```swift
    func testOutlineRibbonsAndLineUpRule() {
        let r = GhostView.ribbon(outlines: [[SIMD3(0, 0, 0), SIMD3(1, 0, 0), SIMD3(1, 0, 1)]], width: 0.02)
        XCTAssertEqual(r.indices.count, 2 * 4 * 3)
        XCTAssertTrue(r.positions.allSatisfy { $0.x.isFinite && $0.y.isFinite && $0.z.isFinite })
        let ok: [String: Any] = ["transform": [Double](repeating: 0, count: 16), "yaw_rad": 0.1, "worst_m": 0.03, "misfits": [["id": 2, "m": 0.03]]]
        XCTAssertNotNil(ShotRecorder.lineUp(fit: ok).transform)
        let bad: [String: Any] = ["transform": [Double](repeating: 0, count: 16), "yaw_rad": 0.1, "worst_m": 0.12, "misfits": [["id": 2, "m": 0.01], ["id": 12, "m": 0.12]]]
        let r2 = ShotRecorder.lineUp(fit: bad)
        XCTAssertNil(r2.transform)
        XCTAssertTrue(r2.message.contains("FORWARD"), r2.message)
        XCTAssertTrue(r2.message.contains("12 cm"), r2.message)
    }
```

- [ ] **Step 2: Run, expect compile failure.**

- [ ] **Step 3: `GhostView.swift`**:

```swift
import RealityKit
import UIKit

/// The earlier scan drawn over the live view: a see-through grey shell and red hole outlines.
enum GhostView {
    struct Ribbon { var positions: [SIMD3<Float>]; var indices: [UInt32] }

    /// Each outline segment as two thin strips (one upright, one flat), so it shows from any side.
    static func ribbon(outlines: [[SIMD3<Float>]], width: Float) -> Ribbon {
        var r = Ribbon(positions: [], indices: [])
        let up = SIMD3<Float>(0, 1, 0)
        for line in outlines where line.count >= 2 {
            for (a, b) in zip(line, line.dropFirst()) {
                let d = b - a
                guard simd_length(d) > 1e-6 else { continue }
                var side = simd_cross(d, up)
                side = simd_length(side) > 1e-6 ? simd_normalize(side) * width / 2 : SIMD3(width / 2, 0, 0)
                let lift = up * width / 2
                for off in [lift, side] {
                    let base = UInt32(r.positions.count)
                    r.positions += [a - off, a + off, b + off, b - off]
                    r.indices += [base, base + 1, base + 2, base, base + 2, base + 3]
                }
            }
        }
        return r
    }

    /// One anchor holding the shell and the outlines, at `transform` (capture frame of this session).
    @MainActor static func entity(_ g: Core.GhostMesh, transform: simd_float4x4) -> AnchorEntity? {
        let anchor = AnchorEntity(world: transform)
        anchor.name = "ghost"
        var shell = MeshDescriptor(name: "shell")
        shell.positions = MeshBuffers.Positions(g.positions)
        shell.primitives = .triangles(g.indices)
        var grey = UnlitMaterial(color: UIColor(white: 0.85, alpha: 1))
        grey.blending = .transparent(opacity: 0.3)
        if let m = try? MeshResource.generate(from: [shell]) { anchor.addChild(ModelEntity(mesh: m, materials: [grey])) }
        let rib = ribbon(outlines: g.outlines, width: 0.015)
        if !rib.indices.isEmpty {
            var edges = MeshDescriptor(name: "holes")
            edges.positions = MeshBuffers.Positions(rib.positions)
            edges.primitives = .triangles(rib.indices)
            if let m = try? MeshResource.generate(from: [edges]) {
                anchor.addChild(ModelEntity(mesh: m, materials: [UnlitMaterial(color: .systemRed)]))
            }
        }
        return anchor
    }
}
```

(Both faces of the shell should show: if the shell vanishes from behind, set `grey.faceCulling = .none`.)

- [ ] **Step 4: Recorder** — add:

```swift
    /// The parent's ghost and where it sits in this session (nil: hidden until cards line it up).
    @Published var ghost: Core.GhostMesh?
    @Published var ghostTransform: simd_float4x4?
    /// Pass frame to shot frame from the cards (column-major), when lined up by cards.
    private(set) var cardTransform: [Double]?
    private var warnedCards: Set<StationCard> = []
    private var parentData: Data?

    /// The rule for a card line-up: worst misfit up to 5 cm is accepted.
    nonisolated static func lineUp(fit: [String: Any]) -> (transform: [Double]?, message: String) {
        let worst = fit["worst_m"] as? Double ?? .infinity
        guard let t = fit["transform"] as? [Double], t.count == 16, worst <= 0.05 else {
            let mis = (fit["misfits"] as? [[String: Any]] ?? []).max { ($0["m"] as? Double ?? 0) < ($1["m"] as? Double ?? 0) }
            let word = (mis?["id"] as? Int).flatMap(StationCard.init(rawValue:))?.word ?? "A card"
            let cm = Int(((mis?["m"] as? Double) ?? worst) * 100)
            return (nil, "\(word) doesn't match where it was (\(cm) cm). Put it back, or carry on without the earlier scan.")
        }
        return (t, String(format: "Lined up by cards (worst %.0f mm).", worst * 1000))
    }
```

In `start()` when `parent != nil`: set `warnedCards = []`, `cardTransform = nil`, `ghostTransform = parentInSession ? matrix_identity_float4x4 : nil`, and if `ghost == nil` load it:

```swift
        if let p = parent, ghost == nil {
            Task {
                let t0 = Date()
                let loaded = await Task.detached { () -> (Data?, Core.GhostMesh?) in
                    let d = try? Data(contentsOf: p)
                    return (d, d.flatMap { Core.ghost(bundle: $0) })
                }.value
                parentData = loaded.0; ghost = loaded.1
                if let g = loaded.1 { Log.write("ghost", "\(g.indices.count / 3) triangles, \(g.outlines.count) outlines, \(Int(Date().timeIntervalSince(t0) * 1000)) ms") }
            }
        }
        if !parentInSession { message = "Look at two cards to show the earlier scan." }
```

After saving a pass, set `ghost = nil` and `parentData = nil` so the next pass rebuilds it from the new file.

In `cardsDone`, after `pinNewCards()`, when `parent != nil` and `let d = parentData`:

```swift
        let settled = cardTracker.seen.filter { $0.track.settled }
        let recs = cardTracker.records.filter { r in settled.contains { $0.track.card.rawValue == r.id } }
        if parentInSession {
            let offs = Core.cardsFit(bundle: d, cards: recs, sameSession: true)["offsets"] as? [[String: Any]] ?? []
            for o in offs {
                guard let id = o["id"] as? Int, let m = o["m"] as? Double, m > 0.05,
                      let c = StationCard(rawValue: id), !warnedCards.contains(c) else { continue }
                warnedCards.insert(c)
                message = "\(c.word) is \(Int(m * 100)) cm from where it was: card moved, or tracking slipped?"
                Log.write("cards", "check: \(c.word) \(Int(m * 1000)) mm from the earlier scan")
            }
        } else if cardTransform == nil, recs.count >= 2,
                  let fit = Core.cardsFit(bundle: d, cards: recs, sameSession: false)["fit"] as? [String: Any] {
            let r = Self.lineUp(fit: fit)
            message = r.message
            if let t = r.transform {
                cardTransform = t
                // shot frame -> this session's frame is the inverse of pass -> shot
                let m = simd_float4x4(columns: (SIMD4(Float(t[0]), Float(t[1]), Float(t[2]), Float(t[3])),
                                                SIMD4(Float(t[4]), Float(t[5]), Float(t[6]), Float(t[7])),
                                                SIMD4(Float(t[8]), Float(t[9]), Float(t[10]), Float(t[11])),
                                                SIMD4(Float(t[12]), Float(t[13]), Float(t[14]), Float(t[15]))))
                ghostTransform = m.inverse
                Log.write("cards", String(format: "lined up by cards: yaw %.1f deg, worst %.0f mm",
                                          (fit["yaw_rad"] as? Double ?? 0) * 180 / .pi, (fit["worst_m"] as? Double ?? 0) * 1000))
            } else {
                Log.write("cards", "card line-up refused: \(r.message)")
            }
        }
```

In `stop`, the Task 6 refusal becomes: if `parent != nil && !parentInSession && cardTransform == nil`, refuse with "Look at two cards so this pass can be lined up." (unchanged text) and keep capturing.

- [ ] **Step 5: View** — `ARMeshView` gains `var ghost: Core.GhostMesh?` and `var ghostTransform: simd_float4x4?`; in `updateUIView`:

```swift
        // Build the ghost once; afterwards only move it. Rebuilding the mesh on every update would cost
        // the scan its frame rate.
        let old = uiView.scene.anchors.first { $0.name == "ghost" }
        if let g = ghost, let t = ghostTransform {
            if let old {
                old.transform = Transform(matrix: t)
            } else if let e = GhostView.entity(g, transform: t) {
                uiView.scene.addAnchor(e)
            }
        } else if let old {
            uiView.scene.removeAnchor(old)
        }
```

The recorder sets `ghost = nil` after each saved pass, which removes the old entity; the next pass loads the new file's ghost and the view builds it once. Pass `ghost: rec.ghost, ghostTransform: rec.ghostTransform` from `CaptureView`.

- [ ] **Step 6: Run app tests** → `** TEST SUCCEEDED **`; also a generic device build: `xcodebuild -scheme CaveCADCapture -destination 'generic/platform=iOS' CODE_SIGNING_ALLOWED=NO build 2>&1 | tail -1` → `** BUILD SUCCEEDED **`

- [ ] **Step 7: Commit**

```bash
git add apps/capture-ios/Sources apps/capture-ios/Tests apps/capture-ios/project.yml
git commit -m "capture-ios: earlier scan as a ghost with hole outlines; card line-up after a restart; moved-card check"
```

---

### Task 8: On the phone with Nathan

**Goal:** Nathan scans a station, presses Stop, Scan more over a gap, Stop; the log and the saved shot show two passes, the ghost, the backoff and the frame rate.

**Files:** none (install and read).

**Acceptance Criteria:**
- [ ] Log shows `ghost: N triangles, M outlines` with N under 30000.
- [ ] Log shows `settled, backing off` during a pass and an `ARKit ... fps` line per summary.
- [ ] Log shows `saved pass 1` after the first Stop and `saved pass 2` after the second (the core's pass count read back from the file).
- [ ] "View shot" on the phone opens the station's shot with both passes' geometry.
- [ ] Nathan reports whether the red outlines matched the real gaps.

**Verify:** `apps/capture-ios/pull-log.sh 00008150-0011281122D2401C` → the lines above.

**Steps:**

- [ ] **Step 1:** `apps/capture-ios/install.sh 2ANLK72UUS 00008150-0011281122D2401C` → `installed and launched`.
- [ ] **Step 2:** Ask Nathan to scan a station with cards out, Stop, then Scan more aimed at a gap, Stop.
- [ ] **Step 3:** Pull the log and report in plain words: ghost size and build time, backoff, fps before and during backoff, card check warnings, both passes saved.
- [ ] **Step 4:** Update `HANDOFF.md` (gitignored, local) and the memory note `cavecad-capture-lidar` with the result.

---

## Self-review notes

- Spec coverage: flow (T6), storage + passes + safe rewrite (T1, T6), ghost (T2, T4, T7), card fallback + check (T3, T4, T7), lightweight backoff + fps log (T5), errors: unreadable parent (T6 `writePass` message), save failure keeps the old file (T6 `replace`), misfit refusal (T7), tracking lost mid-visit is "not same session" and waits for cards (T7). Not built (spec "Not in this version"): world map, later trips, ICP, removing a pass, remeshing.
- Names used across tasks: `Pass`, `NewPass`, `PassMesh`, `add_pass`, `Ghost`, `ghost`, `CardFit`, `cards_fit`, `card_offsets` (core); `cc_shot_add_pass`, `cc_shot_ghost`, `cc_cards_fit` (FFI); `Core.addPass`, `Core.ghost`, `Core.GhostMesh`, `Core.cardsFit`; `ShotFiles.parent`, `ShotFiles.replace`; `ShotJob.writePass`; `ShotRecorder.prepare`, `scanMore`, `lineUp`, `parent`, `parentInSession`, `cardTransform`, `ghost`, `ghostTransform`; `GhostView.ribbon`, `GhostView.entity`; `CardTrack.settled`, `CardTracker.allSettled`, `meshBatch(limit:skipSettled:)`.
