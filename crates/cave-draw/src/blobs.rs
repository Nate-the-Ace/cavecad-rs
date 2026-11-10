//! Named blobs of text kept in the cave as settings: how sheets (and the settings of the tool that
//! generates them) are stored. One entity per blob, its id made from its key, so two people saving
//! the same blob write to the same entity and agree, and each save is one undoable edit.

use crate::setting_id;
use cave_doc::{Action, Document, Field, Kind, Value};

/// The value stored under `key`, if any.
pub fn get(doc: &Document, key: &str) -> Option<String> {
    doc.entity(setting_id(key))
        .map(|e| e.text(Field::SettingValue))
        .filter(|t| !t.is_empty())
}

/// Every `(key, value)` whose key starts with `prefix`, in key order.
pub fn list(doc: &Document, prefix: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = doc
        .ids_of(Kind::Setting)
        .into_iter()
        .filter_map(|id| doc.entity(id))
        .map(|e| (e.text(Field::SettingKey), e.text(Field::SettingValue)))
        .filter(|(k, v)| k.starts_with(prefix) && !v.is_empty())
        .collect();
    out.sort();
    out
}

/// Actions that store `value` under `key` (nothing when it is already there).
pub fn put(doc: &Document, key: &str, value: &str) -> Vec<Action> {
    let id = setting_id(key);
    let mut actions = Vec::new();
    if doc.entity(id).is_none() {
        actions.push(Action::Create {
            id,
            kind: Kind::Setting,
        });
        actions.push(Action::Set {
            id,
            field: Field::SettingKey,
            value: Value::Text(key.into()),
        });
    }
    if doc
        .entity(id)
        .is_none_or(|e| e.text(Field::SettingValue) != value)
    {
        actions.push(Action::Set {
            id,
            field: Field::SettingValue,
            value: Value::Text(value.into()),
        });
    }
    actions
}

/// Actions that remove whatever is stored under `key` (nothing when nothing is).
pub fn remove(doc: &Document, key: &str) -> Vec<Action> {
    let id = setting_id(key);
    if doc.entity(id).is_some() {
        vec![Action::Remove {
            id,
            kind: Kind::Setting,
        }]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cave_doc::ActorId;

    #[test]
    fn blobs_are_stored_listed_and_removed() {
        let mut doc = Document::new(ActorId(1));
        let a = put(&doc, "sheet:a", "{\"n\":1}");
        doc.edit(a).unwrap();
        doc.edit(put(&doc, "sheet:b", "{}")).unwrap();
        doc.edit(put(&doc, "other", "x")).unwrap();
        assert_eq!(list(&doc, "sheet:").len(), 2);
        assert_eq!(get(&doc, "sheet:a").as_deref(), Some("{\"n\":1}"));
        assert!(put(&doc, "sheet:a", "{\"n\":1}").is_empty());
        doc.edit(remove(&doc, "sheet:a")).unwrap();
        assert_eq!(list(&doc, "sheet:").len(), 1);
        assert!(get(&doc, "sheet:a").is_none());
        // saving after a removal brings it back
        doc.edit(put(&doc, "sheet:a", "again")).unwrap();
        assert_eq!(get(&doc, "sheet:a").as_deref(), Some("again"));
    }
}
