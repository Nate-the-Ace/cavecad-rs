# Surface Data and Entrance Location Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers-extended-cc:subagent-driven-development (recommended) or superpowers-extended-cc:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give every cave a world location (WGS84 + NAVD88), download its surface (USGS NAIP photo + 3DEP elevation) into the `.cavecad` file, draw photo/contours/terrain under the cave, and let the person drag a circle to move the entrance to the lowest ground inside it, refetching the surface automatically.

**Architecture:** All math lives in a new wasm-safe core crate `cave-geo` (frames, datum, window, TIFF grid, contours, low-point pick, settings records). `cave-wasm/src/geo.rs` exposes JSON doors. The web app fetches from USGS directly (both services send `Access-Control-Allow-Origin: *`), keeps the files in OPFS keyed by the request, packs them into `.cavecad` as `surface/*` parts, and draws them in the existing three.js view.

**Tech Stack:** Rust 2024 workspace (`cave-geomag` UTM/convergence, `cave-doc` settings), wasm-bindgen, Svelte 5 + three.js (`apps/web`), Leaflet 1.9.4 for the location picker.

**User decisions (already made):**
- Same workflow as the old app: one Surface Data button, aerial photo + ground elevation, contours from the elevation.
- "Drag a circle: I drag out the area myself, and it snaps to the lowest point inside what I drew."
- "Always redownload: fetch the surface again automatically after every entrance move."
- "Keep USGS 3DEP only for now, like the old app. State sources can come later."
- "Use the fixed point from an imported survey file if there is one, otherwise ask me with the map picker."
- Common coordinate system for every cave so a later build can show many caves together (multi-cave view itself not in this build).
- "we should support storing the surface in the .cavecad file."

Spec: `docs/superpowers/specs/2026-10-26-surface-data-design.md`.

---

## Conventions every task follows

- Work in the worktree `~/Documents/github/cavecad-rs-surface` (branch `surface-data`).
- Toolchain: `export PATH=/opt/homebrew/opt/rustup/bin:$PATH` before any `cargo`.
- Format only touched crates: `cargo fmt -p <crate>` (other crates are not rustfmt-clean).
- Before each commit that touches Rust: `cargo clippy -p <crate> --all-targets -- -D warnings` and `cargo test -p <crate>`.
- After adding public items: `python3 tools/make_code_index.py` and commit `docs/CODE-INDEX.md`.
- Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- **Never** put a latitude/longitude into a `tracing` call, a URL of the app, or a test fixture that is a real cave. The only real-world fixture is a 3DEP grid of a town block in Bowling Green, KY (not a cave).

## Frame facts (used by several tasks)

- **Cave frame:** metres, x east, y north (TRUE north: survey bearings are stored true), z up. Entrance station position comes from the solved scene.
- **UTM grid:** WGS84 UTM, zone `floor((lon+180)/6)+1`, negative south of the equator; EPSG `32600+zone` north, `32700+|zone|` south.
- **Convergence γ** = `cave_geomag::utm_convergence(lat, lon, zone)`: grid north's angle EAST of true north, degrees. A grid offset `(de, dn)` becomes cave `(x, y) = (de·cosγ + dn·sinγ, −de·sinγ + dn·cosγ)` (γ in radians); the inverse is the transpose.
- **Grid cells:** value `(c, r)` (row 0 at the top) sits at the cell CENTRE: `e = xmin + (c+0.5)·sx`, `n = ymax − (r+0.5)·sy`, `sx = (xmax−xmin)/w`, `sy = (ymax−ymin)/h`.
- **Image pixels:** pixel corner `(px, py)` (py down) sits at `e = xmin + px·sx`, `n = ymax − py·sy`.

---

### Task 1: `cave-geo` crate with world/cave frames

**Goal:** A new core crate placing a cave in the world: UTM zone choice, lat/lon↔UTM, and a `Placement` that maps between cave metres and UTM/lat-lon through the entrance.

**Files:**
- Create: `crates/cave-geo/Cargo.toml`
- Create: `crates/cave-geo/src/lib.rs`
- Create: `crates/cave-geo/src/frame.rs`
- Test: `crates/cave-geo/tests/frame.rs`
- Modify: `CLAUDE.md` (wasm build line: add `-p cave-geo`)

**Acceptance Criteria:**
- [ ] `utm_zone` gives 16 at lon −86.4 and −16 at (−33, −86.4).
- [ ] A point due TRUE north of the entrance (same longitude) maps to cave x within 1 mm of 0.
- [ ] `cave_to_latlon(utm_to_cave(e, n))` round-trips to < 1 mm.
- [ ] Entrance maps to its own lat/lon exactly (< 1e-9 deg).
- [ ] `cargo build --target wasm32-unknown-unknown -p cave-geo` succeeds.

**Verify:** `cargo test -p cave-geo --test frame` → all pass

**Steps:**

- [ ] **Step 1: Create the crate manifest**

```toml
# crates/cave-geo/Cargo.toml
[package]
name = "cave-geo"
edition.workspace = true
version.workspace = true
publish.workspace = true

[lints]
workspace = true

[dependencies]
cave-geomag = { path = "../cave-geomag" }
cave-doc = { path = "../cave-doc" }
cave-id = { path = "../cave-id" }
serde = { version = "1", features = ["derive"] }
serde_json = { version = "1", features = ["float_roundtrip"] }

[dev-dependencies]
cave-draw = { path = "../cave-draw" }
```

- [ ] **Step 2: Write the failing tests**

```rust
// crates/cave-geo/tests/frame.rs
use cave_geo::frame::{LatLon, Placement, utm_zone};

const BG: LatLon = LatLon { lat: 36.99, lon: -86.44 }; // a town block, not a cave

#[test]
fn zone_choice() {
    assert_eq!(utm_zone(BG), 16);
    assert_eq!(utm_zone(LatLon { lat: -33.0, lon: -86.44 }), -16);
    assert_eq!(cave_geo::frame::epsg(16), 32616);
    assert_eq!(cave_geo::frame::epsg(-16), 32716);
}

#[test]
fn entrance_is_its_own_coordinate() {
    let p = Placement::new([12.0, -7.0], BG);
    let ll = p.cave_to_latlon(12.0, -7.0);
    assert!((ll.lat - BG.lat).abs() < 1e-9 && (ll.lon - BG.lon).abs() < 1e-9);
}

#[test]
fn true_north_is_cave_y() {
    // Bowling Green is east of zone 16's central meridian (-87), so grid north is east of true north.
    let p = Placement::new([0.0, 0.0], BG);
    assert!(p.convergence_deg() > 0.0);
    let north = LatLon { lat: BG.lat + 0.001, lon: BG.lon };
    let (e, n) = p.latlon_to_utm(north);
    let xy = p.utm_to_cave(e, n);
    assert!(xy[0].abs() < 1e-3, "x = {}", xy[0]);
    assert!(xy[1] > 110.0 && xy[1] < 112.0, "y = {}", xy[1]);
}

#[test]
fn round_trip() {
    let p = Placement::new([100.0, 50.0], BG);
    for (x, y) in [(0.0, 0.0), (350.0, -120.0), (-800.0, 900.0)] {
        let (e, n) = p.cave_to_utm(x, y);
        let back = p.utm_to_cave(e, n);
        assert!((back[0] - x).abs() < 1e-3 && (back[1] - y).abs() < 1e-3);
        let ll = p.cave_to_latlon(x, y);
        let (e2, n2) = p.latlon_to_utm(ll);
        assert!((e2 - e).abs() < 1e-3 && (n2 - n).abs() < 1e-3);
    }
}
```

- [ ] **Step 3: Run to see it fail**

Run: `cargo test -p cave-geo --test frame`
Expected: FAIL (crate has no `frame` module)

- [ ] **Step 4: Implement**

```rust
// crates/cave-geo/src/lib.rs
//! Where a cave is on the Earth: one world frame (WGS84 latitude/longitude, NAVD88 heights) for
//! every cave, the surface downloaded over it (USGS NAIP photo, USGS 3DEP elevation, both requested
//! in the entrance's UTM zone), contours from that elevation, and the entrance pick. Pure: no I/O.

pub mod frame;
```

```rust
// crates/cave-geo/src/frame.rs
//! The cave frame (metres, x east, y TRUE north, z up) tied to the world through the entrance.

use serde::{Deserialize, Serialize};

/// A WGS84 latitude and longitude in degrees, north and east positive.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LatLon {
    /// Degrees, north positive.
    pub lat: f64,
    /// Degrees, east positive.
    pub lon: f64,
}

/// The UTM zone holding `ll`: 1..=60, negative south of the equator.
pub fn utm_zone(ll: LatLon) -> i32 {
    let z = (((ll.lon + 180.0) / 6.0).floor() as i32).clamp(0, 59) + 1;
    if ll.lat < 0.0 { -z } else { z }
}

/// The EPSG code of a WGS84 UTM zone (32600 + zone north, 32700 + zone south).
pub fn epsg(zone: i32) -> u32 {
    if zone < 0 { 32700 + zone.unsigned_abs() } else { 32600 + zone as u32 }
}

/// The cave frame tied to the world: the entrance's cave position and its latitude/longitude.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    anchor: [f64; 2],
    at: LatLon,
    zone: i32,
    anchor_utm: (f64, f64),
    gamma: f64,
}

impl Placement {
    /// The entrance at cave position `anchor` (metres) is at `at`; UTM is the entrance's zone.
    pub fn new(anchor: [f64; 2], at: LatLon) -> Self {
        Self::in_zone(anchor, at, utm_zone(at))
    }

    /// As [`Placement::new`], in a chosen zone (a stored surface keeps the zone it was cut in).
    pub fn in_zone(anchor: [f64; 2], at: LatLon, zone: i32) -> Self {
        let anchor_utm = cave_geomag::latlon_to_utm(at.lat, at.lon, zone);
        let gamma = cave_geomag::utm_convergence(at.lat, at.lon, zone).to_radians();
        Placement { anchor, at, zone, anchor_utm, gamma }
    }

    /// The UTM zone this placement works in.
    pub fn zone(&self) -> i32 {
        self.zone
    }

    /// Grid north's angle east of true north at the entrance, degrees.
    pub fn convergence_deg(&self) -> f64 {
        self.gamma.to_degrees()
    }

    /// The entrance's latitude/longitude.
    pub fn at(&self) -> LatLon {
        self.at
    }

    /// The entrance's cave position.
    pub fn anchor(&self) -> [f64; 2] {
        self.anchor
    }

    /// UTM easting/northing (this zone) to cave metres.
    pub fn utm_to_cave(&self, e: f64, n: f64) -> [f64; 2] {
        let (de, dn) = (e - self.anchor_utm.0, n - self.anchor_utm.1);
        let (s, c) = self.gamma.sin_cos();
        [self.anchor[0] + de * c + dn * s, self.anchor[1] - de * s + dn * c]
    }

    /// Cave metres to UTM easting/northing (this zone).
    pub fn cave_to_utm(&self, x: f64, y: f64) -> (f64, f64) {
        let (dx, dy) = (x - self.anchor[0], y - self.anchor[1]);
        let (s, c) = self.gamma.sin_cos();
        (self.anchor_utm.0 + dx * c - dy * s, self.anchor_utm.1 + dx * s + dy * c)
    }

    /// Latitude/longitude to UTM in this placement's zone.
    pub fn latlon_to_utm(&self, ll: LatLon) -> (f64, f64) {
        cave_geomag::latlon_to_utm(ll.lat, ll.lon, self.zone)
    }

    /// Cave metres to latitude/longitude.
    pub fn cave_to_latlon(&self, x: f64, y: f64) -> LatLon {
        let (e, n) = self.cave_to_utm(x, y);
        let (lat, lon) = cave_geomag::utm_to_latlon(e, n, self.zone);
        LatLon { lat, lon }
    }
}
```

- [ ] **Step 5: Run tests, wasm build, CLAUDE.md line**

Run: `cargo test -p cave-geo --test frame` → PASS.
Run: `cargo build --target wasm32-unknown-unknown -p cave-geo` → Finished.
Edit `CLAUDE.md`: in the wasm build command add ` -p cave-geo` after `-p cave-snap`.

- [ ] **Step 6: Commit**

```bash
cargo fmt -p cave-geo && cargo clippy -p cave-geo --all-targets -- -D warnings
python3 tools/make_code_index.py
git add crates/cave-geo CLAUDE.md docs/CODE-INDEX.md Cargo.lock
git commit -m "feat(cave-geo): world frame and cave placement through the entrance"
```

---

### Task 2: Datums and the imported fixed-point frame

**Goal:** Convert a fixed point given in UTM on NAD27/NAD83/WGS84 to WGS84 latitude/longitude, and name that frame in a serializable `FixedFrame`.

**Files:**
- Create: `crates/cave-geo/src/datum.rs`
- Modify: `crates/cave-geo/src/lib.rs` (add `pub mod datum;`)
- Test: `crates/cave-geo/tests/datum.rs`

**Acceptance Criteria:**
- [ ] `Datum::parse` reads "North American 1927", "NAD27", "North American 1983", "NAD83", "WGS 1984", "WGS84"; unknown names give `None`.
- [ ] NAD27→WGS84 at (37.0, −86.4) moves the point 5–60 m horizontally; NAD83 and WGS84 are unchanged.
- [ ] `FixedFrame::to_latlon` of a NAD83 UTM point equals `cave_geomag::utm_to_latlon` exactly.
- [ ] `FixedFrame` round-trips through JSON.

**Verify:** `cargo test -p cave-geo --test datum` → all pass

**Steps:**

- [ ] **Step 1: Failing tests**

```rust
// crates/cave-geo/tests/datum.rs
use cave_geo::datum::{Datum, FixedFrame};
use cave_geo::frame::LatLon;

fn metres(a: LatLon, b: LatLon) -> f64 {
    let dy = (a.lat - b.lat) * 111_000.0;
    let dx = (a.lon - b.lon) * 111_000.0 * a.lat.to_radians().cos();
    (dx * dx + dy * dy).sqrt()
}

#[test]
fn names() {
    assert_eq!(Datum::parse("North American 1927"), Some(Datum::Nad27));
    assert_eq!(Datum::parse("nad27"), Some(Datum::Nad27));
    assert_eq!(Datum::parse("North American 1983"), Some(Datum::Nad83));
    assert_eq!(Datum::parse("NAD83"), Some(Datum::Nad83));
    assert_eq!(Datum::parse("WGS 1984"), Some(Datum::Wgs84));
    assert_eq!(Datum::parse("WGS84"), Some(Datum::Wgs84));
    assert_eq!(Datum::parse("Tokyo"), None);
}

#[test]
fn nad27_shift_is_tens_of_metres() {
    let p = LatLon { lat: 37.0, lon: -86.4 };
    let w = Datum::Nad27.to_wgs84(p);
    let d = metres(p, w);
    assert!(d > 5.0 && d < 60.0, "shift {d} m");
    assert_eq!(Datum::Nad83.to_wgs84(p), p);
    assert_eq!(Datum::Wgs84.to_wgs84(p), p);
}

#[test]
fn fixed_frame() {
    let f = FixedFrame { zone: 16, datum: Datum::Nad83 };
    let ll = f.to_latlon(550_000.0, 4_094_000.0);
    let (lat, lon) = cave_geomag::utm_to_latlon(550_000.0, 4_094_000.0, 16);
    assert_eq!((ll.lat, ll.lon), (lat, lon));
    let j = serde_json::to_string(&f).unwrap();
    assert_eq!(serde_json::from_str::<FixedFrame>(&j).unwrap(), f);
}
```

- [ ] **Step 2: Run to see it fail** — `cargo test -p cave-geo --test datum` → FAIL (no `datum` module).

- [ ] **Step 3: Implement**

```rust
// crates/cave-geo/src/datum.rs
//! Horizontal datums a survey file names for its fixed points, and the shift to WGS84.
//!
//! NAD27 is shifted with the CONUS mean three-parameter transform (dX −8, dY +160, dZ +176 m) through
//! Earth-centred coordinates: good to a few metres, which the entrance pick then corrects. NAD83 is
//! taken as WGS84 (they differ by about a metre in the US, less than a 3DEP cell).

use crate::frame::LatLon;
use serde::{Deserialize, Serialize};

/// A horizontal datum CaveCAD converts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Datum {
    /// WGS 1984 (GPS).
    Wgs84,
    /// North American 1983 (taken as WGS84).
    Nad83,
    /// North American 1927 (Clarke 1866 ellipsoid).
    Nad27,
}

const WGS84: (f64, f64) = (6_378_137.0, 1.0 / 298.257_223_563);
const CLARKE1866: (f64, f64) = (6_378_206.4, 1.0 / 294.978_698_214);
const NAD27_SHIFT: [f64; 3] = [-8.0, 160.0, 176.0];

impl Datum {
    /// Reads a datum name as Compass, Walls or a person writes it.
    pub fn parse(name: &str) -> Option<Datum> {
        let u: String = name.to_ascii_uppercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        if u.contains("1927") || u.contains("NAD27") {
            Some(Datum::Nad27)
        } else if u.contains("1983") || u.contains("NAD83") {
            Some(Datum::Nad83)
        } else if u.contains("WGS") && (u.contains("84") || u.contains("1984")) {
            Some(Datum::Wgs84)
        } else {
            None
        }
    }

    /// A point on this datum as WGS84.
    pub fn to_wgs84(self, p: LatLon) -> LatLon {
        match self {
            Datum::Wgs84 | Datum::Nad83 => p,
            Datum::Nad27 => {
                let [x, y, z] = to_ecef(p, CLARKE1866);
                from_ecef([x + NAD27_SHIFT[0], y + NAD27_SHIFT[1], z + NAD27_SHIFT[2]], WGS84)
            }
        }
    }
}

fn to_ecef(p: LatLon, (a, f): (f64, f64)) -> [f64; 3] {
    let e2 = f * (2.0 - f);
    let (lat, lon) = (p.lat.to_radians(), p.lon.to_radians());
    let n = a / (1.0 - e2 * lat.sin().powi(2)).sqrt();
    [n * lat.cos() * lon.cos(), n * lat.cos() * lon.sin(), n * (1.0 - e2) * lat.sin()]
}

fn from_ecef([x, y, z]: [f64; 3], (a, f): (f64, f64)) -> LatLon {
    let e2 = f * (2.0 - f);
    let p = (x * x + y * y).sqrt();
    let mut lat = (z / (p * (1.0 - e2))).atan();
    for _ in 0..8 {
        let n = a / (1.0 - e2 * lat.sin().powi(2)).sqrt();
        let h = p / lat.cos() - n;
        lat = (z / (p * (1.0 - e2 * n / (n + h)))).atan();
    }
    LatLon { lat: lat.to_degrees(), lon: y.atan2(x).to_degrees() }
}

/// The coordinate system an import's fixed points are written in: a UTM zone on a datum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixedFrame {
    /// UTM zone, negative south.
    pub zone: i32,
    /// The datum the numbers are on.
    pub datum: Datum,
}

impl FixedFrame {
    /// A fixed point's easting/northing (metres) as WGS84 latitude/longitude.
    pub fn to_latlon(&self, e: f64, n: f64) -> LatLon {
        let (lat, lon) = cave_geomag::utm_to_latlon(e, n, self.zone);
        self.datum.to_wgs84(LatLon { lat, lon })
    }
}
```

