# CaveCAD Capture: gimbal-driven LiDAR room capture

Date: 2026-11-17. Status: design approved by Nathan, not built.

An iPhone app that sits in the Osmo Mobile 7, turns the phone through a room with the gimbal while the
phone's LiDAR captures it, and stitches several shots of the same room into one 3D room that appears in
CaveCAD (app and server) with its walls drawn on the map.

Vocabulary (carried from the QCAD era, still binding): a LiDAR file is a **capture**, never a scan. "Scan"
means a scanned notebook page.

## Decisions (Nathan, 2026-11-17)

- **A separate app**, "CaveCAD Capture", with its own icon, not a tab inside CaveCAD. Its data must
  integrate **directly** into CaveCAD on the app and on the server: no import step, no file to move by hand.
- **Stop between shots.** Each shot is its own capture session. The rig can be switched off, carried,
  climbed with. Shots are lined up afterwards by matching their overlap.
- **Every shot sits over a survey station.** The station gives the shot's rough place in the cave, so
  matching starts close and the room lands in the right place on the map.
- **First version goes as far as walls on the map:** a stitched, coloured 3D room in CaveCAD's 3D view, a
  GLB/PLY export, and the room's walls sliced into the plan drawing.

## Revision, 2026-11-20 (Nathan, after the first shots on the phone)

- **Capture's only controls are the station chooser and Start/Stop.** The forward-station pick, the
  crosshair heading tap and the height field are gone ("excessive").
- **Heading comes from the phone's compass** (ARKit `.gravityAndHeading`). This replaces the "No compass"
  limit below. The OM7 clamp's magnet spoiled the X1's compass, so every shot records the compass's
  accuracy and the app warns (does not block) when it is worse than 20°.
- **Height is fixed at the gimbal's pivot, 0.30 m** above the surface its tripod stands on (measured in the
  2026-11-16 floor sweep, `tools/cave-osmo/FINDINGS.md`). Same gimbal, so no field.
- **Picking stations and orienting a capture by hand belong in CaveCAD**, in a LiDAR suite of tools planned
  later (its own spec): where a compass heading is wrong or a shot needs moving, it is fixed there, not on
  the phone. That includes declination for a shot whose compass gave magnetic north (no location fix): the
  shot keeps the raw compass state (`true_north`, `accuracy_deg`, taken when ARKit aligned to north) and
  the suite corrects it from the cave's location (`cave-geomag`). Until then such shots are placed as
  they are and labelled, never silently. Shots taken with a heading tap (two exist) still place as before.

## Station cards (Nathan, 2026-11-20)

Three reusable, business-card-sized cards (85 × 55 mm), each one simplified glyph plus a printed word:
**CURRENT**, **FORWARD** and **REAR**. The same three cards are used at every station; nothing is printed
per station.

- **Placing:** CURRENT on the mark of the station chosen in the app, FORWARD on the next station, REAR on
  the previous one. Propped up facing the rig where possible (a card lying flat seen at a shallow angle from
  metres away will not read).
- **Which stations they mean:** the app already knows: the chosen station is CURRENT; FORWARD and REAR are
  its neighbours along the surveyed legs. Only where that is ambiguous (a junction, an end) does the app ask,
  so the controls stay the station chooser and Start/Stop.
- **Marking while scanning:** the app looks for the glyphs in the camera frames during the sweep, finds each
  card's 3D point with the LiDAR depth, and marks it in the live view. Nothing to tap.
- **What they give:** CURRENT puts the station point where it really is, so the rig need not stand exactly
  on the mark and its height stops mattering. CURRENT plus FORWARD or REAR gives the shot's heading from the
  survey itself; the compass becomes the fallback (it is still recorded). All three over-determine it and
  show a bad card placement as a misfit.
- **Recorded in the shot:** each card seen, its glyph, its 3D point in the capture frame, and how many frames
  saw it. Placement prefers cards, then the compass; the shot says which was used.
- **Limits:** cards need the rig's light on them; a ~45 mm glyph reads to about 4 m from the phone's camera
  frames. Glyph family and detector are chosen in the plan (a fiducial tag family with a small, permissively
  licensed detector, so no network or heavy library); the printable card sheet ships with the app and,
  later, from CaveCAD.

## Devices without LiDAR (Nathan, 2026-11-21)

