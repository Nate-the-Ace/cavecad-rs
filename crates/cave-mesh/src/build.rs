//! Lofting the station rings into triangle strips.

use crate::frame::{RingPoint, cross, normalize, ring_at};
use cave_id::Id;
use cave_survey::{LegKind, LegResult, Splay, StationLrud, Vec3};
use cave_walls::{AXIS_TOLERANCE_DEG, Axes, plan_bearing, stations_axes};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::f64::consts::PI;

/// How the passage is coloured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    /// One colour per survey trip.
    Trip,
    /// Shallow to deep.
    Depth,
}

const TRIP_COLORS: [[f64; 3]; 6] = [
    [0.85, 0.84, 0.78],
    [0.90, 0.55, 0.30],
    [0.40, 0.70, 0.90],
    [0.60, 0.85, 0.45],
    [0.88, 0.48, 0.62],
    [0.75, 0.70, 0.35],
];

fn depth_color(t: f64) -> [f64; 3] {
    let t = if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.5
    };
    [0.25 + 0.60 * t, 0.45 + 0.40 * t, 0.70 + 0.25 * (1.0 - t)]
}

/// Everything the mesh is built from. Positions may be raw or adjusted.
pub struct MeshInput<'a> {
    /// Where each station is.
    pub stations: &'a BTreeMap<Id, Vec3>,
    /// The resolved legs in resolution order.
    pub legs: &'a [LegResult],
    /// Splays that may become wall points.
    pub splays: &'a [Splay],
    /// The walls on the first shot arriving at each station.
    pub first_arrival_lruds: &'a HashMap<Id, StationLrud>,
    /// Walls typed on a trip's opening row, for stations nothing arrives at.
    pub start_lruds: &'a HashMap<Id, StationLrud>,
    /// Trip index of each station.
    pub station_trip: &'a HashMap<Id, usize>,
    /// Colouring.
    pub color: ColorMode,
}

/// Flat triangle buffers: every triangle has its own three vertices and a flat normal.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    /// x, y, z per vertex.
    pub positions: Vec<f64>,
    /// Unit normal per vertex.
    pub normals: Vec<f64>,
    /// Red, green, blue per vertex.
    pub colors: Vec<f64>,
    /// Triangle indices (`0, 1, 2, 3, 4, 5, ...`).
    pub indices: Vec<u32>,
    /// `(min, max)` around the stations, rings and all, or `None` when empty.
    pub bounds: Option<(Vec3, Vec3)>,
}

impl Mesh {
    /// Number of triangles.
    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }

    fn quad(&mut self, p: [Vec3; 4], col_a: [f64; 3], col_b: [f64; 3]) {
        for (a, b, c, col) in [(p[0], p[1], p[2], col_b), (p[0], p[2], p[3], col_a)] {
            let Some(n) = normalize(cross(b.minus(a), c.minus(a))) else {
                continue;
            };
            let base = (self.positions.len() / 3) as u32;
            for v in [a, b, c] {
                self.positions.extend([v.x, v.y, v.z]);
                self.normals.extend([n.x, n.y, n.z]);
                self.colors.extend(col);
            }
            self.indices.extend([base, base + 1, base + 2]);
        }
    }

    fn grow(&mut self, p: Vec3) {
        self.bounds = Some(match self.bounds {
            None => (p, p),
            Some((lo, hi)) => (
                Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z)),
                Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z)),
            ),
        });
    }
}

fn nearest_by_angle(ring: &[RingPoint], angle: f64) -> RingPoint {
    let mut best = ring[0];
    let mut best_gap = f64::INFINITY;
    for r in ring {
        let mut gap = (r.angle - angle).abs();
        if gap > PI {
            gap = 2.0 * PI - gap;
        }
        if gap < best_gap {
            best_gap = gap;
            best = *r;
        }
    }
    best
}

/// Strips between two rings, sampled at `max(len)` evenly spaced angles; each strip takes the
/// nearest measured point of each ring.
fn loft(mesh: &mut Mesh, a: &[RingPoint], b: &[RingPoint], col_a: [f64; 3], col_b: [f64; 3]) {
    let n = a.len().max(b.len());
    if n < 3 || a.is_empty() || b.is_empty() {
        return;
    }
    let angle_of = |i: usize| -PI + 2.0 * PI * ((i % n) as f64 / n as f64);
    for i in 0..n {
        let (a0, a1) = (
            nearest_by_angle(a, angle_of(i)),
            nearest_by_angle(a, angle_of(i + 1)),
        );
        let (b0, b1) = (
            nearest_by_angle(b, angle_of(i)),
            nearest_by_angle(b, angle_of(i + 1)),
        );
        mesh.quad([a0.p, a1.p, b1.p, b0.p], col_a, col_b);
    }
}