Note: `utm_to_latlon` uses the WGS84 ellipsoid for every datum. For NAD27 the UTM grid is on Clarke 1866; the ellipsoid difference in the inverse is under 1 m in a zone, below the 3-parameter transform's own error. Leave this comment in the code above `to_latlon`.

- [ ] **Step 4: Pass + commit**

```bash
cargo test -p cave-geo --test datum
cargo fmt -p cave-geo && cargo clippy -p cave-geo --all-targets -- -D warnings
python3 tools/make_code_index.py
git add crates/cave-geo docs/CODE-INDEX.md
git commit -m "feat(cave-geo): NAD27/NAD83/WGS84 datums and the fixed-point frame"
```

---

### Task 3: Download window, pixel sizing, request URLs

**Goal:** The UTM box to download (cave plan extent + 25% margin, ≥ 150 m), pixel sizes (ported `pixelSize`), and the two exportImage URLs.

**Files:**
- Create: `crates/cave-geo/src/request.rs`
- Modify: `crates/cave-geo/src/lib.rs` (`pub mod request;`)
- Test: `crates/cave-geo/tests/request.rs`

**Acceptance Criteria:**
- [ ] A 100×40 m cave box gives a window ≥ 150 m on both axes; a 400×200 box gives ≥ 500×250 (rotation can only grow it).
- [ ] The window contains all four padded cave corners.
- [ ] `pixel_size` keeps aspect: |w/h − bw/bh| < 1% on a 500×250 box; caps at 4000; floors at 256; a 25:1 box gives long axis 4000 and short axis 160 (floor yields).
- [ ] URLs carry `bboxSR=<epsg>&imageSR=<epsg>`, `format=jpg` (photo) and `format=tiff&pixelType=F32` (elevation), bbox to 3 decimals.

**Verify:** `cargo test -p cave-geo --test request` → all pass

**Steps:**

- [ ] **Step 1: Failing tests**

```rust
// crates/cave-geo/tests/request.rs
use cave_geo::frame::{LatLon, Placement};
use cave_geo::request::{Bounds, PHOTO, ELEVATION, dem_url, naip_url, pixel_size, window};

const BG: LatLon = LatLon { lat: 36.99, lon: -86.44 };

#[test]
fn window_floor_and_margin() {
    let p = Placement::new([0.0, 0.0], BG);
    let w = window([0.0, 0.0, 100.0, 40.0], &p);
    assert!(w.width() >= 150.0 && w.height() >= 150.0);
    let w = window([-200.0, -100.0, 200.0, 100.0], &p);
    assert!(w.width() >= 500.0 && w.height() >= 250.0);
    for (x, y) in [(-250.0, -125.0), (250.0, -125.0), (250.0, 125.0), (-250.0, 125.0)] {
        let (e, n) = p.cave_to_utm(x, y);
        assert!(e >= w.xmin - 1e-6 && e <= w.xmax + 1e-6 && n >= w.ymin - 1e-6 && n <= w.ymax + 1e-6);
    }
}

#[test]
fn pixels_keep_aspect() {
    let b = Bounds { xmin: 0.0, ymin: 0.0, xmax: 500.0, ymax: 250.0 };
    let (w, h) = pixel_size(&b, PHOTO);
    assert!(((w as f64 / h as f64) - 2.0).abs() < 0.02);
    assert!(w <= 4000 && h <= 4000);
    let tiny = Bounds { xmin: 0.0, ymin: 0.0, xmax: 10.0, ymax: 10.0 };
    assert_eq!(pixel_size(&tiny, PHOTO), (256, 256));
    let long = Bounds { xmin: 0.0, ymin: 0.0, xmax: 3750.0, ymax: 150.0 };
    assert_eq!(pixel_size(&long, PHOTO), (4000, 160));
    let dem = pixel_size(&b, ELEVATION);
    assert_eq!(dem, (500, 250));
}

#[test]
fn urls() {
    let b = Bounds { xmin: 550000.0, ymin: 4094000.0, xmax: 550200.5, ymax: 4094200.0 };
    let u = naip_url(&b, 16, (400, 400));
    assert!(u.starts_with("https://imagery.nationalmap.gov/arcgis/rest/services/USGSNAIPImagery/ImageServer/exportImage?"));
    assert!(u.contains("bbox=550000.000,4094000.000,550200.500,4094200.000"));
    assert!(u.contains("bboxSR=32616&imageSR=32616&size=400,400&format=jpg&f=image"));
    let d = dem_url(&b, 16, (200, 200));
    assert!(d.starts_with("https://elevation.nationalmap.gov/arcgis/rest/services/3DEPElevation/ImageServer/exportImage?"));
    assert!(d.contains("&format=tiff&pixelType=F32&f=image"));
}
```

- [ ] **Step 2: Run to fail** — `cargo test -p cave-geo --test request` → FAIL.

- [ ] **Step 3: Implement**

```rust
// crates/cave-geo/src/request.rs
//! What to download: the UTM window around a cave, its pixel size, and the USGS request URLs.
//! Ported from the old CsGeoProject (groundExtent, pixelSize, naipUrl, demUrl), with UTM in place of
//! Web Mercator so ground pixels are true metres.

use crate::frame::{Placement, epsg};
use serde::{Deserialize, Serialize};

/// A box in UTM metres (one zone).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    /// West edge, easting.
    pub xmin: f64,
    /// South edge, northing.
    pub ymin: f64,
    /// East edge.
    pub xmax: f64,
    /// North edge.
    pub ymax: f64,
}

impl Bounds {
    /// East-west size, metres.
    pub fn width(&self) -> f64 {
        self.xmax - self.xmin
    }
    /// North-south size, metres.
    pub fn height(&self) -> f64 {
        self.ymax - self.ymin
    }
}

/// Margin added around the cave (0.25 = 25%).
pub const MARGIN: f64 = 0.25;
/// Smallest window per axis, metres.
pub const FLOOR_M: f64 = 150.0;

/// A service's pixel rules.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Service {
    /// Native metres per pixel.
    pub native_m: f64,
    /// Largest side, pixels.
    pub max_px: f64,
    /// Smallest side, pixels (yields to aspect).
    pub min_px: f64,
}

/// NAIP: 0.3 m native, 4000 px service limit.
pub const PHOTO: Service = Service { native_m: 0.3, max_px: 4000.0, min_px: 256.0 };
/// 3DEP: ~1 m native; 512 px is plenty for contours and terrain.
pub const ELEVATION: Service = Service { native_m: 1.0, max_px: 512.0, min_px: 64.0 };

/// The UTM window for a cave whose plan extent is `[min_x, min_y, max_x, max_y]` (cave metres):
/// padded by [`MARGIN`], at least [`FLOOR_M`] each way, then the UTM box holding the padded corners.
pub fn window(cave: [f64; 4], place: &Placement) -> Bounds {
    let (cx, cy) = ((cave[0] + cave[2]) / 2.0, (cave[1] + cave[3]) / 2.0);
    let hw = ((cave[2] - cave[0]).abs() * (1.0 + MARGIN)).max(FLOOR_M) / 2.0;
    let hh = ((cave[3] - cave[1]).abs() * (1.0 + MARGIN)).max(FLOOR_M) / 2.0;
    let mut b = Bounds { xmin: f64::MAX, ymin: f64::MAX, xmax: f64::MIN, ymax: f64::MIN };
    for (x, y) in [(cx - hw, cy - hh), (cx + hw, cy - hh), (cx + hw, cy + hh), (cx - hw, cy + hh)] {
        let (e, n) = place.cave_to_utm(x, y);
        b.xmin = b.xmin.min(e);
        b.ymin = b.ymin.min(n);
        b.xmax = b.xmax.max(e);
        b.ymax = b.ymax.max(n);
    }
    b
}

/// Pixel size for a window: native where it fits, one shared factor for both axes so the aspect is
/// always the window's; where the cap and the floor clash, the floor yields (old pixelSize bug).
pub fn pixel_size(b: &Bounds, s: Service) -> (u32, u32) {
    let (mut w, mut h) = (b.width() / s.native_m, b.height() / s.native_m);
    let down = 1f64.min(s.max_px / w).min(s.max_px / h);
    w *= down;
    h *= down;
    let up = 1f64.max(s.min_px / w).max(s.min_px / h).min(s.max_px / w.max(h));
    w *= up;
    h *= up;
    ((w.round() as u32).max(1), (h.round() as u32).max(1))
}

const NAIP: &str = "https://imagery.nationalmap.gov/arcgis/rest/services/USGSNAIPImagery/ImageServer/exportImage";
const DEM: &str = "https://elevation.nationalmap.gov/arcgis/rest/services/3DEPElevation/ImageServer/exportImage";

fn query(b: &Bounds, zone: i32, size: (u32, u32)) -> String {
    let sr = epsg(zone);
    format!(
        "bbox={:.3},{:.3},{:.3},{:.3}&bboxSR={sr}&imageSR={sr}&size={},{}",
        b.xmin, b.ymin, b.xmax, b.ymax, size.0, size.1
    )
}

/// The NAIP photograph request (JPEG).
pub fn naip_url(b: &Bounds, zone: i32, size: (u32, u32)) -> String {
    format!("{NAIP}?{}&format=jpg&f=image", query(b, zone, size))
}

/// The 3DEP elevation request (float32 TIFF, metres NAVD88).
pub fn dem_url(b: &Bounds, zone: i32, size: (u32, u32)) -> String {
    format!("{DEM}?{}&format=tiff&pixelType=F32&f=image", query(b, zone, size))
}
```

Worked numbers behind the test: 3750/0.3 = 12500 and 150/0.3 = 500, down = 0.32 gives 4000 × 160; up = max(1, 1.6) capped at 4000/4000 = 1, so the floor yields: (4000, 160). ELEVATION on 500×250 m: 500 × 250 px, no scaling either way: (500, 250).

- [ ] **Step 4: Pass + commit**

```bash
cargo test -p cave-geo --test request
cargo fmt -p cave-geo && cargo clippy -p cave-geo --all-targets -- -D warnings
python3 tools/make_code_index.py
git add crates/cave-geo docs/CODE-INDEX.md
git commit -m "feat(cave-geo): UTM download window, pixel sizing and USGS request URLs"
```

---

### Task 4: Elevation grid (float TIFF reader, sampling)

**Goal:** Read the 3DEP float32 TIFF (tiled or stripped, uncompressed) into a `Grid`, with no-data detection, range, bilinear sampling, and cell↔UTM mapping.

**Files:**
- Create: `crates/cave-geo/src/grid.rs`
- Create: `crates/cave-geo/tests/data/bowling-green-200.tif` (fetched, below)
- Modify: `crates/cave-geo/src/lib.rs` (`pub mod grid;`)
- Test: `crates/cave-geo/tests/grid.rs`

**Acceptance Criteria:**
- [ ] A synthetic 3×3 little-endian stripped TIFF reads back its 9 values; big-endian too.
- [ ] Compressed / 16-bit / 2-sample / non-TIFF input returns `Err` naming the reason.
- [ ] The real 200×200 fixture parses (tiled 128×128), has no no-data cells, range within 120–250 m.
- [ ] `sample_at` is bilinear and returns `None` next to a no-data cell or outside the grid.
- [ ] `cell_utm` then `utm_cell` round-trips.

**Verify:** `cargo test -p cave-geo --test grid` → all pass

**Steps:**

- [ ] **Step 1: Fetch the fixture (a town block, not a cave)**

```bash
mkdir -p crates/cave-geo/tests/data
curl -s -o crates/cave-geo/tests/data/bowling-green-200.tif "https://elevation.nationalmap.gov/arcgis/rest/services/3DEPElevation/ImageServer/exportImage?bbox=550000,4094000,550200,4094200&bboxSR=32616&imageSR=32616&size=200,200&format=tiff&pixelType=F32&f=image"
file crates/cave-geo/tests/data/bowling-green-200.tif   # TIFF image data, little-endian ... width=200
```

- [ ] **Step 2: Failing tests**

```rust
// crates/cave-geo/tests/grid.rs
use cave_geo::grid::Grid;
use cave_geo::request::Bounds;

/// A minimal uncompressed stripped float32 TIFF.
fn tiff(w: u16, h: u16, vals: &[f32], le: bool) -> Vec<u8> {
    let u16b = |v: u16| if le { v.to_le_bytes() } else { v.to_be_bytes() };
    let u32b = |v: u32| if le { v.to_le_bytes() } else { v.to_be_bytes() };
    let mut out = Vec::new();
    out.extend_from_slice(if le { b"II" } else { b"MM" });
    out.extend_from_slice(&u16b(42));
    let data_at = 8u32;
    let data: Vec<u8> = vals.iter().flat_map(|v| if le { v.to_le_bytes() } else { v.to_be_bytes() }).collect();
    let ifd_at = data_at + data.len() as u32;
    out.extend_from_slice(&u32b(ifd_at));
    out.extend_from_slice(&data);
    let tags: [(u16, u16, u32); 8] = [(256, 3, w as u32), (257, 3, h as u32), (258, 3, 32), (259, 3, 1), (273, 4, data_at), (277, 3, 1), (278, 3, h as u32), (339, 3, 3)];
    out.extend_from_slice(&u16b(tags.len() as u16));
    for (t, ty, v) in tags {
        out.extend_from_slice(&u16b(t));
        out.extend_from_slice(&u16b(ty));
        out.extend_from_slice(&u32b(1));
        if ty == 3 { out.extend_from_slice(&u16b(v as u16)); out.extend_from_slice(&[0, 0]); } else { out.extend_from_slice(&u32b(v)); }
    }
    out.extend_from_slice(&u32b(0));
    out
}

#[test]
fn reads_synthetic_both_orders() {
    let v: Vec<f32> = (0..9).map(|i| 100.0 + i as f32).collect();
    for le in [true, false] {
        let g = Grid::parse_tiff(&tiff(3, 3, &v, le)).unwrap();
        assert_eq!((g.w, g.h), (3, 3));
        assert_eq!(g.values, v);
    }
}

#[test]
fn refuses_what_it_cannot_read() {
    assert!(Grid::parse_tiff(b"not a tiff at all").is_err());
    let mut t = tiff(3, 3, &[0.0; 9], true);
    // compression tag (4th entry) value -> 5 (LZW)
    let ifd = u32::from_le_bytes([t[4], t[5], t[6], t[7]]) as usize;
    t[ifd + 2 + 3 * 12 + 8] = 5;
    let e = Grid::parse_tiff(&t).unwrap_err();
    assert!(e.contains("compressed"), "{e}");
}

#[test]
fn real_3dep_reply() {
    let g = Grid::parse_tiff(include_bytes!("data/bowling-green-200.tif")).unwrap();
    assert_eq!((g.w, g.h), (200, 200));
    let (lo, hi) = g.range().unwrap();
    assert!(lo > 120.0 && hi < 250.0 && lo < hi, "{lo}..{hi}");
    assert!(g.values.iter().all(|v| !Grid::no_data(*v)));
}

#[test]
fn sampling() {
    let g = Grid { w: 2, h: 2, values: vec![0.0, 10.0, 20.0, 30.0] };
    assert_eq!(g.sample_at(0.5, 0.5), Some(15.0));
    assert_eq!(g.sample_at(1.0, 0.0), Some(10.0));
    assert_eq!(g.sample_at(-0.1, 0.0), None);
    let holed = Grid { w: 2, h: 2, values: vec![0.0, -1.0e30, 20.0, 30.0] };
    assert_eq!(holed.sample_at(0.5, 0.5), None);
}

#[test]
fn cells_and_utm() {
    let b = Bounds { xmin: 1000.0, ymin: 2000.0, xmax: 1200.0, ymax: 2100.0 };
    let g = Grid { w: 200, h: 100, values: vec![0.0; 20_000] };
    assert_eq!(g.cell_utm(&b, 0.0, 0.0), (1000.5, 2099.5));
    let (c, r) = g.utm_cell(&b, 1100.25, 2050.75);
    let (e, n) = g.cell_utm(&b, c, r);
    assert!((e - 1100.25).abs() < 1e-9 && (n - 2050.75).abs() < 1e-9);
}
```

- [ ] **Step 3: Run to fail** — `cargo test -p cave-geo --test grid` → FAIL.

- [ ] **Step 4: Implement** (port of `CsContour.parseFloatTiff`, `range`, `sampleAt`, `isNoData`)

