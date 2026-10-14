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

**No legacy compatibility** (Nathan, 2026-10-11): the QCAD-era app had one user, so this version is the only version. No importer for old-app drawings, no matching the old XDATA tag vocabulary, no "old suite can read our files" goal. DXF stays our own interchange: embedded document plus backup carriers for exact round trip, readable by other CAD programs for viewing. The old math is still reused and golden-tested (that is about correctness, not file compatibility). The `tests/old_app` fixtures stay only as sample inputs for the golden math tests.

**Under-passage shading** (2026-10-11): lines that run under another passage are drawn dim and dashed, judged from the camera (`cave-scene/src/under.rs`): each leg is a tube of radius max(measured left/right, 1.5 m); a point is under a leg when the sight line (parallel in ortho, from the eye in perspective) passes within that radius and the leg is at least 2 m nearer. Owner-passage legs are exempt. The viewer asks the engine again 150 ms after the camera settles (`engine.shading`) and on projection switch. Open: long splays aimed along the passage (B12-B15 in Pitfall) overlap legs and need a decision (hide along-passage, cap length, or tidy test data); the 2 m clearance is a guess. Next: type-to-style palette, type/trip filters, Open/Save DXF UI.

**Long splays along the passage** (Nathan, 2026-10-12, chose "hide"): a splay aimed within 20 degrees of a leg at its station (either way, judged in 3D so a shaft behaves like a level passage and no bearing is invented) is not drawn and does not shade (`cave-scene`, `ALONG_PASSAGE_DEG`). It stays in the data and still counts for walls and the passage surface. The 2 m under-passage clearance is still a guess.

**Type palette** (2026-10-12): `cave-style` is the one table from type to style: colour *role* (ink), line weight, linetype, opacity, and the DXF layer name. The viewer maps inks to its light/dark theme and reads weight/linetype/opacity from the table (`engine.palette()`); the DXF export maps inks to colour indexes and writes each layer's colour and linetype (a real DASHED linetype) into the LAYER table. A test keeps any one DXF layer from mixing looks. An unknown type (a symbol nobody styled) gets the default look. Types today: wall, ledge, pit, centerline, splay, station, label, closure, tie, closure-note, profile, profile-wall, profile-sketch; the user-editable palette (and per-symbol styles) comes with the symbol tools.

**Type and trip filters** (2026-10-12): a view setting, never stored on anything authored. The viewer keeps the hidden types and trips and passes them to the engine (`ViewFilter`); the scene, the under-passage shading and the passage surface are all worked out from what stays visible, so a hidden trip neither draws nor shades others. A station shared by two trips stays while either is visible; a station only a hidden trip touched disappears; walls and the surface are built from visible shots only (a shared station still keeps its measured cross-section). The Show menu lists only types the open cave has, plus its trips; it resets trips (ids are per cave) when another cave opens. Not yet filterable: profile sketches (they are drawn through the Profile panel), and DXF export ignores the filter on purpose (a file always holds the whole cave).

**Open and Save DXF in the browser** (2026-10-12): `cave-wasm` exposes `open_dxf` (all-or-nothing: a damaged, foreign, binary or too-new file changes nothing and says why in plain words) and `save_dxf`; `apps/web/src/lib/files.ts` does the file part. With the File System Access API (Chromium) Open remembers the file so Save writes back to it and Save as asks where; without it Open uses a file input and Save downloads a copy. Ctrl/Cmd+O, S and Shift+S work. Files never leave the machine. A cave opened from a file replaces the one on screen (demo picker shows the file name), resets undo and the trip filter. Not built: an "unsaved changes" warning before replacing a cave, remembering recent files, autosave to OPFS.


**Display units** (Nathan, 2026-10-12): View tab, Units group: Metric (default) or Imperial. Display only: the cave is always stored and solved in metres, and DXF export is unaffected. The choice is a view setting remembered on this machine (localStorage). Shown in feet today: map scale bar, Profile elevation axis and run lengths (`apps/web/src/lib/units.ts`). Any new length shown to people must go through `formatLength`/`metresPer`. Not built: unit-aware entry fields (feet-and-inches typing) and a per-cave units setting.

**Units apply to entry fields too** (Nathan, 2026-10-12): the unit setting defines what is typed into the fields of every tool, not only what is displayed. `LengthField` is the one entry component for distances: the held value is metres, a bare number means the current unit, an explicit unit wins (`30 cm`, `12 ft`, `5'6"`, `5 ft 6`), bad text is flagged and never stored, and the field re-renders when the setting changes. Survey data being imported keeps its trip's own distance unit (`TripDistanceUnit`); this setting is about what a person types and reads in tools. No tool has length fields yet, so none are converted; the first ones (Draw, Station Table, Cross Section) must use it. Not yet covered: angles and other quantities.

**Profiles start at their tie-in station** (Nathan, 2026-10-12): every profile band begins with the station where its run joins its parent, shown first at s = 0 (run `B` now starts at `A12` in Pitfall, which the name `B` never said). Ported from the old `CsProfile.hierarchy` (`cave-profile::hierarchy`): the tie is decided by the surveyed shots, not the name. Phase 1 uses legs that placed a station (the end placed earlier is the parent), breaks cycles, and phase 2 lets a run still without a parent use closure/tie legs, skipping its own descendants. Golden tests compare parents, ties and bands with the old answers on all four test caves (the generator now records `hierarchy`). A run with no leg to another run (a root, or a disconnected part) has no tie. Not ported: second ties, name-versus-graph mismatch reports, band order. This replaces the earlier "name-derived parents only" gap.


