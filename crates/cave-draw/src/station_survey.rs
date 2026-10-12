//! Editing what the survey says at a station: the passage dimensions (LRUD), the notes, and whether the
//! station is an agreed benchmark. The survey keeps walls on shots, so an edit goes to the shot that
//! supplies them (the last shot arriving at the station that recorded any, else the last arriving;
//! a station nothing arrives at takes its trip's opening row). Values are metres here and are turned
//! back into the trip's own unit when written.

use crate::{DrawError, MAX_TEXT};
use cave_doc::{Action, Document, Field, Kind, Value};
use cave_id::Id;

const FEET_TO_METRES: f64 = 0.3048;

/// Passage dimensions at a station, metres; `None` is not measured, an open flag is "looked, no wall".
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Walls {
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

/// Where a station's walls are written: a shot, or a trip's opening row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WallsSource {
    /// The shot or trip that holds them.
    pub entity: Id,
    /// True when it is a trip's opening row.
    pub trip: bool,
    /// Metres in one of the trip's distance units.
    pub factor: f64,
}

type Fields = [(Field, Field); 4];

const SHOT: Fields = [
    (Field::ShotLeft, Field::ShotLeftOpen),
    (Field::ShotRight, Field::ShotRightOpen),
    (Field::ShotUp, Field::ShotUpOpen),
    (Field::ShotDown, Field::ShotDownOpen),
];
const TRIP: Fields = [
    (Field::TripStartLeft, Field::TripStartLeftOpen),
    (Field::TripStartRight, Field::TripStartRightOpen),
    (Field::TripStartUp, Field::TripStartUpOpen),
    (Field::TripStartDown, Field::TripStartDownOpen),
];

fn ordered_shots(doc: &Document) -> Vec<Id> {
    let mut shots: Vec<(f64, Id)> = doc
        .ids_of(Kind::Shot)
        .into_iter()
        .map(|id| {
            let order = doc
                .entity(id)
                .and_then(|e| e.number(Field::ShotOrder))
                .unwrap_or(f64::MAX);
            (order, id)
        })
        .collect();
    shots.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    shots.into_iter().map(|s| s.1).collect()
}

/// Metres in one of a trip's distance units (1 for metric or no trip, 0.3048 for feet).
pub fn factor_of(doc: &Document, trip: Option<Id>) -> f64 {
    match trip
        .and_then(|t| doc.entity(t))
        .map(|t| t.text(Field::TripDistanceUnit))
        .as_deref()
    {
        Some("ft") => FEET_TO_METRES,
        _ => 1.0,
    }
}

fn has_evidence(doc: &Document, id: Id, fields: &Fields) -> bool {
    doc.entity(id).is_some_and(|e| {
        fields
            .iter()
            .any(|(v, o)| e.number(*v).is_some() || e.flag(*o))
    })
}

/// The non-splay, non-excluded shots arriving at a station, in survey order.
fn arriving(doc: &Document, station: Id) -> Vec<Id> {
    ordered_shots(doc)
        .into_iter()
        .filter(|id| {
            doc.entity(*id).is_some_and(|e| {
                !e.flag(Field::ShotSplay)
                    && !e.flag(Field::ShotExcludeFromAll)
                    && e.reference(Field::ShotTo) == Some(station)
                    && e.reference(Field::ShotFrom).is_some()
            })
        })
        .collect()
}

/// Where this station's walls live (see the module's words), or `None` when no shot touches it.
pub fn walls_source(doc: &Document, station: Id) -> Option<WallsSource> {
    let arrivals = arriving(doc, station);
    if !arrivals.is_empty() {
        let id = arrivals
            .iter()
            .rev()
            .find(|id| has_evidence(doc, **id, &SHOT))
            .or(arrivals.last())
            .copied()?;
        let trip = doc.entity(id).and_then(|e| e.reference(Field::ShotTrip));
        return Some(WallsSource {
            entity: id,
            trip: false,
            factor: factor_of(doc, trip),
        });
    }
    let leaving = ordered_shots(doc).into_iter().find(|id| {
        doc.entity(*id).is_some_and(|e| {
            !e.flag(Field::ShotSplay) && e.reference(Field::ShotFrom) == Some(station)
        })
    })?;
    let trip = doc
        .entity(leaving)
        .and_then(|e| e.reference(Field::ShotTrip))?;
    Some(WallsSource {
        entity: trip,
        trip: true,
        factor: factor_of(doc, Some(trip)),
    })
}

