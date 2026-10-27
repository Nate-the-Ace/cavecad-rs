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
        let u: String = name
            .to_ascii_uppercase()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();
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
                from_ecef(
                    [x + NAD27_SHIFT[0], y + NAD27_SHIFT[1], z + NAD27_SHIFT[2]],
                    WGS84,
                )
            }
        }
    }
}

fn to_ecef(p: LatLon, (a, f): (f64, f64)) -> [f64; 3] {
    let e2 = f * (2.0 - f);
    let (lat, lon) = (p.lat.to_radians(), p.lon.to_radians());
    let n = a / (1.0 - e2 * lat.sin().powi(2)).sqrt();
    [
        n * lat.cos() * lon.cos(),
        n * lat.cos() * lon.sin(),
        n * (1.0 - e2) * lat.sin(),
    ]
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
    LatLon {
        lat: lat.to_degrees(),
        lon: y.atan2(x).to_degrees(),
    }
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
    // utm_to_latlon uses the WGS84 ellipsoid for every datum. For NAD27 the UTM grid is on Clarke 1866;
    // the ellipsoid difference in the inverse is under 1 m in a zone, below the 3-parameter transform's
    // own error.
    /// A fixed point's easting/northing (metres) as WGS84 latitude/longitude.
    pub fn to_latlon(&self, e: f64, n: f64) -> LatLon {
        let (lat, lon) = cave_geomag::utm_to_latlon(e, n, self.zone);
        self.datum.to_wgs84(LatLon { lat, lon })
    }
}
