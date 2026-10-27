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
- **`surface`**: what was downloaded, so every device can get the same thing:
  `{zone, bounds:[xmin,ymin,xmax,ymax], photo:{w,h}?, elevation:{w,h}?, for:{lat,lon}}` in UTM
  metres of `zone`. `for` is the location it was cut around; the 3D terrain and contours read
  `bounds`, **never recompute it** (the window grows as the cave is drawn; recomputing slides the
  hillside, measured in the old app).
- **`contour-interval`**: the contour spacing in the cave's display unit, default 10 ft / 5 m.

No survey `Elevation`, station position or fixed coordinate is rewritten by anything here.

## The downloaded files (derived, cached per device)

The photo and the elevation grid are **not** part of the synced cave. They are a cache: any device
that opens the cave reads the `surface` setting and downloads the same request itself the first time
it needs it. This needs no file syncing (the app has none) and keeps the server's cave small.

- Cached in the cave folder's `surface/` when the cave has one (`photo.png`, `elevation.tif`, each
  with its request beside it as `.json`), otherwise in the browser's private storage (OPFS) keyed by
  cave id. A cache entry whose request differs from the `surface` setting is stale and refetched.
- Requests go straight from the app to USGS (both services answer browser requests with
  `Access-Control-Allow-Origin: *`, checked 2026-10-26):
  - photo: `imagery.nationalmap.gov/.../USGSNAIPImagery/ImageServer/exportImage`, PNG, native
    0.3 m, at most 4000 px a side;
  - elevation: `elevation.nationalmap.gov/.../3DEPElevation/ImageServer/exportImage`, float32 TIFF,
    native ~1 m, at most 512 px a side.
  Pixel size keeps one shared scale for both axes so the aspect is always right; where the 256 px
  floor and the cap clash, the floor yields (old `pixelSize` bug, regression test ported).
- Window: the cave's **plan** extent (stations, walls, map linework; never the profile, sheets or the
  surface itself) plus 25% margin, at least 150 m across.
- A reply that is not an image or a readable grid (USGS returns errors as JSON with status 200) is an
  error, never a placed surface. A grid with no data means "outside 3DEP coverage".
- NAIP photographs are re-flown every few years, so two devices can hold different photographs of the
  same window. Accepted: the photo is context. Everything measured (ground height, the picked entrance)
  is stored as numbers, so it does not depend on which copy a device has.

## The Surface Data button

1. Finds the location: stored `location`; else an imported fixed point (converted, above); else the
   map picker. Nothing touches the network before a location exists.
2. Checkboxes, both on: **Aerial photo**, **Ground elevation** (contours and 3D terrain come from it).
3. Downloads, caches, writes `surface` and `location.ground_m` in one undoable edit, and reports in
   one message: what was fetched, how far across, ground height at the entrance in the display unit.
4. Running it again replaces both (a pass left unticked keeps what it had).

## The map picker

An in-app panel, not a browser tab: USGS imagery tiles (`basemap.nationalmap.gov`, public domain,
keyless) with a search-free pan/zoom, a click to drop the point, and a field that also takes typed
decimal or degrees-minutes-seconds. Also lets the person choose which station is the entrance.

## What you see

- **Map:** the photo under everything at half strength, contours over it, survey on top. Every fifth
  contour is a major line, labelled in the display unit. Contours are **derived** from the cached grid
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

- Coordinates never go into logs (`cave-log` redaction covers the new messages; tested), URLs of the
  app, exports or packages. Requests to USGS necessarily carry the window; that is the same exposure
  the old app had and is accepted.
- The cached files and the `location` and `surface` settings are location data: a sanitized export
  leaves them out (the cave folder spec already lists `surface/` as location data).

## Pieces

- **`cave-geo`** (new core crate, wasm-safe, no I/O): WGS84↔UTM via `cave-geomag`, NAD27→WGS84,
  zone choice, convergence, cave-frame↔world placement, window and pixel sizing, request URLs as
  text, low-point search in a grid, contour levels and marching squares (port of `CsContour` and
  `CsGeoProject` math with their `js_unit` numbers), a float32 TIFF reader.
- **`cave-doc`**: nothing new; the three settings use the existing `Setting` entity.
- **`cave-formats`**: importers record the fixed points' coordinate system and datum.
- **`cave-wasm`**: doors for location, surface request, contours from a grid, low-point pick.
- **Web app:** Surface Data button and report, map picker panel, Map layers (photo, contours, pick
  circle), 3D terrain, the cache (`lib/surface-cache.ts`, folder or OPFS), fetch with timeout.

## Testing

- Unit: projection round trips; UTM convergence sign; NAD27 shift against published control points;
  pixel sizing aspect regression; low-point search (ties, empty circle, no-data cells); contours
  ported from the old `js_unit` numbers; TIFF reader on a saved 3DEP reply.
- Golden: old `CsGeoProject` and `CsContour` run under node, Rust matches (same as the survey port).
- Convergence: two people setting the location at once end with one whole location, never mixed parts.
- Privacy: log redaction of every new message; sanitized export has no `location`/`surface`.
- Browser: Pitfall Cave given a test location, Surface Data fetched, entrance picked inside a drawn
  circle, cave moved and surface refetched. Plumbline Pit checked too (vertical cave rule).

## Not in this build

State elevation sources; the many-caves area view; GPS tracks in `surface/`; showing absolute
elevations in callouts, profile or reports; Survex/Therion coordinate systems beyond what
`cave-formats` already converts.
