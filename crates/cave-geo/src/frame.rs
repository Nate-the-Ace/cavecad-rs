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
    if zone < 0 {
        32700 + zone.unsigned_abs()
    } else {
        32600 + zone as u32
    }
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
        Placement {
            anchor,
            at,
            zone,
            anchor_utm,
            gamma,
        }
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
        [
            self.anchor[0] + de * c + dn * s,
            self.anchor[1] - de * s + dn * c,
        ]
    }

    /// Cave metres to UTM easting/northing (this zone).
    pub fn cave_to_utm(&self, x: f64, y: f64) -> (f64, f64) {
        let (dx, dy) = (x - self.anchor[0], y - self.anchor[1]);
        let (s, c) = self.gamma.sin_cos();
        (
            self.anchor_utm.0 + dx * c - dy * s,
            self.anchor_utm.1 + dx * s + dy * c,
        )
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