```rust
// crates/cave-geo/src/grid.rs
//! The 3DEP elevation grid: a float32 GeoTIFF read into rows (row 0 at the top, north), metres
//! NAVD88. Ported from the old CsContour.parseFloatTiff: tiled (what the service sends, 128x128
//! tiles) or stripped, uncompressed only -- anything else is an error naming why, never a wrong grid.

use crate::request::Bounds;

/// An elevation grid, row-major, row 0 the north edge, values in metres.
#[derive(Clone, Debug, PartialEq)]
pub struct Grid {
    /// Columns.
    pub w: usize,
    /// Rows.
    pub h: usize,
    /// `w * h` values.
    pub values: Vec<f32>,
}

struct Reader<'a> {
    b: &'a [u8],
    le: bool,
}

impl Reader<'_> {
    fn u16(&self, o: usize) -> Result<u16, String> {
        let s = self.b.get(o..o + 2).ok_or("TIFF is cut short")?;
        Ok(if self.le { u16::from_le_bytes([s[0], s[1]]) } else { u16::from_be_bytes([s[0], s[1]]) })
    }
    fn u32(&self, o: usize) -> Result<u32, String> {
        let s = self.b.get(o..o + 4).ok_or("TIFF is cut short")?;
        let a = [s[0], s[1], s[2], s[3]];
        Ok(if self.le { u32::from_le_bytes(a) } else { u32::from_be_bytes(a) })
    }
    fn f32(&self, o: usize) -> Result<f32, String> {
        self.u32(o).map(f32::from_bits)
    }
}

struct Tag {
    ty: u16,
    count: u32,
    at: usize,
}

impl Grid {
    /// True for a hole: NaN, infinite, or far below any real ground (3DEP's no-data is a huge negative).
    pub fn no_data(v: f32) -> bool {
        !v.is_finite() || v < -12_000.0
    }

    /// Reads the service's float32 TIFF.
    pub fn parse_tiff(b: &[u8]) -> Result<Grid, String> {
        if b.len() < 8 {
            return Err(format!("not a TIFF: only {} bytes", b.len()));
        }
        let le = match &b[0..2] {
            b"II" => true,
            b"MM" => false,
            _ => return Err("not a TIFF: bad byte-order mark".into()),
        };
        let r = Reader { b, le };
        if r.u16(2)? != 42 {
            return Err("not a classic TIFF".into());
        }
        let ifd = r.u32(4)? as usize;
        let count = r.u16(ifd)? as usize;
        let mut tags = std::collections::HashMap::new();
        for i in 0..count {
            let e = ifd + 2 + i * 12;
            tags.insert(r.u16(e)?, Tag { ty: r.u16(e + 2)?, count: r.u32(e + 4)?, at: e + 8 });
        }
        let scalar = |t: u16| -> Result<Option<u32>, String> {
            match tags.get(&t) {
                None => Ok(None),
                Some(x) if x.ty == 3 => Ok(Some(r.u16(x.at)? as u32)),
                Some(x) => Ok(Some(r.u32(x.at)?)),
            }
        };
        let list = |t: u16| -> Result<Option<Vec<u32>>, String> {
            let Some(x) = tags.get(&t) else { return Ok(None) };
            let size = if x.ty == 3 { 2 } else { 4 };
            let base = if x.count as usize * size <= 4 { x.at } else { r.u32(x.at)? as usize };
            (0..x.count as usize)
                .map(|k| if x.ty == 3 { r.u16(base + k * 2).map(u32::from) } else { r.u32(base + k * 4) })
                .collect::<Result<Vec<_>, _>>()
                .map(Some)
        };
        let (w, h) = match (scalar(256)?, scalar(257)?) {
            (Some(w), Some(h)) if w > 0 && h > 0 => (w as usize, h as usize),
            _ => return Err("TIFF has no usable dimensions".into()),
        };
        if let Some(c) = scalar(259)?
            && c != 1
        {
            return Err(format!("TIFF is compressed (compression {c}); only the service's uncompressed form is read"));
        }
        if let Some(s) = scalar(277)?
            && s != 1
        {
            return Err(format!("TIFF has {s} samples per pixel; expected one elevation band"));
        }
        if scalar(258)? != Some(32) {
            return Err("TIFF sample is not 32-bit".into());
        }
        if scalar(339)? != Some(3) {
            return Err("TIFF sample is not IEEE float".into());
        }
        let mut values = Vec::with_capacity(w * h);
        if let (Some(tw), Some(tl), Some(offs)) = (scalar(322)?, scalar(323)?, list(324)?) {
            let (tw, tl) = (tw as usize, tl as usize);
            let across = w.div_ceil(tw);
            for row in 0..h {
                for col in 0..w {
                    let tile = (row / tl) * across + col / tw;
                    let base = *offs.get(tile).ok_or("TIFF tile missing")? as usize;
                    values.push(r.f32(base + 4 * ((row % tl) * tw + col % tw))?);
                }
            }
        } else if let Some(offs) = list(273)? {
            let rps = scalar(278)?.map_or(h, |v| v as usize);
            for row in 0..h {
                let base = *offs.get(row / rps).ok_or("TIFF strip missing")? as usize + 4 * ((row % rps) * w);
                for col in 0..w {
                    values.push(r.f32(base + 4 * col)?);
                }
            }
        } else {
            return Err("TIFF has neither tiles nor strips".into());
        }
        Ok(Grid { w, h, values })
    }

    /// Lowest and highest real value, or `None` when the grid is all holes.
    pub fn range(&self) -> Option<(f64, f64)> {
        let mut out: Option<(f64, f64)> = None;
        for &v in &self.values {
            if Self::no_data(v) {
                continue;
            }
            let v = f64::from(v);
            out = Some(out.map_or((v, v), |(lo, hi)| (lo.min(v), hi.max(v))));
        }
        out
    }

    /// The value at cell `(c, r)`, or `None` for a hole or outside.
    pub fn at(&self, c: usize, r: usize) -> Option<f64> {
        if c >= self.w || r >= self.h {
            return None;
        }
        let v = self.values[r * self.w + c];
        (!Self::no_data(v)).then_some(f64::from(v))
    }

    /// Bilinear value at fractional cell coordinates (cell centres at integers), or `None` next to a hole.
    pub fn sample_at(&self, x: f64, y: f64) -> Option<f64> {
        if !(x >= 0.0 && y >= 0.0) || x > (self.w - 1) as f64 || y > (self.h - 1) as f64 {
            return None;
        }
        if self.w < 2 || self.h < 2 {
            return self.at(0, 0);
        }
        let c0 = (x.floor() as usize).min(self.w - 2);
        let r0 = (y.floor() as usize).min(self.h - 2);
        let (fx, fy) = (x - c0 as f64, y - r0 as f64);
        let (tl, tr) = (self.at(c0, r0)?, self.at(c0 + 1, r0)?);
        let (bl, br) = (self.at(c0, r0 + 1)?, self.at(c0 + 1, r0 + 1)?);
        Some(tl * (1.0 - fx) * (1.0 - fy) + tr * fx * (1.0 - fy) + bl * (1.0 - fx) * fy + br * fx * fy)
    }

    /// The UTM point of fractional cell `(c, r)` (cell centres), for a grid cut to `b`.
    pub fn cell_utm(&self, b: &Bounds, c: f64, r: f64) -> (f64, f64) {
        let (sx, sy) = (b.width() / self.w as f64, b.height() / self.h as f64);
        (b.xmin + (c + 0.5) * sx, b.ymax - (r + 0.5) * sy)
    }

    /// The fractional cell of a UTM point (inverse of [`Grid::cell_utm`]).
    pub fn utm_cell(&self, b: &Bounds, e: f64, n: f64) -> (f64, f64) {
        let (sx, sy) = (b.width() / self.w as f64, b.height() / self.h as f64);
        ((e - b.xmin) / sx - 0.5, (b.ymax - n) / sy - 0.5)
    }
}
```

- [ ] **Step 5: Pass + commit**

```bash
cargo test -p cave-geo --test grid
cargo fmt -p cave-geo && cargo clippy -p cave-geo --all-targets -- -D warnings
python3 tools/make_code_index.py
git add crates/cave-geo docs/CODE-INDEX.md
git commit -m "feat(cave-geo): read the 3DEP float TIFF into an elevation grid"
```

---

### Task 5: Contours (levels, marching squares, stitching)

**Goal:** Port `CsContour.levels`, `lines` and `stitch` with the old `js_unit` cases.

**Files:**
- Create: `crates/cave-geo/src/contour.rs`
- Modify: `crates/cave-geo/src/lib.rs` (`pub mod contour;`)
- Test: `crates/cave-geo/tests/contour.rs`

**Acceptance Criteria:**
- [ ] `levels(186.6, 232.0, 10.0)` = `[190, 200, 210, 220, 230]`; empty range and zero interval give `[]`.
- [ ] Ramp 3×3 `[0,1,2]×3` at 0.5 → one open run of 3 points, all x = 0.5, y from 0 to 2.
- [ ] 5×5 peak at 2.0 → one closed run; a no-data corner at level 0.5 does not panic and yields ≥ 1 run.
- [ ] The real fixture at every 2 m level yields at least one run and every point lies inside the grid.

**Verify:** `cargo test -p cave-geo --test contour` → all pass

**Steps:**

- [ ] **Step 1: Failing tests**

```rust
// crates/cave-geo/tests/contour.rs
use cave_geo::contour::{levels, lines};
use cave_geo::grid::Grid;

#[test]
fn level_list() {
    assert_eq!(levels(186.6, 232.0, 10.0), vec![190.0, 200.0, 210.0, 220.0, 230.0]);
    assert!(levels(5.0, 5.0, 10.0).is_empty());
    assert!(levels(0.0, 100.0, 0.0).is_empty());
}

#[test]
fn ramp() {
    let g = Grid { w: 3, h: 3, values: vec![0., 1., 2., 0., 1., 2., 0., 1., 2.] };
    let runs = lines(&g, 0.5);
    assert_eq!(runs.len(), 1);
    assert!(!runs[0].closed);
    assert_eq!(runs[0].points.len(), 3);
    assert!(runs[0].points.iter().all(|p| (p[0] - 0.5).abs() < 1e-9));
    let ys: Vec<f64> = runs[0].points.iter().map(|p| p[1]).collect();
    assert_eq!(ys.iter().cloned().fold(f64::MAX, f64::min), 0.0);
    assert_eq!(ys.iter().cloned().fold(f64::MIN, f64::max), 2.0);
}

#[test]
fn peak_closes_and_holes_are_safe() {
    let mut v = vec![0., 0., 0., 0., 0., 0., 1., 1., 1., 0., 0., 1., 4., 1., 0., 0., 1., 1., 1., 0., 0., 0., 0., 0., 0.];
    let g = Grid { w: 5, h: 5, values: v.clone() };
    let runs = lines(&g, 2.0);
    assert_eq!(runs.len(), 1);
    assert!(runs[0].closed);
    v[0] = -999_999.0;
    let holed = Grid { w: 5, h: 5, values: v };
    assert!(!lines(&holed, 0.5).is_empty());
}

#[test]
fn real_grid() {
    let g = Grid::parse_tiff(include_bytes!("data/bowling-green-200.tif")).unwrap();
    let (lo, hi) = g.range().unwrap();
    let lv = levels(lo, hi, 2.0);
    assert!(!lv.is_empty());
    let runs: Vec<_> = lv.iter().flat_map(|l| lines(&g, *l)).collect();
    assert!(!runs.is_empty());
    for r in runs {
        for p in r.points {
            assert!(p[0] >= 0.0 && p[0] <= 199.0 && p[1] >= 0.0 && p[1] <= 199.0);
        }
    }
}
```

- [ ] **Step 2: Run to fail** — `cargo test -p cave-geo --test contour` → FAIL.

- [ ] **Step 3: Implement** (direct port; same case table, saddle rule and 1/4096 key rounding)

```rust
// crates/cave-geo/src/contour.rs
//! Contour lines from an elevation grid: marching squares with linear interpolation, stitched into
//! runs. Points are in CELL coordinates (x = column, y = row from the top; cell centres at integers);
//! the caller maps them to the world. Ported from the old CsContour.levels/lines/stitch.

use crate::grid::Grid;
use std::collections::HashMap;

/// One contour line in cell coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    /// The points, in order.
    pub points: Vec<[f64; 2]>,
    /// True when the line closes on itself (the last point joins the first; not repeated).
    pub closed: bool,
}

/// Every multiple of `interval` strictly inside `(min, max)`, walked by index so float drift never
/// skips or repeats a level.
pub fn levels(min: f64, max: f64, interval: f64) -> Vec<f64> {
    let mut out = Vec::new();
    if !(interval > 0.0) || !min.is_finite() || !max.is_finite() {
        return out;
    }
    let mut k = (min / interval).ceil() as i64;
    while (k as f64) * interval < max {
        let v = k as f64 * interval;
        if v > min {
            out.push(v);
        }
        k += 1;
    }
    out
}

/// The contour runs at `level` (same unit as the grid). Cells touching a hole contribute nothing.
pub fn lines(g: &Grid, level: f64) -> Vec<Run> {
    let interp = |a: f64, b: f64| {
        let d = b - a;
        if d == 0.0 { 0.5 } else { ((level - a) / d).clamp(0.0, 1.0) }
    };
    let mut segs: Vec<[f64; 4]> = Vec::new();
    for r in 0..g.h.saturating_sub(1) {
        for c in 0..g.w.saturating_sub(1) {
            let (Some(tl), Some(tr), Some(br), Some(bl)) = (g.at(c, r), g.at(c + 1, r), g.at(c + 1, r + 1), g.at(c, r + 1)) else {
                continue;
            };
            let idx = (u8::from(tl >= level) << 3) | (u8::from(tr >= level) << 2) | (u8::from(br >= level) << 1) | u8::from(bl >= level);
            if idx == 0 || idx == 15 {
                continue;
            }
            let (cf, rf) = (c as f64, r as f64);
            let top = [cf + interp(tl, tr), rf];
            let right = [cf + 1.0, rf + interp(tr, br)];
            let bottom = [cf + interp(bl, br), rf + 1.0];
            let left = [cf, rf + interp(tl, bl)];
            let mut add = |p: [f64; 2], q: [f64; 2]| segs.push([p[0], p[1], q[0], q[1]]);
            match idx {
                1 => add(left, bottom),
                2 => add(bottom, right),
                3 => add(left, right),
                4 => add(top, right),
                6 => add(top, bottom),
                7 => add(left, top),
                8 => add(top, left),
                9 => add(top, bottom),
                11 => add(top, right),
                12 => add(right, left),
                13 => add(right, bottom),
                14 => add(bottom, left),
                5 | 10 => {
                    let high = (tl + tr + br + bl) / 4.0 >= level;
                    match (idx, high) {
                        (5, true) => { add(left, top); add(bottom, right); }
                        (5, false) => { add(left, bottom); add(top, right); }
                        (_, true) => { add(top, right); add(bottom, left); }
                        (_, false) => { add(top, left); add(bottom, right); }
                    }
                }
                _ => {}
            }
        }
    }
    stitch(&segs)
}

fn key(x: f64, y: f64) -> (i64, i64) {
    ((x * 4096.0).round() as i64, (y * 4096.0).round() as i64)
}

/// Chains loose segments into runs by shared endpoints; every segment is used once.
fn stitch(segs: &[[f64; 4]]) -> Vec<Run> {
    let mut ends: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, s) in segs.iter().enumerate() {
        ends.entry(key(s[0], s[1])).or_default().push(i);
        ends.entry(key(s[2], s[3])).or_default().push(i);
    }
    let mut used = vec![false; segs.len()];
    let mut out = Vec::new();
    for i in 0..segs.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        let mut pts = std::collections::VecDeque::from([[segs[i][0], segs[i][1]], [segs[i][2], segs[i][3]]]);
        for at_end in [true, false] {
            loop {
                let tip = if at_end { *pts.back().unwrap() } else { *pts.front().unwrap() };
                let k = key(tip[0], tip[1]);
                let Some(&next) = ends.get(&k).and_then(|l| l.iter().find(|&&j| !used[j])) else { break };
                used[next] = true;
                let s = segs[next];
                let p = if key(s[0], s[1]) == k { [s[2], s[3]] } else { [s[0], s[1]] };
                if at_end { pts.push_back(p) } else { pts.push_front(p) }
            }
        }
        let mut points: Vec<[f64; 2]> = pts.into_iter().collect();
        let closed = points.len() > 2 && key(points[0][0], points[0][1]) == key(points[points.len() - 1][0], points[points.len() - 1][1]);
        if closed {
            points.pop();
        }
        out.push(Run { points, closed });
    }
    out
}
```

- [ ] **Step 4: Pass + commit**

```bash
cargo test -p cave-geo --test contour
cargo fmt -p cave-geo && cargo clippy -p cave-geo --all-targets -- -D warnings
python3 tools/make_code_index.py
git add crates/cave-geo docs/CODE-INDEX.md
git commit -m "feat(cave-geo): contour levels and marching squares, ported with the old tests"
```

---

### Task 6: Lowest ground inside a circle

**Goal:** Given a grid, its window, a placement, and a circle in cave metres, return the lowest cell inside (ties nearest the centre) as cave point + lat/lon + ground height.

**Files:**
- Create: `crates/cave-geo/src/pick.rs`
- Modify: `crates/cave-geo/src/lib.rs` (`pub mod pick;`)
- Test: `crates/cave-geo/tests/pick.rs`

**Acceptance Criteria:**
- [ ] A bowl grid (value = distance² from cell (6, 4)) picked with a circle covering (6, 4) returns that cell's centre.
- [ ] A circle that excludes the true minimum returns the lowest cell INSIDE it, never outside.
- [ ] Equal lows: the one nearer the circle centre wins.
- [ ] Holes are skipped; a circle with only holes or entirely off the grid returns `None`.
- [ ] Returned `lat/lon` equals `place.cave_to_latlon(point)`.

**Verify:** `cargo test -p cave-geo --test pick` → all pass

**Steps:**

- [ ] **Step 1: Failing tests**

