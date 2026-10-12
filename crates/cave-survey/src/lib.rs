//! Survey math: shots to station positions, loop closure measurement, and least-squares
//! adjustment. Everything here is derived data: it reads a [`cave_doc::Document`] and stores
//! nothing back.
//!
//! All lengths are metres, x east, y north, z up. Azimuths are degrees clockwise from true north.
//! Magnetic declination is already applied to azimuths when they are stored.
//!
//! Pipeline: [`extract`] (document to legs) -> [`resolve`] (positions and closures) ->
//! [`adjust`] (spread closure error). Honesty rule: the closures and loops reported to people are
//! always the as-surveyed ones, never recomputed from adjusted coordinates.

mod adjust;
mod extract;
mod network;
mod traverse;
mod vec3;

pub use adjust::{
    Adjusted, CG_TOLERANCE_FRACTION, MIN_VARIANCE, NO_ADJUST_WEIGHT_FACTOR, PLUMB_SIGMA_ANGLE_DEG,
    Params, Residual, Shift, Summary, adjust,
};
pub use extract::{Extracted, SkipReason, Skipped, Splay, StationLrud, extract};
pub use network::{
    Closure, ClosureKind, Fixed, Leg, LegKind, LegResult, Placed, Resolved, resolve,
};
pub use traverse::{
    Offset, PLUMB_DEG, Reading, TapeMode, effective_azimuth, effective_inclination, is_plumb,
    offset,
};
pub use vec3::Vec3;
