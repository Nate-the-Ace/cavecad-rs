//! Guide lines: the continuation of acquired ends, tracking and polar lines, and parallels.

use crate::camera::Camera;
use crate::curve::dist;
use crate::mode::Mode;
use crate::scene::{
    Acquired, Candidate, Continuation, Frame, Guide, GuideKind, Scene, Session, Settings, Thing,
    dot,
};
use std::f64::consts::TAU;

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: V3, b: V3, k: f64) -> V3 {
    [a[0] + b[0] * k, a[1] + b[1] * k, a[2] + b[2] * k]
}

/// What a guide is made of.
pub(crate) enum Shape {
    /// A straight line through `origin`; `forward_only` stops it at the origin (an extension).
    Line {
        origin: V3,
        dir: V3,
        forward_only: bool,
    },
    /// A circle's worth of arc.
    Arc {
        frame: Frame,
        center: [f64; 2],
        radius: f64,
        from: f64,
        ccw: bool,
    },
}

pub(crate) struct Line {
    pub kind: GuideKind,
    pub shape: Shape,
    /// Where measuring starts for the tooltip (the acquired point).
    pub base: V3,
    pub label: &'static str,
}

/// The closest approach of the world line `origin + dir * k` to the pixel's ray: the line's parameter and the
/// distance between them on the screen.
fn closest_on_line(camera: &Camera, cursor: [f64; 2], origin: V3, dir: V3) -> Option<(f64, f64)> {
    let a = camera.project(origin)?;
    let b = camera.project(add(origin, dir, 1.0))?;
    let e = [b[0] - a[0], b[1] - a[1]];
    let l = (e[0] * e[0] + e[1] * e[1]).sqrt();
    if l < 1e-9 {
        return None;
    }
    let e = [e[0] / l, e[1] / l];
    let perp = ((cursor[0] - a[0]) * e[1] - (cursor[1] - a[1]) * e[0]).abs();
    // The parameter from the ray's closest approach (exact under perspective).
    let (ro, rd) = camera.ray(cursor)?;
    let w0 = sub(origin, ro);
    let (bb, d, ee) = (dot(dir, rd), dot(dir, w0), dot(rd, w0));
    let den = 1.0 - bb * bb;
    let k = if den.abs() < 1e-12 {
        0.0
    } else {
        (bb * ee - d) / den
    };
    Some((k, perp))
}

fn angle_label(plane: Option<&Frame>, base: V3, at: V3, name: &str) -> String {
    let d =
        ((at[0] - base[0]).powi(2) + (at[1] - base[1]).powi(2) + (at[2] - base[2]).powi(2)).sqrt();
    match plane {
        Some(f) => {
            let (a, b) = (f.to_uv(base), f.to_uv(at));
            let deg = (b[1] - a[1])
                .atan2(b[0] - a[0])
                .to_degrees()
                .rem_euclid(360.0);
            format!("{name}: {d:.2} < {deg:.0}\u{b0}")
        }
        None => format!("{name}: {d:.2}"),
    }
}

