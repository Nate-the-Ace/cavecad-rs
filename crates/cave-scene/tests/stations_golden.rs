//! The station table must say what the old app's said for the same caves: the same stations, the same
//! kinds of place (lead, open end, junction, control, loop, noted), trips, degrees, notes and elevations.

use cave_doc::{Action, ActorId, Document, Field, Kind};
use cave_scene::station_rows;
use serde_json::Value;

/// The shot lists in the fixtures carry no notes; the old app's are put on the shots, in survey order.
fn put_notes(doc: &mut Document, old: &Value) {
    let mut shots: Vec<(f64, cave_id::Id)> = doc
        .ids_of(Kind::Shot)
        .into_iter()
        .map(|id| {
            (
                doc.entity(id)
                    .unwrap()
                    .number(Field::ShotOrder)
                    .unwrap_or(f64::MAX),
                id,
            )
        })
        .collect();
    shots.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let notes = old["shotNotes"].as_array().unwrap();
    assert_eq!(shots.len(), notes.len(), "the same shots");
    for ((_, id), note) in shots.iter().zip(notes) {
        let text = note.as_str().unwrap();
        if !text.is_empty() {
            doc.edit(vec![Action::Set {
                id: *id,
                field: Field::ShotNote,
                value: cave_doc::Value::Text(text.into()),
            }])
            .unwrap();
        }
    }
}

#[test]
fn the_station_table_matches_the_old_app() {
    let want: Value = serde_json::from_str(include_str!("golden/stations.json")).unwrap();
    for (cave, file) in [
        ("PitfallCave", "PitfallCave.json"),
        ("FingerprintCave", "FingerprintCave.json"),
        ("PlumblinePit", "PlumblinePit.json"),
        ("StairstepCave", "StairstepCave.json"),
    ] {
        let path = format!(
            "{}/../cave-survey/tests/golden/{file}",
            env!("CARGO_MANIFEST_DIR")
        );
        let mut doc =
            cave_fixture::from_shot_list(&std::fs::read_to_string(path).unwrap(), ActorId(1))
                .unwrap();
        put_notes(&mut doc, &want[cave]);
        let rows = station_rows(&doc, false, "");
        let old = want[cave]["rows"].as_array().unwrap();
        // the old app resolves in the survey's own unit; ours is metres
        let unit = if want[cave]["unit"] == "ft" {
            0.3048
        } else {
            1.0
        };
        assert_eq!(rows.len(), old.len(), "{cave}: how many stations");
        for (r, o) in rows.iter().zip(old) {
            let label = format!("{cave} {}", r.station);
            assert_eq!(r.station, o["station"].as_str().unwrap(), "{label}: order");
            let kinds: Vec<&str> = o["kinds"]
                .as_array()
                .unwrap()
                .iter()
                .map(|k| k.as_str().unwrap())
                .collect();
            assert_eq!(r.kinds, kinds, "{label}: kinds");
            let trips: Vec<usize> = o["trips"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t.as_u64().unwrap() as usize)
                .collect();
            assert_eq!(r.trips, trips, "{label}: trips");
            assert_eq!(
                r.degree as u64,
                o["degree"].as_u64().unwrap(),
                "{label}: degree"
            );
            assert_eq!(
                r.note_text,
                o["noteText"].as_str().unwrap(),
                "{label}: notes"
            );
            assert_eq!(
                r.loops.len() as u64,
                o["loops"].as_u64().unwrap(),
                "{label}: loops"
            );
            match o["z"].as_f64() {
                Some(z) => assert!(
                    (r.z.unwrap() - z * unit).abs() < 1e-6,
                    "{label}: elevation {:?} vs {z}",
                    r.z
                ),
                None => assert!(r.z.is_none(), "{label}: elevation"),
            }
        }
    }
}

#[test]
fn rows_never_carry_a_position() {
    let path = format!(
        "{}/../cave-survey/tests/golden/PitfallCave.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let doc =
        cave_fixture::from_shot_list(&std::fs::read_to_string(path).unwrap(), ActorId(1)).unwrap();
    let json = serde_json::to_string(&station_rows(&doc, true, "")).unwrap();
    for key in ["\"x\"", "\"y\"", "\"east", "\"north", "\"pos", "\"at\""] {
        assert!(!json.contains(key), "a row mentions {key}");
    }
}
