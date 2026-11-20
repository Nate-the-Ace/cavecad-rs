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
