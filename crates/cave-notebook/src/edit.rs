//! What a person writes on the page, as document actions. Nothing is applied here: the caller
//! makes one undoable edit of the actions returned.

use crate::page::{applied_declination, trip_shots};
use cave_doc::{Action, Document, Field, Kind, Value};
use cave_draw::{DrawError, Walls, WallsSource, set_walls_at, shot_now, walls_source};
use cave_id::{Id, IdSource};

/// Why an edit was refused. Nothing changes when one is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditError {
    /// There is no such trip.
    NoSuchTrip,
    /// There is no such shot.
    NoSuchShot,
    /// There is no such station.
    NoSuchStation,
    /// A value that cannot be right (a negative tape, a clino past vertical, a blank name, ...).
    Invalid(&'static str),
    /// Another station already has that name.
    NameTaken,
}

impl From<DrawError> for EditError {
    fn from(_: DrawError) -> Self {
        EditError::Invalid("that value is out of range")
    }
}

/// One of the five readings of a shot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reading {
    /// Tape.
    Distance,
    /// Foresight compass (magnetic).
    AzimuthFs,
    /// Backsight compass (magnetic).
    AzimuthBs,
    /// Foresight clino.
    InclinationFs,
    /// Backsight clino.
    InclinationBs,
}

fn shot_entity(doc: &Document, shot: Id) -> Result<&cave_doc::Entity, EditError> {
    doc.entity(shot)
        .filter(|e| e.kind() == Kind::Shot)
        .ok_or(EditError::NoSuchShot)
}

/// Sets one reading of a shot: metres for the tape, degrees for the rest. A compass reading is what
/// the compass said (magnetic); the trip's declination turns it into the true bearing the survey keeps.
/// `None` clears it (not measured).
pub fn set_reading(
    doc: &Document,
    shot: Id,
    which: Reading,
    value: Option<f64>,
) -> Result<Vec<Action>, EditError> {
    let e = shot_entity(doc, shot)?;
    let trip = e.reference(Field::ShotTrip).and_then(|t| doc.entity(t));
    let decl = applied_declination(e, trip);
    let mut edit = shot_now(doc, shot).ok_or(EditError::NoSuchShot)?;
    let true_bearing = |v: Option<f64>| v.map(|m| (m + decl).rem_euclid(360.0));
    match which {
        Reading::Distance => edit.distance = value,
        Reading::AzimuthFs => edit.azimuth = true_bearing(value),
        Reading::AzimuthBs => edit.back_azimuth = true_bearing(value),
        Reading::InclinationFs => edit.inclination = value,
        Reading::InclinationBs => edit.back_inclination = value,
    }
    Ok(cave_draw::set_shot(doc, shot, edit)?)
}

/// Sets the passage dimensions beside a station line. `target` is the shot that arrives at the
/// station, or the station itself (written where its dimensions live: the trip's opening row).
pub fn set_walls(doc: &Document, target: Id, walls: Walls) -> Result<Vec<Action>, EditError> {
    let kind = doc.entity(target).map(|e| e.kind());
    match kind {
        Some(Kind::Shot) => {
            let trip = doc
                .entity(target)
                .and_then(|e| e.reference(Field::ShotTrip));
            let src = WallsSource {
                entity: target,
                trip: false,
                factor: cave_draw::factor_of(doc, trip),
            };
            Ok(set_walls_at(doc, src, walls)?)
        }
        Some(Kind::Station) => {
            let src = walls_source(doc, target).ok_or(EditError::NoSuchStation)?;
            Ok(set_walls_at(doc, src, walls)?)
        }
        _ => Err(EditError::NoSuchStation),
    }
}

/// Sets the note on a line: `source` is the shot or trip the page gave for it.
pub fn set_notes(doc: &Document, source: Id, text: &str) -> Result<Vec<Action>, EditError> {
    Ok(cave_draw::set_note(doc, source, text)?)
}

/// A header field of a trip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TripField {
    /// The trip's name.
    Name,
    /// The date, `YYYY-MM-DD` or empty.
    Date,
    /// Who was on the trip.
    Team,
    /// What they measured with.
    Instruments,
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        })
}

