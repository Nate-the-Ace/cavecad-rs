//! Shaped lines: ornament generated along a drawn curve (ledge hachures, pit ticks, slope fans,
//! flowstone and rimstone scallops, wall glyphs). Only the curve is authored; the ornament is derived
//! from it every time, so editing the curve regenerates it. A port of the pure geometry in the old
//! `CsShapeLine` (stations, ticks, fans, scallop chains, glyph placements, automatic edging), checked
//! against its answers in `tests/golden.rs`; lengths are metres.
//!
//! Ornament lies on one side of the direction of travel: side `+1` is the right of travel (the
//! right normal of tangent `(tx, ty)` is `(ty, -tx)`), `-1` the left.

use cave_fill::Rng;
use serde::Serialize;

const FT: f64 = 0.3048;

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
    /// A chain of arcs, one per chord, each bowing to one side.
    Scallops {
        /// The DXF bulge of each arc: tan of a quarter of its sweep (0.5 is about 106 degrees).
        bulge: f64,
    },
    /// A catalogue symbol placed every spacing along the curve, a little off it.
    Glyphs {
        /// The symbol (a `cave-symbols` key).
        symbol: &'static str,
        /// How far out from the curve, metres.
        offset: f64,
        /// Randomness in position (metres), rotation (degrees) and size (a fraction).
        jitter: (f64, f64, f64),
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
    /// Length of a tick or fan line, metres (unused by scallops and glyphs).
    pub size: f64,
    /// True when the spine itself is scaffolding and is not drawn.
    pub hide_spine: bool,
}

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
    Shape {
        key: "glyphs",
        kind: Kind::Glyphs {
            symbol: "sym-breakdown",
            offset: 1.2 * FT,
            jitter: (0.4 * FT, 20.0, 0.25),
        },
        spacing: 8.0 * FT,
        size: 0.0,
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
    // `Math.round` in the old code rounds halves up; Rust's `round` rounds them away from zero, which is
    // the same for the positive lengths here.
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

/// A chain of vertices along the curve, each segment bulged toward a side.
#[derive(Clone, Debug, PartialEq)]
pub struct ScallopChain {
    /// The vertices.
    pub points: Vec<[f64; 2]>,
    /// The bulge of the segment that starts at each vertex (the last vertex of an open chain starts none).
    pub bulges: Vec<f64>,
    /// True when the chain loops back to its first vertex.
    pub closed: bool,
}

/// The old `CsShapeLine.scallops`: vertices at about `chord` spacing, every segment bulged by
/// `bulge_mag` toward `side` (a positive DXF bulge bows to the right of travel).
pub fn scallop_chain(
    pts: &[[f64; 2]],
    closed: bool,
    chord: f64,
    bulge_mag: f64,
    side: f64,
) -> ScallopChain {
    let empty = ScallopChain {
        points: Vec::new(),
        bulges: Vec::new(),
        closed,
    };
    if pts.len() < 2 || chord <= 0.0 {
        return empty;
    }
    let cum = cumulative(pts, closed);
    let total = cum[cum.len() - 1];
    if total <= 0.0 {
        return empty;
    }
    let n = ((total / chord).round() as usize).max(if closed { 3 } else { 1 });
    let step = total / n as f64;
    let bulge = side * bulge_mag;
    let count = if closed { n } else { n + 1 };
    let mut points = Vec::new();
    let mut bulges = Vec::new();
    for i in 0..count {
        let p = point_at(pts, closed, &cum, step * i as f64);
        points.push([p.x, p.y]);
        bulges.push(if !closed && i == count - 1 {
            0.0
        } else {
            bulge
        });
    }
    ScallopChain {
        points,
        bulges,
        closed,
    }
}

/// Points after the start of the arc from `a` to `b` with DXF `bulge` (positive bows to the right
/// of travel, its centre on the left).
fn bulge_arc(a: [f64; 2], b: [f64; 2], bulge: f64) -> Vec<[f64; 2]> {
    if bulge.abs() < 1e-12 {
        return vec![b];
    }
    let theta = 4.0 * bulge.atan();
    let chord = dist(a, b);
    if chord < 1e-12 {
        return Vec::new();
    }
    let r = chord / (2.0 * (theta.abs() / 2.0).sin());
    let (mx, my) = ((a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0);
    let (ux, uy) = ((b[0] - a[0]) / chord, (b[1] - a[1]) / chord);
    let h = (r * r - (chord / 2.0).powi(2)).max(0.0).sqrt();
    let (lx, ly) = (-uy, ux);
    let major = if theta.abs() > std::f64::consts::PI {
        -1.0
    } else {
        1.0
    };
    let sgn = if bulge > 0.0 { 1.0 } else { -1.0 };
    let (cx, cy) = (mx + lx * h * major * sgn, my + ly * h * major * sgn);
    let a1 = (a[1] - cy).atan2(a[0] - cx);
    let n = ((theta.abs() / (std::f64::consts::PI / 12.0)).ceil() as usize).max(2);
    (1..=n)
        .map(|i| {
            let ang = a1 + theta * i as f64 / n as f64;
            [cx + r * ang.cos(), cy + r * ang.sin()]
        })
        .collect()
}

/// A scallop chain as one polyline of arcs.
pub fn chain_polyline(c: &ScallopChain) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    let n = c.points.len();
    if n < 2 {
        return out;
    }
    out.push(c.points[0]);
    let segments = if c.closed { n } else { n - 1 };
    for i in 0..segments {
        out.extend(bulge_arc(c.points[i], c.points[(i + 1) % n], c.bulges[i]));
    }
    out
}

/// Scallops: one polyline of arcs along the curve, each bowing to `side`.
pub fn scallops(
    pts: &[[f64; 2]],
    closed: bool,
    chord: f64,
    bulge: f64,
    side: f64,
) -> Vec<Vec<[f64; 2]>> {
    let line = chain_polyline(&scallop_chain(pts, closed, chord, bulge, side));
    if line.len() < 2 {
        Vec::new()
    } else {
        vec![line]
    }
}

/// A glyph's place: where, how it is turned (radians, the curve's tangent plus jitter) and its size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    /// Position.
    pub x: f64,
    /// Position.
    pub y: f64,
    /// Rotation.
    pub angle: f64,
    /// Size multiplier.
    pub scale_mul: f64,
}

