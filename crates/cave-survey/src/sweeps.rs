//! Sweeps (an instrument's scan mode) made into passage rings for everything drawn from splays: the walls, the 3D
//! passage and the cross-section. A sweep's raw splays (up to a few hundred, some strays) are replaced by its clean
//! ring (`cave-sweep`); the notebook keeps every shot. The LRUD the ring gives fills only the sides no tape measured
//! (Nathan, 2026-11-13: quietly, and a measured LRUD always wins).

use crate::extract::{Splay, StationLrud};
use crate::network::Leg;
use crate::traverse::Reading;
use crate::vec3::Vec3;
use cave_id::Id;
use std::collections::HashMap;

/// Fewest shots in a sweep for it to be made into a ring (fewer stay as plain splays).
pub const MIN_SWEEP_SHOTS: usize = 8;

/// Which way the passage runs at a station: the legs that start or end there, each counted the way it was surveyed.
fn passage_at(legs: &[Leg], station: Id) -> Option<[f64; 3]> {
    let mut sum = [0.0; 3];
    for l in legs.iter().filter(|l| l.from == station || l.to == station) {
        let v = [l.offset.x, l.offset.y, l.offset.z];
        let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if n > 1e-9 {
            for k in 0..3 {
                sum[k] += v[k] / n;
            }
        }
    }
    let n = (sum[0] * sum[0] + sum[1] * sum[1] + sum[2] * sum[2]).sqrt();
    (n > 1e-6).then(|| [sum[0] / n, sum[1] / n, sum[2] / n])
}

fn fill(lrud: &mut StationLrud, from: [f64; 4], azimuth: Option<f64>) {
    let sides = [
        &mut lrud.left,
        &mut lrud.right,
        &mut lrud.up,
        &mut lrud.down,
    ];
    let mut filled = false;
    for (side, v) in sides.into_iter().zip(from) {
        if side.is_none() && v.is_finite() && v > 0.0 {
            *side = Some(v);
            filled = true;
        }
    }
    if filled && lrud.azimuth.is_none() {
        lrud.azimuth = azimuth;
    }
}

/// Replaces each sweep's splays (by station and sweep) with its ring, and fills missing LRUD from it.
pub(crate) fn ring_sweeps(
    splays: Vec<Splay>,
    legs: &[Leg],
    lruds: &mut HashMap<Id, StationLrud>,
    first_arrival: &mut HashMap<Id, StationLrud>,
) -> Vec<Splay> {
    if splays.iter().all(|s| s.sweep.is_none()) {
        return splays;
    }
    // the sweeps, in the order they were shot
    let mut order: Vec<(Id, Id)> = Vec::new();
    let mut groups: HashMap<(Id, Id), Vec<Splay>> = HashMap::new();
    let mut out = Vec::new();
    for s in splays {
        match s.sweep {
            Some(w) if !s.exclude_from_plot => {
                let key = (s.from, w);
                if !groups.contains_key(&key) {
                    order.push(key);
                }
                groups.entry(key).or_default().push(s);
            }
            _ => out.push(s),
        }
    }
    for key in order {
        let group = groups.remove(&key).unwrap_or_default();
        let station = key.0;
        let vectors: Vec<[f64; 3]> = group
            .iter()
            .map(|s| [s.offset.x, s.offset.y, s.offset.z])
            .collect();
        let ring = (group.len() >= MIN_SWEEP_SHOTS)
            .then(|| cave_sweep::ring(&vectors, passage_at(legs, station)))
            .flatten();
        let Some(ring) = ring else {
            out.extend(group); // too few, or nothing to go on: plain splays
            continue;
        };
        let first = group[0].shot;
        for p in &ring.points {
            let d = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            out.push(Splay {
                shot: first,
                from: station,
                reading: Reading {
                    distance: d,
                    azimuth: Some(p[0].atan2(p[1]).to_degrees().rem_euclid(360.0)),
                    inclination: Some((p[2] / d.max(1e-9)).clamp(-1.0, 1.0).asin().to_degrees()),
                    back_azimuth: None,
                    back_inclination: None,
                    azimuth_omitted: false,
                },
                offset: Vec3 {
                    x: p[0],
                    y: p[1],
                    z: p[2],
                },
                exclude_from_plot: false,
                sweep: Some(key.1),
            });
        }
        let a = ring.axis;
        let facing =
            (a[0].hypot(a[1]) > 0.2).then(|| a[0].atan2(a[1]).to_degrees().rem_euclid(360.0));
        fill(lruds.entry(station).or_default(), ring.lrud, facing);
        if let Some(f) = first_arrival.get_mut(&station) {
            fill(f, ring.lrud, facing);
        }
    }
    out
}
