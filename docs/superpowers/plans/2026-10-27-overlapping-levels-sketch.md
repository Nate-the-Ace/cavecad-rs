# Sketching Over Overlapping Levels Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers-extended-cc:subagent-driven-development (recommended) or superpowers-extended-cc:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A map line drawn where two passages overlap in plan stays on the passage it was drawn on, shows which passage it took, flips to the other level in one tap, and old lines that hopped levels can be fixed in one go.

**Architecture:** A new anchoring function in `cave-doc` picks the whole stroke's chain of survey legs at once (dynamic programming over candidate legs, with a cost for jumping between unconnected passages or heights) instead of the nearest leg per point. `cave-draw` uses it for map strokes; `cave-scene` reports which legs a line hangs on and which lines hopped; `cave-wasm` exposes that; the web map glows the legs, shows an "Other level" chip and a "Fix N lines" button.

**Tech Stack:** Rust workspace (`cave-doc`, `cave-draw`, `cave-scene`, `cave-wasm`), Svelte 5 + TypeScript web app (`apps/web`), wasm-bindgen.

**User decisions (already made):**
- Approach C: automatic stays-on-its-passage anchoring + glow + one-tap "Other level" flip + repair command for old lines (brainstorm 2026-10-27).
- Spec: `docs/superpowers/specs/2026-10-27-overlapping-levels-sketch-design.md`.

**Deviations from spec, found while planning (code facts):**
- Map lines carry no trip in the Rust app (`map_line` writes no `FeatureTrip`; the trip filter only applies to shots and section sketches). Spec section 2's "trip stamping" has nothing to change. Dropped.
- There is no "continue the last stroke" feature in the Rust app yet, so "extending pins to the last leg" has no caller. Dropped; `reanchor_stroke`'s `pin` is the hook for it later.
- The flip is a chip only, not also "tap the other passage" (tapping the map already selects lines/shots; a second meaning for a tap is a conflict). YAGNI.

**Toolchain:** `export PATH=/opt/homebrew/opt/rustup/bin:$PATH` before any cargo command. Before each commit the repo's hand-run checks apply: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `python3 tools/make_code_index.py` (regenerate; commit `docs/CODE-INDEX.md` with the change).

**Work on a branch:** `git switch -c overlap-levels` from current `survey-sketch` tip (or from `origin/main` if survey-sketch has been merged by then).

---

## File structure

- Modify `crates/cave-doc/src/anchored.rs`: `Frame` gains `za`/`zb`; shared helpers (`check_stroke`, `usable`, `build_path`, `basis`, `distance_to_segment`, `along`) become `pub(crate)`; `anchor_stroke` is rewritten on top of them (same behaviour).
- Create `crates/cave-doc/src/level_anchor.rs`: the path rule (`anchor_stroke_path`), leg connectivity (`Hops`), `other_level`, `frame_index`, `path_hops_level`. One purpose: keeping a plan stroke on one level.
- Modify `crates/cave-doc/src/lib.rs`: module + exports.
- Modify `crates/cave-scene/src/maplines.rs`: `map_frames` fills z; `placed_line`, `line_level`, `hopped_lines`.
- Modify `crates/cave-scene/src/profile.rs:433`: profile frames set `za`/`zb` to 0.
- Modify `crates/cave-draw/src/lib.rs`: `map_line` and map `edit_stroke` use the path rule; new `reanchor_stroke`.
- Modify `crates/cave-wasm/src/lib.rs`: `add_map_line` returns the new id; new `map_line_level`, `reanchor_line`, `hopped_lines`, `fix_hopped_lines`.
- Modify `apps/web/src/lib/engine.ts`, `apps/web/src/lib/viewport/cave-view.ts`, `apps/web/src/components/Viewport3D.svelte`, `apps/web/harness/mock-impl.js`.

---

### Task 1: Frames carry elevation; anchoring helpers shared

**Goal:** `Frame` has `za`/`zb`, every construction site fills them, and `anchor_stroke` is split into reusable helpers with unchanged behaviour.

**Files:**
- Modify: `crates/cave-doc/src/anchored.rs` (struct at :15, `basis`/`distance_to_segment` at :63-72, `anchor_stroke` at :75-131, test literals at :199-210 and :269)
- Modify: `crates/cave-scene/src/maplines.rs:14-30`
- Modify: `crates/cave-scene/src/profile.rs:433-438`
- Modify: `crates/cave-draw/src/lib.rs:519-524` (test literal)

**Acceptance Criteria:**
- [ ] `Frame` has `pub za: f64` and `pub zb: f64`.
- [ ] `map_frames` sets them from solved station `z`; `profile_frames` sets 0.0.
- [ ] All existing `cave-doc`, `cave-draw`, `cave-scene` tests pass unchanged.

**Verify:** `cargo test -p cave-doc -p cave-draw -p cave-scene` → all pass

**Steps:**

- [ ] **Step 1: Add the fields.** In `anchored.rs` replace the struct:

```rust
/// Two neighbouring stations and where they are in the drawing's own 2D space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// First station.
    pub a: Id,
    /// Second station.
    pub b: Id,
    /// Position of `a`.
    pub pa: [f64; 2],
    /// Position of `b`.
    pub pb: [f64; 2],
    /// Elevation of `a` (metres up). Only plan anchoring reads it; profile frames leave it 0.
    pub za: f64,
    /// Elevation of `b`.
    pub zb: f64,
}
```

- [ ] **Step 2: Split `anchor_stroke` into helpers.** Replace `basis`, `distance_to_segment` and `anchor_stroke` (lines 63-131) with:

