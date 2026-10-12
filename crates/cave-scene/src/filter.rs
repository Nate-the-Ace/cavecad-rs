//! What the canvas shows, as a view setting: hide by type, hide by trip.
//!
//! This is never stored on anything a person authored. The viewer keeps the choice, hands it to
//! [`crate::build_filtered`], and the scene (including which lines run under which passage) is
//! worked out from what stays visible.

use cave_id::Id;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Which types and trips are hidden. The default hides nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct ViewFilter {
    /// Palette keys (`wall`, `splay`, ...) that are hidden.
    #[serde(default)]
    pub hidden_types: BTreeSet<String>,
    /// Trips (ids as text) that are hidden.
    #[serde(default)]
    pub hidden_trips: BTreeSet<String>,
}

impl ViewFilter {
    /// True if things of palette type `key` are shown.
    pub fn type_shown(&self, key: &str) -> bool {
        !self.hidden_types.contains(key)
    }

    /// True if things from `trip` are shown. Things with no trip are always shown.
    pub fn trip_shown(&self, trip: Option<Id>) -> bool {
        trip.is_none_or(|t| !self.hidden_trips.contains(&t.to_string()))
    }

    /// True if nothing is hidden.
    pub fn is_empty(&self) -> bool {
        self.hidden_types.is_empty() && self.hidden_trips.is_empty()
    }
}

/// A trip the filter can hide.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TripInfo {
    /// The trip's id as text (what [`ViewFilter::hidden_trips`] holds).
    pub id: String,
    /// Its name.
    pub name: String,
}
