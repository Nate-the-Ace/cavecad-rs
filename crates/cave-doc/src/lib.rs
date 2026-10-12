//! The in-memory cave: trips, stations and shots, edited only through stamped [`Action`]s.
//!
//! Every field is a last-writer-wins register, so applying the same edits in any order, or twice,
//! gives the same document. That is what lets two people edit offline and merge later, and it
//! makes undo an ordinary edit. This crate does no I/O and reads no clock; see `docs/DESIGN.md`.
//!
//! Layers of data: authored survey data lives here; drawing data comes next; derived data
//! (positions, adjustment, walls) is never stored and is recomputed from these.

mod check;
mod clock;
mod document;
mod entity;
mod field;
mod history;
mod json;
mod op;

pub use check::{Issue, validate};
pub use clock::{ActorId, Clock, Stamp};
pub use document::{ApplyError, Document, Edit};
pub use entity::{Entity, Lww};
pub use field::{Field, Kind, Value, ValueType};
pub use history::History;
pub use json::{JsonError, SCHEMA};
pub use op::{Action, Op};