```rust
pub(crate) fn basis(pa: [f64; 2], pb: [f64; 2]) -> Option<([f64; 2], f64)> {
    let d = [pb[0] - pa[0], pb[1] - pa[1]];
    let len = d[0].hypot(d[1]);
    (len > 1e-9 && len.is_finite()).then_some((d, len))
}

/// How far along the leg from `pa` (0..1) the point nearest `p` is.
pub(crate) fn along(p: [f64; 2], pa: [f64; 2], d: [f64; 2], len: f64) -> f64 {
    (((p[0] - pa[0]) * d[0] + (p[1] - pa[1]) * d[1]) / (len * len)).clamp(0.0, 1.0)
}

pub(crate) fn distance_to_segment(p: [f64; 2], pa: [f64; 2], d: [f64; 2], len: f64) -> f64 {
    let t = along(p, pa, d, len);
    (p[0] - (pa[0] + t * d[0])).hypot(p[1] - (pa[1] + t * d[1]))
}

/// Refuses a stroke too short, with a non-finite point, or with pen weights that do not fit.
pub(crate) fn check_stroke(
    points: &[[f64; 2]],
    widths: &Option<Vec<f64>>,
) -> Result<(), AnchorError> {
    if points.len() < 2 {
        return Err(AnchorError::TooShort);
    }
    if points
        .iter()
        .any(|p| !p[0].is_finite() || !p[1].is_finite())
    {
        return Err(AnchorError::BadStroke);
    }
    if let Some(w) = widths
        && (w.len() != points.len() || w.iter().any(|x| !x.is_finite() || *x <= 0.0))
    {
        return Err(AnchorError::BadStroke);
    }
    Ok(())
}

/// The frames a stroke can hang on (not collapsed to a point): index, direction, length.
pub(crate) fn usable(frames: &[Frame]) -> Vec<(usize, [f64; 2], f64)> {
    frames
        .iter()
        .enumerate()
        .filter_map(|(i, f)| basis(f.pa, f.pb).map(|(d, l)| (i, d, l)))
        .collect()
}

/// The anchored path for `points`, point `k` hanging on `frames[choice[k]]`.
pub(crate) fn build_path(
    points: &[[f64; 2]],
    frames: &[Frame],
    choice: &[usize],
    widths: Option<Vec<f64>>,
) -> AnchoredPath {
    let mut used: Vec<usize> = Vec::new();
    let mut out = Vec::with_capacity(points.len());
    for (p, &i) in points.iter().zip(choice) {
        let (d, len) = basis(frames[i].pa, frames[i].pb).expect("chosen frames are usable");
        let pa = frames[i].pa;
        let (rx, ry) = (p[0] - pa[0], p[1] - pa[1]);
        let slot = used.iter().position(|&x| x == i).unwrap_or_else(|| {
            used.push(i);
            used.len() - 1
        });
        out.push(AnchorPoint {
            frame: slot as u32,
            u: (rx * d[0] + ry * d[1]) / (len * len),
            v: (-rx * d[1] + ry * d[0]) / len,
        });
    }
    AnchoredPath {
        closed: false,
        frames: used.iter().map(|&i| (frames[i].a, frames[i].b)).collect(),
        points: out,
        widths,
    }
}

/// Anchors a stroke drawn in the drawing's 2D space to the nearest of `frames`, point by point.
/// Right for the profile band and anywhere passages cannot overlap; plan strokes use
/// [`anchor_stroke_path`](crate::anchor_stroke_path).
pub fn anchor_stroke(
    points: &[[f64; 2]],
    frames: &[Frame],
    widths: Option<Vec<f64>>,
) -> Result<AnchoredPath, AnchorError> {
    check_stroke(points, &widths)?;
    let usable = usable(frames);
    if usable.is_empty() {
        return Err(AnchorError::NoFrames);
    }
    let choice: Vec<usize> = points
        .iter()
        .map(|p| {
            usable
                .iter()
                .min_by(|x, y| {
                    distance_to_segment(*p, frames[x.0].pa, x.1, x.2)
                        .total_cmp(&distance_to_segment(*p, frames[y.0].pa, y.1, y.2))
                })
                .expect("usable is not empty")
                .0
        })
        .collect();
    Ok(build_path(points, frames, &choice, widths))
}
```

- [ ] **Step 3: Fill the new fields at every construction site.**
  - `anchored.rs` tests (two `Frame { .. }` in `setup()` and one at :269): add `za: 0.0, zb: 0.0,` to each.
  - `crates/cave-draw/src/lib.rs:519` test literal: add `za: 0.0, zb: 0.0,`.
  - `crates/cave-scene/src/profile.rs:433`:

```rust
            .map(|w| Frame {
                a: w[0].id,
                b: w[1].id,
                pa: [w[0].s, w[0].z],
                pb: [w[1].s, w[1].z],
                za: 0.0,
                zb: 0.0,
            })
```

  - `crates/cave-scene/src/maplines.rs` in `map_frames`:

```rust
            Some(Frame {
                a: l.from,
                b: l.to,
                pa: [a.x, a.y],
                pb: [b.x, b.y],
                za: a.z,
                zb: b.z,
            })
```

- [ ] **Step 4: Run** `cargo test -p cave-doc -p cave-draw -p cave-scene` → all pass. `cargo clippy --all-targets -- -D warnings` → clean.

- [ ] **Step 5: Commit**

```bash
git add crates/cave-doc/src/anchored.rs crates/cave-scene/src/maplines.rs crates/cave-scene/src/profile.rs crates/cave-draw/src/lib.rs
git commit -m "anchored: frames carry elevation; anchoring split into shared helpers"
```

---

### Task 2: Path anchoring that stays on one level

**Goal:** `anchor_stroke_path` keeps a plan stroke on the passage it was drawn on where another passage overlaps in plan, with an optional pinned first leg; plus `other_level`, `frame_index`, `path_hops_level`.

**Files:**
- Create: `crates/cave-doc/src/level_anchor.rs`
- Modify: `crates/cave-doc/src/lib.rs:20` (module + exports)

**Acceptance Criteria:**
- [ ] Crossing fixture: a stroke along the upper passage over the crossing anchors only to upper legs; `anchor_stroke` (old rule) on the same stroke uses at least one lower leg (proves the fixture reproduces the bug).
- [ ] Moving the lower passage's stations does not move the placed upper stroke.
- [ ] With `pin` on a lower leg, a stroke drawn over the crossing anchors only to lower legs.
- [ ] Single-level stroke: `anchor_stroke_path` == `anchor_stroke`.
- [ ] `other_level` returns a lower leg for a start at the crossing chosen on the upper level, and `None` on a single-level cave.
- [ ] `path_hops_level` is true for the old-rule path, false for the new-rule path.

**Verify:** `cargo test -p cave-doc level_anchor` → 6 passed

**Steps:**

- [ ] **Step 1: Write the module with its failing tests.** Create `crates/cave-doc/src/level_anchor.rs`:

