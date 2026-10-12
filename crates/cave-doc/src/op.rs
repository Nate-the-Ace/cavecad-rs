//! The vocabulary of edits.

use crate::clock::Stamp;
use crate::field::{Field, Kind, Value};
use cave_id::Id;
use serde::{Deserialize, Serialize};

/// One change a person makes.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum Action {
    /// Make an entity exist.
    Create {
        /// Which entity.
        id: Id,
        /// What kind it is.
        kind: Kind,
    },
    /// Remove an entity (it can be re-created by a later edit).
    Remove {
        /// Which entity.
        id: Id,
        /// What kind it is.
        kind: Kind,
    },
    /// Set one field.
    Set {
        /// Which entity.
        id: Id,
        /// Which field.
        field: Field,
        /// The new value; its type must match the field.
        value: Value,
    },
}

/// An action plus when and by whom: the unit that is logged and sent to other editors.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Op {
    /// Logical time of the edit.
    pub stamp: Stamp,
    /// What changed.
    pub action: Action,
}
