//! Anchoring a plan stroke as one path, so it stays on the passage it was drawn on where
//! passages on different levels overlap in plan.
//!
//! Per point nearest-leg anchoring ([`anchor_stroke`](crate::anchor_stroke)) lets a wall traced
//! along an upper passage grab the lower passage wherever that is closer in plan. Here the whole
//! stroke picks one chain of legs: staying on a leg is free, stepping to a connected leg is cheap,
//! jumping to an unconnected passage or another height is dear.

use crate::anchored::{
    AnchorError, AnchoredPath, Frame, along, basis, build_path, check_stroke, distance_to_segment,
    usable,
};
use cave_id::Id;
use std::collections::{HashMap, VecDeque};

/// Cost, in metres of plan distance, of each hop along the survey between two legs.
const HOP_COST: f64 = 0.5;
/// Hops along the survey beyond which two legs count as different passages.
const HOP_CAP: u32 = 8;
/// Cost of jumping to a leg more than `HOP_CAP` hops away, or not connected at all.
const UNCONNECTED_COST: f64 = 50.0;
/// Cost per metre of height between the leg left and the leg joined, when they share no station.
const Z_COST: f64 = 2.0;
/// Legs farther than this beyond a point's nearest leg are not considered for that point.
const SEARCH_RADIUS: f64 = 15.0;
/// Most legs considered per point.
const MAX_CANDIDATES: usize = 16;
/// An other-level leg this close (plan metres) to the stroke's first point makes its start
/// ambiguous: the stroke may belong to either level.
pub const AMBIGUOUS_RADIUS: f64 = 4.0;
/// Height (metres) between two legs beyond which they are different levels even when the survey
/// joins them nearby, as levels in a real cave usually are.
const LEVEL_STEP: f64 = 3.0;

/// True when legs `i` and `j` belong to different levels: not connected within `HOP_CAP`, or
/// more than `LEVEL_STEP` apart in height (`zi`, `zj`).
fn other_levels(hops: &mut Hops, i: usize, j: usize, zi: f64, zj: f64) -> bool {
    hops.between(i, j).is_none() || (zi - zj).abs() > LEVEL_STEP
}

fn mid_z(f: &Frame) -> f64 {
    (f.za + f.zb) / 2.0
}

/// How many hops along the survey separate two legs (legs sharing a station are one hop apart).
pub struct Hops<'a> {
    frames: &'a [Frame],
    at: HashMap<Id, Vec<usize>>,
    reach: HashMap<usize, HashMap<usize, u32>>,
}

impl<'a> Hops<'a> {
    /// Connectivity of `frames`, through the stations they share.
    pub fn new(frames: &'a [Frame]) -> Self {
        let mut at: HashMap<Id, Vec<usize>> = HashMap::new();
        for (i, f) in frames.iter().enumerate() {
            at.entry(f.a).or_default().push(i);
            at.entry(f.b).or_default().push(i);
        }
        Hops {
            frames,
            at,
            reach: HashMap::new(),
        }
    }

    /// Hops from leg `i` to leg `j`, or `None` when more than `HOP_CAP` apart or not connected.
    pub fn between(&mut self, i: usize, j: usize) -> Option<u32> {
        let (at, frames) = (&self.at, self.frames);
        self.reach
            .entry(i)
            .or_insert_with(|| {
                let mut dist = HashMap::from([(i, 0u32)]);
                let mut queue = VecDeque::from([i]);
                while let Some(f) = queue.pop_front() {
                    let d = dist[&f];
                    if d == HOP_CAP {
                        continue;
                    }
                    for s in [frames[f].a, frames[f].b] {
                        for &g in at.get(&s).into_iter().flatten() {
                            dist.entry(g).or_insert_with(|| {
                                queue.push_back(g);
                                d + 1
                            });
                        }
                    }
                }
                dist
            })
            .get(&j)
            .copied()
    }
}

/// Elevation on leg `i` at the point nearest `p`.
fn z_at(frames: &[Frame], i: usize, p: [f64; 2]) -> f64 {
    let f = &frames[i];
    match basis(f.pa, f.pb) {
        Some((d, len)) => f.za + along(p, f.pa, d, len) * (f.zb - f.za),
        None => f.za,
    }
}

fn distance(frames: &[Frame], i: usize, p: [f64; 2]) -> f64 {
    let f = &frames[i];
    basis(f.pa, f.pb).map_or(f64::INFINITY, |(d, len)| {
        distance_to_segment(p, f.pa, d, len)
    })
}

