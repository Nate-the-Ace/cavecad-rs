//! The relational modes: places that follow from the geometry and, for some, from the last point.

use crate::curve::{Flat, dist};
use crate::mode::Mode;
use crate::points::Ctx;
use crate::scene::{Candidate, Geometry, Source};

fn make(ctx: &Ctx, f: &Flat, mode: Mode, j: usize, s: f64, rank: Option<f64>) -> Option<Candidate> {
    let (_, world, (seg, t)) = f.at(j, s);
    let screen = ctx.camera.project(world)?;
    let d = dist(screen, ctx.cursor);
    let label = if f.thing.label.is_empty() {
        mode.name().to_string()
    } else {
        format!("{}: {}", mode.name(), f.thing.label)
    };
    Some(Candidate {
        mode,
        screen,
        world,
        source: Some(Source {
            id: f.thing.id.clone(),
            seg,
            t,
        }),
        label,
        dist: rank.unwrap_or(d),
    })
}

/// The closest point of each thing to the cursor.
pub(crate) fn nearest(ctx: &Ctx, flats: &[Flat]) -> Vec<Candidate> {
    flats
        .iter()
        .filter_map(|f| {
            f.nearest_px(ctx.cursor)
                .and_then(|(d, j, s)| make(ctx, f, Mode::Nearest, j, s, Some(d)))
        })
        .collect()
}

/// The foot of a perpendicular dropped from `last` (world) on each thing.
pub(crate) fn perpendicular(ctx: &Ctx, flats: &[Flat], last: [f64; 3]) -> Vec<Candidate> {
    let mut out = Vec::new();
    for f in flats {
        let p = f.thing.frame.to_uv(last);
        for j in 0..f.segments() {
            let (a, b) = f.seg_uv(j);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let l2 = dx * dx + dy * dy;
            if l2 < 1e-18 {
                continue;
            }
            let s = ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2;
            if (0.0..=1.0).contains(&s) {
                out.extend(make(ctx, f, Mode::Perpendicular, j, s, None));
            }
        }
    }
    out
}

/// Where a line from `last` (world) touches each curved thing.
pub(crate) fn tangent(ctx: &Ctx, flats: &[Flat], last: [f64; 3]) -> Vec<Candidate> {
    let mut out = Vec::new();
    for f in flats {
        // Only things that are smooth have a tangent: not the corners of a polyline.
        if matches!(f.thing.geom, Geometry::Polyline { .. }) {
            continue;
        }
        let p = f.thing.frame.to_uv(last);
        let n = f.uv.len();
        // The unit direction of travel at each vertex and how far the line from `last` is from it.
        let g: Vec<Option<f64>> = (0..n)
            .map(|i| {
                let (prev, next) = (
                    if i == 0 {
                        if f.closed { n - 1 } else { 0 }
                    } else {
                        i - 1
                    },
                    if i + 1 < n {
                        i + 1
                    } else if f.closed {
                        0
                    } else {
                        n - 1
                    },
                );
                let t = [f.uv[next][0] - f.uv[prev][0], f.uv[next][1] - f.uv[prev][1]];
                let l = (t[0] * t[0] + t[1] * t[1]).sqrt();
                (l > 1e-12).then(|| (t[0] * (f.uv[i][1] - p[1]) - t[1] * (f.uv[i][0] - p[0])) / l)
            })
            .collect();
        for j in 0..f.segments() {
            let k = (j + 1) % n;
            let (Some(a), Some(b)) = (g[j], g[k]) else {
                continue;
            };
            if a == 0.0 || (a < 0.0) != (b < 0.0) {
                let s = if (a - b).abs() < 1e-18 {
                    0.0
                } else {
                    a / (a - b)
                };
                out.extend(make(ctx, f, Mode::Tangent, j, s.clamp(0.0, 1.0), None));
            }
        }
    }
    out
}

/// Where two segments cross, as the fraction along each, or `None`.
pub(crate) fn cross_segments(
    a: ([f64; 2], [f64; 2]),
    b: ([f64; 2], [f64; 2]),
) -> Option<(f64, f64)> {
    let (r, s) = (
        [a.1[0] - a.0[0], a.1[1] - a.0[1]],
        [b.1[0] - b.0[0], b.1[1] - b.0[1]],
    );
    let den = r[0] * s[1] - r[1] * s[0];
    if den.abs() < 1e-12 {
        return None;
    }
    let q = [b.0[0] - a.0[0], b.0[1] - a.0[1]];
    let (t, u) = (
        (q[0] * s[1] - q[1] * s[0]) / den,
        (q[0] * r[1] - q[1] * r[0]) / den,
    );
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some((t, u))
}