/// Which way is outside at one glyph station, and whether the station is skipped.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edging {
    /// `1` right of travel, `-1` left.
    pub side: f64,
    /// True when the rock has passage on both faces (a fin): draw nothing there.
    pub skip: bool,
    /// True when the station's own evidence decided the side.
    pub sure: bool,
}

/// How far off the wall's own tangent a station must sit to say which side the cave is on (sine of about 15 degrees).
pub const SIDE_CONFIDENCE: f64 = 0.25;
/// How much farther than the nearest station another may be and still count as "cave over there too".
pub const BOTH_SIDES_RATIO: f64 = 3.0;

fn nearest(stations: &[[f64; 2]], p: [f64; 2]) -> Option<(f64, [f64; 2])> {
    stations
        .iter()
        .map(|s| (dist(*s, p), *s))
        .min_by(|a, b| a.0.total_cmp(&b.0))
}

/// The old `autoSides`: for each glyph station along a wall, which side is outside, judged against
/// the survey stations (the cave is toward the nearest station, so outside is the other way). A
/// station nearly along the wall cannot say, so it takes the nearest confident neighbour's answer;
/// stepping outward getting closer to some other station means a fin, and the glyph is skipped.
pub fn auto_sides(
    pts: &[[f64; 2]],
    closed: bool,
    spacing: f64,
    survey: &[[f64; 2]],
    probe: f64,
) -> Vec<Edging> {
    let st = stations(pts, closed, spacing);
    if survey.is_empty() {
        return st
            .iter()
            .map(|_| Edging {
                side: 1.0,
                skip: false,
                sure: false,
            })
            .collect();
    }
    let reach = if probe > 0.0 { probe } else { 1.0 };
    let mut out: Vec<Edging> = Vec::with_capacity(st.len());
    for s in &st {
        let p = [s.x, s.y];
        let (nx, ny) = (s.ty, -s.tx);
        let Some((near_d, near)) = nearest(survey, p) else {
            out.push(Edging {
                side: 1.0,
                skip: false,
                sure: false,
            });
            continue;
        };
        let (vx, vy) = (near[0] - p[0], near[1] - p[1]);
        let vlen = vx.hypot(vy);
        if vlen <= 0.0 {
            out.push(Edging {
                side: 1.0,
                skip: false,
                sure: false,
            });
            continue;
        }
        let perp = (vx * nx + vy * ny) / vlen;
        let sure = perp.abs() >= SIDE_CONFIDENCE;
        let side = if perp > 0.0 { -1.0 } else { 1.0 };
        let mut skip = false;
        if sure {
            let o = [p[0] + nx * side * reach, p[1] + ny * side * reach];
            if let Some((far_d, far)) = nearest(survey, o) {
                let back = dist(far, p);
                if far_d < back && back <= near_d * BOTH_SIDES_RATIO {
                    skip = true;
                }
            }
        }
        out.push(Edging { side, skip, sure });
    }
    // an unsure station takes the answer of the nearest confident one on either side
    let n = out.len();
    let mut last_sure: Vec<isize> = Vec::with_capacity(n);
    let mut seen = -1;
    for (i, e) in out.iter().enumerate() {
        if e.sure && !e.skip {
            seen = i as isize;
        }
        last_sure.push(seen);
    }
    let mut next_sure = vec![-1isize; n];
    seen = -1;
    for i in (0..n).rev() {
        if out[i].sure && !out[i].skip {
            seen = i as isize;
        }
        next_sure[i] = seen;
    }
    for i in 0..n {
        if out[i].sure {
            continue;
        }
        let (before, after) = (last_sure[i], next_sure[i]);
        out[i].side = if before < 0 && after < 0 {
            1.0
        } else if before < 0 {
            out[after as usize].side
        } else if after < 0 || (i as isize - before) <= (after - i as isize) {
            out[before as usize].side
        } else {
            out[after as usize].side
        };
    }
    out
}

