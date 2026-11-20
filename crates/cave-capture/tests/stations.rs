//! Station listing tests.

use cave_capture::stations::{document_from_room, stations};
use cave_doc::ActorId;

fn fixture_ops() -> String {
    let doc = cave_fixture::from_shot_list(
        r#"{"unit":"m","name":"T","shots":[
            {"from":"A","to":"B","distance":10,"azimuth":0,"inclination":0},
            {"from":"B","to":"C","distance":10,"azimuth":90,"inclination":0}]}"#,
        ActorId(1),
    )
    .unwrap();
    serde_json::to_string(&doc.snapshot()).unwrap()
}

#[test]
fn snapshot_gives_stations_in_order() {
    let snap = format!(r#"{{"upTo":4,"ops":{}}}"#, fixture_ops());
    let doc = document_from_room(Some(&snap), &[]).unwrap();
    let s = stations(&doc);
    let labels: Vec<_> = s.iter().map(|x| x.label.as_str()).collect();
    assert_eq!(labels, ["A", "B", "C"]);
    let c = s[2].pos;
    assert!(
        (c.x - 10.0).abs() < 1e-6 && (c.y - 10.0).abs() < 1e-6 && c.z.abs() < 1e-6,
        "{c:?}"
    );
}

#[test]
fn entries_apply_on_top() {
    let entry = format!(r#"{{"c":"dev1","ops":{}}}"#, fixture_ops());
    let doc = document_from_room(None, &[entry.as_str()]).unwrap();
    assert_eq!(stations(&doc).len(), 3);
}

#[test]
fn unreadable_entry_named() {
    let err = document_from_room(None, &["{not json"]).unwrap_err();
    assert!(err.contains("entry 1"), "{err}");
}

const LOOP: &str = r#"{"unit":"m","name":"L","shots":[
    {"from":"A","to":"B","distance":10,"azimuth":0,"inclination":0},
    {"from":"B","to":"C","distance":10,"azimuth":90,"inclination":0},
    {"from":"C","to":"D","distance":10,"azimuth":180,"inclination":0},
    {"from":"D","to":"A","distance":11,"azimuth":270,"inclination":0}]}"#;

#[test]
fn stations_are_the_adjusted_positions() {
    let doc = cave_fixture::from_shot_list(LOOP, ActorId(1)).unwrap();
    let ex = cave_survey::extract(&doc);
    let raw = cave_survey::resolve(&ex.legs, &ex.fixed);
    let adj = cave_survey::adjust(&ex.legs, &raw, &cave_survey::Params::default());
    assert!(adj.adjusted, "loop should adjust");
    let s = stations(&doc);
    assert_eq!(s.len(), 4);
    let mut moved = false;
    for st in &s {
        assert_eq!(st.pos, adj.stations[&st.id]);
        moved |= st.pos != raw.stations[&st.id].pos;
    }
    assert!(moved, "adjustment must differ from raw resolve");
}

#[test]
fn entry_order_and_duplicates_converge() {
    let a = format!(r#"{{"c":"d1","ops":{}}}"#, fixture_ops());
    let doc1 = document_from_room(None, &[a.as_str()]).unwrap();
    let doc2 = document_from_room(None, &[a.as_str(), a.as_str()]).unwrap();
    let ops: Vec<serde_json::Value> = serde_json::from_str(&fixture_ops()).unwrap();
    let mut rev = ops.clone();
    rev.reverse();
    let ea = format!(
        r#"{{"c":"d1","ops":{}}}"#,
        serde_json::to_string(&ops[..ops.len() / 2]).unwrap()
    );
    let eb = format!(
        r#"{{"c":"d2","ops":{}}}"#,
        serde_json::to_string(&ops[ops.len() / 2..]).unwrap()
    );
    let fwd = document_from_room(None, &[ea.as_str(), eb.as_str()]).unwrap();
    let bwd = document_from_room(None, &[eb.as_str(), ea.as_str()]).unwrap();
    let r = format!(
        r#"{{"c":"d3","ops":{}}}"#,
        serde_json::to_string(&rev).unwrap()
    );
    let reversed = document_from_room(None, &[r.as_str()]).unwrap();
    let base = stations(&doc1);
    assert_eq!(base.len(), 3);
    for d in [&doc2, &fwd, &bwd, &reversed] {
        assert_eq!(stations(d), base);
    }
}

/// The CaveCAD client stores ops as one JSON text inside the snapshot and entries (found on the
/// phone, 2026-11-20: every real cave failed to open). Both shapes must read the same.
#[test]
fn ops_as_json_text_like_the_server() {
    let ops = fixture_ops();
    let snap = serde_json::json!({ "upTo": 4, "ops": ops }).to_string();
    let entry = serde_json::json!({ "c": "dev1", "ops": ops }).to_string();
    assert_eq!(
        stations(&document_from_room(Some(&snap), &[]).unwrap()).len(),
        3
    );
    assert_eq!(
        stations(&document_from_room(None, &[entry.as_str()]).unwrap()).len(),
        3
    );
}

#[test]
fn errors_do_not_echo_cave_data() {
    let snap = serde_json::json!({ "upTo": 1, "ops": "[{\"secret\":\"Truitt\"}]" }).to_string();
    let err = document_from_room(Some(&snap), &[]).unwrap_err();
    assert!(!err.contains("Truitt"), "{err}");
}