/// The direction of travel through a station: the mean of the unit vectors of the legs touching it,
/// with legs that leave the same way folded to one so a tie-in does not tilt the ring.
fn direction_at(
    name: Id,
    legs_at: &[&LegResult],
    stations: &BTreeMap<Id, Vec3>,
    axes: &HashMap<Id, Axes>,
) -> Option<Vec3> {
    let mut sum = Vec3::ZERO;
    let mut n = 0;
    let mut seen: HashSet<usize> = HashSet::new();
    for leg in legs_at {
        let (Some(&a), Some(&b)) = (stations.get(&leg.from), stations.get(&leg.to)) else {
            continue;
        };
        let Some(v) = normalize(b.minus(a)) else {
            continue;
        };
        let outward = if leg.from == name {
            plan_bearing(a, b)
        } else {
            plan_bearing(b, a)
        };
        if let Some(way) = axes.get(&name).and_then(|ax| ax.nearest_cluster(outward))
            && !seen.insert(way)
        {
            continue;
        }
        sum = sum.plus(v);
        n += 1;
    }
    (n > 0).then(|| normalize(sum)).flatten()
}

/// Builds the passage surface. Only ordinary ("new") legs are lofted; closures and ties draw no
/// surface of their own.
pub fn build(input: &MeshInput) -> Mesh {
    let mut mesh = Mesh::default();
    let axes = stations_axes(input.stations, input.legs, AXIS_TOLERANCE_DEG);
    let mut counts: HashMap<Id, usize> = HashMap::new();
    let mut legs_at: HashMap<Id, Vec<&LegResult>> = HashMap::new();
    for l in input.legs {
        for s in [l.from, l.to] {
            *counts.entry(s).or_default() += 1;
            legs_at.entry(s).or_default().push(l);
        }
    }
    let mut by_station: HashMap<Id, Vec<&Splay>> = HashMap::new();
    for s in input.splays.iter().filter(|s| !s.exclude_from_plot) {
        by_station.entry(s.from).or_default().push(s);
    }
    let branches = |n: Id| {
        axes.get(&n).map_or_else(
            || counts.get(&n).copied().unwrap_or(0) >= 3,
            Axes::is_junction,
        )
    };

    let (z_low, z_high) = input
        .stations
        .values()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p.z), hi.max(p.z))
        });
    let z_span = z_high - z_low;
    let color_at = |s: Id, p: Vec3| -> [f64; 3] {
        match input.color {
            ColorMode::Depth => depth_color(if z_span > 1e-9 {
                (p.z - z_low) / z_span
            } else {
                0.5
            }),
            ColorMode::Trip => {
                TRIP_COLORS[input.station_trip.get(&s).copied().unwrap_or(0) % TRIP_COLORS.len()]
            }
        }
    };

    let mut cache: HashMap<Id, Vec<RingPoint>> = HashMap::new();
    let mut ring_for = |name: Id, st: Vec3, dir: Vec3| -> Vec<RingPoint> {
        let junction = branches(name);
        if !junction && let Some(r) = cache.get(&name) {
            return r.clone();
        }
        let lrud = input
            .first_arrival_lruds
            .get(&name)
            .or_else(|| input.start_lruds.get(&name));
        let ring = ring_at(
            st,
            dir,
            lrud,
            by_station.get(&name).map_or(&[][..], |v| v.as_slice()),
        );
        if !junction {
            cache.insert(name, ring.clone());
        }
        ring
    };

    for leg in input.legs {
        let (Some(&a), Some(&b)) = (input.stations.get(&leg.from), input.stations.get(&leg.to))
        else {
            continue;
        };
        mesh.grow(a);
        mesh.grow(b);
        if leg.kind != LegKind::New {
            continue;
        }
        let Some(along) = normalize(b.minus(a)) else {
            continue;
        };
        let dir_of = |s: Id| -> Vec3 {
            if branches(s) {
                along
            } else {
                direction_at(
                    s,
                    legs_at.get(&s).map_or(&[][..], |v| v.as_slice()),
                    input.stations,
                    &axes,
                )
                .unwrap_or(along)
            }
        };
        let (dir_a, dir_b) = (dir_of(leg.from), dir_of(leg.to));
        let (ring_a, ring_b) = (ring_for(leg.from, a, dir_a), ring_for(leg.to, b, dir_b));
        if ring_a.len() >= 3 && ring_b.len() >= 3 {
            loft(
                &mut mesh,
                &ring_a,
                &ring_b,
                color_at(leg.from, a),
                color_at(leg.to, b),
            );
            for r in ring_a.iter().chain(&ring_b) {
                mesh.grow(r.p);
            }
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use cave_doc::ActorId;
    use cave_survey::{extract, resolve};

    fn mesh_of(json: &str, color: ColorMode) -> Mesh {
        let doc = cave_fixture::from_shot_list(json, ActorId(1)).unwrap();
        let ex = extract(&doc);
        let r = resolve(&ex.legs, &ex.fixed);
        let stations: BTreeMap<Id, Vec3> = r.stations.iter().map(|(id, p)| (*id, p.pos)).collect();
        build(&MeshInput {
            stations: &stations,
            legs: &r.legs,
            splays: &ex.splays,
            first_arrival_lruds: &ex.first_arrival_lruds,
            start_lruds: &ex.start_lruds,
            station_trip: &ex.station_trip,
            color,
        })
    }

    const TUBE: &str = r#"{"unit":"m","name":"t","trips":[{"name":"t","startLrud":{"left":1,"right":1,"up":1,"down":1}}],"shots":[
        {"from":"A1","to":"A2","distance":10,"azimuth":0,"inclination":0,"left":1,"right":1,"up":1,"down":1},
        {"from":"A2","to":"A3","distance":10,"azimuth":0,"inclination":0,"left":1,"right":1,"up":1,"down":1}]}"#;

    #[test]
    fn a_passage_with_four_walls_measured_is_a_closed_tube() {
        let m = mesh_of(TUBE, ColorMode::Trip);
        assert_eq!(
            m.triangles(),
            2 * 4 * 2,
            "two legs, four strips each, two triangles per strip"
        );
        assert!(
            m.positions.iter().all(|v| v.is_finite()) && m.normals.iter().all(|v| v.is_finite())
        );
        let (lo, hi) = m.bounds.unwrap();
        assert!(
            (lo.x + 1.0).abs() < 1e-9 && (hi.x - 1.0).abs() < 1e-9,
            "one metre either side"
        );
        assert!((lo.z + 1.0).abs() < 1e-9 && (hi.z - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_leg_without_enough_measurements_draws_no_surface() {
        let text = r#"{"unit":"m","name":"t","shots":[{"from":"A1","to":"A2","distance":10,"azimuth":0,"inclination":0,"left":1,"right":1}]}"#;
        assert_eq!(
            mesh_of(text, ColorMode::Trip).triangles(),
            0,
            "two points do not make a ring"
        );
    }

    #[test]
    fn a_splay_into_a_dome_adds_a_wall_point() {
        let plain = mesh_of(TUBE, ColorMode::Trip);
        let dome = TUBE.replace("\"shots\":[", "\"shots\":[{\"from\":\"A2\",\"to\":\"\",\"distance\":4,\"azimuth\":90,\"inclination\":60,\"splay\":true},");
        let with_splay = mesh_of(&dome, ColorMode::Trip);
        assert!(
            with_splay.triangles() > plain.triangles(),
            "the splay tip became part of the ring"
        );
        assert!(
            with_splay.bounds.unwrap().1.z > 3.0,
            "the dome shows as height"
        );
    }

    #[test]
    fn a_vertical_shaft_builds_finite_geometry() {
        let text = r#"{"unit":"m","name":"shaft","shots":[
            {"from":"P1","to":"P2","distance":20,"inclination":-90,"azimuthOmitted":true,"left":2,"right":2,"up":2,"down":2},
            {"from":"P2","to":"P3","distance":20,"inclination":-90,"azimuthOmitted":true,"left":2,"right":2,"up":2,"down":2},
            {"from":"P1","to":"","distance":1,"azimuth":0,"inclination":0,"splay":true}]}"#;
        let m = mesh_of(text, ColorMode::Depth);
        assert!(m.triangles() > 0);
        assert!(
            m.positions
                .iter()
                .chain(&m.normals)
                .chain(&m.colors)
                .all(|v| v.is_finite())
        );
    }

    #[test]
    fn depth_colours_run_from_deep_to_shallow() {
        let text = r#"{"unit":"m","name":"d","trips":[{"name":"d","startLrud":{"left":1,"right":1,"up":1,"down":1}}],"shots":[
            {"from":"A1","to":"A2","distance":10,"azimuth":0,"inclination":-45,"left":1,"right":1,"up":1,"down":1},
            {"from":"A2","to":"A3","distance":10,"azimuth":0,"inclination":-45,"left":1,"right":1,"up":1,"down":1}]}"#;
        let m = mesh_of(text, ColorMode::Depth);
        let reds: Vec<f64> = m.colors.chunks(3).map(|c| c[0]).collect();
        assert!(
            reds.iter().cloned().fold(f64::MAX, f64::min) < 0.5
                && reds.iter().cloned().fold(0.0, f64::max) > 0.8
        );
    }
}
