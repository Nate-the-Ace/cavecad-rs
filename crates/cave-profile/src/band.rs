//! One run unrolled into a band: stations at distance-along-run and elevation, plus ceiling and
//! floor from the U and D measurements.

use crate::runs::{Graph, Run, better_chain, longest_chain};
use cave_id::Id;
use cave_survey::{LegKind, StationLrud};
use std::collections::HashMap;

/// A station on the band.
#[derive(Clone, Debug, PartialEq)]
pub struct BandStation {
    /// The station.
    pub id: Id,
    /// Its label.
    pub label: String,
    /// Distance along the run in metres (the horizontal axis).
    pub s: f64,
    /// Elevation in metres (the vertical axis).
    pub z: f64,
    /// Ceiling height above the station, if measured.
    pub up: Option<f64>,
    /// Floor depth below the station, if measured.
    pub down: Option<f64>,
}

/// A surveyed leg on the band.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BandLeg {
    /// The shot.
    pub shot: Id,
    /// Start station.
    pub from: Id,
    /// End station.
    pub to: Id,
    /// How resolution used the leg.
    pub kind: LegKind,
    /// `s` at the start.
    pub from_s: f64,
    /// Elevation at the start.
    pub from_z: f64,
    /// `s` at the end.
    pub to_s: f64,
    /// Elevation at the end.
    pub to_z: f64,
}

/// Why a chain stopped short.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    /// A station has no elevation. Never defaulted to 0: that would rebase an absolute datum.
    NoElevation,
    /// No walked leg joins the two stations (a stop, not an invented link).
    NoLeg,
}

/// One run laid out on its band.
#[derive(Clone, Debug, PartialEq)]
pub struct Band {
    /// The run key.
    pub key: String,
    /// The station of the parent run this band ties in at, if any.
    pub tie: Option<Id>,
    /// Elevation of the band's first station.
    pub datum: Option<f64>,
    /// Stations in band order.
    pub stations: Vec<BandStation>,
    /// Legs between consecutive stations.
    pub legs: Vec<BandLeg>,
    /// Run stations left off the band (side branches).
    pub omitted: Vec<Id>,
    /// Where the chain stopped, if it did.
    pub stopped: Option<(Id, StopReason)>,
}

impl Band {
    fn runs_of(&self, value: impl Fn(&BandStation) -> Option<f64>) -> Vec<Vec<[f64; 2]>> {
        let mut out: Vec<Vec<[f64; 2]>> = Vec::new();
        let mut cur: Vec<[f64; 2]> = Vec::new();
        for st in &self.stations {
            match value(st) {
                Some(v) => cur.push([st.s, v]),
                None => {
                    if cur.len() >= 2 {
                        out.push(std::mem::take(&mut cur));
                    }
                    cur.clear();
                }
            }
        }
        if cur.len() >= 2 {
            out.push(cur);
        }
        out
    }

    /// Ceiling lines `(s, z + up)`: unbroken stretches of stations that have an up measurement.
    pub fn ceiling_runs(&self) -> Vec<Vec<[f64; 2]>> {
        self.runs_of(|s| s.up.map(|u| s.z + u))
    }

    /// Floor lines `(s, z - down)`.
    pub fn floor_runs(&self) -> Vec<Vec<[f64; 2]>> {
        self.runs_of(|s| s.down.map(|d| s.z - d))
    }
}

/// Unrolls a run onto its band. `tie` is the parent station to start from (for a spur), already
/// checked to exist. `z_of` gives each station's elevation; a station without one stops the chain.
pub fn unroll_band(
    run: &Run,
    tie: Option<Id>,
    graph: &Graph,
    labels: &HashMap<Id, String>,
    z_of: &dyn Fn(Id) -> Option<f64>,
    lruds: &HashMap<Id, StationLrud>,
) -> Band {
    let found = longest_chain(run, graph, labels);
    let mut chain = found.chain;
    let mut tied = None;
    if let Some(tie) = tie {
        // Attach where the chain first touches the tie station, keep the longer side of the
        // attachment (ties broken by the chain comparison), and start the band at the tie.
        if let Some(at) = chain
            .iter()
            .position(|c| graph.tie_leg_between(tie, *c).is_some())
        {
            let left_only = at;
            let right_only = chain.len() - at - 1;
            let left_kept: Vec<Id> = chain[..=at].iter().rev().copied().collect();
            let right_kept: Vec<Id> = chain[at..].to_vec();
            let keep_right = if left_only != right_only {
                right_only > left_only
            } else {
                better_chain(&right_kept, &left_kept, labels)
            };
            chain = if keep_right { right_kept } else { left_kept };
        }
        chain.insert(0, tie);
        tied = Some(tie);
    }

    let label = |s: Id| labels.get(&s).cloned().unwrap_or_default();
    let datum = chain.first().and_then(|s| z_of(*s));
    let (mut stations, mut legs): (Vec<BandStation>, Vec<BandLeg>) = (Vec::new(), Vec::new());
    let (mut stopped, mut s) = (None, 0.0);
    for (i, &id) in chain.iter().enumerate() {
        let Some(z) = z_of(id) else {
            stopped = Some((id, StopReason::NoElevation));
            break;
        };
        if i > 0 {
            let prev = chain[i - 1];
            let leg = if tied.is_some() && i == 1 {
                graph.tie_leg_between(prev, id)
            } else {
                graph.leg_between(prev, id)
            };
            let Some(leg) = leg else {
                stopped = Some((id, StopReason::NoLeg));
                break;
            };
            let last = stations.last().expect("a station precedes every leg");
            s += leg.plan.abs();
            legs.push(BandLeg {
                shot: leg.shot,
                from: prev,
                to: id,
                kind: leg.kind,
                from_s: last.s,
                from_z: last.z,
                to_s: s,
                to_z: z,
            });
        }
        let lrud = lruds.get(&id);
        stations.push(BandStation {
            id,
            label: label(id),
            s,
            z,
            up: lrud.and_then(|l| l.up),
            down: lrud.and_then(|l| l.down),
        });
    }

    let on: std::collections::HashSet<Id> = chain.iter().copied().collect();
    let omitted = run
        .stations
        .iter()
        .copied()
        .filter(|x| !on.contains(x))
        .collect();
    Band {
        key: run.key.clone(),
        tie: tied,
        datum,
        stations,
        legs,
        omitted,
        stopped,
    }
}
