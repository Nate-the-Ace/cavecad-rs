# Surface Data and Entrance Location — design

Date: 2026-10-26. Branch: `surface-data`. Port of the QCAD-era Surface Data
(`Core/CsSurfaceData.js`, `SurfaceData/SurfaceData.js`) and Entrance Location
(`EntranceLocation/EntranceLocation.js`, `Core/CsLocationPick.js`, `Core/CsGeoProject.js`)
tools, read in full before this was written. Their comments record decisions kept below.

## Nathan's decisions (2026-10-26)

1. Same workflow as the old app: one Surface Data button, aerial photo and ground elevation,
   contours drawn from the elevation.
2. Entrance revision: **the person drags a circle** on the Map; the entrance snaps to the lowest
   ground inside it.
3. After every entrance move the surface is **downloaded again automatically** (no question).
4. Elevation comes from **USGS 3DEP only** for now. State sources (KyFromAbove and the like) later.
5. A cave with no location takes the **fixed point of an imported survey file** when there is one,
   otherwise asks with the map picker.
6. **One common world frame** for every cave, so a later build can show many caves of an area
   together. The multi-cave view itself is not in this build.

## Coordinate frames

- **World (stored):** WGS84 latitude/longitude in degrees, heights in metres NAVD88. This is the only
  frame a location is stored in. Any two caves are related through it.
- **Cave (derived):** each cave's survey stays in its own metres, solved from its shots, true north
  up, z up. Nothing about a cave's survey changes when its location is set or revised.
- **Surface (per download):** the photo and elevation grid are requested in the **UTM zone of the
  entrance** (WGS84 UTM, EPSG 326xx/327xx). UTM is true scale to 0.04% within a zone, so pixels are
  square metres on the ground. The old app used Web Mercator, which stretches distances by
  1/cos(latitude) (about 25% in Kentucky); that is why it had to rescale everything. A later area
  view of many caves uses the same zone rule.
- **Placing the surface under a cave:** the entrance station's solved position is tied to its stored
  latitude/longitude. A point of the grid is turned to latitude/longitude (UTM inverse), then into the
  cave frame as east/north metres from the entrance, rotated by the UTM grid convergence at the
  entrance (grid north is not true north; up to a few degrees at a zone edge). The pure math lives in a
  new core crate (below) and is tested against hand-checked points.

Imported fixed points arrive in the importer's metres with no record of their coordinate system.
This build adds that record: the importers write the fixed points' coordinate system (UTM zone and
**datum**) to a cave setting. Compass and Walls name NAD27 or NAD83; NAD27 points in Kentucky sit
10–30 m from WGS84. NAD27 is shifted to WGS84 with the standard Molodensky transform (metres-level,
which the entrance revision then corrects); NAD83 is taken as WGS84 (sub-metre difference). A
coordinate system CaveCAD cannot convert is not used as a location: the map picker asks instead.

## What is stored (source)

All in the cave document, as edits (undoable, synced like everything else). Two `Setting` entities,
each one value written whole, so the parts of a location can never be half-updated by two people:

