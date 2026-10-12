//! Wall polylines for a resolved survey.
//!
//! Walks the legs in resolution order, collecting each side's wall points. A run breaks at a
//! junction (three or more distinct ways out), at a station with no wall evidence at all, at a
//! closure leg, and where the next leg does not continue from the previous one's arrival. Each
//! break starts a new polyline rather than inventing a connection.
//!
//! What is not a break: a loop tie-in (the closure leg merges with the leg it runs alongside) and
//! a side written "P" (open passage is a reading; the run carries through it). Every point in every
//! run is a measured tick end or a splay tip; only which of them are joined can vary.

use crate::axes::{AXIS_TOLERANCE_DEG, Axes, passage_azimuth_at, stations_axes};
use crate::points::{Side, has_plan_evidence, station_wall_points};
use cave_id::Id;
use cave_survey::{
    LegKind, LegResult, Reading, Splay, StationLrud, Vec3, effective_azimuth, is_plumb,
};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Everything wall building reads. Positions may be raw or adjusted.
pub struct WallInput<'a> {
    /// Where each station is.
    pub stations: &'a BTreeMap<Id, Vec3>,
    /// The resolved legs in resolution order.
    pub legs: &'a [LegResult],
    /// What was read on each leg, by shot.
    pub readings: &'a HashMap<Id, Reading>,
    /// Walls measured at each station by the last shot arriving there.
    pub lruds: &'a HashMap<Id, StationLrud>,
    /// Walls typed on a trip's opening row, for stations nothing arrives at.
    pub start_lruds: &'a HashMap<Id, StationLrud>,
    /// Splays that may become wall points.
    pub splays: &'a [Splay],
}

/// One continuous wall line and the stations that contributed points to it.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    /// The wall points in order (3D).
    pub points: Vec<Vec3>,
    /// Stations that contributed at least one point on this side, in run order, once each.
    pub stations: Vec<Id>,
}

/// Both walls of the passage.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Walls {
    /// Left-hand wall runs.
    pub left: Vec<Run>,
    /// Right-hand wall runs.
    pub right: Vec<Run>,
}

#[derive(Default)]
struct Buf {
    points: Vec<Vec3>,
    names: Vec<Id>,
    seen: HashSet<Id>,
    runs: Vec<Run>,
}

impl Buf {
    fn append(&mut self, pts: Vec<Vec3>, station: Id) {
        if pts.is_empty() {
            return;
        }
        self.points.extend(pts);
        if self.seen.insert(station) {
            self.names.push(station);
        }
    }

    fn flush(&mut self) {
        if self.points.len() >= 2 {
            self.runs.push(Run {
                points: std::mem::take(&mut self.points),
                stations: std::mem::take(&mut self.names),
            });
        }
        self.points.clear();
        self.names.clear();
        self.seen.clear();
    }
}

/// Builds the wall runs for both sides.
pub fn wall_runs(input: &WallInput) -> Walls {
    let axes = stations_axes(input.stations, input.legs, AXIS_TOLERANCE_DEG);
    let mut counts: HashMap<Id, usize> = HashMap::new();
    for l in input.legs {
        *counts.entry(l.from).or_default() += 1;
        *counts.entry(l.to).or_default() += 1;
    }
    let mut by_station: HashMap<Id, Vec<&Splay>> = HashMap::new();
    for s in input.splays.iter().filter(|s| !s.exclude_from_plot) {
        by_station.entry(s.from).or_default().push(s);
    }
    // The station the axes never saw (never placed, or every leg a pitch) has no geometry to
    // cluster, so the old leg count is the only evidence left.
    let is_junction = |n: Id| -> bool {
        axes.get(&n).map_or_else(
            || counts.get(&n).copied().unwrap_or(0) > 2,
            Axes::is_junction,
        )
    };
    let arrival_az = |leg: &LegResult| input.readings.get(&leg.shot).and_then(effective_azimuth);

    let (mut left, mut right) = (Buf::default(), Buf::default());
    let points_for =
        |name: Id, side: Side, lrud: Option<&StationLrud>, passage: Option<f64>| -> Vec<Vec3> {
            match input.stations.get(&name) {
                Some(&st) => station_wall_points(
                    st,
                    passage,
                    lrud,
                    by_station.get(&name).map_or(&[][..], |v| v.as_slice()),
                    side,
                ),
                None => Vec::new(),
            }
        };

    // The station a run BEGINS at is never any leg's arrival, so its own walls are seeded here.
    let seed = |leg: &LegResult, left: &mut Buf, right: &mut Buf| {
        let name = leg.from;
        if is_junction(name) {
            return;
        }
        let passage = passage_azimuth_at(&axes, name, arrival_az(leg));
        let mut lrud = input.lruds.get(&name).copied();
        if lrud.is_none()
            && let Some(start) = input.start_lruds.get(&name)
        {
            // The tick swings off the bearing the caver faced; a pitch has none, so a plumb or
            // bearing-less first leg gives no bearing and the passage direction is used instead.
            let plumb = input
                .readings
                .get(&leg.shot)
                .is_some_and(|r| r.azimuth_omitted || is_plumb(r));
            lrud = Some(StationLrud {
                azimuth: if plumb { None } else { arrival_az(leg) },
                ..*start
            });
        }
        left.append(points_for(name, Side::Left, lrud.as_ref(), passage), name);
        right.append(points_for(name, Side::Right, lrud.as_ref(), passage), name);
    };

    let mut prev_to: Option<Id> = None;
    for leg in input.legs {
        if leg.kind == LegKind::Closure {
            left.flush();
            right.flush();
            prev_to = None;
            continue;
        }
        let mut opens_run = prev_to.is_none();
        if prev_to.is_some_and(|p| p != leg.from) {
            // A second branch off a junction placed right after the first: what is buffered is a
            // real run, what is next is not part of it.
            left.flush();
            right.flush();
            opens_run = true;
        }
        if opens_run {
            seed(leg, &mut left, &mut right);
        }
        let name = leg.to;
        let lrud = input.lruds.get(&name);
        let passage = passage_azimuth_at(&axes, name, arrival_az(leg));
        let lp = points_for(name, Side::Left, lrud, passage);
        let rp = points_for(name, Side::Right, lrud, passage);
        if is_junction(name) {
            left.append(lp, name);
            right.append(rp, name);
            left.flush();
            right.flush();
        } else if lp.is_empty() && rp.is_empty() && !has_plan_evidence(lrud) {
            left.flush();
            right.flush();
        } else {
            left.append(lp, name);
            right.append(rp, name);
        }
        prev_to = Some(name);
    }
    left.flush();
    right.flush();
    Walls {
        left: left.runs,
        right: right.runs,
    }
}
