//! Object snap: given the cursor and the snappable geometry of a view, return ranked snap candidates, the
//! guide lines to draw, and the state of a pick in progress.
//!
//! Pure and wasm32-safe (no file, thread, clock or entropy). The browser owns the camera, the overlay and the
//! input events; it hands this crate the things in view, the camera as a matrix, the cursor, the settings and
//! the [`Session`] (the explicit state of a pick: acquired points, track points, the one-pick override), and gets
//! a [`SnapResult`] back. Nothing is held between calls.
//!
//! # Space
//! Every curve is described in its own drawing plane ([`Frame`]: an origin and two orthonormal axes), so a plan
//! line, a profile line and a section line are all the same kind of thing. *World-defined* points (endpoints,
//! midpoints, centres, quadrants, ...) are computed exactly in that plane and then placed in the world and
//! projected. *Proximity* modes (nearest, perpendicular, tangent, intersection, extension crossings) work on the
//! curves flattened to polylines in the plane and projected to the screen, so what is chosen is what is seen under
//! a perspective camera; the candidate's world point is found again from the source `(thing, segment, t)`.
//!
//! # Modules
//! - [`camera`]: projecting world points and picking rays.
//! - [`mode`]: the snap modes, their tiers and names.
//! - [`scene`]: the things, settings, session and result types.
//! - [`snap()`]: the entry point.

pub mod camera;
mod curve;
mod guides;
pub mod mode;
mod points;
mod rank;
mod relational;
pub mod scene;
mod session;
mod snapper;

pub use camera::Camera;
pub use mode::{Mode, Tier};
pub use scene::{
    Acquired, Candidate, Continuation, Frame, Geometry, Guide, GuideKind, Scene, Session, Settings,
    SnapResult, Source, Thing, ThingKind,
};
pub use session::PickOutcome;
pub use snapper::{along_guide, snap};
