//! The extended elevation: one survey run unrolled onto a single horizontal axis.
//!
//! Ported from the old `CsProfile` (read its comments in `cavecad-main/.../Core/CsProfile.js`).
//! Not a projected profile: the horizontal axis is distance travelled along the passage, so no
//! passage hides behind another and every leg draws at its true measured plan length.
//!
//! One band per survey run, and a run is decided by the STATION NAME, because that is where the
//! surveyor's intent already lives: split a name into alternating letter and digit groups; the run
//! key is every group but the last. `A20` is run `A`; `A13a1` is run `A13a`, a spur tying in at
//! `A13`. Names decide membership; the shot graph decides order. There is no length threshold.
//!
//! Slice one covers grouping, the longest chain, the tie to a parent run (from the shot graph, as
//! the old app does), and the ceiling and floor from U and D. Not ported yet: second ties, splay
//! classification for profile walls (`bandWallRuns`), and multi-band layout.

mod band;
mod hierarchy;
mod names;
mod runs;

pub use band::{Band, BandLeg, BandStation, StopReason, unroll_band};
pub use hierarchy::{Hierarchy, hierarchy};
pub use names::{Split, run_key_of, seq_num_of, seq_order, split_name, tie_name_of_run};
pub use runs::{Chain, Graph, LegInfo, Run, Runs, better_chain, group_runs, longest_chain};
