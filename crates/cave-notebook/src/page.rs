//! A notebook page derived from the document: one trip, laid out as the notes are written.

use crate::BACKSIGHT_TOLERANCE_DEG;
use cave_doc::{Document, Entity, Field, Kind};
use cave_draw::{Walls, WallsSource, factor_of, walls_at, walls_source};
use cave_id::Id;
use cave_survey::{ClosureKind, extract, resolve};
use serde::Serialize;
use std::collections::HashSet;

/// Passage dimensions beside a station, metres. `None` is not measured; an open flag is "looked, no wall".
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Lrud {
    /// Distance to the left wall.
    pub left: Option<f64>,
    /// Distance to the right wall.
    pub right: Option<f64>,
    /// Distance to the ceiling.
    pub up: Option<f64>,
    /// Distance to the floor.
    pub down: Option<f64>,
    /// Open to the left.
    pub left_open: bool,
    /// Open to the right.
    pub right_open: bool,
    /// Open above.
    pub up_open: bool,
    /// Open below.
    pub down_open: bool,
}

impl From<Walls> for Lrud {
    fn from(w: Walls) -> Self {
        Lrud {
            left: w.left,
            right: w.right,
            up: w.up,
            down: w.down,
            left_open: w.left_open,
            right_open: w.right_open,
            up_open: w.up_open,
            down_open: w.down_open,
        }
    }
}

/// What one line of the page is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RowKind {
    /// A station with its passage dimensions.
    Station,
    /// The measurements of a shot between two stations.
    Shot,
    /// The measurements of a splay (no station at the far end).
    Splay,
    /// A blank line: the chain breaks here.
    Gap,
}

/// One line of the page. Lengths are metres and angles are degrees; the page's compass cells are magnetic.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Row {
    /// What kind of line it is.
    pub kind: RowKind,
    /// The station's name, or a splay's (`A3.2`); empty on a gap or a shot between stations.
    pub label: String,
    /// The station this line is about (station lines only).
    pub station: Option<Id>,
    /// The shot this line measures (shot and splay lines), or that arrives at this station.
    pub shot: Option<Id>,
    /// A station written to start, or restart, a chain: nothing arrives at it on this page.
    pub anchor: bool,
    /// Passage dimensions beside a station line.
    pub lrud: Option<Lrud>,
    /// True when editing the dimensions here writes to this line's own record (not another trip's).
    pub lrud_editable: bool,
    /// The note written on this line.
    pub note: String,
    /// What a note edit writes to (a shot, or a trip's opening note); `None` when there is nowhere.
    pub note_source: Option<Id>,
    /// Tape reading.
    pub distance: Option<f64>,
    /// Foresight compass, magnetic.
    pub az_fs: Option<f64>,
    /// Backsight compass, magnetic.
    pub az_bs: Option<f64>,
    /// Foresight clino.
    pub inc_fs: Option<f64>,
    /// Backsight clino (reads sign-flipped).
    pub inc_bs: Option<f64>,
    /// Running length of the page's surveyed shots, through this one.
    pub total: Option<f64>,
    /// The shot is left out of everything (a void shot).
    pub excluded: bool,
    /// Why this line looks wrong, or empty.
    pub warn: String,
}

impl Row {
    fn blank(kind: RowKind) -> Self {
        Row {
            kind,
            label: String::new(),
            station: None,
            shot: None,
            anchor: false,
            lrud: None,
            lrud_editable: false,
            note: String::new(),
            note_source: None,
            distance: None,
            az_fs: None,
            az_bs: None,
            inc_fs: None,
            inc_bs: None,
            total: None,
            excluded: false,
            warn: String::new(),
        }
    }
}

/// How a loop or tie closed, for the page's footer.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ClosureRow {
    /// `loop` or `tie`.
    pub kind: &'static str,
    /// The closing shot.
    pub shot: Id,
    /// The page line of that shot.
    pub row: usize,
    /// The stations around the ring, by name.
    pub path: Vec<String>,
    /// Plan error, metres.
    pub horizontal: f64,
    /// Vertical error, metres.
    pub vertical: f64,
    /// Length of the ring, metres.
    pub length: f64,
    /// Error as a percent of the ring's length (`None` for a tie).
    pub percent: Option<f64>,
}

/// The page's running totals.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Totals {
    /// Surveyed length (splays and left-out shots do not count), metres.
    pub length: f64,
    /// Shots between stations.
    pub shots: usize,
    /// Splays.
    pub splays: usize,
    /// Stations named on the page.
    pub stations: usize,
    /// Lines flagged with a warning.
    pub warnings: usize,
}

