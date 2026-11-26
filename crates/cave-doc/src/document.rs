//! The document: every entity, and the one rule for changing them.

use crate::clock::{ActorId, Clock, Stamp};
use crate::entity::{Entity, Lww};
use crate::field::{Field, Kind, Value};
use crate::op::{Action, Op};
use cave_id::Id;
use std::collections::BTreeMap;

/// Why an action was refused. A refused action changes nothing.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ApplyError {
    /// The value's type does not match the field's.
    TypeMismatch,
    /// A number, point or path coordinate was NaN or infinite, or a path was too short.
    NotFinite,
    /// The id already belongs to an entity of a different kind.
    KindMismatch(Id),
}

/// The result of a local edit: the stamped ops to log or send, and the actions that undo it.
#[derive(Clone, PartialEq, Debug)]
pub struct Edit {
    /// What happened, with stamps. Send these to other editors.
    pub ops: Vec<Op>,
    /// Actions that put everything back, already in the order to apply them.
    pub inverse: Vec<Action>,
}

/// All trips, stations and shots, plus this editor's clock.
#[derive(Clone, Debug)]
pub struct Document {
    pub(crate) entities: BTreeMap<Id, Entity>,
    pub(crate) clock: Clock,
    /// Local ops not yet taken for sending (see `take_outbox`).
    outbox: Vec<Op>,
    /// How many ops have changed something so far (see `revision`).
    revision: u64,
}

impl PartialEq for Document {
    /// Two documents are equal when their contents are; clocks are bookkeeping.
    fn eq(&self, other: &Self) -> bool {
        self.entities == other.entities
    }
}

impl Document {
    /// An empty document edited by `actor`.
    pub fn new(actor: ActorId) -> Self {
        Document {
            entities: BTreeMap::new(),
            clock: Clock::new(actor),
            outbox: Vec::new(),
            revision: 0,
        }
    }

    /// The editor making local changes.
    pub fn actor(&self) -> ActorId {
        self.clock.actor()
    }

    /// Becomes a different editor from now on. Every device editing the same cave needs its own
    /// actor, or two of them could stamp different edits identically.
    pub fn set_actor(&mut self, actor: ActorId) {
        self.clock.set_actor(actor);
    }

    /// A number that goes up every time an op actually changes something (an op already applied, or
    /// older than what is there, does not count). Equal numbers mean no change in between, which is how
    /// the app knows whether there is anything unsaved.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The ops made locally (including by undo and redo) since the last call, oldest first.
    /// A sync layer sends these to the other editors.
    pub fn take_outbox(&mut self) -> Vec<Op> {
        std::mem::take(&mut self.outbox)
    }

    /// The whole document as ops: applying them to an empty document (any actor) rebuilds it, with
    /// every stamp intact, so it merges cleanly with whatever the others do next.
    pub fn snapshot(&self) -> Vec<Op> {
        let mut ops = Vec::new();
        for (id, e) in &self.entities {
            if e.exists.stamp > Stamp::ZERO {
                let action = if e.exists.value {
                    Action::Create {
                        id: *id,
                        kind: e.kind,
                    }
                } else {
                    Action::Remove {
                        id: *id,
                        kind: e.kind,
                    }
                };
                ops.push(Op {
                    stamp: e.exists.stamp,
                    action,
                });
            }
            for (field, l) in &e.fields {
                ops.push(Op {
                    stamp: l.stamp,
                    action: Action::Set {
                        id: *id,
                        field: *field,
                        value: l.value.clone(),
                    },
                });
            }
        }
        ops
    }

