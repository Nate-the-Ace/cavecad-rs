//! Least-squares loop closure adjustment, ported from the old `CsAdjust`.
//!
//! Resolution walks a spanning tree, so a loop's whole error lands on whichever leg closed it.
//! This spreads it over every leg, weighted by how wrong each leg is likely to be, holding the
//! anchored and fixed stations.
//!
//! Each leg `a -> b` is one observation per axis: `x_b - x_a = dx`, weight `w = 1/sigma^2` with
//! `sigma^2 = sigma_tape^2 + (distance * sigma_angle)^2`, so long legs absorb more. Setting the
//! derivative to zero at each free station gives the weighted graph Laplacian; with one scalar
//! sigma per leg the three axes share one matrix. It is never built: a Jacobi-preconditioned
//! conjugate gradient needs only `L*v`, which is a loop over the legs, and it starts from the raw
//! resolved coordinates, so a survey without loops needs zero iterations.
//!
//! Honesty rule: the closures reported in [`Adjusted`] are the as-surveyed ones, copied from the
//! input. Recomputing them from adjusted coordinates would report every cave as closing at 0%.
//!
//! A leg flagged `no_adjust` is held by weight (a large multiple of the median), not removed, so
//! the matrix stays well conditioned. A plumbed pitch (bearing omitted) is weighted by
//! [`PLUMB_SIGMA_ANGLE_DEG`] instead of the compass error. If the solver does not converge the
//! raw coordinates are returned and the result says so: a half-solved network looks adjusted.

use crate::network::{Closure, Leg, Resolved};
use crate::vec3::Vec3;
use cave_id::Id;
use std::collections::{BTreeMap, HashMap, HashSet};

/// Angular error assumed for a plumbed pitch (its direction came from gravity, not a compass).
pub const PLUMB_SIGMA_ANGLE_DEG: f64 = 0.5;
/// A held leg's weight is this many times the median weight.
pub const NO_ADJUST_WEIGHT_FACTOR: f64 = 1e6;
/// Smallest variance allowed (weights are its reciprocal).
pub const MIN_VARIANCE: f64 = 1e-12;
/// Default convergence tolerance, as a fraction of the network's extent.
pub const CG_TOLERANCE_FRACTION: f64 = 1e-12;

/// Instrument precision and solver limits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    /// Tape error in metres (about 0.03 m for hand tape work).
    pub sigma_tape: f64,
    /// Compass and clino error in degrees.
    pub sigma_angle_deg: f64,
    /// Iteration cap; `None` chooses one from the network size.
    pub max_iterations: Option<usize>,
    /// Convergence tolerance as a fraction of the network extent; `None` uses the default.
    pub cg_tolerance: Option<f64>,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            sigma_tape: 0.03,
            sigma_angle_deg: 1.5,
            max_iterations: None,
            cg_tolerance: None,
        }
    }
}

/// How far a station moved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shift {
    /// Change in x, y, z.
    pub v: Vec3,
    /// 3D distance moved.
    pub distance: f64,
}

/// What a leg absorbed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Residual {
    /// The shot.
    pub shot: Id,
    /// Start station.
    pub from: Id,
    /// End station.
    pub to: Id,
    /// Adjusted displacement minus measured displacement.
    pub v: Vec3,
    /// Size of the residual.
    pub distance: f64,
    /// Residual divided by the leg's sigma.
    pub standardized: f64,
}

/// How the solve went.
#[derive(Clone, Debug, PartialEq)]
pub struct Summary {
    /// True if the solver reached its tolerance.
    pub converged: bool,
    /// Conjugate gradient iterations used (the most over the three axes).
    pub iterations: usize,
    /// Stations held in place.
    pub pinned: Vec<Id>,
    /// Plain-language reason when not converged.
    pub warning: Option<String>,
}

/// The result of adjusting.
#[derive(Clone, Debug, PartialEq)]
pub struct Adjusted {
    /// Final positions (the raw ones if `adjusted` is false).
    pub stations: BTreeMap<Id, Vec3>,
    /// How far each station moved from the raw position.
    pub shifts: BTreeMap<Id, Shift>,
    /// One per leg, in resolution order.
    pub residuals: Vec<Residual>,
    /// False when the solve did not converge and the raw coordinates were kept.
    pub adjusted: bool,
    /// Solver report.
    pub summary: Summary,
    /// The as-surveyed closures, unchanged from the input.
    pub closures: Vec<Closure>,
}

