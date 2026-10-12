# Design decisions

Recorded 2026-10-11. Update when a decision changes, with the date.

- **Own core.** No QCAD, no CADCraft. The old app is frozen at tag `qcad-final-2026-10-11`.
- **Chromium-first, local-first.** Caves live on the user's machine (File System Access API, OPFS
  working copy). Cloud providers (Drive, Dropbox, OneDrive, WebDAV) come later behind the same
  `cave-store::Store` trait. Tablets reach files through cloud APIs. Nothing uploads to our server.
- **UI.** Modern ribbon on Tauri (decided 2026-10-11): one web front end serves the desktop app and
  the hosted Chromium app, over the Rust engine (native in Tauri, WASM in the browser). Engine
  crates stay UI-independent. Frontend framework: undecided.
- **Console.** Read-only panel fed by `cave-log`; one log per cave file (`<cave>.log`).
- **Co-editing later, planned now.** Stable ids, operation log, source/derived split, atomic
  strokes, last-writer-wins fields, content-addressed blobs, schema version in the log. Phases:
  op log, async merge through shared folder, live relay, permissions.
- **Port order** (the QCAD-era suite, Expedition Planner cut, Handbook last): survey core, import
  and export, checks, draw panel, shelf, Feature Trace, Symbol Palette, Station Table, Entrance
  and basemap, Profile, Cross Section, Shaped Lines, Area Fill, Callout, Linetype Maker, Scans,
  Sheets, Legend and Package, housekeeping, 3D viewer, Notebook, Xref and Surface data, Handbook.

## Standing considerations (checked at every step)

Recorded 2026-10-11 at Nathan's request: these are on the road map and must be weighed by each preliminary step, even before they are built.

**Xrefs (external references)** - not built. A referenced cave stays a separate read-only `Document`; the scene builder composes several documents with a placement transform (no block copying, no layer renaming: ids are already unique). Needs: an Xref entity (store-relative path, overlay or attach, placement, auto-update), loading through `cave-store` with version tags for "update available", cycle detection for attach chains (port the old `CsXref.plan`), and a permission story for files outside the folder the browser was granted. Paths must be relative to the cave's folder: absolute paths mean nothing on the web or in cloud storage.

**Multi-user simultaneous editing** - foundations built (stable ids, one edit path, order-independent merge, per-editor undo, `validate`). Known gaps, to close in this order:
1. Every tool edits through `Document::edit`; nothing writes around it.
2. Convergence tests cover every kind of entity, and undo mixed with remote edits (today: stations only).
3. Shot order is a plain LWW number: add fractional ordering so concurrent inserts do not collide.
4. Whole-value LWW loses one side of a concurrent edit to the same field: fine for labels and strokes, wrong for long notes (needs a text type) and tag maps (per-key).
5. Per-trip file split, stored operation log, transport, presence (see the co-editing phases above).
6. Xrefs + editing: a changed referenced file must reload without clobbering local edits.

**Per-step checklist** (ask before calling a feature done): does it go through `Document::edit`? Is derived data deterministic and recomputed, never stored? Does it work when the cave is an xref (no global state, no assumption of one document)? Does it work on Plumbline Pit (vertical) as well as Pitfall? Does it avoid logging or storing entrance coordinates?