```rust
// crates/cave-geo/tests/pick.rs
use cave_geo::frame::{LatLon, Placement};
use cave_geo::grid::Grid;
use cave_geo::pick::lowest_in_circle;
use cave_geo::request::Bounds;

const BG: LatLon = LatLon { lat: 36.99, lon: -86.44 };

fn setup(values: impl Fn(usize, usize) -> f32) -> (Grid, Bounds, Placement) {
    let place = Placement::new([0.0, 0.0], BG);
    let (e0, n0) = place.cave_to_utm(0.0, 0.0);
    let b = Bounds { xmin: e0 - 10.0, ymin: n0 - 10.0, xmax: e0 + 10.0, ymax: n0 + 10.0 };
    let mut v = Vec::new();
    for r in 0..20 {
        for c in 0..20 {
            v.push(values(c, r));
        }
    }
    (Grid { w: 20, h: 20, values: v }, b, place)
}

fn cave_of(g: &Grid, b: &Bounds, p: &Placement, c: f64, r: f64) -> [f64; 2] {
    let (e, n) = g.cell_utm(b, c, r);
    p.utm_to_cave(e, n)
}

#[test]
fn finds_the_bowl() {
    let (g, b, p) = setup(|c, r| ((c as f32 - 6.0).powi(2) + (r as f32 - 4.0).powi(2)) + 100.0);
    let at = cave_of(&g, &b, &p, 6.0, 4.0);
    let hit = lowest_in_circle(&g, &b, &p, [at[0] + 1.5, at[1] - 1.0], 4.0).unwrap();
    assert!((hit.point[0] - at[0]).abs() < 1e-6 && (hit.point[1] - at[1]).abs() < 1e-6);
    assert_eq!(hit.ground_m, 100.0);
    let ll = p.cave_to_latlon(hit.point[0], hit.point[1]);
    assert_eq!((hit.at.lat, hit.at.lon), (ll.lat, ll.lon));
}

#[test]
fn stays_inside_the_circle() {
    let (g, b, p) = setup(|c, r| ((c as f32 - 6.0).powi(2) + (r as f32 - 4.0).powi(2)) + 100.0);
    let far = cave_of(&g, &b, &p, 15.0, 15.0);
    let hit = lowest_in_circle(&g, &b, &p, far, 2.0).unwrap();
    let d = ((hit.point[0] - far[0]).powi(2) + (hit.point[1] - far[1]).powi(2)).sqrt();
    assert!(d <= 2.0 + 1e-9);
    assert!(hit.ground_m > 100.0);
}

#[test]
fn ties_go_to_the_centre() {
    let (g, b, p) = setup(|c, r| if (c == 3 && r == 3) || (c == 8 && r == 3) { 50.0 } else { 90.0 });
    let near8 = cave_of(&g, &b, &p, 7.0, 3.0);
    let hit = lowest_in_circle(&g, &b, &p, near8, 6.0).unwrap();
    let want = cave_of(&g, &b, &p, 8.0, 3.0);
    assert!((hit.point[0] - want[0]).abs() < 1e-6);
}

#[test]
fn holes_and_off_grid() {
    let (g, b, p) = setup(|_, _| -1.0e30);
    assert!(lowest_in_circle(&g, &b, &p, [0.0, 0.0], 5.0).is_none());
    let (g, b, p) = setup(|_, _| 10.0);
    assert!(lowest_in_circle(&g, &b, &p, [500.0, 500.0], 5.0).is_none());
}
```

- [ ] **Step 2: Run to fail** — `cargo test -p cave-geo --test pick` → FAIL.

- [ ] **Step 3: Implement**

```rust
// crates/cave-geo/src/pick.rs
//! The entrance pick: the lowest ground inside a circle the person drags on the Map. An entrance is
//! at the bottom of something, so the click only has to be close (old EntranceLocation snapped to the
//! lowest CONTOUR vertex; this reads the grid itself, so a low spot between contour lines counts).

use crate::frame::{LatLon, Placement};
use crate::grid::Grid;
use crate::request::Bounds;

/// Where the pick landed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LowPoint {
    /// Cave metres.
    pub point: [f64; 2],
    /// WGS84.
    pub at: LatLon,
    /// Ground there, metres NAVD88.
    pub ground_m: f64,
}

/// The lowest cell centre within `radius` (metres) of `centre` (cave metres); a tie goes to the
/// cell nearer the centre. `None` when the circle holds no real ground.
pub fn lowest_in_circle(g: &Grid, b: &Bounds, place: &Placement, centre: [f64; 2], radius: f64) -> Option<LowPoint> {
    let (ce, cn) = place.cave_to_utm(centre[0], centre[1]);
    let (cc, cr) = g.utm_cell(b, ce, cn);
    let span = (radius / (b.width() / g.w as f64).min(b.height() / g.h as f64)).ceil() + 1.0;
    let (w, h) = (g.w as i64, g.h as i64);
    let c_lo = ((cc - span).floor() as i64).max(0);
    let r_lo = ((cr - span).floor() as i64).max(0);
    let c_hi = ((cc + span).ceil() as i64).min(w - 1);
    let r_hi = ((cr + span).ceil() as i64).min(h - 1);
    let mut best: Option<(f64, f64, [f64; 2])> = None; // (ground, distance, point)
    for r in r_lo..=r_hi {
        for c in c_lo..=c_hi {
            let Some(v) = g.at(c as usize, r as usize) else { continue };
            let (e, n) = g.cell_utm(b, c as f64, r as f64);
            let p = place.utm_to_cave(e, n);
            let d = ((p[0] - centre[0]).powi(2) + (p[1] - centre[1]).powi(2)).sqrt();
            if d > radius {
                continue;
            }
            if best.is_none_or(|(bv, bd, _)| v < bv || (v == bv && d < bd)) {
                best = Some((v, d, p));
            }
        }
    }
    best.map(|(ground_m, _, point)| LowPoint { point, at: place.cave_to_latlon(point[0], point[1]), ground_m })
}
```

A circle entirely off the grid gives an empty range (`lo > hi`), so the loops do nothing and the result is `None`.

- [ ] **Step 4: Pass + commit**

```bash
cargo test -p cave-geo --test pick
cargo fmt -p cave-geo && cargo clippy -p cave-geo --all-targets -- -D warnings
python3 tools/make_code_index.py
git add crates/cave-geo docs/CODE-INDEX.md
git commit -m "feat(cave-geo): lowest ground inside a dragged circle"
```

---

### Task 7: Stored records (location, surface, interval, fixed frame) and the location candidate

**Goal:** Read/write the four `Setting` records as whole JSON values, and decide where a cave's location comes from (stored → imported fixed point → ask).

**Files:**
- Create: `crates/cave-geo/src/record.rs`
- Modify: `crates/cave-geo/src/lib.rs` (`pub mod record;`)
- Test: `crates/cave-geo/tests/record.rs`

**Acceptance Criteria:**
- [ ] `setting_id(k)` equals `cave_draw::setting_id(k)` for all four keys.
- [ ] `set_location` then `location` returns the same record; a second `set_location` on an existing entity emits only the value `Set`.
- [ ] Two documents applying two different `set_location` edits in opposite orders converge to the same whole record (never mixed fields).
- [ ] `candidate`: stored location wins; else a station with `StationFixedX/Y` + a `fixed-frame` setting gives `Imported` with lat/lon from `FixedFrame::to_latlon`; else `Ask` naming the station labelled `A1` if any.
- [ ] A `ground_m` that is absent stays absent (never 0) through JSON.

**Verify:** `cargo test -p cave-geo --test record` → all pass

**Steps:**

- [ ] **Step 1: Read the doc API you will use** — `crates/cave-doc/src/lib.rs`: `Document::entity(id) -> Option<&Entity>`, `Entity::text(Field)`, `Entity::number(Field) -> Option<f64>`, `Document::ids_of(Kind)`, `Document::edit(Vec<Action>) -> Result<Edit, _>`, `Document::apply`/merge used by convergence tests in `crates/cave-doc/tests/` (copy the two-replica pattern from the existing convergence test there; find it with `grep -rn "converge" crates/cave-doc/tests`).

- [ ] **Step 2: Failing tests**

```rust
// crates/cave-geo/tests/record.rs
use cave_doc::{Action, Document, Field, Kind, Value};
use cave_geo::datum::{Datum, FixedFrame};
use cave_geo::record::*;
use cave_id::Id;

fn station(doc: &mut Document, label: &str) -> Id {
    let id = Id::from_name(&format!("test-station:{label}"));
    doc.edit(vec![
        Action::Create { id, kind: Kind::Station },
        Action::Set { id, field: Field::StationLabel, value: Value::Text(label.into()) },
    ]).unwrap();
    id
}

#[test]
fn ids_match_cave_draw() {
    for k in [LOCATION, SURFACE, CONTOUR_INTERVAL, FIXED_FRAME] {
        assert_eq!(setting_id(k), cave_draw::setting_id(k));
    }
}

#[test]
fn location_round_trip() {
    let mut doc = Document::default();
    let a1 = station(&mut doc, "A1");
    let loc = Location { station: a1, lat: 36.99, lon: -86.44, ground_m: None, source: "typed".into() };
    doc.edit(set_location(&doc, &loc)).unwrap();
    assert_eq!(location(&doc), Some(loc.clone()));
    let again = Location { ground_m: Some(160.25), ..loc };
    let acts = set_location(&doc, &again);
    assert_eq!(acts.len(), 1);
    doc.edit(acts).unwrap();
    assert_eq!(location(&doc).unwrap().ground_m, Some(160.25));
    let j = serde_json::to_string(&Location { ground_m: None, ..again }).unwrap();
    assert!(!j.contains("ground_m"), "{j}");
}

#[test]
fn candidate_order() {
    let mut doc = Document::default();
    let a1 = station(&mut doc, "A1");
    assert_eq!(candidate(&doc), Candidate::Ask { station: Some(a1) });
    let fx = station(&mut doc, "FIX");
    doc.edit(vec![
        Action::Set { id: fx, field: Field::StationFixedX, value: Value::Number(Some(550_000.0)) },
        Action::Set { id: fx, field: Field::StationFixedY, value: Value::Number(Some(4_094_000.0)) },
    ]).unwrap();
    // fixed coordinates without a frame are not a location
    assert_eq!(candidate(&doc), Candidate::Ask { station: Some(a1) });
    doc.edit(set_fixed_frame(&doc, FixedFrame { zone: 16, datum: Datum::Nad83 })).unwrap();
    let want = FixedFrame { zone: 16, datum: Datum::Nad83 }.to_latlon(550_000.0, 4_094_000.0);
    assert_eq!(candidate(&doc), Candidate::Imported { station: fx, at: want });
    let loc = Location { station: a1, lat: 1.0, lon: 2.0, ground_m: None, source: "map".into() };
    doc.edit(set_location(&doc, &loc)).unwrap();
    assert_eq!(candidate(&doc), Candidate::Stored(loc));
}
```

Add a convergence test in the same file using the replica pattern found in Step 1: replica A and B both start from the same doc, A writes location L1, B writes L2; exchange ops both ways; assert `location(&a) == location(&b)` and that it equals L1 or L2 entirely.

- [ ] **Step 3: Run to fail** — `cargo test -p cave-geo --test record` → FAIL.

- [ ] **Step 4: Implement**

```rust
// crates/cave-geo/src/record.rs
//! What a cave stores about where it is, as drawing-wide `Setting` entities. Each record is ONE
//! JSON value written whole, so two people setting the location at once end with one of the two
//! locations, never the latitude of one and the longitude of the other.

use crate::datum::FixedFrame;
use crate::frame::LatLon;
use crate::request::Bounds;
use cave_doc::{Action, Document, Field, Kind, Value};
use cave_id::Id;
use serde::{Deserialize, Serialize};

/// Setting key of the cave's location.
pub const LOCATION: &str = "location";
/// Setting key of the downloaded surface's request.
pub const SURFACE: &str = "surface";
/// Setting key of the contour spacing.
pub const CONTOUR_INTERVAL: &str = "contour-interval";
/// Setting key of the imported fixed points' coordinate system.
pub const FIXED_FRAME: &str = "fixed-frame";

/// The id of a named setting (same rule as `cave_draw::setting_id`).
pub fn setting_id(key: &str) -> Id {
    Id::from_name(&format!("setting:{key}"))
}

/// Where the cave is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Location {
    /// The entrance station (its id, not its label).
    pub station: Id,
    /// WGS84 degrees.
    pub lat: f64,
    /// WGS84 degrees.
    pub lon: f64,
    /// Raw 3DEP ground at that point, metres NAVD88. Absent = unknown, NEVER zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ground_m: Option<f64>,
    /// `import`, `typed`, `map` or `surface-pick`.
    pub source: String,
}

impl Location {
    /// The coordinate.
    pub fn at(&self) -> LatLon {
        LatLon { lat: self.lat, lon: self.lon }
    }
}

/// What was downloaded, in UTM metres of `zone`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SurfaceRecord {
    /// UTM zone of `bounds`.
    pub zone: i32,
    /// The window.
    pub bounds: Bounds,
    /// Photo size in pixels, when one was fetched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub photo: Option<[u32; 2]>,
    /// Elevation grid size, when one was fetched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elevation: Option<[u32; 2]>,
    /// The location it was cut around (a revised entrance makes this stale).
    pub for_at: LatLon,
    /// Download date, `YYYY-MM-DD` (passed in; core code reads no clock).
    pub fetched: String,
}

fn read<T: for<'de> Deserialize<'de>>(doc: &Document, key: &str) -> Option<T> {
    let e = doc.entity(setting_id(key))?;
    serde_json::from_str(&e.text(Field::SettingValue)).ok()
}

fn write<T: Serialize>(doc: &Document, key: &str, value: &T) -> Vec<Action> {
    let id = setting_id(key);
    let text = serde_json::to_string(value).expect("records serialize");
    let mut out = Vec::new();
    if doc.entity(id).is_none() {
        out.push(Action::Create { id, kind: Kind::Setting });
        out.push(Action::Set { id, field: Field::SettingKey, value: Value::Text(key.into()) });
    }
    if doc.entity(id).is_none_or(|e| e.text(Field::SettingValue) != text) {
        out.push(Action::Set { id, field: Field::SettingValue, value: Value::Text(text) });
    }
    out
}

/// The stored location.
pub fn location(doc: &Document) -> Option<Location> {
    read(doc, LOCATION)
}
/// Actions storing `loc` whole.
pub fn set_location(doc: &Document, loc: &Location) -> Vec<Action> {
    write(doc, LOCATION, loc)
}
/// The stored surface request.
pub fn surface(doc: &Document) -> Option<SurfaceRecord> {
    read(doc, SURFACE)
}
/// Actions storing the surface request whole.
pub fn set_surface(doc: &Document, s: &SurfaceRecord) -> Vec<Action> {
    write(doc, SURFACE, s)
}
/// The contour spacing in METRES, if the person set one.
pub fn contour_interval_m(doc: &Document) -> Option<f64> {
    read::<f64>(doc, CONTOUR_INTERVAL).filter(|v| v.is_finite() && *v > 0.0)
}
/// Actions storing the contour spacing (metres).
pub fn set_contour_interval_m(doc: &Document, m: f64) -> Vec<Action> {
    write(doc, CONTOUR_INTERVAL, &m)
}
/// The imported fixed points' coordinate system.
pub fn fixed_frame(doc: &Document) -> Option<FixedFrame> {
    read(doc, FIXED_FRAME)
}
/// Actions storing the imported fixed points' coordinate system.
pub fn set_fixed_frame(doc: &Document, f: FixedFrame) -> Vec<Action> {
    write(doc, FIXED_FRAME, &f)
}

/// Where Surface Data gets the cave's location.
#[derive(Clone, Debug, PartialEq)]
pub enum Candidate {
    /// Already stored.
    Stored(Location),
    /// From an imported fixed point (converted to WGS84).
    Imported {
        /// The fixed station.
        station: Id,
        /// Its coordinate.
        at: LatLon,
    },
    /// Ask with the map picker; the station labelled A1 is the suggested entrance.
    Ask {
        /// The station labelled `A1`, if there is one.
        station: Option<Id>,
    },
}

/// Stored location, else the first fixed station (by label) when the import named its frame, else ask.
pub fn candidate(doc: &Document) -> Candidate {
    if let Some(l) = location(doc) {
        return Candidate::Stored(l);
    }
    let mut stations: Vec<(String, Id)> = doc
        .ids_of(Kind::Station)
        .into_iter()
        .filter_map(|id| doc.entity(id).map(|e| (e.text(Field::StationLabel), id)))
        .collect();
    stations.sort();
    if let Some(frame) = fixed_frame(doc) {
        for (_, id) in &stations {
            let e = doc.entity(*id).expect("listed");
            if let (Some(x), Some(y)) = (e.number(Field::StationFixedX), e.number(Field::StationFixedY)) {
                return Candidate::Imported { station: *id, at: frame.to_latlon(x, y) };
            }
        }
    }
    Candidate::Ask { station: stations.iter().find(|(l, _)| l == "A1").map(|(_, id)| *id) }
}
```

If `Entity::text` returns `&str` rather than `String`, adjust the two call sites (`.to_string()` / compare `&str`). If `Document::default()` is not how the tests in `crates/cave-doc/tests` build an empty document, use theirs.

- [ ] **Step 5: Pass + commit**

```bash
cargo test -p cave-geo --test record
cargo fmt -p cave-geo && cargo clippy -p cave-geo --all-targets -- -D warnings
python3 tools/make_code_index.py
git add crates/cave-geo docs/CODE-INDEX.md
git commit -m "feat(cave-geo): location, surface, contour and fixed-frame records; location candidate"
```

---

### Task 8: Importers record the fixed points' frame

**Goal:** Compass `.mak` (`&datum;` + `@`/`$` zone) and Walls `.wpj` (`.REF` zone + quoted datum name) record a `FixedFrame` on the survey; `cave-formats::document` writes it as the `fixed-frame` setting.

**Files:**
- Modify: `crates/cave-formats/Cargo.toml` (add `cave-geo = { path = "../cave-geo" }`)
- Modify: `crates/cave-formats/src/model.rs:258-295` (`Survey`: add `fixed_frame`)
- Modify: `crates/cave-formats/src/project.rs:330-375` (`MakOptions`: read `&`, apply), `:487-510` (`wpj_ref` datum), Walls frame propagation into the merged survey
- Modify: `crates/cave-formats/src/document.rs:190-215` (write the setting)
- Test: `crates/cave-formats/tests/edges.rs` (add cases)