- **`location`**: `{station, lat, lon, ground_m, source}`.
  - `station`: the entrance station's **id** (not its label). Default: the station labelled A1
    (every cave entrance is A1 by convention), else the fixed station from the import, else the person
    picks one.
  - `lat`, `lon`: WGS84 degrees.
  - `ground_m`: raw 3DEP ground elevation at that point, metres NAVD88, or absent when the lookup
    failed. **Raw ground, not the entrance's own elevation**: how far an entrance sits below the
    ground is a display-time guess and is never stored. Absent means UNKNOWN; nothing may read it as 0
    (the old suite's elevation datum trap).
  - `source`: `import`, `typed`, `map`, or `surface-pick`, for the report and for trust.
- **`surface`**: what was downloaded (the same record as `surface/surface.json`, kept as a setting
  so edits and undo cover it):
  `{zone, bounds:{xmin,ymin,xmax,ymax}, photo:[w,h]?, elevation:[w,h]?, for_at:{lat,lon}, fetched}` in UTM
  metres of `zone` (`surface/surface.json` is this exact JSON). `for_at` is the location it was cut around; the 3D terrain and contours read
  `bounds`, **never recompute it** (the window grows as the cave is drawn; recomputing slides the
  hillside, measured in the old app).
- **`contour-interval`**: the contour spacing, stored in METRES (default 10 ft / 5 m); shown and
  typed in the display unit, rounded to 3 decimals so 10 ft stays 10 ft.

No survey `Elevation`, station position or fixed coordinate is rewritten by anything here.

## The surface files (part of the cave)

Nathan, 2026-10-26: the app is 3D from the start, so the surface belongs IN the `.cavecad` file. The
photo and elevation grid are part of the cave, saved and opened with it, exactly as linked scans
already are (`scans/<path>` parts, `lib/cavefile.ts`). The downloaded data is what the cave was
mapped against, so it is kept, not re-fetched: every copy of the file shows the same ground.

- **In the file:** `surface/photo.jpg`, `surface/elevation.tif`, and `surface/surface.json` (the
  request: zone, bounds, pixel sizes, service, download date). `manifest.json` lists them. A
  **"Surface in file"** toggle, on by default, mirrors "Scans in file". Readers that do not know the
  parts ignore them (the existing rule).
- **Sizes:** elevation 512x512 float32 is about 1 MB (deflated in the zip); the photo is requested as
  JPEG (quality 85), a few MB at 4000 px, instead of the old app's PNG (tens of MB).
- **In a cave folder:** the same files are also written to `surface/` beside the `.cavecad`, so other
  programs (QGIS) can open them. The file is still the cave; the folder copy is for convenience.
- **While open:** held in the browser's private storage (OPFS) keyed by cave id, so the Map and 3D
  view do not unzip on every draw. Files are keyed by their request, so undoing a re-download
  still finds the earlier files for the rest of the session; a save writes only the files the current
  `surface` setting names.
- **Server-synced caves:** the server keeps the cave's edits, not its files. A device that receives
  the cave from the server without its surface files downloads them from USGS using `surface.json`'s
  request, and says so. Uploading surface files to the server is for the later file-sync work, not
  this build.
- Requests go straight from the app to USGS (both services answer browser requests with
  `Access-Control-Allow-Origin: *`, checked 2026-10-26):
  - photo: `imagery.nationalmap.gov/.../USGSNAIPImagery/ImageServer/exportImage`, JPEG, native
    0.3 m, at most 4000 px a side;
  - elevation: `elevation.nationalmap.gov/.../3DEPElevation/ImageServer/exportImage`, float32 TIFF,
    native ~1 m, at most 512 px a side.
  Pixel size keeps one shared scale for both axes so the aspect is always right; where the 256 px
  floor and the cap clash, the floor yields (old `pixelSize` bug, regression test ported).
- Window: the cave's **plan** extent (stations, walls, map linework; never the profile, sheets or the
  surface itself) plus 25% margin, at least 150 m across.
- A reply that is not an image or a readable grid (USGS returns errors as JSON with status 200) is an
  error, never a placed surface. A grid with no data means "outside 3DEP coverage".

## The Surface Data button

1. Finds the location: stored `location`; else an imported fixed point (converted, above); else the
   map picker. Nothing touches the network before a location exists.
2. Checkboxes, both on: **Aerial photo**, **Ground elevation** (contours and 3D terrain come from it).
3. Downloads, stores the files with the cave, writes `surface` and `location.ground_m` in one undoable edit, and reports in
   one message: what was fetched, how far across, ground height at the entrance in the display unit.
4. Running it again replaces both (a pass left unticked keeps what it had).

## The map picker

An in-app panel, not a browser tab: USGS imagery tiles (`basemap.nationalmap.gov`, public domain,
keyless) with a search-free pan/zoom, a click to drop the point, and a field that also takes typed
decimal or degrees-minutes-seconds. Also lets the person choose which station is the entrance.

## What you see

- **Map:** the photo under everything at half strength, contours over it, survey on top. Every fifth
  contour is a major line, labelled in the display unit. Contours are **derived** from the stored grid
  (marching squares, ported from `CsContour` with its tests) whenever the grid or interval changes;
  they are never stored.
- **3D view:** the elevation grid as a ground surface over the cave, placed by the stored ground
  height (rough absolute point for the entrance, everything else relative to it):
  `offset = (ground_m − entrance depth) − entrance station z`, applied to the ground on read. The
  entrance depth is a display constant (5 ft, as in the old `CsElevation`). No `ground_m` means no
  terrain, never terrain at 0.
- Toggles for photo, contours and terrain are view settings (per person), like the type filters.

## Revising the entrance (Pick entrance on surface)

