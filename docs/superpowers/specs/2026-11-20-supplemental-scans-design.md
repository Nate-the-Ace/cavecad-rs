# CaveCAD Capture: supplemental scans

Status: design approved by Nathan, 2026-11-20. Extends `2026-11-17-lidar-room-capture-design.md`
(section "Station cards" and build step 4). Branch `lidar-capture`, app `apps/capture-ios`, core
`crates/cave-capture`.

## Goal

Scan a station, see where the scan has gaps, and scan more on the same visit, with the new passes tied into
the first automatically. Keep the phone's resources for scanning.

## Decisions (Nathan, 2026-11-20)

- Supplemental scans happen on the **same visit**, minutes after the first scan, cards still in place.
- One **file per station**: each pass is added into the station's shot file.
- During a supplemental pass the earlier scan shows as a **grey see-through low-poly ghost with red hole
  edges**.
- Card work **backs off once cards settle**; live markers stay.
- Line-up: **keep ARKit tracking running between passes** (exact, free); after a restart, **line up from the
  cards**. No saved ARKit world map.
- Hand scans are the normal case; on-station gimbal scans stay supported. Both work the same here.

## Flow

1. Capture opens on a station. If the cave already has a shot over that station (newest first), the screen
   says "Adding to the scan from 10:52" and Start reads **Scan more**. To start over, delete the old shot
   (swipe in Saved shots); there is no separate "new shot" button.
2. Stop saves the pass but **does not end the ARKit session**. It ends only on leaving the Capture screen or when the
   app closes. After Stop the
   screen offers **Scan more** straight away.
3. Scan more shows the ghost, runs a pass, Stop rewrites the station's file.

## Storage (one file per station)

- **Same tracking (normal):** ARKit's mesh already holds the first pass, refined by the second. The rewrite
  stores ARKit's whole current mesh, the photos of every pass (earlier ones copied from the old file), and the
  cards.
- **Tracking lost (app was closed):** the new pass is lined up by cards; its mesh, photos' poses and cards are
  transformed into the first pass's frame and **added** to the old mesh (overlapping triangles allowed, as the
  main spec already allows).
- **Passes recorded:** `ShotHeader` gains `#[serde(default)] passes: Vec<Pass>`; each `Pass` has
  `captured_at_ms`, `aligned_by` (`"first"`, `"same_session"`, `"cards"`), the `transform` applied (4x4,
  identity unless cards), `keyframes` (index range into the shot's keyframes) and its own `cards`. A file with
  no `passes` reads as one pass. The format number stays 1 (additive field).
- The top-level `cards` holds the latest estimate for each card in the first pass's frame.
- **Safe rewrite:** the new bundle is built from the old file plus the pass in the Rust core
  (`cave_capture::add_pass`), written to a temporary file, then moved over the old one. A failed save leaves
  the old file untouched and says so.
- Photo limit (`maxKeyframes`) counts per pass.

## The ghost

- **Built once per pass, off the main thread, in the core:** `cave_capture::ghost(mesh, cell_m) -> Ghost`
  simplifies the shot mesh by vertex clustering on a `cell_m` grid (default 0.05 m), giving roughly 5-20 k
  triangles, and returns the **hole outlines**: boundary edges (used by one triangle) joined into loops or
  chains; outlines shorter than 0.15 m are dropped.
- **Drawn** in the live view as one unlit light-grey see-through mesh and red line segments. ARKit's live
  mesh draws on top as now. The ghost does not update during the pass.
- **Where it sits:** with tracking kept, at identity (same frame). After a restart it stays hidden until the
  cards line it up; the screen says "Look at two cards to show the earlier scan."

## Line-up by cards, and the check

- A card is **settled** at 8 or more kept sightings with typical spread under 1 cm.
- **Fallback:** once the new pass has settled CURRENT plus FORWARD or REAR, the gravity-aligned turn (yaw) and
  shift that best map those cards onto the same cards in the file give the transform (least squares over the
  matched cards; 2 cards determine it, 3 over-determine it). If any matched card misfits by more than 5 cm,
  the line-up is refused: "FORWARD doesn't match where it was (12 cm). Put it back, or carry on without the
  earlier scan." The pass then saves as its own geometry at the compass placement, flagged in the log.
- **Check with tracking kept:** each card seen again is compared with its position in the file; more than
  5 cm off warns on screen ("CURRENT is 7 cm from where it was: card moved, or tracking slipped?") and in the
  log. Nothing is changed automatically.
- ICP fine matching (main spec, build step 4) refines card line-ups later; not in this build.

## Keeping it light

- **Card detection backs off:** while any seen card is unsettled, 3 per second at full resolution, 2 threads
  (as now). When all seen cards are settled: one every 2 s, `quad_decimate` 2, 1 thread. A new card is still
  found; past about 3 m it may need a closer look at half resolution.
- **Mesh passes** meet only unsettled cards' sight lines; Stop still meets all of them with the final mesh.
- **Ghost** built once, drawn low poly and unlit.
- **Log** per pass: detector state changes and time per run, ghost build time and triangle count, ARKit frame
  rate (frames per second over each 5 s summary).

## Pieces

| Where | What |
|---|---|
| `crates/cave-capture` | `Pass` in the header; `ghost()` (cluster simplify + hole outlines); `cards_transform()` (yaw + shift from matched cards, misfits); `add_pass()` (old bundle + new pass to new bundle, transforming a card-lined pass) |
| `apps/capture-ios/rust` | FFI doors for `ghost`, `cards_transform`, `add_pass` |
| `ShotRecorder.swift` | session kept across passes; parent shot; pass state; backoff; check; save through `add_pass` |
| `CaptureView.swift` | "Scan more", ghost and hole lines in the AR view, warnings |
| `CardTracking.swift` | `settled` per track; transform of a pass's cards |
| `ShotFiles.swift` | parent shot lookup by station; atomic replace |

## Errors

- Old file unreadable: say so, offer to start a new shot instead (the old file is kept).
- Save fails: old file untouched; message names the reason.
- Cards misfit: refuse the card line-up (above); never bend a pass to fit.
- Session lost mid-pass (interruption that ARKit cannot recover): the pass is kept up to that point and
  saved like a card-lined pass if cards allow, else as its own geometry, flagged.

## Testing

- Core: `ghost` on a known mesh (a box with one face missing: one hole outline of the right length; a
  closed box: none; cell size bounds the triangle count); `cards_transform` recovers a known yaw and shift
  within 1 mm / 0.01 deg from 2 and 3 cards, and reports a moved card as a misfit; `add_pass` keeps every old
  keyframe and photo byte-for-byte, appends the new ones, transforms a card pass, and an old header without
  `passes` still decodes.
- App: settled rule; backoff switching; parent lookup by station; atomic replace leaves the old file on a
  failed write; simulator tests + generic iOS build.
- On the phone: scan, Stop, Scan more over a gap; the log shows ghost size, backoff and frame rate; the
  saved file opens in the viewer with both passes.

## Not in this version

Saved ARKit world maps; supplemental scans on a later trip (cards will make this possible, later); ICP
refinement; removing a single pass from a file; remeshing.
