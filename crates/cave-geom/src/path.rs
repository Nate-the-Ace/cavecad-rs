//! A traced line: a polyline with arcs, or a smooth curve through fit points.
//!
//! A path may carry `widths`: one relative pen weight per point (1.0 is the layer's normal weight),
//! recorded from pen pressure. Widths change only how the line is drawn, never where it is, so
//! every measurement and merge ignores them. They are optional and absent for ordinary lines.

use crate::arc::Arc;
use crate::point::{Point, Rect};
use crate::transform::Transform;
use serde::{Deserialize, Serialize};

/// One corner of a polyline and the bulge of the segment that leaves it.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Vertex {
    /// Position.
    pub p: Point,
    /// `tan(sweep / 4)` of the arc to the next vertex; 0 is straight. Ignored on the last
    /// vertex of an open path.
    pub bulge: f64,
}

impl Vertex {
    /// A corner with a straight segment after it.
    pub const fn at(x: f64, y: f64) -> Self {
        Vertex {
            p: Point::new(x, y),
            bulge: 0.0,
        }
    }
}

/// Why a path was refused.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PathError {
    /// Fewer points than a line needs (2).
    TooShort,
    /// A coordinate or bulge was NaN or infinite.
    NotFinite,
    /// `widths` is not one positive, finite number per point.
    BadWidths,
}

/// A traced line.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum Path {
    /// Straight and arc segments between vertices.
    Polyline {
        /// The corners, in drawing order.
        vertices: Vec<Vertex>,
        /// True if the last vertex connects back to the first.
        closed: bool,
        /// Relative pen weight at each vertex (from pressure), or none for a uniform line.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        widths: Option<Vec<f64>>,
    },
    /// A smooth curve passing through every fit point.
    Spline {
        /// Points the curve passes through, in order.
        points: Vec<Point>,
        /// True if the curve loops back to the first point.
        closed: bool,
        /// Relative pen weight at each point (from pressure), or none for a uniform line.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        widths: Option<Vec<f64>>,
    },
}

impl Path {
    /// A straight-segment polyline through `points`.
    pub fn polyline(points: &[Point], closed: bool) -> Path {
        Path::Polyline {
            vertices: points.iter().map(|&p| Vertex { p, bulge: 0.0 }).collect(),
            closed,
            widths: None,
        }
    }

    /// The same path with per-point pen weights (one per point, each finite and positive).
    pub fn with_widths(self, widths: Vec<f64>) -> Result<Path, PathError> {
        let ok =
            widths.len() == self.points().len() && widths.iter().all(|w| w.is_finite() && *w > 0.0);
        if !ok {
            return Err(PathError::BadWidths);
        }
        Ok(match self {
            Path::Polyline {
                vertices, closed, ..
            } => Path::Polyline {
                vertices,
                closed,
                widths: Some(widths),
            },
            Path::Spline { points, closed, .. } => Path::Spline {
                points,
                closed,
                widths: Some(widths),
            },
        })
    }

    /// Per-point pen weights, if the stroke was recorded with pressure.
    pub fn widths(&self) -> Option<&[f64]> {
        match self {
            Path::Polyline { widths, .. } | Path::Spline { widths, .. } => widths.as_deref(),
        }
    }

    /// Checks the path is usable. Everything stored goes through this.
    pub fn validate(&self) -> Result<(), PathError> {
        let (count, finite) = match self {
            Path::Polyline { vertices, .. } => (
                vertices.len(),
                vertices
                    .iter()
                    .all(|v| v.p.is_finite() && v.bulge.is_finite()),
            ),
            Path::Spline { points, .. } => (points.len(), points.iter().all(|p| p.is_finite())),
        };
        let widths_ok = self
            .widths()
            .is_none_or(|w| w.len() == count && w.iter().all(|x| x.is_finite() && *x > 0.0));
        if !finite {
            Err(PathError::NotFinite)
        } else if count < 2 {
            Err(PathError::TooShort)
        } else if !widths_ok {
            Err(PathError::BadWidths)
        } else {
            Ok(())
        }
    }

    /// The fit or corner points, ignoring curvature.
    pub fn points(&self) -> Vec<Point> {
        match self {
            Path::Polyline { vertices, .. } => vertices.iter().map(|v| v.p).collect(),
            Path::Spline { points, .. } => points.clone(),
        }
    }

    /// The box around the path. Exact for polylines (arcs included). For a spline it is the box
    /// around the fit points; the curve can swing slightly outside it.
    pub fn bounds(&self) -> Option<Rect> {
        let pts = self.points();
        let first = *pts.first()?;
        let mut r = pts
            .iter()
            .fold(Rect::from_points(first, first), |r, &p| r.include(p));
        if let Path::Polyline {
            vertices, closed, ..
        } = self
        {
            for (a, b, bulge) in segments(vertices, *closed) {
                if let Some(arc) = Arc::from_bulge(a, b, bulge) {
                    r = r.union(arc.bounds());
                }
            }
        }
        Some(r)
    }

    /// Length along the path. Exact for polylines; for a spline, the length of the straight
    /// chords between fit points (a lower bound).
    pub fn length(&self) -> f64 {
        match self {
            Path::Polyline {
                vertices, closed, ..
            } => segments(vertices, *closed)
                .map(|(a, b, bulge)| {
                    Arc::from_bulge(a, b, bulge).map_or_else(|| a.distance(b), |arc| arc.length())
                })
                .sum(),
            Path::Spline { points, closed, .. } => {
                let open: f64 = points.windows(2).map(|w| w[0].distance(w[1])).sum();
                match (closed, points.first(), points.last()) {
                    (true, Some(f), Some(l)) => open + l.distance(*f),
                    _ => open,
                }
            }
        }
    }

    /// The path after a transform. A flip reverses the turning direction, so bulges change sign.
    pub fn transformed(&self, t: &Transform) -> Path {
        match self {
            Path::Polyline {
                vertices,
                closed,
                widths,
            } => Path::Polyline {
                vertices: vertices
                    .iter()
                    .map(|v| Vertex {
                        p: t.apply(v.p),
                        bulge: if t.flips() { -v.bulge } else { v.bulge },
                    })
                    .collect(),
                closed: *closed,
                widths: widths.clone(),
            },
            Path::Spline {
                points,
                closed,
                widths,
            } => Path::Spline {
                points: points.iter().map(|&p| t.apply(p)).collect(),
                closed: *closed,
                widths: widths.clone(),
            },
        }
    }
}

fn segments(v: &[Vertex], closed: bool) -> impl Iterator<Item = (Point, Point, f64)> + '_ {
    let n = v.len();
    let count = if closed && n > 1 {
        n
    } else {
        n.saturating_sub(1)
    };
    (0..count).map(move |i| (v[i].p, v[(i + 1) % n].p, v[i].bulge))
}
