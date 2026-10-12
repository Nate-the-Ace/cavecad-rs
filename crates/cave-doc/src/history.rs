//! Undo and redo for one editor.

use crate::document::{ApplyError, Document, Edit};
use crate::op::Action;

/// One editor's undo and redo stacks. Undo reverses only this editor's own edits.
#[derive(Default, Debug)]
pub struct History {
    undo: Vec<Vec<Action>>,
    redo: Vec<Vec<Action>>,
}

impl History {
    /// An empty history.
    pub fn new() -> Self {
        Self::default()
    }

    /// Remembers a local edit so it can be undone. A new edit clears redo.
    pub fn record(&mut self, edit: &Edit) {
        self.undo.push(edit.inverse.clone());
        self.redo.clear();
    }

    /// True if there is something to undo.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// True if there is something to redo.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Undoes the last recorded edit. Returns the new edit (log or send its ops), or `None`.
    pub fn undo(&mut self, doc: &mut Document) -> Result<Option<Edit>, ApplyError> {
        let Some(actions) = self.undo.pop() else {
            return Ok(None);
        };
        let edit = doc.edit(actions)?;
        self.redo.push(edit.inverse.clone());
        Ok(Some(edit))
    }

    /// Redoes the last undone edit.
    pub fn redo(&mut self, doc: &mut Document) -> Result<Option<Edit>, ApplyError> {
        let Some(actions) = self.redo.pop() else {
            return Ok(None);
        };
        let edit = doc.edit(actions)?;
        self.undo.push(edit.inverse.clone());
        Ok(Some(edit))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ActorId;
    use crate::field::{Field, Kind, Value};
    use cave_id::{IdSource, SeqIds};

    #[test]
    fn undo_and_redo_restore_values() {
        let mut d = Document::new(ActorId(1));
        let mut h = History::new();
        let id = SeqIds::new(1).next_id();
        let e = d
            .edit(vec![
                Action::Create {
                    id,
                    kind: Kind::Station,
                },
                Action::Set {
                    id,
                    field: Field::StationLabel,
                    value: Value::Text("A1".into()),
                },
            ])
            .unwrap();
        h.record(&e);
        let e = d
            .edit(vec![Action::Set {
                id,
                field: Field::StationLabel,
                value: Value::Text("A2".into()),
            }])
            .unwrap();
        h.record(&e);

        h.undo(&mut d).unwrap();
        assert_eq!(d.entity(id).unwrap().text(Field::StationLabel), "A1");
        h.undo(&mut d).unwrap();
        assert!(d.entity(id).is_none());
        assert!(!h.can_undo() && h.can_redo());

        h.redo(&mut d).unwrap();
        h.redo(&mut d).unwrap();
        assert_eq!(d.entity(id).unwrap().text(Field::StationLabel), "A2");
    }

    #[test]
    fn new_edit_clears_redo() {
        let mut d = Document::new(ActorId(1));
        let mut h = History::new();
        let id = SeqIds::new(1).next_id();
        let e = d
            .edit(vec![Action::Create {
                id,
                kind: Kind::Trip,
            }])
            .unwrap();
        h.record(&e);
        h.undo(&mut d).unwrap();
        let e = d
            .edit(vec![Action::Create {
                id,
                kind: Kind::Trip,
            }])
            .unwrap();
        h.record(&e);
        assert!(!h.can_redo());
    }
}