**Acceptance Criteria:**
- [ ] A `.mak` with `@550000.0,4094000.0,150.0,16,0.0;` and `&North American 1927;` gives `survey.fixed_frame == Some(FixedFrame{zone:16, datum:Nad27})`.
- [ ] A `.mak` with a zone but no `&` line gives `None` (datum unknown → map picker asks).
- [ ] A `.wpj` `.REF ... 16 ... 27 "North American 1927"` gives zone 16, NAD27.
- [ ] `to_document` of such a survey stores the `fixed-frame` setting, and `cave_geo::record::fixed_frame` reads it back.
- [ ] All existing `cave-formats` tests still pass (golden JSON unchanged: the field is skipped when `None`).

**Verify:** `cargo test -p cave-formats` → all pass

**Steps:**

- [ ] **Step 1: Failing tests** — append to `crates/cave-formats/tests/edges.rs`, using the same `read_project` call style the file's existing `.mak` tests use (find one with `grep -n "read_project" crates/cave-formats/tests/edges.rs` and copy its reader closure):

```rust
#[test]
fn mak_records_fixed_frame() {
    use cave_geo::datum::{Datum, FixedFrame};
    let mak = "@550000.0,4094000.0,150.0,16,0.0;\n&North American 1927;\n#a.dat,A1[m,550000.0,4094000.0,150.0];\n";
    let dat = "Cave\nSURVEY NAME: A\nSURVEY DATE: 1 1 2020\nSURVEY TEAM:\nx\nDECLINATION: 0.00 FORMAT: DDDDLUDRADLN CORRECTIONS: 0.00 0.00 0.00\n\nFROM TO LEN BEAR INC LEFT UP DOWN RIGHT\n\nA1 A2 10.0 0.0 0.0 1 1 1 1\n\x0c\n";
    let read = |p: &str| (p == "a.dat").then(|| dat.to_string());
    let r = cave_formats::read_project(cave_formats::ProjectFormat::CompassMak, "c.mak", mak, &read);
    assert_eq!(r.survey.fixed_frame, Some(FixedFrame { zone: 16, datum: Datum::Nad27 }));
    let no_datum = mak.replace("&North American 1927;\n", "");
    let r = cave_formats::read_project(cave_formats::ProjectFormat::CompassMak, "c.mak", &no_datum, &read);
    assert_eq!(r.survey.fixed_frame, None);
}

#[test]
fn document_stores_fixed_frame() {
    use cave_geo::datum::{Datum, FixedFrame};
    let mut s = cave_formats::Survey::default();
    s.fixed_frame = Some(FixedFrame { zone: 16, datum: Datum::Nad83 });
    let doc = cave_formats::to_document(&s).unwrap();   // use the real name of the Survey -> Document function
    assert_eq!(cave_geo::record::fixed_frame(&doc), Some(FixedFrame { zone: 16, datum: Datum::Nad83 }));
}
```

Adjust the `.dat` text to match a passing fixture in `crates/cave-formats/tests/` if the minimal one above does not parse, and use the real public name of the survey→document function from `crates/cave-formats/src/document.rs` (look for `pub fn` near line 1). Add the matching `.wpj` test using the `.REF` line format documented at `project.rs:487`.

- [ ] **Step 2: Run to fail** — `cargo test -p cave-formats` → FAIL (no field).

- [ ] **Step 3: Implement**

`model.rs`, inside `Survey` after `sketches`:

```rust
    /// The coordinate system `fixed` is written in (UTM zone + datum), when the project names it.
    /// `None` means unknown: the location is then asked for, never guessed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed_frame: Option<cave_geo::datum::FixedFrame>,
```

`project.rs` `MakOptions`: add field `datum: Option<cave_geo::datum::Datum>,`; in `read` add arm
`Some('&') => self.datum = cave_geo::datum::Datum::parse(body).or(self.datum),`; in `apply` (top, before the early `return`):

```rust
        if let (Some(zone), Some(datum)) = (self.zone.filter(|z| *z != 0), self.datum) {
            survey.fixed_frame = Some(cave_geo::datum::FixedFrame { zone, datum });
        }
```

Update the module doc at `project.rs:5`: `&` datum is now read (not "read past").