/// Anchors a plan stroke as one path. `pin`, when given, is the index into `frames` of the leg
/// the first point must hang on (the "other level" flip).
pub fn anchor_stroke_path(
    points: &[[f64; 2]],
    frames: &[Frame],
    widths: Option<Vec<f64>>,
    pin: Option<usize>,
) -> Result<AnchoredPath, AnchorError> {
    check_stroke(points, &widths)?;
    let usable: Vec<usize> = usable(frames).into_iter().map(|(i, _, _)| i).collect();
    if usable.is_empty() || pin.is_some_and(|p| !usable.contains(&p)) {
        return Err(AnchorError::NoFrames);
    }
    let candidates: Vec<Vec<(usize, f64)>> = points
        .iter()
        .enumerate()
        .map(|(k, p)| {
            if k == 0
                && let Some(pin) = pin
            {
                return vec![(pin, distance(frames, pin, *p))];
            }
            let mut all: Vec<(usize, f64)> = usable
                .iter()
                .map(|&i| (i, distance(frames, i, *p)))
                .collect();
            all.sort_by(|a, b| a.1.total_cmp(&b.1));
            let near = all[0].1;
            all.into_iter()
                .take_while(|c| c.1 <= near + SEARCH_RADIUS)
                .take(MAX_CANDIDATES)
                .collect()
        })
        .collect();
    let mut hops = Hops::new(frames);
    let mut cost: Vec<f64> = candidates[0].iter().map(|c| c.1).collect();
    let mut back: Vec<Vec<usize>> = vec![Vec::new()];
    for k in 1..points.len() {
        let mut next = Vec::with_capacity(candidates[k].len());
        let mut from = Vec::with_capacity(candidates[k].len());
        for &(j, dj) in &candidates[k] {
            let zj = z_at(frames, j, points[k]);
            let (best, arg) = candidates[k - 1]
                .iter()
                .enumerate()
                .map(|(m, &(i, _))| {
                    let step = match (i == j, hops.between(i, j)) {
                        (true, _) => 0.0,
                        // legs sharing a station meet at its height: a slope is not a jump
                        (false, Some(1)) => HOP_COST,
                        (false, h) => {
                            h.map_or(UNCONNECTED_COST, |h| HOP_COST * h as f64)
                                + Z_COST * (z_at(frames, i, points[k - 1]) - zj).abs()
                        }
                    };
                    (cost[m] + step, m)
                })
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .expect("every point has a candidate");
            next.push(best + dj);
            from.push(arg);
        }
        cost = next;
        back.push(from);
    }
    let mut m = (0..cost.len())
        .min_by(|a, b| cost[*a].total_cmp(&cost[*b]))
        .expect("the last point has a candidate");
    let mut choice = vec![0usize; points.len()];
    for k in (0..points.len()).rev() {
        choice[k] = candidates[k][m].0;
        if k > 0 {
            m = back[k][m];
        }
    }
    Ok(build_path(points, frames, &choice, widths))
}