/// Sets a header field of a trip (only written when it changes).
pub fn set_trip_field(
    doc: &Document,
    trip: Id,
    which: TripField,
    text: &str,
) -> Result<Vec<Action>, EditError> {
    let t = doc
        .entity(trip)
        .filter(|e| e.kind() == Kind::Trip)
        .ok_or(EditError::NoSuchTrip)?;
    let text: String = text.trim().chars().take(200).collect();
    let field = match which {
        TripField::Name => Field::TripName,
        TripField::Date => {
            if !text.is_empty() && !is_date(&text) {
                return Err(EditError::Invalid("a date is written 2025-03-02"));
            }
            Field::TripDate
        }
        TripField::Team => Field::TripTeam,
        TripField::Instruments => Field::TripInstruments,
    };
    if t.text(field) == text {
        return Ok(Vec::new());
    }
    Ok(vec![Action::Set {
        id: trip,
        field,
        value: Value::Text(text),
    }])
}

/// Changes the declination a trip's compass readings are corrected by. The page's cells hold what
/// the compass read, so every bearing in the trip turns by the difference (the whole trip swings).
pub fn set_trip_declination(doc: &Document, trip: Id, new: f64) -> Result<Vec<Action>, EditError> {
    let t = doc
        .entity(trip)
        .filter(|e| e.kind() == Kind::Trip)
        .ok_or(EditError::NoSuchTrip)?;
    if !new.is_finite() || new.abs() > 90.0 {
        return Err(EditError::Invalid(
            "declination is between -90 and 90 degrees",
        ));
    }
    let mut actions = Vec::new();
    for shot in trip_shots(doc, trip) {
        let Some(e) = doc.entity(shot) else { continue };
        let d = new - applied_declination(e, Some(t));
        for field in [Field::ShotAzimuth, Field::ShotBackAzimuth] {
            if let Some(a) = e.number(field)
                && d.abs() > 1e-9
            {
                actions.push(Action::Set {
                    id: shot,
                    field,
                    value: Value::Number(Some((a + d).rem_euclid(360.0))),
                });
            }
        }
        if e.number(Field::ShotDeclination).is_some() {
            // Its bearing is now a `new` bearing, which is what the trip's value says.
            actions.push(Action::Set {
                id: shot,
                field: Field::ShotDeclination,
                value: Value::Number(None),
            });
        }
    }
    if t.number(Field::TripDeclination) != Some(new) {
        actions.push(Action::Set {
            id: trip,
            field: Field::TripDeclination,
            value: Value::Number(Some(new)),
        });
    }
    Ok(actions)
}

fn station_named(doc: &Document, name: &str) -> Option<Id> {
    doc.ids_of(Kind::Station).into_iter().find(|id| {
        doc.entity(*id)
            .is_some_and(|e| e.text(Field::StationLabel) == name)
    })
}

fn max_order(doc: &Document, kind: Kind, field: Field) -> f64 {
    doc.ids_of(kind)
        .into_iter()
        .filter_map(|id| doc.entity(id).and_then(|e| e.number(field)))
        .fold(-1.0, f64::max)
}

/// Adds a shot at the end of a trip's page, from the station named `from` (made if it is new) to the
/// station named `to` (made if it is new; an existing name joins that station and closes a loop).
/// A splay has no `to`. Its readings start empty. Returns the new shot's id with the actions.
pub fn add_shot(
    doc: &Document,
    ids: &mut dyn IdSource,
    trip: Id,
    from: &str,
    to: &str,
    splay: bool,
) -> Result<(Id, Vec<Action>), EditError> {
    doc.entity(trip)
        .filter(|e| e.kind() == Kind::Trip)
        .ok_or(EditError::NoSuchTrip)?;
    let (from, to) = (from.trim(), to.trim());
    if from.is_empty() {
        return Err(EditError::Invalid("a shot starts at a named station"));
    }
    if !splay && to.is_empty() {
        return Err(EditError::Invalid("a shot ends at a named station"));
    }
    if !splay && from == to {
        return Err(EditError::Invalid("a shot joins two different stations"));
    }
    let mut actions = Vec::new();
    let mut station = |name: &str, actions: &mut Vec<Action>| -> Id {
        station_named(doc, name).unwrap_or_else(|| {
            let id = ids.next_id();
            actions.push(Action::Create {
                id,
                kind: Kind::Station,
            });
            actions.push(Action::Set {
                id,
                field: Field::StationLabel,
                value: Value::Text(name.into()),
            });
            id
        })
    };
    let from_id = station(from, &mut actions);
    let to_id = (!splay).then(|| station(to, &mut actions));
    let id = ids.next_id();
    actions.push(Action::Create {
        id,
        kind: Kind::Shot,
    });
    let mut set = |field, value| actions.push(Action::Set { id, field, value });
    set(Field::ShotTrip, Value::Ref(Some(trip)));
    set(Field::ShotFrom, Value::Ref(Some(from_id)));
    if let Some(to_id) = to_id {
        set(Field::ShotTo, Value::Ref(Some(to_id)));
    }
    if splay {
        set(Field::ShotSplay, Value::Flag(true));
    }
    set(
        Field::ShotOrder,
        Value::Number(Some(max_order(doc, Kind::Shot, Field::ShotOrder) + 1.0)),
    );
    Ok((id, actions))
}