/// A trip's page.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Page {
    /// The trip.
    pub trip: Id,
    /// Its place among the trips (0 first).
    pub index: usize,
    /// Trip name.
    pub name: String,
    /// Date as `YYYY-MM-DD`, or empty.
    pub date: String,
    /// Who was on the trip.
    pub team: String,
    /// What they measured with.
    pub instruments: String,
    /// `ft` or `m`: the unit the notes were written in.
    pub unit: String,
    /// Magnetic declination the compass cells are corrected by, degrees east.
    pub declination: Option<f64>,
    /// The lines, top to bottom.
    pub rows: Vec<Row>,
    /// Running totals.
    pub totals: Totals,
    /// Loops and ties closed by this trip's shots.
    pub closures: Vec<ClosureRow>,
}

/// A trip in the page chooser.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TripChoice {
    /// The trip.
    pub id: Id,
    /// Its place among the trips (0 first).
    pub index: usize,
    /// Trip name.
    pub name: String,
    /// Trip date, or empty.
    pub date: String,
    /// Shots on its page (splays included).
    pub shots: usize,
}

fn ordered(doc: &Document, kind: Kind, field: Field) -> Vec<Id> {
    let mut v: Vec<(f64, Id)> = doc
        .ids_of(kind)
        .into_iter()
        .map(|id| {
            (
                doc.entity(id)
                    .and_then(|e| e.number(field))
                    .unwrap_or(f64::MAX),
                id,
            )
        })
        .collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    v.into_iter().map(|x| x.1).collect()
}

/// Every trip in order, with its shot count, for choosing a page.
pub fn trips(doc: &Document) -> Vec<TripChoice> {
    let shots = ordered(doc, Kind::Shot, Field::ShotOrder);
    ordered(doc, Kind::Trip, Field::TripOrder)
        .into_iter()
        .enumerate()
        .map(|(index, id)| {
            let e = doc.entity(id);
            TripChoice {
                id,
                index,
                name: e.map(|e| e.text(Field::TripName)).unwrap_or_default(),
                date: e.map(|e| e.text(Field::TripDate)).unwrap_or_default(),
                shots: shots
                    .iter()
                    .filter(|s| {
                        doc.entity(**s)
                            .is_some_and(|e| e.reference(Field::ShotTrip) == Some(id))
                    })
                    .count(),
            }
        })
        .collect()
}

pub(crate) fn label(doc: &Document, station: Option<Id>) -> String {
    station
        .and_then(|s| doc.entity(s))
        .map(|e| e.text(Field::StationLabel))
        .unwrap_or_default()
}

/// The declination a shot's bearing was computed with: its own, else its trip's, else none.
pub(crate) fn applied_declination(shot: &Entity, trip: Option<&Entity>) -> f64 {
    shot.number(Field::ShotDeclination)
        .or_else(|| trip.and_then(|t| t.number(Field::TripDeclination)))
        .unwrap_or(0.0)
}

fn magnetic(true_deg: Option<f64>, decl: f64) -> Option<f64> {
    true_deg.map(|a| (a - decl).rem_euclid(360.0))
}

