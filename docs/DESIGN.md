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

**Pen and tablet input** (Nathan, 2026-10-11: tablets are a target, so drawing can be "artsy" where it makes sense). Pressure only ever changes how a line is *drawn*, never where it is: surveyed coordinates stay exact. Built so far: a stroke (`cave_geom::Path`) can carry optional per-point `widths` (relative pen weight, 1.0 = the layer's normal weight); they are validated, survive transforms and merges (stored whole with the stroke), and are absent for ordinary lines. Not built: the web input layer (Pointer Events: pressure, tilt, coalesced and predicted events, pen versus finger, eraser button, palm rejection), a `cave-ink` crate (pressure curve, smoothing, resampling, simplification; pure and testable with recorded strokes), a variable-width ribbon renderer, and mapping to DXF variable-width polylines. Where pressure makes sense: Feature Trace (walls, ledges), cross-section and sketch strokes, Shaped Lines accents, annotation ink. Where it does not: station placement, symbols, text, anything measured. Finger = pan, zoom and orbit; pen = draw. Layer weight (importance) stays the base, pressure is a modifier, with a per-tool on/off.
