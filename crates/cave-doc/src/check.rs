//! Problems a merge can leave behind. Merging never fails on these; the Check tools show them.

use crate::document::Document;
use crate::field::{Field, Kind, Value};
use cave_id::Id;
use std::collections::BTreeMap;

/// Something wrong with the survey or drawing that a person should look at.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Issue {
    /// A reference points at something that does not exist or is the wrong kind (for example
    /// a station deleted by someone else).
    DanglingReference {
        /// The entity holding the reference.
        entity: Id,
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
    /// A required field is empty (a shot with no FROM station, a feature with no layer or line).
    MissingRequired {
        /// The incomplete entity.
        entity: Id,
        /// The empty field.
        field: Field,
    },
}

/// References that must point at a live entity of the given kind.
const REFS: &[(Field, Kind)] = &[
    (Field::ShotFrom, Kind::Station),
    (Field::ShotTo, Kind::Station),
    (Field::ShotTrip, Kind::Trip),
    (Field::FeatureTrip, Kind::Trip),
    (Field::FeatureStation, Kind::Station),
    (Field::SymbolTrip, Kind::Trip),
    (Field::TextTrip, Kind::Trip),
    (Field::TextTrip, Kind::Trip),
];

/// Fields every entity of the kind must have filled in.
const REQUIRED: &[(Field, Kind)] = &[
    (Field::ShotFrom, Kind::Shot),
    (Field::SymbolAt, Kind::Symbol),
    (Field::TextAt, Kind::Text),
];

const KINDS: &[Kind] = &[
    Kind::Trip,
    Kind::Station,
    Kind::Shot,
    Kind::Feature,
    Kind::Symbol,
    Kind::Text,
];

fn is_empty(v: &crate::field::Value) -> bool {
    use crate::field::Value::*;
    matches!(v, Ref(None) | Point(None) | Path(None))
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
    for &kind in KINDS {
        for id in doc.ids_of(kind) {
            let Some(e) = doc.entity(id) else { continue };
            for &(field, target_kind) in REFS.iter().filter(|(f, _)| f.kind() == kind) {
                if let Some(target) = e.reference(field)
                    && doc.entity(target).is_none_or(|t| t.kind() != target_kind)
                {
                    out.push(Issue::DanglingReference {
                        entity: id,
                        field,
                        target,
                    });
                }
            }
            for &(field, _) in REQUIRED.iter().filter(|(_, k)| *k == kind) {
                if is_empty(&e.get(field)) {
                    out.push(Issue::MissingRequired { entity: id, field });
                }
            }
            if kind == Kind::Feature {
                let anchored = e.get(Field::FeatureAnchored);
                let drawn = e.path(Field::FeaturePath).is_some()
                    || matches!(anchored, Value::Anchored(Some(_)));
                if !drawn {
                    out.push(Issue::MissingRequired {
                        entity: id,
                        field: Field::FeaturePath,
                    });
                }
                // A stroke hung on stations that no longer exist cannot be placed.
                if let Value::Anchored(Some(a)) = anchored {
                    let mut seen = std::collections::BTreeSet::new();
                    for st in a.stations() {
                        if seen.insert(st)
                            && doc.entity(st).is_none_or(|t| t.kind() != Kind::Station)
                        {
                            out.push(Issue::DanglingReference {
                                entity: id,
                                field: Field::FeatureAnchored,
                                target: st,
                            });
                        }
                    }
                }
            }
            if kind == Kind::Shot
                && e.reference(Field::ShotTo).is_none()
                && !e.flag(Field::ShotSplay)
            {
                out.push(Issue::MissingRequired {
                    entity: id,
                    field: Field::ShotTo,
                });
            }
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
    use cave_geom::{Path, Point};
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

    fn set(id: Id, field: Field, value: Value) -> Action {
        Action::Set { id, field, value }
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
            set(s, Field::ShotFrom, Value::Ref(Some(a))),
            set(s, Field::ShotTo, Value::Ref(Some(b))),
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
            set(s, Field::ShotFrom, Value::Ref(Some(a))),
            set(s, Field::ShotTo, Value::Ref(Some(b))),
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
            entity: s,
            field: Field::ShotTo,
            target: b
        }));
        assert!(issues.contains(&Issue::MissingRequired {
            entity: t,
            field: Field::ShotFrom
        }));
        assert!(issues.contains(&Issue::MissingRequired {
            entity: t,
            field: Field::ShotTo
        }));
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

    #[test]
    fn drawing_entities_need_geometry() {
        let mut d = Document::new(ActorId(1));
        let mut ids = SeqIds::new(1);
        let (good, bare, gone) = (ids.next_id(), ids.next_id(), ids.next_id());
        let line = Path::polyline(&[Point::new(0.0, 0.0), Point::new(1.0, 1.0)], false);
        d.edit(vec![
            Action::Create {
                id: good,
                kind: Kind::Feature,
            },
            set(good, Field::FeaturePath, Value::Path(Some(line.clone()))),
            Action::Create {
                id: bare,
                kind: Kind::Feature,
            },
            Action::Create {
                id: gone,
                kind: Kind::Feature,
            },
            set(gone, Field::FeatureTrip, Value::Ref(Some(ids.next_id()))),
            set(gone, Field::FeaturePath, Value::Path(Some(line))),
        ])
        .unwrap();
        let issues = validate(&d);
        assert!(!issues.iter().any(|i| matches!(i, Issue::MissingRequired { entity, .. } | Issue::DanglingReference { entity, .. } if *entity == good)));
        assert!(issues.contains(&Issue::MissingRequired {
            entity: bare,
            field: Field::FeaturePath
        }));
        assert!(
            issues
                .iter()
                .any(|i| matches!(i, Issue::DanglingReference { entity, field: Field::FeatureTrip, .. } if *entity == gone))
        );
    }

    #[test]
    fn short_or_non_finite_paths_are_refused() {
        let mut d = Document::new(ActorId(1));
        let id = SeqIds::new(1).next_id();
        let short = Path::polyline(&[Point::new(0.0, 0.0)], false);
        assert!(
            d.edit(vec![set(id, Field::FeaturePath, Value::Path(Some(short)))])
                .is_err()
        );
    }
}
