# Sketching over overlapping levels

Status: design approved 2026-10-27, not built.

## Problem

Where two passages on different levels overlap in plan, map linework drawn over one of them
hops to the other. `anchor_stroke` (`crates/cave-doc/src/anchored.rs`) anchors **each point
independently** to the leg nearest it **in flat 2D**. A wall traced along the upper passage
anchors to whichever leg is closer in plan, so one stroke can end up half on the upper passage
and half on the lower. Consequences:

- After an adjustment (loop closure, edited shots) the stroke tears or bends toward the wrong
  passage, because its points follow different legs.
- `map_lines_3d` lifts each point to the elevation of the leg it hangs on, so in 3D the line
  zig-zags between levels.
- Trip stamping and the trip filter inherit the wrong passage.

Profile and section sketches are not affected: a profile band is one run and a section is one
station.

## Decision

Approach C: the stroke follows the passage it started on automatically, the attached passage
glows, one tap flips it to the other level when the start was ambiguous, and a repair command
re-attaches lines already drawn.

## 1. Engine: anchor the stroke as one path

`crates/cave-doc/src/anchored.rs`

- `Frame` gains `za: f64` and `zb: f64` (elevation of each end). Frames are transient (built
  per call by `map_frames`), so this is not a file-format change. `profile_frames` fills them
  with 0; the profile path does not use them.
- New `anchor_stroke_path(points, frames, widths, pin: Option<usize>)`:
  - Leg graph: two frames are neighbours when they share a station id. Hop distance between
    frames by breadth-first search, capped (beyond the cap counts as "unconnected").
  - Per point cost: plan distance to the leg (the existing `distance_to_segment`).
  - Switch cost between consecutive points: 0 for the same leg; small for neighbours; grows
    with hop distance; large for unconnected; plus a penalty per metre of difference in
    elevation at the two legs' closest points.
  - Best chain over all points by dynamic programming (Viterbi). Only legs within a search
    radius of each point are candidates, so cost stays small on big caves.
  - `pin`: when set, the first point must use that frame.
  - Output is the same `AnchoredPath` as today: stored data unchanged, old drawings load as is.
- Weights are named constants with a comment on what each trades off; tune against the test
  fixture, not by feel.
- `anchor_stroke` (per point) stays for profile and section.

## 2. Use the path rule everywhere a map line is attached

- `cave_draw::map_line` and `cave_draw::edit_stroke` (map scope) call `anchor_stroke_path`.
- Extending a line (continuing the last stroke) pins to the last leg the existing line used.
- `map_frames` (`crates/cave-scene/src/maplines.rs`) fills `za`/`zb` from the solved positions.
- Trip stamping for map lines and symbols takes the trip from the legs the stroke anchored to,
  not from the flat-nearest station.

## 3. Glow and tap to flip (web)

`apps/web/src/panels/SketchPane.svelte` and the map drawing layer.

- While drawing, and for a few seconds after the stroke lands, the legs it is attached to are
  highlighted.
- Ambiguous start: another unconnected passage has a leg within a few metres (plan) of the
  first point. Then a small "Other level" chip shows by the stroke. Tapping it, or tapping the
  other passage, re-anchors the same stroke with `pin` set to that passage's nearest leg, as its
  own edit: one undo returns the stroke to its first attachment, a second undo removes it.
- New wasm entry: re-anchor a map line with a pinned leg (`reanchor_line(id, a, b)`), going
  through the one edit path.

## 4. Repair existing lines

- Engine: `hopped_lines(doc)` lists map lines whose anchored legs include two that are
  unconnected (beyond the hop cap) or differ in elevation beyond a threshold.
- `reanchor(doc, line)`: place the line where it is drawn now, re-anchor with
  `anchor_stroke_path` pinned to the leg its first point used, emit an edit.
- Menu command "Re-attach lines to their level": shows how many it found, fixes them all as
  one undoable edit.

## 5. Tests

Fixture in Rust (`cave-doc` / `cave-scene` tests): two passages crossing in plan, about 8 m
apart vertically, joined far away so they are in one survey.

- A line traced along the upper passage across the crossing anchors only to upper legs.
- Moving the lower passage (adjustment) does not move that line.
- Same line started at the crossing with `pin` on the lower passage anchors only to lower legs.
- On a single-level cave the new rule gives the same anchoring as `anchor_stroke` (no
  regression for ordinary drawing).
- `hopped_lines` finds a deliberately hopped line; `reanchor` fixes it and keeps its drawn
  shape within tolerance.
- `map_lines_3d` of the fixed line stays at the upper passage's elevation.
- Web: `sketch.check.ts` covers the chip appearing only for an ambiguous start and the flip.

## Out of scope

- A manual "drawing on this level" picker (approach B).
- Fading or hiding other levels while drawing.
- Symbols placed by click (single point): they use the flat-nearest rule today and keep it;
  revisit if they misfile.