fn variance(leg: &Leg, p: &Params) -> f64 {
    let sa = if leg.azimuth_omitted {
        PLUMB_SIGMA_ANGLE_DEG
    } else {
        p.sigma_angle_deg
    };
    let angular = leg.distance * sa.to_radians();
    (p.sigma_tape * p.sigma_tape + angular * angular).max(MIN_VARIANCE)
}

fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 1.0;
    }
    let mut s = values.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    let mid = s.len() / 2;
    if s.len() % 2 == 1 {
        s[mid]
    } else {
        (s[mid - 1] + s[mid]) / 2.0
    }
}

struct Edge {
    a: Option<usize>,
    b: Option<usize>,
    w: f64,
}

/// Preconditioned conjugate gradient on the Laplacian. Returns (solution, iterations, converged).
fn solve(
    diag: &[f64],
    edges: &[Edge],
    rhs: &[f64],
    x0: &[f64],
    tol: f64,
    max_iter: usize,
) -> (Vec<f64>, usize, bool) {
    let n = diag.len();
    let matvec = |x: &[f64]| {
        let mut y = vec![0.0; n];
        for e in edges {
            match (e.a, e.b) {
                (Some(i), Some(j)) => {
                    y[i] += e.w * (x[i] - x[j]);
                    y[j] += e.w * (x[j] - x[i]);
                }
                (Some(i), None) => y[i] += e.w * x[i],
                (None, Some(j)) => y[j] += e.w * x[j],
                (None, None) => {}
            }
        }
        y
    };
    let mut x = x0.to_vec();
    let ax = matvec(&x);
    let mut r: Vec<f64> = (0..n).map(|i| rhs[i] - ax[i]).collect();
    let step = |r: &[f64]| {
        (0..r.len())
            .map(|i| (r[i] / diag[i]).abs())
            .fold(0.0, f64::max)
    };
    if step(&r) <= tol {
        return (x, 0, true);
    }
    let mut z: Vec<f64> = (0..n).map(|i| r[i] / diag[i]).collect();
    let mut p = z.clone();
    let mut rz: f64 = (0..n).map(|i| r[i] * z[i]).sum();
    for it in 1..=max_iter {
        let ap = matvec(&p);
        let pap: f64 = (0..n).map(|i| p[i] * ap[i]).sum();
        if pap <= 0.0 {
            return (x, it, false);
        }
        let alpha = rz / pap;
        for i in 0..n {
            x[i] += alpha * p[i];
            r[i] -= alpha * ap[i];
        }
        if step(&r) <= tol {
            return (x, it, true);
        }
        z = (0..n).map(|i| r[i] / diag[i]).collect();
        let rz_new: f64 = (0..n).map(|i| r[i] * z[i]).sum();
        let beta = rz_new / rz;
        rz = rz_new;
        for i in 0..n {
            p[i] = z[i] + beta * p[i];
        }
    }
    (x, max_iter, false)
}