fn angle_gap(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

fn shot_lrud(doc: &Document, shot: Id, factor: f64) -> Lrud {
    walls_at(
        doc,
        WallsSource {
            entity: shot,
            trip: false,
            factor,
        },
    )
    .into()
}

/// The shots of one trip in survey order.
pub(crate) fn trip_shots(doc: &Document, trip: Id) -> Vec<Id> {
    ordered(doc, Kind::Shot, Field::ShotOrder)
        .into_iter()
        .filter(|s| {
            doc.entity(*s)
                .is_some_and(|e| e.reference(Field::ShotTrip) == Some(trip))
        })
        .collect()
}

/// The page for `trip`, or `None` when there is no such trip.
pub fn page(doc: &Document, trip: Id) -> Option<Page> {
    let t = doc.entity(trip).filter(|e| e.kind() == Kind::Trip)?;
    let index = ordered(doc, Kind::Trip, Field::TripOrder)
        .iter()
        .position(|x| *x == trip)
        .unwrap_or(0);
    let factor = factor_of(doc, Some(trip));
    let mut rows: Vec<Row> = Vec::new();
    let mut totals = Totals::default();
    let mut names: HashSet<String> = HashSet::new();
    let mut at: Option<Id> = None;
    let mut first_anchor = true;
    let mut splays_at: std::collections::HashMap<Id, usize> = Default::default();

    for shot in trip_shots(doc, trip) {
        let Some(e) = doc.entity(shot) else { continue };
        let from = e.reference(Field::ShotFrom);
        let splay = e.flag(Field::ShotSplay);
        let excluded = e.flag(Field::ShotExcludeFromAll);

        if from.is_some() && at != from {
            if !rows.is_empty() {
                rows.push(Row::blank(RowKind::Gap));
            }
            let mut r = Row::blank(RowKind::Station);
            r.station = from;
            r.label = label(doc, from);
            r.anchor = true;
            if let Some(src) = from.and_then(|s| walls_source(doc, s)) {
                r.lrud = Some(walls_at(doc, src).into());
                r.lrud_editable = src.trip && src.entity == trip;
            }
            if first_anchor {
                r.note = t.text(Field::TripStartNote);
                r.note_source = Some(trip);
            }
            first_anchor = false;
            names.insert(r.label.clone());
            rows.push(r);
            at = from;
        }

        let decl = applied_declination(e, Some(t));
        let mut row = Row::blank(if splay { RowKind::Splay } else { RowKind::Shot });
        row.shot = Some(shot);
        row.excluded = excluded;
        row.distance = e.number(Field::ShotDistance).map(|d| d * factor);
        let omitted = e.flag(Field::ShotAzimuthOmitted);
        let (az, az_back) = (
            e.number(Field::ShotAzimuth),
            e.number(Field::ShotBackAzimuth),
        );
        let (inc, inc_back) = (
            e.number(Field::ShotInclination),
            e.number(Field::ShotBackInclination),
        );
        row.az_fs = magnetic(az, decl);
        row.az_bs = magnetic(az_back, decl);
        row.inc_fs = inc;
        row.inc_bs = inc_back;
        row.note = e.text(Field::ShotNote);
        let mut warn = Vec::new();
        if let (Some(f), Some(b)) = (az, az_back)
            && angle_gap(f, b + 180.0) > BACKSIGHT_TOLERANCE_DEG
        {
            warn.push(format!(
                "compass foresight and backsight differ by {:.1}\u{b0}",
                angle_gap(f, b + 180.0)
            ));
        }
        if let (Some(f), Some(b)) = (inc, inc_back)
            && (f + b).abs() > BACKSIGHT_TOLERANCE_DEG
        {
            warn.push(format!(
                "clino foresight and backsight differ by {:.1}\u{b0}",
                (f + b).abs()
            ));
        }
        if !excluded
            && (row.distance.is_none()
                || (az.is_none() && az_back.is_none() && !omitted)
                || (inc.is_none() && inc_back.is_none()))
        {
            warn.push("a reading is missing: the shot is not placed".into());
        }
        row.warn = warn.join("; ");
        if !row.warn.is_empty() {
            totals.warnings += 1;
        }

        if splay {
            totals.splays += 1;
            let n = splays_at.entry(from.unwrap_or(shot)).or_insert(0);
            *n += 1;
            row.label = format!("{}.{}", label(doc, from), n);
            row.note_source = Some(shot);
            rows.push(row);
            continue;
        }

        let counts = !excluded && !e.flag(Field::ShotExcludeFromLength);
        if counts {
            totals.length += row.distance.unwrap_or(0.0);
            totals.shots += 1;
        }
        row.total = Some(totals.length);
        rows.push(row);

        let to = e.reference(Field::ShotTo);
        let mut r = Row::blank(RowKind::Station);
        r.station = to;
        r.label = label(doc, to);
        r.shot = Some(shot);
        r.lrud = Some(shot_lrud(doc, shot, factor));
        r.lrud_editable = true;
        r.note = e.text(Field::ShotNote);
        r.note_source = Some(shot);
        names.insert(r.label.clone());
        rows.push(r);
        at = to;
    }
    names.remove("");
    totals.stations = names.len();

    // Loops and ties closed by this trip's shots, solved on the raw survey (what was written).
    let ex = extract(doc);
    let resolved = resolve(&ex.legs, &ex.fixed);
    let mut closures = Vec::new();
    for c in &resolved.closures {
        let Some(row) = rows
            .iter()
            .position(|r| r.kind == RowKind::Shot && r.shot == Some(c.shot))
        else {
            continue;
        };
        closures.push(ClosureRow {
            kind: if c.kind == ClosureKind::Loop {
                "loop"
            } else {
                "tie"
            },
            shot: c.shot,
            row,
            path: c.path.iter().map(|s| label(doc, Some(*s))).collect(),
            horizontal: c.horizontal,
            vertical: c.vertical,
            length: c.traverse_length,
            percent: c.percent,
        });
    }

    Some(Page {
        trip,
        index,
        name: t.text(Field::TripName),
        date: t.text(Field::TripDate),
        team: t.text(Field::TripTeam),
        instruments: t.text(Field::TripInstruments),
        unit: match t.text(Field::TripDistanceUnit).as_str() {
            "ft" => "ft".into(),
            _ => "m".into(),
        },
        declination: t.number(Field::TripDeclination),
        rows,
        totals,
        closures,
    })
}
