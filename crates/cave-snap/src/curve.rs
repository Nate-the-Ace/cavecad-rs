//! Things flattened to polylines in their plane, and projected to the screen.

use crate::camera::Camera;
use crate::scene::{Geometry, Thing};
use cave_geom::{Arc, Point, spline};
use std::f64::consts::TAU;

/// Samples per span of a spline.
const SPLINE_PER: usize = 8;
/// How far the flattened curve may stray from the true one on the screen, pixels.
const TOLERANCE_PX: f64 = 0.25;

/// A curve as a polyline: in its plane, in the world, and on the screen.
pub(crate) struct Flat<'a> {
    pub thing: &'a Thing,
    /// Vertices in the thing's plane.
    pub uv: Vec<[f64; 2]>,
    /// For each vertex, the source segment and parameter it came from.
    pub src: Vec<(usize, f64)>,
    /// The last vertex joins the first.
    pub closed: bool,
    /// Vertices on the screen (`None` when behind the camera).
    pub px: Vec<Option<[f64; 2]>>,
}

fn arc_steps(radius: f64, sweep: f64, px_per_unit: f64) -> usize {
    let r_px = (radius * px_per_unit).max(0.5);
    let step = 2.0 * (1.0 - (TOLERANCE_PX / r_px).min(1.0)).acos().max(1e-3);
    ((sweep.abs() / step).ceil() as usize).clamp(4, 2000)
}

/// Pixels per world unit near a thing (from projecting two points a unit apart in its plane).
fn px_per_unit(thing: &Thing, camera: &Camera, at: [f64; 2]) -> f64 {
    let (a, b) = (
        thing.frame.world(at),
        thing.frame.world([at[0] + 1.0, at[1]]),
    );
    match (camera.project(a), camera.project(b)) {
        (Some(p), Some(q)) => ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2))
            .sqrt()
            .max(1e-6),
        _ => 1.0,
    }
}

impl<'a> Flat<'a> {
    /// Flattens a curve (a point has no curve: `None`).
    pub fn of(thing: &'a Thing, camera: &Camera) -> Option<Flat<'a>> {
        let mut uv = Vec::new();
        let mut src = Vec::new();
        let mut closed = false;
        match &thing.geom {
            Geometry::Point { .. } => return None,
            Geometry::Polyline {
                pts,
                bulges,
                closed: c,
            } => {
                if pts.len() < 2 {
                    return None;
                }
                closed = *c && pts.len() > 2;
                let n = if closed { pts.len() } else { pts.len() - 1 };
                let scale = px_per_unit(thing, camera, pts[0]);
                for i in 0..n {
                    let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                    let bulge = bulges.get(i).copied().unwrap_or(0.0);
                    match Arc::from_bulge(Point::new(a[0], a[1]), Point::new(b[0], b[1]), bulge) {
                        Some(arc) => {
                            let k = arc_steps(arc.radius, arc.sweep, scale);
                            for s in 0..k {
                                let ang = arc.start_angle + arc.sweep * s as f64 / k as f64;
                                uv.push(if s == 0 {
                                    a
                                } else {
                                    [
                                        arc.center.x + arc.radius * ang.cos(),
                                        arc.center.y + arc.radius * ang.sin(),
                                    ]
                                });
                                src.push((i, s as f64 / k as f64));
                            }
                        }
                        None => {
                            uv.push(a);
                            src.push((i, 0.0));
                        }
                    }
                }
                if !closed {
                    uv.push(pts[pts.len() - 1]);
                    src.push((pts.len() - 2, 1.0));
                }
            }
            Geometry::Spline { pts, closed: c } => {
                if pts.len() < 2 {
                    return None;
                }
                closed = *c && pts.len() > 2;
                let samples = spline::smooth(pts, closed, SPLINE_PER);
                let spans = if pts.len() < 3 { 1 } else { SPLINE_PER };
                for (i, s) in samples.iter().enumerate() {
                    uv.push(*s);
                    src.push(if pts.len() < 3 {
                        (0, i as f64 / (samples.len() - 1).max(1) as f64)
                    } else {
                        (i / spans, (i % spans) as f64 / spans as f64)
                    });
                }
            }
            Geometry::Circle { center, radius } => {
                if radius.is_nan() || *radius <= 0.0 {
                    return None;
                }
                closed = true;
                let k = arc_steps(*radius, TAU, px_per_unit(thing, camera, *center));
                for s in 0..k {
                    let ang = TAU * s as f64 / k as f64;
                    uv.push([
                        center[0] + radius * ang.cos(),
                        center[1] + radius * ang.sin(),
                    ]);
                    src.push((0, s as f64 / k as f64));
                }
            }
            Geometry::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                if radius.is_nan() || *radius <= 0.0 || *sweep == 0.0 {
                    return None;
                }
                let k = arc_steps(*radius, *sweep, px_per_unit(thing, camera, *center));
                for s in 0..=k {
                    let ang = start + sweep * s as f64 / k as f64;
                    uv.push([
                        center[0] + radius * ang.cos(),
                        center[1] + radius * ang.sin(),
                    ]);
                    src.push((0, s as f64 / k as f64));
                }
            }
        }
        let px = uv
            .iter()
            .map(|p| camera.project(thing.frame.world(*p)))
            .collect();
        Some(Flat {
            thing,
            uv,
            src,
            closed,
            px,
        })
    }