/// Spreads loop closure error over the network. `legs` must be the legs given to `resolve`.
pub fn adjust(legs: &[Leg], resolved: &Resolved, params: &Params) -> Adjusted {
    let by_shot: HashMap<Id, &Leg> = legs.iter().map(|l| (l.shot, l)).collect();
    let used: Vec<&Leg> = resolved
        .legs
        .iter()
        .filter_map(|r| by_shot.get(&r.shot).copied())
        .collect();
    let pinned: HashSet<Id> = resolved.anchors.iter().copied().collect();
    let raw: BTreeMap<Id, Vec3> = resolved
        .stations
        .iter()
        .map(|(id, p)| (*id, p.pos))
        .collect();

    let free: Vec<Id> = raw
        .keys()
        .copied()
        .filter(|id| !pinned.contains(id))
        .collect();
    let index: HashMap<Id, usize> = free.iter().enumerate().map(|(i, id)| (*id, i)).collect();

    let variances: Vec<f64> = used.iter().map(|l| variance(l, params)).collect();
    let weights: Vec<f64> = variances.iter().map(|v| 1.0 / v).collect();
    let med = median(&weights);
    let weights: Vec<f64> = used
        .iter()
        .zip(&weights)
        .map(|(l, w)| {
            if l.no_adjust {
                NO_ADJUST_WEIGHT_FACTOR * med
            } else {
                *w
            }
        })
        .collect();

    let edges: Vec<Edge> = used
        .iter()
        .zip(&weights)
        .map(|(l, w)| Edge {
            a: index.get(&l.from).copied(),
            b: index.get(&l.to).copied(),
            w: *w,
        })
        .collect();
    let mut diag = vec![0.0; free.len()];
    for e in &edges {
        for i in [e.a, e.b].into_iter().flatten() {
            diag[i] += e.w;
        }
    }

    let (min, max) = raw
        .values()
        .fold((None::<Vec3>, None::<Vec3>), |(lo, hi), p| {
            let lo = lo.map_or(*p, |l| Vec3::new(l.x.min(p.x), l.y.min(p.y), l.z.min(p.z)));
            let hi = hi.map_or(*p, |h| Vec3::new(h.x.max(p.x), h.y.max(p.y), h.z.max(p.z)));
            (Some(lo), Some(hi))
        });
    let extent = match (min, max) {
        (Some(a), Some(b)) => b.minus(a).norm(),
        _ => 0.0,
    };
    let tol =
        (params.cg_tolerance.unwrap_or(CG_TOLERANCE_FRACTION) * extent).max(f64::MIN_POSITIVE);
    let max_iter = params.max_iterations.unwrap_or(10 * free.len() + 100);

    let mut solved: Vec<[f64; 3]> = free
        .iter()
        .map(|id| [raw[id].x, raw[id].y, raw[id].z])
        .collect();
    let (mut iterations, mut converged) = (0, true);
    if !free.is_empty() {
        for axis in 0..3 {
            let comp = |v: Vec3| [v.x, v.y, v.z][axis];
            let mut rhs = vec![0.0; free.len()];
            for ((l, e), w) in used.iter().zip(&edges).zip(&weights) {
                let d = comp(l.offset);
                if let Some(b) = e.b {
                    rhs[b] += w * d;
                    if e.a.is_none() {
                        rhs[b] += w * comp(raw[&l.from]);
                    }
                }
                if let Some(a) = e.a {
                    rhs[a] -= w * d;
                    if e.b.is_none() {
                        rhs[a] += w * comp(raw[&l.to]);
                    }
                }
            }
            let x0: Vec<f64> = solved.iter().map(|s| s[axis]).collect();
            let (x, it, ok) = solve(&diag, &edges, &rhs, &x0, tol, max_iter);
            for (s, v) in solved.iter_mut().zip(x) {
                s[axis] = v;
            }
            iterations = iterations.max(it);
            converged &= ok;
        }
    }

    let summary = Summary {
        converged,
        iterations,
        pinned: resolved.anchors.clone(),
        warning: (!converged).then(|| {
            "The adjustment did not settle, so the survey is shown as surveyed (not adjusted)."
                .to_string()
        }),
    };
    if !converged {
        return Adjusted {
            stations: raw,
            shifts: BTreeMap::new(),
            residuals: Vec::new(),
            adjusted: false,
            summary,
            closures: resolved.closures.clone(),
        };
    }

    let mut stations = raw.clone();
    for (id, s) in free.iter().zip(&solved) {
        stations.insert(*id, Vec3::new(s[0], s[1], s[2]));
    }
    let shifts = stations
        .iter()
        .map(|(id, p)| {
            let v = p.minus(raw[id]);
            (
                *id,
                Shift {
                    v,
                    distance: v.norm(),
                },
            )
        })
        .collect();
    let residuals = used
        .iter()
        .zip(&variances)
        .map(|(l, var)| {
            let v = stations[&l.to].minus(stations[&l.from]).minus(l.offset);
            Residual {
                shot: l.shot,
                from: l.from,
                to: l.to,
                v,
                distance: v.norm(),
                standardized: v.norm() / var.sqrt(),
            }
        })
        .collect();
    Adjusted {
        stations,
        shifts,
        residuals,
        adjusted: true,
        summary,
        closures: resolved.closures.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::{Fixed, resolve};
    use crate::traverse::{Reading, TapeMode, offset};
    use cave_id::{IdSource, SeqIds};

    struct Net {
        ids: SeqIds,
        names: HashMap<&'static str, Id>,
        legs: Vec<Leg>,
    }

    impl Net {
        fn new() -> Self {
            Net {
                ids: SeqIds::new(1),
                names: HashMap::new(),
                legs: Vec::new(),
            }
        }
        fn id(&mut self, n: &'static str) -> Id {
            *self.names.entry(n).or_insert_with(|| self.ids.next_id())
        }
        fn shot(
            &mut self,
            from: &'static str,
            to: &'static str,
            d: f64,
            az: f64,
            inc: f64,
        ) -> usize {
            let (f, t, s) = (self.id(from), self.id(to), self.ids.next_id());
            let r = Reading {
                distance: d,
                azimuth: Some(az),
                inclination: Some(inc),
                ..Default::default()
            };
            let o = offset(&r, TapeMode::Slope).unwrap();
            self.legs.push(Leg {
                shot: s,
                from: f,
                to: t,
                offset: o.v,
                distance: d,
                no_adjust: false,
                azimuth_omitted: false,
            });
            self.legs.len() - 1
        }
    }

    fn near(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() < tol
    }

    /// Equal weights (every leg has variance 1) so the answer is doable by hand.
    const EQ: Params = Params {
        sigma_tape: 1.0,
        sigma_angle_deg: 0.0,
        max_iterations: None,
        cg_tolerance: None,
    };

    fn square() -> Net {
        let mut n = Net::new();
        n.shot("A4", "A1", 10.5, 270.0, 0.0);
        n.shot("A1", "A2", 10.0, 0.0, 0.0);
        n.shot("A2", "A3", 10.0, 90.0, 0.0);
        n.shot("A3", "A4", 10.0, 180.0, 0.0);
        n.shot("A3", "B1", 5.0, 90.0, 0.0);
        n
    }

    #[test]
    fn square_misclosure_is_shared_in_quarters_as_solved_by_hand() {
        let mut n = square();
        let r = resolve(&n.legs, &[]);
        let a = adjust(&n.legs, &r, &EQ);
        assert!(a.adjusted && a.summary.converged);
        let shift = |n: &mut Net, s| a.shifts[&n.id(s)];
        assert!(
            near(shift(&mut n, "A4").distance, 0.0, 1e-9),
            "the pinned anchor does not move"
        );
        assert!(near(shift(&mut n, "A1").v.x, 0.125, 1e-9));
        assert!(near(shift(&mut n, "A2").v.x, 0.25, 1e-9));
        assert!(near(shift(&mut n, "A3").v.x, 0.375, 1e-9));
        assert!(
            near(shift(&mut n, "B1").v.x, 0.375, 1e-9),
            "the branch rides along"
        );
        assert!(
            near(shift(&mut n, "A2").v.y, 0.0, 1e-9) && near(shift(&mut n, "A2").v.z, 0.0, 1e-9)
        );
        assert_eq!(a.summary.pinned, vec![n.id("A4")]);
    }

    #[test]
    fn each_ring_leg_absorbs_an_eighth_and_the_branch_nothing() {
        let n = square();
        let r = resolve(&n.legs, &[]);
        let a = adjust(&n.legs, &r, &EQ);
        assert_eq!(a.residuals.len(), r.legs.len(), "one residual per leg");
        let by_shot: HashMap<Id, &Residual> = a.residuals.iter().map(|x| (x.shot, x)).collect();
        for leg in &n.legs[..4] {
            assert!(near(by_shot[&leg.shot].v.x.abs(), 0.125, 1e-9));
            assert!(
                near(by_shot[&leg.shot].standardized, 0.125, 1e-9),
                "sigma is 1 here"
            );
        }
        assert!(near(by_shot[&n.legs[4].shot].distance, 0.0, 1e-9));
    }

    #[test]
    fn residuals_balance_at_every_free_station() {
        let n = square();
        let r = resolve(&n.legs, &[]);
        let a = adjust(&n.legs, &r, &EQ);
        let mut g: HashMap<Id, Vec3> = HashMap::new();
        for res in &a.residuals {
            let to = g.entry(res.to).or_default();
            *to = to.plus(res.v);
            let from = g.entry(res.from).or_default();
            *from = from.minus(res.v);
        }
        for (id, v) in g {
            if !a.summary.pinned.contains(&id) {
                assert!(v.norm() < 1e-9, "least-squares condition at {id:?}");
            }
        }
    }

    #[test]
    fn closures_stay_as_surveyed() {
        let n = square();
        let r = resolve(&n.legs, &[]);
        let a = adjust(&n.legs, &r, &EQ);
        assert_eq!(
            a.closures, r.closures,
            "never recomputed from adjusted coordinates"
        );
        assert!(near(a.closures[0].distance, 0.5, 1e-9));
    }

    #[test]
    fn a_survey_without_loops_needs_no_iterations() {
        let mut n = Net::new();
        n.shot("A", "B", 10.0, 0.0, 5.0);
        n.shot("B", "C", 7.0, 90.0, -3.0);
        let r = resolve(&n.legs, &[]);
        let a = adjust(&n.legs, &r, &Params::default());
        assert_eq!(a.summary.iterations, 0);
        for (id, p) in &r.stations {
            assert_eq!(a.stations[id], p.pos);
        }
    }

    #[test]
    fn both_fixed_stations_are_pinned() {
        let mut n = Net::new();
        n.shot("H1", "H2", 10.0, 90.0, 0.0);
        n.shot("H2", "H3", 10.0, 90.0, 0.0);
        let (h1, h3) = (n.id("H1"), n.id("H3"));
        let r = resolve(
            &n.legs,
            &[
                Fixed {
                    station: h1,
                    pos: Vec3::ZERO,
                },
                Fixed {
                    station: h3,
                    pos: Vec3::new(20.6, 0.0, 0.0),
                },
            ],
        );
        let a = adjust(&n.legs, &r, &EQ);
        assert!(a.summary.pinned.contains(&h1) && a.summary.pinned.contains(&h3));
        assert!(near(a.stations[&h3].x, 20.6, 1e-9));
        assert!(
            near(a.stations[&n.id("H2")].x, 10.3, 1e-9),
            "0.6 split evenly across two equal legs"
        );
    }

    #[test]
    fn a_held_leg_keeps_its_length_and_the_neighbour_absorbs_the_error() {
        let mut n = Net::new();
        let held = n.shot("H1", "H2", 10.0, 90.0, 0.0);
        n.legs[held].no_adjust = true;
        n.shot("H2", "H3", 10.0, 90.0, 0.0);
        let (h1, h3) = (n.id("H1"), n.id("H3"));
        let r = resolve(
            &n.legs,
            &[
                Fixed {
                    station: h1,
                    pos: Vec3::ZERO,
                },
                Fixed {
                    station: h3,
                    pos: Vec3::new(20.6, 0.0, 0.0),
                },
            ],
        );
        let a = adjust(&n.legs, &r, &EQ);
        assert!(a.summary.converged);
        let (x1, x2, x3) = (
            a.stations[&h1].x,
            a.stations[&n.id("H2")].x,
            a.stations[&h3].x,
        );
        assert!(near(x2 - x1, 10.0, 1e-4));
        assert!((x3 - x2 - 10.0).abs() > 0.5);
    }

    #[test]
    fn a_starved_solve_returns_the_raw_survey_and_says_so() {
        let n = square();
        let r = resolve(&n.legs, &[]);
        let stuck = Params {
            max_iterations: Some(1),
            cg_tolerance: Some(1e-30),
            ..EQ
        };
        let a = adjust(&n.legs, &r, &stuck);
        assert!(!a.summary.converged && !a.adjusted);
        assert!(a.summary.warning.is_some());
        for (id, p) in &r.stations {
            assert_eq!(a.stations[id], p.pos, "coordinates exactly as surveyed");
        }
        assert_eq!(a.closures, r.closures);
    }

    #[test]
    fn long_legs_absorb_more_than_short_ones() {
        let mut n = Net::new();
        n.shot("A", "B", 100.0, 90.0, 0.0);
        n.shot("B", "C", 1.0, 0.0, 0.0);
        n.shot("C", "A", 100.0, 270.0, 0.0);
        n.shot("A", "C", 1.0, 0.0, 0.0);
        let r = resolve(&n.legs, &[]);
        let a = adjust(&n.legs, &r, &Params::default());
        assert!(a.summary.converged);
        let long = a
            .residuals
            .iter()
            .find(|x| x.shot == n.legs[0].shot)
            .unwrap();
        let short = a
            .residuals
            .iter()
            .find(|x| x.shot == n.legs[1].shot)
            .unwrap();
        assert!(long.distance >= short.distance);
    }
}