**Cross Section panel, first slice** (2026-10-12; superseded below: sections are cut at stations, not anywhere on a leg): a panel (tabbed with Profile; View tab and the Home ribbon's Cross Section button) that cuts the passage across at any point along a surveyed leg. Pick a leg, slide or type the distance from its start station (a `LengthField`, so it follows the unit setting). The outline is built from the same rings as the 3D tube (`cave-mesh::cut_outline`, using every measured LRUD tick and splay tip at the two stations), so a cut at a station is exactly the tube's ring there (tested), and between stations it is a straight blend of the two end rings. The caption always says how far the cut is from the nearer station, and the measured points of both ends are drawn as evidence (dots and squares). A station with fewer than three distinct measured directions is refused in plain words (the first station of a chain has none: walls are recorded with the shot into a station); where "fill gaps" supplies an inferred ring (the panel uses it) the caption says so. In a vertical shaft north is up the page (same fixed frame as the tube, no bearing invented). Departure from the old app (`CsSectionCut`): that sampled 32 evenly spaced angles and carried a rotation-minimising frame; here the outline has only as many vertices as there are measurements and the frame is the tube's, so panel and tube always agree. Not built yet: sketching on a section (stored as anchored features, like profile sketches), sections placed in the 3D tube (`sectionRing`), picking the cut point by clicking the map, saving a section to a sheet. Each cut re-solves and rebuilds the rings (fine for the test caves; cache if big caves feel slow).

**Cross sections snap to stations and are sketched there** (Nathan, 2026-10-12): a section exists at a station, because that is where it is sketched. The Cross Section panel now picks a station (dropdown, previous/next, or a plain click on a station in the map, which rings it there and brings the panel forward) instead of a point on a leg. The measured ring at the station (the tube's own, built from the first leg arriving at it, else leaving it) is shown dashed as a GUIDE to trace against, never as the passage; the pen or mouse draws the real shape over it (pressure kept as widths, pan with middle button or fingers, wheel zooms, undo/redo shared with Profile). A station with too few measurements says so in plain words and can still be sketched on (the field-book page is the evidence). A stroke is a `section-sketch` feature (`cave_draw::section_sketch`) naming its station (`FeatureStation`, validated like any reference), with points `[across, up]` in metres from the station in the tube's frame at that station, so it follows the station when the survey is adjusted and rotates with the passage if the survey changes direction. It carries the trip that surveyed the station, so the trip filter hides a sketch with its passage. In a shaft north is up the page. Station-only sections replace the leg-and-slider cut. Not built: erasing or selecting a stroke, sketches inside the 3D tube (`sectionRing`), a legend of what each stroke means (type palette entry for section sketches), saving a section to a sheet, sketching over a scanned field-book page.


**Theme changer** (Nathan, 2026-10-12: dark mode text and glyphs were black and unreadable): ported from the old app's `Theme` (`apps/web/src/lib/theme.ts`): dark, light or auto (follow the system) and ten colours (Ocean, Teal, Forest, Amber, Ember, Crimson, Rose, Violet, Indigo, Graphite) plus a custom highlight colour, with the same palette formulas (greys tinted by the colour's hue). View tab, Theme group. Default is dark/ocean, as in the old app. The choice is ALWAYS applied explicitly as CSS variables, `data-theme` and `color-scheme` before the app mounts (never left to the browser's `prefers-color-scheme`, which is what let dark text land on a dark page when the app was shown inside a viewer with a different theme); it is remembered on this machine. The 3D view, Profile, Cross Section and the dock all redraw on a change (`onTheme`), and dockview's stock greys are replaced with the palette. Not done: the view cube's face colours are still fixed.

**Sketches in the Map and Profile views** (Nathan, 2026-10-12): cross-section sketches show beside their station in the Map and the Profile, at true size, joined to the station by a dashed leader, and are PLACED BY HAND: drag one and the drop is saved. The placement is its own record per station and view (`section-placement` feature, `cave_draw::place_section`, tags x/y/z; map offset is east/north/up metres, profile is along-the-band/elevation), so it is undoable, merges, and two views never clash; validation treats it as a record, not a missing drawing. Until placed, a section sits just east of its station on the map and just above it on the band. Each view has its own show/hide toggle ("Sections" in the Map bar and the Profile bar; "Profile sketches" in the Map bar), remembered on this machine; a cross-section glyph always faces the screen, so it reads from any angle. Profile sketches also show in the Map, in place: each point sits in the vertical plane through the leg it hangs on (`profile_sketches_3d`: along the leg's ground direction and up from its start station), so they follow the survey like any anchored stroke. A drawing standing in a vertical plane cannot be read from straight above or below, so these fade out as the camera tilts toward vertical (fully visible within about 30 degrees of level, gone beyond about 21 degrees from vertical: the Top and Bottom views). Not built: placing profile sketches by hand on the map (they are tied to their run), snapping a placed section to the view plane, and per-station show/hide.

**Draw tools replace per-view sketch buttons** (Nathan, 2026-10-12): the Profile and Cross Section no longer have their own pencil/hand buttons. The Draw tab offers Wall, Ledge, Pit and Sketch (`cave-style::DRAW_TOOLS`, each a palette type); pick one and draw in whichever view you like, and the line goes to that view's context: the open profile (`cave.run`), the cross-section station (`cave.station`), or the map's plan. Picking the tool again or pressing Escape puts it down; with no tool, a left drag pans. On the map a stroke is drawn only from the Top or Bottom view (where plan is what is seen; otherwise the toolbar says to snap to Top); it is anchored to the survey's legs like a profile stroke (`cave_draw::map_line`, `cave_scene::map_lines_3d`) so it follows adjustment, and rides the passage's elevation. A stroke is now one thing drawn in a view: `FeatureType` is what it is (wall, ledge, pit, sketch: the palette type that styles it, including linetype) and the new `FeatureView` (profile, section, map) says where it was drawn and so what its points mean; the old `profile-sketch`/`section-sketch` types are gone. The DXF export puts profile linework on its own layers (`CAVE-WALLS-PROFILE`, ...); exporting section and map linework is not built yet.

**Undo and redo belong to a panel** (Nathan, 2026-10-12): each panel keeps its own history (`profile`, `section`, `map`) in the engine, so Profile's Undo and Redo only touch what was done in the Profile, Cross Section's only what was done there, and the Map has its own pair in its toolbar. An edit lands in the history of the view it was made in (a stroke by the view it was drawn in, a placement by the view it was placed in, an edit or delete by the stroke's view). Opening another cave starts every history afresh. A later edit in another panel can touch the same stroke; undo then simply applies what that panel did.

**Leader and label of a cross section** (Nathan, 2026-10-12): the dashed leader from a station to its section stops on the OUTER SHELL of the drawing (it runs toward the middle of the sketch and ends at the first stroke it meets, or at the stroke nearest the station if it would miss them all) instead of running to the section's origin inside it, and the drawing carries the station's label centred just beneath it. The leader still ties the drawing to its station.

**Editable spline linework** (Nathan, 2026-10-12): drawn linework is a smooth curve (centripetal Catmull-Rom, `apps/web/src/lib/spline.ts`) through a few fit points, not a dense freehand trace. When the pen lifts, the stroke is reduced to the fewest points that keep the curve within about 2.5 px of the drawing (Ramer-Douglas-Peucker), pen weights going with the points that stay; a section stroke is stored as a `Spline` path, profile and map strokes as anchored fit points (the anchors are the fit points; the curve is drawn through them in every view). With no drawing tool picked, a click on a line selects it (halo, and its fit points show as square handles in the view it was drawn in); drag a handle to move that point; double-click the selected line to add a fit point there; Delete removes the line; Escape lets go. Each is one undoable edit in that view's history (`cave_draw::edit_stroke`, `remove_stroke`, wasm `edit_line`/`delete_line`); profile and map strokes are re-anchored to the survey when their points move. The selection is shared (the selected line gets a halo in the Map too), but only the view a line was drawn in edits it; on the map, editing works from the Top or Bottom view. `lib/line-editor.ts` is the one implementation for the Profile and Cross Section canvases (checked by `npm test` in `apps/web`). Not built: dragging whole lines, smoothing/straightening a segment, closing a line into a loop, and corner points (every fit point is smooth).

**Map undo/redo, and the rule for future panels** (Nathan, 2026-10-12): the Map's Undo and Redo sit in its main toolbar (and Ctrl/Cmd+Z, Shift+Ctrl/Cmd+Z while the map has focus) and only undo what was drawn, edited, deleted or placed on the map. All three panels now use one `PanelUndo` component so they behave the same; any future panel that can be edited gets its own scope and its own `PanelUndo` (recorded in CLAUDE.md).


**One Show menu on the Map** (Nathan, 2026-10-12): every visibility option of the Map lives in a single "Show" dropdown in its toolbar: Adjusted, Walls, Passage, Profile sketches, Sections and Fill gaps (under "On the map"), then the type and trip filters. Only the demo picker, undo/redo and the elevation-colour button stay outside. Escape or a press on the map closes the menu. Any new "show or hide" option for the Map goes in this menu.

**Station ring only while the Cross Section can be seen** (Nathan, 2026-10-12): the ring that marks the station the Cross Section shows is drawn on the map only while that panel is actually visible (open and in front, not closed or behind another tab). `panelState.visible` in `workspace.svelte.ts` tracks this for every panel.

**Contextual ribbon tab and spline resolution** (Nathan, 2026-10-12): while a command is running, the ribbon grows a tab named after it (Wall, Ledge, ...) that comes to the front, holds that command's settings and a Done button, and hands the person back to the tab they were on when the command ends (Done, Escape, or picking the tool again). What a command offers is data (`apps/web/src/lib/commands.ts`: `optionsFor(command)` returns option definitions: a range or a toggle with a label and a hint), so the ribbon shows any future command's settings the same way (`ContextOptions.svelte`); values live in `cave.options`, are remembered on this machine, and every consumer reads the same ones. Every drawing tool has two: **Curve detail** (1 to 10; the resolution of the splines: the tolerance a freehand stroke is reduced within runs from 8 px at 1 to 0.6 px at 10, default 5, so the same stroke keeps 6, 11 or 35 fit points at 1, 5 and 10) and **Pen pressure** on or off (off draws every line at the normal weight). Detail applies to strokes drawn after it is set; re-fitting a line already drawn is not built (its dense trace is not kept, only its fit points, which can be moved, added and deleted). Rule for new commands: put their settings in `commands.ts` so they appear in the contextual tab; do not add per-panel option buttons.

**Selecting a line brings up its tab; Close; picking a line up; right click** (Nathan, 2026-10-12):
- *Contextual tab for a selection.* Selecting a drawn line in any view brings a tab forward ("Edit Wall", ...) with what can be done to it: change what it is (Wall, Ledge, Pit, Sketch), Close or Open its curve, Delete, Done. Deselecting (Done, Escape, a click on empty space) returns to the tab you were on. A running command's tab takes precedence over a selection's. Each change is one undoable edit in the view the line was drawn in (`cave_draw::set_stroke_kind`, `set_stroke_closed`, wasm `set_line_kind`, `set_line_closed`, `line_info`).
- *Close.* A closed spline runs from its last fit point back to its first, smoothly (periodic Catmull-Rom, no corner at the join); it needs at least three fit points. Stored as `closed` on a section stroke's spline path and on an anchored profile or map stroke (`AnchoredPath.closed`), kept when a fit point is moved, added or deleted. Drawing tools have a "Close curve" setting in their tab so a curve can be drawn closed from the start, and the selection tab can close or open it later.
- *Pick up where you left off.* With a tool active, a stroke that starts on an end of an open line of the same type in the same view carries that line on (an end ring shows while the pointer is over one): the new fit points join the line's own end point, so panning the view between strokes leaves no gap; drawn away from the START of a line, they go in front. Anywhere else a new line starts. Closed lines have no ends and a stroke of another type starts its own line. Reach is 12 px.
- *Right click ends the command* (like most CAD): in the Profile and Cross Section a right click puts the tool down; on the Map a right CLICK does, while a right DRAG still pans (decided on release, because some systems send the context-menu event on press).

**Fixed ribbon height** (Nathan, 2026-10-12): the contextual tab used to be a few pixels taller than the others, which moved every panel below it. The ribbon is now one fixed height (98 px: a 59 px body for a group's controls and a 20 px group name, set from `--ribbon-h`, `--ribbon-body`, `--ribbon-name`) on every tab, and every control inside is sized from those, so nothing can change it. Measured: the bar is 98 px and the panels start at the same place on Home, Draw, Sheets, View, a running command's tab and a selection's tab. The command settings group fits by putting the on/off settings side by side on one row.

**Fingers draw while a tool runs** (Nathan, 2026-10-12: on an iPad a finger only panned, even with the Wall command running): with a drawing tool running, a single finger draws in the Profile, Cross Section and Map (Top view); two fingers pan and pinch-zoom together, and a second finger landing mid-stroke drops that stroke (it was the start of a gesture). With no tool, one finger pans, taps select a line and a finger can pick up a fit point or a cross-section drawing. The contextual tab has a "Draw with finger" setting (default on); off gives the old behaviour (a pen draws, a finger pans). This replaces "finger = pan, pen = draw" from the pen-input notes. `lib/touch.ts` (checked by `npm test`) tracks the fingers; the iPad itself still needs a real try: the checks use emulated touch. Palm rejection (ignoring the hand while a pen is down) is part of the pen-input work still to come.

**Profile walls from splays** (2026-10-12): the ceiling and floor of a band now use every splay as well as the U and D measurements (`cave-profile::band_wall_runs`, ported from `CsProfile.bandWallRuns` and checked against the old output on all four test caves: every ceiling and floor point and every tick). A splay lands at its station's distance along the run plus its own distance along the passage (the bearing of the leg arriving at the station), at the elevation it reaches; pointing up it joins the ceiling, down the floor, in order along the run. A splay within 10 degrees of level (the boundary itself counts as level) says nothing about the ceiling or floor, so it is a tick of its own (small squares on the band, points on the profile wall layer in the DXF) and never bends a line; a splay with no inclination contributes nothing. Lines break, and are never joined across the break, at a junction, at a closure, at a station with no vertical evidence, and past an open (P) ceiling or floor where a pitch leads on. A splay along the passage axis still counts here (its vertical part is a real measurement), unlike in plan. Not ported: the fixed-azimuth option and the skipped-splay count.


## 2026-10-12: Shaped lines, first slice (ticks, fans, scallops)

Ledge and Pit now draw hachures, and three new types exist: Flowstone and Rimstone (scallops) and
Slope (fans). Only the curve is stored; the ornament is regenerated from the drawn spline every
time it is painted, so editing the line moves the marks. New crate `cave-shape` is a port of the
pure geometry in the old `CsShapeLine` (stations, ticks, fans, scallops; old spacings and sizes,
feet converted to metres), with hand-solved unit tests. Scallop and fan styles hide their spine, as
the old app did. The marks lie on the right of travel; `Field::FeatureFlip` and a "Flip side"
button in the selected-line tab move them to the left (one undoable edit in the line's own panel).
Not yet: Wall Glyphs and automatic outside-of-wall sides (`autoSides`), ornament in the 3D map
view, ornament in DXF export (the spine exports; marks do not yet), per-line spacing and scale.

## 2026-10-12: Live editing test setup (laptop plus iPad)

Goal: two real devices editing one cave at once, to find out what breaks before the real
transport is built. It reuses what the document already had (ops with actor and logical clock,
order-independent merge, undo as ordinary edits) and adds only plumbing.
- `cave-doc`: `Document` keeps an outbox of local ops (`take_outbox`, undo and redo included),
  can describe itself as ops (`snapshot`) so a joiner rebuilds it with every stamp intact, and
  can change its editor (`set_actor`). Every device gets its own random actor when a cave is
  opened; two devices that shared an actor could stamp different edits identically.
- Found by the first two-browser run: serde_json reads some floats one digit off, which left the
  devices holding slightly different caves. `float_roundtrip` is on in `cave-doc` and `cave-wasm`,
  with a test that fails without it.
- Relay (`apps/web/scripts/sync-relay.mjs`, served at `/sync` by the dev and preview servers): a
  dumb forwarder. Devices join a named room; the first into an empty room supplies the cave, later
  ones receive the room's whole op log, then ops are passed on. It never reads an op and keeps the
  log only in memory while the server runs. Nothing leaves the machine the server runs on.
- Web: View tab, "Live editing" group (room name, Join, Leave, who is in). Local edits are sent
  every 80 ms; opening another cave leaves the room. `npm run dev` now listens on the network.
- To try it: run `tools/live-test.sh` on the laptop (it prints the address; or `npm run dev` by hand), open the "Network" address it prints (for example
  `http://192.168.1.20:5173`) on both the laptop and the iPad, open View, type the same room name
  on both and press Join. The published artifact link cannot do this (it is a static page with no
  relay behind it).
- Checked: `cave-doc/tests/sync.rs` (join by snapshot, concurrent edits, undo sent as an edit,
  bit-exact floats), `tests/relay.check.mjs` (real sockets), and two browsers drawing in turn
  ended with identical documents.
Not built yet (in this order): see other people (presence, who is editing what, their cursors),
a reconnect that catches up after a dropped connection, a stored log so a room survives a server
restart, trimming the log, per-key merge for tags and a text type for long notes, and a relay that
is not the laptop (the same protocol served from a small always-on server).

**Avatars** (2026-10-12, Nathan): while live, everyone in the room shows as a round avatar (initials on
their colour) at the right end of the tab bar, this device first and ringed; hover for the name. A
device's name defaults to what it is (Laptop, iPad, Phone) and can be changed in View, Live editing;
the colour comes from its id. The relay passes each device's name and colour on as plain text and
sends everyone the roster whenever someone joins or leaves. Not yet: what each person is editing,
their cursors, or choosing a picture.

## 2026-10-12: Pen input, first layer

`apps/web/src/lib/pen.ts` (pure, checked in `tests/pen.check.ts`) holds the rules; the Map, Profile
and Cross Section all use it.
- **Palm rejection** (`PenGuard`): a touch is ignored while the pen is touching the screen, for
  600 ms after it was last near the screen (hovering counts), or when the contact is bigger than a
  fingertip (32 px, where the device reports a size). A touch rejected when it lands stays ignored
  until it lifts, however long the palm rests. A pen landing cancels anything a resting hand had
  started (a pan, a half-drawn stroke). Mouse and pen are never ignored.
- **Eraser**: the pen's eraser end (or button 5) deletes the line it passes over, one undoable
  edit per line in that panel's own history.
- **Pressure** is one function (`widthFor`, 0.4 to 1.6 times the line's weight); it only changes
  how a line is drawn. Coalesced events were already used for smooth strokes.
- Checked in a real browser with pen events (pressure widths, a touch during a pen stroke
  ignored). The eraser was checked with a synthetic event, because the browser's test protocol
  cannot press the eraser bit. No real Apple Pencil has been tried yet.
Not built: tilt (nothing uses it yet), a barrel-button action, hover cursor for the eraser, the
`cave-ink` crate (pressure curve, smoothing, resampling), a variable-width ribbon renderer and DXF
variable-width export, and an eraser that splits a line instead of deleting it whole.

## 2026-10-12: Symbols (the original app's set, placing, and a way to add more sets)

**What the original did.** The old app kept every symbol as a `SYM_*` block in its NSS template, with
a catalogue (`CsSymbols.CATALOG`: NSS name, UIS alias, home layer, category) and a palette panel: a grid
of pictures, click to arm, click in a view to place, drag to aim.
**What is built.**
- `tools/extract_symbols.mjs` pulls all 80 of those blocks and their catalogue entries into
  `crates/cave-symbols/data/symbols.json` (lines, arcs, circles and bulged polylines flattened to
  polylines and circles, each scaled to unit radius, y up). The new crate `cave-symbols` serves the
  catalogue; tests check every drawing is finite and unit-sized and the keys are unique.
- A placed symbol is an ordinary stroke of type `sym-<name>` with two points: where it sits, and a
  second point whose direction is the symbol's "up" and whose distance is its radius. Because it is a
  stroke it gets, with no new plumbing: anchoring to the survey, per-panel undo, the type and trip
  filters' data, selection, moving and resizing by its two handles, the eraser, and live editing
  between devices. A click places it at the Symbol size setting (contextual tab, default 1.5 m radius);
  a drag sets size and direction. Works in the Map (plan), Profile and Cross Section.
- Symbols panel (a tab beside the Console, also on the View tab and a Draw tab button): search,
  grouped by category, hover shows the UIS name.
- DXF: a symbol on the Profile exports as its drawing (polylines) on the symbol's own layer.
- `tools/import_symbol_set.mjs`: adds another set from a folder of SVG files (paths with curves and
  arcs, shapes, transforms, solid fills), and reports which symbols the catalogue already has by name
  so only missing ones are added. Checked with `npm test` (`tools/check_import_symbols.mjs`).
**Not done.** The UIS and USGS artwork itself: this build has no route to those downloads, and the
only USGS point-symbol set that turned up in a search is for geologic maps, so it needs the exact
files from the cartographer. The old app's own audit found no UIS or NSS point symbol that Therion
lacked apart from rigging, which the original already shipped. Also not done: symbol types in the Show
menu's type list, symbols in the Map and Cross Section DXF export (those views export nothing yet),
a colour per category, scaling by sheet scale instead of metres.

**Symbols, second pass: the UIS and USGS sets** (2026-10-12; Nathan supplied the carto.net page for the
official UIS list and the USGS OFR 95-526 zip).
- *UIS.* The page lists 42 entries with names, translations and example pictures (plan and profile) but
  the pictures are map fragments, not clean artwork, and many entries are line styles or area fills
  (outline of a gallery, underlying passage, presumed dimensions, ceiling form, dripline, step, contours,
  joints and faults; water, sump, sinter pools, flowstone, sand, clay-covered walls) or text (height of a
  room, elevation difference, altitude figure). `tools/make_uis_symbols.mjs` redraws the 35 UIS point
  symbols the catalogue lacked as vectors (set `uis`): entrance arrow, chimney, pit mark, chimney-pit,
  wavy stream arrow, seepage, ice, cauliflower calcite, disc, stalagmite, stalactite, curtain, pillar,
  helictite, soda straw, crystal, moonmilk, wall calcite, flowstone dashes, pebbles, guano, bone,
  anastomosis, karren, scallop, flute, paleoflow, inclined joint, human activity, camp, altitude point,
  survey point, north arrow, air draught, section direction arrow. They sit beside the original NSS-style
  drawings (marked UIS in the palette) rather than replacing them: a cartographer picks the set.
- *USGS.* OFR 95-526 is the geologic map symbol standard (about 900 symbols on 28 EPS sheets, public
  domain). It holds almost nothing cave specific, and the sheets give drawings and cartographic
  specifications but mostly not the symbols' names (those are in the printed OFR 95-525), so a symbol is
  added by naming its reference number in `tools/usgs_picks.json` and running
  `tools/add_usgs_symbols.mjs`. Added so far: Rapids (2.44.1) and Falls (2.44.2), the two named water
  features on sheet 23 that a cave map uses. `tools/extract_usgs_eps.mjs` reads the Illustrator EPS
  directly (no PostScript tool needed). Strike and dip, joint and bedding symbols (sheets 5 to 6 and
  others) need names confirmed and carry a dip number, so they wait for the cartographer's picks and for
  a text label on a placed symbol.
- The dev server now rebuilds the engine when a `crates/*/data/*.json` file changes.

## 2026-10-12: Words on strokes (symbol labels and the Text tool)

`Field::FeatureText` holds the words of a stroke, set in the same edit that creates it
(`cave_draw::with_text`) and changed later in one undoable edit (`set_stroke_text`, the selected
line's tab). Two uses: a **label** beside a placed symbol (a dip, a depth, "P 19"), and the **Text**
tool (type of its own, `text`): click to place the words, drag to set their size (the distance) and
direction (the aim), like a symbol. The contextual tab of a symbol or text tool has a Label or Text
field and a Size; the words are not remembered between visits (a forgotten label would quietly go on
tomorrow's symbols). A text stroke with no words is refused with a hint. The same two-point model keeps
everything else working (anchoring to the survey, per-panel undo, eraser, live editing). Profile DXF
export writes text as TEXT entities. Limits: 200 characters; text does not wrap; the Map's
text is projected like any line.

## 2026-10-12: Area fills (the Fill tool) and the end of the Trace placeholders

The Draw tab's Trace group (Feature, Shaped, Fill placeholders) is gone: Feature tracing is what the
linework tools already do, Shaped lines are the Ledge, Pit, Flowstone, Rimstone and Slope tools, and Fill is
now real. Pick a kind of ground in the Area fill group, press Fill, and draw all the way around the area in
the Map, Profile or Cross Section (a fill is always a loop; too small a loop is refused with a hint).
- New crate `cave-fill`, a port of the old `CsArea`: 13 fills (blocks, debris, pebbles, sand, clay,
  bedrock, water, sump, flowstone, moonmilk, guano, ice, bones) with the old densities, sizes and
  spacing rules. Scatter fills use the old Park-Miller generator and the old rejection sampler, and
  `tests/golden.rs` checks 30 cases against the old app's own output (count, positions, element, size,
  angle, in order). Hatch (sump, flowstone), dots (moonmilk), a solid tint (water) and outline-only
  (bedrock) are new code. The old app drew in feet, so patterns are laid out in feet and converted.
- Only the outline is stored (a closed spline stroke of type `fill-*`). The marks are derived: the
  engine makes them from the smooth outline and a seed taken from the stroke's id, so every device and
  every redraw shows the same marks, and editing the outline refills it. The web app caches marks per
  outline. A huge outline gets sparser instead of slower (6,000 marks at most).
- Marks are true size (a pebble pattern is the old app's feet-sized pebbles, in metres), so they look
  like dots at the scale of a whole cave profile and fill in as you zoom, as on paper at a large scale.
- The selected fill's tab changes its kind from a list. DXF export of fills is part of the DXF work.

## 2026-10-12: Line styles from the UIS list

The UIS entries that are line styles are now drawing tools in two new Draw tab groups, **Passage
lines** (underlying passage: short dashes, wide gaps; presumed dimensions: long dashes, narrow gaps;
too narrow to pass: dashes; dripline: the hachure line, a shaped line) and **Geology and relief**
(joint: dash-dot; fault: solid, heavier; bedding plane: dashes; contour line: thin). Geology draws in
a magenta, as on the UIS list (a new ink role, `geology`, with a light and a dark shade). Line
patterns are real: `Linetype` gained `LongDash`, `Sparse` and `DashDot`, each with a pattern in metres
and a DXF name, and the DXF export writes every pattern into its line-type table (a test checks every
pattern a type uses can be written). The Draw tab groups come from `cave-style::TOOL_GROUPS`. The type
row of a selected line became a list, since there are now fifteen kinds of line. Still not done: the
UIS step (the existing Ledge covers it), gradient lines, and the double-line fault of the UIS list.

## 2026-10-12: Everything drawn is in the Show menu

The scene's type list now includes every type of stroke the cave holds (lines, text, fills, symbols),
each once. The Show menu lists them (symbols in their own "Symbols" section under their names) and it
re-reads when opened, so what was just drawn is there. Hiding a type hides it everywhere: the Profile,
Cross Section and Map draw nothing of it, and it cannot be picked, edited or erased while hidden (the
line editor, the eraser and the map's hit tests skip it). It is a view setting only: nothing is stored on
the drawing. DXF export still writes everything, as before.

## 2026-10-12: DXF export of everything drawn, in every view

The export now writes what is drawn in the Map and in each station's Cross Section, as well as the
Profile, and it writes each stroke in its own look instead of as a line through its fit points:
- **Curves are smooth.** A line is written as the centripetal Catmull-Rom curve the screen draws
  (`cave_geom::spline::smooth`, checked against the web app's own code in `tests/spline_golden.rs`),
  with the pen weight carried along it.
- **Symbols** are their drawings on the symbol's own layer, with their label as text; **text** is
  text; **fills** are their outline, their hatch or scatter lines and their dots as circles (made by
  the same code and seed as the screen, so they match); **shaped lines** are their spine and their
  ornament on the side the stroke says (flipped or not).
- **Sections** sit beside their stations, at the hand-placed offset or the default beside the station,
  in the Map (layer suffix `-SECTION`) and on the bands (`-PROFILE`); map strokes carry `-MAP`. Layers
  stay one per type and view, never per trip unless asked.
- Checked by `crates/cave-dxf/tests/views.rs`: every kind lands on its layer, curves have their
  samples, the file reads back as the identical cave, and exporting twice gives the same file.
Not done: the water tint (a solid fill is only its outline in DXF), a leader line from a section to
its station, and per-trip layers for strokes.

## 2026-10-12: Shaped lines finished (wall glyphs, automatic edging, the Map, the DXF)

- `cave-shape` is now a full port of the pure geometry of the old `CsShapeLine`, with
  `tests/golden.rs` checking stations, hachures, fans, scallop chains, automatic edging and seeded
  glyph placements against the old app's answers on four spines. Two corrections to the first slice
  that the golden check forced: scallops are chains of true circular arcs given by a DXF bulge
  (positive bows right; 0.5 is about 106 degrees), not sine bumps; and a **closed** ticks or fans line
  (a pit) puts its marks on the **inside** whichever way it was drawn, as the old pit tool did (flip
  sends them out).
- **Wall glyphs** (Passage lines group): a catalogue symbol (stone blocks) every 8 ft along a wall, a
  little off it, with seeded jitter in position, turn and size. The side is decided per glyph from the
  survey, as the old app's `autoSides` did: outside is away from the nearest station; a station nearly
  along the wall defers to the nearest confident neighbour; passage on both faces (a fin) draws no
  stone. "Flip side" inverts every glyph's side. The survey is whatever stations the view has in the
  curve's own coordinates: plan stations on the Map, band stations on the Profile, the station itself
  in a Cross Section. The seed comes from the stroke's id, so every device shows the same stones.
- Ornament now shows on the **Map** (the plan curve's marks, lifted to the line's height) and in the
  **DXF** (spine and marks on the side the stroke says), as well as in the Profile and Cross Section.
  The web app caches each stroke's ornament until its curve, the survey or the flip changes.

## 2026-10-12: Presence and reconnecting

- **Pointers.** While sharing, each device tells the others where its pointer is, in the view's own
  metres, with which view and run or station it is looking at and which tool it has. The relay passes
  these on (stamped with the sender's own id, never stored, so a late joiner is not sent stale ones, and
  capped in size) and the Map, Profile and Cross Section draw other people's pointers as a small arrow
  in their colour with their name, only in the view, run or station they are in; a pointer not heard
  from for six seconds is dropped. Reports are rate limited (about 14 a second). The avatar's tooltip
  says what each person is doing ("Bo is drawing ledge in the Profile").
- **Reconnecting.** If the connection drops, this device keeps its cave and carries on ("Reconnecting,
  your edits are kept"), retrying after 1, 2, 4, 8, then every 15 seconds. On return it applies whatever
  the room has that it missed and sends what it made while away; ops merge in any order, so nothing is
  lost or doubled. If the relay was restarted (an empty room), the first device back supplies the cave
  from its own copy. A first connection that never gets through says so instead of retrying forever.
- Checked: `tests/relay.check.mjs` (presence is forwarded under the sender's id, not stored, size
  capped) and two real browsers: one hears the other's pointer in the right view and run; with one
  browser's connection closed under it, each drew a stroke, and after it reconnected both held
  identical documents with both strokes.
Not done: following another person's view, a notification when someone joins, and a stored room log
(a restarted relay only recovers from the devices that are still open).

## 2026-10-12: The unsaved-changes warning

`Document::revision()` is a number that goes up each time an op really changes something (an op
already applied, or older than what is there, does not count, so a reconnect that hands back the whole
log does not make the cave look edited). The web app remembers the revision at the last open, load or
save; when they differ the cave has unsaved changes: the footer says "Unsaved changes", the tab title
gets a dot, closing or reloading the page asks first (the browser words the question), and replacing the
cave (opening a file, picking another demo) asks "This cave has changes you have not saved. Replace it
anyway?"; declining puts the picker back. Edits from other people in a live room count as changes too
(the file on this machine is not what they were saving). Joining a room replaces the cave and counts as
clean. Not done: autosave to the browser's storage, and a "save, discard or cancel" choice (the browser
confirm has only two buttons).

## 2026-10-12: Fitting a drawn line again

The selected line's tab has a **Curve detail** slider (1 to 10). Letting go of it fits the line again,
as one undoable edit in the line's own panel. The hand movement a line was drawn from is long gone (only
fit points are stored), so what is fitted is the curve on screen: sampled finely and reduced to the fewest
points that keep it within a fraction of its length of itself (3% at detail 1, 0.67% at 5, 0.1% at 10).
Lower detail gives fewer points, easier to edit; higher gives more, following the curve closely; the ends,
the closed or open shape and the pen weights stay. Symbols and text have no curve and are left alone.
`refit` is in `lib/spline.ts` (checked in `tests/spline.check.ts`), `refitLine` in the line editor and the
Map's view (checked in `tests/lineeditor.check.ts`); the request goes through `cave.refit` and whichever
panel holds the line does it (a panel opened later does not redo an old request).

## 2026-10-12: The Station Table

A new **Stations** panel (a tab beside the Console, also on the View tab): one row per station with what
kind of place it is (lead, open end, junction, control, loop, noted, flagged), its trips, elevation, the
notes written there and the team's own status, note and who has it, typed straight into the row. Filter by
kind (any-of), text and status; double-click a row to centre the Map on it and open its Cross Section;
download the rows shown as a checklist CSV (elevations only, never positions).
- Port of the old `CsStationTable` and the parts of `CsFrontier` it uses (`cave-scene/src/stations.rs`),
  checked against the old app on all four test caves (`tests/stations_golden.rs`): same stations, kinds,
  trips, degrees, notes, loops and elevations. A lead is not a record: it is a note that says LEAD as a
  whole word; an open end is a one-leg station that is neither control nor written off with EOS in its
  note. Nothing derived is stored.
- The team's mark is three fields on the station (`StationStatus`, `StationTeamNote`, `StationAssigned`),
  written one undoable edit at a time in the panel's own history, and so shared with everyone editing the
  cave live (the old app kept it in a shared sidecar file for the same reason). Only what changed is written.
- No coordinates: a test checks a row never mentions a position. "Flagged" here means a shot arriving at the
  station could not be used (missing or impossible readings), not the old validator's wider checks; a lead
  with a later trip beyond it gets a "pushed?" suggestion that only a person can apply.
Not done: the old trip-planner packet (printable card), and the stations.json sidecar (live editing replaces it).

## 2026-10-12: Callout (a note with a leader arrow)
- A callout is one stroke of type `callout` with two points: what it points at, then where the note goes
  (drag from the thing to the spot). The words are the stroke's text; the leader (tip, elbow, landing on
  the note's edge) and the arrow head are derived every time (`cave-shape/src/callout.rs`), a port of the
  old `CsCallout.reflow`/`sideFor`, golden-checked on 60 cases (`tests/callout_golden.rs`). The old app
  needed a linked text + leader pair joined by an id; one stroke cannot drift apart.
- The leader leaves the note's near edge facing the arrow, so the note grows away from it; note height is a
  third of the drag, so the drag sets size like the Text tool. Works in the Map, Profile and Cross Section
  and is exported to DXF (text, leader and arrow head on `CAVE-CALLOUTS-<view>`).
- Not done: several arrows on one note, the curved leader style (the engine has it, no button yet), the
  note styles (hazard, dig...: use Show/types instead), and the elevation callout that reads a floor height.

## 2026-10-12: Sheets, first slice (the map on paper)
- `cave-sheet` ports the old `CsSheetSetup` maths: the 12 papers, 20 plot scales, which scale fits a cave on a
  sheet (`fit`), the scale bar's round divisions (`bar_for`, metric scales count metres) and the packing of
  title block, scale bar and north arrow into the band under the map (`furniture`). Golden-checked against the
  old code on 432 fits, 72 layouts and every bar (`tests/golden.rs`). Furniture comes out as plain shapes so
  screen, PNG and later DXF/PDF draw the same sheet.
- Sheets tab: paper, way up (best fits the cave), scale (best fit, metric scales first when the unit is metric),
  title block words, which pieces to show, Save PNG (200 dpi) and Print (the browser's dialog, "Save as PDF"
  there gives a PDF on the right paper size). The Sheet panel shows exactly what Print produces: walls (outer
  shell), survey legs and everything drawn on the map, in black on white.
- Sheet settings are kept in this browser (like view settings), not in the cave.
- Not done: several sheets / tiling a big cave, elevation sheets, legend, detail boxes, magnetic north arm
  (declination is not stored yet), sheet templates, putting the sheet in the DXF, dragging pieces around.

## 2026-10-12: Ribbon folds to fit, stations are survey-point triangles
- A ribbon group that does not fit the width folds into one dropdown button (rightmost first, so at least
  one button of every group always stays); its popup opens under it with the group's buttons two to a row
  (groups made of forms, such as the Sheets fields, appear as they are). Widths are measured from an unseen
  full-size copy of the groups, so folding follows what each group really needs. View > Ribbon settings >
  Compact layout folds every group whatever the width (kept on this machine).
- Survey stations are drawn as the UIS survey point (a triangle, with a dot when the station is a fixed
  anchor of the survey) in the Map, the Profile, and as the station's own mark in the Cross Section,
  replacing the round dots and the axis cross. Size is in pixels, the same at every zoom.

## 2026-10-12: Station tab, benchmarks
- Clicking a station (Map, Profile, or a row of the Stations table) picks it and the ribbon shows a Station
  tab (one thing is picked at a time: a tool, a line, or a station; Escape or Done lets go). It shows the
  kind of place, legs and elevation (never a position), and edits the passage dimensions (left, right, up,
  down, each with an "open" flag) and the notes, with the Benchmark toggle and the Stations undo/redo.
- The survey keeps walls and notes on shots, so an edit goes where the survey holds it: the walls of a
  station are on the last shot arriving at it that recorded any (else the last arriving one); a station
  nothing arrives at (the start of a trip) uses the trip's opening row. A station with several arriving
  shots has one note field per shot (the note chooser names each "shot A4 -> A5"). Values are metres and
  are written in the trip's own unit; only what differs is written (`cave-draw/src/station_survey.rs`,
  tests in `cave-scene/tests/station_detail.rs`).
- Benchmark is a station flag (`StationBenchmark`): an agreed benchmark is drawn as the fixed survey
  point (dotted triangle); every other station is the plain survey point. This replaces drawing survey
  anchors with the dot, which was wrong: an anchor is a maths choice, a benchmark is a decision.

## 2026-10-12: Selection in the highlight colour; one framework for picked things
- Selected things are drawn in the theme's highlight colour (`--accent`): a filled halo and ring on a picked
  station, a bright line along a picked leg or splay, a strong halo on a selected line or placed symbol, a
  highlighted Stations row. (Before, the halo was the thing's own colour at 25%, which hardly showed.)
- `apps/web/src/lib/inspectors.ts` is the framework for "pick it, get a tab": each pickable kind (station,
  leg, splay) registers its tab's groups and the component that draws them; the ribbon builds the tab from
  the registry. To add a thing, add its kind to `PickKind`, a component, and one registry entry. Edits go
  through the engine as one undoable step in the thing's panel history.
- Legs and splays: click one on the Map to get its tab (tape, bearing, inclination, back readings, note, and
  the flags Not on map, Void, Hold fixed). `cave-draw::set_shot` writes only what changed (metres in, the
  trip's unit out; inclination limited to -90..90). Picking legs in the Profile is not done yet (its
  legs carry no shot id).
- Deselecting: only a picked station is ringed (Map), ringed (Profile) or highlighted (Stations table); the
  station the Cross Section panel shows no longer stays lit after a click on nothing. A click on empty
  space in the Map or Profile lets go.

## 2026-10-12: User icon, people list, feedback reports
- The user icon sits at the very top right of the tab bar (your initials in your colour; a count badge when
  others are in a shared session) and opens one window: People (this device first, then everyone in the live
  room with what they are doing; join or leave a room), Options (your name, units, compact ribbon, reset
  layout) and Feedback. It replaces the row of avatars.
- Feedback: kind (wrong, idea, question), a message, optionally the log, app details and a screenshot, saved
  as one zip (`feedback.txt`, `log.txt`, `screenshot.png`; a plain store-only zip writer, tested against
  `unzip -t`). The log is the engine's own (already redacts coordinates) plus errors the page raised; the
  details are counts, browser and screen size, never the cave, its file name or any position. The
  screenshot uses the browser's screen capture (it asks first; the window steps aside), or an attached or
  pasted image where capture is not allowed. Save report, Copy text and Send are offered.
- Send posts the zip to `/feedback` on the same site. The dev and preview server keep these in a git-ignored
  `feedback/` folder; the hosted artifact has no such address, so there Send says so and the person saves
  the report. Where reports should go in production (a server, an email address, an issue tracker) is a
  decision for Nathan; nothing is sent anywhere without a click.

## 2026-10-12: Screenshots without permission; room owners make invites
- The screenshot is made by the page itself (`lib/snapshot.ts`: the browser draws the elements, the canvases
  are copied on top where they sit, the 3D map is redrawn just before it is copied), so it needs no
  permission and works in the hosted page and on tablets. The first version used the browser's screen
  capture, which is blocked or absent in many places, and did not work for Nathan.
- Rooms have an owner: the first device in. It is given a key (kept in this browser per room) that lets it
  come back, and it can make invites from its user window: a secret link that works for 1 hour to 7 days and
  for one person, up to five, or anyone with the link, with a label ("for Sam"), and can take one back. Making
  the first invite closes the room to people without one (the owner can reopen it); a person who was admitted
  can reconnect without using the invite again. The relay checks all of this (`tests/relay.check.mjs`); the
  link opens the app with the room filled in and a Join button. Invites live in the relay's memory like the
  rooms do: they end when the server restarts. They need a relay, so the hosted artifact (which has none)
  cannot start rooms.

**Survey Notebook** (2026-10-12): a notes-page panel (`cave-notebook` crate, `NotebookPanel.svelte`), ported in spirit from the old `SurveyNotebook.js`. One page per trip, derived from the document every time (nothing stored): station lines with L R U D, the shot line between stations with tape, compass and clino (foresight and backsight), a running total, a blank line where a chain breaks (a branch restarts at the station written after it), splays as `<station>.<n>`, loop and tie closures listed under the page. Compass cells are magnetic; the survey keeps the true bearing, so a typed reading is converted by the shot's (else the trip's) declination, and changing a trip's declination turns every bearing in the trip while the cells keep what the compass read. Every cell is an ordinary document edit in the panel's own `notebook` undo scope; lengths go through `LengthField`'s parser and `lengthText`/`formatLength`. Dimensions on a station line edit the arriving shot's own fields (not "whichever shot holds them"), except a trip's first station, which edits the trip's opening row; a station recorded on another trip's page is shown read-only. Departures from the old page: two lines per shot (station, then measurements) instead of one row per station; the old "both readings" LRUD (`5/10`) has no field in the new model so is not shown; the page edits the real survey live instead of drawing it on a Draw press; no Import/Export buttons (the file tools are the Home tab's). Not built: inserting a shot in the middle of a page (append and delete only), Compass/Survex/Therion/CSV export from the page.

**One-off importer for old drawings** (2026-10-12; Nathan allows one-off importers, still no legacy file compatibility): `tools/import_old_dxf.mjs <drawing.dxf> <out.json>` reads the survey out of a QCAD-era drawing (custom properties stored as XDATA) by running the old `CsRevise.surveyFromDocument` against a stand-in document, so the answer is the old reader's own. Checked equal to the old reader's output on all four synthetic caves (`npm run test:importer`). The output is the shot-list JSON the app already opens, with no drawing coordinates at all and fixed points reduced to elevation. The web Open dialog accepts that `.json`. Real caves' output belongs outside git: `apps/web/public/local/` is ignored (served in dev at `/local/`). `cave-fixture` now also carries trip date, team, instruments, declination and start note, and each shot's note and declination.

**Scans panel** (2026-10-12; first slice: viewer plus notebook link, aligning a scan under the Map and Profile is the next step): `ScansPanel.svelte`, `ScanViewer.svelte`, `lib/scans.svelte.ts`, `lib/scan-tree.ts`, and the `cave-scans` crate. The scans are the person's own files and stay on their machine (web-first, local-first): a folder is picked with the browser's folder picker (Chromium; the handle is remembered in IndexedDB and offered again with one click, because the browser asks before reading it again), or files are dropped on the panel. They are read in the browser and shown from local object URLs; nothing is uploaded and no picture goes into the cave or a shared room. What the cave stores is only the link: `Field::TripScans`, a trip's scan paths relative to the folder, one per line, edited through `cave_scans::link`/`unlink` (paths that are absolute, climb with `..`, carry a drive letter or control characters are refused, so a link can never point outside the folder and never reveals where on the machine the folder is). Links are an ordinary document field, so they are undoable in the panel's own `scans` scope, merge like any edit, and round-trip through the DXF (tested). The Notebook and the Scans panel share one current trip (`cave.trip`): turning the notebook to a trip brings up its first linked scan (unless what is showing already belongs to it), and picking a scan linked to exactly one trip turns the notebook to it. The viewer zooms about the pointer (wheel, pinch), pans by drag, fits on double-click or `0`, and turns a quarter at a time (view only, not stored); images only render in it, a PDF is handed to the browser's own viewer. The tree sorts as people read (`p2` before `p10`), folds folders, filters by name, and marks scans with the number of the trip they are linked to. Not built: station marks on a scan (click a station on the scan, as the old viewer allowed), PDF to page split, scan "complete" marks, thumbnails, aligning a scan under the Map/Profile (the old Sketch Scans insert-and-align), trimming a scan to its sketch.

**Scans aligned under the Map** (2026-10-12; Nathan: "align scans under the map so I can trace over them"): a linked scan can be placed under the Map by matching spots on it to the stations they are. In the Scans panel, **Align under map** turns on matching: click a spot on the scan that can be found on the Map (a station mark), then click that station on the Map (or choose it in the list); the spot shows on the scan as a pin with the station's name, and the list of matches can be pruned. Two matches place the scan (move, turn, resize; never a reflection: `cave_scans::fit` works with y up on the scan so a turn is a turn); three or more also **stretch and skew** to take out scanner distortion (default, as the old Align Image did; "Stretch to fit" off keeps the shape, a least-squares similarity). Points in a line fall back to the rigid fit. The panel says how far the matched spots land from their stations on average. What is stored is only the matches (`Field::TripScanAlign`, per trip, scan path to mode and `px,py,station;...`), never the resulting transform: the placement is derived every time from where the stations are NOW, so a scan follows loop-closure adjustment and revisions (tested: the same matches place a scan differently when the stations move). Unlinking a scan clears its matches. Matches are ordinary edits in the `scans` undo scope, merge like any edit and round-trip through the DXF (tested). The Map draws each placed scan as a textured quad (`CaveView.setScans`): drawn first and without depth so the opaque passage surface never hides it, at the strength set in the Scans panel (default half), under the survey lines; "Show under map" and "Strength" live in the Scans panel; a scan hides with its trip when the trip filter hides it, and only scans in the open folder are drawn. The picture itself still never leaves the machine or enters the cave. Not built: aligning under the Profile or a Cross Section, a scan placed by hand without stations (drag, turn), a one-spot placement, and clicking on the Map to pick the spot first (the order is scan first, then station).

**Trimming a scan to a traced outline** (2026-10-12; Nathan: trim the excess scan to a traced line so the views are less cluttered): in the Scans panel, **Trim to outline** lets the person drag a loop round the sketch to keep (a hand-drawn line, thinned by `cave_scans::simplify`, Ramer-Douglas-Peucker with an explicit stack, never more than 600 points and never straying more than 1.5 pixels from what was drawn), or click its corners one by one and close the outline (Enter, Backspace takes a corner back, Esc cancels). The outline is stored per trip and scan as pixels from the scan's top left (`Field::TripScanTrim`), never the trimmed picture: the scan file is untouched, the trim is an ordinary undoable edit in the `scans` scope, merges, round-trips through the DXF, and goes away when the scan is unlinked (all tested). A scan with an outline shows dimmed outside it in the viewer, and the Map draws only the part inside: the picture is clipped on a canvas to the outline's bounding box (shrunk to at most 4096 px) so nothing outside it costs memory or clutter. A scan has one outline, so a page with several sketches that need separate placements cannot be split yet. Not built: several outlines on one scan, editing an outline's corners after closing it, a soft edge, trimming under Profile or Cross Section (scans are not shown there yet).

**Scans in the Profile and Cross Section** (2026-10-12): a scan can now be placed in each drawing, not only the Map. Placement and trim are kept per **view** (`map`, `profile:<run>`, `section:<station id>`; the record key is `<view>|<path>` in `TripScanAlign` and `TripScanTrim`), because each drawing has its own frame (the Profile is along the run and elevation, a Cross Section is across and up from its station) and one field-book page often serves several (a plan sketch and cross sections on the same page). A match is now either a **station** (follows the survey: in the Profile it is that station's place on the band, on the Map its position, in a Cross Section only the station itself, at the origin) or a **point** in the view's own metres, clicked in the view, for what no station marks (a wall point on a section, a spot between stations). In the Scans panel, "Place in" picks Map, the Profile's run on show, or the Cross Section's station on show; the match step is the same (click a spot on the scan, then the place: a station on the Map, or any place in the Profile or Cross Section, or choose a station from the list). Both 2D views draw their placed scans first, under everything, with the same fit, strength and trim as the Map (`lib/scan-draw.ts`, the six numbers that take a scan pixel to the screen, tested). Scans hide with their trip's filter. Unlinking a scan clears its matches and trims in every view (tested), and everything round-trips through the DXF (tested). By default the Scans panel now opens below the Profile instead of tabbed with it: aligning needs the scan and the view side by side. Not built: a scan under the 3D tube, and placing a scan in a view by dragging it.

**Scans folder where the directory picker is refused** (2026-10-12; Nathan could not add a scans folder in the hosted copy): the browser's directory picker (`showDirectoryPicker`) is blocked inside a frame, which is how the hosted artifact runs. "Open folder" now uses it only when the page is the top window; otherwise (or if it is refused) it opens the plain folder input (`<input webkitdirectory>`), which every browser has and which works in a frame, with the folder's own name stripped so paths start inside it. It cannot be remembered for next time. Dropping a whole **folder** on the panel now works too (it is walked at any depth with `webkitGetAsEntry`; one dropped folder becomes the scans folder), as well as loose files. Checked with stand-ins for the browser's chooser and for dropped entries; the real native chooser is for a person to try.

**A cave from nothing** (2026-10-12; Nathan: the workflow needs to work from nothing and build a cave up from the scans): the Home tab has **New** (first in the File group). It asks for the cave's name and the unit the notes are written in (feet or metres), starts a cave with one empty trip (`cave_notebook::new_cave`; the name is kept on that trip as `TripCaveName`, the unit as the trip's distance unit), and arranges the workspace for the job (`workLayout`): Scans on the left (38% wide), the Notebook beside it, the Map small underneath with the Profile and Cross Section behind it; the other panels come back from the View tab. The display unit follows the choice (feet makes the app imperial) so numbers are typed exactly as the notes read. The new cave counts as unsaved from the first moment (`markUnsaved`). The way of working, with no automatic reading of handwriting (the scans are for a person to read): open the scans folder, pick the first page, type the first shot's stations in the Notebook's From and To (From starts at `A1` on an empty page, To is the next name in the sequence and Enter adds the shot), and the cursor lands in the new shot's tape; Tab goes tape, compass, clino, then the station's L R U D and note, and the add bar's To is the next stop. Every view answers for an empty cave and for a cave with unmeasured shots (tested: Map scene, runs, profile, sections, station table, passage mesh), and a brand-new cave saves and opens again (tested). Link the scan to the trip (Link to trip) to keep a page with its trip. Not built: reading a page for the person (OCR or a vision model), starting a trip's date and team from the scan, several scans to one cave at once with drag to order.

**Complete / Incomplete marks on scans** (2026-10-12; Nathan: right-click each scan to say what has been touched and what needs attention): right-click any page in the Scans tree for "Mark complete" / "Mark incomplete"; right-click a folder for "Mark all complete" / "Mark all incomplete" (everything inside it, at every level). Pages are incomplete until marked. A complete page shows a check and is dimmed; each folder shows how many of its pages are complete (`3/5`) and gets a check when all are; a **Hide complete** box in the panel drops complete pages and finished folders so only what needs attention is left, with the overall count beside it. The mark is stored in the cave, not the browser, so it is shared in a room, undoable in the Scans panel's history and saved with the DXF: a new entity kind `Scan` (`ScanPath`, `ScanComplete`) whose id is derived from the path (`cave_scans::scan_id`), so two people marking the same page write to the same record and agree (tested), and marking a page that was never marked incomplete writes nothing. A folder has no mark of its own: it is complete when everything in it is, so a later page added to the folder makes it incomplete again. Marks are kept by path, so they apply when the same folder is opened again on another machine. Not built: other states (in progress, needs checking), marking from the viewer or the keyboard, marking a page complete automatically once it is linked and typed.

**The scans folder survives a refresh** (2026-10-12; Nathan: the scans folder was getting lost on refreshes): the browser's folder handle could only be remembered where the directory picker works (not in the hosted frame) and still needed a click, so a refresh dropped the pages. Now, whenever a scans folder is opened (picker, plain folder input, or drop), this browser **keeps a copy of the pages** in IndexedDB (store `files`, path to the page, plus a `copy` record with the folder name and size); on start-up (`App.svelte`, before any panel is open, so the Map's scans draw straight away) the copy is brought back with no click. The panel says "Kept from last time · N MB" with a "Forget it" link, and "Copy kept in this browser" after a fresh open. It stays on this machine and never goes to the cave or a room. A copy is a snapshot: change the folder on disk and open it again to refresh (the old remembered-folder "Reopen" button still does that where the picker works). If the browser has no room, the pages work for the visit and the panel says they will need opening again. Only one folder is kept at a time. Not built: autosaving the cave itself to the browser (an unsaved cave is still lost on refresh), keeping several scans folders, and a size limit or clean-up of old copies.

**The `Trimmed` folder is for the views** (2026-10-12; Nathan: trimmed scans live in `/Trimmed` and are only for the view panels): any page inside a folder named `Trimmed` (case does not matter, at any depth) is no longer listed, counted, marked or filtered in the Scans panel, and the folder itself does not appear (`isDerived` in `scan-tree.ts`, tested). The pages are still read and still kept in the browser's copy, so a view that wants a trimmed page can draw it; they are just not the person's own scans. Folder counts, "Hide complete", "Mark all complete" and expand/collapse all ignore them. Only a folder named exactly `Trimmed` counts (`Trimmed Notes` is an ordinary folder). Note this app trims by an outline stored in the cave (see "Trimming a scan"), not by writing files, so nothing here creates a `Trimmed` folder; the rule is for folders brought over from the old app.

**Reveal in Finder / Explorer, and a resizable scan tree** (2026-10-12; Nathan: "Reveal in Finder" on the right-click menu, taking him to the real scan and not the browser's copy; and the tree must be resizable). A web page cannot open Finder or Explorer, and the browser never says where a picked folder lives, so there is no one-click reveal in the browser (a desktop shell, the Tauri decision, could do it natively). What the menu does instead: **Reveal in Finder** (**Show in Explorer** on Windows) asks once where the scans folder lives on this computer (the person pastes the folder's full path; the box says how to copy it in Finder or Explorer), keeps that path in this browser only (`localStorage`, per folder name; never in the cave or a room: it is a path on one machine), and from then on every page and folder gives its full real path (`scan-location.ts`: the separator follows the pasted path, so Windows paths stay Windows paths), copied to the clipboard when the browser allows it and always shown to copy by hand, with the one step to open it (Finder: Shift+Cmd+G, paste, Return, which opens the folder with the page selected; Explorer: paste into the address bar). It never points at the cache the browser keeps. "Change folder location" fixes a wrong or moved path. The tree is resized by dragging the divider between it and the viewer (120 px up to nearly the whole panel), by the arrow keys on the divider, and double-click resets it; the width is remembered on this machine.

**The desktop app, and Reveal without a question** (2026-10-12; Nathan: do not ask for the scans folder's path, capture it when the folder is picked, and he chose to build the desktop app): a browser never says where a picked folder lives, so capturing the path at pick time is only possible in a native window. `apps/desktop` is the Tauri shell the 2026-10-11 decision called for: it loads the same `apps/web` build, and adds four commands (`pick_folder` through the system dialog, giving the real path; `list_scans`; `read_scan` for one page's bytes; `reveal`, which opens Finder or Explorer on the real file via the opener plugin). Every command that takes a path goes through `files::safe_join` (no absolute paths, no `..`, tested), reads only pictures and PDFs, and a page is never read from outside the chosen folder. In the web app `lib/desktop.ts` is the only door: inside the desktop app Open folder uses the system dialog, keeps the real path (remembered, so the folder is read fresh from disk on every start and a changed page is the changed page: no browser copy is kept), and Reveal opens the file manager on the real page with no question; in a browser nothing changes (the hosted copy keeps the paste-the-path fallback and the browser's kept copy). The desktop crate is its own workspace so the engine still builds for the browser. Built and launched on macOS (debug bundle; the app opened and the web app ran inside it); the system folder dialog and the Finder reveal themselves need a person to try, because macOS dialogs cannot be driven from the tools used here. Not built: signing or notarising the app (it opens only on the machine that built it until then), Windows and Linux builds, the engine running natively instead of as WASM, dropping a folder onto the desktop window (the native drop is turned off so the browser drop keeps working, without a path), an updater, and installing the app anywhere.

**The Cave Shelf, and a clean slate** (2026-10-12; Nathan: the app starts clean slate, with a startup window like the old Cave Shelf that asks which cave to open, adds a trip to the selected cave, among other things; read the old one and replicate it): the old `CaveShelf` (a window that replaced the auto-created template drawing: caves in a left rail, the selected cave's trip table and open ends on the right, Open Drawing, New Trip from the open end, New Cave, Import, "Open this window at startup", a right-click menu with Favorite, Open Project Folder, Forget) is the model. What the new app does: nothing opens by itself (the Map no longer loads a demo; every view answers for no cave), and the shelf (`CaveShelf.svelte`) opens first unless someone sent a room invite. **Left rail:** the caves this browser keeps (favourites first, then the most recently used; each line says trips and length, and "open now"), a search box, the four test caves under **Samples**, and **New cave…** and **Import file…**. **Right pane:** the cave's name, length, stations, trips, depth and how well its loops closed, when it was last kept, its **open ends** as chips (pick one to start a trip there; the newest is chosen for you), and the **trip table** (Trip, Date, Decl, Team, Shots, From, To, Ends at; counted from the survey, double-click a trip to open the cave at its page). **Buttons:** Open, New trip (or "New trip from D10": opens the cave, starts a trip, puts the Notebook's From at that station and lays out scans beside the notebook), Close (or Start empty before any cave is open), and the startup checkbox. **Right-click a cave:** Open, New trip, Favorite, Download a file…, Forget… (it deletes only the copy this browser keeps). The summary is the new `cave-shelf` crate (`summarize`, tested on Pitfall Cave, an empty cave and Plumbline Pit): worked out from the cave, kept with the record so the list needs no reading, and `shelf_summarize` reads a stored cave without opening it. **Where caves live:** a cave is kept as its DXF text in IndexedDB on this machine, a new cave is kept at once, a file opened or imported is kept (the same file again updates its record), and an open cave is kept again about a second after each edit and when the page is hidden, so a refresh or closing the tab loses nothing; samples are working copies and are not kept unless saved. The unsaved dot now means "not yet kept". **Differences from the old shelf:** no folder registry (the old app listed project folders on disk; the browser cannot, so the shelf is the browser's own store, and the desktop app can later add real folders); no Package, Open Project Folder or thumbnail yet; trips are read-only in the table (the old table let name, date and team be typed; here the Notebook's trip header is where they are edited); Import takes DXF files saved by CaveCAD (the old Compass, Survex, Walls and CSV importers are not ported). Not built: those, and renaming a cave from the shelf.

**Peer-to-peer live editing, no server** (2026-10-12; Nathan: build it with the paste-a-code step, eliminate server dependence, it is the flagship feature; and the code box belongs in the Cave Shelf so a person can tune into a session in progress and receive the host's files and visuals): two devices connect straight to each other with WebRTC data channels (`p2p.svelte.ts`); no relay, no account, no host to run. **Introducing two devices** is the only step that needs a human: the host makes an **invite code** (`CAVE1-…`, about 700 characters: a compressed connection description, `p2p-code.ts`) and sends it in any message; the guest pastes it in the **Cave Shelf** ("Join a live session…" under the cave list) and gets a **reply code** to send back; the host pastes that in the **Share live** window (ribbon, Live editing group) and the two connect by themselves. **The host asks before letting anyone in** ("Let in" / "Turn away", or a checkbox to admit without asking), so a code that leaks does not let a stranger into the cave. **What travels:** the host's whole cave on joining (the guest's cave is replaced by the host's; what the guest had open is kept on their shelf first), then every edit in both directions (the host is only the hub that forwards: edits merge in any order, so there is no referee), each person's pointer and what they are drawing (the same presence the old room showed), and **what the host is looking at** (trip, run, station, scan page), which a guest follows unless they untick "Follow what the host is looking at". **Scan pages come across on request:** if the host shows a page the guest does not have, the guest asks and the host sends the picture itself over the same channel (in 16 KB pieces with back-pressure, `p2p-wire.ts` cuts and rejoins big messages); the page is kept for that visit only and is not a folder on the guest's machine. A guest who loses the host keeps their copy and is told; joining again with new codes merges the two copies (the guest sends what it made while away, the host replies with its own). **Connecting across networks** uses a public STUN lookup so each device learns its public address (checkbox, on by default; no cave data goes through it; off means same-network only); direct connections fail on some strict networks, which would need a relay (not built). A code contains a device's network address, so it is for the one person it is sent to; everything between devices is encrypted by WebRTC. The old server rooms (`npm run dev` relay) still exist in the account menu for development. **Checked:** codes and framing in `npm run test:p2p` (squeezing, forgiving paste, junk refused, 100 KB messages through frames, interleaving, a dropped frame); and real WebRTC between two copies of the app in one browser: invite, reply, approval, the host's cave arriving, an edit each way within a second, pointers, the host's trip and a scan page the guest did not have arriving byte for byte and being shown, and the host leaving. Two separate browser tabs in the testing browser could not connect (its network sandbox), so the first test between two real devices is for a person. Not built: more than the host as hub (a guest cannot reach another guest directly), a relay for networks that block direct connections, an invite link instead of two codes, and the host's own camera/zoom position (only trip, run, station and scan are followed).

**A Live category on the Home tab** (2026-10-12; Nathan: there was no way to start a live session once a cave is open): the Home ribbon has a **Live** group with **Share** (opens the Share live window: Start sharing, invite codes, who is in), **Join** (opens the Cave Shelf straight on its Join a live session page) and **Leave**; Share and Join light up while that device is hosting or has joined. The View tab's group stays, and the Cave Shelf's own Join button is the same page.

## 2026-10-12 — Relay (TURN) fallback for live sessions
Direct WebRTC fails on symmetric NAT / CGNAT / strict firewalls. Added an optional relay: the person pastes a Metered "credentials link" (or turn addresses + username + password) under "Relay for strict networks" in Share and Join. Stored only in localStorage (`cavecad-relay`), never committed or put in a code. `p2p.svelte.ts` `iceServers()` = STUN + relay servers; ICE still tries direct first, the relay carries only DTLS-encrypted data. "Test relay" gathers with `iceTransportPolicy: 'relay'` to prove the login works. Parsing is in `p2p-ice.ts` (tested in `tests/p2p.check.ts`). Real two-network verification needs real credentials.

## 2026-10-12 — Built-in relay link
One shared Metered account serves everyone: the credentials link (`https://<app>.metered.live/api/v1/turn/credentials?apiKey=…`) is a repository secret `RELAY_URL`, passed to the Pages and desktop builds as `VITE_RELAY_URL`. Never committed. Trade-off accepted: the link ends up inside the shipped JS, so anyone who looks can read it; worst case is someone burning the free quota, fixed by rotating the key. The per-person Relay settings stay as an override.

## 2026-10-12 — CaveCAD's own file; DXF is an export
Saving to DXF lost what a drawing has no slot for. A cave is now saved as `.cavecad`: a zip with `manifest.json` (format, `version`, app build, scan list), `cave.json` (the whole document's ops, from `engine.syncSnapshot()`, so nothing is squeezed) and `scans/<path>` (the scan pages the cave links, on by default, toggle "Scans in file"). Readers ignore unknown parts and refuse a newer `version` with a reason. The Cave Shelf keeps the same native JSON (old DXF records still open). Open reads `.cavecad`, a project zip (finds the cave inside), `.dxf` and the old importer's `.json`. Export group: DXF, and Project (zip: the cave, its DXF, and every page in the scans folder). Code: `lib/cavefile.ts`, `lib/unzip.ts`, `lib/files.ts`; tests `npm run test:cavefile`; the engine round trip (open, save native, load) was checked identical on the sample cave.

## 2026-10-12 — Scans sync in live sessions; scans lie at their stations' height
Live sessions now exchange scan pages for every scan linked in the cave, both ways (`scanBeat` in `p2p.svelte.ts`, every 2.5 s): each device announces the linked pages it holds (`has`), asks for missing ones (`want`), guests ask the host, and the host fetches from another guest when it does not hold a page itself and passes it on. Requests that go unanswered are asked again after 30 s. Verified with two devices, a page added on each side reached the other. The Map lays each scan flat at the average height of the stations its matches are (`Placed.stations`), 0.3 m under them, instead of under the whole survey's lowest point.

**Classroom** (2026-10-13, branch `classroom`; Nathan: "Start a Classroom", invitees are observers, the teacher's interface takes over theirs, their pointers stay visible, a hand-raise button asks questions into a logged chat): a way to teach a cave live.
- **Roles.** The teacher starts a class (Home > Live > Classroom): a room on the relay made with `mode: 'class'`, so its owner is the teacher and everyone else is a student. The teacher gets a link (`?class=ROOM&invite=TOKEN`, good for 12 hours, any number of students); a student opens it, types a name, and joins as an observer. A student's link can never start a room (`joinOnly`), and a room name that already exists is refused for a teacher before its cave can replace theirs. Students cannot join before the teacher has shared the cave.
- **Students only watch, enforced three times.** The relay drops a student's ops (and says so); the engine refuses every write while `readOnly.on` (`lib/readonly.ts`: every `engine` method must be listed as a read or a write, and a test fails when a new one is not); and the page ignores clicks, keys, wheel and drops outside the class's own drawer (`classroom.svelte.ts`, capture-phase listeners; pointer movement is left alone so pointers can be reported). A student's copy of the cave is not kept on the shelf.
- **The teacher's interface takes over.** `lib/classroom-mirror.ts` is a registry of "what am I looking at" parts, each with a reader and a writer: the ribbon's tab, the panel arrangement (rebuilt only when the arrangement changes; sizes and the tab in front are matched without rebuilding), the Map's camera (`CaveView.getCamera/setCamera`: target, direction, how much it shows, so a window of another shape shows at least the same), the Profile and Cross Section (the world rectangle on show, fitted into the student's window), and the trip, run, station, tool, selection, units, filters and options. The teacher's page reads them every 150 ms and sends only what changed; the relay keeps the latest, so a latecomer is given it first. A part that mounts after the state arrived is given it at once. Not mirrored: the scan viewer's zoom (scan pages are not on a student's machine), notebook scroll, anything in a dialog.
- **Pointers.** Students' pointers are sent to the teacher only (not to each other), through the same cursor reports as before, so the teacher sees each student's pointer, with their name, in whichever view they are in; the teacher's pointer goes to everyone. Because a student's views are the teacher's, the two pointers are in the same place in the cave.
- **Hands and chat, logged.** A student raises or lowers a hand (a queue, longest first, shown to everyone with the time waiting); the teacher calls on a student, which lowers it. Chat is one line at a time (500 characters, a student once every 0.7 s). Every line and every hand event is in the class's record: kept by the relay in memory, appended to `classroom-logs/<class>-<time>.jsonl` when the server has a log folder (the dev server does; the folder is git-ignored), backfilled to anyone who joins late, and downloadable by the teacher as a text file. Ending the class stops chat and clears hands; the record stays. Do not put a cave's entrance location in the chat (the record is kept).
- **Two ways to carry a class, one set of rules** (Nathan: the interface can sync over p2p just like the cave does). The rules (who may say what, the hand queue, the record, the teacher's view) are `lib/classroom-room.mjs`, with no network in it; `scripts/classroom-relay.mjs` and `p2p.svelte.ts` each only reach the people. *On the relay* (dev or Docker server's `/sync`): one link for the whole class, any number of students, the record also appended to a file on the server; the hosted static build has no relay. *Peer to peer*: the teacher's page is the hub (`startClassHosting`), students come in with the usual invite and reply codes and are let in at once, a guest's edits are refused by the host, a guest's pointer is kept for the teacher only (not forwarded to other guests), and chat, hands and the view travel as the same messages over the data channel; there is no server, so no log file, but the teacher downloads the record as before. The dialog offers both (the hosted copy defaults to direct). A student can join a p2p class from the Cave Shelf's Join like any guest: the welcome says it is a class, and the page becomes read-only and mirrored.
- **Not built:** letting the teacher release students to look around on their own, handing a student the pen for a while, a student's pointer in the scan viewer, breakout groups, saving the transcript into the cave file, the Docker server's own log folder setting, a student's reply code made easier than pasting (one link per student for direct classes).

**A link, then direct** (2026-10-13, Nathan: a shareable link is the ideal way to start the handshake; use the CaveCAD Gmail account's Apps Script, as the feedback receiver does): the third way to carry a class. A direct (WebRTC) connection needs each side's offer or answer swapped, which two pages that have never met cannot do themselves; a link can hold only the session id, so a **signaling mailbox** (`tools/signal-mailbox`, an Apps Script under cavecad.app@gmail.com, its own project) passes the first notes and then drops out.
- **The flow** (`lib/signal-handshake.ts`, pure, over a `Mailbox`): the link is `?live=SESSION`, the same for every student. A student's page leaves `hello:ME`; the teacher's page (polling every 1.5 s) makes an invite of its own for that student with the existing p2p code, leaves `offer:ME`, and the student's page, which has been polling for it, answers it with the existing `joinWithCode` and leaves `answer:ME`, which the teacher's page hands to `takeReply`. From then on it is the p2p class of before (the teacher's page is the hub, students only watch). A class that arrives together is handled together, and an offer that fails to be made is tried again on the next pass.
- **The mailbox only ever holds those three kinds of note** (names are checked against `hello|offer|answer:ID`), each at most 12 kB, in the script cache for 30 minutes under an unguessable session id, behind the same sort of build-time key as the feedback receiver, with a per-minute cap. It never sees a cave, chat or view: only each device's network addresses and encryption fingerprint, as in any video call. The decisions are in `Logic.gs` and are tested in node with a fake cache, and the browser client is tested against that same `Logic.gs` (`tests/signal.check.ts` runs a three-student handshake through the real script logic behind a fake `fetch`).
- **Where the address comes from:** `VITE_SIGNAL_ENDPOINT` and `VITE_SIGNAL_KEY` (repository secrets `SIGNAL_ENDPOINT`, `SIGNAL_KEY`, passed to the Pages and desktop builds), or one typed into the Start a Classroom dialog, kept on that machine; a locally set mailbox travels inside the link (`mb`, `mk`) so students' copies, built without one, can use it too.
- **Strict networks:** the connection falls back on the TURN relay already configured (Metered), exactly as for any p2p session; the mailbox does not carry data.
- **Not built:** emailing each student their link from the account's Gmail (a roster; consumer Gmail sends about 100 a day), a mailbox on our own server (`/signal`), a faster handshake by trickling ICE (a student waits about five to ten seconds).

**The invite link is for every live session, not only classes** (Nathan: "it needs to work for ALL multiuser invites"). Nothing in the handshake (`signal-handshake.ts`, the mailbox) knows about classes: it introduces a page to a host that is already hosting, and what the guest then is (an editor in an ordinary shared session, an observer in a class) is decided by the host's welcome, as before. The wiring is now general: `startLinkSharing()` in `p2p.svelte.ts` makes one link (`?live=SESSION`) for whatever this page hosts, starting to host first if needed, and Share live (Home > Live) has an "Invite by link" box with Copy beside the codes. People who open it appear in the host's list as "Wants to join" unless "Let people in without asking" is on, exactly as for codes; a class turns that on and makes them watchers. The join page a link opens is neutral until the welcome arrives ("Join the session"; the observer wording only for a class link made by a teacher), and clears itself when a plain session welcomes the guest. The mailbox address and key fields are one shared component (`MailboxSettings.svelte`) used in Share live and Start a Classroom. Relay rooms keep their own link (`?join=ROOM&invite=TOKEN`, Avatar menu), which already served any room.
- **Not built:** a notice to the host when someone opens the link while the Share dialog is closed (they sit as "Wants to join" until the host looks), turning the link off without stopping the session.

## 2026-10-13 — The link is the only way to go live
The invite-code / reply-code handshake is gone from every screen. Sharing is one link per session: the Share dialog makes it (`startLinkSharing`), anyone opens it or pastes it (Cave Shelf > Join a live session > paste the link, `openLinkText` in `classroom.svelte.ts`; `readJoinLink` accepts a whole address, the part after the `?`, or the bare session code) and then types a name; the page leaves a hello in the mailbox (`tools/signal-mailbox`), the host answers, and the devices connect directly. The code machinery in `p2p.svelte.ts` (`makeInvite`, `takeReply`, `joinWithCode`) remains only as the engine under the link. Classes use the same link. The mailbox address and key come from the build (`VITE_SIGNAL_ENDPOINT`, `VITE_SIGNAL_KEY`, from GitHub secrets `SIGNAL_ENDPOINT` / `SIGNAL_KEY`) or are typed once per machine; for local work they sit in `apps/web/.env.local` (gitignored).

## 2026-10-13 — Speed before WAN
Measured with two windows and real mailbox. (1) Classroom: the teacher sends only the parts of the view that changed (`sentParts`), and a student re-reads the cave (`changed()`) only when something that alters what is drawn changed; a camera move no longer wakes every panel. (2) Wire (`p2p-wire.ts`): frames are `id.index.count.text` (no JSON-inside-JSON escaping), long messages (> 1500 chars) are deflated and sent as `~` + base64 (the sample cave's welcome went from 246 KB to about 20 KB), and a broadcast is stringified and squeezed once for all peers. Incoming messages are handled in order even when one needs a moment to open (`inOrder` in `wire()`), so scan-page pieces still follow their header. A frame in the older JSON envelope is still read; a device on an older build cannot read the new frames, so update both ends together. (3) Connection setup: `gathered()` stops once the asked-for kinds of address (public, relay) have arrived and 350 ms are quiet, instead of waiting for the browser to say done; relay credentials are cached for 5 minutes; the host polls the mailbox every second. A join dropped from about 10 s to about 5 s. (4) Scan pages are requested two at a time with the page on show first. (5) Joins and leaves are lines in the class record; a raised hand or an emoji raises a card for the teacher.

## 2026-10-13 — A link can open the desktop app
The desktop app registers the `cavecad://` scheme (tauri-plugin-deep-link; the bundle's Info.plist lists it; Windows/Linux register it on install or first run) and is single-instance: a second launch by a link hands its link to the app already open (`tauri-plugin-single-instance` with `deep-link`) and exits. Rust keeps links that arrive before the page is ready (`Links`, `take_links`) and also calls `window.__cavecadLink(url)`; `listenForLinks` in `desktop.ts` wires that to `openJoinUrl`, which reads `cavecad://join?live=...` with the same `readJoinLink` as a web link and then follows the usual path (joined at once under the saved name, else asks for a name). A join link has two forms: the web link (hosted copy; built from the hosted address when the host is the desktop app, `share-base.ts`) and the `cavecad://` link; the Share dialog and class dock show both, and the web join page offers "Open in the desktop app". A browser cannot know whether the app is installed, so that is a link the person chooses to press. Not verified here: actually opening the installed app from a link (the registered scheme was checked in the built `Info.plist`).

## 2026-10-13 — A class can keep the cave off students' computers
Starting a class has an option, on by default: "Students cannot keep the cave" (`private` on the class, carried to each student in the class welcome as `private`; a class from an older build counts as private). For a private class a student's page refuses to save, export (DXF, project) or print (`readOnly.noKeep`, checked in `files.ts` and `sheet-export.ts`), never stores to the shelf, and when they leave, the connection drops or the class ends (at once, with a notice; the chat stays) it lets go of everything: the cave is replaced by an empty one, the scan pages and their URLs are cleared (`clearScans`), the view state is forgotten and the page is as at the start (`wipeStudentCave`). Checked: during the class a student holds the cave (71 stations, a scan) with save/export refused and nothing in the shelf; after leaving the engine has no stations, trips or scan links, no scan paths, and localStorage holds only the layout and name. A class started with the option off lets students save and leaves their copy. Cannot be prevented: a screenshot or photo, or someone reading the page's memory while the class runs.

## 2026-10-13 — Per-line linetype, linetype scale and hatch density; areas selectable by their hatch
A selected line's contextual tab ("Edit …") has a Line style group: a linetype (the type's own, solid, dashed, long dash, sparse, dash-dot), a **Linetype scale** (×0.25 to ×4, multiplies the dashes and gaps), and for an area a **Hatch density** (×0.25 to ×4: scatter marks, hatch lines or dots per area), with Reset. Stored on the line as `FeatureStyle` = `linetype;scale;density` (empty is the type's look; `cave-draw::normalize_style` checks and fixes the form), so it is saved in the file, syncs in a live session, and is one undoable edit in the view the line was drawn in. Dragging a slider shows the change at once in every view (`liveStyle`) and saves on release. Density is `cave_fill::marks_with(..., amount)`; the 6,000-mark cap still applies to a very large area. Dashes are still pixel patterns, not yet tied to a drawing scale (the agreed next step: a drawing scale and per-type patterns in paper millimetres). Clicking anywhere inside an area now selects it (a wall or symbol over it still wins), in the Map, Profile and Cross Section; and a line can be selected in the Map from any camera angle (moving its points still needs the top view).

## 2026-10-13 — Drawing scale: patterns and fills sized on paper
The drawing has a scale, 1:N (default 1:500), kept as a `Setting` entity (`Kind::Setting`, id from the name `drawing-scale`, so it is saved in the file, syncs in a session and follows the same rules as everything else; `cave-draw::drawing_scale / set_drawing_scale`). Linetype patterns are now defined **on paper in millimetres** (`Linetype::paper_mm`; dashed 2.0 on / 1.2 off, long dash 4.8 / 1.0, sparse 1.2 / 3.4, dash-dot 2.8 / 1.2 / dot / 1.2; a dot is `DOT_MM` = 0.4) and sent to the web with each type (`Style.pattern`) and in `linetypes()`. A view turns a pattern into dashes with `dash.ts`: millimetres × pixels per paper millimetre, which is pixels per metre of ground × N / 1000, times the line's own Linetype scale; a pattern too fine to see is drawn solid and a huge magnification is held to what a canvas draws. The survey's own lines in the 3D Map use the same pattern in metres (`dashMetres`). A printed sheet uses `looks({ pxMm })`: a millimetre on paper is a millimetre whatever the ground. Fills are laid out as authored for 1:500 (`BASE_SCALE`) on an outline `length` = N/500 times smaller and then everything is made that many times larger (`cave_fill::marks_at`), so marks and hatch keep their size and spacing on paper at any scale. The View tab has the control (presets 1:100 to 1:5000 or any number from 10 to 100,000); changing it redraws every view, reaches students, and a class student cannot change it. At 1:500 an area fills exactly as before; dashes changed from fixed pixels to paper millimetres. Fill marks still stop at 6,000 per area. Not done: a per-type pattern editor (the patterns above are fixed), and printed sheets still lay fills out at the drawing's scale rather than the sheet's own.

## 2026-10-13 — A Leader object; shaped lines and bedrock
**Leader.** What was "Callout" (a note with a leader arrow) is now the **Leader** in its own **Annotation** group beside Text, no longer among the line types in Linework. (Its stored type is still `callout`, so old files open.) A leader keeps its own settings in the line's style text after the line part (`;arrow;shape;frame;size`, checked by `cave-draw::normalize_style`): arrowhead (arrow, filled, dot, none), line (elbow, straight, curved), note frame (none, box, underline) and text size (×1 is 3 mm on paper, so it is sized by the drawing scale, not by how far it was dragged, as before). They are set in its contextual tab with a live preview and one undoable edit, in every view; layout is `cave_shape::callout::layout_with`. Not built yet: several arrows on one note, and attaching a leader's tip to a station so it follows the survey.
**Shaped lines** (ledge, pit, flowstone, rimstone, slope, dripline, wall glyphs) are picked by any part of their marks, not only the spine: the editor takes an `ornament` hook and the Map computes the same marks it draws (`distanceToPolyline`).
**Bedrock** area fill now draws bedding lines with vertical joints, stepped a little row to row, the same marks as the rock halo (`cave_fill::bedrock`, `Pattern::Bedrock { cell }`); before it drew its outline and nothing inside. It scales with the drawing scale and the density control like the other fills.
