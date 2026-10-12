//! The circular arc hiding in a bulged polyline segment.

use crate::point::{Point, Rect};
use std::f64::consts::{FRAC_PI_2, TAU};

/// An arc from `start` to `end` turning through `sweep` radians (positive = counter-clockwise).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Arc {
    /// Centre of the circle.
    pub center: Point,
    /// Radius.
    pub radius: f64,
    /// Angle of the start point as seen from the centre.
    pub start_angle: f64,
    /// Signed angle swept from start to end.
    pub sweep: f64,
}

impl Arc {
    /// The arc through `a` and `b` with the given bulge, or `None` when it is a straight line
    /// (zero bulge or coincident points) or the numbers are not finite.
    pub fn from_bulge(a: Point, b: Point, bulge: f64) -> Option<Arc> {
        let chord = a.distance(b);
        if !(a.is_finite() && b.is_finite() && bulge.is_finite()) || bulge == 0.0 || chord == 0.0 {
            return None;
        }
        let sweep = 4.0 * bulge.atan();
        // Distance from the chord midpoint to the centre, positive on the left of a -> b.
        let h = (chord / 2.0) / (sweep / 2.0).tan();
        let (nx, ny) = (-(b.y - a.y) / chord, (b.x - a.x) / chord);
        let center = Point::new((a.x + b.x) / 2.0 + nx * h, (a.y + b.y) / 2.0 + ny * h);
        let radius = (chord / 2.0) / (sweep.abs() / 2.0).sin();
        Some(Arc {
            center,
            radius,
            start_angle: (a.y - center.y).atan2(a.x - center.x),
            sweep,
        })
    }

    /// Length along the arc.
    pub fn length(&self) -> f64 {
        self.radius * self.sweep.abs()
    }

    /// The exact box around the arc, including any extreme points the sweep passes through.
    pub fn bounds(&self) -> Rect {
        let at = |ang: f64| {
            Point::new(
                self.center.x + self.radius * ang.cos(),
                self.center.y + self.radius * ang.sin(),
            )
        };
        let mut r = Rect::from_points(at(self.start_angle), at(self.start_angle + self.sweep));
        for k in 0..4 {
            let axis = k as f64 * FRAC_PI_2;
            // How far counter-clockwise from the start this axis direction is, in [0, TAU).
            let rel = (axis - self.start_angle).rem_euclid(TAU);
            let inside = if self.sweep >= 0.0 {
                rel <= self.sweep
            } else {
                (TAU - rel) % TAU <= -self.sweep
            };
            if inside {
                r = r.include(at(axis));
            }
        }
        r
    }
}