/// The old `glyphPlacementsAuto`: a glyph per station on its own side, with seeded jitter. The four
/// random draws are made for every station, skipped or not, so editing one end of a wall never
/// reshuffles the glyphs at the other.
#[allow(clippy::too_many_arguments)]
pub fn glyph_placements(
    pts: &[[f64; 2]],
    closed: bool,
    spacing: f64,
    offset: f64,
    sides: &[Edging],
    jitter_pos: f64,
    jitter_rot: f64,
    jitter_scale: f64,
    rng: &mut Rng,
) -> Vec<Glyph> {
    let st = stations(pts, closed, spacing);
    let mut out = Vec::new();
    for (i, s) in st.iter().enumerate() {
        let jt = (rng.next_f64() * 2.0 - 1.0) * jitter_pos;
        let jn = (rng.next_f64() * 2.0 - 1.0) * jitter_pos;
        let jr = (rng.next_f64() * 2.0 - 1.0) * jitter_rot;
        let js = (rng.next_f64() * 2.0 - 1.0) * jitter_scale;
        let pick = sides.get(i).copied().unwrap_or(Edging {
            side: 1.0,
            skip: false,
            sure: false,
        });
        if pick.skip {
            continue;
        }
        let (nx, ny) = normal(s, pick.side);
        let (bx, by) = (s.x + nx * offset, s.y + ny * offset);
        out.push(Glyph {
            x: bx + s.tx * jt + nx * jn,
            y: by + s.ty * jt + ny * jn,
            angle: s.ty.atan2(s.tx) + jr,
            scale_mul: (1.0 + js).max(0.05),
        });
    }
    out
}