/// The nearest leg within [`AMBIGUOUS_RADIUS`] of `p` on another level than `chosen`: the
/// other level a stroke starting at `p` could belong to. `None` when the start is not ambiguous.
pub fn other_level(p: [f64; 2], frames: &[Frame], chosen: usize) -> Option<usize> {
    let mut hops = Hops::new(frames);
    let zc = z_at(frames, chosen, p);
    usable(frames)
        .into_iter()
        .map(|(i, _, _)| (i, distance(frames, i, p)))
        .filter(|&(i, d)| {
            d <= AMBIGUOUS_RADIUS && other_levels(&mut hops, chosen, i, zc, z_at(frames, i, p))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// Index in `frames` of the leg between stations `a` and `b` (either way round).
pub fn frame_index(frames: &[Frame], (a, b): (Id, Id)) -> Option<usize> {
    frames
        .iter()
        .position(|f| (f.a == a && f.b == b) || (f.a == b && f.b == a))
}

/// True when the path steps between two consecutive points onto a leg on another level than the
/// one before: a line that hopped levels under the old rule. Legs missing from `frames` say
/// nothing.
pub fn path_hops_level(path: &AnchoredPath, frames: &[Frame]) -> bool {
    let index: Vec<Option<usize>> = path
        .frames
        .iter()
        .map(|ab| frame_index(frames, *ab))
        .collect();
    let mut hops = Hops::new(frames);
    path.points.windows(2).any(|w| {
        let (x, y) = (w[0].frame as usize, w[1].frame as usize);
        x != y
            && matches!((index[x], index[y]), (Some(i), Some(j))
                if other_levels(&mut hops, i, j, mid_z(&frames[i]), mid_z(&frames[j])))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchor_stroke;
    use cave_id::{IdSource, SeqIds};

    /// Upper passage along x at z 0; lower passage along y at z -8, crossing it in plan at the
    /// origin; joined far away (U4 - J - L0), so it is one survey, 6 hops round.
    struct Crossing {
        frames: Vec<Frame>,
        pos: HashMap<Id, [f64; 2]>,
        upper: Vec<usize>,
        lower: Vec<usize>,
    }

    fn crossing() -> Crossing {
        let mut ids = SeqIds::new(1);
        let mut st = |x: f64, y: f64, z: f64| (ids.next_id(), [x, y], z);
        let u: Vec<_> = (0..5)
            .map(|k| st(-20.0 + 10.0 * k as f64, 0.0, 0.0))
            .collect();
        let j = st(30.0, -30.0, -4.0);
        let l: Vec<_> = (0..5)
            .map(|k| st(0.0, -20.0 + 10.0 * k as f64, -8.0))
            .collect();
        let leg = |p: &(Id, [f64; 2], f64), q: &(Id, [f64; 2], f64)| Frame {
            a: p.0,
            b: q.0,
            pa: p.1,
            pb: q.1,
            za: p.2,
            zb: q.2,
        };
        let mut frames: Vec<Frame> = u.windows(2).map(|w| leg(&w[0], &w[1])).collect();
        let upper: Vec<usize> = (0..frames.len()).collect();
        frames.push(leg(&u[4], &j));
        frames.push(leg(&j, &l[0]));
        let first_lower = frames.len();
        frames.extend(l.windows(2).map(|w| leg(&w[0], &w[1])));
        let lower: Vec<usize> = (first_lower..frames.len()).collect();
        let pos = u
            .iter()
            .chain([&j])
            .chain(l.iter())
            .map(|s| (s.0, s.1))
            .collect();
        Crossing {
            frames,
            pos,
            upper,
            lower,
        }
    }

    /// A wall traced 2 m north of the upper passage, right across the crossing.
    fn upper_wall() -> Vec<[f64; 2]> {
        (0..13).map(|k| [-15.0 + 2.5 * k as f64, 2.0]).collect()
    }

    fn legs_used(c: &Crossing, path: &AnchoredPath) -> Vec<usize> {
        path.frames
            .iter()
            .map(|ab| frame_index(&c.frames, *ab).unwrap())
            .collect()
    }

    #[test]
    fn level_anchor_stays_on_the_upper_passage_over_the_crossing() {
        let c = crossing();
        let old = anchor_stroke(&upper_wall(), &c.frames, None).unwrap();
        assert!(
            legs_used(&c, &old).iter().any(|i| c.lower.contains(i)),
            "fixture must reproduce the bug"
        );
        let new = anchor_stroke_path(&upper_wall(), &c.frames, None, None).unwrap();
        assert!(legs_used(&c, &new).iter().all(|i| c.upper.contains(i)));
    }

    #[test]
    fn level_anchor_moving_the_lower_passage_leaves_the_upper_line() {
        let c = crossing();
        let path = anchor_stroke_path(&upper_wall(), &c.frames, None, None).unwrap();
        let before = path.place(&|id| c.pos.get(&id).copied()).unwrap();
        let lower_ids: Vec<Id> = c
            .lower
            .iter()
            .flat_map(|&i| [c.frames[i].a, c.frames[i].b])
            .collect();
        let moved: HashMap<Id, [f64; 2]> = c
            .pos
            .iter()
            .map(|(id, p)| {
                (
                    *id,
                    if lower_ids.contains(id) {
                        [p[0] + 3.0, p[1]]
                    } else {
                        *p
                    },
                )
            })
            .collect();
        let after = path.place(&|id| moved.get(&id).copied()).unwrap();
        assert!(
            before
                .iter()
                .zip(&after)
                .all(|(a, b)| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9)
        );
    }

    #[test]
    fn level_anchor_pin_puts_the_stroke_on_the_lower_passage() {
        let c = crossing();
        let wall: Vec<[f64; 2]> = (0..9).map(|k| [1.5, -10.0 + 2.5 * k as f64]).collect();
        let pin = c.lower[2]; // L2-L3, the lower leg under the crossing
        let path = anchor_stroke_path(&wall, &c.frames, None, Some(pin)).unwrap();
        assert!(legs_used(&c, &path).iter().all(|i| c.lower.contains(i)));
    }

    #[test]
    fn level_anchor_single_level_matches_the_old_rule() {
        let mut ids = SeqIds::new(1);
        let (a, b, cc) = (ids.next_id(), ids.next_id(), ids.next_id());
        let frames = vec![
            Frame {
                a,
                b,
                pa: [0.0, 0.0],
                pb: [10.0, 0.0],
                za: 0.0,
                zb: 0.0,
            },
            Frame {
                a: b,
                b: cc,
                pa: [10.0, 0.0],
                pb: [10.0, -8.0],
                za: 0.0,
                zb: -1.0,
            },
        ];
        let stroke = [
            [1.0, 1.0],
            [5.0, 1.0],
            [9.0, 1.0],
            [11.0, -3.0],
            [11.0, -7.0],
        ];
        assert_eq!(
            anchor_stroke_path(&stroke, &frames, None, None).unwrap(),
            anchor_stroke(&stroke, &frames, None).unwrap()
        );
        assert_eq!(other_level([1.0, 1.0], &frames, 0), None);
    }

    #[test]
    fn level_anchor_other_level_finds_the_lower_leg_at_the_crossing() {
        let c = crossing();
        let chosen = c.upper[1]; // U1-U2, ending at the crossing
        let other = other_level([0.5, 1.0], &c.frames, chosen).unwrap();
        assert!(c.lower.contains(&other));
    }

    #[test]
    fn level_anchor_hopped_path_is_detected() {
        let c = crossing();
        let old = anchor_stroke(&upper_wall(), &c.frames, None).unwrap();
        let new = anchor_stroke_path(&upper_wall(), &c.frames, None, None).unwrap();
        assert!(path_hops_level(&old, &c.frames));
        assert!(!path_hops_level(&new, &c.frames));
    }
}