    /// The actions that turn this document into what `target` holds: what `target` has live and this does not is created,
    /// what differs is set back, and what this has live and `target` does not is removed. Drawing-wide settings are left
    /// alone (the room id, the scale, the location belong to the cave as it is now, not to a version of its survey). Applied
    /// as one edit, this is a restore: the document reads as `target` did, yet nothing is forgotten, because the restore is
    /// itself an edit that every editor receives and can undo.
    pub fn actions_to_reach(&self, target: &Document) -> Vec<Action> {
        let ids: std::collections::BTreeSet<Id> = self
            .entities
            .keys()
            .chain(target.entities.keys())
            .copied()
            .collect();
        let (mut create, mut set, mut remove) = (Vec::new(), Vec::new(), Vec::new());
        for id in ids {
            let mine = self.entities.get(&id);
            let theirs = target.entities.get(&id);
            let Some(kind) = theirs.map(|e| e.kind).or(mine.map(|e| e.kind)) else {
                continue;
            };
            if kind == Kind::Setting || mine.zip(theirs).is_some_and(|(a, b)| a.kind != b.kind) {
                continue;
            }
            let mine_live = mine.is_some_and(|e| e.exists.value);
            let theirs_live = theirs.is_some_and(|e| e.exists.value);
            if theirs_live {
                if !mine_live {
                    create.push(Action::Create { id, kind });
                }
                let fields: std::collections::BTreeSet<Field> = mine
                    .iter()
                    .flat_map(|e| e.fields.keys())
                    .chain(theirs.iter().flat_map(|e| e.fields.keys()))
                    .copied()
                    .collect();
                for field in fields {
                    let want = theirs.map_or_else(|| field.default_value(), |e| e.get(field));
                    let have = mine.map_or_else(|| field.default_value(), |e| e.get(field));
                    if want != have {
                        set.push(Action::Set {
                            id,
                            field,
                            value: want,
                        });
                    }
                }
            } else if mine_live {
                remove.push(Action::Remove { id, kind });
            }
        }
        create.extend(set);
        create.extend(remove);
        create
    }

    /// A live entity, or `None` if absent or removed.
    pub fn entity(&self, id: Id) -> Option<&Entity> {
        self.entities.get(&id).filter(|e| e.is_live())
    }

    /// Ids of all live entities of one kind, in id order.
    pub fn ids_of(&self, kind: Kind) -> Vec<Id> {
        self.entities
            .iter()
            .filter(|(_, e)| e.kind == kind && e.is_live())
            .map(|(id, _)| *id)
            .collect()
    }

    fn check(&self, a: &Action) -> Result<(), ApplyError> {
        let (id, kind) = match a {
            Action::Create { id, kind } | Action::Remove { id, kind } => (*id, *kind),
            Action::Set { id, field, value } => {
                if value.value_type() != field.value_type() {
                    return Err(ApplyError::TypeMismatch);
                }
                if !value.is_valid() {
                    return Err(ApplyError::NotFinite);
                }
                (*id, field.kind())
            }
        };
        match self.entities.get(&id) {
            Some(e) if e.kind != kind => Err(ApplyError::KindMismatch(id)),
            _ => Ok(()),
        }
    }

    /// Applies an op from anywhere (this editor or another). Safe to repeat and to reorder:
    /// the later stamp wins per field.
    pub fn apply(&mut self, op: &Op) -> Result<(), ApplyError> {
        self.check(&op.action)?;
        self.clock.observe(op.stamp);
        let stamp = op.stamp;
        match &op.action {
            Action::Create { id, kind } | Action::Remove { id, kind } => {
                let live = matches!(op.action, Action::Create { .. });
                let e = self
                    .entities
                    .entry(*id)
                    .or_insert_with(|| Entity::blank(*kind));
                if stamp > e.exists.stamp {
                    e.exists = Lww { value: live, stamp };
                    self.revision += 1;
                }
            }
            Action::Set { id, field, value } => {
                let e = self
                    .entities
                    .entry(*id)
                    .or_insert_with(|| Entity::blank(field.kind()));
                if e.fields.get(field).is_none_or(|old| stamp > old.stamp) {
                    e.fields.insert(
                        *field,
                        Lww {
                            value: value.clone(),
                            stamp,
                        },
                    );
                    self.revision += 1;
                }
            }
        }
        Ok(())
    }

    /// Makes a local edit out of one or more actions, all or nothing. Returns the ops (to log
    /// and send) and the inverse (to undo).
    pub fn edit(&mut self, actions: Vec<Action>) -> Result<Edit, ApplyError> {
        for a in &actions {
            self.check(a)?;
        }
        let mut ops = Vec::with_capacity(actions.len());
        let mut inverse = Vec::with_capacity(actions.len());
        for action in actions {
            inverse.push(self.inverse_of(&action));
            let op = Op {
                stamp: self.clock.tick(),
                action,
            };
            self.apply(&op)?;
            self.outbox.push(op.clone());
            ops.push(op);
        }
        inverse.reverse();
        Ok(Edit { ops, inverse })
    }

