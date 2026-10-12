//! What the survey says about one station, for the station tab: where it is in the survey (kind of
//! place, legs, trips, elevation), the passage dimensions (LRUD) and the notes written there, each
//! with the thing that holds it so an edit goes to the right place, and whether it is a benchmark.
//! Elevation only, never a plan position.

use crate::station_rows;
use cave_doc::{Document, Field};
use cave_draw::{Walls, note_sources, walls_at, walls_source};
use cave_id::Id;
use serde::Serialize;

/// One note and where it is kept.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NoteField {
    /// The shot or trip the note is written on (an id as text); what `set_note` edits.
    pub source: String,
    /// What it is written on: `shot A4 -> A5`, or `trip opening note`.
    pub on: String,
    /// The words.
    pub text: String,
}

/// A station's walls as the survey holds them (metres).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WallsOut {
    /// Left.
    pub left: Option<f64>,
    /// Right.
    pub right: Option<f64>,
    /// Up.
    pub up: Option<f64>,
    /// Down.
    pub down: Option<f64>,
    /// Open left.
    pub left_open: bool,
    /// Open right.
    pub right_open: bool,
    /// Open up.
    pub up_open: bool,
    /// Open down.
    pub down_open: bool,
    /// Where they are kept: `shot A4 -> A5` or `trip opening row`.
    pub on: String,
}

/// Everything the station tab shows.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StationDetail {
    /// The station's id as text.
    pub id: String,
    /// Its label.
    pub label: String,
    /// Kinds of place it is (`lead`, `junction`, ...).
    pub kinds: Vec<&'static str>,
    /// Legs that touch it.
    pub degree: usize,
    /// Trips that surveyed it, by position (0 first).
    pub trips: Vec<usize>,
    /// Elevation in metres.
    pub z: Option<f64>,
    /// Why it is flagged, in words.
    pub flags: Vec<String>,
    /// An agreed benchmark.
    pub benchmark: bool,
    /// The walls, or `None` when no shot touches the station.
    pub walls: Option<WallsOut>,
    /// The notes written there.
    pub notes: Vec<NoteField>,
}

fn label_of(doc: &Document, id: Option<Id>) -> String {
    id.and_then(|i| doc.entity(i))
        .map(|e| e.text(Field::StationLabel))
        .unwrap_or_default()
}

fn describe_shot(doc: &Document, shot: Id) -> String {
    match doc.entity(shot) {
        Some(e) => format!(
            "shot {} -> {}",
            label_of(doc, e.reference(Field::ShotFrom)),
            label_of(doc, e.reference(Field::ShotTo))
        ),
        None => String::new(),
    }
}

/// The detail of one station, or `None` when there is no such station.
pub fn station_detail(doc: &Document, use_adjustment: bool, station: Id) -> Option<StationDetail> {
    let e = doc.entity(station)?;
    let sid = station.to_string();
    let row = station_rows(doc, use_adjustment, "")
        .into_iter()
        .find(|r| r.id == sid);
    let walls = walls_source(doc, station).map(|src| {
        let w: Walls = walls_at(doc, src);
        WallsOut {
            left: w.left,
            right: w.right,
            up: w.up,
            down: w.down,
            left_open: w.left_open,
            right_open: w.right_open,
            up_open: w.up_open,
            down_open: w.down_open,
            on: if src.trip {
                "trip opening row".to_string()
            } else {
                describe_shot(doc, src.entity)
            },
        }
    });
    let notes = note_sources(doc, station)
        .into_iter()
        .map(|(source, text)| NoteField {
            on: match doc.entity(source).map(|s| s.kind()) {
                Some(cave_doc::Kind::Trip) => "trip opening note".to_string(),
                _ => describe_shot(doc, source),
            },
            source: source.to_string(),
            text,
        })
        .collect();
    Some(StationDetail {
        id: sid,
        label: e.text(Field::StationLabel),
        kinds: row.as_ref().map(|r| r.kinds.clone()).unwrap_or_default(),
        degree: row.as_ref().map_or(0, |r| r.degree),
        trips: row.as_ref().map(|r| r.trips.clone()).unwrap_or_default(),
        z: row.as_ref().and_then(|r| r.z),
        flags: row.as_ref().map(|r| r.flags.clone()).unwrap_or_default(),
        benchmark: e.flag(Field::StationBenchmark),
        walls,
        notes,
    })
}
