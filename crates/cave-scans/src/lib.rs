//! Which scanned pages go with which trip.
//!
//! The pictures never enter the cave: they stay in a folder on the person's machine and are read
//! there. What the cave keeps is only the link, a trip's list of paths relative to that folder, so
//! sharing a cave never shares a scan, and a path can never point outside the scans folder.

use cave_doc::{Action, Document, Field, Kind, Value};
use cave_id::Id;
use serde::Serialize;

/// Longest path kept.
pub const MAX_PATH: usize = 300;

/// Why a link edit was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScanError {
    /// There is no such trip.
    NoSuchTrip,
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
    Ok(write(trip, &kept))
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
}
