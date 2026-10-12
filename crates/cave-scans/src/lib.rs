//! Which scanned pages go with which trip.
//!
//! The pictures never enter the cave: they stay in a folder on the person's machine and are read
//! there. What the cave keeps is only the link, a trip's list of paths relative to that folder, so
//! sharing a cave never shares a scan, and a path can never point outside the scans folder.

mod align;
mod trim;

pub use align::{
    Alignment, Anchor, Fit, FitMode, Pair, Placed, Positions, Target, add_anchor, alignment, fit,
    placements, remove_anchor, set_alignment, set_mode,
};
pub use trim::{Outline, set_trim, simplify, trim};

use cave_doc::{Action, Document, Field, Kind, Value};
use cave_id::Id;
use serde::Serialize;

/// The view scans are placed under the map in.
pub const MAP: &str = "map";

/// A view as it is kept: `map`, `profile:<run key>` or `section:<station id>`. A scan can be placed
/// differently in each, since each drawing has its own frame.
pub fn clean_view(view: &str) -> Option<String> {
    let v = view.trim();
    let ok = v == MAP
        || v.strip_prefix("profile:").is_some_and(|k| !k.is_empty())
        || v.strip_prefix("section:").is_some_and(|k| !k.is_empty());
    (ok && v.len() <= 120 && !v.contains('|') && !v.chars().any(|c| c.is_control()))
        .then(|| v.to_string())
}

/// The key a scan's record in a view is kept under: `<view>|<path>`.
pub(crate) fn key(view: &str, path: &str) -> Option<String> {
    Some(format!("{}|{}", clean_view(view)?, clean_path(path)?))
}

pub(crate) fn key_checked(view: &str, path: &str) -> Result<String, ScanError> {
    clean_view(view).ok_or(ScanError::BadView)?;
    key(view, path).ok_or(ScanError::BadPath)
}

/// A kept key back into its view and path.
pub(crate) fn split_key(key: &str) -> Option<(&str, &str)> {
    key.split_once('|')
}

/// Longest path kept.
pub const MAX_PATH: usize = 300;

/// Why a link edit was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScanError {
    /// There is no such trip.
    NoSuchTrip,
    /// The view is not `map`, `profile:<run>` or `section:<station>`.
    BadView,
    /// A match names a station that is not in the cave.
    NoSuchStation,
    /// The outline is not a closed shape with some area (three or more points, finite, on the scan).
    BadOutline,
    /// The path is empty, absolute, climbs out of the folder, or is otherwise not a plain relative path.
    BadPath,
}

/// One scan linked to one trip.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Link {
    /// The trip.
    pub trip: Id,
    /// The scan, relative to the scans folder, with `/` between folders.
    pub path: String,
}

/// A path as it is kept: `/` between folders, no leading slash, no `.` or `..` parts. `None` when it
/// is empty, absolute (a slash or a drive letter), climbs out of the folder, or has control characters.
pub fn clean_path(raw: &str) -> Option<String> {
    let p = raw.trim().replace('\\', "/");
    if p.is_empty() || p.starts_with('/') || p.len() > MAX_PATH || p.chars().any(|c| c.is_control())
    {
        return None;
    }
    if p.as_bytes().get(1) == Some(&b':') {
        return None; // a drive letter
    }
    let parts: Vec<&str> = p.split('/').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() || parts.iter().any(|s| *s == "." || *s == "..") {
        return None;
    }
    Some(parts.join("/"))
}

