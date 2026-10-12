//! The surveyed passage as a 3D surface, ported from the old `CsMesh3d`.
//!
//! At each station the wall points actually measured there (the four LRUD ticks AND the tip of
//! every splay shot from it) are projected into the plane perpendicular to the passage and sorted
//! by angle: that ring is the station's cross-section. Consecutive rings are lofted with triangle
//! strips.
//!
//! Rules carried over, none of them negotiable:
//! - A splay is a wall point (a splay into a dome shows the void instead of denting the tube).
//! - Nothing between measured points: straight strips, no smoothing or inferred curvature.
//! - A junction ends a run: where three or more distinct ways meet, one ring cannot describe the
//!   place, so each leg gets its own ring along its own direction instead of guessing a surface.
//! - Station elevations are never defaulted; the mesh is built only from placed stations.
//! - The frame of a vertical shaft is deterministic (north is the reference), so it does not spin
//!   from station to station down the same pitch.
//!
//! Output is plain buffers (positions, normals, colours, indices) so the renderer knows nothing
//! about caves.

mod build;
mod frame;

pub use build::{
    ColorMode, LegRings, LoftMode, Mesh, MeshInput, build, build_with_rings, cut_outline,
};
pub use frame::{Frame, RingPoint, frame_at, ring_at};
