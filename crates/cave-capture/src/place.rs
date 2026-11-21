//! Placing a shot over its station: which station a shot stands over (by id, or by typed label for
//! a shot waiting for its station), the turn and shift that put its capture frame into the survey,
//! and its mesh points relative to the station. Shared by Capture (through its FFI) and CaveCAD.

use crate::bundle::ShotHeader;
use crate::frame::{Placement, from_compass, heading_from_tap};
use crate::stations::labels_match;
use cave_survey::Vec3;

/// A station a shot can be matched against.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StationRef<'a> {
    /// Its id (`cave_id::Id` as text).
    pub id: &'a str,
    /// Its label.
    pub label: &'a str,
    /// Survey position, metres (x east, y north, z up).
    pub pos: Vec3,
}

/// The shot's station and its tap target, as indices into `all`, resolved as Capture resolves them:
/// a shot with a `station_id` matches by id only (never by label); a shot waiting for its station
/// matches by [`labels_match`] on its typed label. The target is the first tap's station, by id.
/// `None` when the shot's station is not in `all`.
pub fn station_for(h: &ShotHeader, all: &[StationRef]) -> Option<(usize, Option<usize>)> {
    let over = if h.is_waiting() {
        all.iter()
            .position(|s| labels_match(&h.station_label, s.label))?
    } else {
        all.iter().position(|s| s.id == h.station_id)?
    };
    let target = h
        .taps
        .first()
        .and_then(|t| all.iter().position(|s| s.id == t.target_station_id));
    Some((over, target))
}

/// Places a shot over `station`. A compass shot places from its station alone (`target` is
/// ignored); an older tap shot needs its tap target's position. `None` when it cannot be placed.
pub fn placement(h: &ShotHeader, station: Vec3, target: Option<Vec3>) -> Option<Placement> {
    if h.compass.is_some() {
        return from_compass(station, h.start_camera, h.instrument_height_m);
    }
    let tap = h.taps.first()?;
    heading_from_tap(
        station,
        target?,
        tap.dir,
        h.start_camera,
        h.instrument_height_m,
    )
}

/// Capture-frame mesh positions (xyz per vertex) placed by `p`, minus `station`, x y z per point.
/// Subtracted in f64 before narrowing: survey coordinates can be millions of metres, where f32
/// keeps only about half a metre.
pub fn relative_points(positions: &[f32], p: &Placement, station: Vec3) -> Vec<f32> {
    let mut out = Vec::with_capacity(positions.len());
    for c in positions.as_chunks::<3>().0 {
        let q = p.apply([c[0] as f64, c[1] as f64, c[2] as f64]);
        out.extend_from_slice(&[
            (q.x - station.x) as f32,
            (q.y - station.y) as f32,
            (q.z - station.z) as f32,
        ]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(json: &str) -> ShotHeader {
        serde_json::from_str(json).unwrap()
    }

    const TAP: &str = r#"{"shot_id":"s","cave_id":"c","station_id":"a","station_label":"A","captured_at_ms":0,"device":"t",
        "instrument_height_m":0.0,"start_camera":[0,0,0],
        "taps":[{"target_station_id":"b","target_label":"B","origin":[0,0,0],"dir":[0,0,-1]}],
        "tracking":"normal","arkit_version":"26.1","gravity":[0,-1,0]}"#;

    const COMPASS: &str = r#"{"shot_id":"s","cave_id":"c","station_id":"a","station_label":"A","captured_at_ms":0,"device":"t",
        "instrument_height_m":0.3,"start_camera":[0,0,0],"taps":[],
        "compass":{"true_north":true,"accuracy_deg":6.0},
        "tracking":"normal","arkit_version":"26.1","gravity":[0,-1,0]}"#;

    const POS: [f32; 9] = [0.0, 0.0, -5.0, 1.0, 0.0, -5.0, 0.0, 1.0, -5.0];

    fn v(x: f64, y: f64, z: f64) -> Vec3 {
        Vec3 { x, y, z }
    }

    #[test]
    fn compass_shot_ignores_the_target() {
        let h = header(COMPASS);
        let st = v(100.0, 200.0, 50.0);
        let a = placement(&h, st, None).unwrap();
        let b = placement(&h, st, Some(v(0.0, 0.0, 0.0))).unwrap();
        assert_eq!(a, b);
        let pts = relative_points(&POS, &a, st);
        // (0,0,-5) is 5 m north of the start camera, 0.3 m above the station
        assert!(pts[0].abs() < 1e-5 && (pts[1] - 5.0).abs() < 1e-5 && (pts[2] - 0.3).abs() < 1e-5);
    }

    #[test]
    fn tap_shot_needs_its_target() {
        let h = header(TAP);
        let st = v(500_000.0, 4_000_000.0, 300.0);
        assert!(placement(&h, st, None).is_none());
        let p = placement(&h, st, Some(v(500_010.0, 4_000_000.0, 300.0))).unwrap();
        let pts = relative_points(&POS, &p, st);
        assert_eq!(pts.len(), 9);
        // as the FFI's placed_positions_are_relative_to_the_station: 5 m east
        assert!((pts[0] - 5.0).abs() < 1e-4, "{pts:?}");
        assert!(pts[1].abs() < 1e-4 && pts[2].abs() < 1e-4, "{pts:?}");
    }

    fn refs() -> Vec<StationRef<'static>> {
        vec![
            StationRef {
                id: "x",
                label: "A",
                pos: v(0.0, 0.0, 0.0),
            },
            StationRef {
                id: "a",
                label: "Z",
                pos: v(1.0, 0.0, 0.0),
            },
            StationRef {
                id: "b",
                label: "A7",
                pos: v(2.0, 0.0, 0.0),
            },
        ]
    }

    #[test]
    fn station_with_an_id_matches_by_id_only() {
        let h = header(COMPASS);
        // "x" carries the shot's label "A", but the id "a" wins
        assert_eq!(station_for(&h, &refs()), Some((1, None)));
        let mut lost = h.clone();
        lost.station_id = "nope".into();
        assert_eq!(station_for(&lost, &refs()), None);
    }

    #[test]
    fn waiting_shot_matches_by_label() {
        let mut h = header(COMPASS);
        h.station_id.clear();
        h.station_label = " a 7".into();
        assert_eq!(station_for(&h, &refs()), Some((2, None)));
        h.station_label = "A9".into();
        assert_eq!(station_for(&h, &refs()), None);
    }

    #[test]
    fn tap_target_is_found_by_id() {
        let h = header(TAP);
        assert_eq!(station_for(&h, &refs()), Some((1, Some(2))));
    }
}