`wpj_ref`: after the 12 numbers, take the first double-quoted substring of `rest` and `Datum::parse` it; return it alongside `GeoRef` (change the return type to `Option<(GeoRef, Option<Datum>)>`, update its one caller). Where the `.wpj` reader builds the merged survey, set `survey.fixed_frame = Some(FixedFrame { zone: georef.zone, datum })` when both are known (use the root book's `.REF`).

`document.rs`, after the fixed-point loop (before `Ok(doc)`):

```rust
    if let Some(frame) = survey.fixed_frame {
        edit(&mut doc, cave_geo::record::set_fixed_frame(&doc, frame))?;
    }
```

And in the document→survey direction (`document.rs:~367`), read it back: `survey.fixed_frame = cave_geo::record::fixed_frame(doc);`.

- [ ] **Step 4: Pass + commit**

```bash
cargo test -p cave-formats && cargo test -p cave-geo
cargo fmt -p cave-formats && cargo clippy -p cave-formats --all-targets -- -D warnings
python3 tools/make_code_index.py
git add crates/cave-formats docs/CODE-INDEX.md Cargo.lock
git commit -m "feat(cave-formats): record the fixed points' UTM zone and datum on import"
```

---

### Task 9: Surface layers in the cave frame (`cave-geo::layers`)

**Goal:** One pure function turning a stored surface + grid + placement into what the views draw: the photo's pixel→cave matrix, contour polylines in cave metres (major every 5th), and a terrain mesh with the datum offset applied; plus the datum offset rule.

**Files:**
- Create: `crates/cave-geo/src/layers.rs`
- Modify: `crates/cave-geo/src/lib.rs` (`pub mod layers;`)
- Test: `crates/cave-geo/tests/layers.rs`

**Acceptance Criteria:**
- [ ] `photo_matrix` maps pixel (0,0) to the cave point of the window's NW corner and (w,h) to the SE corner (< 1 mm).
- [ ] `datum_offset(Some(200.0), 1.524, -30.0)` = 200 − 1.524 − (−30) = 228.476; `datum_offset(None, …)` = `None`.
- [ ] `contours` labels: levels are in the DISPLAY unit (ft or m) and majors are those whose index (level/interval) is a multiple of 5.
- [ ] `terrain` returns `w*h` vertices; with offset `None` it returns `None` (no terrain at 0).

**Verify:** `cargo test -p cave-geo --test layers` → all pass

**Steps:**

- [ ] **Step 1: Failing tests**

```rust
// crates/cave-geo/tests/layers.rs
use cave_geo::frame::{LatLon, Placement};
use cave_geo::grid::Grid;
use cave_geo::layers::{ENTRANCE_DEPTH_M, contours, datum_offset, photo_matrix, terrain};
use cave_geo::request::Bounds;

const BG: LatLon = LatLon { lat: 36.99, lon: -86.44 };

#[test]
fn matrix_corners() {
    let p = Placement::new([5.0, 5.0], BG);
    let (e, n) = p.cave_to_utm(5.0, 5.0);
    let b = Bounds { xmin: e - 100.0, ymin: n - 50.0, xmax: e + 100.0, ymax: n + 50.0 };
    let m = photo_matrix(&b, (400, 200), &p);
    let apply = |px: f64, py: f64| [m[0] * px + m[1] * py + m[2], m[3] * px + m[4] * py + m[5]];
    let nw = p.utm_to_cave(b.xmin, b.ymax);
    let se = p.utm_to_cave(b.xmax, b.ymin);
    let a = apply(0.0, 0.0);
    let z = apply(400.0, 200.0);
    assert!((a[0] - nw[0]).abs() < 1e-3 && (a[1] - nw[1]).abs() < 1e-3);
    assert!((z[0] - se[0]).abs() < 1e-3 && (z[1] - se[1]).abs() < 1e-3);
}

#[test]
fn offset_rule() {
    assert!((ENTRANCE_DEPTH_M - 1.524).abs() < 1e-12);
    assert!((datum_offset(Some(200.0), -30.0).unwrap() - 228.476).abs() < 1e-9);
    assert_eq!(datum_offset(None, -30.0), None);
}

#[test]
fn majors_every_fifth() {
    let g = Grid { w: 3, h: 3, values: vec![0., 30., 60., 0., 30., 60., 0., 30., 60.] };
    let p = Placement::new([0.0, 0.0], BG);
    let (e, n) = p.cave_to_utm(0.0, 0.0);
    let b = Bounds { xmin: e, ymin: n, xmax: e + 3.0, ymax: n + 3.0 };
    let lines = contours(&g, &b, &p, 10.0, false, None);
    let lv: Vec<(f64, bool)> = lines.iter().map(|l| (l.level, l.major)).collect();
    assert!(lv.contains(&(50.0, true)) && lv.contains(&(10.0, false)));
    // feet: 10 ft interval, levels in feet
    let ft = contours(&g, &b, &p, 10.0, true, None);
    assert!(ft.iter().any(|l| l.level == 190.0)); // 60 m = 196.85 ft, so 190 ft is the top level
}

#[test]
fn terrain_needs_a_datum() {
    let g = Grid { w: 2, h: 2, values: vec![1., 2., 3., 4.] };
    let p = Placement::new([0.0, 0.0], BG);
    let (e, n) = p.cave_to_utm(0.0, 0.0);
    let b = Bounds { xmin: e, ymin: n, xmax: e + 2.0, ymax: n + 2.0 };
    assert!(terrain(&g, &b, &p, None).is_none());
    let t = terrain(&g, &b, &p, Some(-100.0)).unwrap();
    assert_eq!(t.xyz.len(), 12);
    assert_eq!(t.xyz[2], 1.0 - 100.0);
}
```

- [ ] **Step 2: Run to fail** — `cargo test -p cave-geo --test layers` → FAIL.

- [ ] **Step 3: Implement**

```rust
// crates/cave-geo/src/layers.rs
//! What the Map and 3D view draw from a stored surface, in cave metres.
//!
//! Heights: the survey's own elevations are never rewritten. One rigid shift relates them to the
//! ground: offset = (ground at the entrance − entrance depth) − the entrance station's survey z,
//! applied to the GROUND when it is drawn (old CsElevation). No stored ground means no terrain,
//! never terrain at 0 (the old suite's elevation datum trap).

use crate::contour::{levels, lines};
use crate::frame::Placement;
use crate::grid::Grid;
use crate::request::Bounds;
use serde::Serialize;

/// How far an entrance is taken to sit below the ground, metres (5 ft). Display only, never stored.
pub const ENTRANCE_DEPTH_M: f64 = 5.0 * 0.3048;
/// Every this-many-th contour level is a major line.
pub const MAJOR_EVERY: i64 = 5;
const FT_PER_M: f64 = 1.0 / 0.3048;

/// Metres to add to a ground height to put it in the survey's frame, or `None` when the ground at
/// the entrance is unknown. `entrance_z` is the entrance station's solved z (metres).
pub fn datum_offset(ground_m: Option<f64>, entrance_z: f64) -> Option<f64> {
    ground_m.map(|g| g - ENTRANCE_DEPTH_M - entrance_z)
}

/// The six numbers `[a, b, c, d, e, f]` taking photo pixel corners `(px, py)` (py down) to cave
/// metres: `x = a·px + b·py + c`, `y = d·px + e·py + f` (the scans' matrix convention).
pub fn photo_matrix(b: &Bounds, size: (u32, u32), place: &Placement) -> [f64; 6] {
    let (sx, sy) = (b.width() / f64::from(size.0), b.height() / f64::from(size.1));
    let o = place.utm_to_cave(b.xmin, b.ymax);
    let px = place.utm_to_cave(b.xmin + sx, b.ymax);
    let py = place.utm_to_cave(b.xmin, b.ymax - sy);
    [px[0] - o[0], py[0] - o[0], o[0], px[1] - o[1], py[1] - o[1], o[1]]
}

/// One contour line in cave metres.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Contour {
    /// The level, in the display unit.
    pub level: f64,
    /// Every [`MAJOR_EVERY`]th level.
    pub major: bool,
    /// Closed ring.
    pub closed: bool,
    /// `[x, y, z, x, y, z, ...]`; z is the level shifted by the datum offset, or 0 without one.
    pub xyz: Vec<f64>,
}

/// Contours at `interval` (in the display unit: feet when `feet`), mapped into the cave frame.
pub fn contours(g: &Grid, b: &Bounds, place: &Placement, interval: f64, feet: bool, offset: Option<f64>) -> Vec<Contour> {
    let Some((lo, hi)) = g.range() else { return Vec::new() };
    let k = if feet { FT_PER_M } else { 1.0 };
    let mut out = Vec::new();
    for level in levels(lo * k, hi * k, interval) {
        let level_m = level / k;
        let major = ((level / interval).round() as i64) % MAJOR_EVERY == 0;
        for run in lines(g, level_m) {
            let mut xyz = Vec::with_capacity(run.points.len() * 3);
            for p in &run.points {
                let (e, n) = g.cell_utm(b, p[0], p[1]);
                let c = place.utm_to_cave(e, n);
                xyz.extend_from_slice(&[c[0], c[1], offset.map_or(0.0, |o| level_m + o)]);
            }
            out.push(Contour { level, major, closed: run.closed, xyz });
        }
    }
    out
}

/// The ground as a mesh: `w*h` vertices row by row (holes get NaN z and are skipped by the view).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Terrain {
    /// Columns.
    pub w: usize,
    /// Rows.
    pub h: usize,
    /// `[x, y, z, ...]` in cave metres.
    pub xyz: Vec<f64>,
}

/// The terrain in the cave frame, or `None` without a datum offset.
pub fn terrain(g: &Grid, b: &Bounds, place: &Placement, offset: Option<f64>) -> Option<Terrain> {
    let off = offset?;
    let mut xyz = Vec::with_capacity(g.w * g.h * 3);
    for r in 0..g.h {
        for c in 0..g.w {
            let (e, n) = g.cell_utm(b, c as f64, r as f64);
            let p = place.utm_to_cave(e, n);
            xyz.extend_from_slice(&[p[0], p[1], g.at(c, r).map_or(f64::NAN, |v| v + off)]);
        }
    }
    Some(Terrain { w: g.w, h: g.h, xyz })
}
```

- [ ] **Step 4: Pass + commit**

```bash
cargo test -p cave-geo
cargo fmt -p cave-geo && cargo clippy -p cave-geo --all-targets -- -D warnings
cargo build --target wasm32-unknown-unknown -p cave-geo
python3 tools/make_code_index.py
git add crates/cave-geo docs/CODE-INDEX.md
git commit -m "feat(cave-geo): photo matrix, contours and terrain in the cave frame"
```

---

### Task 10: wasm doors (`cave-wasm/src/geo.rs`)

**Goal:** JSON doors the web app calls: status, set location, plan a download, commit a download, layers, ground at the entrance, pick.

**Files:**
- Create: `crates/cave-wasm/src/geo.rs`
- Modify: `crates/cave-wasm/Cargo.toml` (add `cave-geo`)
- Modify: `crates/cave-wasm/src/lib.rs` (`mod geo;` next to `mod snap;`)
- Modify: `apps/web/src/lib/engine.ts` (typed wrappers)

**Acceptance Criteria:**
- [ ] `geo_status()` returns `{"location":…|null,"surface":…|null,"candidate":{kind,station?,lat?,lon?},"interval_m":…|null,"stations":[{id,label}]}`.
- [ ] `geo_set_location(json)` is one undoable edit recorded under `"settings"`; bad JSON → `error: …`.
- [ ] `surface_plan(photo, elevation, adjusted)` returns `{zone,bounds,for_at,photo?:{w,h,url},elevation?:{w,h,url}}` using the scene's x/y bounds and the stored location; no location → `error: no location`.
- [ ] `surface_commit(plan_json, ground_m, today)` writes `surface` and (when `ground_m` is finite) `location.ground_m` in ONE edit; NaN leaves `ground_m` as it was.
- [ ] `surface_layers(tiff, interval, feet, adjusted)` returns `{photo_matrix?, contours:[…], terrain?}`; `surface_ground(tiff)` returns the bilinear ground at the stored location or NaN; `surface_pick(tiff, cx, cy, r, adjusted)` returns `{x,y,lat,lon,ground_m}` or `error: no ground inside the circle`.
- [ ] No `tracing` call in `geo.rs` carries a latitude, longitude, easting or northing (grep check below).
- [ ] `cargo build --target wasm32-unknown-unknown -p cave-wasm` succeeds.

**Verify:** `cargo test -p cave-wasm && cargo build --target wasm32-unknown-unknown -p cave-wasm && ! grep -nE "tracing::.*(lat|lon|\\.e\\b|\\.n\\b|bounds)" crates/cave-wasm/src/geo.rs`

**Steps:**

- [ ] **Step 1: Read the patterns** — `crates/cave-wasm/src/lib.rs`: `DOC` thread-local (line ~23), `record_edit` (line ~1512), `set_drawing_scale` (line ~568) for the edit+record shape, and `scene(adjusted, filter)` (line ~1141) for how a `cave_scene::Scene` is built (copy its internal call, not the JSON round trip).

- [ ] **Step 2: Implement `geo.rs`**

```rust
// crates/cave-wasm/src/geo.rs
//! Doors for Surface Data and the entrance location. Coordinates cross this boundary as JSON for the
//! person's own screen; they are NEVER logged (cave-log redaction is only a safety net).

use crate::{DOC, record_edit};
use cave_doc::Document;
use cave_geo::frame::{LatLon, Placement};
use cave_geo::grid::Grid;
use cave_geo::record::{self, Candidate, Location, SurfaceRecord};
use cave_geo::request::{self, Bounds, ELEVATION, PHOTO};
use serde::{Deserialize, Serialize};
use serde_json::json;
use wasm_bindgen::prelude::*;

fn err(e: impl std::fmt::Display) -> String {
    format!("error: {e}")
}

/// The solved scene of the open cave (same call `scene()` makes).
fn solved(doc: &Document, adjusted: bool) -> cave_scene::Scene {
    // Replace with the exact builder `crate::scene` uses, without the view filter.
    crate::build_scene(doc, adjusted)
}

/// The entrance station's solved position, metres.
fn entrance(scene: &cave_scene::Scene, station: cave_id::Id) -> Option<[f64; 3]> {
    let id = station.to_string();
    scene.stations.iter().find(|s| s.id == id).map(|s| [s.at.x, s.at.y, s.at.z])
}

fn placement(doc: &Document, adjusted: bool) -> Result<(Placement, Location, [f64; 3]), String> {
    let loc = record::location(doc).ok_or("no location")?;
    let scene = solved(doc, adjusted);
    let at = entrance(&scene, loc.station).ok_or("the entrance station has no position")?;
    Ok((Placement::new([at[0], at[1]], loc.at()), loc, at))
}

/// Placement in the zone the surface was cut in (a stored surface keeps its zone).
fn surface_placement(doc: &Document, adjusted: bool) -> Result<(Placement, Location, [f64; 3], SurfaceRecord), String> {
    let (p, loc, at) = placement(doc, adjusted)?;
    let s = record::surface(doc).ok_or("no surface")?;
    Ok((Placement::in_zone(p.anchor(), loc.at(), s.zone), loc, at, s))
}

/// Everything the Surface Data dialog needs (JSON).
#[wasm_bindgen]
pub fn geo_status() -> String {
    DOC.with(|d| {
        let slot = d.borrow();
        let Some(doc) = slot.as_ref() else { return err("no cave is open") };
        let candidate = match record::candidate(doc) {
            Candidate::Stored(_) => json!({"kind": "stored"}),
            Candidate::Imported { station, at } => json!({"kind": "imported", "station": station.to_string(), "lat": at.lat, "lon": at.lon}),
            Candidate::Ask { station } => json!({"kind": "ask", "station": station.map(|s| s.to_string())}),
        };
        let stations: Vec<_> = doc
            .ids_of(cave_doc::Kind::Station)
            .into_iter()
            .filter_map(|id| doc.entity(id).map(|e| json!({"id": id.to_string(), "label": e.text(cave_doc::Field::StationLabel)})))
            .collect();
        json!({
            "location": record::location(doc),
            "surface": record::surface(doc),
            "candidate": candidate,
            "interval_m": record::contour_interval_m(doc),
            "stations": stations,
        })
        .to_string()
    })
}

#[derive(Deserialize)]
struct SetLocation {
    station: String,
    lat: f64,
    lon: f64,
    source: String,
    #[serde(default)]
    ground_m: Option<f64>,
}

/// Stores the cave's location (one undoable edit). JSON `{station, lat, lon, source, ground_m?}`.
#[wasm_bindgen]
pub fn geo_set_location(json_in: &str) -> String {
    let req: SetLocation = match serde_json::from_str(json_in) {
        Ok(r) => r,
        Err(e) => return err(e),
    };
    if !(req.lat.abs() <= 90.0 && req.lon.abs() <= 180.0) {
        return err("that is not a latitude and longitude");
    }
    DOC.with(|d| {
        let mut slot = d.borrow_mut();
        let Some(doc) = slot.as_mut() else { return err("no cave is open") };
        let Ok(station) = req.station.parse::<cave_id::Id>() else { return err("unknown station") };
        let loc = Location { station, lat: req.lat, lon: req.lon, ground_m: req.ground_m.filter(|g| g.is_finite()), source: req.source };
        let actions = record::set_location(doc, &loc);
        if actions.is_empty() {
            return "ok".into();
        }
        match doc.edit(actions) {
            Ok(edit) => {
                record_edit("settings", &edit);
                "ok".into()
            }
            Err(e) => err(format!("{e:?}")),
        }
    })
}

#[derive(Serialize, Deserialize)]
struct Part {
    w: u32,
    h: u32,
    url: String,
}

#[derive(Serialize, Deserialize)]
struct Plan {
    zone: i32,
    bounds: Bounds,
    for_at: LatLon,
    #[serde(skip_serializing_if = "Option::is_none")]
    photo: Option<Part>,
    #[serde(skip_serializing_if = "Option::is_none")]
    elevation: Option<Part>,
}

/// What to download for the open cave (JSON plan), from its plan extent and stored location.
#[wasm_bindgen]
pub fn surface_plan(photo: bool, elevation: bool, adjusted: bool) -> String {
    DOC.with(|d| {
        let slot = d.borrow();
        let Some(doc) = slot.as_ref() else { return err("no cave is open") };
        let (place, loc, at) = match placement(doc, adjusted) {
            Ok(v) => v,
            Err(e) => return err(e),
        };
        let scene = solved(doc, adjusted);
        let cave = scene.bounds.map_or([at[0], at[1], at[0], at[1]], |b| [b[0], b[1], b[3], b[4]]);
        let b = request::window(cave, &place);
        let zone = place.zone();
        let part = |s, url: fn(&Bounds, i32, (u32, u32)) -> String| {
            let (w, h) = request::pixel_size(&b, s);
            Part { w, h, url: url(&b, zone, (w, h)) }
        };
        let plan = Plan {
            zone,
            bounds: b,
            for_at: loc.at(),
            photo: photo.then(|| part(PHOTO, request::naip_url)),
            elevation: elevation.then(|| part(ELEVATION, request::dem_url)),
        };
        serde_json::to_string(&plan).unwrap_or_else(err)
    })
}

/// Ground (metres NAVD88) at the stored location from an elevation TIFF cut to `plan_json`'s bounds, or NaN.
#[wasm_bindgen]
pub fn surface_ground(plan_json: &str, tiff: &[u8]) -> f64 {
    let (Ok(plan), Ok(grid)) = (serde_json::from_str::<Plan>(plan_json), Grid::parse_tiff(tiff)) else { return f64::NAN };
    let place = Placement::in_zone([0.0, 0.0], plan.for_at, plan.zone);
    let (e, n) = place.latlon_to_utm(plan.for_at);
    let (c, r) = grid.utm_cell(&plan.bounds, e, n);
    grid.sample_at(c, r).unwrap_or(f64::NAN)
}

/// Records a finished download (one edit): the surface request and, when finite, the ground at the entrance.
#[wasm_bindgen]
pub fn surface_commit(plan_json: &str, ground_m: f64, today: &str) -> String {
    let plan: Plan = match serde_json::from_str(plan_json) {
        Ok(p) => p,
        Err(e) => return err(e),
    };
    DOC.with(|d| {
        let mut slot = d.borrow_mut();
        let Some(doc) = slot.as_mut() else { return err("no cave is open") };
        let rec = SurfaceRecord {
            zone: plan.zone,
            bounds: plan.bounds,
            photo: plan.photo.as_ref().map(|p| [p.w, p.h]).or_else(|| record::surface(doc).and_then(|s| s.photo)),
            elevation: plan.elevation.as_ref().map(|p| [p.w, p.h]).or_else(|| record::surface(doc).and_then(|s| s.elevation)),
            for_at: plan.for_at,
            fetched: today.to_string(),
        };
        let mut actions = record::set_surface(doc, &rec);
        if ground_m.is_finite()
            && let Some(mut loc) = record::location(doc)
        {
            loc.ground_m = Some(ground_m);
            actions.extend(record::set_location(doc, &loc));
        }
        if actions.is_empty() {
            return "ok".into();
        }
        match doc.edit(actions) {
            Ok(edit) => {
                record_edit("settings", &edit);
                "ok".into()
            }
            Err(e) => err(format!("{e:?}")),
        }
    })
}

/// Sets the contour spacing (metres). One edit.
#[wasm_bindgen]
pub fn set_contour_interval(metres: f64) -> String {
    if !(metres.is_finite() && metres > 0.0) {
        return err("the spacing must be a positive length");
    }
    DOC.with(|d| {
        let mut slot = d.borrow_mut();
        let Some(doc) = slot.as_mut() else { return err("no cave is open") };
        let actions = record::set_contour_interval_m(doc, metres);
        if actions.is_empty() {
            return "ok".into();
        }
        match doc.edit(actions) {
            Ok(edit) => {
                record_edit("settings", &edit);
                "ok".into()
            }
            Err(e) => err(format!("{e:?}")),
        }
    })
}

/// What the views draw (JSON `{photo_matrix?, photo_size?, contours, terrain?}`). `tiff` may be empty (photo only).
#[wasm_bindgen]
pub fn surface_layers(tiff: &[u8], interval: f64, feet: bool, adjusted: bool) -> String {
    DOC.with(|d| {
        let slot = d.borrow();
        let Some(doc) = slot.as_ref() else { return err("no cave is open") };
        let (place, loc, at, s) = match surface_placement(doc, adjusted) {
            Ok(v) => v,
            Err(e) => return err(e),
        };
        let offset = cave_geo::layers::datum_offset(loc.ground_m, at[2]);
        let photo = s.photo.map(|[w, h]| cave_geo::layers::photo_matrix(&s.bounds, (w, h), &place));
        let (contours, terrain) = match Grid::parse_tiff(tiff) {
            Ok(g) => (
                cave_geo::layers::contours(&g, &s.bounds, &place, interval, feet, offset),
                cave_geo::layers::terrain(&g, &s.bounds, &place, offset),
            ),
            Err(_) => (Vec::new(), None),
        };
        json!({"photo_matrix": photo, "photo_size": s.photo, "contours": contours, "terrain": terrain, "stale": s.for_at != loc.at()}).to_string()
    })
}

/// The entrance pick: lowest ground inside the circle (cave metres). JSON `{x, y, lat, lon, ground_m}`.
#[wasm_bindgen]
pub fn surface_pick(tiff: &[u8], cx: f64, cy: f64, radius: f64, adjusted: bool) -> String {
    DOC.with(|d| {
        let slot = d.borrow();
        let Some(doc) = slot.as_ref() else { return err("no cave is open") };
        let (place, _, _, s) = match surface_placement(doc, adjusted) {
            Ok(v) => v,
            Err(e) => return err(e),
        };
        let grid = match Grid::parse_tiff(tiff) {
            Ok(g) => g,
            Err(e) => return err(e),
        };
        match cave_geo::pick::lowest_in_circle(&grid, &s.bounds, &place, [cx, cy], radius) {
            Some(p) => json!({"x": p.point[0], "y": p.point[1], "lat": p.at.lat, "lon": p.at.lon, "ground_m": p.ground_m}).to_string(),
            None => err("no ground inside the circle"),
        }
    })
}
```

Notes for the implementer:
- `crate::build_scene`: if `lib.rs` has no shared builder, extract the body of `pub fn scene(adjusted, filter)` that produces the `cave_scene::Scene` into `pub(crate) fn build_scene(doc: &Document, adjusted: bool) -> cave_scene::Scene` (no filter) and make `scene()` call it. Do not change `scene()`'s output.
- Check `StationDot.at`'s type (`Pt`) field names in `crates/cave-scene/src/lib.rs` and adjust `entrance()`.
- `Id`'s text form: use whatever `StationDot.id` uses (it is "The station's id as text"); parse it back with the same `FromStr` impl (`grep -n "impl.*FromStr\|impl.*Display" crates/cave-id/src/lib.rs`).
- `SurfaceRecord.for_at != loc.at()` needs `LatLon: PartialEq` (it has it).

- [ ] **Step 3: engine.ts wrappers** — in `apps/web/src/lib/engine.ts`, next to the other wasm imports and the `engine` object, add:

```ts
// Surface Data and the entrance location (cave-wasm/src/geo.rs). Coordinates stay on this screen: never log them.
export interface GeoLocation { station: string; lat: number; lon: number; ground_m?: number; source: string }
export interface SurfaceRecord { zone: number; bounds: { xmin: number; ymin: number; xmax: number; ymax: number }; photo?: [number, number]; elevation?: [number, number]; for_at: { lat: number; lon: number }; fetched: string }
export interface GeoStatus {
  location: GeoLocation | null;
  surface: SurfaceRecord | null;
  candidate: { kind: 'stored' } | { kind: 'imported'; station: string; lat: number; lon: number } | { kind: 'ask'; station: string | null };
  interval_m: number | null;
  stations: { id: string; label: string }[];
}
export interface SurfacePlan { zone: number; bounds: SurfaceRecord['bounds']; for_at: { lat: number; lon: number }; photo?: { w: number; h: number; url: string }; elevation?: { w: number; h: number; url: string } }
export interface SurfaceContour { level: number; major: boolean; closed: boolean; xyz: number[] }
export interface SurfaceLayers { photo_matrix: number[] | null; photo_size: [number, number] | null; contours: SurfaceContour[]; terrain: { w: number; h: number; xyz: number[] } | null; stale: boolean }
```

and on the `engine` object (wrapping each door; parse JSON; an `error: ` string becomes `{ error }`):

```ts
  geoStatus: (): GeoStatus | null => { const t = geo_status(); return t.startsWith('error') ? null : JSON.parse(t); },
  geoSetLocation: (l: GeoLocation): string => geo_set_location(JSON.stringify(l)),
  surfacePlan: (photo: boolean, elevation: boolean, adjusted: boolean): SurfacePlan | { error: string } => { const t = surface_plan(photo, elevation, adjusted); return t.startsWith('error') ? { error: t.slice(7) } : JSON.parse(t); },
  surfaceGround: (plan: SurfacePlan, tiff: Uint8Array): number => surface_ground(JSON.stringify(plan), tiff),
  surfaceCommit: (plan: SurfacePlan, groundM: number, today: string): string => surface_commit(JSON.stringify(plan), groundM, today),
  setContourInterval: (m: number): string => set_contour_interval(m),
  surfaceLayers: (tiff: Uint8Array, interval: number, feet: boolean, adjusted: boolean): SurfaceLayers | null => { const t = surface_layers(tiff, interval, feet, adjusted); return t.startsWith('error') ? null : JSON.parse(t); },
  surfacePick: (tiff: Uint8Array, cx: number, cy: number, r: number, adjusted: boolean): { x: number; y: number; lat: number; lon: number; ground_m: number } | { error: string } => { const t = surface_pick(tiff, cx, cy, r, adjusted); return t.startsWith('error') ? { error: t.slice(7) } : JSON.parse(t); },
```

Add the eight names to the import from the wasm package at the top of `engine.ts` (same import list the other doors use).

- [ ] **Step 4: Build, check, commit**

```bash
cargo fmt -p cave-wasm && cargo clippy -p cave-wasm --all-targets -- -D warnings && cargo test -p cave-wasm
cargo build --target wasm32-unknown-unknown -p cave-wasm
! grep -nE "tracing::.*(lat|lon|bounds)" crates/cave-wasm/src/geo.rs
(cd apps/web && npx svelte-check --tsconfig ./tsconfig.json)
python3 tools/make_code_index.py
git add crates/cave-wasm apps/web/src/lib/engine.ts docs/CODE-INDEX.md Cargo.lock
git commit -m "feat(cave-wasm): surface data and entrance location doors"
```

---

### Task 11: Surface files — fetch, OPFS store, `.cavecad` parts

**Goal:** Download the planned files with a timeout and validation, keep them in OPFS keyed by the request, and save/open them as `surface/*` parts of the `.cavecad` file ("Surface in file", on by default).

**Files:**
- Create: `apps/web/src/lib/surface-files.ts`
- Modify: `apps/web/src/lib/cavefile.ts` (surface parts in `pack`/`unpack`, manifest list)
- Modify: `apps/web/src/lib/files.ts:158-161` (`caveBytes` includes surface), `:201-210` (open stores bundled surface)
- Test: `apps/web/tests/cavefile.check.ts` (extend), `apps/web/tests/surfacefiles.check.ts` (new), `apps/web/package.json` (`test:surfacefiles` + add to `test`)

**Acceptance Criteria:**
- [ ] `pack({…, surface: {record, photo, elevation}})` then `unpack` returns the same three byte-for-byte; manifest lists `surface/photo.jpg`, `surface/elevation.tif`, `surface/surface.json` with sizes.
- [ ] `pack` without `surface` writes no `surface/` entries; an old file without them unpacks with `surface: null`.
- [ ] `requestKey(plan)` is stable for equal plans and differs when bounds/zone/size differ.
- [ ] `validateImage`/`validateTiff` reject a JSON error body (`{"error":…}`) served with status 200.
- [ ] `fetchWithTimeout` aborts after the given milliseconds.

**Verify:** `cd apps/web && npm run test:cavefile && npm run test:surfacefiles`

**Steps:**

- [ ] **Step 1: Failing tests**

```ts
// apps/web/tests/surfacefiles.check.ts
import { requestKey, looksLikeJpeg, looksLikeTiff } from '../src/lib/surface-files.ts';
const ok = (c: boolean, m: string) => { if (!c) { console.error('FAIL', m); process.exit(1); } console.log('ok', m); };
const plan = { zone: 16, bounds: { xmin: 1, ymin: 2, xmax: 3, ymax: 4 }, for_at: { lat: 0, lon: 0 }, photo: { w: 10, h: 10, url: 'u' } };
ok(requestKey(plan, 'photo') === requestKey(structuredClone(plan), 'photo'), 'key stable');
ok(requestKey(plan, 'photo') !== requestKey({ ...plan, zone: 17 }, 'photo'), 'key changes with zone');
ok(requestKey(plan, 'photo') !== requestKey(plan, 'elevation'), 'key differs per part');
const json = new TextEncoder().encode('{"error":{"code":400}}');
ok(!looksLikeJpeg(json) && !looksLikeTiff(json), 'JSON error body refused');
ok(looksLikeJpeg(new Uint8Array([0xff, 0xd8, 0xff, 0xe0])), 'JPEG magic');
ok(looksLikeTiff(new Uint8Array([0x49, 0x49, 42, 0])) && looksLikeTiff(new Uint8Array([0x4d, 0x4d, 0, 42])), 'TIFF magic both orders');
```

Extend `apps/web/tests/cavefile.check.ts` with a round trip: pack with `surface: { record: '{"zone":16}', photo: new Uint8Array([1,2,3]), elevation: new Uint8Array([4,5]) }`, unpack, compare bytes and record text; pack without, assert `bundle.surface === null`.

Add to `package.json` scripts: `"test:surfacefiles": "node --experimental-strip-types tests/surfacefiles.check.ts"` and append `&& npm run test:surfacefiles` to `"test"`.

- [ ] **Step 2: Run to fail** — `cd apps/web && npm run test:surfacefiles` → FAIL (module missing).

- [ ] **Step 3: Implement `surface-files.ts`**

```ts
// The surface files (aerial photo, elevation grid): downloaded straight from USGS, kept in the browser's
// private storage (OPFS) under a key made from the REQUEST, and packed into the .cavecad file. Keyed by the
// request so undoing a re-download still finds the earlier files, and two caves cut to the same window share them.
// Never log a URL from here: it carries the window, which locates the cave.

import type { SurfacePlan } from './engine';

export type Part = 'photo' | 'elevation';
export const TIMEOUT_MS = 60_000;

/** A stable key for one part of a plan (FNV-1a over the canonical request). */
export function requestKey(plan: Pick<SurfacePlan, 'zone' | 'bounds'> & Partial<SurfacePlan>, part: Part): string {
  const p = part === 'photo' ? plan.photo : plan.elevation;
  const b = plan.bounds;
  const text = [part, plan.zone, b.xmin.toFixed(3), b.ymin.toFixed(3), b.xmax.toFixed(3), b.ymax.toFixed(3), p?.w ?? 0, p?.h ?? 0].join('|');
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) h = Math.imul(h ^ text.charCodeAt(i), 0x01000193) >>> 0;
  return `${part}-${h.toString(16).padStart(8, '0')}`;
}

export const looksLikeJpeg = (b: Uint8Array) => b.length > 3 && b[0] === 0xff && b[1] === 0xd8 && b[2] === 0xff;
export const looksLikeTiff = (b: Uint8Array) => b.length > 4 && ((b[0] === 0x49 && b[1] === 0x49 && b[2] === 42 && b[3] === 0) || (b[0] === 0x4d && b[1] === 0x4d && b[2] === 0 && b[3] === 42));

/** fetch with a deadline. */
export async function fetchWithTimeout(url: string, ms = TIMEOUT_MS): Promise<Uint8Array> {
  const ctl = new AbortController();
  const t = setTimeout(() => ctl.abort(), ms);
  try {
    const r = await fetch(url, { signal: ctl.signal, cache: 'no-store' });
    if (!r.ok) throw new Error(`the service answered ${r.status}`);
    return new Uint8Array(await r.arrayBuffer());
  } catch (e) {
    if ((e as Error).name === 'AbortError') throw new Error(`the download did not finish within ${ms / 1000} seconds`);
    throw e;
  } finally {
    clearTimeout(t);
  }
}

/** Downloads one part and checks it is what was asked for (USGS sends errors as JSON with status 200). */
export async function download(plan: SurfacePlan, part: Part): Promise<Uint8Array> {
  const p = part === 'photo' ? plan.photo : plan.elevation;
  if (!p) throw new Error(`no ${part} in this plan`);
  const bytes = await fetchWithTimeout(p.url);
  if (part === 'photo' ? !looksLikeJpeg(bytes) : !looksLikeTiff(bytes)) throw new Error(`the service sent something that is not ${part === 'photo' ? 'a photograph' : 'an elevation grid'} (usually an error message). Try again.`);
  return bytes;
}

async function dir(): Promise<FileSystemDirectoryHandle | null> {
  try {
    const root = await navigator.storage.getDirectory();
    return await root.getDirectoryHandle('surface', { create: true });
  } catch {
    return null;
  }
}

/** Keeps a part's bytes under its key (OPFS). Quietly does nothing where OPFS is unavailable. */
export async function store(key: string, bytes: Uint8Array): Promise<void> {
  const d = await dir();
  if (!d) return;
  const f = await d.getFileHandle(key, { create: true });
  const w = await f.createWritable();
  await w.write(bytes);
  await w.close();
}

/** A kept part, or null. */
export async function load(key: string): Promise<Uint8Array | null> {
  const d = await dir();
  if (!d) return null;
  try {
    const f = await (await d.getFileHandle(key)).getFile();
    return new Uint8Array(await f.arrayBuffer());
  } catch {
    return null;
  }
}
```

- [ ] **Step 4: `cavefile.ts` parts** — update the header comment (add `surface/photo.jpg`, `surface/elevation.tif`, `surface/surface.json  the surface the cave was mapped against (optional)`), then:

```ts
export interface SurfaceParts {
  /** The `surface` setting's JSON (what was downloaded). */
  record: string;
  photo: Uint8Array | null;
  elevation: Uint8Array | null;
}
```

- `Manifest`: add `surface?: { path: string; bytes: number }[];`
- `Bundle`: add `surface: SurfaceParts | null;`
- `pack` opts: add `surface?: SurfaceParts | null`; build `const surf: ZipEntry[] = opts.surface ? [{ name: 'surface/surface.json', data: enc.encode(opts.surface.record) }, ...(opts.surface.photo ? [{ name: 'surface/photo.jpg', data: opts.surface.photo }] : []), ...(opts.surface.elevation ? [{ name: 'surface/elevation.tif', data: opts.surface.elevation }] : [])] : [];` include `...surf` in the zip list and `surface: surf.map((e) => ({ path: e.name, bytes: e.data.length }))` in the manifest.
- `unpack`: `const sj = byName.get('surface/surface.json'); const surface = sj ? { record: dec.decode(sj), photo: byName.get('surface/photo.jpg') ?? null, elevation: byName.get('surface/elevation.tif') ?? null } : null;` and return it in the bundle.
- `VERSION` stays 1 (readers ignore unknown parts — the existing rule).

- [ ] **Step 5: `files.ts`** — in `caveBytes(withScans, withSurface = true)`: when `withSurface` and `engine.geoStatus()?.surface` exists, build parts from `surface.svelte.ts`'s current bytes (Task 12 exposes `currentSurfaceParts(): Promise<SurfaceParts | null>`; until Task 12 lands, add the parameter and pass `null`). In `openCave` after `engine.syncLoad` succeeds with a bundle: `if (bundle.surface) await adoptSurfaceParts(bundle.surface);` (also from Task 12). Find every `caveBytes(` caller (`grep -rn "caveBytes(" apps/web/src`) and add the "Surface in file" toggle beside the existing "Scans in file" toggle, on by default, remembered in the same place that toggle is.

- [ ] **Step 6: Pass + commit**

```bash
cd apps/web && npm run test:cavefile && npm run test:surfacefiles && npx svelte-check --tsconfig ./tsconfig.json && cd ../..
git add apps/web
git commit -m "feat(web): surface files fetched from USGS, kept in OPFS and packed into .cavecad"
```

---

### Task 12: Surface state, Surface Data dialog, location picker

**Goal:** The Surface Data button: find the location (stored → imported → picker), download photo/elevation, commit, report; a Leaflet location picker with USGS imagery that also takes typed decimal or DMS and lets the person choose the entrance station.

**Files:**
- Create: `apps/web/src/lib/surface.svelte.ts`
- Create: `apps/web/src/lib/latlon-text.ts`
- Create: `apps/web/src/components/SurfaceData.svelte`
- Create: `apps/web/src/components/LocationPicker.svelte`
- Modify: `apps/web/src/components/Ribbon.svelte` (button)
- Modify: `apps/web/package.json` (`leaflet@1.9.4`, `@types/leaflet`)
- Test: `apps/web/tests/latlon.check.ts`, `package.json` script `test:latlon` (+ add to `test`)

**Acceptance Criteria:**
- [ ] `parseLatLon("36.99, -86.44")`, `"36 59 24 N 86 26 24 W"`, `36°59'24\"N 86°26'24\"W` all parse to (36.99, −86.44) within 1e-6; garbage gives `null`; out-of-range gives `null`.
- [ ] With a stored location, Surface Data downloads without asking; with an imported fixed point, it shows the point and uses it on OK; with neither, the picker opens with A1 preselected as entrance.
- [ ] After a download: `surface` setting written, `ground_m` stored, files in OPFS, report shows width in metres and ground at the entrance in the display unit (no coordinates in the console).
- [ ] Re-running replaces both parts; an unticked part keeps what it had.
- [ ] Failure messages: offline/timeout, non-image reply, "outside 3DEP coverage" (all-holes grid).

**Verify:** `cd apps/web && npm run test:latlon && npx svelte-check --tsconfig ./tsconfig.json`; then browser check in Task 15.

**Steps:**

- [ ] **Step 1: Failing test + parser**

```ts
// apps/web/tests/latlon.check.ts
import { parseLatLon } from '../src/lib/latlon-text.ts';
const ok = (c: boolean, m: string) => { if (!c) { console.error('FAIL', m); process.exit(1); } console.log('ok', m); };
const near = (p: { lat: number; lon: number } | null) => !!p && Math.abs(p.lat - 36.99) < 1e-6 && Math.abs(p.lon + 86.44) < 1e-6;
ok(near(parseLatLon('36.99, -86.44')), 'decimal');
ok(near(parseLatLon('36 59 24 N 86 26 24 W')), 'DMS spaced');
ok(near(parseLatLon(`36°59'24"N 86°26'24"W`)), 'DMS symbols');
ok(parseLatLon('banana') === null, 'garbage');
ok(parseLatLon('91, 0') === null, 'range');
```

```ts
// apps/web/src/lib/latlon-text.ts
/** A typed latitude/longitude: decimal ("36.99, -86.44") or degrees-minutes-seconds with N/S/E/W. */
export function parseLatLon(text: string): { lat: number; lon: number } | null {
  const t = text.trim().toUpperCase();
  const dec = t.match(/^(-?\d+(?:\.\d+)?)\s*[, ]\s*(-?\d+(?:\.\d+)?)$/);
  let lat: number, lon: number;
  if (dec) {
    lat = +dec[1];
    lon = +dec[2];
  } else {
    const parts = [...t.matchAll(/(\d+(?:\.\d+)?)\D+?(\d+(?:\.\d+)?)?\D*?(\d+(?:\.\d+)?)?\D*?([NSEW])/g)];
    if (parts.length !== 2) return null;
    const val = (m: RegExpMatchArray) => (+m[1] + (+(m[2] ?? 0)) / 60 + (+(m[3] ?? 0)) / 3600) * (m[4] === 'S' || m[4] === 'W' ? -1 : 1);
    const a = parts.find((m) => m[4] === 'N' || m[4] === 'S');
    const o = parts.find((m) => m[4] === 'E' || m[4] === 'W');
    if (!a || !o) return null;
    lat = val(a);
    lon = val(o);
  }
  return Number.isFinite(lat) && Number.isFinite(lon) && Math.abs(lat) <= 90 && Math.abs(lon) <= 180 ? { lat, lon } : null;
}
```

Run `npm run test:latlon` → PASS (fix the regex if a case fails; the test is the contract).

- [ ] **Step 2: `surface.svelte.ts`** (state + the whole flow, UI-free)

```ts
// Surface Data: the cave's surface (photo + elevation) as this screen holds it, and the steps that change it.
// Coordinates and URLs are never logged.

import { engine, type SurfacePlan, type SurfaceLayers } from './engine';
import { download, load, requestKey, store, type Part } from './surface-files';
import type { SurfaceParts } from './cavefile';
import { cave, changed } from './cave.svelte';

export const surface = $state({
  /** Object URL of the photo, or ''. */
  photoUrl: '',
  /** The elevation TIFF bytes, or null. */
  tiff: null as Uint8Array | null,
  /** What the views draw (recomputed on cave/interval/unit change). */
  layers: null as SurfaceLayers | null,
  /** Bumps when the files change. */
  version: 0,
  busy: false,
});

let photoBytes: Uint8Array | null = null;

function setPhoto(bytes: Uint8Array | null) {
  if (surface.photoUrl) URL.revokeObjectURL(surface.photoUrl);
  photoBytes = bytes;
  surface.photoUrl = bytes ? URL.createObjectURL(new Blob([bytes], { type: 'image/jpeg' })) : '';
}

const today = () => new Date().toISOString().slice(0, 10);

/** The plan the stored surface was cut to (for keys), from the `surface` setting. */
function storedPlan(): (SurfacePlan & { photo?: { w: number; h: number; url: string }; elevation?: { w: number; h: number; url: string } }) | null {
  const s = engine.geoStatus()?.surface;
  if (!s) return null;
  return { zone: s.zone, bounds: s.bounds, for_at: s.for_at, photo: s.photo ? { w: s.photo[0], h: s.photo[1], url: '' } : undefined, elevation: s.elevation ? { w: s.elevation[0], h: s.elevation[1], url: '' } : undefined };
}

/** Loads the stored surface's files from OPFS (after open, undo, or a cave switch). Returns what is missing. */
export async function reloadSurface(): Promise<Part[]> {
  const plan = storedPlan();
  if (!plan) {
    setPhoto(null);
    surface.tiff = null;
    surface.version++;
    return [];
  }
  const missing: Part[] = [];
  const p = plan.photo ? await load(requestKey(plan, 'photo')) : null;
  if (plan.photo && !p) missing.push('photo');
  const e = plan.elevation ? await load(requestKey(plan, 'elevation')) : null;
  if (plan.elevation && !e) missing.push('elevation');
  setPhoto(p);
  surface.tiff = e;
  surface.version++;
  return missing;
}

/** Downloads what is asked for, keeps it, records it. Returns the report lines. */
export async function fetchSurface(want: { photo: boolean; elevation: boolean }): Promise<{ ok: boolean; lines: string[] }> {
  const plan = engine.surfacePlan(want.photo, want.elevation, cave.adjusted);
  if ('error' in plan) return { ok: false, lines: [plan.error] };
  surface.busy = true;
  const lines: string[] = [];
  let ground = NaN;
  try {
    if (plan.photo) {
      try {
        const b = await download(plan, 'photo');
        await store(requestKey(plan, 'photo'), b);
        setPhoto(b);
        lines.push(`Aerial photo: ${plan.photo.w} x ${plan.photo.h} px, about ${Math.round(plan.bounds.xmax - plan.bounds.xmin)} m across.`);
      } catch (e) {
        lines.push(`Aerial photo: ${(e as Error).message}`);
        delete plan.photo;
      }
    }
    if (plan.elevation) {
      try {
        const b = await download(plan, 'elevation');
        ground = engine.surfaceGround(plan, b);
        await store(requestKey(plan, 'elevation'), b);
        surface.tiff = b;
        lines.push(Number.isFinite(ground) ? `Ground elevation: ground at the entrance ${fmtLen(ground)} (NAVD88).` : 'Ground elevation: the service has no elevation data here (3DEP covers the United States).');
      } catch (e) {
        lines.push(`Ground elevation: ${(e as Error).message}`);
        delete plan.elevation;
      }
    }
    if (plan.photo || plan.elevation) {
      const r = engine.surfaceCommit(plan, ground, today());
      if (r !== 'ok') lines.push(r.replace(/^error: /, ''));
      changed();
    }
    surface.version++;
    return { ok: !!(plan.photo || plan.elevation), lines };
  } finally {
    surface.busy = false;
  }
}

function fmtLen(m: number): string {
  return cave.units === 'ft' ? `${(m / 0.3048).toFixed(1)} ft` : `${m.toFixed(1)} m`;
}

/** The parts for saving into the .cavecad file. */
export async function currentSurfaceParts(): Promise<SurfaceParts | null> {
  const s = engine.geoStatus()?.surface;
  if (!s) return null;
  return { record: JSON.stringify(s), photo: photoBytes, elevation: surface.tiff };
}

/** Takes the surface files out of an opened .cavecad file. */
export async function adoptSurfaceParts(parts: SurfaceParts): Promise<void> {
  const plan = storedPlan();
  if (!plan) return;
  if (parts.photo && plan.photo) await store(requestKey(plan, 'photo'), parts.photo);
  if (parts.elevation && plan.elevation) await store(requestKey(plan, 'elevation'), parts.elevation);
  await reloadSurface();
}
```

Check `cave.units`' actual values in `cave.svelte.ts` (`savedUnits()`), and use them in `fmtLen`. Wire `reloadSurface()` to run when `cave.opened` changes and after undo/redo of a settings edit (find where views re-read on `cave.version`; add an `$effect` in `SurfaceData.svelte`'s parent or in `App.svelte` that calls `reloadSurface()` when `engine.geoStatus()?.surface` changes by JSON value). If `reloadSurface` reports missing parts for a stored surface (a server-synced copy without its files), call `fetchSurface` for those parts and show "The surface for this cave was downloaded again on this device."

Replace the `null` placeholders left in Task 11's `files.ts` with `currentSurfaceParts()` / `adoptSurfaceParts()`.

- [ ] **Step 3: `LocationPicker.svelte`** — a modal:
  - `npm install leaflet@1.9.4 @types/leaflet@1.9.12` in `apps/web`.
  - Map: `L.map(el).setView(start ?? [37.5, -86.0], start ? 16 : 7)`; base layer `L.tileLayer('https://basemap.nationalmap.gov/arcgis/rest/services/USGSImageryOnly/MapServer/tile/{z}/{y}/{x}', { maxZoom: 19, attribution: 'USGS' })`; click drops a marker and fills the text field with `lat.toFixed(6), lon.toFixed(6)`.
  - Text field (decimal or DMS via `parseLatLon`), invalid → inline "Couldn't read that coordinate."
  - Entrance station `<select>` from `geoStatus().stations`, preselected to the candidate's station (A1).
  - Props: `start: {lat, lon} | null`, `station: string | null`; event/callback `onpick({ lat, lon, station })`, `oncancel()`.
  - Import `leaflet/dist/leaflet.css` in the component.

- [ ] **Step 4: `SurfaceData.svelte`** — a modal opened from the Ribbon:
  - Two checkboxes (Aerial photo, Ground elevation), both on.
  - Contour spacing field in the display unit (default 10 ft / 5 m; stored via `engine.setContourInterval(metres)`).
  - On OK: `const st = engine.geoStatus()`; `stored` → go; `imported` → message "Using the fixed point from the imported survey (station X)." then `engine.geoSetLocation({ station, lat, lon, source: 'import' })`; `ask` → open `LocationPicker`, on pick `geoSetLocation({…, source: 'map' or 'typed'})`. Then `fetchSurface(want)` and show the report lines in the dialog (not the console).
  - Shows the current location (station label + coordinate, to the person only), with "Change location…" opening the picker.

- [ ] **Step 5: Ribbon button** — in `Ribbon.svelte`, in the group where the other cave-wide tools live (look for the DrawingScale / Survey options group), add a "Surface Data" button (icon `Mountain` from `@lucide/svelte`) that opens `SurfaceData.svelte`, and a "Pick entrance on surface" button (icon `Crosshair`) that Task 14 wires; disabled until `surface.tiff` is non-null.

- [ ] **Step 6: Commit**

```bash
cd apps/web && npm run test:latlon && npx svelte-check --tsconfig ./tsconfig.json && cd ../..
git add apps/web
git commit -m "feat(web): Surface Data dialog, location picker and surface state"
```

---

### Task 13: Drawing the surface (photo, contours, terrain, toggles)

**Goal:** The Map/3D view shows the photo at half strength under everything, contours over it (majors darker and labelled in the display unit), and the terrain as a translucent surface in 3D; three view toggles.

**Files:**
- Modify: `apps/web/src/lib/viewport/cave-view.ts` (new `setContours`, `setTerrain`; photo goes through `setScans`)
- Modify: `apps/web/src/components/Viewport3D.svelte:~92-120` (feed layers), toolbar toggles
- Modify: `apps/web/src/lib/surface.svelte.ts` (`recomputeLayers()`)

**Acceptance Criteria:**
- [ ] Photo appears under the survey at opacity 0.5, through the existing scan drawing (key `surface-photo`), draped on the terrain heights when a terrain exists, else at the survey's height like a scan.
- [ ] Contours: minor lines thin, majors thicker/darker; each major run labelled at its middle point in the display unit.
- [ ] Terrain: a mesh from `layers.terrain` (cells with a NaN corner skipped), 35% opacity, depth-write off so the cave shows through; shown whenever the Terrain toggle is on.
- [ ] Toggles "Photo", "Contours", "Terrain" (per person, via `option`/`setOption`, defaults on/on/off) hide/show each.
- [ ] Changing units or contour spacing redraws contours without any download.

**Verify:** Task 15 browser check; `cd apps/web && npx svelte-check --tsconfig ./tsconfig.json`

**Steps:**

- [ ] **Step 1: `recomputeLayers` in `surface.svelte.ts`**

```ts
/** Rebuilds what the views draw from the kept files (no network). */
export function recomputeLayers() {
  const st = engine.geoStatus();
  if (!st?.surface || !st.location) {
    surface.layers = null;
    return;
  }
  const feet = cave.units === 'ft';
  const intervalM = st.interval_m ?? (feet ? 10 * 0.3048 : 5);
  const interval = feet ? intervalM / 0.3048 : intervalM;
  surface.layers = engine.surfaceLayers(surface.tiff ?? new Uint8Array(), Math.round(interval * 1000) / 1000, feet, cave.adjusted);
}
```

Call it from an `$effect` that reads `surface.version`, `cave.version`, `cave.units`, `cave.adjusted` (wrap the call in `untrack` per the repo's "$effect read+write hangs" rule).

- [ ] **Step 2: Photo through `setScans`** — in `Viewport3D.svelte` where the scans list is built (around line 117), append when `surface.photoUrl && surface.layers?.photo_matrix && option('surfacePhoto') !== false`:

```ts
list.push({ key: 'surface-photo', url: surface.photoUrl, matrix: surface.layers.photo_matrix, z: surveyZ, opacity: 0.5, trim: null, warp: null, plane: null, height: terrainHeight });
```

`terrainHeight`: when `surface.layers.terrain` exists, a function `(x, y) => z` sampling the terrain grid by bilinear interpolation in the cave frame (build once per layers change: the terrain is a regular grid in UTM rotated by convergence — invert with the four corner points: compute `u,v` cell coordinates by solving the 2×2 affine from corners (0,0),(w−1,0),(0,h−1)); return `null` when no terrain. Make sure the surface photo is drawn first (lowest render order) — check `setScans`' render order and give `surface-photo` the lowest.

- [ ] **Step 3: `setContours` in `cave-view.ts`** — a separate `THREE.Group` (`this.surfaceGroup`, added to the scene beside `this.content`, not cleared by `rebuild()`):

```ts
  private surfaceGroup = new THREE.Group();
  private surfaceMaterials: LineMaterial[] = [];

  /** Contour lines (cave metres, z already in the survey frame or 0) and their major labels. */
  setContours(list: { xyz: number[]; major: boolean; closed: boolean; level: number }[], unit: string, visible: boolean) {
    for (const c of [...this.surfaceGroup.children]) if (c.userData.kind === 'contour') { this.surfaceGroup.remove(c); (c as THREE.Mesh).geometry?.dispose(); }
    for (const m of this.surfaceMaterials) m.dispose();
    this.surfaceMaterials = [];
    if (!visible) return this.requestRender();
    for (const major of [false, true]) {
      const segs: number[] = [];
      for (const c of list.filter((l) => l.major === major)) {
        const p = c.xyz;
        const n = p.length / 3;
        for (let i = 0; i + 1 < n + (c.closed ? 1 : 0); i++) {
          const a = i * 3, b = ((i + 1) % n) * 3;
          segs.push(p[a], p[a + 1], p[a + 2], p[b], p[b + 1], p[b + 2]);
        }
      }
      if (!segs.length) continue;
      const geo = new LineSegmentsGeometry();
      geo.setPositions(segs);
      const mat = new LineMaterial({ color: major ? 0x8a5a2b : 0xb08860, linewidth: major ? 1.6 : 0.8, transparent: true, opacity: major ? 0.9 : 0.6 });
      mat.resolution.set(this.width, this.height);
      const mesh = new LineSegments2(geo, mat);
      mesh.userData.kind = 'contour';
      mesh.renderOrder = -1;
      this.surfaceGroup.add(mesh);
      this.surfaceMaterials.push(mat);
    }
    // labels: one per major run at its middle point (use the same text-sprite helper the station labels use;
    // find it with `grep -n "Sprite\|label" src/lib/viewport/cave-view.ts`), text `${level} ${unit}`, userData.kind = 'contour'.
    this.requestRender();
  }
```

Also keep `this.surfaceMaterials` resolutions updated wherever the class updates `this.materials` resolutions on resize.

- [ ] **Step 4: `setTerrain`**

```ts
  /** The ground as a translucent mesh (w*h grid of cave-metre points; NaN z = hole). */
  setTerrain(t: { w: number; h: number; xyz: number[] } | null, visible: boolean) {
    for (const c of [...this.surfaceGroup.children]) if (c.userData.kind === 'terrain') { this.surfaceGroup.remove(c); (c as THREE.Mesh).geometry.dispose(); ((c as THREE.Mesh).material as THREE.Material).dispose(); }
    if (!t || !visible) return this.requestRender();
    const idx: number[] = [];
    const ok = (i: number) => Number.isFinite(t.xyz[i * 3 + 2]);
    for (let r = 0; r + 1 < t.h; r++)
      for (let c = 0; c + 1 < t.w; c++) {
        const a = r * t.w + c, b = a + 1, d = a + t.w, e = d + 1;
        if (ok(a) && ok(b) && ok(d) && ok(e)) idx.push(a, d, b, b, d, e);
      }
    const pos = new Float32Array(t.xyz.map((v) => (Number.isFinite(v) ? v : 0)));
    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.BufferAttribute(pos, 3));
    geo.setIndex(idx);
    geo.computeVertexNormals();
    const mat = new THREE.MeshLambertMaterial({ color: 0x9c8f73, transparent: true, opacity: 0.35, depthWrite: false, side: THREE.DoubleSide });
    const mesh = new THREE.Mesh(geo, mat);
    mesh.userData.kind = 'terrain';
    mesh.renderOrder = -2;
    this.surfaceGroup.add(mesh);
    this.requestRender();
  }
