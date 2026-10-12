//! Problems a merge can leave behind. Merging never fails on these; the Check tools show them.

use crate::document::Document;
use crate::field::{Field, Kind};
use cave_id::Id;
use std::collections::BTreeMap;

/// Something wrong with the survey that a person should look at.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Issue {
    /// A shot points at a station or trip that does not exist (for example, deleted by someone else).
    DanglingReference {
        /// The shot.
        shot: Id,
        /// Which reference field.
        field: Field,
        /// The missing target.
        target: Id,
    },
    /// Two or more stations share a label (two people each made `A5` while offline).
    DuplicateLabel {
        /// The shared label.
        label: String,
        /// The stations using it.
        stations: Vec<Id>,
    },
    /// A shot has no FROM station.
    ShotWithoutFrom {
        /// The shot.
        shot: Id,
    },
    /// A shot that is not a splay has no TO station.
    ShotWithoutTo {
        /// The shot.
        shot: Id,
    },
}

/// Lists every issue in the document, in a stable order.
pub fn validate(doc: &Document) -> Vec<Issue> {
    let mut out = Vec::new();
    let mut labels: BTreeMap<String, Vec<Id>> = BTreeMap::new();
    for id in doc.ids_of(Kind::Station) {
        let label = doc
            .entity(id)
            .map(|e| e.text(Field::StationLabel))
            .unwrap_or_default();
        if !label.is_empty() {
            labels.entry(label).or_default().push(id);
        }
    }
    for (label, stations) in labels {
        if stations.len() > 1 {
            out.push(Issue::DuplicateLabel { label, stations });
        }
    }
    for shot in doc.ids_of(Kind::Shot) {
        let Some(e) = doc.entity(shot) else { continue };
        for (field, kind) in [
            (Field::ShotFrom, Kind::Station),
            (Field::ShotTo, Kind::Station),
            (Field::ShotTrip, Kind::Trip),
        ] {
            if let Some(target) = e.reference(field)
                && doc.entity(target).is_none_or(|t| t.kind() != kind)
            {
                out.push(Issue::DanglingReference {
                    shot,
                    field,
                    target,
                });
            }
        }
        if e.reference(Field::ShotFrom).is_none() {
            out.push(Issue::ShotWithoutFrom { shot });
        }
        if e.reference(Field::ShotTo).is_none() && !e.flag(Field::ShotSplay) {
            out.push(Issue::ShotWithoutTo { shot });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ActorId;
    use crate::field::Value;
    use crate::op::Action;
    use cave_id::{IdSource, SeqIds};

    fn station(d: &mut Document, id: Id, label: &str) {
        d.edit(vec![
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
    }

    #[test]
    fn clean_survey_has_no_issues() {
        let mut d = Document::new(ActorId(1));
        let mut ids = SeqIds::new(1);
        let (a, b, s) = (ids.next_id(), ids.next_id(), ids.next_id());
        station(&mut d, a, "A1");
        station(&mut d, b, "A2");
        d.edit(vec![
            Action::Create {
                id: s,
                kind: Kind::Shot,
            },
            Action::Set {
                id: s,
                field: Field::ShotFrom,
                value: Value::Ref(Some(a)),
            },
            Action::Set {
                id: s,
                field: Field::ShotTo,
                value: Value::Ref(Some(b)),
            },
        ])
        .unwrap();
        assert!(validate(&d).is_empty());
    }

    #[test]
    fn finds_duplicates_dangling_and_missing_ends() {
        let mut d = Document::new(ActorId(1));
        let mut ids = SeqIds::new(1);
        let (a, b, s, t) = (ids.next_id(), ids.next_id(), ids.next_id(), ids.next_id());
        station(&mut d, a, "A5");
        station(&mut d, b, "A5");
        d.edit(vec![
            Action::Create {
                id: s,
                kind: Kind::Shot,
            },
            Action::Set {
                id: s,
                field: Field::ShotFrom,
                value: Value::Ref(Some(a)),
            },
            Action::Set {
                id: s,
                field: Field::ShotTo,
                value: Value::Ref(Some(b)),
            },
            Action::Create {
                id: t,
                kind: Kind::Shot,
            },
        ])
        .unwrap();
        d.edit(vec![Action::Remove {
            id: b,
            kind: Kind::Station,
        }])
        .unwrap();
        let issues = validate(&d);
        assert!(issues.contains(&Issue::DanglingReference {
            shot: s,
            field: Field::ShotTo,
            target: b
        }));
        assert!(issues.contains(&Issue::ShotWithoutFrom { shot: t }));
        assert!(issues.contains(&Issue::ShotWithoutTo { shot: t }));
        assert!(
            !issues
                .iter()
                .any(|i| matches!(i, Issue::DuplicateLabel { .. }))
        );
    }

    #[test]
    fn duplicate_labels_are_reported() {
        let mut d = Document::new(ActorId(1));
        let mut ids = SeqIds::new(1);
        let (a, b) = (ids.next_id(), ids.next_id());
        station(&mut d, a, "A5");
        station(&mut d, b, "A5");
        assert_eq!(
            validate(&d),
            vec![Issue::DuplicateLabel {
                label: "A5".into(),
                stations: vec![a, b]
            }]
        );
    }
}
