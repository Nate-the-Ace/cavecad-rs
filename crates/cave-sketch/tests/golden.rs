//! The sketch geometry must come out exactly as the web app's TypeScript did: the golden file is written by
//! `tools/golden/gen_sketch_golden.mjs` running the OLD `sketch-geometry.ts` (survey-sketch before the port).

use cave_sketch::*;
use serde_json::{Value, json};

fn norm(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, v)| (k.clone(), norm(v)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(norm).collect()),
        other => other.clone(),
    }
}

fn same(a: &Value, b: &Value, at: &str) {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            assert!(
                (x - y).abs() <= 1e-9 * (1.0 + x.abs().max(y.abs())),
                "{at}: {x} vs {y}"
            );
        }
        (Value::Object(x), Value::Object(y)) => {
            let mut keys: Vec<_> = x.keys().chain(y.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                match (x.get(k), y.get(k)) {
                    (Some(p), Some(q)) => same(p, q, &format!("{at}.{k}")),
                    (p, q) => panic!("{at}.{k}: {p:?} vs {q:?}"),
                }
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            assert_eq!(x.len(), y.len(), "{at}: length");
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                same(p, q, &format!("{at}[{i}]"));
            }
        }
        _ => assert_eq!(a, b, "{at}"),
    }
}

fn check(got: Value, want: &Value, at: &str) {
    same(&norm(&got), &norm(want), at);
}

#[test]
fn every_survey_is_laid_out_and_viewed_alike() {
    let g: Value = serde_json::from_str(include_str!("golden/sketch.json")).unwrap();
    for c in g["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let shots: Vec<SurveyShot> = serde_json::from_value(c["shots"].clone()).unwrap();
        let dims: DimsByStation = serde_json::from_value(c["dims"].clone()).unwrap();
        check(
            json!(shots.iter().map(shot_vector).collect::<Vec<_>>()),
            &c["vectors"],
            &format!("{name}: vectors"),
        );
        let l = layout(&shots);
        check(
            serde_json::to_value(&l).unwrap(),
            &c["layout"],
            &format!("{name}: layout"),
        );
        check(
            serde_json::to_value(plan_view(&l, &dims)).unwrap(),
            &c["plan"],
            &format!("{name}: plan"),
        );
        check(
            serde_json::to_value(profile_view(&l, &dims)).unwrap(),
            &c["profile"],
            &format!("{name}: profile"),
        );
        let sections = c["sections"].as_object().unwrap();
        for (label, want) in sections {
            check(
                serde_json::to_value(section_view(&l, &dims, label)).unwrap(),
                want,
                &format!("{name}: section {label}"),
            );
        }
        // Every station of the layout has its section, and a station that is not placed has none.
        assert_eq!(sections.len(), l.stations.len() + 1, "{name}");
    }
}

fn marks(v: &Value) -> Vec<Mark> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| Mark {
            label: s["label"].as_str().unwrap().into(),
            p: [s["p"][0].as_f64().unwrap(), s["p"][1].as_f64().unwrap()],
        })
        .collect()
}

fn points(v: &Value) -> Vec<Pt> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()])
        .collect()
}

#[test]
fn strokes_stay_hung_on_their_stations() {
    let g: Value = serde_json::from_str(include_str!("golden/sketch.json")).unwrap();
    for (i, c) in g["anchors"].as_array().unwrap().iter().enumerate() {
        let stations = marks(&c["stations"]);
        let pts = points(&c["pts"]);
        let widths: Option<Vec<f64>> = serde_json::from_value(c["widths"].clone()).unwrap();
        let a = anchor_stroke(&stations, &pts, "wall", widths);
        check(
            serde_json::to_value(&a).unwrap(),
            &c["anchored"],
            &format!("stroke {i}"),
        );
        let Some(a) = a else {
            continue;
        };
        check(
            json!(place_stroke(&stations, &a)),
            &c["placed"],
            &format!("stroke {i}: placed"),
        );
        let moved: Vec<Mark> = stations
            .iter()
            .map(|s| Mark {
                label: s.label.clone(),
                p: [s.p[0] + 5.0, s.p[1] - 2.0],
            })
            .collect();
        check(
            json!(place_stroke(&moved, &a)),
            &c["moved"],
            &format!("stroke {i}: moved with the station"),
        );
        let gone: Vec<Mark> = stations
            .iter()
            .filter(|s| s.label != a.anchor)
            .cloned()
            .collect();
        assert!(place_stroke(&gone, &a).is_none());
        assert!(c["gone"].is_null());
    }
}
