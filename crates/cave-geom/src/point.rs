//! Points and boxes.

use serde::{Deserialize, Serialize};

/// A position in drawing units.
#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct Point {
    /// Horizontal, increasing right.
    pub x: f64,
    /// Vertical, increasing up.
    pub y: f64,
}

impl Point {
    /// A point at `(x, y)`.
    pub const fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }

    /// True when both coordinates are finite numbers.
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    /// Straight-line distance to `other`.
    pub fn distance(self, other: Point) -> f64 {
        (self.x - other.x).hypot(self.y - other.y)
    }
}

/// An axis-aligned box.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Rect {
    /// Lower-left corner.
    pub min: Point,
    /// Upper-right corner.
    pub max: Point,
}

impl Rect {
    /// The smallest box holding both points.
    pub fn from_points(a: Point, b: Point) -> Self {
        Rect {
            min: Point::new(a.x.min(b.x), a.y.min(b.y)),
            max: Point::new(a.x.max(b.x), a.y.max(b.y)),
        }
    }

    /// The smallest box holding this box and `p`.
    pub fn include(self, p: Point) -> Self {
        Rect {
            min: Point::new(self.min.x.min(p.x), self.min.y.min(p.y)),
            max: Point::new(self.max.x.max(p.x), self.max.y.max(p.y)),
        }
    }

    /// The smallest box holding both boxes.
    pub fn union(self, other: Rect) -> Self {
        self.include(other.min).include(other.max)
    }

    /// Width, height.
    pub fn size(self) -> (f64, f64) {
        (self.max.x - self.min.x, self.max.y - self.min.y)
    }
}
