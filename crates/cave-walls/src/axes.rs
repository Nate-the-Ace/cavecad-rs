//! Which way the passage runs at each station, and whether the station is a real junction.
//!
//! Legs are clustered by direction, not counted: a loop tie-in has three legs but only two ways
//! out (the closure leg arrives back down a passage that is already walked), so it is not a
//! junction and the walls go through it.

use cave_id::Id;
use cave_survey::{LegResult, Vec3};
use std::collections::{BTreeMap, HashMap};

/// Below this plan separation (metres) two stations are the same point as far as a bearing goes.
/// A plumbed leg's plan projection is trigonometric dust (`cos(90 degrees)` is not 0), and without
/// this a free-fall reads as a confident bearing of north.
pub const COINCIDENT_PLAN: f64 = 1e-6;
/// Two bearings this close (degrees) are the same way out.
pub const AXIS_TOLERANCE_DEG: f64 = 30.0;
/// Two ways out at least this far from each other make a through station with one passage axis.
pub const THROUGH_SEPARATION_DEG: f64 = 120.0;

/// Folds an angle into `[0, 360)`; anything a hair under a full turn is north.
pub fn normalize_az(deg: f64) -> Option<f64> {
    if !deg.is_finite() {
        return None;
    }
    let d = deg.rem_euclid(360.0);
    Some(if d >= 360.0 - 1e-9 { 0.0 } else { d })
}

/// Signed angle from `a` to `b` in degrees, in `(-180, 180]`.
pub fn relative_bearing(a: f64, b: f64) -> f64 {
    let mut d = (b - a) % 360.0;
    if d <= -180.0 {
        d += 360.0;
    } else if d > 180.0 {
        d -= 360.0;
    }
    d
}

/// Plan bearing from `a` to `b`, or `None` when they coincide in plan (no direction exists).
pub fn plan_bearing(a: Vec3, b: Vec3) -> Option<f64> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    if !dx.is_finite()
        || !dy.is_finite()
        || (dx.abs() <= COINCIDENT_PLAN && dy.abs() <= COINCIDENT_PLAN)
    {
        return None;
    }
    normalize_az(dx.atan2(dy).to_degrees())
}

/// One way out of a station: the circular mean bearing and how many legs leave that way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dir {
    /// Mean bearing in degrees.
    pub mean: f64,
    /// Number of legs in the cluster.
    pub count: usize,
}

/// Groups bearings into distinct ways out. Greedy and order-dependent by design: deterministic
/// for a given input order, which is stable because resolution order is.
pub fn cluster_bearings(bearings: &[f64], tol: f64) -> Vec<Dir> {
    let mut taken = vec![false; bearings.len()];
    let mut out = Vec::new();
    for i in 0..bearings.len() {
        if taken[i] {
            continue;
        }
        taken[i] = true;
        let (mut sx, mut sy) = (
            bearings[i].to_radians().sin(),
            bearings[i].to_radians().cos(),
        );
        let (mut n, mut mean) = (1usize, bearings[i]);
        for j in i + 1..bearings.len() {
            if taken[j] || relative_bearing(mean, bearings[j]).abs() > tol {
                continue;
            }
            taken[j] = true;
            sx += bearings[j].to_radians().sin();
            sy += bearings[j].to_radians().cos();
            n += 1;
            mean = normalize_az(sx.atan2(sy).to_degrees()).unwrap_or(mean);
        }
        out.push(Dir { mean, count: n });
    }
    out
}

/// The ways out of one station.
#[derive(Clone, Debug, PartialEq)]
pub struct Axes {
    /// Distinct ways out, in seed order.
    pub dirs: Vec<Dir>,
    /// Number of legs with a plan direction that touch the station.
    pub legs: usize,
}

/// The ways out of every station, read from the placed geometry (every leg counts, including
/// closures). Stations whose legs all have no plan direction (pitches) are absent: a missing
/// entry means "nothing known", never "no ways out".
pub fn stations_axes(
    stations: &BTreeMap<Id, Vec3>,
    legs: &[LegResult],
    tol: f64,
) -> HashMap<Id, Axes> {
    let mut raw: HashMap<Id, Vec<f64>> = HashMap::new();
    for leg in legs {
        let (Some(&a), Some(&b)) = (stations.get(&leg.from), stations.get(&leg.to)) else {
            continue;
        };
        let Some(f) = plan_bearing(a, b) else {
            continue;
        };
        raw.entry(leg.from).or_default().push(f);
        raw.entry(leg.to)
            .or_default()
            .push(normalize_az(f + 180.0).unwrap_or(f));
    }
    raw.into_iter()
        .map(|(id, b)| {
            (
                id,
                Axes {
                    dirs: cluster_bearings(&b, tol),
                    legs: b.len(),
                },
            )
        })
        .collect()
}

