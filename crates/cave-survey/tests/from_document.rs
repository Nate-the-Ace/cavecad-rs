//! End to end: build a cave in a document, then place and adjust it.

use cave_doc::{Action, ActorId, Document, Field, Kind, Value};
use cave_id::{Id, IdSource, SeqIds};
use cave_survey::{ClosureKind, Params, SkipReason, adjust, extract, resolve};

struct Cave {
    doc: Document,
    ids: SeqIds,
    trip: Id,
    order: f64,
}

impl Cave {
    fn new(unit: &str) -> Self {
        let mut ids = SeqIds::new(9);
        let mut doc = Document::new(ActorId(1));
        let trip = ids.next_id();
        doc.edit(vec![
            Action::Create {
                id: trip,
                kind: Kind::Trip,
            },
            Action::Set {
                id: trip,
                field: Field::TripDistanceUnit,
                value: Value::Text(unit.into()),
            },
        ])
        .unwrap();
        Cave {
            doc,
            ids,
            trip,
            order: 0.0,
        }
    }

    fn station(&mut self, label: &str) -> Id {
        let id = self.ids.next_id();
        self.doc
            .edit(vec![
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

    fn shot(&mut self, from: Id, to: Id, d: f64, az: f64, inc: f64) -> Id {
        self.order += 1.0;
        let id = self.ids.next_id();
        let n = |v| Value::Number(Some(v));
        self.doc
            .edit(vec![
                Action::Create {
                    id,
                    kind: Kind::Shot,
                },
                Action::Set {
                    id,
                    field: Field::ShotTrip,
                    value: Value::Ref(Some(self.trip)),
                },
                Action::Set {
                    id,
                    field: Field::ShotFrom,
                    value: Value::Ref(Some(from)),
                },
                Action::Set {
                    id,
                    field: Field::ShotTo,
                    value: Value::Ref(Some(to)),
                },
                Action::Set {
                    id,
                    field: Field::ShotDistance,
                    value: n(d),
                },
                Action::Set {
                    id,
                    field: Field::ShotAzimuth,
                    value: n(az),
                },
                Action::Set {
                    id,
                    field: Field::ShotInclination,
                    value: n(inc),
                },
                Action::Set {
                    id,
                    field: Field::ShotOrder,
                    value: n(self.order),
                },
            ])
            .unwrap();
        id
    }
}

#[test]
fn a_square_in_a_document_closes_and_adjusts() {
    let mut c = Cave::new("m");
    let (a1, a2, a3, a4) = (
        c.station("A1"),
        c.station("A2"),
        c.station("A3"),
        c.station("A4"),
    );
    c.shot(a4, a1, 10.5, 270.0, 0.0);
    c.shot(a1, a2, 10.0, 0.0, 0.0);
    c.shot(a2, a3, 10.0, 90.0, 0.0);
    c.shot(a3, a4, 10.0, 180.0, 0.0);

    let ex = extract(&c.doc);
    assert_eq!(ex.legs.len(), 4);
    let r = resolve(&ex.legs, &ex.fixed);
    let l = r.loops().next().expect("one loop");
    assert!((l.distance - 0.5).abs() < 1e-9);
    assert_eq!(l.kind, ClosureKind::Loop);

    let adj = adjust(&ex.legs, &r, &Params::default());
    assert!(adj.adjusted);
    let moved = adj.shifts.values().map(|s| s.distance).fold(0.0, f64::max);
    assert!(moved > 0.0 && moved < 0.5);
}

#[test]
fn feet_are_converted_to_metres() {
    let mut c = Cave::new("ft");
    let (a, b) = (c.station("A"), c.station("B"));
    c.shot(a, b, 100.0, 0.0, 0.0);
    let ex = extract(&c.doc);
    assert!((ex.legs[0].distance - 30.48).abs() < 1e-9);
    assert!((ex.legs[0].offset.y - 30.48).abs() < 1e-9);
}

#[test]
fn splays_and_unusable_shots_are_reported_not_hidden() {
    let mut c = Cave::new("m");
    let (a, b) = (c.station("A"), c.station("B"));
    let splay = c.shot(a, b, 3.0, 0.0, 0.0);
    c.doc
        .edit(vec![Action::Set {
            id: splay,
            field: Field::ShotSplay,
            value: Value::Flag(true),
        }])
        .unwrap();
    let broken = c.shot(a, b, 3.0, 0.0, 0.0);
    c.doc
        .edit(vec![Action::Set {
            id: broken,
            field: Field::ShotAzimuth,
            value: Value::Number(None),
        }])
        .unwrap();
    let ex = extract(&c.doc);
    assert!(ex.legs.is_empty());
    let reasons: Vec<_> = ex.skipped.iter().map(|s| s.reason).collect();
    assert!(reasons.contains(&SkipReason::Splay) && reasons.contains(&SkipReason::Unusable));
}

#[test]
fn fixed_stations_become_anchors_and_survey_order_is_honoured() {
    let mut c = Cave::new("m");
    let (a, b) = (c.station("A"), c.station("B"));
    let n = |v| Value::Number(Some(v));
    c.doc
        .edit(vec![
            Action::Set {
                id: b,
                field: Field::StationFixedX,
                value: n(500.0),
            },
            Action::Set {
                id: b,
                field: Field::StationFixedY,
                value: n(600.0),
            },
            Action::Set {
                id: b,
                field: Field::StationFixedZ,
                value: n(50.0),
            },
        ])
        .unwrap();
    c.shot(a, b, 10.0, 0.0, 0.0);
    let ex = extract(&c.doc);
    assert_eq!(ex.fixed.len(), 1);
    let r = resolve(&ex.legs, &ex.fixed);
    assert!(
        (r.stations[&a].pos.y - 590.0).abs() < 1e-9,
        "A is placed backwards from the fixed B"
    );
}
