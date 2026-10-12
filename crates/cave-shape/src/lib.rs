//! Shaped lines: ornament generated along a drawn curve (ledge hachures, pit ticks, slope fans,
//! flowstone and rimstone scallops). Only the curve is authored; the ornament is derived from it
//! every time, so editing the curve regenerates it. A port of the pure geometry in the old
//! `CsShapeLine` (stations, ticks, fans, scallops); lengths are metres.
//!
//! Ornament lies on one side of the direction of travel: side `+1` is the right of travel (the
//! right normal of tangent `(tx, ty)` is `(ty, -tx)`), `-1` the left.

use serde::Serialize;

/// What an ornament looks like.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// Short lines at right angles to the curve.
    Ticks,
    /// Clusters of three splayed lines.
    Fans {
        /// Angle of the flank lines off the centre line, in degrees.
        splay_deg: f64,
    },
    /// A row of arc bulges, chord by chord.
    Scallops {
        /// How far each arc bows, as a fraction of its chord.
        bulge: f64,
    },
}

/// One shaped style: how its ornament is laid out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    /// The palette type this style is for.
    pub key: &'static str,
    /// Kind of ornament.
    pub kind: Kind,
    /// Distance between ornaments along the curve, metres.
    pub spacing: f64,
    /// Length of a tick or fan line, metres (unused by scallops).
    pub size: f64,
    /// True when the spine itself is scaffolding and is not drawn.
    pub hide_spine: bool,
}

const FT: f64 = 0.3048;

/// Every shaped style. Numbers are the old app's (feet converted to metres).
pub const SHAPES: &[Shape] = &[
    Shape {
        key: "ledge",
        kind: Kind::Ticks,
        spacing: 3.0 * FT,
        size: 2.0 * FT,
        hide_spine: false,
    },
    Shape {
        key: "dripline",
        kind: Kind::Ticks,
        spacing: 0.4,
        size: 0.22,
        hide_spine: false,
    },
    Shape {
        key: "pit",
        kind: Kind::Ticks,
        spacing: 3.0 * FT,
        size: 2.0 * FT,
        hide_spine: false,
    },
    Shape {
        key: "flowstone",
        kind: Kind::Scallops { bulge: 0.5 },
        spacing: 3.0 * FT,
        size: 0.0,
        hide_spine: true,
    },
    Shape {
        key: "rimstone",
        kind: Kind::Scallops { bulge: 0.62 },
        spacing: 2.0 * FT,
        size: 0.0,
        hide_spine: true,
    },
    Shape {
        key: "slope",
        kind: Kind::Fans { splay_deg: 22.0 },
        spacing: 6.0 * FT,
        size: 3.0 * FT,
        hide_spine: true,
    },
];

/// The shaped style for a type, if it has one.
pub fn shape(key: &str) -> Option<&'static Shape> {
    SHAPES.iter().find(|s| s.key == key)
}

/// Flank lines of a fan run a little shorter than the centre line.
const FAN_FLANK: f64 = 0.82;

/// A point on the curve with the unit tangent there.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Station {
    /// Position.
    pub x: f64,
    /// Position.
    pub y: f64,
    /// Unit tangent.
    pub tx: f64,
    /// Unit tangent.
    pub ty: f64,
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

/// Cumulative arc length; for a closed curve the closing piece is one extra entry.
fn cumulative(pts: &[[f64; 2]], closed: bool) -> Vec<f64> {
    let mut cum = vec![0.0];
    for i in 1..pts.len() {
        cum.push(cum[i - 1] + dist(pts[i - 1], pts[i]));
    }
    if closed && pts.len() > 1 {
        cum.push(cum[cum.len() - 1] + dist(pts[pts.len() - 1], pts[0]));
    }
    cum
}

