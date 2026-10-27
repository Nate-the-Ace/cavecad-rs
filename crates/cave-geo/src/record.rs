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
        LatLon {
            lat: self.lat,
            lon: self.lon,
        }
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
        out.push(Action::Create {
            id,
            kind: Kind::Setting,
        });
        out.push(Action::Set {
            id,
            field: Field::SettingKey,
            value: Value::Text(key.into()),
        });
    }
    if doc
        .entity(id)
        .is_none_or(|e| e.text(Field::SettingValue) != text)
    {
        out.push(Action::Set {
            id,
            field: Field::SettingValue,
            value: Value::Text(text),
        });
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
            if let (Some(x), Some(y)) = (
                e.number(Field::StationFixedX),
                e.number(Field::StationFixedY),
            ) {
                return Candidate::Imported {
                    station: *id,
                    at: frame.to_latlon(x, y),
                };
            }
        }
    }
    Candidate::Ask {
        station: stations.iter().find(|(l, _)| l == "A1").map(|(_, id)| *id),
    }
}
