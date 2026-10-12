//! Lofting the station rings into triangle strips.

use crate::frame::{RingPoint, cross, frame_at, normalize, ring_at};
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

/// How two neighbouring rings are joined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoftMode {
    /// The old app's rule: sample `max(len)` evenly spaced angles and take each ring's nearest
    /// measured point. Clustered measurements merge, so some measured points never appear. Kept for
    /// parity checks against the old output.
    Nearest,
    /// Every measured point of both rings becomes a vertex: strips run between the union of both
    /// rings' angles, each ring read by straight interpolation between its neighbouring points.
    Full,
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
    /// How rings are joined.
    pub loft: LoftMode,
    /// Draw a section through a station with no measurements, blended from its nearest measured
    /// neighbours and drawn paler. An inference, never a measurement, so it is optional.
    pub fill_gaps: bool,
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
    /// How many unmeasured stations were given an inferred section.
    pub filled: usize,
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

fn lerp3(a: Vec3, b: Vec3, t: f64) -> Vec3 {
    Vec3::new(
        a.x + (b.x - a.x) * t,
        a.y + (b.y - a.y) * t,
        a.z + (b.z - a.z) * t,
    )
}

/// Reads a cyclic, angle-sorted list at `theta` by straight interpolation between neighbours.
fn cyclic_interp<T: Copy>(
    angles: &[f64],
    vals: &[T],
    theta: f64,
    lerp: impl Fn(T, T, f64) -> T,
) -> T {
    let n = angles.len();
    if n == 1 {
        return vals[0];
    }
    let (i0, i1, a0, a1) = match angles.iter().position(|&a| a > theta) {
        Some(0) => (n - 1, 0, angles[n - 1] - 2.0 * PI, angles[0]),
        Some(h) => (h - 1, h, angles[h - 1], angles[h]),
        None => (n - 1, 0, angles[n - 1], angles[0] + 2.0 * PI),
    };
    let t = if (a1 - a0).abs() < 1e-12 {
        0.0
    } else {
        (theta - a0) / (a1 - a0)
    };
    lerp(vals[i0], vals[i1], t)
}

/// A ring reduced to one point per angle: where two measurements share an angle the farther one
/// wins (the wall is at least that far), as the old sketched-section rings did.
struct Envelope {
    angles: Vec<f64>,
    points: Vec<Vec3>,
    radii: Vec<f64>,
}

fn envelope(ring: &[RingPoint], center: Vec3) -> Envelope {
    let mut e = Envelope {
        angles: Vec::new(),
        points: Vec::new(),
        radii: Vec::new(),
    };
    for r in ring {
        let radius = r.p.minus(center).norm();
        if let Some(last) = e.angles.last()
            && (r.angle - last).abs() < 1e-9
        {
            let k = e.angles.len() - 1;
            if radius > e.radii[k] {
                e.points[k] = r.p;
                e.radii[k] = radius;
            }
            continue;
        }
        e.angles.push(r.angle);
        e.points.push(r.p);
        e.radii.push(radius);
    }
    e
}

/// Strips through EVERY measured point of both rings.
fn loft_full(mesh: &mut Mesh, a: &Envelope, b: &Envelope, col_a: [f64; 3], col_b: [f64; 3]) {
    if a.angles.len() < 3 || b.angles.len() < 3 {
        return;
    }
    let mut angles: Vec<f64> = a.angles.iter().chain(&b.angles).copied().collect();
    angles.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    angles.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
    let m = angles.len();
    for k in 0..m {
        let (t0, t1) = (
            angles[k],
            if k + 1 == m {
                angles[0] + 2.0 * PI
            } else {
                angles[k + 1]
            },
        );
        let at = |e: &Envelope, t: f64| cyclic_interp(&e.angles, &e.points, t, lerp3);
        mesh.quad([at(a, t0), at(a, t1), at(b, t1), at(b, t0)], col_a, col_b);
    }
}

