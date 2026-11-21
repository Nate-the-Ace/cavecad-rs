//! The stations CaveCAD shares with Capture on the same phone, offline: one keychain item per cave
//! (generic password, service `cavecad.stations`, account = the server cave id, access group
//! `<team>.app.cavecad.shared`) holding this JSON. Capture merges it into the stations it kept, so
//! a station added in CaveCAD while offline can be stood over (and waiting shots find it).
//!
//! The format is shared by both apps: `crates/cave-capture/tests/fixtures/shared-stations.json` is
//! the reference text, and Capture's Swift test embeds the same text.

use crate::stations::StationSeed;
use serde::{Deserialize, Serialize};

/// Version of the shared-stations JSON.
pub const SHARED_VERSION: u32 = 1;

/// The keychain item's service name.
pub const SHARED_SERVICE: &str = "cavecad.stations";

/// One cave's stations as CaveCAD shares them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedStations {
    /// Format version ([`SHARED_VERSION`]).
    pub v: u32,
    /// The server cave id (the keychain account).
    pub cave_id: String,
    /// The cave's name, for showing.
    pub name: String,
    /// When the cave last changed in CaveCAD, milliseconds since 1970.
    pub updated_at: i64,
    /// Every placed station, in survey order.
    pub stations: Vec<SharedStation>,
}

/// One shared station: adjusted survey position, rounded to the millimetre.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SharedStation {
    /// Its id (`cave_id::Id` as text).
    pub id: String,
    /// Its label.
    pub label: String,
    /// East, metres.
    pub x: f64,
    /// North, metres.
    pub y: f64,
    /// Up, metres.
    pub z: f64,
}

fn mm(v: f64) -> f64 {
    let r = (v * 1000.0).round() / 1000.0;
    if r == 0.0 { 0.0 } else { r }
}

/// The shared form of `seeds` (from [`crate::stations::stations`]), coordinates rounded to 1 mm.
pub fn shared_stations(
    cave_id: &str,
    name: &str,
    updated_at: i64,
    seeds: &[StationSeed],
) -> SharedStations {
    SharedStations {
        v: SHARED_VERSION,
        cave_id: cave_id.to_string(),
        name: name.to_string(),
        updated_at,
        stations: seeds
            .iter()
            .map(|s| SharedStation {
                id: s.id.to_string(),
                label: s.label.clone(),
                x: mm(s.pos.x),
                y: mm(s.pos.y),
                z: mm(s.pos.z),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cave_id::Id;
    use cave_survey::Vec3;

    fn seed(n: u128, label: &str, x: f64, y: f64, z: f64) -> StationSeed {
        StationSeed {
            id: Id::from_u128(n),
            label: label.into(),
            pos: Vec3::new(x, y, z),
        }
    }

    /// The fixture's input: the reference text must be exactly what this encodes to.
    fn fixture() -> SharedStations {
        shared_stations(
            "01JCAVE0000000000000000000",
            "Sullivan Cave",
            1_763_700_000_000,
            &[
                seed(1, "A1", 0.0, 0.0, 0.0),
                seed(0x1234_5678_9abc, "A2", 10.123_456, -3.000_4, 1.999_95),
                seed(
                    u128::MAX >> 3,
                    "Z 7",
                    1_234_567.891_2,
                    4_321_098.765_4,
                    -12.3456,
                ),
            ],
        )
    }

    #[test]
    fn shared_keys_are_exactly_the_format() {
        let v: serde_json::Value = serde_json::to_value(fixture()).unwrap();
        let mut keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["caveId", "name", "stations", "updatedAt", "v"]);
        let mut sk: Vec<_> = v["stations"][0]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        sk.sort();
        assert_eq!(sk, ["id", "label", "x", "y", "z"]);
    }

    #[test]
    fn shared_coordinates_round_to_the_millimetre() {
        let s = fixture();
        assert_eq!(s.stations[1].x, 10.123);
        assert_eq!(s.stations[1].y, -3.0);
        assert_eq!(s.stations[1].z, 2.0);
        assert_eq!(s.stations[2].x, 1_234_567.891);
        assert_eq!(s.stations[2].z, -12.346);
    }

    #[test]
    fn shared_fixture_is_byte_for_byte() {
        let want = include_str!("../tests/fixtures/shared-stations.json");
        let got = serde_json::to_string(&fixture()).unwrap();
        assert_eq!(got, want.trim_end(), "fixture drifted: {got}");
        let back: SharedStations = serde_json::from_str(want).unwrap();
        assert_eq!(back, fixture());
    }

    #[test]
    fn shared_size_budget_5000_stations() {
        let seeds: Vec<_> = (0..5000u128)
            .map(|i| {
                seed(
                    i * 7_919 + (1u128 << 100),
                    &format!("AB{i:04}"),
                    1_234_567.0 + i as f64 * 0.731,
                    4_321_098.0 - i as f64 * 0.417,
                    -150.0 + i as f64 * 0.013,
                )
            })
            .collect();
        let s = shared_stations("01JCAVE0000000000000000000", "Big", 1, &seeds);
        assert_eq!(s.stations[0].id.len(), 26);
        let n = serde_json::to_string(&s).unwrap().len();
        println!("shared stations: 5000 stations = {n} bytes");
        assert!(n < 512 * 1024, "{n} bytes");
    }
}