```rust
//! Anchoring a plan stroke as one path, so it stays on the passage it was drawn on where
//! passages on different levels overlap in plan.
//!
//! Per point nearest-leg anchoring ([`anchor_stroke`](crate::anchor_stroke)) lets a wall traced
//! along an upper passage grab the lower passage wherever that is closer in plan. Here the whole
//! stroke picks one chain of legs: staying on a leg is free, stepping to a connected leg is cheap,
//! jumping to an unconnected passage or another height is dear.

use crate::anchored::{
    AnchorError, AnchoredPath, Frame, along, basis, build_path, check_stroke,
    distance_to_segment, usable,
};
use cave_id::Id;
use std::collections::{HashMap, VecDeque};

/// Cost, in metres of plan distance, of each hop along the survey between two legs.
const HOP_COST: f64 = 0.5;
/// Hops along the survey beyond which two legs count as different passages.
const HOP_CAP: u32 = 8;
/// Cost of jumping to a leg more than `HOP_CAP` hops away, or not connected at all.
const UNCONNECTED_COST: f64 = 50.0;
/// Cost per metre of height between the leg left and the leg joined.
const Z_COST: f64 = 2.0;
/// Legs farther than this beyond a point's nearest leg are not considered for that point.
const SEARCH_RADIUS: f64 = 15.0;
/// Most legs considered per point.
const MAX_CANDIDATES: usize = 16;
/// An other-level leg this close (plan metres) to the stroke's first point makes its start
/// ambiguous: the stroke may belong to either level.
pub const AMBIGUOUS_RADIUS: f64 = 4.0;
/// Height (metres) between two legs beyond which they are different levels even when the survey
/// joins them nearby, as levels in a real cave usually are.
const LEVEL_STEP: f64 = 3.0;

/// True when legs `i` and `j` belong to different levels: not connected within `HOP_CAP`, or
/// more than `LEVEL_STEP` apart in height (`zi`, `zj`).
fn other_levels(hops: &mut Hops, i: usize, j: usize, zi: f64, zj: f64) -> bool {
    hops.between(i, j).is_none() || (zi - zj).abs() > LEVEL_STEP
}

fn mid_z(f: &Frame) -> f64 {
    (f.za + f.zb) / 2.0
}

/// How many hops along the survey separate two legs (legs sharing a station are one hop apart).
pub struct Hops<'a> {
    frames: &'a [Frame],
    at: HashMap<Id, Vec<usize>>,
    reach: HashMap<usize, HashMap<usize, u32>>,
}

impl<'a> Hops<'a> {
    pub fn new(frames: &'a [Frame]) -> Self {
        let mut at: HashMap<Id, Vec<usize>> = HashMap::new();
        for (i, f) in frames.iter().enumerate() {
            at.entry(f.a).or_default().push(i);
            at.entry(f.b).or_default().push(i);
        }
        Hops { frames, at, reach: HashMap::new() }
    }

    /// Hops from leg `i` to leg `j`, or `None` when more than `HOP_CAP` apart or not connected.
    pub fn between(&mut self, i: usize, j: usize) -> Option<u32> {
        let (at, frames) = (&self.at, self.frames);
        self.reach
            .entry(i)
            .or_insert_with(|| {
                let mut dist = HashMap::from([(i, 0u32)]);
                let mut queue = VecDeque::from([i]);
                while let Some(f) = queue.pop_front() {
                    let d = dist[&f];
                    if d == HOP_CAP {
                        continue;
                    }
                    for s in [frames[f].a, frames[f].b] {
                        for &g in at.get(&s).into_iter().flatten() {
                            dist.entry(g).or_insert_with(|| {
                                queue.push_back(g);
                                d + 1
                            });
                        }
                    }
                }
                dist
            })
            .get(&j)
            .copied()
    }
}

/// Elevation on leg `i` at the point nearest `p`.
fn z_at(frames: &[Frame], i: usize, p: [f64; 2]) -> f64 {
    let f = &frames[i];
    match basis(f.pa, f.pb) {
        Some((d, len)) => f.za + along(p, f.pa, d, len) * (f.zb - f.za),
        None => f.za,
    }
}

fn distance(frames: &[Frame], i: usize, p: [f64; 2]) -> f64 {
    let f = &frames[i];
    basis(f.pa, f.pb).map_or(f64::INFINITY, |(d, len)| distance_to_segment(p, f.pa, d, len))
}

/// Anchors a plan stroke as one path. `pin`, when given, is the index into `frames` of the leg
/// the first point must hang on (the "other level" flip).
pub fn anchor_stroke_path(
    points: &[[f64; 2]],
    frames: &[Frame],
    widths: Option<Vec<f64>>,
    pin: Option<usize>,
) -> Result<AnchoredPath, AnchorError> {
    check_stroke(points, &widths)?;
    let usable: Vec<usize> = usable(frames).into_iter().map(|(i, _, _)| i).collect();
    if usable.is_empty() || pin.is_some_and(|p| !usable.contains(&p)) {
        return Err(AnchorError::NoFrames);
    }
    let candidates: Vec<Vec<(usize, f64)>> = points
        .iter()
        .enumerate()
        .map(|(k, p)| {
            if k == 0
                && let Some(pin) = pin
            {
                return vec![(pin, distance(frames, pin, *p))];
            }
            let mut all: Vec<(usize, f64)> =
                usable.iter().map(|&i| (i, distance(frames, i, *p))).collect();
            all.sort_by(|a, b| a.1.total_cmp(&b.1));
            let near = all[0].1;
            all.into_iter()
                .take_while(|c| c.1 <= near + SEARCH_RADIUS)
                .take(MAX_CANDIDATES)
                .collect()
        })
        .collect();
    let mut hops = Hops::new(frames);
    let mut cost: Vec<f64> = candidates[0].iter().map(|c| c.1).collect();
    let mut back: Vec<Vec<usize>> = vec![Vec::new()];
    for k in 1..points.len() {
        let mut next = Vec::with_capacity(candidates[k].len());
        let mut from = Vec::with_capacity(candidates[k].len());
        for &(j, dj) in &candidates[k] {
            let zj = z_at(frames, j, points[k]);
            let (best, arg) = candidates[k - 1]
                .iter()
                .enumerate()
                .map(|(m, &(i, _))| {
                    let step = if i == j {
                        0.0
                    } else {
                        let walk = hops
                            .between(i, j)
                            .map_or(UNCONNECTED_COST, |h| HOP_COST * h as f64);
                        walk + Z_COST * (z_at(frames, i, points[k - 1]) - zj).abs()
                    };
                    (cost[m] + step, m)
                })
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .expect("every point has a candidate");
            next.push(best + dj);
            from.push(arg);
        }
        cost = next;
        back.push(from);
    }
    let mut m = (0..cost.len())
        .min_by(|a, b| cost[*a].total_cmp(&cost[*b]))
        .expect("the last point has a candidate");
    let mut choice = vec![0usize; points.len()];
    for k in (0..points.len()).rev() {
        choice[k] = candidates[k][m].0;
        if k > 0 {
            m = back[k][m];
        }
    }
    Ok(build_path(points, frames, &choice, widths))
}

/// The nearest leg within [`AMBIGUOUS_RADIUS`] of `p` on another level than `chosen`: the
/// other level a stroke starting at `p` could belong to. `None` when the start is not ambiguous.
pub fn other_level(p: [f64; 2], frames: &[Frame], chosen: usize) -> Option<usize> {
    let mut hops = Hops::new(frames);
    let zc = z_at(frames, chosen, p);
    usable(frames)
        .into_iter()
        .map(|(i, _, _)| (i, distance(frames, i, p)))
        .filter(|&(i, d)| {
            d <= AMBIGUOUS_RADIUS && other_levels(&mut hops, chosen, i, zc, z_at(frames, i, p))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// Index in `frames` of the leg between stations `a` and `b` (either way round).
pub fn frame_index(frames: &[Frame], (a, b): (Id, Id)) -> Option<usize> {
    frames
        .iter()
        .position(|f| (f.a == a && f.b == b) || (f.a == b && f.b == a))
}

/// True when the path steps between two consecutive points onto a leg on another level than the
/// one before: a line that hopped levels under the old rule. Legs missing from `frames` say
/// nothing.
pub fn path_hops_level(path: &AnchoredPath, frames: &[Frame]) -> bool {
    let index: Vec<Option<usize>> = path.frames.iter().map(|ab| frame_index(frames, *ab)).collect();
    let mut hops = Hops::new(frames);
    path.points.windows(2).any(|w| {
        let (x, y) = (w[0].frame as usize, w[1].frame as usize);
        x != y
            && matches!((index[x], index[y]), (Some(i), Some(j))
                if other_levels(&mut hops, i, j, mid_z(&frames[i]), mid_z(&frames[j])))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchor_stroke;
    use cave_id::{IdSource, SeqIds};

    /// Upper passage along x at z 0; lower passage along y at z -8, crossing it in plan at the
    /// origin; joined far away (U4 - J - L0), so it is one survey, 6 hops round.
    struct Crossing {
        frames: Vec<Frame>,
        pos: HashMap<Id, [f64; 2]>,
        upper: Vec<usize>,
        lower: Vec<usize>,
    }

    fn crossing() -> Crossing {
        let mut ids = SeqIds::new(1);
        let mut st = |x: f64, y: f64, z: f64| (ids.next_id(), [x, y], z);
        let u: Vec<_> = (0..5).map(|k| st(-20.0 + 10.0 * k as f64, 0.0, 0.0)).collect();
        let j = st(30.0, -30.0, -4.0);
        let l: Vec<_> = (0..5).map(|k| st(0.0, -20.0 + 10.0 * k as f64, -8.0)).collect();
        let leg = |p: &(Id, [f64; 2], f64), q: &(Id, [f64; 2], f64)| Frame {
            a: p.0,
            b: q.0,
            pa: p.1,
            pb: q.1,
            za: p.2,
            zb: q.2,
        };
        let mut frames: Vec<Frame> = u.windows(2).map(|w| leg(&w[0], &w[1])).collect();
        let upper: Vec<usize> = (0..frames.len()).collect();
        frames.push(leg(&u[4], &j));
        frames.push(leg(&j, &l[0]));
        let first_lower = frames.len();
        frames.extend(l.windows(2).map(|w| leg(&w[0], &w[1])));
        let lower: Vec<usize> = (first_lower..frames.len()).collect();
        let pos = u.iter().chain([&j]).chain(l.iter()).map(|s| (s.0, s.1)).collect();
        Crossing { frames, pos, upper, lower }
    }

    /// A wall traced 2 m north of the upper passage, right across the crossing.
    fn upper_wall() -> Vec<[f64; 2]> {
        (0..13).map(|k| [-15.0 + 2.5 * k as f64, 2.0]).collect()
    }

    fn legs_used(c: &Crossing, path: &AnchoredPath) -> Vec<usize> {
        path.frames.iter().map(|ab| frame_index(&c.frames, *ab).unwrap()).collect()
    }

    #[test]
    fn level_anchor_stays_on_the_upper_passage_over_the_crossing() {
        let c = crossing();
        let old = anchor_stroke(&upper_wall(), &c.frames, None).unwrap();
        assert!(legs_used(&c, &old).iter().any(|i| c.lower.contains(i)), "fixture must reproduce the bug");
        let new = anchor_stroke_path(&upper_wall(), &c.frames, None, None).unwrap();
        assert!(legs_used(&c, &new).iter().all(|i| c.upper.contains(i)));
    }

    #[test]
    fn level_anchor_moving_the_lower_passage_leaves_the_upper_line() {
        let c = crossing();
        let path = anchor_stroke_path(&upper_wall(), &c.frames, None, None).unwrap();
        let before = path.place(&|id| c.pos.get(&id).copied()).unwrap();
        let lower_ids: Vec<Id> = c.lower.iter().flat_map(|&i| [c.frames[i].a, c.frames[i].b]).collect();
        let moved: HashMap<Id, [f64; 2]> = c
            .pos
            .iter()
            .map(|(id, p)| (*id, if lower_ids.contains(id) { [p[0] + 3.0, p[1]] } else { *p }))
            .collect();
        let after = path.place(&|id| moved.get(&id).copied()).unwrap();
        assert!(before.iter().zip(&after).all(|(a, b)| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9));
    }

    #[test]
    fn level_anchor_pin_puts_the_stroke_on_the_lower_passage() {
        let c = crossing();
        let wall: Vec<[f64; 2]> = (0..9).map(|k| [1.5, -10.0 + 2.5 * k as f64]).collect();
        let pin = c.lower[2]; // L2-L3, the lower leg under the crossing
        let path = anchor_stroke_path(&wall, &c.frames, None, Some(pin)).unwrap();
        assert!(legs_used(&c, &path).iter().all(|i| c.lower.contains(i)));
    }

    #[test]
    fn level_anchor_single_level_matches_the_old_rule() {
        let mut ids = SeqIds::new(1);
        let (a, b, cc) = (ids.next_id(), ids.next_id(), ids.next_id());
        let frames = vec![
            Frame { a, b, pa: [0.0, 0.0], pb: [10.0, 0.0], za: 0.0, zb: 0.0 },
            Frame { a: b, b: cc, pa: [10.0, 0.0], pb: [10.0, -8.0], za: 0.0, zb: -1.0 },
        ];
        let stroke = [[1.0, 1.0], [5.0, 1.0], [9.0, 1.0], [11.0, -3.0], [11.0, -7.0]];
        assert_eq!(
            anchor_stroke_path(&stroke, &frames, None, None).unwrap(),
            anchor_stroke(&stroke, &frames, None).unwrap()
        );
        assert_eq!(other_level([1.0, 1.0], &frames, 0), None);
    }

    #[test]
    fn level_anchor_other_level_finds_the_lower_leg_at_the_crossing() {
        let c = crossing();
        let chosen = c.upper[1]; // U1-U2, ending at the crossing
        let other = other_level([0.5, 1.0], &c.frames, chosen).unwrap();
        assert!(c.lower.contains(&other));
    }

    #[test]
    fn level_anchor_hopped_path_is_detected() {
        let c = crossing();
        let old = anchor_stroke(&upper_wall(), &c.frames, None).unwrap();
        let new = anchor_stroke_path(&upper_wall(), &c.frames, None, None).unwrap();
        assert!(path_hops_level(&old, &c.frames));
        assert!(!path_hops_level(&new, &c.frames));
    }
}
```