/// The walls held at a source, in metres.
pub fn walls_at(doc: &Document, src: WallsSource) -> Walls {
    let Some(e) = doc.entity(src.entity) else {
        return Walls::default();
    };
    let f = if src.trip { &TRIP } else { &SHOT };
    let n = |i: usize| e.number(f[i].0).map(|v| v * src.factor);
    Walls {
        left: n(0),
        right: n(1),
        up: n(2),
        down: n(3),
        left_open: e.flag(f[0].1),
        right_open: e.flag(f[1].1),
        up_open: e.flag(f[2].1),
        down_open: e.flag(f[3].1),
    }
}

/// Sets a station's walls: writes only what differs from what the survey holds now. A dimension must be
/// a finite number of metres, zero or more.
pub fn set_station_walls(
    doc: &Document,
    station: Id,
    walls: Walls,
) -> Result<Vec<Action>, DrawError> {
    let src = walls_source(doc, station).ok_or(DrawError::NoStations)?;
    set_walls_at(doc, src, walls)
}

/// Sets the walls held at one source (a shot, or a trip's opening row): the same edit as
/// [`set_station_walls`], for a caller that already knows which row it means (a notebook line).
pub fn set_walls_at(
    doc: &Document,
    src: WallsSource,
    walls: Walls,
) -> Result<Vec<Action>, DrawError> {
    let now = walls_at(doc, src);
    let f = if src.trip { &TRIP } else { &SHOT };
    let wanted = [
        (walls.left, walls.left_open, now.left, now.left_open),
        (walls.right, walls.right_open, now.right, now.right_open),
        (walls.up, walls.up_open, now.up, now.up_open),
        (walls.down, walls.down_open, now.down, now.down_open),
    ];
    let mut actions = Vec::new();
    for (i, (v, open, was_v, was_open)) in wanted.into_iter().enumerate() {
        if v.is_some_and(|x| !x.is_finite() || x < 0.0) {
            return Err(DrawError::BadStroke);
        }
        let same = match (v, was_v) {
            (None, None) => true,
            (Some(a), Some(b)) => (a - b).abs() < 1e-9,
            _ => false,
        };
        if !same {
            actions.push(Action::Set {
                id: src.entity,
                field: f[i].0,
                value: Value::Number(v.map(|x| x / src.factor)),
            });
        }
        if open != was_open {
            actions.push(Action::Set {
                id: src.entity,
                field: f[i].1,
                value: Value::Flag(open),
            });
        }
    }
    Ok(actions)
}

/// The notes written at a station as `(source, text)`: the note on each shot arriving at it, and the
/// trip's opening note when it is where a trip starts. The source is what [`set_note`] edits.
pub fn note_sources(doc: &Document, station: Id) -> Vec<(Id, String)> {
    let mut out = Vec::new();
    let first = ordered_shots(doc).into_iter().find(|id| {
        doc.entity(*id)
            .is_some_and(|e| !e.flag(Field::ShotSplay) && !e.flag(Field::ShotExcludeFromAll))
    });
    if let Some(e) = first.and_then(|id| doc.entity(id))
        && e.reference(Field::ShotFrom) == Some(station)
        && let Some(trip) = e.reference(Field::ShotTrip)
    {
        out.push((
            trip,
            doc.entity(trip)
                .map(|t| t.text(Field::TripStartNote))
                .unwrap_or_default(),
        ));
    }
    for id in arriving(doc, station) {
        let text = doc
            .entity(id)
            .map(|e| e.text(Field::ShotNote))
            .unwrap_or_default();
        out.push((id, text));
    }
    out
}

/// Sets a note: the words of a shot's note or a trip's opening note. Nothing is written when unchanged.
pub fn set_note(doc: &Document, source: Id, text: &str) -> Result<Vec<Action>, DrawError> {
    let e = doc.entity(source).ok_or(DrawError::NoStations)?;
    let field = match e.kind() {
        Kind::Shot => Field::ShotNote,
        Kind::Trip => Field::TripStartNote,
        _ => return Err(DrawError::NoStations),
    };
    let text: String = text.trim().chars().take(MAX_TEXT).collect();
    if e.text(field) == text {
        return Ok(Vec::new());
    }
    Ok(vec![Action::Set {
        id: source,
        field,
        value: Value::Text(text),
    }])
}

