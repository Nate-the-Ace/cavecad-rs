//! The in-cave Survey session: shots from an instrument straight into the notebook.
//!
//! [`rules`] are pure decisions (what is this reading, where does the surveyor stand next); [`Session`] carries
//! them out against the open cave through a [`Host`], writing each shot as one undoable notebook edit.

pub mod rules;
mod session;

pub use rules::{Mode, Plan, PlanKind, SessionState};
pub use session::{
    Entry, EntryView, HeldView, Host, Incoming, Session, Snapshot, StandingWalls, Target,
};
