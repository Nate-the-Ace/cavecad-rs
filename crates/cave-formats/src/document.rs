//! A [`Survey`] into CaveCAD's document and back: what Import and the survey-file exports go through.
//!
//! Into the document: one Trip per survey trip, one Station per name, one Shot per shot, distances
//! kept in the survey's unit (the trip records it) and fixed coordinates converted to metres. What the
//! document has no field for (an LRUD side's several readings) is kept as the primary reading only.
//!
//! Out of it: trips and shots in their own order, every distance in trip 0's unit (other trips are
//! converted), fixed coordinates back from metres, and a horizontal-tape trip's distances turned into
//! the slope distances every survey format expects (the same leg, measured the way the formats read).

use crate::model::{Fixed, Lrud, Shot, Survey, Trip, convert};
use cave_doc::{Action, ActorId, Document, Field, Kind, Value};
use cave_id::{Id, IdSource, SeqIds};
use std::collections::HashMap;

const FEET_TO_METRES: f64 = 0.3048;

/// A number the document can hold (it refuses NaN and infinities): `None` otherwise.
fn finite(v: Option<f64>) -> Value {
    Value::Number(v.filter(|x| x.is_finite()))
}

/// Builds a document from a survey, edited by `actor`. Trips with no name are named by their date,
/// or "Trip N".
pub fn to_document(survey: &Survey, actor: ActorId) -> Result<Document, String> {
    let mut survey = survey.clone();
    survey.ensure_trips();
    let mut ids = SeqIds::new(0x5eed);
    let mut doc = Document::new(actor);
    let unit = survey.unit().to_string();
    let edit = |doc: &mut Document, actions: Vec<Action>| {
        doc.edit(actions).map(|_| ()).map_err(|e| format!("{e:?}"))
    };

    let mut trips = Vec::new();
    for (i, t) in survey.trips.iter().enumerate() {
        let id = ids.next_id();
        let name = if !t.name.is_empty() {
            t.name.clone()
        } else if !t.date.is_empty() {
            t.date.clone()
        } else {
            format!("Trip {}", i + 1)
        };
        let set = |field, value| Action::Set { id, field, value };
        let mut actions = vec![
            Action::Create {
                id,
                kind: Kind::Trip,
            },
            set(Field::TripName, Value::Text(name)),
            set(Field::TripDistanceUnit, Value::Text(unit.clone())),
            set(Field::TripOrder, Value::Number(Some(i as f64))),
        ];
        for (text, field) in [
            (&t.date, Field::TripDate),
            (&t.team, Field::TripTeam),
            (&t.instruments, Field::TripInstruments),
            (&t.declination_source, Field::TripDeclinationSource),
            (&t.start_note, Field::TripStartNote),
        ] {
            if !text.is_empty() {
                actions.push(set(field, Value::Text(text.clone())));
            }
        }
        if t.declination != 0.0 && t.declination.is_finite() {
            actions.push(set(
                Field::TripDeclination,
                Value::Number(Some(t.declination)),
            ));
        }
        let cave = if !survey.cave_name.is_empty() {
            &survey.cave_name
        } else {
            &survey.name
        };
        if i == 0 && !cave.is_empty() {
            actions.push(set(Field::TripCaveName, Value::Text(cave.clone())));
        }
        if let Some(l) = &t.start_lrud {
            for (v, field) in [
                (l.left, Field::TripStartLeft),
                (l.right, Field::TripStartRight),
                (l.up, Field::TripStartUp),
                (l.down, Field::TripStartDown),
            ] {
                if v.is_some_and(f64::is_finite) {
                    actions.push(set(field, finite(v)));
                }
            }
            for (on, field) in [
                (l.left_open, Field::TripStartLeftOpen),
                (l.right_open, Field::TripStartRightOpen),
                (l.up_open, Field::TripStartUpOpen),
                (l.down_open, Field::TripStartDownOpen),
            ] {
                if on {
                    actions.push(set(field, Value::Flag(true)));
                }
            }
        }
        edit(&mut doc, actions)?;
        trips.push(id);
    }

    let mut stations: HashMap<String, Id> = HashMap::new();
    let mut station = |doc: &mut Document, name: &str, ids: &mut SeqIds| -> Result<Id, String> {
        if let Some(id) = stations.get(name) {
            return Ok(*id);
        }
        let id = ids.next_id();
        edit(
            doc,
            vec![
                Action::Create {
                    id,
                    kind: Kind::Station,
                },
                Action::Set {
                    id,
                    field: Field::StationLabel,
                    value: Value::Text(name.into()),
                },
            ],
        )?;
        stations.insert(name.to_string(), id);
        Ok(id)
    };

    for (i, s) in survey.shots.iter().enumerate() {
        let id = ids.next_id();
        let set = |field, value| Action::Set { id, field, value };
        let mut actions = vec![
            Action::Create {
                id,
                kind: Kind::Shot,
            },
            set(
                Field::ShotTrip,
                Value::Ref(Some(trips[s.trip.min(trips.len() - 1)])),
            ),
            set(Field::ShotOrder, Value::Number(Some(i as f64))),
        ];
        for (name, field) in [(&s.from, Field::ShotFrom), (&s.to, Field::ShotTo)] {
            if !name.is_empty() {
                let st = station(&mut doc, name, &mut ids)?;
                actions.push(set(field, Value::Ref(Some(st))));
            }
        }
        for (v, field) in [
            (Some(s.distance), Field::ShotDistance),
            (Some(s.azimuth), Field::ShotAzimuth),
            (Some(s.inclination), Field::ShotInclination),
            (s.back_azimuth, Field::ShotBackAzimuth),
            (s.back_inclination, Field::ShotBackInclination),
            (s.left, Field::ShotLeft),
            (s.right, Field::ShotRight),
            (s.up, Field::ShotUp),
            (s.down, Field::ShotDown),
        ] {
            if v.is_some_and(f64::is_finite) {
                actions.push(set(field, finite(v)));
            }
        }
        if !s.notes.is_empty() {
            actions.push(set(Field::ShotNote, Value::Text(s.notes.clone())));
        }
        if s.declination.is_some_and(f64::is_finite) {
            actions.push(set(Field::ShotDeclination, finite(s.declination)));
        }
        for (on, field) in [
            (s.splay, Field::ShotSplay),
            (s.exclude_from_all, Field::ShotExcludeFromAll),
            (s.exclude_from_plot, Field::ShotExcludeFromPlot),
            (s.exclude_from_length, Field::ShotExcludeFromLength),
            (s.no_adjust, Field::ShotNoAdjust),
            (s.azimuth_omitted, Field::ShotAzimuthOmitted),
            (s.left_open, Field::ShotLeftOpen),
            (s.right_open, Field::ShotRightOpen),
            (s.up_open, Field::ShotUpOpen),
            (s.down_open, Field::ShotDownOpen),
        ] {
            if on {
                actions.push(set(field, Value::Flag(true)));
            }
        }
        edit(&mut doc, actions)?;
    }

    let to_m = if unit == "ft" { FEET_TO_METRES } else { 1.0 };
    // name order, so the same survey always gives the same document
    let mut fixed: Vec<(&str, &Fixed)> = survey.fixed.iter().collect();
    fixed.sort_by(|a, b| a.0.cmp(b.0));
    for (name, p) in fixed {
        let st = station(&mut doc, name, &mut ids)?;
        let m = |v: Option<f64>| finite(v.map(|v| v * to_m));
        edit(
            &mut doc,
            vec![
                Action::Set {
                    id: st,
                    field: Field::StationFixedX,
                    value: m(p.x),
                },
                Action::Set {
                    id: st,
                    field: Field::StationFixedY,
                    value: m(p.y),
                },
                Action::Set {
                    id: st,
                    field: Field::StationFixedZ,
                    value: m(p.z),
                },
            ],
        )?;
    }
    Ok(doc)
}