Note on the fixture: the upper and lower legs at the crossing are 7 hops apart (U1U2→U2U3→U3U4→U4J→JL0→L0L1→L1L2→L2L3), within `HOP_CAP`, deliberately: real caves join their levels nearby. So "other level" in `other_level` and `path_hops_level` rests on the 8 m height gap (`LEVEL_STEP`), and the path rule's switch cost is hops (3.5) + height (16). Do not loosen any assertion to make a test pass; tune the named constants and record why.

- [ ] **Step 2: Wire the module.** In `crates/cave-doc/src/lib.rs` add `mod level_anchor;` beside `mod anchored;` and extend exports:

```rust
pub use anchored::{AnchorError, AnchorPoint, AnchoredPath, Frame, anchor_stroke};
pub use level_anchor::{
    AMBIGUOUS_RADIUS, Hops, anchor_stroke_path, frame_index, other_level, path_hops_level,
};
```

- [ ] **Step 3: Run** `cargo test -p cave-doc level_anchor` → 6 passed. If the first test's "fixture must reproduce the bug" fails, the fixture is wrong, not the code: move the wall closer to the lower passage (y 1.0) and re-run.

- [ ] **Step 4: Commit**

```bash
git add crates/cave-doc/src/level_anchor.rs crates/cave-doc/src/lib.rs
git commit -m "cave-doc: anchor a plan stroke as one path so it stays on its level"
```