    /// How many flat segments there are (the closing one included).
    pub fn segments(&self) -> usize {
        if self.closed {
            self.uv.len()
        } else {
            self.uv.len().saturating_sub(1)
        }
    }

    fn next(&self, j: usize) -> usize {
        (j + 1) % self.uv.len()
    }

    /// Flat segment `j` in the plane.
    pub fn seg_uv(&self, j: usize) -> ([f64; 2], [f64; 2]) {
        (self.uv[j], self.uv[self.next(j)])
    }

    /// Flat segment `j` on the screen, when both ends are in front of the camera.
    pub fn seg_px(&self, j: usize) -> Option<([f64; 2], [f64; 2])> {
        Some((self.px[j]?, self.px[self.next(j)]?))
    }

    /// The point at fraction `s` along flat segment `j`: plane position, world position, and where in the source.
    pub fn at(&self, j: usize, s: f64) -> ([f64; 2], [f64; 3], (usize, f64)) {
        let (a, b) = self.seg_uv(j);
        let uv = [a[0] + (b[0] - a[0]) * s, a[1] + (b[1] - a[1]) * s];
        let (sa, sb) = (self.src[j], self.src[self.next(j)]);
        // Within one source segment the parameter runs on; into the next one it runs out to this one's end.
        let source = if sa.0 == sb.0 && sb.1 > sa.1 {
            (sa.0, sa.1 + (sb.1 - sa.1) * s)
        } else {
            (sa.0, sa.1 + (1.0 - sa.1) * s)
        };
        (uv, self.thing.frame.world(uv), source)
    }

    /// The smallest distance on the screen from `p` to the curve, with the segment and fraction it is nearest at.
    pub fn nearest_px(&self, p: [f64; 2]) -> Option<(f64, usize, f64)> {
        let mut best: Option<(f64, usize, f64)> = None;
        for j in 0..self.segments() {
            let Some((a, b)) = self.seg_px(j) else {
                continue;
            };
            let (d, s) = point_segment(p, a, b);
            if best.is_none_or(|(bd, _, _)| d < bd) {
                best = Some((d, j, s));
            }
        }
        best
    }

    /// Length along the flat curve in the plane.
    pub fn length_uv(&self) -> f64 {
        (0..self.segments())
            .map(|j| dist(self.seg_uv(j).0, self.seg_uv(j).1))
            .sum()
    }
}

pub(crate) fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// Distance from `p` to segment `ab`, and the fraction along it of the nearest point.
pub(crate) fn point_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> (f64, f64) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let s = if l2 < 1e-18 {
        0.0
    } else {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0)
    };
    (dist(p, [a[0] + dx * s, a[1] + dy * s]), s)
}