**Pen and tablet input** (Nathan, 2026-10-11: tablets are a target, so drawing can be "artsy" where it makes sense). Pressure only ever changes how a line is *drawn*, never where it is: surveyed coordinates stay exact. Built so far: a stroke (`cave_geom::Path`) can carry optional per-point `widths` (relative pen weight, 1.0 = the type's normal weight); they are validated, survive transforms and merges (stored whole with the stroke), and are absent for ordinary lines. Not built: the web input layer (Pointer Events: pressure, tilt, coalesced and predicted events, pen versus finger, eraser button, palm rejection), a `cave-ink` crate (pressure curve, smoothing, resampling, simplification; pure and testable with recorded strokes), a variable-width ribbon renderer, and mapping to DXF variable-width polylines. Where pressure makes sense: Feature Trace (walls, ledges), cross-section and sketch strokes, Shaped Lines accents, annotation ink. Where it does not: station placement, symbols, text, anything measured. Finger = pan, zoom and orbit; pen = draw. The type's line weight (importance) stays the base, pressure is a modifier, with a per-tool on/off.

**Profile panel** (2026-10-11): pick a run, see its extended elevation, sketch on it. Runs come from station names (`cave-profile`, ported from `CsProfile` and checked against the old output on all four test caves); the band is `(s, z)`: distance along the run (true measured plan length) and elevation. Sketches are ordinary `Feature` entities (`FeatureType = profile-sketch`, `FeatureRun = <run key>`, path in band coordinates, optional pen-pressure widths) created through `cave-draw` and `Document::edit`, so they undo, redo and will merge. They sit on no layer: there are no layers. Sketches are anchored to stations (`cave_doc::AnchoredPath`): each point is stored relative to the nearest pair of neighbouring stations (how far along that leg, and how many metres off it), so a sketch follows the survey when it is adjusted or edited: it stretches with a leg that gets longer, turns with a leg that turns, and keeps its sideways offsets in metres. A sketch whose stations are gone is counted as "unplaced" (and `validate` flags the dangling reference) rather than drawn in the wrong place. Known gaps: only name-derived parents for letter runs; no profile walls from splays (`bandWallRuns`); no eraser, smoothing, or stroke selection yet.

**3D passage tube** (2026-10-11): `cave-mesh`, ported from `CsMesh3d` and checked vertex for vertex (positions, normals, colours in both modes) against the old output on all four test caves. At each station the measured wall points (the four LRUD ticks and every splay tip) are projected into the plane perpendicular to the passage and sorted by angle; neighbouring rings are lofted with straight triangle strips. No smoothing, no inferred curvature. A junction gets one ring per leg along that leg; a shaft's frame is fixed (north is the reference) so it does not spin. Colour modes built: trip and depth. Known quirks kept on purpose, to match the old app: a splay at exactly the same angle as an LRUD tick is ignored (first point at an angle wins), and a station with fewer than three measured wall points draws no surface. Not ported yet: sketched cross-sections as rings (`sectionRing`), open-side gaps, the other colour modes (distance, size, date, closure, splay coverage, cover), the ghost of the raw survey, lead markers, and station outlines. Rendering is three.js with a headlamp, vertex colours and polygon offset so wall outlines stay visible.

**3D tube, second pass** (2026-10-11, Nathan: real caves have stations with no measurements, and the tube must not break or drop measured points). Two deliberate departures from the old app, both optional in code so the parity checks still run against the old behaviour (`LoftMode::Nearest`, no fill):
- *Full lofting* (`LoftMode::Full`, the app default): strips run between the union of both rings' angles, each ring read by straight interpolation between its neighbouring measured points, so EVERY measured LRUD tick and splay tip becomes a vertex. The old sampling merged clustered measurements and silently dropped some. Where two measurements share an angle the farther one wins.
- *Fill gaps* (`fill_gaps`, the "Fill gaps" checkbox, on by default): a station with fewer than three wall points gets a section blended (polar, weighted by walking distance) from its nearest measured neighbours reached through other unmeasured stations, or copied from the single nearest one at a dead end. It is an inference, not a measurement: it is drawn paler (colour tinted toward grey-blue), counted in the console ("N unmeasured stations filled in..."), switchable, and never alters measured geometry (tested).


**No layers** (Nathan, 2026-10-11): the document has no layer entity. Everything is organised by **type** (what it is: wall, ledge, pit, symbol name, ...) and **trip** (whose ground it sits on). Style (colour, weight, linetype) is derived from the type by one palette; showing and hiding is a view filter by type and by trip, not stored per entity. DXF layers exist only as an export artefact, named from type (and trip) so other CAD programs still get a sensible drawing; the importer reads them back as nothing. Removed: `Kind::Layer`, all `Layer*` fields, `FeatureLayer`/`SymbolLayer`/`TextLayer`; added `TextTrip`. Next: the palette (type to style), type and trip filters in the web UI, export layer names from type/trip.
