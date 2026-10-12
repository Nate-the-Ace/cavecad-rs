//! Two devices editing one cave: join by snapshot, exchange what each did, end up identical.

use cave_doc::{Action, ActorId, Document, Field, History, Kind, Value};
use cave_id::{IdSource, SeqIds};

fn label(id: cave_id::Id, text: &str) -> Action {
    Action::Set {
        id,
        field: Field::StationLabel,
        value: Value::Text(text.into()),
    }
}

fn join(from: &Document, actor: u64) -> Document {
    let mut d = Document::new(ActorId(actor));
    for op in from.snapshot() {
        d.apply(&op).unwrap();
    }
    assert!(d.take_outbox().is_empty(), "joining sends nothing");
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
fn snapshot_rebuilds_the_document_exactly() {
    let mut ids = SeqIds::new(3);
    let mut a = Document::new(ActorId(1));
    let (s1, s2) = (ids.next_id(), ids.next_id());
    for s in [s1, s2] {
        a.edit(vec![
            Action::Create {
                id: s,
                kind: Kind::Station,
            },
            label(s, "x"),
        ])
        .unwrap();
    }
    a.edit(vec![Action::Remove {
        id: s2,
        kind: Kind::Station,
    }])
    .unwrap();
    let b = join(&a, 2);
    assert_eq!(a, b);
    assert!(b.entity(s2).is_none() && b.entity(s1).is_some());
}

#[test]
fn concurrent_edits_from_two_devices_converge() {
    let mut ids = SeqIds::new(9);
    let s = ids.next_id();
    let mut a = Document::new(ActorId(1));
    a.edit(vec![
        Action::Create {
            id: s,
            kind: Kind::Station,
        },
        label(s, "start"),
    ])
    .unwrap();
    let mut b = join(&a, 2);
    let t = ids.next_id();
    // Both edit the same field and each adds a different station while apart.
    a.edit(vec![label(s, "from laptop")]).unwrap();
    a.edit(vec![Action::Create {
        id: t,
        kind: Kind::Station,
    }])
    .unwrap();
    let u = ids.next_id();
    b.edit(vec![label(s, "from ipad")]).unwrap();
    b.edit(vec![Action::Create {
        id: u,
        kind: Kind::Station,
    }])
    .unwrap();
    exchange(&mut a, &mut b);
    assert_eq!(a, b);
    assert!(a.entity(t).is_some() && a.entity(u).is_some());
    // One of the two label edits won, the same one on both devices.
    assert_eq!(
        a.entity(s).unwrap().text(Field::StationLabel),
        b.entity(s).unwrap().text(Field::StationLabel)
    );
}

#[test]
fn undo_is_sent_like_any_other_edit() {
    let mut ids = SeqIds::new(5);
    let s = ids.next_id();
    let mut a = Document::new(ActorId(1));
    a.edit(vec![
        Action::Create {
            id: s,
            kind: Kind::Station,
        },
        label(s, "one"),
    ])
    .unwrap();
    let mut b = join(&a, 2);
    let mut h = History::new();
    let e = a.edit(vec![label(s, "two")]).unwrap();
    h.record(&e);
    exchange(&mut a, &mut b);
    assert_eq!(b.entity(s).unwrap().text(Field::StationLabel), "two");
    h.undo(&mut a).unwrap();
    exchange(&mut a, &mut b);
    assert_eq!(b.entity(s).unwrap().text(Field::StationLabel), "one");
    assert_eq!(a, b);
}

#[test]
fn a_device_with_its_own_actor_never_collides_with_another() {
    let mut a = Document::new(ActorId(1));
    let mut b = Document::new(ActorId(1));
    b.set_actor(ActorId(2));
    let id = SeqIds::new(1).next_id();
    let ea = a
        .edit(vec![Action::Create {
            id,
            kind: Kind::Station,
        }])
        .unwrap();
    let eb = b
        .edit(vec![Action::Create {
            id,
            kind: Kind::Station,
        }])
        .unwrap();
    assert_ne!(ea.ops[0].stamp, eb.ops[0].stamp);
}

#[test]
fn ops_survive_the_wire_bit_for_bit() {
    // Devices exchange ops as JSON. A number that comes back one digit off would make two devices
    // disagree about the same cave forever, so every float must round-trip exactly.
    let id = SeqIds::new(2).next_id();
    let mut d = Document::new(ActorId(1));
    let mut x = 0.1_f64;
    let mut ops = Vec::new();
    for _ in 0..200 {
        x = x * 1.618_034_1 + 0.000_000_1;
        ops.extend(
            d.edit(vec![Action::Set {
                id,
                field: Field::StationFixedZ,
                value: Value::Number(Some(x % 977.0)),
            }])
            .unwrap()
            .ops,
        );
    }
    let back: Vec<cave_doc::Op> =
        serde_json::from_str(&serde_json::to_string(&ops).unwrap()).unwrap();
    assert_eq!(ops, back);
}

#[test]
fn the_revision_moves_only_when_something_really_changes() {
    let mut ids = SeqIds::new(4);
    let s = ids.next_id();
    let mut a = Document::new(ActorId(1));
    assert_eq!(a.revision(), 0);
    a.edit(vec![
        Action::Create {
            id: s,
            kind: Kind::Station,
        },
        label(s, "x"),
    ])
    .unwrap();
    let r = a.revision();
    assert!(r > 0);
    // hearing the same ops again (a reconnect hands back the whole log) changes nothing
    let all = a.snapshot();
    for op in &all {
        a.apply(op).unwrap();
    }
    assert_eq!(a.revision(), r, "re-applied ops are not a change");
    // an older write loses and so is not a change either
    let mut b = join(&a, 2);
    let old = b.edit(vec![label(s, "later")]).unwrap().ops;
    a.edit(vec![label(s, "a newer one")]).unwrap();
    a.edit(vec![label(s, "newest")]).unwrap();
    let r2 = a.revision();
    a.apply(&old[0]).unwrap();
    assert_eq!(
        a.revision(),
        r2,
        "an op that loses to what is there is not a change"
    );
}