/// The scans linked to a trip, in the order they were linked.
pub fn scans_of(doc: &Document, trip: Id) -> Vec<String> {
    doc.entity(trip)
        .filter(|e| e.kind() == Kind::Trip)
        .map(|e| {
            e.text(Field::TripScans)
                .lines()
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// Every link in the cave, trips in order.
pub fn links(doc: &Document) -> Vec<Link> {
    doc.ids_of(Kind::Trip)
        .into_iter()
        .flat_map(|trip| {
            scans_of(doc, trip)
                .into_iter()
                .map(move |path| Link { trip, path })
        })
        .collect()
}

fn write(trip: Id, paths: &[String]) -> Vec<Action> {
    vec![Action::Set {
        id: trip,
        field: Field::TripScans,
        value: Value::Text(paths.join("\n")),
    }]
}

/// Links a scan to a trip (nothing changes when it already is).
pub fn link(doc: &Document, trip: Id, path: &str) -> Result<Vec<Action>, ScanError> {
    let path = clean_path(path).ok_or(ScanError::BadPath)?;
    if doc.entity(trip).is_none_or(|e| e.kind() != Kind::Trip) {
        return Err(ScanError::NoSuchTrip);
    }
    let mut now = scans_of(doc, trip);
    if now.contains(&path) {
        return Ok(Vec::new());
    }
    now.push(path);
    Ok(write(trip, &now))
}

/// Takes a scan off a trip (nothing changes when it was not there).
pub fn unlink(doc: &Document, trip: Id, path: &str) -> Result<Vec<Action>, ScanError> {
    let path = clean_path(path).ok_or(ScanError::BadPath)?;
    if doc.entity(trip).is_none_or(|e| e.kind() != Kind::Trip) {
        return Err(ScanError::NoSuchTrip);
    }
    let now = scans_of(doc, trip);
    if !now.contains(&path) {
        return Ok(Vec::new());
    }
    let kept: Vec<String> = now.into_iter().filter(|p| *p != path).collect();
    let mut actions = write(trip, &kept);
    // A scan taken off the trip takes its placements and trims, in every view, with it.
    if let Some(e) = doc.entity(trip) {
        for field in [Field::TripScanAlign, Field::TripScanTrim] {
            let mut map = e.map(field);
            let before = map.len();
            map.retain(|k, _| split_key(k).is_none_or(|(_, p)| p != path));
            if map.len() != before {
                actions.push(Action::Set {
                    id: trip,
                    field,
                    value: Value::Map(map),
                });
            }
        }
    }
    Ok(actions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cave_doc::{ActorId, History};

    fn cave() -> (Document, Vec<Id>) {
        let path = format!(
            "{}/../cave-notebook/tests/data/PitfallCave.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let doc = cave_fixture::from_shot_list(&std::fs::read_to_string(path).unwrap(), ActorId(1))
            .unwrap();
        let trips = doc.ids_of(Kind::Trip);
        (doc, trips)
    }

    #[test]
    fn paths_are_plain_and_relative() {
        assert_eq!(
            clean_path(" 2025 Scans\\9-7 Survey\\p1.png "),
            Some("2025 Scans/9-7 Survey/p1.png".into())
        );
        assert_eq!(clean_path("a//b.png"), Some("a/b.png".into()));
        for bad in [
            "",
            "  ",
            "/etc/passwd",
            "C:/x.png",
            "../x.png",
            "a/../../x.png",
            "./x.png",
            "a\u{0}b",
        ] {
            assert_eq!(clean_path(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn link_unlink_round_trip_and_undo() {
        let (mut doc, trips) = cave();
        let mut h = History::new();
        let edit = doc.edit(link(&doc, trips[0], "p1.png").unwrap()).unwrap();
        h.record(&edit);
        assert!(
            link(&doc, trips[0], "p1.png").unwrap().is_empty(),
            "already linked"
        );
        let edit = doc
            .edit(link(&doc, trips[0], "sub/p2.png").unwrap())
            .unwrap();
        h.record(&edit);
        assert_eq!(scans_of(&doc, trips[0]), ["p1.png", "sub/p2.png"]);
        assert_eq!(links(&doc).len(), 2);
        let edit = doc.edit(unlink(&doc, trips[0], "p1.png").unwrap()).unwrap();
        h.record(&edit);
        assert_eq!(scans_of(&doc, trips[0]), ["sub/p2.png"]);
        h.undo(&mut doc).unwrap();
        assert_eq!(scans_of(&doc, trips[0]), ["p1.png", "sub/p2.png"]);
        assert!(
            scans_of(&doc, trips[1]).is_empty(),
            "other trips are untouched"
        );
    }

    #[test]
    fn a_bad_trip_or_path_is_refused() {
        let (doc, trips) = cave();
        let station = doc.ids_of(Kind::Station)[0];
        assert_eq!(link(&doc, station, "a.png"), Err(ScanError::NoSuchTrip));
        assert_eq!(link(&doc, trips[0], "../a.png"), Err(ScanError::BadPath));
    }

    #[test]
    fn links_survive_a_saved_file() {
        let (mut doc, trips) = cave();
        doc.edit(link(&doc, trips[1], "2025/trip 2/p2.png").unwrap())
            .unwrap();
        let text = cave_dxf::export(&doc, &cave_dxf::ExportOptions::default());
        let back = cave_dxf::import(&text, ActorId(2))
            .unwrap()
            .document
            .unwrap();
        assert_eq!(links(&back), links(&doc));
    }

    use crate::align::Target::{Point, Station};

    #[test]
    fn matches_are_stored_placed_undone_and_dropped_with_the_link() {
        use std::collections::HashMap;
        let (mut doc, trips) = cave();
        let st = doc.ids_of(Kind::Station);
        let mut h = History::new();
        macro_rules! run {
            ($doc:ident, $actions:expr) => {{
                let a = $actions;
                let e = $doc.edit(a).unwrap();
                h.record(&e);
            }};
        }
        run!(doc, link(&doc, trips[0], "p1.png").unwrap());
        run!(
            doc,
            add_anchor(
                &doc,
                trips[0],
                "map",
                "p1.png",
                0.0,
                0.0,
                Station { id: st[0] }
            )
            .unwrap()
        );
        run!(
            doc,
            add_anchor(
                &doc,
                trips[0],
                "map",
                "p1.png",
                100.0,
                0.0,
                Station { id: st[1] }
            )
            .unwrap()
        );
        let al = alignment(&doc, trips[0], "map", "p1.png").unwrap();
        assert_eq!((al.mode, al.anchors.len()), (FitMode::Warp, 2));
        let pos: HashMap<_, _> = [(st[0], (10.0, 20.0)), (st[1], (11.0, 20.0))].into();
        let placed = placements(&doc, "map", &pos);
        assert_eq!(placed.len(), 1);
        assert!((placed[0].matrix[0] - 0.01).abs() < 1e-9 && placed[0].used == 2);
        // The survey moves (adjusted): the same matches place the scan somewhere else.
        let moved: HashMap<_, _> = [(st[0], (0.0, 0.0)), (st[1], (2.0, 0.0))].into();
        assert!((placements(&doc, "map", &moved)[0].matrix[0] - 0.02).abs() < 1e-9);
        // One spot is not a placement; a station that is gone from the view is skipped.
        assert!(placements(&doc, "map", &[(st[0], (0.0, 0.0))].into()).is_empty());
        assert!(
            placements(&doc, "profile:A", &pos).is_empty(),
            "another view has no matches"
        );
        run!(
            doc,
            set_mode(&doc, trips[0], "map", "p1.png", FitMode::Rigid).unwrap()
        );
        assert_eq!(
            alignment(&doc, trips[0], "map", "p1.png").unwrap().mode,
            FitMode::Rigid
        );
        run!(
            doc,
            remove_anchor(&doc, trips[0], "map", "p1.png", 0).unwrap()
        );
        assert_eq!(
            alignment(&doc, trips[0], "map", "p1.png")
                .unwrap()
                .anchors
                .len(),
            1
        );
        run!(doc, unlink(&doc, trips[0], "p1.png").unwrap());
        assert!(
            alignment(&doc, trips[0], "map", "p1.png").is_none(),
            "unlinking drops the placement"
        );
        assert!(placements(&doc, "map", &pos).is_empty());
        assert_eq!(
            add_anchor(
                &doc,
                trips[0],
                "map",
                "p1.png",
                1.0,
                1.0,
                Station { id: trips[1] }
            ),
            Err(ScanError::NoSuchStation)
        );
    }

    #[test]
    fn one_scan_is_placed_separately_in_each_view() {
        use std::collections::HashMap;
        let (mut doc, trips) = cave();
        let st = doc.ids_of(Kind::Station);
        doc.edit(link(&doc, trips[0], "p.png").unwrap()).unwrap();
        for (view, scale) in [("map", 1.0), ("profile:A", 2.0), ("section:X", 3.0)] {
            doc.edit(
                add_anchor(
                    &doc,
                    trips[0],
                    view,
                    "p.png",
                    0.0,
                    0.0,
                    Point { x: 0.0, y: 0.0 },
                )
                .unwrap(),
            )
            .unwrap();
            doc.edit(
                add_anchor(
                    &doc,
                    trips[0],
                    view,
                    "p.png",
                    100.0,
                    0.0,
                    Point {
                        x: 100.0 * scale,
                        y: 0.0,
                    },
                )
                .unwrap(),
            )
            .unwrap();
        }
        doc.edit(
            set_trim(
                &doc,
                trips[0],
                "profile:A",
                "p.png",
                Some(vec![[0.0, 0.0], [90.0, 0.0], [90.0, 90.0]]),
            )
            .unwrap(),
        )
        .unwrap();
        let none: HashMap<_, _> = [(st[0], (0.0, 0.0))].into();
        for (view, scale) in [("map", 1.0), ("profile:A", 2.0), ("section:X", 3.0)] {
            let p = placements(&doc, view, &none);
            assert_eq!(p.len(), 1, "{view}");
            assert!(
                (p[0].matrix[0] - scale).abs() < 1e-9,
                "{view}: {:?}",
                p[0].matrix
            );
            assert_eq!(
                p[0].trim.is_some(),
                view == "profile:A",
                "a trim belongs to its own view"
            );
        }
        // A point in the view needs no station, and unlinking clears every view's record.
        doc.edit(unlink(&doc, trips[0], "p.png").unwrap()).unwrap();
        for view in ["map", "profile:A", "section:X"] {
            assert!(alignment(&doc, trips[0], view, "p.png").is_none());
        }
        assert!(trim(&doc, trips[0], "profile:A", "p.png").is_none());
    }

    #[test]
    fn views_are_checked() {
        assert_eq!(clean_view("map").as_deref(), Some("map"));
        assert!(clean_view("profile:A13a").is_some() && clean_view("section:ABC").is_some());
        for bad in [
            "",
            "plan",
            "profile:",
            "section:",
            "map|x",
            "profile:a\u{1}",
        ] {
            assert!(clean_view(bad).is_none(), "{bad:?}");
        }
        let (doc, trips) = cave();
        assert_eq!(
            add_anchor(
                &doc,
                trips[0],
                "plan",
                "p.png",
                1.0,
                1.0,
                Point { x: 0.0, y: 0.0 }
            ),
            Err(ScanError::BadView)
        );
    }

    #[test]
    fn matches_survive_a_saved_file() {
        let (mut doc, trips) = cave();
        let st = doc.ids_of(Kind::Station);
        doc.edit(link(&doc, trips[0], "p1.png").unwrap()).unwrap();
        doc.edit(
            add_anchor(
                &doc,
                trips[0],
                "map",
                "p1.png",
                12.5,
                30.25,
                Station { id: st[3] },
            )
            .unwrap(),
        )
        .unwrap();
        doc.edit(
            add_anchor(
                &doc,
                trips[0],
                "section:Q",
                "p1.png",
                1.5,
                3.25,
                Point { x: -0.5, y: 2.25 },
            )
            .unwrap(),
        )
        .unwrap();
        let text = cave_dxf::export(&doc, &cave_dxf::ExportOptions::default());
        let back = cave_dxf::import(&text, ActorId(2))
            .unwrap()
            .document
            .unwrap();
        for view in ["map", "section:Q"] {
            assert_eq!(
                alignment(&back, trips[0], view, "p1.png"),
                alignment(&doc, trips[0], view, "p1.png")
            );
        }
    }
}
