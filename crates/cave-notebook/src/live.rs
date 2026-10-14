//! Edits for shots taken as they are measured, in the cave, one at a time: a whole shot (the stations,
//! the line and every reading) as ONE edit instead of an add-shot and a set per reading, the next
//! station's name, and a station's passage dimensions found by its name.
//!
//! Like everything in [`crate::edit`] nothing is applied here: the caller makes one undoable edit of the
//! actions returned.

use crate::edit::{EditError, Reading, add_shot, set_reading};
use crate::page::trip_shots;
use cave_doc::{Action, Document, Field, Kind, Value};
use cave_draw::{Walls, WallsSource, factor_of, set_walls_at, walls_at, walls_source};
use cave_id::{Id, IdSource};
use std::collections::HashSet;

/// The next station name after `from`: its trailing number plus one (`A3` to `A4`, `Z09` to `Z10`),
/// skipping any name in `taken` so a loop is never closed by accident. Empty when there is no suggestion
/// (the name does not end in a number).
pub fn suggest_next(from: &str, taken: &HashSet<String>) -> String {
    let from = from.trim();
    let digits = from.len() - from.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    if digits == 0 {
        return String::new();
    }
    let (prefix, number) = from.split_at(from.len() - digits);
    let Ok(mut n) = number.parse::<u128>() else {
        return String::new();
    };
    let width = if number.starts_with('0') { digits } else { 0 };
    for _ in 0..1000 {
        n += 1;
        let next = format!("{prefix}{n:0>width$}");
        if !taken.contains(&next) {
            return next;
        }
    }
    String::new()
}

/// The names of the stations any shot of the cave starts or ends at: the names a new station must not reuse
/// by accident.
pub fn stations_in_use(doc: &Document) -> HashSet<String> {
    let mut out = HashSet::new();
    for shot in doc.ids_of(Kind::Shot) {
        let Some(e) = doc.entity(shot) else { continue };
        for field in [Field::ShotFrom, Field::ShotTo] {
            if let Some(s) = e.reference(field).and_then(|s| doc.entity(s)) {
                out.insert(s.text(Field::StationLabel));
            }
        }
    }
    out.remove("");
    out
}

/// What was measured on a shot: metres and degrees, compass readings as the compass said (magnetic).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Measured {
    /// Tape.
    pub distance: Option<f64>,
    /// Foresight compass.
    pub azimuth: Option<f64>,
    /// Foresight clino.
    pub inclination: Option<f64>,
    /// Backsight compass.
    pub back_azimuth: Option<f64>,
    /// Backsight clino.
    pub back_inclination: Option<f64>,
}

/// Adds a shot at the end of a trip's page, from `from` to `to` (empty for a splay), with its readings
/// already in it: one edit, so taking it back is one step. Returns the new shot's id with the actions.
pub fn add_measured_shot(
    doc: &Document,
    ids: &mut dyn IdSource,
    trip: Id,
    from: &str,
    to: &str,
    splay: bool,
    m: Measured,
) -> Result<(Id, Vec<Action>), EditError> {
    let ok =
        |v: Option<f64>, lo: f64, hi: f64| v.is_none_or(|x| x.is_finite() && x >= lo && x <= hi);
    if !ok(m.distance, 0.0, 1e6)
        || !ok(m.azimuth, -360.0, 720.0)
        || !ok(m.back_azimuth, -360.0, 720.0)
        || !ok(m.inclination, -90.0, 90.0)
        || !ok(m.back_inclination, -90.0, 90.0)
    {
        return Err(EditError::Invalid("that value is out of range"));
    }
    let (shot, mut actions) = add_shot(doc, ids, trip, from, to, splay)?;
    let trip_entity = doc.entity(trip);
    let decl = trip_entity
        .and_then(|t| t.number(Field::TripDeclination))
        .unwrap_or(0.0);
    let factor = factor_of(doc, Some(trip));
    let true_bearing = |v: Option<f64>| v.map(|x| (x + decl).rem_euclid(360.0));
    for (field, value) in [
        (Field::ShotDistance, m.distance.map(|d| d / factor)),
        (Field::ShotAzimuth, true_bearing(m.azimuth)),
        (Field::ShotInclination, m.inclination),
        (Field::ShotBackAzimuth, true_bearing(m.back_azimuth)),
        (Field::ShotBackInclination, m.back_inclination),
    ] {
        if value.is_some() {
            actions.push(Action::Set {
                id: shot,
                field,
                value: Value::Number(value),
            });
        }
    }
    Ok((shot, actions))
}

