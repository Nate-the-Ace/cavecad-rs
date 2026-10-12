# Design decisions

Recorded 2026-10-11. Update when a decision changes, with the date.

- **Own core.** No QCAD, no CADCraft. The old app is frozen at tag `qcad-final-2026-10-11`.
- **Chromium-first, local-first.** Caves live on the user's machine (File System Access API, OPFS
  working copy). Cloud providers (Drive, Dropbox, OneDrive, WebDAV) come later behind the same
  `cave-store::Store` trait. Tablets reach files through cloud APIs. Nothing uploads to our server.
- **UI.** Modern ribbon, web-capable. Toolkit not yet chosen (leading: web front end over a WASM
  engine). Engine crates are toolkit-independent so this does not block them.
- **Console.** Read-only panel fed by `cave-log`; one log per cave file (`<cave>.log`).
- **Co-editing later, planned now.** Stable ids, operation log, source/derived split, atomic
  strokes, last-writer-wins fields, content-addressed blobs, schema version in the log. Phases:
  op log, async merge through shared folder, live relay, permissions.
- **Port order** (the QCAD-era suite, Expedition Planner cut, Handbook last): survey core, import
  and export, checks, draw panel, shelf, Feature Trace, Symbol Palette, Station Table, Entrance
  and basemap, Profile, Cross Section, Shaped Lines, Area Fill, Callout, Linetype Maker, Scans,
  Sheets, Legend and Package, housekeeping, 3D viewer, Notebook, Xref and Surface data, Handbook.
