# Station cards: detection test build (brief)

Status: ready to build, 2026-11-20. Nathan has the three cards printed and laminated (4 in squares, glyph
76 mm). Spec: `docs/superpowers/specs/2026-11-17-lidar-room-capture-design.md`, section "Station cards".
Branch `lidar-capture`, worktree `~/Documents/github/cavecad-rs-lidar`. Read
`apps/capture-ios/CAVECAD-CONTRACT.md` and the memory note `cavecad-capture-lidar` first.

## Goal of this build

See real detections from the rig before anything depends on them. Detect and mark only; placement still
comes from the compass. Placement from cards is the next build, after Nathan's test.

## What to build

1. **Detector:** vendor AprilTag (github.com/AprilRobotics/apriltag, BSD-2) into
   `apps/capture-ios/Vendor/apriltag`: only the core sources and `tag16h5`. Compile it in the app target
   (C sources + a bridging header or a small module map). Keep its LICENSE file. Accept ONLY ids 2, 12, 26
   with hamming 0 (CURRENT, FORWARD, REAR, see `apps/capture-ios/cards/make_cards.py`); everything else is
   noise (the range test saw stray ids 5, 14, 15, 27).
2. **When:** during capture, on a serial background queue, at most one detection in flight, about 3 per
   second. Use the frame's luma plane (`capturedImage` plane 0) directly, no JPEG. Try ARKit's highest
   video format that still supports `sceneReconstruction` (4K on the 17 Pro Max if offered) because range
   doubles with resolution; log which format ran.
3. **3D point:** the tag's centre pixel → depth from `sceneDepth` (scaled to the depth map) or a raycast
   against the mesh; unproject with the frame's intrinsics and camera transform to the capture frame. Also
   record the tag's 4 corners in 3D when depth exists at them (gives the card's facing; later use).
4. **Track:** per card, keep the running median position, sighting count, nearest/farthest distance, and
   the decision margin from the detector.
5. **Show:** a coloured marker with the card's word floating at its 3D point in the live view (RealityKit
   anchor or a SwiftUI overlay projected each frame). No new buttons.
6. **Log:** first sighting of each card, then a summary every 5 s and at Stop: id, word, median xyz,
   distance, count, spread (how much its position wandered, mm), margin.
7. **Save:** `ShotHeader` gains `#[serde(default)] cards: Vec<CardSeen { id, word, point: [f64;3],
   corners: Vec<[f64;3]>, sightings: u32, spread_m: f64 }>` (Rust, with doc lines and a decode test of an
   old header without it). Format stays 1.
8. **Viewer:** draw seen cards in ShotView as labelled spheres at their points (relative to the station,
   like the room).

## Test (Nathan, at home)

Cards around a room at 1, 2, 3, 4, 5 m, some face-on, some at 45° and 60°, one flat on the floor. Take a
shot. Then `apps/capture-ios/pull-log.sh 00008150-0011281122D2401C` and read: which cards were seen, from
how far, how stable (spread), and whether any were missed. Report that to Nathan in plain words.

## Rules carried from today

Subagent-driven with spec + quality review; tests first for the pure parts (Rust `CardSeen`, the
unprojection math with a known camera, the id filter); simulator tests + generic iOS build; install with
`apps/capture-ios/install.sh 2ANLK72UUS 00008150-0011281122D2401C`; confirm on the phone through the log,
not by assumption.