impl Axes {
    /// Which of the station's ways out a bearing belongs to (the nearest), or `None` for no bearing.
    pub fn nearest_cluster(&self, bearing: Option<f64>) -> Option<usize> {
        let bearing = bearing.filter(|b| b.is_finite())?;
        self.dirs
            .iter()
            .enumerate()
            .map(|(i, d)| (i, relative_bearing(d.mean, bearing).abs()))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    /// Three or more distinct ways out.
    pub fn is_junction(&self) -> bool {
        self.dirs.len() >= 3
    }

    /// The two near-opposite ways out of a through station, as indices into `dirs`.
    pub fn through_pair(&self) -> Option<(usize, usize)> {
        if self.dirs.len() != 2 {
            return None;
        }
        (relative_bearing(self.dirs[0].mean, self.dirs[1].mean).abs() >= THROUGH_SEPARATION_DEG)
            .then_some((0, 1))
    }

    /// The direction the passage runs, which is not always the arriving leg's bearing: at a
    /// through station it is the bisector of the two ways out, oriented to agree with travel.
    pub fn passage_azimuth(&self, arrival: Option<f64>) -> Option<f64> {
        let arrival = arrival.filter(|a| a.is_finite());
        let Some((a, b)) = self.through_pair() else {
            // Not a through station. With no arriving bearing (the foot of a pitch) a station with
            // exactly one way out still answers: that way is where the passage goes.
            if arrival.is_none() && self.dirs.len() == 1 {
                return Some(self.dirs[0].mean);
            }
            return arrival;
        };
        let (u0, u1) = (
            self.dirs[a].mean.to_radians(),
            self.dirs[b].mean.to_radians(),
        );
        let (fx, fy) = (u1.sin() - u0.sin(), u1.cos() - u0.cos());
        if fx == 0.0 && fy == 0.0 {
            return arrival;
        }
        let axis = normalize_az(fx.atan2(fy).to_degrees())?;
        match arrival {
            None => Some(axis),
            Some(arr) if relative_bearing(arr, axis).abs() > 90.0 => normalize_az(axis + 180.0),
            Some(_) => Some(axis),
        }
    }
}

/// `passage_azimuth` for a station that may have no entry in the axes map.
pub fn passage_azimuth_at(axes: &HashMap<Id, Axes>, name: Id, arrival: Option<f64>) -> Option<f64> {
    match axes.get(&name) {
        Some(a) => a.passage_azimuth(arrival),
        None => arrival.filter(|a| a.is_finite()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn clustering_merges_close_bearings_only() {
        assert_eq!(cluster_bearings(&[0.0, 25.0, 180.0], 30.0).len(), 2);
        assert_eq!(cluster_bearings(&[0.0, 35.0, 180.0], 30.0).len(), 3);
        let c = cluster_bearings(&[350.0, 10.0], 30.0);
        assert_eq!(c.len(), 1);
        assert!(
            relative_bearing(c[0].mean, 0.0).abs() < 1e-6,
            "350 and 10 average to north"
        );
    }

    #[test]
    fn relative_bearing_wraps() {
        assert!(
            near(relative_bearing(350.0, 10.0), 20.0) && near(relative_bearing(10.0, 350.0), -20.0)
        );
        assert!(
            near(relative_bearing(0.0, 180.0), 180.0) && near(relative_bearing(0.0, -180.0), 180.0)
        );
    }

    #[test]
    fn normalize_snaps_a_hair_under_a_full_turn_to_north() {
        assert_eq!(normalize_az(359.999_999_999_9), Some(0.0));
        assert_eq!(normalize_az(-90.0), Some(270.0));
        assert_eq!(normalize_az(f64::NAN), None);
    }

    #[test]
    fn a_plumb_has_no_plan_bearing() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        assert_eq!(
            plan_bearing(a, Vec3::new(1e-14, 6e-15, -50.0)),
            None,
            "trigonometric dust is not a direction"
        );
        assert!(near(
            plan_bearing(a, Vec3::new(10.0, 0.0, 0.0)).unwrap(),
            90.0
        ));
    }

    #[test]
    fn through_station_axis_follows_travel_and_a_corner_is_not_through() {
        let through = Axes {
            dirs: vec![
                Dir {
                    mean: 0.0,
                    count: 1,
                },
                Dir {
                    mean: 180.0,
                    count: 1,
                },
            ],
            legs: 2,
        };
        assert!(near(through.passage_azimuth(Some(10.0)).unwrap(), 0.0));
        assert!(near(through.passage_azimuth(Some(190.0)).unwrap(), 180.0));
        let corner = Axes {
            dirs: vec![
                Dir {
                    mean: 0.0,
                    count: 1,
                },
                Dir {
                    mean: 90.0,
                    count: 1,
                },
            ],
            legs: 2,
        };
        assert!(corner.through_pair().is_none());
        assert!(
            near(corner.passage_azimuth(Some(45.0)).unwrap(), 45.0),
            "a corner keeps the arriving bearing"
        );
        assert!(
            passage_azimuth_at(&HashMap::new(), cave_id::Id::from_u128(1), Some(77.0)).unwrap()
                == 77.0
        );
    }

    #[test]
    fn a_dead_end_with_no_arriving_bearing_uses_its_only_way_out() {
        let foot = Axes {
            dirs: vec![Dir {
                mean: 90.0,
                count: 1,
            }],
            legs: 1,
        };
        assert!(near(foot.passage_azimuth(None).unwrap(), 90.0));
        assert_eq!(foot.passage_azimuth(Some(f64::NAN)).unwrap(), 90.0);
    }

    #[test]
    fn three_distinct_ways_out_is_a_junction() {
        let j = Axes {
            dirs: vec![
                Dir {
                    mean: 0.0,
                    count: 1,
                },
                Dir {
                    mean: 120.0,
                    count: 1,
                },
                Dir {
                    mean: 240.0,
                    count: 1,
                },
            ],
            legs: 3,
        };
        assert!(j.is_junction() && j.through_pair().is_none());
    }
}
