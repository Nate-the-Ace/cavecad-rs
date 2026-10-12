//! Which scanned pages have been dealt with, so the team can see what is done and what still needs
//! attention. A page is incomplete until someone marks it complete. The mark is shared like any
//! other edit and kept in the cave; the picture is not.

use crate::{ScanError, clean_path};
use cave_doc::{Action, Document, Field, Kind, Value};
use cave_id::Id;

/// Most pages one edit may mark (a whole scans folder).
pub const MAX_MARKS: usize = 5000;

/// The id of the record for a scan: derived from its path, so everyone who marks the same page
/// writes to the same record and their marks agree instead of piling up.
pub fn scan_id(path: &str) -> Option<Id> {
    Some(Id::from_name(&format!("scan:{}", clean_path(path)?)))
}

/// Every page marked complete, as paths, in path order.
pub fn completed(doc: &Document) -> Vec<String> {
    let mut out: Vec<String> = doc
        .ids_of(Kind::Scan)
        .into_iter()
        .filter_map(|id| doc.entity(id))
        .filter(|e| e.flag(Field::ScanComplete))
        .map(|e| e.text(Field::ScanPath))
        .filter(|p| !p.is_empty())
        .collect();
    out.sort();
    out
}

/// Marks pages complete or incomplete (only what changes is written).
pub fn set_complete(
    doc: &Document,
    paths: &[String],
    complete: bool,
) -> Result<Vec<Action>, ScanError> {
    if paths.len() > MAX_MARKS {
        return Err(ScanError::BadPath);
    }
    let mut actions = Vec::new();
    for raw in paths {
        let path = clean_path(raw).ok_or(ScanError::BadPath)?;
        let id = scan_id(&path).ok_or(ScanError::BadPath)?;
        let now = doc.entity(id);
        if now.is_none() && !complete {
            continue; // never marked: already incomplete
        }
        if now.is_none() {
            actions.push(Action::Create {
                id,
                kind: Kind::Scan,
            });
        }
        if now.is_none_or(|e| e.text(Field::ScanPath) != path) {
            actions.push(Action::Set {
                id,
                field: Field::ScanPath,
                value: Value::Text(path),
            });
        }
        if now.is_none_or(|e| e.flag(Field::ScanComplete) != complete) {
            actions.push(Action::Set {
                id,
                field: Field::ScanComplete,
                value: Value::Flag(complete),
            });
        }
    }
    Ok(actions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cave_doc::{ActorId, History};

    fn doc() -> Document {
        Document::new(ActorId(1))
    }

    fn p(s: &str) -> String {
        s.to_string()
    }

    #[test]
    fn pages_start_incomplete_and_marks_come_and_go_with_undo() {
        let mut d = doc();
        let mut h = History::new();
        assert!(completed(&d).is_empty());
        let e = d
            .edit(set_complete(&d, &[p("a/p2.png"), p("a/p1.png")], true).unwrap())
            .unwrap();
        h.record(&e);
        assert_eq!(completed(&d), ["a/p1.png", "a/p2.png"]);
        assert!(
            set_complete(&d, &[p("a/p1.png")], true).unwrap().is_empty(),
            "already complete"
        );
        assert!(
            set_complete(&d, &[p("never.png")], false)
                .unwrap()
                .is_empty(),
            "never marked: nothing to write"
        );
        let e = d
            .edit(set_complete(&d, &[p("a\\p1.png")], false).unwrap())
            .unwrap();
        h.record(&e);
        assert_eq!(completed(&d), ["a/p2.png"]);
        h.undo(&mut d).unwrap();
        assert_eq!(completed(&d), ["a/p1.png", "a/p2.png"]);
        h.undo(&mut d).unwrap();
        assert!(completed(&d).is_empty());
    }

    #[test]
    fn two_people_marking_the_same_page_agree() {
        // Each starts from nothing and marks the same page: one record, not two.
        let (mut a, mut b) = (doc(), Document::new(ActorId(2)));
        a.edit(set_complete(&a, &[p("x/p.png")], true).unwrap())
            .unwrap();
        b.edit(set_complete(&b, &[p("x/p.png")], true).unwrap())
            .unwrap();
        for op in a.snapshot() {
            b.apply(&op).unwrap();
        }
        assert_eq!(completed(&b), ["x/p.png"]);
        assert_eq!(b.ids_of(Kind::Scan).len(), 1);
        assert_eq!(scan_id("x/p.png"), scan_id("x\\p.png"));
    }

    #[test]
    fn a_bad_path_is_refused() {
        let d = doc();
        assert_eq!(
            set_complete(&d, &[p("../x.png")], true),
            Err(ScanError::BadPath)
        );
        let many: Vec<String> = (0..MAX_MARKS + 1).map(|i| format!("{i}.png")).collect();
        assert_eq!(set_complete(&d, &many, true), Err(ScanError::BadPath));
    }

    #[test]
    fn marks_survive_a_saved_file() {
        let mut d = doc();
        d.edit(set_complete(&d, &[p("keep/me.png")], true).unwrap())
            .unwrap();
        let text = cave_dxf::export(&d, &cave_dxf::ExportOptions::default());
        let back = cave_dxf::import(&text, ActorId(2))
            .unwrap()
            .document
            .unwrap();
        assert_eq!(completed(&back), ["keep/me.png"]);
    }
}