/// The straight-line guides that follow from the session.
pub(crate) fn lines(
    scene: &Scene,
    settings: &Settings,
    session: &Session,
    accept: &dyn Fn(Mode) -> bool,
) -> Vec<Line> {
    let mut out = Vec::new();
    for a in &session.acquired {
        match a {
            Acquired::Extension { at, cont, .. } if accept(Mode::Extension) => match cont {
                Continuation::Line { dir } => out.push(Line {
                    kind: GuideKind::Extension,
                    shape: Shape::Line {
                        origin: *at,
                        dir: *dir,
                        forward_only: true,
                    },
                    base: *at,
                    label: "Extension",
                }),
                Continuation::Arc {
                    frame,
                    center,
                    radius,
                    angle,
                    ccw,
                } => out.push(Line {
                    kind: GuideKind::Extension,
                    shape: Shape::Arc {
                        frame: *frame,
                        center: *center,
                        radius: *radius,
                        from: *angle,
                        ccw: *ccw,
                    },
                    base: *at,
                    label: "Extension",
                }),
            },
            Acquired::Parallel { dir, .. } if accept(Mode::Parallel) => {
                if let Some(last) = session.last_point {
                    out.push(Line {
                        kind: GuideKind::Parallel,
                        shape: Shape::Line {
                            origin: last,
                            dir: *dir,
                            forward_only: false,
                        },
                        base: last,
                        label: "Parallel",
                    });
                }
            }
            _ => {}
        }
    }
    if settings.tracking
        && let Some(plane) = &scene.plane
    {
        let mut bases: Vec<V3> = session.track_points.clone();
        bases.extend(session.from_base);
        for a in &session.acquired {
            if let Acquired::Track { at } = a {
                bases.push(*at);
            }
        }
        let mut angles: Vec<f64> = vec![0.0, 90.0];
        for inc in &settings.polar {
            if *inc > 0.0 {
                let mut k = 0.0;
                while k < 180.0 - 1e-9 {
                    angles.push(k);
                    k += inc;
                }
            }
        }
        angles.sort_by(f64::total_cmp);
        angles.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        for base in bases {
            for deg in &angles {
                let r = deg.to_radians();
                let dir = plane.dir([r.cos(), r.sin()]);
                let axis = (deg % 90.0).abs() < 1e-6;
                out.push(Line {
                    kind: if axis {
                        GuideKind::Track
                    } else {
                        GuideKind::Polar
                    },
                    shape: Shape::Line {
                        origin: base,
                        dir,
                        forward_only: false,
                    },
                    base,
                    label: if axis { "Tracking" } else { "Polar" },
                });
            }
        }
    }
    out
}

/// A guide as drawn: its pixels.
fn draw(camera: &Camera, l: &Line) -> Vec<[f64; 2]> {
    match &l.shape {
        Shape::Line {
            origin,
            dir,
            forward_only,
        } => {
            let (a, b) = (
                camera.project(*origin),
                camera.project(add(*origin, *dir, 1.0)),
            );
            let (Some(a), Some(b)) = (a, b) else {
                return Vec::new();
            };
            let e = [b[0] - a[0], b[1] - a[1]];
            let n = (e[0] * e[0] + e[1] * e[1]).sqrt().max(1e-9);
            let reach = 6000.0;
            let (e, back) = (
                [e[0] / n * reach, e[1] / n * reach],
                if *forward_only { 0.0 } else { 1.0 },
            );
            vec![
                [a[0] - e[0] * back, a[1] - e[1] * back],
                [a[0] + e[0], a[1] + e[1]],
            ]
        }
        Shape::Arc {
            frame,
            center,
            radius,
            from,
            ccw,
        } => {
            let sign = if *ccw { 1.0 } else { -1.0 };
            (0..=180)
                .filter_map(|i| {
                    let a = from + sign * TAU * i as f64 / 180.0;
                    camera.project(
                        frame.world([center[0] + radius * a.cos(), center[1] + radius * a.sin()]),
                    )
                })
                .collect()
        }
    }
}