fn point_at(pts: &[[f64; 2]], closed: bool, cum: &[f64], d: f64) -> Station {
    let total = cum[cum.len() - 1];
    if total <= 0.0 {
        return Station {
            x: pts[0][0],
            y: pts[0][1],
            tx: 1.0,
            ty: 0.0,
        };
    }
    let d = if closed {
        d.rem_euclid(total)
    } else {
        d.clamp(0.0, total)
    };
    let mut i = 1;
    while i < cum.len() && cum[i] < d {
        i += 1;
    }
    let i = i.min(cum.len() - 1);
    let a = pts[i - 1];
    let b = if i < pts.len() { pts[i] } else { pts[0] };
    let seg = cum[i] - cum[i - 1];
    let t = if seg > 0.0 {
        (d - cum[i - 1]) / seg
    } else {
        0.0
    };
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let n = dx.hypot(dy);
    let (tx, ty) = if n > 0.0 {
        (dx / n, dy / n)
    } else {
        (1.0, 0.0)
    };
    Station {
        x: a[0] + dx * t,
        y: a[1] + dy * t,
        tx,
        ty,
    }
}

/// Anchor points evenly spaced along the curve: inset half a spacing from open ends, and evenly
/// divided around a closed curve so the seam is clean.
pub fn stations(pts: &[[f64; 2]], closed: bool, spacing: f64) -> Vec<Station> {
    if pts.len() < 2 || spacing <= 0.0 {
        return Vec::new();
    }
    let cum = cumulative(pts, closed);
    let total = cum[cum.len() - 1];
    if total <= 0.0 {
        return Vec::new();
    }
    let n = if closed {
        ((total / spacing).round() as usize).max(4)
    } else {
        ((total / spacing).floor() as usize).max(1)
    };
    let step = total / n as f64;
    (0..n)
        .map(|i| point_at(pts, closed, &cum, step * (i as f64 + 0.5)))
        .collect()
}

fn normal(s: &Station, side: f64) -> (f64, f64) {
    (s.ty * side, -s.tx * side)
}

/// Hachure segments: one short line per station from the curve out to `side`.
pub fn ticks(
    pts: &[[f64; 2]],
    closed: bool,
    spacing: f64,
    len: f64,
    side: f64,
) -> Vec<Vec<[f64; 2]>> {
    stations(pts, closed, spacing)
        .iter()
        .map(|s| {
            let (nx, ny) = normal(s, side);
            vec![[s.x, s.y], [s.x + nx * len, s.y + ny * len]]
        })
        .collect()
}

/// Slope fans: three lines per station, the centre along the normal and the flanks rotated
/// `splay_deg` either way and a little shorter.
pub fn fans(
    pts: &[[f64; 2]],
    closed: bool,
    spacing: f64,
    len: f64,
    side: f64,
    splay_deg: f64,
) -> Vec<Vec<[f64; 2]>> {
    let splay = splay_deg.to_radians();
    let mut out = Vec::new();
    for s in stations(pts, closed, spacing) {
        let (nx, ny) = normal(&s, side);
        for a in [-splay, 0.0, splay] {
            let (ca, sa) = (a.cos(), a.sin());
            let (dx, dy) = (nx * ca - ny * sa, nx * sa + ny * ca);
            let l = if a == 0.0 { len } else { len * FAN_FLANK };
            out.push(vec![[s.x, s.y], [s.x + dx * l, s.y + dy * l]]);
        }
    }
    out
}

/// Scallops: one polyline of arc bulges along the curve, each bowing to `side`. The curve is cut
/// into chords of about `spacing`; each chord becomes a half-ellipse-like arc of height
/// `bulge * chord`.
pub fn scallops(
    pts: &[[f64; 2]],
    closed: bool,
    spacing: f64,
    bulge: f64,
    side: f64,
) -> Vec<Vec<[f64; 2]>> {
    if pts.len() < 2 || spacing <= 0.0 {
        return Vec::new();
    }
    let cum = cumulative(pts, closed);
    let total = cum[cum.len() - 1];
    if total <= 0.0 {
        return Vec::new();
    }
    let n = ((total / spacing).round() as usize).max(1);
    let step = total / n as f64;
    const ARC: usize = 8;
    let mut line = Vec::new();
    for i in 0..n {
        let a = point_at(pts, closed, &cum, step * i as f64);
        let b = point_at(pts, closed, &cum, step * (i + 1) as f64);
        let (cx, cy) = (b.x - a.x, b.y - a.y);
        let chord = cx.hypot(cy);
        let (ux, uy) = if chord > 0.0 {
            (cx / chord, cy / chord)
        } else {
            (1.0, 0.0)
        };
        let (nx, ny) = (uy * side, -ux * side);
        let h = bulge * chord;
        for k in 0..=ARC {
            if i > 0 && k == 0 {
                continue;
            }
            let t = k as f64 / ARC as f64;
            let lift = h * (std::f64::consts::PI * t).sin();
            line.push([a.x + cx * t + nx * lift, a.y + cy * t + ny * lift]);
        }
    }
    vec![line]
}