/// Takes a shot off the page. Its stations stay in the document, whatever else touches them.
pub fn remove_shot(doc: &Document, shot: Id) -> Result<Vec<Action>, EditError> {
    shot_entity(doc, shot)?;
    Ok(vec![Action::Remove {
        id: shot,
        kind: Kind::Shot,
    }])
}

/// Renames a station (its label: identity does not change, so every page and drawing follows).
pub fn rename_station(doc: &Document, station: Id, name: &str) -> Result<Vec<Action>, EditError> {
    let e = doc
        .entity(station)
        .filter(|e| e.kind() == Kind::Station)
        .ok_or(EditError::NoSuchStation)?;
    let name = name.trim();
    if name.is_empty() {
        return Err(EditError::Invalid("a station needs a name"));
    }
    if e.text(Field::StationLabel) == name {
        return Ok(Vec::new());
    }
    if station_named(doc, name).is_some_and(|other| other != station) {
        return Err(EditError::NameTaken);
    }
    Ok(vec![Action::Set {
        id: station,
        field: Field::StationLabel,
        value: Value::Text(name.into()),
    }])
}

/// Starts a new trip after the last one, in the same unit and declination. Returns its id with the actions.
pub fn new_trip(
    doc: &Document,
    ids: &mut dyn IdSource,
    name: &str,
) -> Result<(Id, Vec<Action>), EditError> {
    let last = doc
        .ids_of(Kind::Trip)
        .into_iter()
        .max_by(|a, b| {
            let order = |id: &Id| {
                doc.entity(*id)
                    .and_then(|e| e.number(Field::TripOrder))
                    .unwrap_or(f64::MAX)
            };
            order(a).total_cmp(&order(b)).then(a.cmp(b))
        })
        .and_then(|id| doc.entity(id));
    let id = ids.next_id();
    let mut actions = vec![
        Action::Create {
            id,
            kind: Kind::Trip,
        },
        Action::Set {
            id,
            field: Field::TripName,
            value: Value::Text(name.trim().chars().take(200).collect()),
        },
        Action::Set {
            id,
            field: Field::TripOrder,
            value: Value::Number(Some(max_order(doc, Kind::Trip, Field::TripOrder) + 1.0)),
        },
    ];
    if let Some(last) = last {
        actions.push(Action::Set {
            id,
            field: Field::TripDistanceUnit,
            value: Value::Text(last.text(Field::TripDistanceUnit)),
        });
        if last.number(Field::TripDeclination).is_some() {
            actions.push(Action::Set {
                id,
                field: Field::TripDeclination,
                value: Value::Number(last.number(Field::TripDeclination)),
            });
        }
    }
    Ok((id, actions))
}

/// A brand new cave: a document with one empty trip (named `trip`), in `unit` (`ft` or `m`) and with
/// the cave's `name` kept on that first trip. Everything else is built up from here, shot by shot.
pub fn new_cave(
    actor: cave_doc::ActorId,
    ids: &mut dyn IdSource,
    name: &str,
    trip: &str,
    unit: &str,
) -> Result<Document, EditError> {
    let mut doc = Document::new(actor);
    let id = ids.next_id();
    let text = |f, s: &str| Action::Set {
        id,
        field: f,
        value: Value::Text(s.trim().chars().take(200).collect()),
    };
    let actions = vec![
        Action::Create {
            id,
            kind: Kind::Trip,
        },
        text(Field::TripName, trip),
        text(Field::TripCaveName, name),
        text(
            Field::TripDistanceUnit,
            if unit == "ft" { "ft" } else { "m" },
        ),
        Action::Set {
            id,
            field: Field::TripOrder,
            value: Value::Number(Some(0.0)),
        },
    ];
    doc.edit(actions)
        .map_err(|_| EditError::Invalid("a new cave could not be started"))?;
    doc.take_outbox();
    Ok(doc)
}
