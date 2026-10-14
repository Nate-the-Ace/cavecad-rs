# Object snap (osnap) design

Date: 2026-10-14. Branch: `object-snap`. Status: design agreed with Nathan in conversation; not yet built.

## Goal

Every point a person picks on the drawing can snap to geometry the way AutoCAD's object snap does:
endpoints, midpoints, circle centres and quadrants, intersections, perpendiculars, tangents,
extensions, and alignment tracking. Behaviour follows AutoCAD (Autodesk's OSNAP and AutoSnap help)
except where this app has no command line (see "Not in scope").

Decisions made with Nathan:

- Scope: **every point pick** goes through one engine (draw strokes, text, leaders, Copy/Paste base
  point, move, vertex edit). `nearestVertex` in `leader-edit.ts` is replaced by it.
- Modes: the **full AutoCAD set** plus a cave-specific Station snap.
- Controls: **AutoCAD-style minus typing**. The Shift+right-click one-pick override menu is a core
  feature, as are Temporary Track Point and Extension.

## Architecture

New crate `crates/cave-snap`: pure Rust, wasm32-safe (no file, thread, clock or entropy), one job:
given the cursor and the projected snappable geometry, return ranked snap candidates. Exposed to the
web app through `cave-wasm`. TypeScript owns the camera, the overlay drawing and the input events.

### Space

The map is a 3D scene, so picks are resolved in **screen space**. Per pointer move, TS hands the
crate the visible snappable things, projected through the camera (`project()` already exists).
Two kinds of mode:

- **World-defined points** (Endpoint, Midpoint, Center, Geometric Center, Quadrant, Insertion, Node,
  Station): computed in world space from the source geometry (arc midpoint by arc length, quadrants
  at 0/90/180/270 degrees of the arc's own circle), then projected. A perspective camera turns a
  circle into an ellipse on screen, so these must not be derived from the projected curve.
- **Proximity modes** (Nearest, Perpendicular, Tangent, Intersection, Apparent/Extended
  Intersection, Parallel, Extension crossings): computed on the projected curves, flattened to
  polylines at 0.25 px tolerance (arcs and splines via `cave-geom`'s `Arc` and `Path`). The result
  is converted back to a world point by the parameter along the source (id, segment, t). Exact
  endpoint tangents of arcs and splines (for Extension) come from `cave-geom` directly.

Every candidate carries the source thing's stable `cave_id::Id` (never a list position), the segment
index and parameter, the mode, the screen position, and the world point to use.

### Inputs

TS builds a `SnapScene` per pick: polylines (with bulge arcs), splines (fit points), circles from
shaped lines, stations, symbol insertion points, text and leader points. It also passes the active
view's drawing plane (Top/Bottom, Profile, Section, or none for an oblique camera), the previous
point of the pick in progress (for Perpendicular, Tangent, Parallel, Extension), and the session
state below. Things on hidden layers and the thing being edited are excluded by TS, as
`nearestVertex(selfId)` does today.

### API sketch

    snap(scene, cursor_px, settings, session) -> SnapResult
      SnapResult { best: Option<Candidate>, others: Vec<Candidate>, guides: Vec<Guide> }
      Candidate  { mode, screen: [f32;2], world: [f64;3], source: Option<(Id, seg, t)>, label }
      Guide      { kind: Extension|Track|Polar, from_px, dir_or_arc, label }

`session` is the explicit state of a pick in progress (acquired points, Temporary Track Points, the
From base, the one-pick override). It is passed in and returned, never held globally, so the crate
stays pure and testable.

### Ranking (as AutoCAD)

Within the aperture box: point modes beat relational modes; closest on screen wins within a tier;
Nearest is last. Ties break by mode order then source id, so results are deterministic. Tab cycles
`others`, in that order, then back to `best`.

## Modes

| Group | Modes |
|---|---|
| Point | Endpoint, Midpoint (each segment of a polyline; whole curve for a spline), Center, Geometric Center (closed paths), Quadrant, Insertion, Node, **Station** |
| Relational | Intersection, Apparent Intersection, Extended Intersection, Extension, Perpendicular, Tangent, Nearest, Parallel |
| Reference | From, Mid Between 2 Points, Temporary Track Point, running tracking |

Intersection and Apparent Intersection: in screen space every crossing is "apparent". Plain
Intersection keeps only crossings where the two world points are within a small distance of each
other in the drawing plane (or both lie on the same Z level in an oblique view); Apparent keeps all.
Extended Intersection extends both curves along their natural paths and is override-only, as in
AutoCAD.

## Controls

### Running snaps

A status-bar **Snap** button toggles them; F3 does too. A small arrow opens a checklist popover.
Defaults on: Endpoint, Midpoint, Center, Quadrant, Intersection, Station, Insertion. Settings
(modes on, marker, tooltip, magnet, aperture size, polar increments, tracking on) are remembered on
this machine (localStorage, every access in try/catch).

### One-pick override menu (Shift+right-click)

- Opens at the cursor whenever a tool is waiting for a point. Ctrl+right-click opens the same menu.
  Long-press (pen, touch) and a status-bar button open it too.
- Choosing an entry applies to the next pick only and searches only that mode.
- A miss returns nothing and the tool keeps waiting (AutoCAD override behaviour), so a wrong point is
  never silently accepted.
- Plain right-click keeps its existing meaning (for example dropping the paste ghost).
- Entries: From, Temporary Track Point, Mid Between 2 Points; Endpoint, Midpoint, Center, Geometric
  Center, Quadrant, Insertion, Node, Station; Intersection, Apparent Intersection, Extension,
  Perpendicular, Tangent, Parallel, Nearest; None (one free pick); Snap Settings. Each entry shows
  its marker glyph.
- A web menu built by us; no platform menu.

### Feedback (AutoSnap)

Marker shape per mode (square Endpoint, triangle Midpoint, circle Center, diamond Quadrant, X
Intersection, hourglass Nearest, right-angle mark Perpendicular, and so on), a tooltip naming the
mode and the thing, a magnet that locks the crosshair to the snap point, and an aperture box
(default 10 px, the reach used today). All four are settings. Drawn on the existing overlay layer.

### Extension

Hover an endpoint about 0.3 s to acquire it (`+` marker). A dotted guide continues the line in its
direction; an arc continues as its circle; a spline follows its end tangent. The tooltip reads
`Extension: 3.20 < 45°`. Pick anywhere on the guide. Two acquired points give an Extended
Intersection where their guides cross; a guide crossing a real line gives an Intersection.
Hovering an acquired point again releases it.

### Temporary Track Point and tracking

Override-menu entry. Pick a point (it may use any snap); it is marked `+`. The cursor then tracks
horizontal, vertical and polar-increment lines through it (default 15, 30, 45, 90 degrees; a
setting). Chaining several adds guide lines and the pick can land where two cross. Escape drops them.
Running tracking (hover to acquire any snap point, guides come off it) uses the same machinery.

**Distance field.** While a guide is active, typing a number opens a small field beside the cursor;
Enter places the pick that far along the guide. This is part of a pick, not a command line.

**3D limit.** Angles and distances are meaningful only in the drawing plane, so tracking and the
distance field are on in Top, Bottom, Profile and Section. From an oblique camera, Extension still
shows and picks along its screen line, tracking angles are off, and the toolbar says why. This
matches the existing rule that map strokes are drawn only from Top or Bottom.

## What is stored

Nothing new. A snap only chooses a point. A leader tip that lands on a vertex still records its
fastening as today (`index=id#vertex`, DESIGN.md "Fastening to objects"). Settings are
per-machine view state, not cave data.

## Constraints from the repo

- Core crate: no I/O, wasm32 build line in CLAUDE.md gains `-p cave-snap`.
- Never log or store entrance coordinates. Snap results carry world points; the crate and the TS
  side log nothing about them (use counts and modes only).
- Plumbline Pit and Stairstep are checked alongside Pitfall Cave: snapping must not invent a
  bearing, and a vertical passage's plan is a point.
- No generic CAD commands: the menu only helps pick points for our own tools.
- New public items get one-line `///` docs; run `python3 tools/make_code_index.py`.

## Testing

- `cave-snap` unit tests, hand-solved numbers, one group per mode: endpoint/midpoint on lines,
  bulge arcs and splines; arc midpoint by length; quadrants of a rotated arc; circle-circle,
  line-arc, line-line, spline-line intersections incl. tangency and near-parallel; perpendicular and
  tangent from a point to line, arc and circle (two tangents: nearest wins); parallel; extension of
  line, arc, spline; extension-extension crossing.
- Ranking: determinism (same input twice, shuffled input order), tier order, Tab cycle order, aperture
  edge, ties.
- Session: Temporary Track Point chains, From, Mid Between 2 Points, override miss returns nothing.
- Projection: a circle under a perspective camera still reports its true Center and Quadrants.
- Fixtures: Pitfall Cave, Plumbline Pit (plan degenerates), Stairstep.
- TS: `snapPick` migration tests proving leader fastening and the Copy base point behave as before.
- In the browser: draw a Wall to a station, a midpoint, and an intersection; Shift+right-click
  menu; Tab; Extension; Temporary Track Point with typed distance, each in Top and in an oblique view.
- Checks before each commit, per CLAUDE.md (fmt, clippy -D warnings, test, wasm32 build, code index
  `--check`, `npx svelte-check`).

## Build order

1. `cave-snap`: types, point modes, aperture and ranking, tests.
2. Proximity modes: nearest, perpendicular, tangent, parallel, intersection family, tests.
3. Session modes: extension, tracking and polar, Temporary Track Point, From, Mid Between 2 Points,
   tests.
4. `cave-wasm` bindings and the TS `snapPick`; migrate `nearestVertex` callers (leader tip, Copy
   base point, line editor); behaviour unchanged at this step.
5. UI: overlay markers/tooltip/magnet, status-bar button and popover, Shift+right-click menu, Tab,
   distance field, settings.
6. Draw tools, text, move and vertex edit onto `snapPick`; handbook page; DESIGN.md entry.

## Not in scope

- Typed overrides (END, MID, M2P) and any command line.
- Snapping to scan images or aerial imagery.
- 3D-viewer picking; snapping is for the 2D-style drawing picks only.
- Object snap in the Cross Section sketch pen (freehand strokes), revisited later.

## Open details to settle during build

- The exact distance at which plain Intersection treats two crossings as one point.
- Whether Station snap needs its own marker or reuses Insertion's.
