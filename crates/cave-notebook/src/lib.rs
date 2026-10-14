//! The survey notebook: the survey laid out the way a notes page is, and the edits that go with it.
//!
//! Stations run down the left with their passage dimensions (LRUD) beside them; each shot's tape,
//! compass and clino (foresight and backsight) sit on the line *between* the two stations it joins.
//! A blank line means the chain broke, and the station written after it is where the next shots
//! start (a branch); a splay is a shot with no station at its far end, named `<station>.<n>`.
//!
//! A page is derived from the document every time ([`page`]); nothing here is stored. Every change a
//! person makes on the page is a set of ordinary document actions ([`edit`]), so it is undoable and
//! merges like any other edit. Compass cells hold what the compass read (magnetic): the trip's
//! declination turns them into the true bearings the survey stores.

mod edit;
mod live;
mod page;

pub use edit::{
    EditError, Reading, TripField, add_shot, new_cave, new_trip, remove_shot, rename_station,
    set_notes, set_reading, set_trip_declination, set_trip_field, set_walls,
};
pub use live::{
    Measured, Side, add_measured_shot, set_readings, set_station_wall, set_station_walls,
    station_walls, stations_in_use, suggest_next,
};
pub use page::{ClosureRow, Lrud, Page, Row, RowKind, Totals, TripChoice, page, trips};

/// Degrees by which a foresight and backsight may disagree before the page flags them.
pub const BACKSIGHT_TOLERANCE_DEG: f64 = 3.0;
