//! The Rust walls must match the old CaveCAD (JavaScript) walls on its own test caves.
//! Expected outlines come from `tools/golden/gen_survey_golden.mjs` (plan x, y in feet, stations by
//! name); the survey goes through the whole pipeline: document, extract, resolve, walls.

use cave_doc::{ActorId, Field};
use cave_survey::{extract, resolve};
use cave_walls::{Run, WallInput, wall_runs};
use serde_json::Value;
use std::collections::BTreeMap;

const FEET: f64 = 0.3048;

fn check(file: &str) {
    let dir = format!(
        "{}/../cave-survey/tests/golden/{file}",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(dir).unwrap();
    let expected: Value = serde_json::from_str(&text).unwrap();
    let doc = cave_fixture::from_shot_list(&text, ActorId(1)).unwrap();
    let ex = extract(&doc);
    let r = resolve(&ex.legs, &ex.fixed);
    let stations: BTreeMap<_, _> = r.stations.iter().map(|(id, p)| (*id, p.pos)).collect();
    let walls = wall_runs(&WallInput {
        stations: &stations,
        legs: &r.legs,
        readings: &ex.readings,
        lruds: &ex.last_arrival_lruds,
        start_lruds: &ex.start_lruds,
        splays: &ex.splays,
    });
    let label = |id| {
        doc.entity(id)
            .map(|e| e.text(Field::StationLabel))
            .unwrap_or_default()
    };

    for (side, got, want) in [
        ("left", &walls.left, &expected["walls"]["left"]),
        ("right", &walls.right, &expected["walls"]["right"]),
    ] {
        let want = want.as_array().unwrap();
        assert_eq!(got.len(), want.len(), "{file} {side}: run count");
        for (i, (g, w)) in got.iter().zip(want).enumerate() {
            compare(file, side, i, g, w, &label);
        }
    }
}

fn compare(
    file: &str,
    side: &str,
    i: usize,
    got: &Run,
    want: &Value,
    label: &dyn Fn(cave_id::Id) -> String,
) {
    let pts = want["points"].as_array().unwrap();
    assert_eq!(
        got.points.len(),
        pts.len(),
        "{file} {side} run {i}: point count"
    );
    for (k, (g, w)) in got.points.iter().zip(pts).enumerate() {
        let (wx, wy) = (w[0].as_f64().unwrap() * FEET, w[1].as_f64().unwrap() * FEET);
        assert!(
            (g.x - wx).abs() < 1e-5 && (g.y - wy).abs() < 1e-5,
            "{file} {side} run {i} point {k}: ({:.4},{:.4}) vs ({wx:.4},{wy:.4})",
            g.x,
            g.y
        );
    }
    let names: Vec<String> = got.stations.iter().map(|s| label(*s)).collect();
    let want_names: Vec<String> = want["stations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect();
    assert_eq!(names, want_names, "{file} {side} run {i}: stations");
}

#[test]
fn pitfall_walls_match_the_old_walls() {
    check("PitfallCave.json");
}

#[test]
fn fingerprint_walls_match_the_old_walls() {
    check("FingerprintCave.json");
}

#[test]
fn plumbline_pit_walls_match_the_old_walls() {
    check("PlumblinePit.json");
}

#[test]
fn stairstep_walls_match_the_old_walls() {
    check("StairstepCave.json");
}

/// A loop closing on a junction arrives from the other side, facing the other way: the station's
/// walls must keep the first arrival's bearing (the shot the station panel edits), not swap sides.
#[test]
fn a_junction_keeps_the_walls_of_its_first_arrival() {
    let dir = format!(
        "{}/../cave-survey/tests/golden/FingerprintCave.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(dir).unwrap();
    let doc = cave_fixture::from_shot_list(&text, ActorId(1)).unwrap();
    let ex = extract(&doc);
    let a6 = ex
        .lruds
        .iter()
        .find(|(id, _)| {
            doc.entity(**id)
                .is_some_and(|e| e.text(Field::StationLabel) == "A6")
        })
        .map(|(_, l)| l)
        .unwrap();
    assert_eq!(
        a6.azimuth,
        Some(77.5),
        "faces A5 to A6, not U4 to A6 (311.5)"
    );
}