/// How a guide pulls on the cursor: the candidate on it nearest the cursor, if close enough, and the guide to draw.
pub(crate) fn pull(
    scene: &Scene,
    settings: &Settings,
    l: &Line,
    cursor: [f64; 2],
) -> Option<(Candidate, Guide)> {
    let camera = &scene.camera;
    let (world, perp) = match &l.shape {
        Shape::Line {
            origin,
            dir,
            forward_only,
        } => {
            let (k, perp) = closest_on_line(camera, cursor, *origin, *dir)?;
            if *forward_only && k < 0.0 {
                return None;
            }
            (add(*origin, *dir, k), perp)
        }
        Shape::Arc {
            frame,
            center,
            radius,
            from,
            ccw,
        } => {
            let sign = if *ccw { 1.0 } else { -1.0 };
            let mut best: Option<(V3, f64)> = None;
            for i in 0..=720 {
                let a = from + sign * TAU * i as f64 / 720.0;
                let w = frame.world([center[0] + radius * a.cos(), center[1] + radius * a.sin()]);
                if let Some(p) = camera.project(w) {
                    let d = dist(p, cursor);
                    if best.is_none_or(|(_, bd)| d < bd) {
                        best = Some((w, d));
                    }
                }
            }
            best?
        }
    };
    if perp > settings.aperture {
        return None;
    }
    let screen = camera.project(world)?;
    let mode = match l.kind {
        GuideKind::Extension => Mode::Extension,
        GuideKind::Parallel => Mode::Parallel,
        _ => Mode::Tracking,
    };
    let label = angle_label(scene.plane.as_ref(), l.base, world, l.label);
    let dir = match &l.shape {
        Shape::Line { dir, .. } => Some(*dir),
        Shape::Arc { .. } => None,
    };
    let origin = match &l.shape {
        Shape::Line { origin, .. } => *origin,
        Shape::Arc { .. } => l.base,
    };
    let guide = Guide {
        kind: l.kind,
        origin,
        dir,
        px: draw(camera, l),
        label: label.clone(),
    };
    Some((
        Candidate {
            mode,
            screen,
            world,
            source: None,
            label,
            dist: perp,
        },
        guide,
    ))
}

/// Where two straight guides cross (world), both extended without end, in the drawing plane.
pub(crate) fn cross(plane: &Frame, a: &Line, b: &Line) -> Option<V3> {
    let (
        Shape::Line {
            origin: oa,
            dir: da,
            ..
        },
        Shape::Line {
            origin: ob,
            dir: db,
            ..
        },
    ) = (&a.shape, &b.shape)
    else {
        return None;
    };
    let (p, d1) = (plane.to_uv(*oa), plane.to_uv(add(*oa, *da, 1.0)));
    let (q, d2) = (plane.to_uv(*ob), plane.to_uv(add(*ob, *db, 1.0)));
    let (r, s) = ([d1[0] - p[0], d1[1] - p[1]], [d2[0] - q[0], d2[1] - q[1]]);
    let den = r[0] * s[1] - r[1] * s[0];
    if den.abs() < 1e-9 {
        return None;
    }
    let t = ((q[0] - p[0]) * s[1] - (q[1] - p[1]) * s[0]) / den;
    Some(plane.world([p[0] + r[0] * t, p[1] + r[1] * t]))
}