/// Marks a station as an agreed benchmark, or not.
pub fn set_station_benchmark(
    doc: &Document,
    station: Id,
    on: bool,
) -> Result<Vec<Action>, DrawError> {
    let e = doc
        .entity(station)
        .filter(|e| e.kind() == Kind::Station)
        .ok_or(DrawError::NoStations)?;
    if e.flag(Field::StationBenchmark) == on {
        return Ok(Vec::new());
    }
    Ok(vec![Action::Set {
        id: station,
        field: Field::StationBenchmark,
        value: Value::Flag(on),
    }])
}

/// What a person can change about one shot (a leg between stations, or a splay). Lengths are metres
/// (written in the trip's unit); angles are degrees; `None` means not measured.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShotEdit {
    /// Tape reading.
    pub distance: Option<f64>,
    /// Compass bearing.
    pub azimuth: Option<f64>,
    /// Clinometer reading.
    pub inclination: Option<f64>,
    /// Back bearing.
    pub back_azimuth: Option<f64>,
    /// Back inclination.
    pub back_inclination: Option<f64>,
    /// Left out of the plotted map.
    pub exclude_from_plot: bool,
    /// Left out of everything (a void shot).
    pub exclude_from_all: bool,
    /// Held as measured when the survey is adjusted.
    pub no_adjust: bool,
}

/// Sets what is measured on a shot: only what differs from now is written. A distance must be zero or
/// more, a bearing finite, an inclination between -90 and 90 degrees.
pub fn set_shot(doc: &Document, shot: Id, edit: ShotEdit) -> Result<Vec<Action>, DrawError> {
    let e = doc
        .entity(shot)
        .filter(|e| e.kind() == Kind::Shot)
        .ok_or(DrawError::NoStations)?;
    let factor = factor_of(doc, e.reference(Field::ShotTrip));
    let ok =
        |v: Option<f64>, lo: f64, hi: f64| v.is_none_or(|x| x.is_finite() && x >= lo && x <= hi);
    if !ok(edit.distance, 0.0, 1e6)
        || !ok(edit.azimuth, -360.0, 720.0)
        || !ok(edit.back_azimuth, -360.0, 720.0)
        || !ok(edit.inclination, -90.0, 90.0)
        || !ok(edit.back_inclination, -90.0, 90.0)
    {
        return Err(DrawError::BadStroke);
    }
    let mut actions = Vec::new();
    let mut number = |field: Field, want: Option<f64>, scale: f64| {
        let now = e.number(field);
        let same = match (want, now) {
            (None, None) => true,
            (Some(a), Some(b)) => (a / scale - b).abs() < 1e-9,
            _ => false,
        };
        if !same {
            actions.push(Action::Set {
                id: shot,
                field,
                value: Value::Number(want.map(|v| v / scale)),
            });
        }
    };
    number(Field::ShotDistance, edit.distance, factor);
    number(Field::ShotAzimuth, edit.azimuth, 1.0);
    number(Field::ShotInclination, edit.inclination, 1.0);
    number(Field::ShotBackAzimuth, edit.back_azimuth, 1.0);
    number(Field::ShotBackInclination, edit.back_inclination, 1.0);
    for (field, want) in [
        (Field::ShotExcludeFromPlot, edit.exclude_from_plot),
        (Field::ShotExcludeFromAll, edit.exclude_from_all),
        (Field::ShotNoAdjust, edit.no_adjust),
    ] {
        if e.flag(field) != want {
            actions.push(Action::Set {
                id: shot,
                field,
                value: Value::Flag(want),
            });
        }
    }
    Ok(actions)
}

/// What is measured on a shot now, as a [`ShotEdit`] (distance in metres).
pub fn shot_now(doc: &Document, shot: Id) -> Option<ShotEdit> {
    let e = doc.entity(shot).filter(|e| e.kind() == Kind::Shot)?;
    let f = factor_of(doc, e.reference(Field::ShotTrip));
    Some(ShotEdit {
        distance: e.number(Field::ShotDistance).map(|v| v * f),
        azimuth: e.number(Field::ShotAzimuth),
        inclination: e.number(Field::ShotInclination),
        back_azimuth: e.number(Field::ShotBackAzimuth),
        back_inclination: e.number(Field::ShotBackInclination),
        exclude_from_plot: e.flag(Field::ShotExcludeFromPlot),
        exclude_from_all: e.flag(Field::ShotExcludeFromAll),
        no_adjust: e.flag(Field::ShotNoAdjust),
    })
}
