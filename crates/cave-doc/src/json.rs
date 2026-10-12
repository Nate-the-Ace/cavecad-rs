//! Saving and loading a document as readable, versioned JSON.

use crate::clock::ActorId;
use crate::document::Document;
use crate::entity::Entity;
use cave_id::Id;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Version of the saved layout. Bump when the layout changes and add a migration.
pub const SCHEMA: u32 = 1;

/// Why a document could not be loaded.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum JsonError {
    /// The text was not a valid document.
    Parse(String),
    /// The file was written by a newer version than this one understands.
    NewerSchema {
        /// Version found in the file.
        found: u32,
        /// Newest version this build can read.
        supported: u32,
    },
}

#[derive(Serialize, Deserialize)]
struct Saved {
    schema: u32,
    entities: BTreeMap<Id, Entity>,
}

impl Document {
    /// The whole document as JSON, including removed entities (needed so merges stay correct).
    pub fn to_json(&self) -> String {
        let saved = Saved {
            schema: SCHEMA,
            entities: self.entities.clone(),
        };
        serde_json::to_string_pretty(&saved).expect("document is always serializable")
    }

    /// Loads a document saved by [`Document::to_json`], to be edited by `actor`.
    pub fn from_json(text: &str, actor: ActorId) -> Result<Document, JsonError> {
        let head: serde_json::Value =
            serde_json::from_str(text).map_err(|e| JsonError::Parse(e.to_string()))?;
        let found = head.get("schema").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        if found > SCHEMA {
            return Err(JsonError::NewerSchema {
                found,
                supported: SCHEMA,
            });
        }
        let saved: Saved =
            serde_json::from_value(head).map_err(|e| JsonError::Parse(e.to_string()))?;
        let mut doc = Document::new(actor);
        for e in saved.entities.values() {
            doc.clock.observe(e.exists.stamp);
            for f in e.fields.values() {
                doc.clock.observe(f.stamp);
            }
        }
        doc.entities = saved.entities;
        Ok(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::{Field, Kind, Value};
    use crate::op::Action;
    use cave_id::{IdSource, SeqIds};

    fn sample() -> (Document, Id) {
        let mut d = Document::new(ActorId(1));
        let id = SeqIds::new(1).next_id();
        d.edit(vec![
            Action::Create {
                id,
                kind: Kind::Shot,
            },
            Action::Set {
                id,
                field: Field::ShotDistance,
                value: Value::Number(Some(12.5)),
            },
            Action::Set {
                id,
                field: Field::ShotLeft,
                value: Value::Number(None),
            },
        ])
        .unwrap();
        (d, id)
    }

    #[test]
    fn round_trips() {
        let (d, _) = sample();
        let back = Document::from_json(&d.to_json(), ActorId(2)).unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn loaded_clock_sorts_new_edits_after_old_ones() {
        let (d, id) = sample();
        let mut back = Document::from_json(&d.to_json(), ActorId(2)).unwrap();
        back.edit(vec![Action::Set {
            id,
            field: Field::ShotDistance,
            value: Value::Number(Some(1.0)),
        }])
        .unwrap();
        assert_eq!(
            back.entity(id).unwrap().number(Field::ShotDistance),
            Some(1.0)
        );
    }

    #[test]
    fn rejects_newer_schema_and_garbage() {
        assert_eq!(
            Document::from_json("{\"schema\": 99, \"entities\": {}}", ActorId(1)).unwrap_err(),
            JsonError::NewerSchema {
                found: 99,
                supported: SCHEMA
            }
        );
        assert!(matches!(
            Document::from_json("not json", ActorId(1)),
            Err(JsonError::Parse(_))
        ));
    }
}
