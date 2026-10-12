//! One stored thing and its last-writer-wins fields.

use crate::clock::Stamp;
use crate::field::{Field, Kind, Value};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A value with the stamp of the edit that wrote it. The later stamp always wins.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Lww<T> {
    /// The current value.
    pub value: T,
    /// Stamp of the edit that set it.
    pub stamp: Stamp,
}

/// A trip, station or shot: whether it exists, plus its fields.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Entity {
    pub(crate) kind: Kind,
    pub(crate) exists: Lww<bool>,
    pub(crate) fields: BTreeMap<Field, Lww<Value>>,
}

impl Entity {
    pub(crate) fn blank(kind: Kind) -> Self {
        Entity {
            kind,
            exists: Lww {
                value: false,
                stamp: Stamp::ZERO,
            },
            fields: BTreeMap::new(),
        }
    }

    /// Which kind of entity this is.
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// False once removed (and not re-created later).
    pub fn is_live(&self) -> bool {
        self.exists.value
    }

    /// The field's value, or its default if never set.
    pub fn get(&self, f: Field) -> Value {
        self.fields
            .get(&f)
            .map(|l| l.value.clone())
            .unwrap_or_else(|| f.default_value())
    }

    /// Text field (empty if unset).
    pub fn text(&self, f: Field) -> String {
        match self.get(f) {
            Value::Text(s) => s,
            _ => String::new(),
        }
    }

    /// Number field; `None` means not measured.
    pub fn number(&self, f: Field) -> Option<f64> {
        match self.get(f) {
            Value::Number(n) => n,
            _ => None,
        }
    }

    /// Flag field (false if unset).
    pub fn flag(&self, f: Field) -> bool {
        matches!(self.get(f), Value::Flag(true))
    }

    /// Point field; `None` means not placed.
    pub fn point(&self, f: Field) -> Option<cave_geom::Point> {
        match self.get(f) {
            Value::Point(p) => p,
            _ => None,
        }
    }

    /// Path field; `None` means not drawn.
    pub fn path(&self, f: Field) -> Option<cave_geom::Path> {
        match self.get(f) {
            Value::Path(p) => p,
            _ => None,
        }
    }

    /// Tag-map field (empty if unset).
    pub fn map(&self, f: Field) -> std::collections::BTreeMap<String, String> {
        match self.get(f) {
            Value::Map(m) => m,
            _ => Default::default(),
        }
    }

    /// Reference field.
    pub fn reference(&self, f: Field) -> Option<cave_id::Id> {
        match self.get(f) {
            Value::Ref(r) => r,
            _ => None,
        }
    }
}