On an iPhone or iPad without LiDAR, Capture is a **viewer**: saved shots, the shot viewer, the overall view,
card chains and the station-card print sheet. No Capture button, no capture screen, no gimbal; nothing says
"no LiDAR" as an error. iPad gets a proper iPad layout (device family iPhone + iPad). Shots reach such a device
through the Files app (Capture's folder is shown there) until server upload exists.

## Card chaining (Nathan, 2026-11-21)

Shots at neighbouring stations are joined to each other on the phone through the station cards, with no
survey positions and no CaveCAD. At A3 the CURRENT card marks A3 and FORWARD marks A4; at A4 CURRENT marks A4
and REAR marks A3. Each shot measures both station points, so two shots share two points: with gravity known
and scale locked, that fixes the turn and shift between them (one shared point plus the compass is the weaker
fallback, flagged).

- **Neighbours:** from the survey legs when Capture has them, else from the labels (same prefix, number ±1:
  A3 → A4). A junction or a label that does not follow the pattern is not guessed: those shots stay unchained.
- **Chains:** neighbouring shots form a chain. A chain with any placed station is placed as a whole; a chain of
  only waiting stations holds together and drops into place when one of its stations arrives.
- **Check:** the LiDAR's own A3→A4 vector is compared with the survey leg when there is one; a misfit beyond
  max(10 cm, 3% of the leg) is flagged on both shots, never silently used.
- **Order of trust for a shot's place:** its CURRENT card + a chained neighbour > CURRENT card + compass >
  compass at the station. The shot records which was used.
- Derived, as all placement: no file is rewritten; the chain is worked out from the shots each time.

## Stations not on the server yet (Nathan, 2026-11-21)

A shot can be taken at a station the server has not seen yet (surveyed earlier on the same trip, with no
signal). Two ways in, both:

- **From CaveCAD on the same phone:** Capture reads CaveCAD's offline copy of the cave through the shared App
  Group folder, so stations surveyed in CaveCAD on this trip appear in the picker with their positions.
- **Typed:** an "Other station…" entry where the label is typed (paper notes, another device). The shot is
  saved **waiting for station**: its compass reading and cards are kept, its position is not known.

A waiting shot is placed by itself once its station label appears in the cave Capture has (shared folder,
"Get latest", or a later download): placement is derived, so nothing is redone. Labels are matched as typed,
ignoring case and spaces. Waiting shots are listed as such and upload with the rest once uploads exist
(build-order step 5).

## Hard limits (stated, not solved)

- **Range.** iPhone LiDAR is good to about 5 m. Bigger rooms need more shots. The app shows where nothing
  came back so a missed wall is visible in the cave, not at home.
- **Light.** LiDAR measures depth in the dark, but ARKit tracks the phone from the camera picture plus the
  motion sensors. A dark cave loses tracking. The rig carries a video light aimed with the phone; the app
  refuses to start a shot while ARKit reports too little light and pauses the gimbal while tracking is
  limited.
- **No compass** (superseded by the 2026-11-20 revision above). The OM7 clamp's magnet spoiled the X1's compass and will spoil the phone's. Heading comes
  from the surveyor (see "A shot"), never from the magnetometer.
- **Scale is never free.** ARKit LiDAR captures are metric; matching solves turn and shift only. A free scale
  would soak registration error into a wrong cave length (the datum-trap family).

## Approaches considered

1. **Phone captures, a shared Rust crate stitches (chosen).** ARKit builds each shot's mesh live. Matching,
   colour, export and wall slicing live in a Rust crate that runs on the phone, the desktop and in the
   browser (WASM). The room is stitched in the cave, and CaveCAD can rebuild it anywhere when the survey
   changes.
2. All on the phone in Swift. Fastest start, but CaveCAD could not rebuild a room when stations move.
3. Phone records raw depth frames, desktop rebuilds everything. Best quality, very large files, nothing to
   check until home.

## A shot, in the cave

1. Set the rig (tripod or rock) over a station; pick the station in the app.
2. Put the crosshair on the forward station (any station the survey already places) and tap. That ray plus
   the two stations' surveyed positions gives the shot's heading. A second tap on another station is allowed
   and checked against the first (the app says if they disagree by more than 3°).
3. Press Capture. The gimbal runs the capture pattern as target-mode lists (existing `cave-osmo` target
   mode: at most 15 targets a list, every list leaves and returns home, ~15°/s moves). ARKit scene
   reconstruction runs the whole time. Moves are slower than the X1 dome so the camera keeps sharp frames.
4. Live view: the growing mesh, with holes and out-of-range areas painted. An optional **Fill** pass aims
   the gimbal at the biggest holes (aim solved by the existing gimbal kinematics).
5. Tracking limited or lost: the gimbal holds until ARKit reports normal again; lost for more than 10 s ends
   the shot with what it has, marked "tracking lost".
6. Stop. The shot is saved on the phone (see "Data").

The phone in the clamp is offset from the pan axis, so the shot is not a pure rotation; ARKit handles that.

## Lining shots up (the `cave-capture` crate)

All shots are in ARKit's gravity-aligned, metric, Y-up frame. Survey frame = `(x, -z, y)` of the capture.

- **Seed:** each shot is placed by its station's position and its heading tap. Unknowns left: yaw and
  translation (4 numbers per shot).
- **Fine match:** point-to-plane ICP between overlapping shots on downsampled meshes, solving yaw + shift
  only. Then all shots of a room are adjusted together (pose graph: each pairwise match an edge, each
  station seed a weak edge), so one bad pair cannot drag the room.
- **Quality per shot:** overlap fraction and match residual give a status: *placed* (seed only, no usable
  overlap), *matched*, or *poor match* (residual or shift from the seed over limits; limits are constants in
  the crate, tuned on the home fixture). A shot that will not match stays at its seed, flagged. It is never
  bent to fit.
- **Merge:** shots are kept as separate meshes in the room's frame (no remeshing in v1); overlapping
  triangles are allowed. Colour per vertex from the nearest-in-angle keyframe photo.
- **Walls and sections (Nathan, 2026-11-20: plan AND profile, plus cross-sections):** the merged room is
  sliced into wall polylines in survey coordinates for each view CaveCAD draws:
  - *Plan walls:* a horizontal plane at each shot station's height (plus any heights the person adds).
  - *Profile walls:* a vertical plane along the survey leg (or the profile's projection direction), giving
    the ceiling and floor lines for the Profile view.
  - *Cross-sections:* a vertical plane across the passage at a station or any point the person picks,
    giving the passage outline for the Cross Section view.
  Each slice is closed or open as the scan allows; gaps stay gaps (never invented).
- **Export:** GLB (coloured mesh) and PLY (points with colour), in survey coordinates, entrance coordinates
  never written unless the cave's own export rules allow it.

The crate is pure (no files, clock or threads, per the repo rules), builds for wasm32, and takes shots as
bytes in, room as data out.

## Data and integration

**What is source and what is derived** (the repo's rule: store only what a person did):

- **Source, in the cave's edit history:** one *capture room* record per room: its id, name, trip, and for
  each shot the station id, heading tap(s), capture file id (SHA-256), capture time and status. Adding,
  removing or renaming a shot is one edit. Small; syncs through the normal server-cave edit history and live
  rooms like any other edit.
- **Source, as files:** each shot's capture bundle (mesh, keyframe JPEGs, their camera poses, gravity, ARKit
  tracking log). Stored like scanned pages are today (`apps/server/src/scans.ts` pattern): a file named by
  its SHA-256 per cave, stored once, deleted when no room uses it; owners and editors add, watchers read.
  New table `cave_captures`, larger size limit than scans, resumable chunked upload because shots are tens
  to a few hundred MB.
- **Derived, never stored in the cave:** the room's shot poses after matching, the merged coloured room, the
  walls. CaveCAD rebuilds them deterministically from the source plus the current station positions, and
  caches them per device. A survey change (loop closure, a corrected shot) moves the room and its walls by
  itself.

**The capture bundle format** (one file per shot, versioned): a small header (format version, shot id,
device model, ARKit version), the mesh (positions, normals, triangle indices, float32/uint32), keyframes
(JPEG bytes + 4×4 camera pose + intrinsics each), the heading ray(s) in capture frame. Read and written by
`cave-capture`, so the Swift app and CaveCAD agree by construction.

**Same phone, CaveCAD installed:** both apps share an App Group container.
- Capture reads the open cave from CaveCAD there, including stations surveyed earlier on the same trip that
  have not reached the server. Works with no signal.
- Capture writes finished shots and the room edit there; CaveCAD picks them up at once and sends them on with
  its own sync.

**Capture alone (CaveCAD not on the phone):** signs in with the same CaveCAD account (the server's existing
sign-in and desktop-style bearer handoff), downloads the cave before the trip, queues shots and the room edit
on the phone, uploads when it has signal. Nothing is lost if the app closes; an upload resumes where it
stopped.

**In CaveCAD (desktop, web, iPad):**
- Capture rooms list in a panel: room, its shots with status, a Rebuild button, Export GLB/PLY.
- The 3D view shows the room in place, with a toggle.
- **Draw walls** puts the slices on a *capture walls* layer. They are derived linework: regenerated when the
  room is rebuilt, trip-stamped like traced linework, not editable as such (copy to a sketch layer to edit
  by hand).

## Pieces

| Piece | Where | Job |
| --- | --- | --- |
| `crates/cave-capture` | new crate | bundle format, seed, ICP, pose graph, quality, colour, slice, export |
| CaveCAD Capture app | `apps/capture-ios` (new, SwiftUI) | ARKit session, live view, shot flow, App Group, upload queue |
| Rust bridge for the app | `apps/capture-ios/rust` | `cave-capture` + `cave-osmo` built as a static library with a C interface |
| Gimbal | `tools/cave-osmo` (existing) | target-mode lists, home-to-home; new capture pattern |
| Capture room record | `cave-doc` / notebook ops | the source record and its edits |
| Server | `apps/server/src/captures.ts` (new) | per-cave capture files, chunked upload, access rules |
| CaveCAD UI | `apps/web` | Capture rooms panel, 3D view layer, Draw walls |

The gimbal is driven from Rust (`cave-osmo` with the patched btleplug that finds the bonded OM7), so the app
has one gimbal implementation shared with the Mac tools, not a third copy in Swift.

## Errors

- No light / tracking limited: refuse start, or hold the gimbal, with a plain message.
- Gimbal disconnects mid-shot: the shot keeps what it has and is marked "gimbal lost"; the person can carry
  on by hand-turning the phone.
- Heading taps disagree: warn, keep the first, let the person retake.
- Station deleted from the survey later: the room keeps the shot and shows it as "station gone", seeded from
  its last match.
- Upload fails: queued, retried with backoff; the shot list shows "waiting to upload".
- Phone storage low: warn before a shot when less than 2 GB is free.

## Testing

- `cave-capture`: synthetic rooms (boxes, cylinders, a lumpy tube) cut into overlapping shots with known yaw
  and shift offsets; matching must recover them within 1 cm and 0.5°. Bad seeds beyond the capture range
  must come back *poor match*, not a wrong answer. Order independence of the room build.
- Bundle round trip, Swift writer to Rust reader, with a real shot.
- One real room at home (several shots) kept as a fixture in `crates/cave-fixture`.
- Server: capture upload, resume, dedupe, access (watcher refused), deletion when unused.
- On the phone, each phase checked on the iPhone 17 Pro Max with the OM7 before the next starts.

## Build order

1. **Capture app shell:** sign in, pick cave and station, one handheld shot saved as a bundle on the phone,
   viewable in the app.
2. **Station cards:** the three glyph cards, detection and 3D marking during the scan, placement from
   cards (compass as fallback), printable card sheet.
3. **Gimbal-driven shot:** capture pattern, live holes view, Fill pass, tracking and light holds.
4. **Multi-shot stitch on the phone:** `cave-capture` seed (cards first), ICP, pose graph, quality statuses.
5. **Into CaveCAD:** App Group hand-off, server capture storage and upload, capture room record, Capture
   rooms panel, 3D view layer, GLB/PLY export.
6. **Walls and sections from the capture:** plan walls, profile walls and cross-sections sliced from the
   merged room, as capture layers in the Map, Profile and Cross Section views.
7. **LiDAR suite in CaveCAD** (own spec, later): pick a shot's station, turn and move a capture by hand,
   and whatever else working with captures on the desktop needs.

## Not in this version

Remeshing overlapping shots into one surface; textured (not vertex-coloured) meshes; continuous capture
while walking; capture without a station; Android; other gimbals.
