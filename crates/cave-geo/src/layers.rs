//! What the Map and 3D view draw from a stored surface, in cave metres.
//!
//! Heights: the survey's own elevations are never rewritten. One rigid shift relates them to the
//! ground: offset = (ground at the entrance − entrance depth) − the entrance station's survey z,
//! applied to the GROUND when it is drawn (old CsElevation). No stored ground means no terrain,
//! never terrain at 0 (the old suite's elevation datum trap).

use crate::contour::{levels, lines};
use crate::frame::Placement;
use crate::grid::Grid;
use crate::request::Bounds;
use serde::Serialize;

/// How far an entrance is taken to sit below the ground, metres (5 ft). Display only, never stored.
pub const ENTRANCE_DEPTH_M: f64 = 5.0 * 0.3048;
/// Every this-many-th contour level is a major line.
pub const MAJOR_EVERY: i64 = 5;
const FT_PER_M: f64 = 1.0 / 0.3048;

/// Metres to add to a ground height to put it in the survey's frame, or `None` when the ground at
/// the entrance is unknown. `entrance_z` is the entrance station's solved z (metres).
pub fn datum_offset(ground_m: Option<f64>, entrance_z: f64) -> Option<f64> {
    ground_m.map(|g| g - ENTRANCE_DEPTH_M - entrance_z)
}

/// The six numbers `[a, b, c, d, e, f]` taking photo pixel corners `(px, py)` (py down) to cave
/// metres: `x = a·px + b·py + c`, `y = d·px + e·py + f` (the scans' matrix convention).
pub fn photo_matrix(b: &Bounds, size: (u32, u32), place: &Placement) -> [f64; 6] {
    let (sx, sy) = (
        b.width() / f64::from(size.0),
        b.height() / f64::from(size.1),
    );
    let o = place.utm_to_cave(b.xmin, b.ymax);
    let px = place.utm_to_cave(b.xmin + sx, b.ymax);
    let py = place.utm_to_cave(b.xmin, b.ymax - sy);
    [
        px[0] - o[0],
        py[0] - o[0],
        o[0],
        px[1] - o[1],
        py[1] - o[1],
        o[1],
    ]
}

/// One contour line in cave metres.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Contour {
    /// The level, in the display unit.
    pub level: f64,
    /// Every [`MAJOR_EVERY`]th level.
    pub major: bool,
    /// Closed ring.
    pub closed: bool,
    /// `[x, y, z, x, y, z, ...]`; z is the level shifted by the datum offset, or 0 without one.
    pub xyz: Vec<f64>,
}

/// Contours at `interval` (in the display unit: feet when `feet`), mapped into the cave frame.
pub fn contours(
    g: &Grid,
    b: &Bounds,
    place: &Placement,
    interval: f64,
    feet: bool,
    offset: Option<f64>,
) -> Vec<Contour> {
    let Some((lo, hi)) = g.range() else {
        return Vec::new();
    };
    let k = if feet { FT_PER_M } else { 1.0 };
    let mut out = Vec::new();
    for level in levels(lo * k, hi * k, interval) {
        let level_m = level / k;
        let major = ((level / interval).round() as i64) % MAJOR_EVERY == 0;
        for run in lines(g, level_m) {
            let mut xyz = Vec::with_capacity(run.points.len() * 3);
            for p in &run.points {
                let (e, n) = g.cell_utm(b, p[0], p[1]);
                let c = place.utm_to_cave(e, n);
                xyz.extend_from_slice(&[c[0], c[1], offset.map_or(0.0, |o| level_m + o)]);
            }
            out.push(Contour {
                level,
                major,
                closed: run.closed,
                xyz,
            });
        }
    }
    out
}

/// The ground as a mesh: `w*h` vertices row by row (holes get NaN z and are skipped by the view).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Terrain {
    /// Columns.
    pub w: usize,
    /// Rows.
    pub h: usize,
    /// `[x, y, z, ...]` in cave metres.
    pub xyz: Vec<f64>,
}

/// The terrain in the cave frame, or `None` without a datum offset.
pub fn terrain(g: &Grid, b: &Bounds, place: &Placement, offset: Option<f64>) -> Option<Terrain> {
    let off = offset?;
    let mut xyz = Vec::with_capacity(g.w * g.h * 3);
    for r in 0..g.h {
        for c in 0..g.w {
            let (e, n) = g.cell_utm(b, c as f64, r as f64);
            let p = place.utm_to_cave(e, n);
            xyz.extend_from_slice(&[p[0], p[1], g.at(c, r).map_or(f64::NAN, |v| v + off)]);
        }
    }
    Some(Terrain {
        w: g.w,
        h: g.h,
        xyz,
    })
}
