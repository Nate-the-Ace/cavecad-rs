//! The callout geometry must match the old app's `CsCallout.reflow` on every case in
//! `golden/callout.json` (made by `tools/golden/gen_callout_golden.mjs`).

use cave_shape::callout::{NoteBox, Side, reflow};
use serde_json::Value;

fn pt(v: &Value) -> [f64; 2] {
    [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()]
}

#[test]
fn matches_the_old_reflow() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("golden/callout.json")).unwrap();
    assert!(cases.len() > 50);
    for c in &cases {
        let b = &c["box"];
        let b = NoteBox {
            x1: b["x1"].as_f64().unwrap(),
            y1: b["y1"].as_f64().unwrap(),
            x2: b["x2"].as_f64().unwrap(),
            y2: b["y2"].as_f64().unwrap(),
        };
        let tips: Vec<[f64; 2]> = c["tips"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| [t["x"].as_f64().unwrap(), t["y"].as_f64().unwrap()])
            .collect();
        let side = match c["side"].as_str().unwrap() {
            "left" => Some(Side::Left),
            "right" => Some(Side::Right),
            _ => None,
        };
        let got = reflow(b, &tips, side, c["curved"].as_bool().unwrap());
        let want = &c["out"];
        assert_eq!(
            match got.side {
                Side::Left => "left",
                Side::Right => "right",
            },
            want["side"].as_str().unwrap()
        );
        let l = pt(&want["landing"]);
        assert!((got.landing[0] - l[0]).abs() < 1e-9 && (got.landing[1] - l[1]).abs() < 1e-9);
        let wb = want["branches"].as_array().unwrap();
        assert_eq!(got.branches.len(), wb.len());
        for (g, w) in got.branches.iter().zip(wb) {
            let w = w.as_array().unwrap();
            assert_eq!(g.len(), w.len(), "{c}");
            for (a, b) in g.iter().zip(w) {
                let b = pt(b);
                assert!(
                    (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9,
                    "{c}"
                );
            }
        }
    }
}

#[test]
fn note_grows_away_from_the_arrow() {
    let l = cave_shape::callout::note_box([10.0, 5.0], 10, 2.0, Side::Left);
    assert!(l.x1 == 10.0 && l.x2 > 10.0);
    let r = cave_shape::callout::note_box([10.0, 5.0], 10, 2.0, Side::Right);
    assert!(r.x2 == 10.0 && r.x1 < 10.0);
}
