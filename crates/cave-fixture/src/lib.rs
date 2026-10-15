//! Builds a [`Document`] from a simple shot-list JSON, so the app and tests have real caves to
//! show before the file importers exist.
//!
//! The format is what `tools/golden/gen_survey_golden.mjs` writes (the old app's parsed survey):
//! `{ "unit": "ft", "name": "...", "shots": [ { from, to, distance, azimuth, inclination, ... } ],
//! "fixed": { "A1": { x, y, z } } }`. Distances stay in the file's unit (the trip records it);
//! fixed coordinates are converted to metres. Not a file format people should use: the document is
//! built by `cave_formats::to_document`, the same code that imports real survey files.

use cave_doc::{ActorId, Document};
use cave_formats::Survey;
use serde_json::Value as Json;

/// Parses the JSON and builds a document edited by `actor` (through the same builder Import uses).
pub fn from_shot_list(text: &str, actor: ActorId) -> Result<Document, String> {
    let json: Json = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if !json["shots"].is_array() {
        return Err("no shots".into());
    }
    let mut survey: Survey = serde_json::from_value(json.clone()).map_err(|e| e.to_string())?;
    survey.distance_unit = Some(json["unit"].as_str().unwrap_or("m").to_string());
    let name = json["name"].as_str().unwrap_or("");
    match json["trips"].as_array().filter(|t| !t.is_empty()) {
        Some(trips) => {
            // a trip with no name of its own takes the file's, and every trip the file's unit
            for (t, j) in survey.trips.iter_mut().zip(trips) {
                if !j["name"].is_string() {
                    t.name = name.to_string();
                }
                t.distance_unit = survey.distance_unit.clone();
            }
        }
        None => {
            // one trip, carrying only the file's name
            survey.trips = vec![cave_formats::Trip {
                name: name.to_string(),
                distance_unit: survey.distance_unit.clone(),
                ..cave_formats::Trip::default()
            }];
            survey.start_lrud = None;
        }
    }
    cave_formats::to_document(&survey, actor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cave_doc::Kind;
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