/// Sets several readings of one shot as one edit. `None` clears a reading.
pub fn set_readings(
    doc: &Document,
    shot: Id,
    cells: &[(Reading, Option<f64>)],
) -> Result<Vec<Action>, EditError> {
    let mut actions = Vec::new();
    for (which, value) in cells {
        actions.extend(set_reading(doc, shot, *which, *value)?);
    }
    Ok(actions)
}

/// A wall of a station.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// Left of the direction of travel.
    Left,
    /// Right of the direction of travel.
    Right,
    /// The ceiling.
    Up,
    /// The floor.
    Down,
}

impl Side {
    /// All four, in the order the page writes them.
    pub const ALL: [Side; 4] = [Side::Left, Side::Right, Side::Up, Side::Down];

    /// The value of this wall.
    pub fn of(self, w: &Walls) -> Option<f64> {
        match self {
            Side::Left => w.left,
            Side::Right => w.right,
            Side::Up => w.up,
            Side::Down => w.down,
        }
    }

    fn set(self, w: &mut Walls, v: Option<f64>) {
        let (slot, open) = match self {
            Side::Left => (&mut w.left, &mut w.left_open),
            Side::Right => (&mut w.right, &mut w.right_open),
            Side::Up => (&mut w.up, &mut w.up_open),
            Side::Down => (&mut w.down, &mut w.down_open),
        };
        *slot = v;
        *open = false;
    }
}

fn station_named(doc: &Document, name: &str) -> Option<Id> {
    doc.ids_of(Kind::Station).into_iter().find(|id| {
        doc.entity(*id)
            .is_some_and(|e| e.text(Field::StationLabel) == name)
    })
}

/// Where the passage dimensions of the station named `label` are written on this trip's page: the latest
/// shot that arrives at it, else (for the trip's first station) the trip's opening row. `None` while the
/// station has no line on the page yet (before its first leg).
fn wall_source(doc: &Document, trip: Id, label: &str) -> Option<WallsSource> {
    let station = station_named(doc, label)?;
    let arriving = trip_shots(doc, trip).into_iter().rev().find(|s| {
        doc.entity(*s).is_some_and(|e| {
            !e.flag(Field::ShotSplay) && e.reference(Field::ShotTo) == Some(station)
        })
    });
    if let Some(shot) = arriving {
        return Some(WallsSource {
            entity: shot,
            trip: false,
            factor: factor_of(doc, Some(trip)),
        });
    }
    walls_source(doc, station).filter(|src| src.trip && src.entity == trip)
}

/// The passage dimensions of the station named `label`, as the page shows them, or `None` before the page
/// has a line for it.
pub fn station_walls(doc: &Document, trip: Id, label: &str) -> Option<Walls> {
    wall_source(doc, trip, label).map(|src| walls_at(doc, src))
}

/// Sets (or with `None` clears) wall dimensions of the station named `label`, metres, in one go. `Ok(None)`
/// when the station has no line on the page yet: nothing can be written until it has.
pub fn set_station_walls(
    doc: &Document,
    trip: Id,
    label: &str,
    changes: &[(Side, Option<f64>)],
) -> Result<Option<Vec<Action>>, EditError> {
    let Some(src) = wall_source(doc, trip, label) else {
        return Ok(None);
    };
    let mut walls = walls_at(doc, src);
    for (side, value) in changes {
        side.set(&mut walls, *value);
    }
    Ok(Some(set_walls_at(doc, src, walls)?))
}

/// Sets (or with `None` clears) one wall dimension of the station named `label`; see [`set_station_walls`].
pub fn set_station_wall(
    doc: &Document,
    trip: Id,
    label: &str,
    side: Side,
    value: Option<f64>,
) -> Result<Option<Vec<Action>>, EditError> {
    set_station_walls(doc, trip, label, &[(side, value)])
}
