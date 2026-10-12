//! Which drawn lines run *under* another passage, as seen from any direction.
//!
//! Where one passage lies in front of another, the farther one's lines are drawn dim and dashed so
//! the map reads in layers without anyone sorting anything. Everything is judged in the view: a
//! passage is each leg's centreline as a tube of radius the larger of its measured left/right (at
//! least [`MIN_HALF`]); a point is under a leg when it falls inside that tube's outline seen from
//! the camera and the leg's centreline is at least [`CLEAR`] metres nearer the camera there. From
//! straight above this is "higher passages cover lower ones". Legs sharing a station with the line's own passage never count, or a
//! passage would shade itself on a steep stretch.

use crate::Pt;
use cave_id::Id;
use cave_survey::{LegResult, StationLrud, Vec3};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// Narrowest footprint half-width, metres, for a leg nobody measured walls on.
pub const MIN_HALF: f64 = 1.5;
/// How much higher (metres) another passage's centreline must be to cover a point.
pub const CLEAR: f64 = 2.0;
/// Sampling step along a line, metres.
const STEP: f64 = 0.3;

/// Drawn segments `[x1, y1, z1, x2, y2, z2]`, split by whether another passage lies over them.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Shading {
    /// Splay pieces in the open.
    pub splays_over: Vec<[f64; 6]>,
    /// Splay pieces under another passage.
    pub splays_under: Vec<[f64; 6]>,
    /// Wall pieces in the open.
    pub walls_over: Vec<[f64; 6]>,
    /// Wall pieces under another passage.
    pub walls_under: Vec<[f64; 6]>,
    /// Ordinary leg pieces in the open.
    pub legs_over: Vec<[f64; 6]>,
    /// Ordinary leg pieces under another passage.
    pub legs_under: Vec<[f64; 6]>,
}

/// A leg seen from the camera: `[across, up, depth]` with depth growing toward the camera.
struct Foot {
    /// The leg's ends in the world (for a perspective camera).
    wa: Pt,
    wb: Pt,
    a: Pt,
    b: Pt,
    half: f64,
    from: Id,
    to: Id,
}

/// All passages' footprints, built once and asked about many points.
pub struct Footprints {
    feet: Vec<Foot>,
    u: Pt,
    v: Pt,
    w: Pt,
    /// Where a perspective camera is; `None` for an orthographic one (parallel view rays).
    eye: Option<Pt>,
}

/// Looking straight down (plan view).
pub const TOP: Pt = [0.0, 0.0, 1.0];

