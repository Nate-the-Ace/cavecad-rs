//! The Rust survey math must agree with the old CaveCAD (JavaScript) math on its own test caves.
//! The answer files in `tests/golden/` were produced by running the old code;
//! see `tools/golden/gen_survey_golden.mjs` to refresh them.
//!
//! Units are whatever the file used (feet for Compass): the math is scale-free, so nothing is
//! converted and the instrument sigmas are the old ones in the same units.

use cave_id::{Id, IdSource, SeqIds};
use cave_survey::{Fixed, Leg, Params, Reading, TapeMode, Vec3, adjust, offset, resolve};
use serde_json::Value;
use std::collections::HashMap;

fn num(v: &Value) -> Option<f64> {
    v.as_f64()
}

struct Case {
    legs: Vec<Leg>,
    fixed: Vec<Fixed>,
    names: HashMap<String, Id>,
    expected: Value,
}

fn load(file: &str) -> Case {
    let path = format!("{}/tests/golden/{file}", env!("CARGO_MANIFEST_DIR"));
    let expected: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut ids = SeqIds::new(3);
    let mut names: HashMap<String, Id> = HashMap::new();
    let mut id =
        |n: &str, ids: &mut SeqIds| *names.entry(n.to_string()).or_insert_with(|| ids.next_id());
    let mut legs = Vec::new();
    for s in expected["shots"].as_array().unwrap() {
        let (from, to) = (
            s["from"].as_str().unwrap_or(""),
            s["to"].as_str().unwrap_or(""),
        );
        if from.is_empty()
            || to.is_empty()
            || s["splay"].as_bool() == Some(true)
            || s["excludeFromAll"].as_bool() == Some(true)
        {
            continue;
        }
        let r = Reading {
            distance: num(&s["distance"]).unwrap_or(f64::NAN),
            azimuth: num(&s["azimuth"]),
            inclination: num(&s["inclination"]),
            back_azimuth: num(&s["backAzimuth"]),
            back_inclination: num(&s["backInclination"]),
            azimuth_omitted: s["azimuthOmitted"].as_bool() == Some(true),
        };
        let Some(o) = offset(&r, TapeMode::Slope) else {
            continue;
        };
        legs.push(Leg {
            shot: ids.next_id(),
            from: id(from, &mut ids),
            to: id(to, &mut ids),
            offset: o.v,
            distance: r.distance,
            no_adjust: s["noAdjust"].as_bool() == Some(true),
            azimuth_omitted: r.azimuth_omitted,
        });
    }
    let mut fixed = Vec::new();
    if let Some(f) = expected["fixed"].as_object() {
        for (name, p) in f {
            let s = id(name, &mut ids);
            fixed.push(Fixed {
                station: s,
                pos: Vec3::new(
                    p["x"].as_f64().unwrap(),
                    p["y"].as_f64().unwrap(),
                    p["z"].as_f64().unwrap(),
                ),
            });
        }
    }
    Case {
        legs,
        fixed,
        names,
        expected,
    }
}

fn check(file: &str) {
    let c = load(file);
    let r = resolve(&c.legs, &c.fixed);
    let exp = &c.expected["resolved"];

    let expected_stations = exp["stations"].as_object().unwrap();
    assert_eq!(
        r.stations.len(),
        expected_stations.len(),
        "{file}: station count"
    );
    for (name, p) in expected_stations {
        let got = r.stations[&c.names[name]].pos;
        for (axis, (g, e)) in [got.x, got.y, got.z]
            .iter()
            .zip(p.as_array().unwrap())
            .enumerate()
        {
            assert!(
                (g - e.as_f64().unwrap()).abs() < 1e-6,
                "{file}: {name} axis {axis}: {g} vs {e}"
            );
        }
    }

    let sorted = |mut v: Vec<f64>| {
        v.sort_by(|a, b| a.total_cmp(b));
        v
    };
    let want_loops = sorted(
        exp["loops"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["error"].as_f64().unwrap())
            .collect(),
    );
    let got_loops = sorted(r.loops().map(|l| l.distance).collect());
    assert_eq!(got_loops.len(), want_loops.len(), "{file}: loop count");
    for (g, w) in got_loops.iter().zip(&want_loops) {
        assert!((g - w).abs() < 1e-6, "{file}: loop error {g} vs {w}");
    }
    let want_pct = sorted(
        exp["loops"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|l| l["percent"].as_f64())
            .collect(),
    );
    let got_pct = sorted(r.loops().filter_map(|l| l.percent).collect());
    assert_eq!(got_pct.len(), want_pct.len(), "{file}: loop percent count");
    for (g, w) in got_pct.iter().zip(&want_pct) {
        assert!((g - w).abs() < 1e-6, "{file}: loop percent {g} vs {w}");
    }
    assert_eq!(
        r.ties().count(),
        exp["ties"].as_array().unwrap().len(),
        "{file}: tie count"
    );

    let a = adjust(
        &c.legs,
        &r,
        &Params {
            sigma_tape: 0.1,
            sigma_angle_deg: 1.5,
            max_iterations: None,
            cg_tolerance: None,
        },
    );
    let adj = &c.expected["adjusted"];
    assert_eq!(
        a.adjusted,
        adj["adjusted"].as_bool().unwrap(),
        "{file}: adjusted flag"
    );
    for (name, p) in adj["stations"].as_object().unwrap() {
        let got = a.stations[&c.names[name]];
        for (axis, (g, e)) in [got.x, got.y, got.z]
            .iter()
            .zip(p.as_array().unwrap())
            .enumerate()
        {
            assert!(
                (g - e.as_f64().unwrap()).abs() < 1e-3,
                "{file}: adjusted {name} axis {axis}: {g} vs {e}"
            );
        }
    }
}

#[test]
fn pitfall_cave_matches_the_old_math() {
    check("PitfallCave.json");
}

#[test]
fn fingerprint_cave_matches_the_old_math() {
    check("FingerprintCave.json");
}

#[test]
fn plumbline_pit_matches_the_old_math() {
    check("PlumblinePit.json");
}

#[test]
fn stairstep_cave_matches_the_old_math() {
    check("StairstepCave.json");
}