---

### Task 3: Map strokes use the path rule; re-anchor a stroke

**Goal:** New and edited map strokes anchor with `anchor_stroke_path`; `reanchor_stroke` re-anchors a map stroke with a pinned first leg.

**Files:**
- Modify: `crates/cave-draw/src/lib.rs` (imports :20, `map_line` :158-177, `edit_stroke` :247-268; add `reanchor_stroke` after `edit_stroke`; tests module)

**Acceptance Criteria:**
- [ ] `map_line` and map-view `edit_stroke` call `anchor_stroke_path(.., None)`; profile `edit_stroke` still calls `anchor_stroke`.
- [ ] `reanchor_stroke` on a map line returns one `Action::Set` of `FeatureAnchored` whose first point hangs on the pinned leg, keeps `closed`; refuses a non-map line with `DrawError::BadStroke`.

**Verify:** `cargo test -p cave-draw` → all pass incl. `reanchor_pins_the_first_leg`

**Steps:**

- [ ] **Step 1: Failing test.** In the tests module of `crates/cave-draw/src/lib.rs`, using the existing fixture that returns `(doc, frames, [a, b])` (around :505-525; call it as the other tests do — check its name there) add:

```rust
    #[test]
    fn reanchor_pins_the_first_leg() {
        let (mut doc, frames, _) = setup();
        let (id, actions) =
            map_line(&doc, &mut SeqIds::new(900), "wall", &stroke(), None, &frames).unwrap();
        doc.edit(actions).unwrap();
        let actions = reanchor_stroke(&doc, id, &stroke(), None, &frames, 0).unwrap();
        assert_eq!(actions.len(), 1);
        let Action::Set { field, value: Value::Anchored(Some(p)), .. } = &actions[0] else {
            panic!("expected an anchored set");
        };
        assert_eq!(*field, Field::FeatureAnchored);
        assert_eq!(p.frames[p.points[0].frame as usize], (frames[0].a, frames[0].b));
    }
```

(If the fixture function is named differently than `setup`, use its name; `SeqIds` is from `cave_id`, imported the same way the other tests import it.)

- [ ] **Step 2: Run** `cargo test -p cave-draw reanchor` → FAIL: `cannot find function reanchor_stroke`.

- [ ] **Step 3: Implement.** Import: `use cave_doc::{Action, AnchorError, Document, Field, Frame, Kind, Value, anchor_stroke, anchor_stroke_path};`

In `map_line`, change the doc comment's "anchored to the nearest of them" to "anchored as one path, so it stays on the passage it was drawn on where levels overlap" and replace `anchor_stroke(&pts, frames, widths)?` with `anchor_stroke_path(&pts, frames, widths, None)?`.

In `edit_stroke`, replace the `else` branch:

```rust
    } else {
        let mut anchored = if view == VIEW_MAP {
            anchor_stroke_path(&pts, frames, widths, None)?
        } else {
            anchor_stroke(&pts, frames, widths)?
        };
        anchored.closed = closed && pts.len() >= 3;
        (Field::FeatureAnchored, Value::Anchored(Some(anchored)))
    };
```

After `edit_stroke` add:

```rust
/// Actions that re-anchor a map stroke at its current shape (`points`, plan metres) with its
/// first point held on `frames[pin]`: moves a line drawn where two levels overlap onto the other
/// level, or re-attaches a line that hopped levels.
pub fn reanchor_stroke(
    doc: &Document,
    id: Id,
    points: &[[f64; 2]],
    widths: Option<Vec<f64>>,
    frames: &[Frame],
    pin: usize,
) -> Result<Vec<Action>, DrawError> {
    let (view, _) = drawn_in(doc, id).ok_or(DrawError::NoStations)?;
    if view != VIEW_MAP {
        return Err(DrawError::BadStroke);
    }
    let (pts, widths) = distinct(points, widths)?;
    let mut anchored = anchor_stroke_path(&pts, frames, widths, Some(pin))?;
    anchored.closed = is_closed(doc, id) && pts.len() >= 3;
    Ok(vec![Action::Set {
        id,
        field: Field::FeatureAnchored,
        value: Value::Anchored(Some(anchored)),
    }])
}
```

- [ ] **Step 4: Run** `cargo test -p cave-draw` → all pass. `cargo clippy --all-targets -- -D warnings` → clean.

- [ ] **Step 5: Commit**

```bash
git add crates/cave-draw/src/lib.rs
git commit -m "cave-draw: map strokes anchor as one path; reanchor_stroke pins the first leg"
```

---

### Task 4: Scene answers which level a line is on, and which lines hopped

**Goal:** `cave-scene` gives a map line's current plan points, the 3D legs it hangs on plus the other level it could be on, and the list of lines that hopped levels.

**Files:**
- Modify: `crates/cave-scene/src/maplines.rs` (add after `map_frames`; tests module at end)
- Modify: `crates/cave-scene/src/lib.rs` (re-export, if `maplines` items are re-exported there — follow how `map_frames` is exported)

**Acceptance Criteria:**
- [ ] `placed_line(doc, line, adj)` returns the line's plan points and widths; `None` for a non-map or unplaced line.
- [ ] `line_level(doc, line, adj)` returns `LineLevel { legs, other }`: `legs` = 3D ends of each leg the line hangs on; `other` = station ids `[a, b]` of the other level's leg when the start is ambiguous, else `None`.
- [ ] `hopped_lines(doc, adj)` lists map lines for which `path_hops_level` is true.
- [ ] Test on a two-level document: a line written with the old `anchor_stroke` is in `hopped_lines`; one written with `map_line` is not; `line_level` of a line started at the crossing has `other: Some(..)`.