impl Footprints {
    /// One footprint per leg whose two stations are placed, seen from `view` (the direction from
    /// the cave toward the camera).
    pub fn new(
        legs: &[LegResult],
        pos: &dyn Fn(Id) -> Option<Vec3>,
        lruds: &HashMap<Id, StationLrud>,
        view: Pt,
        eye: Option<Pt>,
    ) -> Self {
        let width = |s: Id| {
            lruds
                .get(&s)
                .map_or(0.0, |l| l.left.unwrap_or(0.0).max(l.right.unwrap_or(0.0)))
        };
        let len = (view[0] * view[0] + view[1] * view[1] + view[2] * view[2]).sqrt();
        let w = if len < 1e-9 {
            TOP
        } else {
            [view[0] / len, view[1] / len, view[2] / len]
        };
        let h = if w[2].abs() < 0.99 {
            [0.0, 0.0, 1.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let u = norm(cross3(h, w));
        let v = cross3(w, u);
        let proj = |p: Pt| [dot(p, u), dot(p, v), dot(p, w)];
        let feet: Vec<Foot> = legs
            .iter()
            .filter_map(|l| {
                let (a, b) = (pos(l.from)?, pos(l.to)?);
                Some(Foot {
                    wa: [a.x, a.y, a.z],
                    wb: [b.x, b.y, b.z],
                    a: proj([a.x, a.y, a.z]),
                    b: proj([b.x, b.y, b.z]),
                    half: width(l.from).max(width(l.to)).max(MIN_HALF),
                    from: l.from,
                    to: l.to,
                })
            })
            .collect();
        Footprints { feet, u, v, w, eye }
    }

    fn covers(&self, p: Pt, own: &HashSet<Id>) -> bool {
        if let Some(eye) = self.eye {
            // Perspective: the sight line from the eye to the point must pass within the leg's
            // radius of the leg, at least CLEAR nearer the eye than the point.
            let len = dist(eye, p);
            if len < 1e-9 {
                return false;
            }
            return self.feet.iter().any(|f| {
                if own.contains(&f.from) || own.contains(&f.to) {
                    return false;
                }
                let (s, d) = closest(eye, p, f.wa, f.wb);
                d <= f.half && s * len <= len - CLEAR
            });
        }
        let p = [dot(p, self.u), dot(p, self.v), dot(p, self.w)];
        self.feet.iter().any(|f| {
            if own.contains(&f.from) || own.contains(&f.to) {
                return false;
            }
            let (dx, dy) = (f.b[0] - f.a[0], f.b[1] - f.a[1]);
            let len2 = dx * dx + dy * dy;
            let t = if len2 < 1e-12 {
                0.0
            } else {
                (((p[0] - f.a[0]) * dx + (p[1] - f.a[1]) * dy) / len2).clamp(0.0, 1.0)
            };
            let (cx, cy) = (f.a[0] + t * dx, f.a[1] + t * dy);
            let d = ((p[0] - cx).powi(2) + (p[1] - cy).powi(2)).sqrt();
            d <= f.half && f.a[2] + t * (f.b[2] - f.a[2]) - p[2] >= CLEAR
        })
    }

    /// Splits the polyline `pts` into pieces in the open and pieces under another passage.
    /// `own` are the stations of the passage the line belongs to.
    pub fn split(
        &self,
        pts: &[Pt],
        own: &HashSet<Id>,
        over: &mut Vec<[f64; 6]>,
        under: &mut Vec<[f64; 6]>,
    ) {
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let len =
                ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt();
            let n = ((len / STEP).ceil() as usize).max(1);
            let at = |i: usize| {
                let t = i as f64 / n as f64;
                [
                    a[0] + t * (b[0] - a[0]),
                    a[1] + t * (b[1] - a[1]),
                    a[2] + t * (b[2] - a[2]),
                ]
            };
            let mut start = 0;
            let mut state = self.covers(at_mid(&at, 0), own);
            for i in 1..=n {
                let s = if i < n {
                    self.covers(at_mid(&at, i), own)
                } else {
                    state
                };
                if s != state || i == n {
                    let (p, q) = (at(start), at(i));
                    let seg = [p[0], p[1], p[2], q[0], q[1], q[2]];
                    if state {
                        under.push(seg)
                    } else {
                        over.push(seg)
                    }
                    start = i;
                    state = s;
                }
            }
        }
    }
}

fn dist(a: Pt, b: Pt) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Closest approach of segment `p1-q1` to segment `p2-q2`: the fraction along the first segment
/// where it happens and the distance there.
fn closest(p1: Pt, q1: Pt, p2: Pt, q2: Pt) -> (f64, f64) {
    let sub = |a: Pt, b: Pt| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let (d1, d2, r) = (sub(q1, p1), sub(q2, p2), sub(p1, p2));
    let (a, e, f) = (dot(d1, d1), dot(d2, d2), dot(d2, r));
    let (mut s, mut t);
    if e < 1e-12 {
        s = if a < 1e-12 {
            0.0
        } else {
            (-dot(d1, r) / a).clamp(0.0, 1.0)
        };
        t = 0.0;
    } else {
        let c = dot(d1, r);
        let b = dot(d1, d2);
        let den = a * e - b * b;
        s = if den > 1e-12 {
            ((b * f - c * e) / den).clamp(0.0, 1.0)
        } else {
            0.0
        };
        t = (b * s + f) / e;
        if t < 0.0 {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else if t > 1.0 {
            t = 1.0;
            s = ((b - c) / a).clamp(0.0, 1.0);
        }
    }
    let c1 = [p1[0] + d1[0] * s, p1[1] + d1[1] * s, p1[2] + d1[2] * s];
    let c2 = [p2[0] + d2[0] * t, p2[1] + d2[1] * t, p2[2] + d2[2] * t];
    (s, dist(c1, c2))
}

fn dot(a: Pt, b: Pt) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross3(a: Pt, b: Pt) -> Pt {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(a: Pt) -> Pt {
    let l = dot(a, a).sqrt().max(1e-12);
    [a[0] / l, a[1] / l, a[2] / l]
}

fn at_mid(at: &impl Fn(usize) -> Pt, i: usize) -> Pt {
    let (p, q) = (at(i), at(i + 1));
    [
        (p[0] + q[0]) / 2.0,
        (p[1] + q[1]) / 2.0,
        (p[2] + q[2]) / 2.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn foot(a: Pt, b: Pt, from: u128, to: u128) -> Foot {
        Foot {
            wa: a,
            wb: b,
            a,
            b,
            half: 2.0,
            from: Id::from_u128(from),
            to: Id::from_u128(to),
        }
    }

    fn fp(feet: Vec<Foot>) -> Footprints {
        // Top view: the basis is (east, north, up)... check by construction below.
        let mut f = Footprints::new(&[], &|_| None, &HashMap::new(), TOP, None);
        f.feet = feet;
        f
    }

    #[test]
    fn a_line_beneath_a_higher_passage_is_under_only_where_they_overlap() {
        // A passage 10 m up runs east along y=0, x 0..10. A low line crosses it at x=5.
        let f = fp(vec![foot([0.0, 0.0, 10.0], [10.0, 0.0, 10.0], 1, 2)]);
        let (mut over, mut under) = (Vec::new(), Vec::new());
        f.split(
            &[[5.0, -6.0, 0.0], [5.0, 6.0, 0.0]],
            &HashSet::new(),
            &mut over,
            &mut under,
        );
        let len = |v: &[[f64; 6]]| v.iter().map(|s| (s[4] - s[1]).abs()).sum::<f64>();
        assert!((len(&under) - 4.0).abs() < 0.4, "about the footprint's 4 m");
        assert!((len(&over) - 8.0).abs() < 0.4);
    }

    #[test]
    fn own_passage_and_equal_height_never_shade() {
        let f = fp(vec![foot([0.0, 0.0, 10.0], [10.0, 0.0, 10.0], 1, 2)]);
        let own: HashSet<Id> = [Id::from_u128(1)].into();
        let (mut over, mut under) = (Vec::new(), Vec::new());
        f.split(
            &[[5.0, -6.0, 0.0], [5.0, 6.0, 0.0]],
            &own,
            &mut over,
            &mut under,
        );
        assert!(under.is_empty());
        f.split(
            &[[5.0, -6.0, 10.0], [5.0, 6.0, 10.0]],
            &HashSet::new(),
            &mut over,
            &mut under,
        );
        assert!(under.is_empty(), "same height is not above");
    }

    #[test]
    fn from_the_side_the_nearer_passage_covers_the_farther_one() {
        // Camera looks from the north (view = +y). A passage along x at y=10 is nearer the camera
        // than a line along x at y=0 at the same height, so the line is under it.
        let mut f = Footprints::new(&[], &|_| None, &HashMap::new(), [0.0, 1.0, 0.0], None);
        let proj = |p: Pt| [dot(p, f.u), dot(p, f.v), dot(p, f.w)];
        let (a, b) = (proj([0.0, 10.0, 0.0]), proj([10.0, 10.0, 0.0]));
        f.feet = vec![foot(a, b, 1, 2)];
        let (mut over, mut under) = (Vec::new(), Vec::new());
        f.split(
            &[[2.0, 0.0, 0.0], [8.0, 0.0, 0.0]],
            &HashSet::new(),
            &mut over,
            &mut under,
        );
        assert!(!under.is_empty() && over.is_empty());
        // And a line nearer the camera than the passage is never under it.
        let (mut over, mut under) = (Vec::new(), Vec::new());
        f.split(
            &[[2.0, 20.0, 0.0], [8.0, 20.0, 0.0]],
            &HashSet::new(),
            &mut over,
            &mut under,
        );
        assert!(under.is_empty());
    }

    #[test]
    fn a_perspective_eye_sees_the_nearer_passage_cover_the_farther() {
        // Eye 100 m up. A passage 10 m up covers the point straight below it, but not one far
        // off to the side whose sight line misses it.
        let mut f = Footprints::new(
            &[],
            &|_| None,
            &HashMap::new(),
            TOP,
            Some([0.0, 0.0, 100.0]),
        );
        f.feet = vec![foot([0.0, 0.0, 10.0], [10.0, 0.0, 10.0], 1, 2)];
        assert!(f.covers([5.0, 0.0, 0.0], &HashSet::new()));
        assert!(!f.covers([5.0, 30.0, 0.0], &HashSet::new()));
        // Sight lines converge on the eye: a point offset 4 m at the floor passes the 2 m tube
        // at 10 m height only 3.6 m off, so a wider offset is clear of it.
        assert!(!f.covers([5.0, 4.0, 0.0], &HashSet::new()));
    }
}