    fn inverse_of(&self, a: &Action) -> Action {
        match a {
            Action::Create { id, kind } | Action::Remove { id, kind } => {
                if self.entity(*id).is_some() {
                    Action::Create {
                        id: *id,
                        kind: *kind,
                    }
                } else {
                    Action::Remove {
                        id: *id,
                        kind: *kind,
                    }
                }
            }
            Action::Set { id, field, .. } => {
                let prior: Value = self
                    .entities
                    .get(id)
                    .map(|e| e.get(*field))
                    .unwrap_or_else(|| field.default_value());
                Action::Set {
                    id: *id,
                    field: *field,
                    value: prior,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::Stamp;
    use cave_id::{IdSource, SeqIds};

    fn doc(a: u64) -> Document {
        Document::new(ActorId(a))
    }

    fn set(id: Id, field: Field, value: Value) -> Action {
        Action::Set { id, field, value }
    }

    #[test]
    fn create_and_set_then_read_back() {
        let mut d = doc(1);
        let id = SeqIds::new(1).next_id();
        d.edit(vec![
            Action::Create {
                id,
                kind: Kind::Station,
            },
            set(id, Field::StationLabel, Value::Text("A1".into())),
        ])
        .unwrap();
        assert_eq!(d.entity(id).unwrap().text(Field::StationLabel), "A1");
        assert_eq!(d.ids_of(Kind::Station), vec![id]);
    }

    #[test]
    fn later_stamp_wins_in_any_order() {
        let id = SeqIds::new(1).next_id();
        let lo = Op {
            stamp: Stamp {
                counter: 1,
                actor: ActorId(1),
            },
            action: set(id, Field::StationLabel, Value::Text("old".into())),
        };
        let hi = Op {
            stamp: Stamp {
                counter: 2,
                actor: ActorId(2),
            },
            action: set(id, Field::StationLabel, Value::Text("new".into())),
        };
        let (mut a, mut b) = (doc(9), doc(8));
        a.apply(&lo).unwrap();
        a.apply(&hi).unwrap();
        b.apply(&hi).unwrap();
        b.apply(&lo).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.entities[&id].text(Field::StationLabel), "new");
    }

    #[test]
    fn applying_twice_changes_nothing() {
        let id = SeqIds::new(1).next_id();
        let op = Op {
            stamp: Stamp {
                counter: 1,
                actor: ActorId(1),
            },
            action: Action::Create {
                id,
                kind: Kind::Trip,
            },
        };
        let mut d = doc(2);
        d.apply(&op).unwrap();
        let once = d.clone();
        d.apply(&op).unwrap();
        assert_eq!(d, once);
    }

    #[test]
    fn field_may_arrive_before_create() {
        let id = SeqIds::new(1).next_id();
        let s = Op {
            stamp: Stamp {
                counter: 2,
                actor: ActorId(1),
            },
            action: set(id, Field::TripName, Value::Text("Main".into())),
        };
        let c = Op {
            stamp: Stamp {
                counter: 1,
                actor: ActorId(1),
            },
            action: Action::Create {
                id,
                kind: Kind::Trip,
            },
        };
        let mut d = doc(2);
        d.apply(&s).unwrap();
        assert!(d.entity(id).is_none());
        d.apply(&c).unwrap();
        assert_eq!(d.entity(id).unwrap().text(Field::TripName), "Main");
    }

    #[test]
    fn bad_edits_are_refused_whole() {
        let mut d = doc(1);
        let mut ids = SeqIds::new(1);
        let (a, b) = (ids.next_id(), ids.next_id());
        let before = d.clone();
        let err = d
            .edit(vec![
                Action::Create {
                    id: a,
                    kind: Kind::Station,
                },
                set(b, Field::StationLabel, Value::Number(Some(1.0))),
            ])
            .unwrap_err();
        assert_eq!(err, ApplyError::TypeMismatch);
        assert_eq!(d, before);
        assert_eq!(
            d.edit(vec![set(
                a,
                Field::ShotDistance,
                Value::Number(Some(f64::NAN))
            )])
            .unwrap_err(),
            ApplyError::NotFinite
        );
    }

    #[test]
    fn id_cannot_change_kind() {
        let mut d = doc(1);
        let id = SeqIds::new(1).next_id();
        d.edit(vec![Action::Create {
            id,
            kind: Kind::Station,
        }])
        .unwrap();
        assert_eq!(
            d.edit(vec![Action::Create {
                id,
                kind: Kind::Shot
            }])
            .unwrap_err(),
            ApplyError::KindMismatch(id)
        );
    }

    #[test]
    fn remove_then_recreate() {
        let mut d = doc(1);
        let id = SeqIds::new(1).next_id();
        d.edit(vec![Action::Create {
            id,
            kind: Kind::Station,
        }])
        .unwrap();
        d.edit(vec![Action::Remove {
            id,
            kind: Kind::Station,
        }])
        .unwrap();
        assert!(d.entity(id).is_none());
        d.edit(vec![Action::Create {
            id,
            kind: Kind::Station,
        }])
        .unwrap();
        assert!(d.entity(id).is_some());
    }

    #[test]
    fn unmeasured_survives_a_set() {
        let mut d = doc(1);
        let id = SeqIds::new(1).next_id();
        d.edit(vec![
            Action::Create {
                id,
                kind: Kind::Shot,
            },
            set(id, Field::ShotLeft, Value::Number(Some(0.0))),
        ])
        .unwrap();
        assert_eq!(d.entity(id).unwrap().number(Field::ShotLeft), Some(0.0));
        d.edit(vec![set(id, Field::ShotLeft, Value::Number(None))])
            .unwrap();
        assert_eq!(d.entity(id).unwrap().number(Field::ShotLeft), None);
    }

    /// A document rebuilt from another's ops: what an earlier version of a cave is.
    fn rebuilt(from: &Document) -> Document {
        let mut d = doc(9);
        for op in from.snapshot() {
            d.apply(&op).unwrap();
        }
        d
    }

    fn label(d: &Document, id: Id) -> Option<String> {
        d.entity(id).map(|e| e.text(Field::StationLabel))
    }

    #[test]
    fn restoring_makes_the_document_read_as_the_earlier_version_and_can_be_undone() {
        let mut ids = SeqIds::new(1);
        let (a, b, c) = (ids.next_id(), ids.next_id(), ids.next_id());
        let mut live = doc(1);
        live.edit(vec![
            Action::Create {
                id: a,
                kind: Kind::Station,
            },
            set(a, Field::StationLabel, Value::Text("A1".into())),
            Action::Create {
                id: b,
                kind: Kind::Station,
            },
            set(b, Field::StationLabel, Value::Text("B".into())),
        ])
        .unwrap();
        let before = rebuilt(&live); // the version to go back to
        // later edits: one station renamed, one removed, one added
        live.edit(vec![
            set(a, Field::StationLabel, Value::Text("changed".into())),
            Action::Remove {
                id: b,
                kind: Kind::Station,
            },
            Action::Create {
                id: c,
                kind: Kind::Station,
            },
            set(c, Field::StationLabel, Value::Text("new".into())),
        ])
        .unwrap();
        assert_eq!(label(&live, a).as_deref(), Some("changed"));
        let actions = live.actions_to_reach(&before);
        let restore = live.edit(actions).unwrap();
        assert_eq!(
            label(&live, a).as_deref(),
            Some("A1"),
            "a renamed station reads as it did"
        );
        assert_eq!(
            label(&live, b).as_deref(),
            Some("B"),
            "a removed one is back"
        );
        assert!(live.entity(c).is_none(), "one added since is gone");
        assert!(
            live.actions_to_reach(&before).is_empty(),
            "restoring twice finds nothing left to do"
        );
        // the restore is an edit like any other: it can be undone, and nothing before it was lost
        live.edit(restore.inverse).unwrap();
        assert_eq!(label(&live, a).as_deref(), Some("changed"));
        assert!(live.entity(c).is_some());
    }

    #[test]
    fn restoring_leaves_the_drawing_wide_settings_alone() {
        let mut ids = SeqIds::new(1);
        let (a, key) = (ids.next_id(), ids.next_id());
        let mut live = doc(1);
        live.edit(vec![Action::Create {
            id: a,
            kind: Kind::Station,
        }])
        .unwrap();
        let before = rebuilt(&live);
        live.edit(vec![
            Action::Create {
                id: key,
                kind: Kind::Setting,
            },
            set(key, Field::SettingValue, Value::Text("room".into())),
        ])
        .unwrap();
        assert!(
            live.actions_to_reach(&before).is_empty(),
            "a setting made since is not a part of the survey's version"
        );
    }
}
