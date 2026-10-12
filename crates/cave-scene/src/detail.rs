//! What the survey says about one station, for the station tab: where it is in the survey (kind of
//! place, legs, trips, elevation), the passage dimensions (LRUD) and the notes written there, each
//! with the thing that holds it so an edit goes to the right place, and whether it is a benchmark.
//! Elevation only, never a plan position.

use crate::station_rows;
use cave_doc::{Document, Field};
use cave_draw::{ShotEdit, Walls, note_sources, shot_now, walls_at, walls_source};
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

/// Everything the shot tab shows: a leg between stations, or a splay.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ShotDetail {
    /// The shot's id as text.
    pub id: String,
    /// True for a splay (a wall hit aimed from a station).
    pub splay: bool,
    /// The station it starts at.
    pub from: String,
    /// The station it ends at (empty for a splay).
    pub to: String,
    /// Trip it was surveyed on, by position (0 first).
    pub trip: usize,
    /// The trip's name.
    pub trip_name: String,
    /// What was measured.
    pub distance: Option<f64>,
    /// Bearing.
    pub azimuth: Option<f64>,
    /// Inclination.
    pub inclination: Option<f64>,
    /// Back bearing.
    pub back_azimuth: Option<f64>,
    /// Back inclination.
    pub back_inclination: Option<f64>,
    /// Left out of the plotted map.
    pub exclude_from_plot: bool,
    /// Left out of everything.
    pub exclude_from_all: bool,
    /// Held as measured in the adjustment.
    pub no_adjust: bool,
    /// The note written on it.
    pub note: String,
    /// Why the survey cannot use it, in words (empty when it is used).
    pub problem: String,
}

/// The detail of one shot, or `None` when there is no such shot.
pub fn shot_detail(doc: &Document, shot: Id) -> Option<ShotDetail> {
    let e = doc
        .entity(shot)
        .filter(|e| e.kind() == cave_doc::Kind::Shot)?;
    let now: ShotEdit = shot_now(doc, shot)?;
    let trip_ref = e.reference(Field::ShotTrip);
    let mut trips: Vec<(f64, Id)> = doc
        .ids_of(cave_doc::Kind::Trip)
        .into_iter()
        .map(|t| {
            (
                doc.entity(t)
                    .and_then(|x| x.number(Field::TripOrder))
                    .unwrap_or(f64::MAX),
                t,
            )
        })
        .collect();
    trips.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let trip = trip_ref
        .and_then(|r| trips.iter().position(|t| t.1 == r))
        .unwrap_or(0);
    let problem = if e.flag(Field::ShotExcludeFromAll) {
        "Left out of everything".to_string()
    } else if !e.flag(Field::ShotSplay)
        && (e.reference(Field::ShotFrom).is_none() || e.reference(Field::ShotTo).is_none())
    {
        "A station is missing".to_string()
    } else if e.number(Field::ShotDistance).is_none() {
        "No distance".to_string()
    } else if !e.flag(Field::ShotAzimuthOmitted)
        && e.number(Field::ShotAzimuth).is_none()
        && e.number(Field::ShotBackAzimuth).is_none()
    {
        "No bearing".to_string()
    } else {
        String::new()
    };
    Some(ShotDetail {
        id: shot.to_string(),
        splay: e.flag(Field::ShotSplay),
        from: label_of(doc, e.reference(Field::ShotFrom)),
        to: label_of(doc, e.reference(Field::ShotTo)),
        trip,
        trip_name: trip_ref
            .and_then(|t| doc.entity(t))
            .map(|t| t.text(Field::TripName))
            .unwrap_or_default(),
        distance: now.distance,
        azimuth: now.azimuth,
        inclination: now.inclination,
        back_azimuth: now.back_azimuth,
        back_inclination: now.back_inclination,
        exclude_from_plot: now.exclude_from_plot,
        exclude_from_all: now.exclude_from_all,
        no_adjust: now.no_adjust,
        note: e.text(Field::ShotNote),
        problem,
    })
}