/// What hovering at `cursor` could acquire from a thing: an end to extend from and a segment to run parallel to.
pub(crate) fn acquirable(
    thing: &Thing,
    flat: &crate::curve::Flat,
    cursor: [f64; 2],
    aperture: f64,
) -> Vec<Acquired> {
    use crate::scene::Geometry;
    let mut out = Vec::new();
    let frame = &thing.frame;
    let end_of = |uv_end: [f64; 2], uv_prev: [f64; 2]| -> Option<Acquired> {
        let at = frame.world(uv_end);
        let d = [uv_end[0] - uv_prev[0], uv_end[1] - uv_prev[1]];
        let l = (d[0] * d[0] + d[1] * d[1]).sqrt();
        (l > 1e-12).then(|| Acquired::Extension {
            at,
            id: thing.id.clone(),
            cont: Continuation::Line {
                dir: frame.dir([d[0] / l, d[1] / l]),
            },
        })
    };
    let near = |uv: [f64; 2]| frame_px(flat, uv).is_some_and(|p| dist(p, cursor) <= aperture);
    match &thing.geom {
        Geometry::Polyline {
            pts,
            bulges,
            closed: false,
        } if pts.len() >= 2 => {
            let n = pts.len();
            if near(pts[0]) {
                out.extend(
                    polyline_end(thing, pts, bulges, true).or_else(|| end_of(pts[0], pts[1])),
                );
            }
            if near(pts[n - 1]) {
                out.extend(
                    polyline_end(thing, pts, bulges, false)
                        .or_else(|| end_of(pts[n - 1], pts[n - 2])),
                );
            }
        }
        Geometry::Spline { pts, closed: false } if pts.len() >= 2 && flat.uv.len() >= 2 => {
            let m = flat.uv.len();
            if near(flat.uv[0]) {
                out.extend(end_of(flat.uv[0], flat.uv[1]));
            }
            if near(flat.uv[m - 1]) {
                out.extend(end_of(flat.uv[m - 1], flat.uv[m - 2]));
            }
        }
        Geometry::Arc {
            center,
            radius,
            start,
            sweep,
        } => {
            let at = |a: f64| [center[0] + radius * a.cos(), center[1] + radius * a.sin()];
            if near(at(start + sweep)) {
                out.push(Acquired::Extension {
                    at: frame.world(at(start + sweep)),
                    id: thing.id.clone(),
                    cont: Continuation::Arc {
                        frame: *frame,
                        center: *center,
                        radius: *radius,
                        angle: start + sweep,
                        ccw: *sweep > 0.0,
                    },
                });
            }
            if near(at(*start)) {
                out.push(Acquired::Extension {
                    at: frame.world(at(*start)),
                    id: thing.id.clone(),
                    cont: Continuation::Arc {
                        frame: *frame,
                        center: *center,
                        radius: *radius,
                        angle: *start,
                        ccw: *sweep < 0.0,
                    },
                });
            }
        }
        _ => {}
    }
    // A straight segment under the cursor can be run parallel to.
    if let Some((d, j, _)) = flat.nearest_px(cursor)
        && d <= aperture
        && !matches!(thing.geom, Geometry::Circle { .. } | Geometry::Arc { .. })
    {
        let (a, b) = flat.seg_uv(j);
        let v = [b[0] - a[0], b[1] - a[1]];
        let l = (v[0] * v[0] + v[1] * v[1]).sqrt();
        if l > 1e-12 && straight_segment(thing, flat.src[j].0) {
            out.push(Acquired::Parallel {
                dir: frame.dir([v[0] / l, v[1] / l]),
                id: thing.id.clone(),
            });
        }
    }
    out
}

fn straight_segment(thing: &Thing, seg: usize) -> bool {
    use crate::scene::Geometry;
    match &thing.geom {
        Geometry::Polyline { bulges, .. } => bulges.get(seg).copied().unwrap_or(0.0) == 0.0,
        Geometry::Spline { .. } => false,
        _ => false,
    }
}

fn frame_px(flat: &crate::curve::Flat, uv: [f64; 2]) -> Option<[f64; 2]> {
    // The screen position of a point of the thing's plane: found by matching the flat vertices (ends are vertices).
    let i = flat.uv.iter().position(|p| dist(*p, uv) < 1e-9)?;
    flat.px[i]
}

/// The continuation of the first or last segment of a polyline when that segment is an arc.
fn polyline_end(
    thing: &Thing,
    pts: &[[f64; 2]],
    bulges: &[f64],
    at_start: bool,
) -> Option<Acquired> {
    use cave_geom::{Arc, Point};
    let n = pts.len();
    let (i, j) = if at_start { (0, 1) } else { (n - 2, n - 1) };
    let bulge = bulges.get(i).copied().unwrap_or(0.0);
    let arc = Arc::from_bulge(
        Point::new(pts[i][0], pts[i][1]),
        Point::new(pts[j][0], pts[j][1]),
        bulge,
    )?;
    let (angle, ccw) = if at_start {
        (arc.start_angle, arc.sweep < 0.0)
    } else {
        (arc.start_angle + arc.sweep, arc.sweep > 0.0)
    };
    let end = if at_start { pts[0] } else { pts[n - 1] };
    Some(Acquired::Extension {
        at: thing.frame.world(end),
        id: thing.id.clone(),
        cont: Continuation::Arc {
            frame: thing.frame,
            center: [arc.center.x, arc.center.y],
            radius: arc.radius,
            angle,
            ccw,
        },
    })
}

/// A guide as pixels (for finding where it crosses the things under the cursor).
pub(crate) fn pull_px(camera: &Camera, l: &Line) -> Vec<[f64; 2]> {
    draw(camera, l)
}
