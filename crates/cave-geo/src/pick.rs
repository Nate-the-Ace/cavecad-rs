//! The entrance pick: the lowest ground inside a circle the person drags on the Map. An entrance is
//! at the bottom of something, so the click only has to be close (old EntranceLocation snapped to the
//! lowest CONTOUR vertex; this reads the grid itself, so a low spot between contour lines counts).

use crate::frame::{LatLon, Placement};
use crate::grid::Grid;
use crate::request::Bounds;

/// Where the pick landed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LowPoint {
    /// Cave metres.
    pub point: [f64; 2],
    /// WGS84.
    pub at: LatLon,
    /// Ground there, metres NAVD88.
    pub ground_m: f64,
}

/// The lowest cell centre within `radius` (metres) of `centre` (cave metres); a tie goes to the
/// cell nearer the centre. `None` when the circle holds no real ground.
pub fn lowest_in_circle(
    g: &Grid,
    b: &Bounds,
    place: &Placement,
    centre: [f64; 2],
    radius: f64,
) -> Option<LowPoint> {
    let (ce, cn) = place.cave_to_utm(centre[0], centre[1]);
    let (cc, cr) = g.utm_cell(b, ce, cn);
    let span = (radius / (b.width() / g.w as f64).min(b.height() / g.h as f64)).ceil() + 1.0;
    let (w, h) = (g.w as i64, g.h as i64);
    let c_lo = ((cc - span).floor() as i64).max(0);
    let r_lo = ((cr - span).floor() as i64).max(0);
    let c_hi = ((cc + span).ceil() as i64).min(w - 1);
    let r_hi = ((cr + span).ceil() as i64).min(h - 1);
    let mut best: Option<(f64, f64, [f64; 2])> = None; // (ground, distance, point)
    for r in r_lo..=r_hi {
        for c in c_lo..=c_hi {
            let Some(v) = g.at(c as usize, r as usize) else {
                continue;
            };
            let (e, n) = g.cell_utm(b, c as f64, r as f64);
            let p = place.utm_to_cave(e, n);
            let d = ((p[0] - centre[0]).powi(2) + (p[1] - centre[1]).powi(2)).sqrt();
            if d > radius {
                continue;
            }
            if best.is_none_or(|(bv, bd, _)| v < bv || (v == bv && d < bd)) {
                best = Some((v, d, p));
            }
        }
    }
    best.map(|(ground_m, _, point)| LowPoint {
        point,
        at: place.cave_to_latlon(point[0], point[1]),
        ground_m,
    })
}