Available once a location and an elevation grid exist.

1. The person drags a circle on the Map (press = centre, drag = radius, shown live).
2. On release: the lowest elevation-grid cell **inside the circle** wins (cell centres; ties go to the
   cell nearest the centre). This uses the grid itself, so it finds a low point between contour lines,
   which the old contour-vertex snap could not.
3. That spot's latitude/longitude and ground height become the new `location` (`source:
   surface-pick`), one undoable edit. The cave moves to meet it; the surface stays where it is,
   because it is fixed to the world and the cave was the thing in the wrong place.
4. The surface downloads again automatically for the new spot. If it fails (offline), the old files
   stay, the message says the surface still shows the old area, and Surface Data offers it again
   next time (the `surface.for` no longer matches the location).
5. A circle with no grid data inside it changes nothing and says so.

## Privacy

Nathan, 2026-10-27: location is not a sharing concern. He trusts whoever he invites to a cave or hands
it to, so the location and the surface files travel with the cave, in saves, exports and project zips.
Nothing is stripped. Logs stay free of coordinates as a matter of tidiness only.

## Pieces

- **`cave-geo`** (new core crate, wasm-safe, no I/O): WGS84↔UTM via `cave-geomag`, NAD27→WGS84,
  zone choice, convergence, cave-frame↔world placement, window and pixel sizing, request URLs as
  text, low-point search in a grid, contour levels and marching squares (port of `CsContour` and
  `CsGeoProject` math with their `js_unit` numbers), a float32 TIFF reader.
- **`cave-doc`**: nothing new; the three settings use the existing `Setting` entity.
- **`cave-formats`**: importers record the fixed points' coordinate system and datum.
- **`cave-wasm`**: doors for location, surface request, contours from a grid, low-point pick.
- **Web app:** Surface Data button and report, map picker panel, Map layers (photo, contours, pick
  circle), 3D terrain, the surface parts in `lib/cavefile.ts`, OPFS copy while open, folder copy, fetch with timeout.

## Testing

- Unit: projection round trips; UTM convergence sign; NAD27 shift against published control points;
  pixel sizing aspect regression; low-point search (ties, empty circle, no-data cells); contours
  ported from the old `js_unit` numbers; TIFF reader on a saved 3DEP reply.
- Golden: old `CsGeoProject` and `CsContour` run under node, Rust matches (same as the survey port).
- Convergence: two people setting the location at once end with one whole location, never mixed parts.
- File: `.cavecad` round trip keeps the surface parts byte for byte; "Surface in file" off writes
  none; a file without them opens and offers the download.
- Privacy: log redaction of every new message; sanitized export has no `location`/`surface`.
- Browser: Pitfall Cave given a test location, Surface Data fetched, entrance picked inside a drawn
  circle, cave moved and surface refetched. Plumbline Pit checked too (vertical cave rule).

## Not in this build

State elevation sources; the many-caves area view; GPS tracks in `surface/`; showing absolute
elevations in callouts, profile or reports; Survex/Therion coordinate systems beyond what
`cave-formats` already converts.

## As built (2026-10-27)

- The download window uses the solved scene's x/y bounds (stations, legs, splays, walls), not traced map linework.
- The location picker is a modal dialog, not a docked panel; its switches live in the Map's Show menu.
- Heights: the ground is brought into the survey frame by SUBTRACTING the datum offset (the first build added it; found in the browser check).
- Only Compass and Walls imports record a fixed frame; a fixed point counts only when it is plausible UTM.
- Re-downloading one part keeps the stored window while the cave has not moved; otherwise a part not re-downloaded is dropped.
- A cave synced without its files re-downloads exactly the recorded request and records nothing.
- The entrance pick and its re-download are on the Map's undo history (two steps); the pick message's Undo does both.
- Terrain crosses from the engine as a Float32Array (`surface_terrain`), not JSON.
- Privacy section superseded (Nathan, 2026-10-27): location travels with the cave.
- The cave-folder copy of `surface/` beside the .cavecad is not built (the cave-folder feature is on the unmerged `new-cave-folder` branch).
- Surface Data dialog edits (location, contour spacing, downloads) land on the `settings` history, which has no undo button yet (like drawing scale).
- A Map undo of an entrance pick can step back over a later location set in the dialog (accepted).
- Every door that replaces the open cave bumps a document serial (`doc_serial`); downloads, syncs and picks that outlive their cave see it change and stop.