/// Where lines through two segments cross, however far out (as fractions along each).
fn cross_lines(a: ([f64; 2], [f64; 2]), b: ([f64; 2], [f64; 2])) -> Option<(f64, f64)> {
    let (r, s) = (
        [a.1[0] - a.0[0], a.1[1] - a.0[1]],
        [b.1[0] - b.0[0], b.1[1] - b.0[1]],
    );
    let den = r[0] * s[1] - r[1] * s[0];
    if den.abs() < 1e-12 {
        return None;
    }
    let q = [b.0[0] - a.0[0], b.0[1] - a.0[1]];
    Some((
        (q[0] * s[1] - q[1] * s[0]) / den,
        (q[0] * r[1] - q[1] * r[0]) / den,
    ))
}

/// Crossings of every two things on the screen. A crossing where the things are also close in the world is an
/// Intersection; any other is an Apparent Intersection.
pub(crate) fn intersections(ctx: &Ctx, flats: &[Flat], tolerance: f64) -> Vec<Candidate> {
    let mut out = Vec::new();
    for (i, a) in flats.iter().enumerate() {
        for b in &flats[i + 1..] {
            for ja in 0..a.segments() {
                let Some(sa) = a.seg_px(ja) else { continue };
                for jb in 0..b.segments() {
                    let Some(sb) = b.seg_px(jb) else { continue };
                    let Some((t, u)) = cross_segments(sa, sb) else {
                        continue;
                    };
                    let (_, wa, (seg, tt)) = a.at(ja, t);
                    let (_, wb, _) = b.at(jb, u);
                    let close = ((wa[0] - wb[0]).powi(2)
                        + (wa[1] - wb[1]).powi(2)
                        + (wa[2] - wb[2]).powi(2))
                    .sqrt()
                        <= tolerance;
                    let mode = if close && (ctx.on)(Mode::Intersection) {
                        Mode::Intersection
                    } else if (ctx.on)(Mode::ApparentIntersection) {
                        Mode::ApparentIntersection
                    } else {
                        continue;
                    };
                    let Some(screen) = ctx.camera.project(wa) else {
                        continue;
                    };
                    let label = if mode == Mode::Intersection {
                        "Intersection".to_string()
                    } else {
                        "Apparent Intersection".to_string()
                    };
                    out.push(Candidate {
                        mode,
                        screen,
                        world: wa,
                        source: Some(Source {
                            id: a.thing.id.clone(),
                            seg,
                            t: tt,
                        }),
                        label,
                        dist: dist(screen, ctx.cursor),
                    });
                }
            }
        }
    }
    out
}

/// Where the straight continuations of the two things nearest the cursor cross (override only).
pub(crate) fn extended_intersection(ctx: &Ctx, flats: &[Flat]) -> Vec<Candidate> {
    let mut near: Vec<(f64, usize, usize, f64)> = flats
        .iter()
        .enumerate()
        .filter_map(|(i, f)| f.nearest_px(ctx.cursor).map(|(d, j, s)| (d, i, j, s)))
        .collect();
    near.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut out = Vec::new();
    if near.len() < 2 {
        return out;
    }
    let (a, b) = (&flats[near[0].1], &flats[near[1].1]);
    let (Some(sa), Some(sb)) = (a.seg_px(near[0].2), b.seg_px(near[1].2)) else {
        return out;
    };
    let Some((t, u)) = cross_lines(sa, sb) else {
        return out;
    };
    let (ua, ub) = (a.seg_uv(near[0].2), b.seg_uv(near[1].2));
    let world_on = |f: &Flat, seg: ([f64; 2], [f64; 2]), s: f64| {
        f.thing.frame.world([
            seg.0[0] + (seg.1[0] - seg.0[0]) * s,
            seg.0[1] + (seg.1[1] - seg.0[1]) * s,
        ])
    };
    let w = world_on(a, ua, t);
    let _ = (ub, u);
    let Some(screen) = ctx.camera.project(w) else {
        return out;
    };
    let (_, _, (seg, tt)) = a.at(near[0].2, t.clamp(0.0, 1.0));
    out.push(Candidate {
        mode: Mode::ExtendedIntersection,
        screen,
        world: w,
        source: Some(Source {
            id: a.thing.id.clone(),
            seg,
            t: tt,
        }),
        label: "Extended Intersection".into(),
        dist: dist(screen, ctx.cursor),
    });
    out
}
