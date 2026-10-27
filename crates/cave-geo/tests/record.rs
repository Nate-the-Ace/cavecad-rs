//! Stored location, surface, contour and fixed-frame records, and the location candidate.

use cave_doc::{Action, ActorId, Document, Field, Kind, Value};
use cave_geo::datum::{Datum, FixedFrame};
use cave_geo::record::*;
use cave_id::Id;

fn station(doc: &mut Document, label: &str) -> Id {
    let id = Id::from_name(&format!("test-station:{label}"));
    doc.edit(vec![
        Action::Create {
            id,
            kind: Kind::Station,
        },
        Action::Set {
            id,
            field: Field::StationLabel,
            value: Value::Text(label.into()),
        },
    ])
    .unwrap();
    id
}

#[test]
fn ids_match_cave_draw() {
    for k in [LOCATION, SURFACE, CONTOUR_INTERVAL, FIXED_FRAME] {
        assert_eq!(setting_id(k), cave_draw::setting_id(k));
    }
}

#[test]
fn location_round_trip() {
    let mut doc = Document::new(ActorId(1));
    let a1 = station(&mut doc, "A1");
    let loc = Location {
        station: a1,
        lat: 36.99,
        lon: -86.44,
        ground_m: None,
        source: "typed".into(),
    };
    doc.edit(set_location(&doc, &loc)).unwrap();
    assert_eq!(location(&doc), Some(loc.clone()));
    let again = Location {
        ground_m: Some(160.25),
        ..loc
    };
    let acts = set_location(&doc, &again);
    assert_eq!(acts.len(), 1);
    doc.edit(acts).unwrap();
    assert_eq!(location(&doc).unwrap().ground_m, Some(160.25));
    let j = serde_json::to_string(&Location {
        ground_m: None,
        ..again
    })
    .unwrap();
    assert!(!j.contains("ground_m"), "{j}");
}

#[test]
fn candidate_order() {
    let mut doc = Document::new(ActorId(1));
    let a1 = station(&mut doc, "A1");
    assert_eq!(candidate(&doc), Candidate::Ask { station: Some(a1) });
    let fx = station(&mut doc, "FIX");
    doc.edit(vec![
        Action::Set {
            id: fx,
            field: Field::StationFixedX,
            value: Value::Number(Some(550_000.0)),
        },
        Action::Set {
            id: fx,
            field: Field::StationFixedY,
            value: Value::Number(Some(4_094_000.0)),
        },
    ])
    .unwrap();
    // fixed coordinates without a frame are not a location
    assert_eq!(candidate(&doc), Candidate::Ask { station: Some(a1) });
    doc.edit(set_fixed_frame(
        &doc,
        FixedFrame {
            zone: 16,
            datum: Datum::Nad83,
        },
    ))
    .unwrap();
    let want = FixedFrame {
        zone: 16,
        datum: Datum::Nad83,
    }
    .to_latlon(550_000.0, 4_094_000.0);
    assert_eq!(
        candidate(&doc),
        Candidate::Imported {
            station: fx,
            at: want
        }
    );
    let loc = Location {
        station: a1,
        lat: 1.0,
        lon: 2.0,
        ground_m: None,
        source: "map".into(),
    };
    doc.edit(set_location(&doc, &loc)).unwrap();
    assert_eq!(candidate(&doc), Candidate::Stored(loc));
}

#[test]
fn local_fixes_are_not_a_location() {
    let mut doc = Document::new(ActorId(1));
    let a1 = station(&mut doc, "A1");
    doc.edit(vec![
        Action::Set {
            id: a1,
            field: Field::StationFixedX,
            value: Value::Number(Some(0.0)),
        },
        Action::Set {
            id: a1,
            field: Field::StationFixedY,
            value: Value::Number(Some(0.0)),
        },
    ])
    .unwrap();
    doc.edit(set_fixed_frame(
        &doc,
        FixedFrame {
            zone: 16,
            datum: Datum::Nad83,
        },
    ))
    .unwrap();
    assert_eq!(candidate(&doc), Candidate::Ask { station: Some(a1) });
}

fn join(from: &Document, actor: u64) -> Document {
    let mut d = Document::new(ActorId(actor));
    for op in from.snapshot() {
        d.apply(&op).unwrap();
    }
    d.take_outbox();
    d
}

fn exchange(a: &mut Document, b: &mut Document) {
    let (from_a, from_b) = (a.take_outbox(), b.take_outbox());
    for op in &from_a {
        b.apply(op).unwrap();
    }
    for op in &from_b {
        a.apply(op).unwrap();
    }
}

#[test]
fn two_people_setting_the_location_converge_on_one_whole_record() {
    for existing in [false, true] {
        let mut a = Document::new(ActorId(1));
        let a1 = station(&mut a, "A1");
        if existing {
            let l0 = Location {
                station: a1,
                lat: 10.0,
                lon: 20.0,
                ground_m: None,
                source: "typed".into(),
            };
            a.edit(set_location(&a, &l0)).unwrap();
        }
        let mut b = join(&a, 2);
        a.take_outbox();
        let l1 = Location {
            station: a1,
            lat: 36.1,
            lon: -86.1,
            ground_m: Some(100.0),
            source: "map".into(),
        };
        let l2 = Location {
            station: a1,
            lat: 37.2,
            lon: -87.2,
            ground_m: None,
            source: "typed".into(),
        };
        a.edit(set_location(&a, &l1)).unwrap();
        b.edit(set_location(&b, &l2)).unwrap();
        exchange(&mut a, &mut b);
        assert_eq!(a, b);
        let got = location(&a).unwrap();
        assert_eq!(location(&b), Some(got.clone()));
        assert!(
            got == l1 || got == l2,
            "mixed record {got:?} (existing={existing})"
        );
    }
}