```

Note: a 512×512 grid is 262k vertices; acceptable. If frame rate drops on Pitfall, decimate by 2 when `w*h > 200_000`.

- [ ] **Step 5: Feed from `Viewport3D.svelte`** — an `$effect` reading `surface.layers`, `option('surfaceContours')`, `option('surfaceTerrain')`, `cave.units` → `view.setContours(layers.contours, unitLabel, on)`, `view.setTerrain(layers.terrain, on)`. Add the three toggles to the view's existing toolbar (beside the Passage / elevation-colour buttons), using `setOption('surfacePhoto' | 'surfaceContours' | 'surfaceTerrain', bool)`; defaults photo on, contours on, terrain off. Show the toggles only when `surface.layers` is non-null.

- [ ] **Step 6: Commit**

```bash
cd apps/web && npx svelte-check --tsconfig ./tsconfig.json && npm test && cd ../..
git add apps/web
git commit -m "feat(web): draw the surface photo, contours and terrain under the cave"
```

---

### Task 14: Pick entrance on surface (drag a circle, move, auto-refetch)

**Goal:** "Pick entrance on surface" lets the person drag a circle on the Map; on release the entrance moves to the lowest ground inside, one undoable edit, and the surface downloads again automatically.

**Files:**
- Modify: `apps/web/src/components/Viewport3D.svelte` (pointer mode `pick-entrance`, circle overlay)
- Modify: `apps/web/src/lib/surface.svelte.ts` (`pickEntrance(cx, cy, r)`)
- Modify: `apps/web/src/components/Ribbon.svelte` (button wiring from Task 12)

**Acceptance Criteria:**
- [ ] Press = centre, drag = radius, live circle drawn in screen space; Escape cancels; a radius under 1 m (on the ground) is ignored.
- [ ] On release: `engine.surfacePick` → `engine.geoSetLocation({station: same, lat, lon, ground_m, source: 'surface-pick'})` (one undo step), then `fetchSurface({photo: had photo, elevation: true})` automatically, no question.
- [ ] The cave visibly moves; the photo/contours/terrain stay fixed to the world until the refetch replaces them.
- [ ] Refetch failure: old files stay, message "The entrance moved, but the surface could not be downloaded again (…). It still shows the old area; Surface Data will offer it again." and `layers.stale` is true.
- [ ] A circle with no ground inside: nothing changes, message "No ground data inside that circle."
- [ ] Undo restores the old location and the old files (kept in OPFS by key) and the view redraws.

**Verify:** Task 15 browser check

**Steps:**

- [ ] **Step 1: `pickEntrance`**

```ts
/** Moves the entrance to the lowest ground inside the circle, then downloads the surface again. */
export async function pickEntrance(cx: number, cy: number, r: number): Promise<string> {
  if (!surface.tiff) return 'Download the ground elevation first (Surface Data).';
  const st = engine.geoStatus();
  if (!st?.location) return 'Set the cave location first (Surface Data).';
  const hit = engine.surfacePick(surface.tiff, cx, cy, r, cave.adjusted);
  if ('error' in hit) return 'No ground data inside that circle.';
  const res = engine.geoSetLocation({ station: st.location.station, lat: hit.lat, lon: hit.lon, ground_m: hit.ground_m, source: 'surface-pick' });
  if (res !== 'ok') return res.replace(/^error: /, '');
  changed();
  const had = st.surface;
  const again = await fetchSurface({ photo: !!had?.photo, elevation: true });
  const moved = `The entrance moved to the lowest ground in the circle (${fmtLen(hit.ground_m)}).`;
  return again.ok ? `${moved} The surface was downloaded again for the new spot.` : `${moved} The surface could not be downloaded again (${again.lines.join(' ')}). It still shows the old area; Surface Data will offer it again.`;
}
```

- [ ] **Step 2: Pointer mode in `Viewport3D.svelte`** — a local `let picking = $state(false)` set by the Ribbon button (export a small store `entrancePick` from `surface.svelte.ts`: `export const entrancePick = $state({ on: false, message: '' })`). While `on`:
  - `pointerdown` (primary button): record the ground point under the cursor in cave metres (use the same screen→model unprojection the view uses for drawing strokes on the map plane; find it with `grep -n "toModel\|unproject\|modelAt" src/components/Viewport3D.svelte src/lib/viewport/cave-view.ts`), and the screen point.
  - `pointermove`: radius = model distance from centre to current model point; draw an SVG/canvas circle overlay (absolute-positioned over the canvas) with screen radius = screen distance.
  - `pointerup`: if radius ≥ 1 m → `entrancePick.message = await pickEntrance(c.x, c.y, radius)`; `entrancePick.on = false`.
  - `Escape` → `entrancePick.on = false`.
  - While `on`, normal selection/drawing handlers return early, and the cursor is `crosshair`.
  - Show `entrancePick.message` in the view's existing status/toast area.

- [ ] **Step 3: Undo** — after undo/redo, call `reloadSurface()` then `recomputeLayers()` (hook where the panel undo triggers `changed()`; `grep -n "undo" src/components/PanelUndo.svelte src/lib/cave.svelte.ts`).

- [ ] **Step 4: Commit**

```bash
cd apps/web && npx svelte-check --tsconfig ./tsconfig.json && cd ../..
git add apps/web
git commit -m "feat(web): pick the entrance on the surface by dragging a circle; surface refetches"
```

---

### Task 15: Privacy checks, docs, end-to-end browser verification

**Goal:** Prove no coordinate leaks into logs or exports, record the feature in DESIGN.md, and verify the whole flow in the browser on Pitfall Cave and Plumbline Pit.

**Files:**
- Test: `crates/cave-dxf/tests/` (new test: location setting not in DXF output) — use the crate's existing export test as template
- Modify: `docs/DESIGN.md` (new dated section)
- Modify: `docs/superpowers/specs/2026-10-26-surface-data-design.md` (As built)

**Acceptance Criteria:**
- [ ] A document with a `location` setting (lat 36.123457, lon −86.654321) exported to DXF and to Compass/Walls/Survex/Therion contains neither `36.123457` nor `86.654321` nor the UTM easting.
- [ ] `grep -rnE "console\.(log|info|warn|debug)\(.*(lat|lon|url|bounds)" apps/web/src/lib/surface*.ts apps/web/src/components/SurfaceData.svelte apps/web/src/components/LocationPicker.svelte` finds nothing.
- [ ] Browser (dev server `cavecad-web`, :5173): Pitfall Cave given a test location (a town block, NOT a real cave), Surface Data fetches photo + elevation, contours and photo show under the cave, terrain toggles, a dragged circle moves the entrance and the surface refetches, undo restores; Save → reopen keeps the surface (no network on reopen: check network requests list has no USGS call); Plumbline Pit does the same with no error.
- [ ] Full checks green: `cargo fmt --check -p cave-geo -p cave-formats -p cave-wasm`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, the wasm build line, `python3 tools/make_code_index.py --check`, `npx svelte-check`, `npm test`.

**Verify:** the commands in the last criterion, plus screenshots from the browser check.

**Steps:**

- [ ] **Step 1: Export privacy test** — in `crates/cave-dxf/tests/`, copy the setup of an existing export test, add `cave_geo::record::set_location` to its document (add `cave-geo` as a dev-dependency of `cave-dxf`), export, assert the three strings are absent. Same for `cave_formats` writers in `crates/cave-formats/tests/edges.rs` (they read the document; the location setting must not become a `*fix` line).

- [ ] **Step 2: Browser verification** — `preview_start {name: "cavecad-web"}`; open Pitfall Cave from the demos; Surface Data → picker → type `36.99, -86.44`; OK; wait for the report; screenshot. Toggle Terrain; tilt the view; screenshot. Pick entrance: drag a circle near the entrance; confirm the message and that `read_network_requests` shows the two USGS requests again; Undo; screenshot. Save the cave, reload the page, open the saved file; confirm no USGS request and the surface shows. Repeat open + Surface Data on Plumbline Pit. Check `read_console_messages` for errors and for any coordinate text.

- [ ] **Step 3: Docs** — `docs/DESIGN.md`: a "2026-10-27 — Surface Data and Entrance Location" section: world frame (WGS84/NAVD88, UTM requests), what is stored (four settings), files in `.cavecad` (`surface/*`, "Surface in file"), drag-circle pick + auto refetch, datum offset rule, NAD27 handling, what is not built (state sources, many-caves view, Survex/Therion frames, server file sync). Spec: add an "As built" section listing any divergence found while building.

- [ ] **Step 4: Final checks + commit**

```bash
cargo fmt --check -p cave-geo -p cave-formats -p cave-wasm
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --target wasm32-unknown-unknown -p cave-id -p cave-store -p cave-log -p cave-geom -p cave-doc -p cave-survey -p cave-notebook -p cave-scans -p cave-pattern -p cave-snap -p cave-geo
python3 tools/make_code_index.py --check
(cd apps/web && npx svelte-check --tsconfig ./tsconfig.json && npm test)
git add -A crates docs apps
git commit -m "test+docs: surface data privacy checks, design notes, as-built"
```

---

## Self-review notes

- Spec coverage: world frame (T1), datums/import frame (T2, T8), window/URLs (T3), grid (T4), contours (T5), pick (T6), stored records + candidate + convergence (T7), layers + datum offset (T9), doors (T10), files in `.cavecad` + OPFS + server-synced refetch (T11, T12 Step 2), button/picker (T12), drawing + toggles (T13), drag circle + auto refetch + undo (T14), privacy + browser + docs (T15).
- Spec divergence to record in As built: the window uses the solved scene's x/y bounds (stations, legs, splays, walls), not traced map linework; the location picker is a modal, not a docked panel; Survex/Therion imports do not record a fixed frame in this build (Compass and Walls do).
