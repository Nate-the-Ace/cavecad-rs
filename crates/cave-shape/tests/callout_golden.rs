//! The callout geometry must match the old app's `CsCallout.reflow` on every case in
//! `golden/callout.json` (made by `tools/golden/gen_callout_golden.mjs`).

use cave_shape::callout::{NoteBox, Side, layout, layout_many, layout_with, reflow};
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

#[test]
fn a_leader_can_have_its_own_text_height_and_run_straight() {
    let auto = layout([0.0, 0.0], [90.0, 0.0], 5, false);
    assert!(
        (auto.height - 30.0).abs() < 1e-9,
        "by default a third of the drag"
    );
    let fixed = layout_with([0.0, 0.0], [90.0, 0.0], 5, false, Some(12.0), false);
    assert_eq!(fixed.height, 12.0, "or what the leader says");
    assert_eq!(fixed.leader.len(), 3, "tip, elbow, landing");
    let straight = layout_with([0.0, 0.0], [90.0, 40.0], 5, false, Some(12.0), true);
    assert_eq!(straight.leader.len(), 2, "straight has no elbow");
    assert_eq!(straight.leader[0], [0.0, 0.0]);
    assert_eq!(
        straight.leader[1][0], straight.note[0],
        "and ends on the note's edge"
    );
    let curved = layout_with([0.0, 0.0], [90.0, 40.0], 5, true, Some(12.0), false);
    assert!(curved.leader.len() > 3, "a curved one is traced in pieces");
    assert_eq!(
        layout([0.0, 0.0], [90.0, 40.0], 5, false),
        layout_with([0.0, 0.0], [90.0, 40.0], 5, false, None, false),
        "the plain layout is unchanged"
    );
}

#[test]
fn a_leader_can_have_several_arrows() {
    let tips = [[0.0, 0.0], [10.0, 60.0], [-5.0, 120.0]];
    let m = layout_many(&tips, [200.0, 60.0], 8, false, Some(12.0), false);
    assert_eq!(m.branches.len(), 3);
    assert_eq!(m.heads.len(), 3);
    assert_eq!(m.height, 12.0);
    let landing = *m.branches[0].last().unwrap();
    for (tip, branch) in tips.iter().zip(&m.branches) {
        assert_eq!(branch[0], *tip, "each leader starts at its own tip");
        assert_eq!(
            *branch.last().unwrap(),
            landing,
            "and they all meet the note at one point"
        );
    }
    assert!(
        m.heads
            .iter()
            .zip(&tips)
            .all(|(h, t)| h.len() == 3 && h[1] == *t),
        "an arrow head at every tip"
    );
    // one arrow is exactly the plain layout
    let one = layout_many(&[[0.0, 0.0]], [90.0, 40.0], 5, false, Some(12.0), false);
    let old = layout_with([0.0, 0.0], [90.0, 40.0], 5, false, Some(12.0), false);
    assert_eq!(one.branches[0], old.leader);
    assert_eq!(one.heads[0], old.head);
    assert_eq!(one.note, old.note);
    let straight = layout_many(&tips, [200.0, 60.0], 8, false, Some(12.0), true);
    assert!(
        straight.branches.iter().all(|b| b.len() == 2),
        "straight leaders have no elbow"
    );
    assert!(
        layout_many(&[], [0.0, 0.0], 3, false, None, false)
            .branches
            .is_empty(),
        "no arrows, no leaders"
    );
}