**Verify:** `cargo test -p cave-scene maplines` → all pass

**Steps:**

- [ ] **Step 1: Failing tests.** Build the document fixture the way the existing tests in `crates/cave-scene/tests/` or `maplines.rs` build a surveyed cave (look for a helper that adds stations and shots; reuse it). Survey: upper run U0-U4 east along y=0 at z 0, a join U4-J-L0, lower run L0-L4 north along x=0 at z -8 (same geometry as Task 2's fixture). Then:

```rust
    #[test]
    fn maplines_hopped_line_is_listed_and_a_path_line_is_not() {
        let mut doc = two_levels(); // the fixture above
        let frames = map_frames(&doc, false);
        let wall: Vec<[f64; 2]> = (0..13).map(|k| [-15.0 + 2.5 * k as f64, 2.0]).collect();
        // an old-rule line, written as the app wrote them before this change
        let (old_id, mut old) = cave_draw::map_line(&doc, &mut cave_id::SeqIds::new(800), "wall", &wall, None, &frames).unwrap();
        let hopped = cave_doc::anchor_stroke(&wall, &frames, None).unwrap();
        old.push(cave_doc::Action::Set { id: old_id, field: Field::FeatureAnchored, value: Value::Anchored(Some(hopped)) });
        doc.edit(old).unwrap();
        let (new_id, new) = cave_draw::map_line(&doc, &mut cave_id::SeqIds::new(850), "wall", &wall, None, &frames).unwrap();
        doc.edit(new).unwrap();
        let list = hopped_lines(&doc, false);
        assert!(list.contains(&old_id));
        assert!(!list.contains(&new_id));
    }

    #[test]
    fn maplines_line_started_at_the_crossing_offers_the_other_level() {
        let mut doc = two_levels();
        let frames = map_frames(&doc, false);
        let stroke = [[0.5, 1.0], [-5.0, 1.0], [-12.0, 1.0]];
        let (id, a) = cave_draw::map_line(&doc, &mut cave_id::SeqIds::new(800), "wall", &stroke, None, &frames).unwrap();
        doc.edit(a).unwrap();
        let level = line_level(&doc, id, false).unwrap();
        assert!(!level.legs.is_empty());
        assert!(level.other.is_some());
        let (pts, _) = placed_line(&doc, id, false).unwrap();
        assert!((pts[0][0] - 0.5).abs() < 1e-6 && (pts[0][1] - 1.0).abs() < 1e-6);
    }
```

- [ ] **Step 2: Run** `cargo test -p cave-scene maplines` → FAIL: unresolved `hopped_lines`, `line_level`, `placed_line`.

- [ ] **Step 3: Implement** in `maplines.rs` (add `use serde::Serialize;`, `use cave_doc::{AnchoredPath, frame_index, other_level, path_hops_level};`, `use cave_id::Id;` as needed):

```rust
/// A map line's anchored path, if `line` is a stroke drawn in plan.
fn plan_path(doc: &Document, line: Id) -> Option<AnchoredPath> {
    let e = doc.entity(line)?;
    if e.text(Field::FeatureView) != "map" {
        return None;
    }
    match e.get(Field::FeatureAnchored) {
        Value::Anchored(Some(a)) => Some(a),
        _ => None,
    }
}

/// A map line's plan points (east, north) and pen weights where the survey puts it now.
pub fn placed_line(
    doc: &Document,
    line: Id,
    use_adjustment: bool,
) -> Option<(Vec<[f64; 2]>, Option<Vec<f64>>)> {
    let path = plan_path(doc, line)?;
    let s = solve(doc, use_adjustment);
    let pts = path.place(&|id| s.positions.get(&id).map(|p| [p.x, p.y]))?;
    Some((pts, path.widths.clone()))
}

/// Which passage a map line hangs on, for the map to glow, and the other level it could be on.
#[derive(Debug, Serialize)]
pub struct LineLevel {
    /// Each leg the line hangs on: its two ends, `[east, north, up]` metres.
    pub legs: Vec<[[f64; 3]; 2]>,
    /// The other level's leg `[a, b]` (station ids) when the line starts where levels overlap.
    pub other: Option<[String; 2]>,
}

/// [`LineLevel`] for `line`, or `None` when it is not a placed map line.
pub fn line_level(doc: &Document, line: Id, use_adjustment: bool) -> Option<LineLevel> {
    let path = plan_path(doc, line)?;
    let s = solve(doc, use_adjustment);
    let at = |id: Id| s.positions.get(&id).map(|p| [p.x, p.y, p.z]);
    let legs = path
        .frames
        .iter()
        .map(|(a, b)| Some([at(*a)?, at(*b)?]))
        .collect::<Option<Vec<_>>>()?;
    let frames = map_frames(doc, use_adjustment);
    let first = path.frames[path.points[0].frame as usize];
    let start = path.place(&|id| s.positions.get(&id).map(|p| [p.x, p.y]))?[0];
    let other = frame_index(&frames, first)
        .and_then(|i| other_level(start, &frames, i))
        .map(|k| [frames[k].a.to_string(), frames[k].b.to_string()]);
    Some(LineLevel { legs, other })
}

/// Map lines that step between legs of different passages: drawn before strokes stayed on
/// their level, and worth re-attaching.
pub fn hopped_lines(doc: &Document, use_adjustment: bool) -> Vec<Id> {
    let frames = map_frames(doc, use_adjustment);
    doc.ids_of(Kind::Feature)
        .into_iter()
        .filter(|id| plan_path(doc, *id).is_some_and(|p| path_hops_level(&p, &frames)))
        .collect()
}
```

(`e.get(..)` returns the value by value or by reference depending on the `Entity` API; match the pattern used by `is_closed` in `crates/cave-draw/src/lib.rs:271`, cloning the path if `get` returns a reference.)

- [ ] **Step 4: Run** `cargo test -p cave-scene` → all pass; clippy clean.

- [ ] **Step 5: Commit**

```bash
git add crates/cave-scene/src
git commit -m "cave-scene: a map line's level, its other level, and lines that hopped"
```

---

### Task 5: Wasm entries

**Goal:** The web app can learn a new line's id, read its level, flip it, list and fix hopped lines.

**Files:**
- Modify: `crates/cave-wasm/src/lib.rs` (`add_map_line` :291-334; add new functions after `set_line_closed`)
- Modify: `apps/web/src/lib/engine.ts:2` (import) and near :415 (wrappers)
- Modify: `apps/web/harness/mock-impl.js` (stubs for the four new exports)

**Acceptance Criteria:**
- [ ] `add_map_line` returns `ok <id>` on success (was `ok`).
- [ ] `map_line_level(id, adjusted)` → JSON `LineLevel` or `null`.
- [ ] `reanchor_line(id, a, b, adjusted)` → `ok` / `error: ...`, one undoable edit.
- [ ] `hopped_lines(adjusted)` → JSON array of ids; `fix_hopped_lines(adjusted)` → `ok <n>`, all fixes as one undoable edit, `ok 0` when none.
- [ ] Wasm builds for the web; `npx svelte-check` clean.

**Verify:** `cargo test -p cave-wasm && cargo clippy -p cave-wasm --all-targets -- -D warnings && (cd apps/web && npx svelte-check)` → pass

**Steps:**

- [ ] **Step 1: `add_map_line` returns the id.** In its closure change `Ok::<(), String>(())` to `Ok::<cave_id::Id, String>(id)` and the match to:

```rust
    match result {
        Ok(id) => {
            tracing::info!(source = "sketch", "added a {kind} line to the map");
            format!("ok {id}")
        }
        Err(e) => format!("error: {e}"),
    }
```

Update its doc comment: "Returns `ok <id>` or `error: ...`."

- [ ] **Step 2: New functions** after `set_line_closed`:

```rust
/// Which passage a map line hangs on (legs to glow) and the other level it could be on, as JSON
/// (`{"legs": [[[e,n,u],[e,n,u]], ...], "other": ["a","b"] | null}`), or `null`.
#[wasm_bindgen]
pub fn map_line_level(id: &str, adjusted: bool) -> String {
    let Ok(line) = id.parse::<cave_id::Id>() else {
        return "null".into();
    };
    DOC.with(|d| {
        d.borrow()
            .as_ref()
            .and_then(|doc| cave_scene::line_level(doc, line, adjusted))
            .and_then(|l| serde_json::to_string(&l).ok())
            .unwrap_or_else(|| "null".into())
    })
}

/// Moves a map line onto another level: re-anchors it where it is drawn now, its first point on
/// the leg between stations `a` and `b`. One undoable edit. Returns `ok` or `error: ...`.
#[wasm_bindgen]
pub fn reanchor_line(id: &str, a: &str, b: &str, adjusted: bool) -> String {
    let (Ok(a), Ok(b)) = (a.parse::<cave_id::Id>(), b.parse::<cave_id::Id>()) else {
        return "error: no such leg".into();
    };
    change_line(id, |doc, line| {
        let frames = cave_scene::map_frames(doc, adjusted);
        let pin = cave_doc::frame_index(&frames, (a, b)).ok_or(cave_draw::DrawError::NoStations)?;
        let (pts, widths) =
            cave_scene::placed_line(doc, line, adjusted).ok_or(cave_draw::DrawError::NoStations)?;
        cave_draw::reanchor_stroke(doc, line, &pts, widths, &frames, pin)
    })
}

/// Map lines that hop between passages on different levels (JSON array of ids).
#[wasm_bindgen]
pub fn hopped_lines(adjusted: bool) -> String {
    DOC.with(|d| {
        let ids: Vec<String> = d
            .borrow()
            .as_ref()
            .map(|doc| cave_scene::hopped_lines(doc, adjusted))
            .unwrap_or_default()
            .into_iter()
            .map(|id| id.to_string())
            .collect();
        serde_json::to_string(&ids).unwrap_or_else(|_| "[]".into())
    })
}

/// Re-attaches every map line that hops levels to the level its first point is on. One
/// undoable edit. Returns `ok <how many>` or `error: ...`.
#[wasm_bindgen]
pub fn fix_hopped_lines(adjusted: bool) -> String {
    let result = DOC.with(|d| {
        let mut slot = d.borrow_mut();
        let doc = slot.as_mut().ok_or("no cave is open")?;
        let frames = cave_scene::map_frames(doc, adjusted);
        let mut actions = Vec::new();
        let lines = cave_scene::hopped_lines(doc, adjusted);
        for line in &lines {
            let Some((pts, widths)) = cave_scene::placed_line(doc, *line, adjusted) else {
                continue;
            };
            let Some(Value::Anchored(Some(path))) =
                doc.entity(*line).map(|e| e.get(cave_doc::Field::FeatureAnchored))
            else {
                continue;
            };
            let Some(pin) = cave_doc::frame_index(&frames, path.frames[path.points[0].frame as usize])
            else {
                continue;
            };
            actions.extend(
                cave_draw::reanchor_stroke(doc, *line, &pts, widths, &frames, pin)
                    .map_err(|e| format!("{e:?}"))?,
            );
        }
        if actions.is_empty() {
            return Ok(0);
        }
        let edit = doc.edit(actions).map_err(|e| format!("{e:?}"))?;
        record_edit("map", &edit);
        Ok::<usize, String>(lines.len())
    });
    match result {
        Ok(n) => format!("ok {n}"),
        Err(e) => format!("error: {e}"),
    }
}
```

(`Value` must be in scope as `cave_doc::Value`; match the `e.get` by-value/by-reference shape used in Task 4. If `DrawError` is not `pub` re-exported from `cave_draw`, use the path the crate exports it under.)

- [ ] **Step 3: engine.ts.** Add `map_line_level, reanchor_line, hopped_lines, fix_hopped_lines` to the import list on line 2, and next to `addMapLine`:

```ts
  /** The legs a map line hangs on (to glow) and the other level it could be on, or null. */
  mapLineLevel: (id: string, adjusted: boolean): { legs: [[number, number, number], [number, number, number]][]; other: [string, string] | null } | null => JSON.parse(map_line_level(id, adjusted)),
  reanchorLine: (id: string, a: string, b: string, adjusted: boolean): string => reanchor_line(id, a, b, adjusted),
  hoppedLines: (adjusted: boolean): string[] => JSON.parse(hopped_lines(adjusted)),
  fixHoppedLines: (adjusted: boolean): string => fix_hopped_lines(adjusted),
```

- [ ] **Step 4: Harness stubs.** In `apps/web/harness/mock-impl.js`, next to the existing `add_map_line`/`edit_line` stubs and in the same style, add: `map_line_level` → `'null'`, `reanchor_line` → `'ok'`, `hopped_lines` → `'[]'`, `fix_hopped_lines` → `'ok 0'`; change the `add_map_line` stub (if it returns `'ok'`) to return `'ok 1'`.

- [ ] **Step 5: Run** the Verify command → pass. Also `cargo build --target wasm32-unknown-unknown -p cave-wasm` → builds.

- [ ] **Step 6: Commit**

```bash
git add crates/cave-wasm/src/lib.rs apps/web/src/lib/engine.ts apps/web/harness/mock-impl.js
git commit -m "cave-wasm: new map line ids, line level, flip, hopped-line repair"
```

---

### Task 6: Map glow, "Other level" chip, "Fix lines" button

**Goal:** After a map stroke lands its legs glow for 6 s; an "Other level" chip flips it when its start is ambiguous; a toolbar button fixes hopped lines.

**Files:**
- Modify: `apps/web/src/lib/viewport/cave-view.ts` (field + `setAttachedLegs` near `setSelectedShot` :1054; drawing just before the "picked leg or splay" block :1411)
- Modify: `apps/web/src/components/Viewport3D.svelte` (`onLineDrawn` :289-301, state, toolbar near :381, chip near where `drawHint` renders, styles)

**Acceptance Criteria:**
- [ ] Drawing a wall over a crossing in the demo/test cave: the upper passage legs glow; "Other level" shows; clicking it moves the line onto the lower passage (line visibly re-attaches; 3D view shows it at the lower elevation); clicking again returns it; one Undo reverts the flip.
- [ ] A stroke on a single-level part of the cave glows but shows no chip.
- [ ] With a hopped line present, "Fix 1 line" shows in the map toolbar; clicking it re-attaches it and the button disappears; Undo brings it back.
- [ ] No console errors; `npx svelte-check` clean.

**Verify:** `cd apps/web && npx svelte-check` → 0 errors; then the live check in Step 5.

**Steps:**

- [ ] **Step 1: cave-view.ts.** Beside `setSelectedShot`:

```ts
  private attachedLegs: [[number, number, number], [number, number, number]][] = [];

  /** The legs a just-drawn map line hangs on, glowed so it is plain which passage (level) it took. */
  setAttachedLegs(list: [[number, number, number], [number, number, number]][]) {
    this.attachedLegs = list;
    this.requestRender();
  }
```

Just before `// the picked leg or splay: ...` in the overlay draw:

```ts
    // the legs a just-drawn line hangs on: a soft glow in the highlight colour
    if (this.attachedLegs.length) {
      g.lineCap = 'round';
      g.strokeStyle = `#${this.theme.accent.getHexString()}`;
      g.globalAlpha = 0.25;
      g.lineWidth = 9;
      for (const [from, to] of this.attachedLegs) {
        const a = this.project(from);
        const b = this.project(to);
        if (!a || !b) continue;
        g.beginPath();
        g.moveTo(a[0], a[1]);
        g.lineTo(b[0], b[1]);
        g.stroke();
      }
      g.globalAlpha = 1;
    }
