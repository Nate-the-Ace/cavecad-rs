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