/// Live entities of `kind`, in their stored order (`order` field, ties by id).
fn ordered(doc: &Document, kind: Kind, order: Field) -> Vec<Id> {
    let mut v: Vec<(f64, Id)> = doc
        .ids_of(kind)
        .into_iter()
        .map(|id| {
            (
                doc.entity(id)
                    .and_then(|e| e.number(order))
                    .unwrap_or(f64::MAX),
                id,
            )
        })
        .collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    v.into_iter().map(|(_, id)| id).collect()
}

fn unit_of(doc: &Document, trip: Option<Id>) -> &'static str {
    match trip
        .and_then(|t| doc.entity(t))
        .map(|t| t.text(Field::TripDistanceUnit))
        .as_deref()
    {
        Some("ft") => "ft",
        _ => "m",
    }
}

/// Reads the survey back out of a document, ready for any writer.
pub fn from_document(doc: &Document) -> Survey {
    let trip_ids = ordered(doc, Kind::Trip, Field::TripOrder);
    let unit = unit_of(doc, trip_ids.first().copied());
    let rank: HashMap<Id, usize> = trip_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, i))
        .collect();
    let label = |s: Option<Id>| {
        s.and_then(|s| doc.entity(s))
            .map(|e| e.text(Field::StationLabel))
            .unwrap_or_default()
    };

    let mut survey = Survey {
        distance_unit: Some(unit.into()),
        ..Survey::default()
    };
    for (i, &tid) in trip_ids.iter().enumerate() {
        let Some(t) = doc.entity(tid) else { continue };
        let from = unit_of(doc, Some(tid));
        let d = |v: Option<f64>| v.map(|v| convert(v, from, unit));
        let start = Lrud {
            left: d(t.number(Field::TripStartLeft)),
            right: d(t.number(Field::TripStartRight)),
            up: d(t.number(Field::TripStartUp)),
            down: d(t.number(Field::TripStartDown)),
            left_open: t.flag(Field::TripStartLeftOpen),
            right_open: t.flag(Field::TripStartRightOpen),
            up_open: t.flag(Field::TripStartUpOpen),
            down_open: t.flag(Field::TripStartDownOpen),
            ..Lrud::default()
        };
        let has_start =
            start.any() || start.left_open || start.right_open || start.up_open || start.down_open;
        if i == 0 {
            survey.cave_name = t.text(Field::TripCaveName);
        }
        survey.trips.push(Trip {
            name: t.text(Field::TripName),
            date: t.text(Field::TripDate),
            team: t.text(Field::TripTeam),
            instruments: t.text(Field::TripInstruments),
            declination: t.number(Field::TripDeclination).unwrap_or(0.0),
            declination_source: t.text(Field::TripDeclinationSource),
            distance_unit: Some(unit.into()),
            start_note: t.text(Field::TripStartNote),
            start_lrud: has_start.then_some(start),
        });
    }

    let mut seen: Vec<Id> = Vec::new();
    for sid in ordered(doc, Kind::Shot, Field::ShotOrder) {
        let Some(e) = doc.entity(sid) else { continue };
        let trip = e.reference(Field::ShotTrip);
        let from_unit = unit_of(doc, trip);
        let horizontal = trip
            .and_then(|t| doc.entity(t))
            .is_some_and(|t| t.text(Field::TripTapeMode) == "horizontal");
        let d = |v: Option<f64>| v.map(|v| convert(v, from_unit, unit));
        let inclination = e.number(Field::ShotInclination).unwrap_or(0.0);
        let mut distance = d(e.number(Field::ShotDistance)).unwrap_or(f64::NAN);
        if horizontal && inclination.abs() < 90.0 - 1e-9 {
            distance /= inclination.to_radians().cos();
        }
        for st in [e.reference(Field::ShotFrom), e.reference(Field::ShotTo)]
            .into_iter()
            .flatten()
        {
            if !seen.contains(&st) {
                seen.push(st);
            }
        }
        survey.shots.push(Shot {
            from: label(e.reference(Field::ShotFrom)),
            to: label(e.reference(Field::ShotTo)),
            distance,
            azimuth: e.number(Field::ShotAzimuth).unwrap_or(0.0),
            inclination,
            back_azimuth: e.number(Field::ShotBackAzimuth),
            back_inclination: e.number(Field::ShotBackInclination),
            azimuth_omitted: e.flag(Field::ShotAzimuthOmitted),
            left: d(e.number(Field::ShotLeft)),
            right: d(e.number(Field::ShotRight)),
            up: d(e.number(Field::ShotUp)),
            down: d(e.number(Field::ShotDown)),
            left_open: e.flag(Field::ShotLeftOpen),
            right_open: e.flag(Field::ShotRightOpen),
            up_open: e.flag(Field::ShotUpOpen),
            down_open: e.flag(Field::ShotDownOpen),
            splay: e.flag(Field::ShotSplay),
            exclude_from_plot: e.flag(Field::ShotExcludeFromPlot),
            exclude_from_all: e.flag(Field::ShotExcludeFromAll),
            exclude_from_length: e.flag(Field::ShotExcludeFromLength),
            no_adjust: e.flag(Field::ShotNoAdjust),
            notes: e.text(Field::ShotNote),
            trip: trip.and_then(|t| rank.get(&t).copied()).unwrap_or(0),
            declination: e.number(Field::ShotDeclination),
            ..Shot::default()
        });
    }

    let mut stations = doc.ids_of(Kind::Station);
    stations.sort();
    let order = seen
        .iter()
        .copied()
        .chain(stations.into_iter().filter(|s| !seen.contains(s)));
    let from_m = if unit == "ft" {
        1.0 / FEET_TO_METRES
    } else {
        1.0
    };
    for sid in order {
        let Some(e) = doc.entity(sid) else { continue };
        let (Some(x), Some(y)) = (
            e.number(Field::StationFixedX),
            e.number(Field::StationFixedY),
        ) else {
            continue;
        };
        let z = e.number(Field::StationFixedZ);
        survey.fixed.set(
            &e.text(Field::StationLabel),
            Fixed {
                x: Some(x * from_m),
                y: Some(y * from_m),
                z: z.map(|z| z * from_m),
            },
        );
    }
    survey.ensure_trips();
    survey
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Format;

    fn pitfall() -> Survey {
        let path = format!("{}/tests/golden/cases.json", env!("CARGO_MANIFEST_DIR"));
        let all: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let c = all["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "PitfallCave.dat")
            .unwrap();
        crate::compass::parse(c["input"].as_str().unwrap())
    }

    #[test]
    fn a_survey_comes_back_out_of_the_document_as_it_went_in() {
        let survey = pitfall();
        let doc = to_document(&survey, ActorId(1)).unwrap();
        let back = from_document(&doc);
        assert_eq!(back.shots.len(), survey.shots.len());
        assert_eq!(back.trips.len(), survey.trips.len());
        // every writer produces the same file from either
        for f in crate::FORMATS {
            assert_eq!(
                f.write(&mut back.clone()),
                f.write(&mut survey.clone()),
                "{}",
                f.id()
            );
        }
    }

    #[test]
    fn a_horizontal_tape_goes_out_as_the_slope_distance() {
        let mut survey = Survey::default();
        survey.shots.push(Shot {
            from: "A".into(),
            to: "B".into(),
            distance: 10.0,
            inclination: 60.0,
            ..Shot::default()
        });
        let mut doc = to_document(&survey, ActorId(1)).unwrap();
        let trip = doc.ids_of(Kind::Trip)[0];
        doc.edit(vec![Action::Set {
            id: trip,
            field: Field::TripTapeMode,
            value: Value::Text("horizontal".into()),
        }])
        .unwrap();
        let back = from_document(&doc);
        assert!((back.shots[0].distance - 20.0).abs() < 1e-9);
    }

    #[test]
    fn other_trips_are_converted_into_the_first_trips_unit() {
        let mut survey = Survey {
            distance_unit: Some("m".into()),
            ..Survey::default()
        };
        let metres = Trip {
            distance_unit: Some("m".into()),
            ..Trip::default()
        };
        survey.trips.push(metres.clone());
        survey.trips.push(Trip {
            date: "2024-01-02".into(),
            ..metres
        });
        for (trip, to) in [(0, "B"), (1, "C")] {
            survey.shots.push(Shot {
                from: "A".into(),
                to: to.into(),
                distance: 10.0,
                left: Some(1.0),
                trip,
                ..Shot::default()
            });
        }
        let mut doc = to_document(&survey, ActorId(1)).unwrap();
        let second = ordered(&doc, Kind::Trip, Field::TripOrder)[1];
        doc.edit(vec![Action::Set {
            id: second,
            field: Field::TripDistanceUnit,
            value: Value::Text("ft".into()),
        }])
        .unwrap();
        let back = from_document(&doc);
        assert_eq!(back.unit(), "m");
        assert_eq!(back.shots[0].distance, 10.0);
        assert!((back.shots[1].distance - 3.048).abs() < 1e-9);
        assert!((back.shots[1].left.unwrap() - 0.3048).abs() < 1e-9);
        assert!(Format::Csv.write(&mut back.clone()).contains("# unit: m"));
    }
}