/// The ornament for a curve in the style of `key`, as polylines, or none when the type has no
/// shape. `side` is `1.0` (right of travel) or `-1.0`; `scale` multiplies the layout lengths.
pub fn ornament(
    key: &str,
    pts: &[[f64; 2]],
    closed: bool,
    side: f64,
    scale: f64,
) -> Vec<Vec<[f64; 2]>> {
    let Some(s) = shape(key) else {
        return Vec::new();
    };
    let scale = if scale > 0.0 { scale } else { 1.0 };
    match s.kind {
        Kind::Ticks => ticks(pts, closed, s.spacing * scale, s.size * scale, side),
        Kind::Fans { splay_deg } => fans(
            pts,
            closed,
            s.spacing * scale,
            s.size * scale,
            side,
            splay_deg,
        ),
        Kind::Scallops { bulge } => scallops(pts, closed, s.spacing * scale, bulge, side),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line10() -> Vec<[f64; 2]> {
        vec![[0.0, 0.0], [10.0, 0.0]]
    }

    #[test]
    fn open_stations_are_inset_half_a_spacing() {
        let s = stations(&line10(), false, 2.0);
        assert_eq!(s.len(), 5);
        assert!((s[0].x - 1.0).abs() < 1e-12 && (s[4].x - 9.0).abs() < 1e-12);
    }

    #[test]
    fn closed_stations_divide_evenly_and_wrap() {
        let sq = vec![[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]];
        let s = stations(&sq, true, 2.0);
        assert_eq!(s.len(), 8);
        assert!((s[0].x - 1.0).abs() < 1e-12);
    }

    #[test]
    fn ticks_go_to_the_right_of_travel() {
        let t = ticks(&line10(), false, 5.0, 1.0, 1.0);
        assert_eq!(t.len(), 2);
        assert!(
            (t[0][1][1] + 1.0).abs() < 1e-12,
            "east-bound, right side is south"
        );
        let l = ticks(&line10(), false, 5.0, 1.0, -1.0);
        assert!((l[0][1][1] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn fans_have_three_lines_and_short_flanks() {
        let f = fans(&line10(), false, 5.0, 1.0, 1.0, 22.0);
        assert_eq!(f.len(), 6);
        let len = |l: &Vec<[f64; 2]>| dist(l[0], l[1]);
        assert!((len(&f[1]) - 1.0).abs() < 1e-12);
        assert!((len(&f[0]) - FAN_FLANK).abs() < 1e-12);
    }

    #[test]
    fn scallops_are_one_continuous_line_that_bows_to_the_side() {
        let s = scallops(&line10(), false, 2.0, 0.5, 1.0);
        assert_eq!(s.len(), 1);
        assert!(s[0].iter().all(|p| p[1] <= 1e-12), "right of east is south");
        let first = s[0][0];
        let last = s[0][s[0].len() - 1];
        assert!(first[0].abs() < 1e-12 && (last[0] - 10.0).abs() < 1e-9);
    }

    #[test]
    fn untyped_and_degenerate_input_make_nothing() {
        assert!(ornament("wall", &line10(), false, 1.0, 1.0).is_empty());
        assert!(ornament("ledge", &[[0.0, 0.0]], false, 1.0, 1.0).is_empty());
        assert!(!ornament("ledge", &line10(), false, 1.0, 1.0).is_empty());
    }

    #[test]
    fn same_curve_same_ornament() {
        let a = ornament("slope", &line10(), false, 1.0, 1.0);
        assert_eq!(a, ornament("slope", &line10(), false, 1.0, 1.0));
    }
}
