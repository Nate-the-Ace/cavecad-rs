//! Passage walls from LRUD (left, right, up, down) measurements and splays.
//!
//! Ported from the old `CsLrud`; read its long comments in
//! `cavecad-main/cave-survey/scripts/CaveSurvey/Core/CsLrud.js` for the reasoning behind each rule.
//! Pure geometry: it reads resolved stations and measurements and returns wall polylines in 3D.
//!
//! The rules that matter most, none of which may be "simplified" away:
//! - L and R are measured facing the direction of travel at the TO station. Right is azimuth + 90.
//! - A splay tip is a measured wall hit and joins the wall on its side.
//! - Walls are straight segments between measured points (previsualisation), never curves.
//! - A junction (three or more distinct ways out, by direction, not by leg count) ends a run. A loop
//!   tie-in is not a junction.
//! - "P" (open passage) is a reading, not a gap: the run carries through it.
//! - A pitch has no bearing: nothing here invents one. A tick with no bearing to aim along is not
//!   drawn, and where the caver faced nothing the passage direction is used instead.

mod axes;
mod points;
mod runs;
mod shell;

pub use axes::{
    AXIS_TOLERANCE_DEG, Axes, COINCIDENT_PLAN, Dir, THROUGH_SEPARATION_DEG, cluster_bearings,
    normalize_az, passage_azimuth_at, plan_bearing, relative_bearing, stations_axes,
};
pub use points::{Side, has_plan_evidence, side_open, station_wall_points, tick_azimuth, tick_end};
pub use runs::{Run, WallInput, Walls, wall_runs};
pub use shell::outer_shell;
