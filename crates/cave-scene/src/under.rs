//! Which drawn lines run *under* another passage.
//!
//! Where one passage lies above another in plan, the lower one's lines are drawn dim and dashed so
//! the map reads in layers without anyone sorting anything. A passage's footprint is each leg's
//! centreline widened to the larger of its measured left/right (at least [`MIN_HALF`]); a point is
//! under a leg when it falls inside that footprint and the leg's centreline is at least [`CLEAR`]
//! metres higher there. Legs sharing a station with the line's own passage never count, or a
//! passage would shade itself on a steep stretch.

use crate::Pt;
use cave_id::Id;
use cave_survey::{LegResult, StationLrud, Vec3};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

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

struct Foot {
    a: Pt,
    b: Pt,
    half: f64,
    from: Id,
    to: Id,
}

/// All passages' footprints, built once and asked about many points.
pub struct Footprints(Vec<Foot>);

impl Footprints {
    /// One footprint per leg whose two stations are placed.
    pub fn new(
        legs: &[LegResult],
        pos: &dyn Fn(Id) -> Option<Vec3>,
        lruds: &HashMap<Id, StationLrud>,
        placed: &BTreeMap<Id, Vec3>,
    ) -> Self {
        let width = |s: Id| {
            lruds
                .get(&s)
                .map_or(0.0, |l| l.left.unwrap_or(0.0).max(l.right.unwrap_or(0.0)))
        };
        let _ = placed;
        Footprints(
            legs.iter()
                .filter_map(|l| {
                    let (a, b) = (pos(l.from)?, pos(l.to)?);
                    Some(Foot {
                        a: [a.x, a.y, a.z],
                        b: [b.x, b.y, b.z],
                        half: width(l.from).max(width(l.to)).max(MIN_HALF),
                        from: l.from,
                        to: l.to,
                    })
                })
                .collect(),
        )
    }

    fn covers(&self, p: Pt, own: &HashSet<Id>) -> bool {
        self.0.iter().any(|f| {
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
            a,
            b,
            half: 2.0,
            from: Id::from_u128(from),
            to: Id::from_u128(to),
        }
    }

    #[test]
    fn a_line_beneath_a_higher_passage_is_under_only_where_they_overlap() {
        // A passage 10 m up runs east along y=0, x 0..10. A low line crosses it at x=5.
        let f = Footprints(vec![foot([0.0, 0.0, 10.0], [10.0, 0.0, 10.0], 1, 2)]);
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
        let f = Footprints(vec![foot([0.0, 0.0, 10.0], [10.0, 0.0, 10.0], 1, 2)]);
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
}