```

- [ ] **Step 2: Viewport3D.svelte script.** Near the other `$state` declarations:

```ts
  let levelChoice = $state<{ id: string; other: [string, string] } | null>(null);
  let levelTimer = 0;
  let hopped = $state(0);

  /** Glows the legs a map line took and offers the other level when its start was ambiguous. */
  function showLevel(id: string) {
    const info = engine.mapLineLevel(id, cave.adjusted);
    view?.setAttachedLegs(info?.legs ?? []);
    levelChoice = info?.other ? { id, other: info.other } : null;
    clearTimeout(levelTimer);
    levelTimer = window.setTimeout(() => {
      view?.setAttachedLegs([]);
      levelChoice = null;
    }, 6000);
  }

  function flipLevel() {
    if (!levelChoice) return;
    const { id, other } = levelChoice;
    const r = engine.reanchorLine(id, other[0], other[1], cave.adjusted);
    if (r !== 'ok') engine.log('warn', 'draw', r);
    changed();
    showLevel(id);
  }

  function fixHopped() {
    const r = engine.fixHoppedLines(cave.adjusted);
    if (!r.startsWith('ok')) engine.log('warn', 'draw', r);
    changed();
  }
```

In `onLineDrawn`, replace the last three lines:

```ts
      const r = engine.addMapLine(cave.tool, points, widths, closed, cave.adjusted, text);
      if (!r.startsWith('ok')) engine.log('warn', 'draw', r);
      changed();
      if (r.startsWith('ok ')) showLevel(r.slice(3));
