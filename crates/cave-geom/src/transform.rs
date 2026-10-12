//! Moves, turns, scales and flips.

use crate::point::Point;
use serde::{Deserialize, Serialize};

/// A 2D affine transform: `x' = a*x + c*y + tx`, `y' = b*x + d*y + ty`.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Transform {
    /// Matrix entries and offset.
    pub a: f64,
    /// See `a`.
    pub b: f64,
    /// See `a`.
    pub c: f64,
    /// See `a`.
    pub d: f64,
    /// See `a`.
    pub tx: f64,
    /// See `a`.
    pub ty: f64,
}

impl Transform {
    /// Leaves everything where it is.
    pub const IDENTITY: Transform = Transform {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    /// Moves by `(dx, dy)`.
    pub fn translate(dx: f64, dy: f64) -> Self {
        Transform {
            tx: dx,
            ty: dy,
            ..Self::IDENTITY
        }
    }

    /// Turns counter-clockwise by `radians` about the origin.
    pub fn rotate(radians: f64) -> Self {
        let (s, c) = radians.sin_cos();
        Transform {
            a: c,
            b: s,
            c: -s,
            d: c,
            tx: 0.0,
            ty: 0.0,
        }
    }

    /// Scales by `sx`, `sy` about the origin. A negative factor flips.
    pub fn scale(sx: f64, sy: f64) -> Self {
        Transform {
            a: sx,
            d: sy,
            ..Self::IDENTITY
        }
    }

    /// Applies `self` first, then `next`.
    pub fn then(self, next: Transform) -> Transform {
        Transform {
            a: next.a * self.a + next.c * self.b,
            b: next.b * self.a + next.d * self.b,
            c: next.a * self.c + next.c * self.d,
            d: next.b * self.c + next.d * self.d,
            tx: next.a * self.tx + next.c * self.ty + next.tx,
            ty: next.b * self.tx + next.d * self.ty + next.ty,
        }
    }

    /// Moves a point.
    pub fn apply(&self, p: Point) -> Point {
        Point::new(
            self.a * p.x + self.c * p.y + self.tx,
            self.b * p.x + self.d * p.y + self.ty,
        )
    }

    /// True when the transform mirrors (reverses turning direction).
    pub fn flips(&self) -> bool {
        self.a * self.d - self.b * self.c < 0.0
    }
}