fn tint(c: [f64; 3]) -> [f64; 3] {
    [c[0] * 0.5 + 0.225, c[1] * 0.5 + 0.275, c[2] * 0.5 + 0.31]
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

/// The two end rings of one ordinary leg, as the mesh used them.
#[derive(Clone, Debug)]
pub struct LegRings {
    /// The leg's shot.
    pub shot: Id,
    /// Start station.
    pub from: Id,
    /// End station.
    pub to: Id,
    /// Start position.
    pub a: Vec3,
    /// End position.
    pub b: Vec3,
    /// Ring at the start (measured points, or an inferred section when `inferred_a`).
    pub ring_a: Vec<RingPoint>,
    /// Ring at the end.
    pub ring_b: Vec<RingPoint>,
    /// The start ring is an inference from neighbours, not a measurement.
    pub inferred_a: bool,
    /// The end ring is an inference from neighbours, not a measurement.
    pub inferred_b: bool,
}

/// A cut across a passage between two rings, `t` of the way from the first to the second
/// (0 = at the start station, 1 = at the end): `(angle, world point)` around the axis, using every
/// measured point of both rings (the same rule as the tube's full lofting, so a cut at a station is
/// the tube's own ring there). `None` when a ring the cut needs has fewer than three points.
pub fn cut_outline(rings: &LegRings, t: f64) -> Option<Vec<(f64, Vec3)>> {
    const AT_END: f64 = 1e-6;
    let (need_a, need_b) = (t < 1.0 - AT_END, t > AT_END);
    if (need_a && rings.ring_a.len() < 3) || (need_b && rings.ring_b.len() < 3) {
        return None;
    }
    // At a station the cut is that station's own ring, with no extra vertices from the far end.
    if !need_b || !need_a {
        let (ring, centre) = if need_a {
            (&rings.ring_a, rings.a)
        } else {
            (&rings.ring_b, rings.b)
        };
        let e = envelope(ring, centre);
        return (e.angles.len() >= 3).then(|| e.angles.into_iter().zip(e.points).collect());
    }
    let (ea, eb) = (
        envelope(&rings.ring_a, rings.a),
        envelope(&rings.ring_b, rings.b),
    );
    if ea.angles.len() < 3 || eb.angles.len() < 3 {
        return None;
    }
    let mut angles: Vec<f64> = ea.angles.iter().chain(&eb.angles).copied().collect();
    angles.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    angles.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
    let at = |e: &Envelope, th: f64| cyclic_interp(&e.angles, &e.points, th, lerp3);
    Some(
        angles
            .into_iter()
            .map(|th| (th, lerp3(at(&ea, th), at(&eb, th), t)))
            .collect(),
    )
}

/// Builds the passage surface. Only ordinary ("new") legs are lofted; closures and ties draw no
/// surface of their own.
pub fn build(input: &MeshInput) -> Mesh {
    build_with_rings(input, &mut |_| {})
}

/// [`build`], also handing each ordinary leg's two end rings to `sink` (whether or not they are
/// complete enough to draw), so a cross-section can be cut from the very same rings.
pub fn build_with_rings(input: &MeshInput, sink: &mut dyn FnMut(&LegRings)) -> Mesh {
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

    let natural_dir = |s: Id| -> Option<Vec3> {
        if branches(s) {
            // A junction has no single direction; any leg's own direction will do for a section.
            legs_at.get(&s).and_then(|ls| {
                ls.iter().find_map(|l| {
                    let (a, b) = (input.stations.get(&l.from)?, input.stations.get(&l.to)?);
                    normalize(b.minus(*a))
                })
            })
        } else {
            direction_at(
                s,
                legs_at.get(&s).map_or(&[][..], |v| v.as_slice()),
                input.stations,
                &axes,
            )
        }
    };
    let raw_ring = |name: Id, st: Vec3, dir: Vec3| -> Vec<RingPoint> {
        let lrud = input
            .first_arrival_lruds
            .get(&name)
            .or_else(|| input.start_lruds.get(&name));
        ring_at(
            st,
            dir,
            lrud,
            by_station.get(&name).map_or(&[][..], |v| v.as_slice()),
        )
    };

    // Walkable neighbours (closures out) with leg lengths, for finding the nearest measured sections.
    let mut walk: HashMap<Id, Vec<(Id, f64)>> = HashMap::new();
    for l in input.legs.iter().filter(|l| l.kind != LegKind::Closure) {
        if let (Some(&a), Some(&b)) = (input.stations.get(&l.from), input.stations.get(&l.to)) {
            let d = b.minus(a).norm();
            walk.entry(l.from).or_default().push((l.to, d));
            walk.entry(l.to).or_default().push((l.from, d));
        }
    }
    let measured_ring = |s: Id| -> Option<Vec<RingPoint>> {
        let st = *input.stations.get(&s)?;
        let ring = raw_ring(s, st, natural_dir(s)?);
        (ring.len() >= 3).then_some(ring)
    };

    // An unmeasured station's section: blended from the nearest measured stations reached by
    // walking only through other unmeasured stations (up to two, weighted by walking distance).
    let infer = |name: Id, st: Vec3, dir: Vec3| -> Option<Vec<RingPoint>> {
        let mut best: Vec<(f64, Id)> = Vec::new();
        let mut dist: HashMap<Id, f64> = HashMap::from([(name, 0.0)]);
        let mut frontier = vec![name];
        while !frontier.is_empty() {
            frontier.sort_by(|a, b| {
                dist[b]
                    .partial_cmp(&dist[a])
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let cur = frontier.pop()?;
            for &(next, d) in walk.get(&cur).map_or(&[][..], |v| v.as_slice()) {
                let nd = dist[&cur] + d;
                if dist.get(&next).is_some_and(|&old| old <= nd) {
                    continue;
                }
                dist.insert(next, nd);
                if measured_ring(next).is_some() {
                    best.push((nd, next));
                } else {
                    frontier.push(next);
                }
            }
        }
        best.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        best.dedup_by_key(|b| b.1);
        best.truncate(2);
        if best.is_empty() {
            return None;
        }
        let total: f64 = best.iter().map(|b| b.0).sum();
        let weights: Vec<f64> = if best.len() == 1 || total <= 1e-12 {
            vec![1.0; 1]
        } else {
            vec![best[1].0 / total, best[0].0 / total]
        };
        let envs: Vec<Envelope> = best
            .iter()
            .take(weights.len())
            .filter_map(|&(_, id)| Some(envelope(&measured_ring(id)?, *input.stations.get(&id)?)))
            .collect();
        if envs.len() != weights.len() {
            return None;
        }
        let mut angles: Vec<f64> = envs.iter().flat_map(|e| e.angles.iter().copied()).collect();
        angles.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
        angles.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
        let frame = frame_at(dir)?;
        Some(
            angles
                .iter()
                .map(|&t| {
                    let r: f64 = envs
                        .iter()
                        .zip(&weights)
                        .map(|(e, w)| {
                            w * cyclic_interp(&e.angles, &e.radii, t, |a, b, x| a + (b - a) * x)
                        })
                        .sum();
                    let (u, v) = (r * t.cos(), r * t.sin());
                    RingPoint {
                        p: Vec3::new(
                            st.x + u * frame.right.x + v * frame.up.x,
                            st.y + u * frame.right.y + v * frame.up.y,
                            st.z + u * frame.right.z + v * frame.up.z,
                        ),
                        angle: t,
                    }
                })
                .collect(),
        )
    };

    let mut cache: HashMap<Id, (Vec<RingPoint>, bool)> = HashMap::new();
    let mut filled: HashSet<Id> = HashSet::new();
    let mut ring_for = |name: Id, st: Vec3, dir: Vec3| -> (Vec<RingPoint>, bool) {
        let junction = branches(name);
        if !junction && let Some(r) = cache.get(&name) {
            return r.clone();
        }
        let mut ring = raw_ring(name, st, dir);
        let mut inferred = false;
        if ring.len() < 3
            && input.fill_gaps
            && let Some(r) = infer(name, st, dir)
        {
            ring = r;
            inferred = true;
            filled.insert(name);
        }
        if !junction {
            cache.insert(name, (ring.clone(), inferred));
        }
        (ring, inferred)
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
        let ((ring_a, inf_a), (ring_b, inf_b)) =
            (ring_for(leg.from, a, dir_a), ring_for(leg.to, b, dir_b));
        sink(&LegRings {
            shot: leg.shot,
            from: leg.from,
            to: leg.to,
            a,
            b,
            ring_a: ring_a.clone(),
            ring_b: ring_b.clone(),
            inferred_a: inf_a,
            inferred_b: inf_b,
        });
        if ring_a.len() >= 3 && ring_b.len() >= 3 {
            let (mut col_a, mut col_b) = (color_at(leg.from, a), color_at(leg.to, b));
            if inf_a || inf_b {
                (col_a, col_b) = (tint(col_a), tint(col_b));
            }
            match input.loft {
                LoftMode::Nearest => loft(&mut mesh, &ring_a, &ring_b, col_a, col_b),
                LoftMode::Full => loft_full(
                    &mut mesh,
                    &envelope(&ring_a, a),
                    &envelope(&ring_b, b),
                    col_a,
                    col_b,
                ),
            }
            for r in ring_a.iter().chain(&ring_b) {
                mesh.grow(r.p);
            }
        }
    }
    mesh.filled = filled.len();
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use cave_doc::ActorId;
    use cave_survey::{extract, resolve};

    fn mesh_with(json: &str, color: ColorMode, loft: LoftMode, fill_gaps: bool) -> Mesh {
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
            loft,
            fill_gaps,
        })
    }

    fn mesh_of(json: &str, color: ColorMode) -> Mesh {
        mesh_with(json, color, LoftMode::Nearest, false)
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

    fn has_vertex(m: &Mesh, x: f64, y: f64, z: f64) -> bool {
        m.positions
            .chunks(3)
            .any(|v| (v[0] - x).abs() < 1e-9 && (v[1] - y).abs() < 1e-9 && (v[2] - z).abs() < 1e-9)
    }

    /// A station with clustered splays: the old sampling loses some, full lofting keeps all.
    const CLUSTERED: &str = r#"{"unit":"m","name":"c","trips":[{"name":"c","startLrud":{"left":2,"right":2,"up":2,"down":2}}],"shots":[
        {"from":"A1","to":"A2","distance":10,"azimuth":0,"inclination":0,"left":2,"right":2,"up":2,"down":2},
        {"from":"A2","to":"A3","distance":10,"azimuth":0,"inclination":0,"left":2,"right":2,"up":2,"down":2},
        {"from":"A2","to":"","distance":3,"azimuth":90,"inclination":10,"splay":true},
        {"from":"A2","to":"","distance":3.5,"azimuth":90,"inclination":22,"splay":true},
        {"from":"A2","to":"","distance":4,"azimuth":90,"inclination":31,"splay":true},
        {"from":"A2","to":"","distance":2.5,"azimuth":270,"inclination":-15,"splay":true}]}"#;

    #[test]
    fn full_lofting_keeps_every_measured_point() {
        let doc = cave_fixture::from_shot_list(CLUSTERED, ActorId(1)).unwrap();
        let ex = extract(&doc);
        let r = resolve(&ex.legs, &ex.fixed);
        let id = *ex
            .first_arrival_lruds
            .keys()
            .find(|k| r.stations[*k].pos.y == 10.0)
            .unwrap();
        let by: Vec<&cave_survey::Splay> = ex.splays.iter().filter(|s| s.from == id).collect();
        let ring = crate::frame::ring_at(
            r.stations[&id].pos,
            Vec3::new(0.0, 1.0, 0.0),
            ex.first_arrival_lruds.get(&id),
            &by,
        );
        assert!(ring.len() >= 8, "four ticks and four splays");
        let full = mesh_with(CLUSTERED, ColorMode::Trip, LoftMode::Full, false);
        for p in &ring {
            assert!(
                has_vertex(&full, p.p.x, p.p.y, p.p.z),
                "measured point at angle {:.2} is a vertex",
                p.angle
            );
        }
        let old = mesh_with(CLUSTERED, ColorMode::Trip, LoftMode::Nearest, false);
        let lost = ring
            .iter()
            .filter(|p| !has_vertex(&old, p.p.x, p.p.y, p.p.z))
            .count();
        assert!(
            lost > 0,
            "the old sampling drops some of these ({lost}), which is what full lofting fixes"
        );
    }

    const GAP: &str = r#"{"unit":"m","name":"g","trips":[{"name":"g","startLrud":{"left":1,"right":3,"up":1,"down":1}}],"shots":[
        {"from":"A1","to":"A2","distance":10,"azimuth":0,"inclination":0,"left":1,"right":3,"up":1,"down":1},
        {"from":"A2","to":"A3","distance":10,"azimuth":0,"inclination":0},
        {"from":"A3","to":"A4","distance":10,"azimuth":0,"inclination":0,"left":3,"right":1,"up":1,"down":1}]}"#;

    #[test]
    fn an_unmeasured_station_breaks_the_tube_unless_gaps_are_filled() {
        // A3 has no LRUD: it is the unmeasured one, sitting between A2 and A4 (both measured).
        let broken = mesh_with(GAP, ColorMode::Trip, LoftMode::Full, false);
        assert_eq!(
            broken.triangles(),
            8,
            "only A1-A2 has a measured section at both ends"
        );
        assert_eq!(broken.filled, 0);

        let filled = mesh_with(GAP, ColorMode::Trip, LoftMode::Full, true);
        assert_eq!(filled.filled, 1, "A3 got an inferred section");
        assert_eq!(filled.triangles(), 24, "all three legs now have a tube");
    }

    #[test]
    fn the_inferred_section_is_halfway_between_its_neighbours_and_paler() {
        let filled = mesh_with(GAP, ColorMode::Trip, LoftMode::Full, true);
        // A3 is at y = 20. A2 has left 1 / right 3, A4 has left 3 / right 1: halfway is 2 and 2.
        assert!(has_vertex(&filled, -2.0, 20.0, 0.0) && has_vertex(&filled, 2.0, 20.0, 0.0));
        let plain = mesh_with(GAP, ColorMode::Trip, LoftMode::Full, false);
        let trip0 = &plain.colors[0..3];
        // Triangles that touch the inferred section are tinted toward grey-blue, the measured one is not.
        let tinted = filled
            .colors
            .chunks(9)
            .filter(|c| (c[0] - trip0[0]).abs() > 1e-6 || (c[1] - trip0[1]).abs() > 1e-6)
            .count();
        assert!(
            tinted > 0 && tinted < filled.triangles(),
            "{tinted} of {} triangles are inferred",
            filled.triangles()
        );
    }

    #[test]
    fn a_dead_end_with_no_measurements_copies_its_nearest_section() {
        let text = r#"{"unit":"m","name":"e","trips":[{"name":"e","startLrud":{"left":2,"right":2,"up":2,"down":2}}],"shots":[
            {"from":"A1","to":"A2","distance":10,"azimuth":0,"inclination":0,"left":2,"right":2,"up":2,"down":2},
            {"from":"A2","to":"A3","distance":5,"azimuth":0,"inclination":0}]}"#;
        let m = mesh_with(text, ColorMode::Trip, LoftMode::Full, true);
        assert_eq!(m.filled, 1);
        assert!(
            has_vertex(&m, -2.0, 15.0, 0.0) && has_vertex(&m, 0.0, 15.0, 2.0),
            "the last section is carried forward"
        );
    }

    #[test]
    fn filling_never_changes_measured_geometry() {
        let (plain, filled) = (
            mesh_with(TUBE, ColorMode::Trip, LoftMode::Full, false),
            mesh_with(TUBE, ColorMode::Trip, LoftMode::Full, true),
        );
        assert_eq!(plain.positions, filled.positions);
        assert_eq!(filled.filled, 0);
    }
}