```

Recount hopped lines wherever the component re-reads the cave after a change (the function the comment at :241 describes — "Every edit ... redraws the survey"): add `hopped = engine.hoppedLines(cave.adjusted).length;` there.

- [ ] **Step 3: Markup.** In the toolbar, after the elevation-colour button (:381):

```svelte
    {#if hopped > 0}
      <button title="Some lines hop between passages on different levels. Re-attach each to the level it starts on." onclick={fixHopped}>Fix {hopped} line{hopped === 1 ? '' : 's'}</button>
    {/if}
```

Beside the element that shows `drawHint`:

```svelte
  {#if levelChoice}
    <button class="level-chip" title="This line starts where two passages overlap. Move it to the other level." onclick={flipLevel}>Other level</button>
  {/if}
```

Style (in the component's `<style>`), matching the hint's placement:

```css
  .level-chip { position: absolute; left: 50%; bottom: 56px; transform: translateX(-50%); z-index: 3; }
```

- [ ] **Step 4: Static checks.** `cd apps/web && npx svelte-check` → 0 errors; `npm test` → pass.

- [ ] **Step 5: Live check.** Start the dev server with `preview_start` (`.claude/launch.json` entry for `apps/web`, `npm run dev`, port 5173; vite rebuilds the wasm via `scripts/vite-plugin-cave-wasm.mjs`). Load a cave with overlapping levels (use the Task 2 geometry typed into the notebook as a new cave if the demo has no crossing). Snap to Top view, pick the wall tool, draw across the crossing along the upper passage, then run through each Acceptance Criterion, with `read_console_messages` showing no errors. Screenshot the glow + chip and the after-flip state.

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/lib/viewport/cave-view.ts apps/web/src/components/Viewport3D.svelte
git commit -m "map: glow the passage a line took, Other level flip, fix hopped lines"
```

---

### Task 7: Finish

**Goal:** Repo checks pass, index regenerated, spec marked built, deployed per the standing rule.

**Files:**
- Modify: `docs/CODE-INDEX.md` (regenerated)
- Modify: `docs/superpowers/specs/2026-10-27-overlapping-levels-sketch-design.md` (Status line + "As built" note listing the three deviations above)

**Acceptance Criteria:**
- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `python3 tools/make_code_index.py --check`, `npx svelte-check` all pass.
- [ ] Spec status reads "built" with the As-built deviations.
- [ ] Branch merged to main and the hosted copy updated per the republish rule (ask Nathan before the CT 209 step).

**Verify:** the five commands above → all pass

**Steps:**

- [ ] **Step 1:** `python3 tools/make_code_index.py`, then run the five checks.
- [ ] **Step 2:** Edit the spec's Status line to `Status: built <date>.` and add an "As built" section with the three deviations from this plan's header.
- [ ] **Step 3: Commit**

```bash
git add docs/CODE-INDEX.md docs/superpowers/specs/2026-10-27-overlapping-levels-sketch-design.md
git commit -m "docs: overlapping-levels sketching built; code index"
```

- [ ] **Step 4:** Merge to main and republish the hosted copy (merge, web.yml build, backup + unpack on CT 209 — needs Nathan's yes for the server step).