/// Signed area of a closed path (positive: counter-clockwise).
pub fn signed_area(pts: &[[f64; 2]]) -> f64 {
    let n = pts.len();
    (0..n)
        .map(|i| pts[i][0] * pts[(i + 1) % n][1] - pts[(i + 1) % n][0] * pts[i][1])
        .sum::<f64>()
        / 2.0
}

/// The side that points into a closed path: around a counter-clockwise loop the inside is on the left.
pub fn inward_side(pts: &[[f64; 2]]) -> f64 {
    if signed_area(pts) > 0.0 { -1.0 } else { 1.0 }
}

/// What a shaped line needs to be dressed: which way, a seed for jitter, and the survey's stations
/// in the same coordinates as the curve (for edging a wall's glyphs).
#[derive(Clone, Copy, Debug)]
pub struct Dress<'a> {
    /// True when the person flipped the ornament to the other side.
    pub flip: bool,
    /// Seed for anything random (glyph jitter).
    pub seed: u64,
    /// Survey stations in the curve's coordinates (empty when unknown: glyphs take the right side).
    pub survey: &'a [[f64; 2]],
}

/// The ornament for a curve in the style of `key`, as polylines, or none when the type has no
/// shape. A closed curve puts ticks and fans on its inside (as the old pit tool did) unless flipped.
pub fn ornament(key: &str, pts: &[[f64; 2]], closed: bool, dress: Dress) -> Vec<Vec<[f64; 2]>> {
    let Some(s) = shape(key) else {
        return Vec::new();
    };
    let flip = if dress.flip { -1.0 } else { 1.0 };
    let side = if closed && !matches!(s.kind, Kind::Scallops { .. }) && pts.len() >= 3 {
        inward_side(pts) * flip
    } else {
        flip
    };
    match s.kind {
        Kind::Ticks => ticks(pts, closed, s.spacing, s.size, side),
        Kind::Fans { splay_deg } => fans(pts, closed, s.spacing, s.size, side, splay_deg),
        Kind::Scallops { bulge } => scallops(pts, closed, s.spacing, bulge, side),
        Kind::Glyphs {
            symbol,
            offset,
            jitter,
        } => {
            let Some(sym) = cave_symbols::symbol(symbol) else {
                return Vec::new();
            };
            let mut sides = auto_sides(
                pts,
                closed,
                s.spacing,
                dress.survey,
                if offset > 0.0 { offset } else { s.spacing },
            );
            if dress.flip {
                for e in &mut sides {
                    e.side = -e.side;
                }
            }
            let mut rng = Rng::new(dress.seed);
            let mut out = Vec::new();
            for g in glyph_placements(
                pts,
                closed,
                s.spacing,
                offset,
                &sides,
                jitter.0,
                jitter.1.to_radians(),
                jitter.2,
                &mut rng,
            ) {
                // the symbol's own x runs along the curve: its up is the curve's tangent turned a quarter
                let r = FT * g.scale_mul;
                let (ux, uy) = (-g.angle.sin(), g.angle.cos());
                out.extend(
                    sym.placed([g.x, g.y], [g.x + r * ux, g.y + r * uy])
                        .into_iter()
                        .map(|(mut line, closed)| {
                            if closed {
                                line.push(line[0]);
                            }
                            line
                        }),
                );
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line10() -> Vec<[f64; 2]> {
        vec![[0.0, 0.0], [10.0, 0.0]]
    }

    fn plain(flip: bool) -> Dress<'static> {
        Dress {
            flip,
            seed: 1,
            survey: &[],
        }
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
    fn a_positive_bulge_bows_to_the_right_and_a_half_bulge_is_a_quarter_chord_deep() {
        // the old app calibrated this: bulge +0.5 from (0,0) to (10,0) has its middle at (5,-2.5)
        let arc = bulge_arc([0.0, 0.0], [10.0, 0.0], 0.5);
        let deepest = arc.iter().map(|p| p[1]).fold(f64::MAX, f64::min);
        assert!((deepest + 2.5).abs() < 0.05, "deepest {deepest}");
        let left = bulge_arc([0.0, 0.0], [10.0, 0.0], -0.5);
        assert!(
            left.iter().all(|p| p[1] >= -1e-9),
            "a negative bulge bows left"
        );
    }

    #[test]
    fn scallops_are_one_continuous_line_that_bows_to_the_side() {
        let s = scallops(&line10(), false, 2.0, 0.5, 1.0);
        assert_eq!(s.len(), 1);
        assert!(s[0].iter().all(|p| p[1] <= 1e-9), "right of east is south");
        let first = s[0][0];
        let last = s[0][s[0].len() - 1];
        assert!(first[0].abs() < 1e-12 && (last[0] - 10.0).abs() < 1e-9);
    }

    #[test]
    fn a_closed_pit_puts_its_marks_inside_whichever_way_it_was_drawn() {
        let ccw = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let cw: Vec<[f64; 2]> = ccw.iter().rev().copied().collect();
        for outline in [&ccw, &cw] {
            for t in ornament("pit", outline, true, plain(false)) {
                let tip = t[1];
                assert!(
                    tip[0] > 0.0 && tip[0] < 10.0 && tip[1] > 0.0 && tip[1] < 10.0,
                    "{tip:?} is inside"
                );
            }
        }
        // flipped, they point out
        let out = ornament("pit", &ccw, true, plain(true));
        assert!(
            out.iter()
                .any(|t| t[1][0] < 0.0 || t[1][0] > 10.0 || t[1][1] < 0.0 || t[1][1] > 10.0)
        );
    }

    #[test]
    fn a_wall_next_to_a_passage_gets_its_stones_on_the_far_side_and_none_in_a_fin() {
        // a wall running east along y = 0, with the cave (survey) to its north
        let wall = vec![[0.0, 0.0], [40.0, 0.0]];
        let survey = [[5.0, 5.0], [15.0, 5.0], [25.0, 5.0], [35.0, 5.0]];
        let sides = auto_sides(&wall, false, 4.0, &survey, 1.0);
        assert!(
            sides.iter().all(|e| e.side == 1.0 && !e.skip),
            "outside is south, the right of east"
        );
        // passage on both faces: a fin, so nothing is drawn
        let both = [[5.0, 2.0], [15.0, 2.0], [5.0, -2.0], [15.0, -2.0]];
        let fin = auto_sides(&[[0.0, 0.0], [20.0, 0.0]], false, 4.0, &both, 3.0);
        assert!(fin.iter().any(|e| e.skip), "{fin:?}");
    }

    #[test]
    fn glyphs_are_seeded_and_skipping_one_does_not_move_the_others() {
        let wall = vec![[0.0, 0.0], [40.0, 0.0]];
        let a = ornament(
            "glyphs",
            &wall,
            false,
            Dress {
                flip: false,
                seed: 9,
                survey: &[],
            },
        );
        assert_eq!(
            a,
            ornament(
                "glyphs",
                &wall,
                false,
                Dress {
                    flip: false,
                    seed: 9,
                    survey: &[]
                }
            )
        );
        assert_ne!(
            a,
            ornament(
                "glyphs",
                &wall,
                false,
                Dress {
                    flip: false,
                    seed: 10,
                    survey: &[]
                }
            )
        );
        assert!(!a.is_empty());
    }

    #[test]
    fn untyped_and_degenerate_input_make_nothing() {
        assert!(ornament("wall", &line10(), false, plain(false)).is_empty());
        assert!(ornament("ledge", &[[0.0, 0.0]], false, plain(false)).is_empty());
        assert!(!ornament("ledge", &line10(), false, plain(false)).is_empty());
    }

    #[test]
    fn same_curve_same_ornament() {
        let a = ornament("slope", &line10(), false, plain(false));
        assert_eq!(a, ornament("slope", &line10(), false, plain(false)));
    }
}
