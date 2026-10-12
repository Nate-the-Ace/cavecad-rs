//! Builds a [`Document`] from a simple shot-list JSON, so the app and tests have real caves to
//! show before the file importers exist.
//!
//! The format is what `tools/golden/gen_survey_golden.mjs` writes (the old app's parsed survey):
//! `{ "unit": "ft", "name": "...", "shots": [ { from, to, distance, azimuth, inclination, ... } ],
//! "fixed": { "A1": { x, y, z } } }`. Distances stay in the file's unit (the trip records it);
//! fixed coordinates are converted to metres. Not a file format people should use.

use cave_doc::{Action, ActorId, Document, Field, Kind, Value};
use cave_id::{Id, IdSource, SeqIds};
use serde_json::Value as Json;
use std::collections::HashMap;

const FEET_TO_METRES: f64 = 0.3048;

fn number(j: &Json) -> Value {
    Value::Number(j.as_f64())
}

/// Parses the JSON and builds a document edited by `actor`.
pub fn from_shot_list(text: &str, actor: ActorId) -> Result<Document, String> {
    let json: Json = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let mut ids = SeqIds::new(0x5eed);
    let mut doc = Document::new(actor);
    let unit = json["unit"].as_str().unwrap_or("m").to_string();

    let trip = ids.next_id();
    doc.edit(vec![
        Action::Create {
            id: trip,
            kind: Kind::Trip,
        },
        Action::Set {
            id: trip,
            field: Field::TripName,
            value: Value::Text(json["name"].as_str().unwrap_or("").into()),
        },
        Action::Set {
            id: trip,
            field: Field::TripDistanceUnit,
            value: Value::Text(unit.clone()),
        },
    ])
    .map_err(|e| format!("{e:?}"))?;

    let mut stations: HashMap<String, Id> = HashMap::new();
    let mut station = |doc: &mut Document, name: &str, ids: &mut SeqIds| -> Result<Id, String> {
        if let Some(id) = stations.get(name) {
            return Ok(*id);
        }
        let id = ids.next_id();
        doc.edit(vec![
            Action::Create {
                id,
                kind: Kind::Station,
            },
            Action::Set {
                id,
                field: Field::StationLabel,
                value: Value::Text(name.into()),
            },
        ])
        .map_err(|e| format!("{e:?}"))?;
        stations.insert(name.to_string(), id);
        Ok(id)
    };

    for (i, s) in json["shots"]
        .as_array()
        .ok_or("no shots")?
        .iter()
        .enumerate()
    {
        let id = ids.next_id();
        let mut actions = vec![
            Action::Create {
                id,
                kind: Kind::Shot,
            },
            Action::Set {
                id,
                field: Field::ShotTrip,
                value: Value::Ref(Some(trip)),
            },
            Action::Set {
                id,
                field: Field::ShotOrder,
                value: Value::Number(Some(i as f64)),
            },
        ];
        for (key, field) in [("from", Field::ShotFrom), ("to", Field::ShotTo)] {
            let name = s[key].as_str().unwrap_or("");
            if !name.is_empty() {
                let st = station(&mut doc, name, &mut ids)?;
                actions.push(Action::Set {
                    id,
                    field,
                    value: Value::Ref(Some(st)),
                });
            }
        }
        for (key, field) in [
            ("distance", Field::ShotDistance),
            ("azimuth", Field::ShotAzimuth),
            ("inclination", Field::ShotInclination),
            ("backAzimuth", Field::ShotBackAzimuth),
            ("backInclination", Field::ShotBackInclination),
            ("left", Field::ShotLeft),
            ("right", Field::ShotRight),
            ("up", Field::ShotUp),
            ("down", Field::ShotDown),
        ] {
            if s[key].is_number() {
                actions.push(Action::Set {
                    id,
                    field,
                    value: number(&s[key]),
                });
            }
        }
        for (key, field) in [
            ("splay", Field::ShotSplay),
            ("excludeFromAll", Field::ShotExcludeFromAll),
            ("excludeFromPlot", Field::ShotExcludeFromPlot),
            ("noAdjust", Field::ShotNoAdjust),
            ("azimuthOmitted", Field::ShotAzimuthOmitted),
            ("leftOpen", Field::ShotLeftOpen),
            ("rightOpen", Field::ShotRightOpen),
            ("upOpen", Field::ShotUpOpen),
            ("downOpen", Field::ShotDownOpen),
        ] {
            if s[key].as_bool() == Some(true) {
                actions.push(Action::Set {
                    id,
                    field,
                    value: Value::Flag(true),
                });
            }
        }
        doc.edit(actions).map_err(|e| format!("{e:?}"))?;
    }

    let to_m = if unit == "ft" { FEET_TO_METRES } else { 1.0 };
    if let Some(fixed) = json["fixed"].as_object() {
        for (name, p) in fixed {
            let st = station(&mut doc, name, &mut ids)?;
            let m = |k: &str| Value::Number(p[k].as_f64().map(|v| v * to_m));
            doc.edit(vec![
                Action::Set {
                    id: st,
                    field: Field::StationFixedX,
                    value: m("x"),
                },
                Action::Set {
                    id: st,
                    field: Field::StationFixedY,
                    value: m("y"),
                },
                Action::Set {
                    id: st,
                    field: Field::StationFixedZ,
                    value: m("z"),
                },
            ])
            .map_err(|e| format!("{e:?}"))?;
        }
    }
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cave_survey::{extract, resolve};

    #[test]
    fn small_survey_loads_and_places() {
        let text = r#"{"unit":"m","name":"T","shots":[
            {"from":"A","to":"B","distance":10,"azimuth":0,"inclination":0},
            {"from":"B","to":"C","distance":10,"azimuth":90,"inclination":0,"left":1.5,"rightOpen":true}]}"#;
        let doc = from_shot_list(text, ActorId(1)).unwrap();
        assert_eq!(doc.ids_of(Kind::Station).len(), 3);
        let ex = extract(&doc);
        assert_eq!(ex.legs.len(), 2);
        assert_eq!(resolve(&ex.legs, &ex.fixed).stations.len(), 3);
    }

    #[test]
    fn pitfall_cave_loads_with_its_five_loops() {
        let path = format!(
            "{}/../cave-survey/tests/golden/PitfallCave.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let doc = from_shot_list(&std::fs::read_to_string(path).unwrap(), ActorId(1)).unwrap();
        let ex = extract(&doc);
        let r = resolve(&ex.legs, &ex.fixed);
        assert_eq!(r.loops().count(), 5);
        assert!(!ex.splays.is_empty());
    }

    #[test]
    fn bad_json_is_an_error_not_a_panic() {
        assert!(from_shot_list("not json", ActorId(1)).is_err());
        assert!(from_shot_list("{}", ActorId(1)).is_err());
    }
}
