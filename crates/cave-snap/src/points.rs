//! The point modes: places that are exactly on a thing, found in its plane and then placed in the world.

use crate::camera::Camera;
use crate::curve::{Flat, dist};
use crate::mode::Mode;
use crate::scene::{Candidate, Geometry, Source, Thing, ThingKind};
use cave_geom::{Arc, Point};
use std::f64::consts::{FRAC_PI_2, TAU};

/// What the point modes need to know.
pub(crate) struct Ctx<'a> {
    pub camera: &'a Camera,
    pub cursor: [f64; 2],
    /// Half the aperture box, pixels.
    pub aperture: f64,
    pub on: &'a dyn Fn(Mode) -> bool,
}

pub(crate) fn candidate(
    ctx: &Ctx,
    thing: &Thing,
    mode: Mode,
    world: [f64; 3],
    seg: usize,
    t: f64,
    rank: Option<f64>,
) -> Option<Candidate> {
    let screen = ctx.camera.project(world)?;
    let d = dist(screen, ctx.cursor);
    let label = if thing.label.is_empty() {
        mode.name().to_string()
    } else {
        format!("{}: {}", mode.name(), thing.label)
    };
    // A Center that fires because the cursor is on the outline ranks behind every other point mode in reach.
    let outline = rank.filter(|r| *r <= ctx.aperture).map(|_| ctx.aperture);
    Some(Candidate {
        mode,
        screen,
        world,
        source: Some(Source {
            id: thing.id.clone(),
            seg,
            t,
        }),
        label,
        dist: outline.map_or(d, |r| r.min(d)),
    })
}

fn arc_of(a: [f64; 2], b: [f64; 2], bulge: f64) -> Option<Arc> {
    Arc::from_bulge(Point::new(a[0], a[1]), Point::new(b[0], b[1]), bulge)
}

/// Whether the quarter point at `angle` lies on an arc.
fn on_arc(start: f64, sweep: f64, angle: f64) -> bool {
    if sweep >= 0.0 {
        (angle - start).rem_euclid(TAU) <= sweep
    } else {
        (start - angle).rem_euclid(TAU) <= -sweep
    }
}

/// The centre and quadrant points of the arc of a circle, with how far along the arc each quadrant is.
fn circle_points(center: [f64; 2], radius: f64, start: f64, sweep: f64) -> Vec<([f64; 2], f64)> {
    (0..4)
        .filter_map(|k| {
            let a = k as f64 * FRAC_PI_2;
            on_arc(start, sweep, a).then(|| {
                let along = if sweep >= 0.0 {
                    (a - start).rem_euclid(TAU)
                } else {
                    (start - a).rem_euclid(TAU)
                } / sweep.abs();
                (
                    [center[0] + radius * a.cos(), center[1] + radius * a.sin()],
                    along,
                )
            })
        })
        .collect()
}

fn centroid(poly: &[[f64; 2]]) -> Option<[f64; 2]> {
    let n = poly.len();
    let (mut a, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let (p, q) = (poly[i], poly[(i + 1) % n]);
        let w = p[0] * q[1] - q[0] * p[1];
        a += w;
        cx += (p[0] + q[0]) * w;
        cy += (p[1] + q[1]) * w;
    }
    (a.abs() > 1e-12).then(|| [cx / (3.0 * a), cy / (3.0 * a)])
}

/// Every point-mode candidate on one thing (the aperture is applied by the caller).
pub(crate) fn of_thing(ctx: &Ctx, thing: &Thing, flat: Option<&Flat>) -> Vec<Candidate> {
    let mut out = Vec::new();
    if let Geometry::Point { at } = &thing.geom {
        let mode = match thing.kind {
            ThingKind::Station => Mode::Station,
            ThingKind::Node | ThingKind::Line => Mode::Node,
            ThingKind::Symbol | ThingKind::Text | ThingKind::Leader => Mode::Insertion,
        };
        if (ctx.on)(mode) {
            out.extend(candidate(ctx, thing, mode, *at, 0, 0.0, None));
        }
        return out;
    }
    let frame = &thing.frame;
    let outline = flat
        .and_then(|f| f.nearest_px(ctx.cursor))
        .map(|(d, _, _)| d);
    let mut add = |mode: Mode, uv: [f64; 2], seg: usize, t: f64, rank: Option<f64>| {
        if (ctx.on)(mode) {
            out.extend(candidate(ctx, thing, mode, frame.world(uv), seg, t, rank));
        }
    };
    match &thing.geom {
        Geometry::Point { .. } => {}
        Geometry::Polyline {
            pts,
            bulges,
            closed,
        } => {
            let n = if *closed && pts.len() > 2 {
                pts.len()
            } else {
                pts.len().saturating_sub(1)
            };
            for (i, p) in pts.iter().enumerate() {
                add(Mode::Endpoint, *p, i, 0.0, None);
            }
            for i in 0..n {
                let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                match arc_of(a, b, bulges.get(i).copied().unwrap_or(0.0)) {
                    Some(arc) => {
                        let mid = arc.start_angle + arc.sweep / 2.0;
                        add(
                            Mode::Midpoint,
                            [
                                arc.center.x + arc.radius * mid.cos(),
                                arc.center.y + arc.radius * mid.sin(),
                            ],
                            i,
                            0.5,
                            None,
                        );
                        let c = [arc.center.x, arc.center.y];
                        add(Mode::Center, c, i, 0.5, outline);
                        for (q, along) in circle_points(c, arc.radius, arc.start_angle, arc.sweep) {
                            add(Mode::Quadrant, q, i, along, None);
                        }
                    }
                    None => add(
                        Mode::Midpoint,
                        [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0],
                        i,
                        0.5,
                        None,
                    ),
                }
            }
            if *closed && let Some(c) = flat.and_then(|f| centroid(&f.uv)) {
                add(Mode::GeometricCenter, c, 0, 0.0, outline);
            }
        }
        Geometry::Spline { pts, closed } => {
            for (i, p) in pts.iter().enumerate() {
                add(Mode::Endpoint, *p, i, 0.0, None);
            }
            if let Some(f) = flat {
                // The middle of the whole curve, by length.
                let half = f.length_uv() / 2.0;
                let mut run = 0.0;
                for j in 0..f.segments() {
                    let (a, b) = f.seg_uv(j);
                    let l = dist(a, b);
                    if run + l >= half && l > 0.0 {
                        let (uv, _, (seg, t)) = f.at(j, (half - run) / l);
                        add(Mode::Midpoint, uv, seg, t, None);
                        break;
                    }
                    run += l;
                }
                if *closed && let Some(c) = centroid(&f.uv) {
                    add(Mode::GeometricCenter, c, 0, 0.0, outline);
                }
            }
        }
        Geometry::Circle { center, radius } => {
            add(Mode::Center, *center, 0, 0.0, outline);
            for (q, along) in circle_points(*center, *radius, 0.0, TAU) {
                add(Mode::Quadrant, q, 0, along, None);
            }
        }
        Geometry::Arc {
            center,
            radius,
            start,
            sweep,
        } => {
            let at = |a: f64| [center[0] + radius * a.cos(), center[1] + radius * a.sin()];
            add(Mode::Endpoint, at(*start), 0, 0.0, None);
            add(Mode::Endpoint, at(start + sweep), 0, 1.0, None);
            add(Mode::Midpoint, at(start + sweep / 2.0), 0, 0.5, None);
            add(Mode::Center, *center, 0, 0.5, outline);
            for (q, along) in circle_points(*center, *radius, *start, *sweep) {
                add(Mode::Quadrant, q, 0, along, None);
            }
        }
    }
    out
}
