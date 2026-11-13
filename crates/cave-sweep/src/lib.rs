//! A sweep (an instrument's scan mode: up to a few hundred splays shot in one go at a station) turned into a passage
//! ring: one clean outline of the passage round the station, for the 3D passage, the cross-section and derived LRUD.
//!
//! Pure geometry, metres. Every splay is still kept as an ordinary splay in the notebook; this only reads them.
//!
//! How:
//! 1. **Clean**: drop misses (almost no distance) and near-duplicates.
//! 2. **Which way the passage runs**: a sweep that lies mostly in one plane (a ring shot across the passage) gives it
//!    as that plane's normal; otherwise (a spray round the room) the caller's direction (from the legs) is used.
//! 3. **Profile**: points are put on the cross-section plane (a spray's far points, up or down the passage, are left
//!    out) and, in each of [`BINS`] directions round the station, one distance is kept: the middle of the points for
//!    a ring, the outer shell for a spray. A direction far out of step with its neighbours (a shot that hit a caver,
//!    or went through a gap) is thrown out.
//! 4. **Fill and smooth**: an empty direction takes the distance between its nearest neighbours, then the outline is
//!    smoothed a little.
//! 5. **Out**: the ring (3D points round the station) and left, right, up and down read off it.

use serde::Serialize;

/// Directions round the station the outline is kept in.
pub const BINS: usize = 32;

/// How a sweep was shot, as worked out from its points.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Style {
    /// Mostly in one plane across the passage.
    Ring,
    /// Round the whole room.
    Spray,
}

/// A sweep made into a passage ring.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Ring {
    /// How it was shot.
    pub style: Style,
    /// Which way the passage runs here (unit vector, x east, y north, z up).
    pub axis: [f64; 3],
    /// The outline, [`BINS`] points round the station (relative to it, metres), anticlockwise looking along `axis`,
    /// starting to the right of the passage.
    pub points: Vec<[f64; 3]>,
    /// Distance from the station in each direction (metres), same order as `points`.
    pub radii: Vec<f64>,
    /// Left, right, up and down, metres, read off the outline.
    pub lrud: [f64; 4],
    /// Which of the input points were used (false: dropped as a miss, a repeat, out of the section, or out of step).
    pub kept: Vec<bool>,
}

type V = [f64; 3];
fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn scale(a: V, k: f64) -> V {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn len(a: V) -> f64 {
    dot(a, a).sqrt()
}
fn unit(a: V) -> Option<V> {
    let l = len(a);
    (l > 1e-9 && l.is_finite()).then(|| scale(a, 1.0 / l))
}

/// A splay as a vector from the station: tape (m), compass and clino (degrees, compass true north).
pub fn splay_vector(distance: f64, azimuth: f64, inclination: f64) -> [f64; 3] {
    let (a, i) = (azimuth.to_radians(), inclination.to_radians());
    [
        distance * i.cos() * a.sin(),
        distance * i.cos() * a.cos(),
        distance * i.sin(),
    ]
}

/// The covariance matrix's eigenvectors, smallest eigenvalue first (Jacobi rotations; 3x3, symmetric).
fn principal(points: &[V]) -> ([f64; 3], [V; 3]) {
    let n = points.len() as f64;
    let mean = points.iter().fold([0.0; 3], |m, p| {
        [m[0] + p[0] / n, m[1] + p[1] / n, m[2] + p[2] / n]
    });
    let mut c = [[0.0f64; 3]; 3];
    for p in points {
        let d = sub(*p, mean);
        for i in 0..3 {
            for j in 0..3 {
                c[i][j] += d[i] * d[j] / n;
            }
        }
    }
    let mut v = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..50 {
        let (mut p, mut q, mut big) = (0, 1, 0.0f64);
        for (i, j) in [(0, 1), (0, 2), (1, 2)] {
            if c[i][j].abs() > big {
                (p, q, big) = (i, j, c[i][j].abs());
            }
        }
        if big < 1e-15 {
            break;
        }
        let theta = (c[q][q] - c[p][p]) / (2.0 * c[p][q]);
        let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
        let t = if theta == 0.0 { 1.0 } else { t };
        let (cs, sn) = (1.0 / (t * t + 1.0).sqrt(), t / (t * t + 1.0).sqrt());
        let mut r = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        r[p][p] = cs;
        r[q][q] = cs;
        r[p][q] = sn;
        r[q][p] = -sn;
        // c = rᵀ c r, v = v r
        let mut cr = [[0.0; 3]; 3];
        for i in 0..3 {
            for j in 0..3 {
                cr[i][j] = (0..3).map(|k| c[i][k] * r[k][j]).sum();
            }
        }
        for i in 0..3 {
            for j in 0..3 {
                c[i][j] = (0..3).map(|k| r[k][i] * cr[k][j]).sum();
            }
        }
        let mut vr = [[0.0; 3]; 3];
        for i in 0..3 {
            for j in 0..3 {
                vr[i][j] = (0..3).map(|k| v[i][k] * r[k][j]).sum();
            }
        }
        v = vr;
    }
    let mut order = [0usize, 1, 2];
    order.sort_by(|&a, &b| c[a][a].total_cmp(&c[b][b]));
    let vals = [
        c[order[0]][order[0]],
        c[order[1]][order[1]],
        c[order[2]][order[2]],
    ];
    let vecs = order.map(|k| [v[0][k], v[1][k], v[2][k]]);
    (vals, vecs)
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

/// Makes a ring from a sweep's splays (vectors from the station, metres, as [`splay_vector`] gives). `passage` is
/// which way the passage runs here (from the legs), used when the sweep does not show it itself. `None` when there is
/// not enough to go on (too few shots, or all on one side).
pub fn ring(splays: &[[f64; 3]], passage: Option<[f64; 3]>) -> Option<Ring> {
    let n = splays.len();
    let mut kept = vec![false; n];
    // 1. misses and repeats
    let mut pts: Vec<(usize, V)> = Vec::new();
    for (i, p) in splays.iter().enumerate() {
        if !p.iter().all(|x| x.is_finite()) || len(*p) < 0.05 {
            continue;
        }
        if pts.iter().any(|(_, q)| len(sub(*p, *q)) < 0.02) {
            continue;
        }
        pts.push((i, *p));
    }
    if pts.len() < 6 {
        return None;
    }
    // 2. which way the passage runs
    let raw: Vec<V> = pts.iter().map(|(_, p)| *p).collect();
    let (vals, vecs) = principal(&raw);
    let flat = vals[2] > 0.0 && vals[0] / vals[2] < 0.12;
    // a flat sweep whose plane runs ALONG the passage (round the room horizontally in a level passage) is not a
    // cross-section: it is read like a spray, across the passage the legs show
    let along_passage = passage
        .and_then(unit)
        .is_some_and(|p| dot(p, vecs[0]).abs() < 0.5);
    let (style, mut axis) = if flat && !along_passage {
        (Style::Ring, vecs[0])
    } else {
        (Style::Spray, passage.and_then(unit).unwrap_or(vecs[2]))
    };
    // a ring's normal points either way: keep it the way the passage goes, or north-ish and level when unknown
    let reference = passage.and_then(unit).unwrap_or([0.0, 1.0, 0.0]);
    if dot(axis, reference) < 0.0 {
        axis = scale(axis, -1.0);
    }
    // the section's up (as near the real up as the passage allows) and right
    let z = [0.0, 0.0, 1.0];
    let up = unit(sub(z, scale(axis, dot(z, axis))))
        .or_else(|| unit(sub([0.0, 1.0, 0.0], scale(axis, axis[1]))))?;
    let right = cross(axis, up);
    // 3. on the section plane
    let mut radial: Vec<f64> = raw
        .iter()
        .map(|p| len(sub(*p, scale(axis, dot(*p, axis)))))
        .collect();
    let typical = median(&mut radial).max(0.2);
    let band = match style {
        Style::Ring => f64::INFINITY,
        Style::Spray => typical.max(0.75),
    };
    let mut bins: Vec<Vec<(usize, f64)>> = vec![Vec::new(); BINS];
    for (i, p) in &pts {
        if dot(*p, axis).abs() > band {
            continue;
        }
        let (x, y) = (dot(*p, right), dot(*p, up));
        let rho = x.hypot(y);
        if rho < 0.02 {
            continue;
        }
        let a = y.atan2(x).rem_euclid(std::f64::consts::TAU);
        let b = ((a / std::f64::consts::TAU * BINS as f64).floor() as usize).min(BINS - 1);
        bins[b].push((*i, rho));
    }
    let value = |v: &mut Vec<f64>| -> Option<f64> {
        if v.is_empty() {
            return None;
        }
        v.sort_by(f64::total_cmp);
        Some(match style {
            Style::Ring => median(v),
            Style::Spray => v[((v.len() as f64 - 1.0) * 0.8).round() as usize], // the outer shell
        })
    };
    let near_median = |vals: &[Option<f64>], k: usize| -> Option<f64> {
        let mut near: Vec<f64> = (1..=3)
            .flat_map(|d| [vals[(k + d) % BINS], vals[(k + BINS - d) % BINS]])
            .flatten()
            .collect();
        (near.len() >= 2).then(|| median(&mut near))
    };
    // first pass: each direction's distance, and from its neighbours what it should be about
    let first: Vec<Option<f64>> = bins
        .iter()
        .map(|b| value(&mut b.iter().map(|(_, x)| *x).collect()))
        .collect();
    // second pass: a shot far out of line with what its neighbours say (a caver, a gap) is dropped
    let mut r: Vec<Option<f64>> = vec![None; BINS];
    for (k, b) in bins.iter().enumerate() {
        let expect = near_median(&first, k).or(first[k]);
        let ok = |x: f64| expect.is_none_or(|e| x <= e * 2.0 && x >= e * 0.4);
        let mut v: Vec<f64> = b.iter().map(|(_, x)| *x).filter(|x| ok(*x)).collect();
        r[k] = value(&mut v);
        if r[k].is_some() {
            for (i, rho) in b {
                kept[*i] = ok(*rho);
            }
        }
    }
    // a whole direction still far out of step with its neighbours is thrown out
    let snapshot = r.clone();
    for k in 0..BINS {
        let (Some(me), Some(m)) = (snapshot[k], near_median(&snapshot, k)) else {
            continue;
        };
        if me > m * 2.5 || me < m * 0.35 {
            r[k] = None;
            for (i, _) in &bins[k] {
                kept[*i] = false;
            }
        }
    }
    let have: Vec<usize> = (0..BINS).filter(|&k| r[k].is_some()).collect();
    if have.len() < 4 {
        return None;
    }
    // 4. fill the gaps round the circle, then smooth (median of 3, then mean of 3)
    let mut filled = vec![0.0; BINS];
    for k in 0..BINS {
        filled[k] = match r[k] {
            Some(v) => v,
            None => {
                let back = (1..BINS)
                    .find(|d| r[(k + BINS - d) % BINS].is_some())
                    .unwrap();
                let fwd = (1..BINS).find(|d| r[(k + d) % BINS].is_some()).unwrap();
                let (a, b) = (
                    r[(k + BINS - back) % BINS].unwrap(),
                    r[(k + fwd) % BINS].unwrap(),
                );
                a + (b - a) * back as f64 / (back + fwd) as f64
            }
        };
    }
    let med: Vec<f64> = (0..BINS)
        .map(|k| {
            median(&mut [
                filled[(k + BINS - 1) % BINS],
                filled[k],
                filled[(k + 1) % BINS],
            ])
        })
        .collect();
    let radii: Vec<f64> = (0..BINS)
        .map(|k| (med[(k + BINS - 1) % BINS] + 2.0 * med[k] + med[(k + 1) % BINS]) / 4.0)
        .collect();
    // 5. the ring, and LRUD off it (the middle of each bin)
    let angle = |k: usize| (k as f64 + 0.5) / BINS as f64 * std::f64::consts::TAU;
    let points: Vec<V> = (0..BINS)
        .map(|k| {
            let (c, s) = (angle(k).cos(), angle(k).sin());
            [
                radii[k] * (c * right[0] + s * up[0]),
                radii[k] * (c * right[1] + s * up[1]),
                radii[k] * (c * right[2] + s * up[2]),
            ]
        })
        .collect();
    let at = |deg: f64| {
        let a = deg.to_radians().rem_euclid(std::f64::consts::TAU) / std::f64::consts::TAU
            * BINS as f64
            - 0.5;
        let (k0, f) = (a.floor().rem_euclid(BINS as f64) as usize, a - a.floor());
        radii[k0] * (1.0 - f) + radii[(k0 + 1) % BINS] * f
    };
    let lrud = [at(180.0), at(0.0), at(90.0), at(270.0)];
    Some(Ring {
        style,
        axis,
        points,
        radii,
        lrud,
        kept,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ring of shots round an elliptic passage running north: half-width `w`, half-height `h`, centred on the
    /// station, with `bad` shots far out of step.
    fn ellipse(w: f64, h: f64, shots: usize, along: f64, bad: &[usize]) -> Vec<V> {
        (0..shots)
            .map(|i| {
                let a = i as f64 / shots as f64 * std::f64::consts::TAU;
                let wobble = 1.0 + 0.03 * ((i * 7919) % 11) as f64 / 11.0;
                let k = if bad.contains(&i) { 4.0 } else { wobble };
                [
                    w * a.cos() * k,
                    along * ((i % 5) as f64 - 2.0) / 2.0,
                    h * a.sin() * k,
                ]
            })
            .collect()
    }

    #[test]
    fn a_ring_across_a_passage_gives_its_outline_and_lrud() {
        let pts = ellipse(2.0, 1.0, 60, 0.05, &[]);
        let r = ring(&pts, Some([0.0, 1.0, 0.0])).unwrap();
        assert_eq!(r.style, Style::Ring);
        assert!(r.axis[1] > 0.95, "runs north: {:?}", r.axis);
        let [l, rt, u, d] = r.lrud;
        for (got, want) in [(l, 2.0), (rt, 2.0), (u, 1.0), (d, 1.0)] {
            assert!((got - want).abs() < want * 0.12, "{:?}", r.lrud);
        }
        assert_eq!(r.points.len(), BINS);
        assert!(r.kept.iter().all(|k| *k));
    }

    #[test]
    fn stray_shots_are_thrown_out_and_do_not_bend_the_outline() {
        let pts = ellipse(2.0, 1.0, 60, 0.05, &[3, 17, 41]);
        let r = ring(&pts, Some([0.0, 1.0, 0.0])).unwrap();
        assert!(!r.kept[3] && !r.kept[17] && !r.kept[41]);
        let max = r.radii.iter().cloned().fold(0.0, f64::max);
        assert!(max < 2.4, "no spike from the strays: {max}");
    }

    #[test]
    fn a_gap_is_filled_from_its_neighbours() {
        // nothing shot on the floor (angles 200 to 340 degrees)
        let pts: Vec<V> = ellipse(2.0, 1.0, 72, 0.05, &[])
            .into_iter()
            .filter(|p| p[2] > -0.35)
            .collect();
        let r = ring(&pts, Some([0.0, 1.0, 0.0])).unwrap();
        assert!(
            r.radii
                .iter()
                .all(|x| x.is_finite() && *x > 0.5 && *x < 2.3)
        );
    }

    #[test]
    fn a_spray_round_the_room_uses_the_legs_direction_and_the_outer_shell() {
        // a passage 3 m wide and 2 m high running east, points sprayed up and down it
        let mut pts = Vec::new();
        for i in 0..200 {
            let a = i as f64 * 2.399963; // golden angle round the section
            let along = ((i * 37) % 21) as f64 / 10.0 - 1.0;
            let inside = if i % 4 == 0 { 0.6 } else { 1.0 }; // some shots hit nearer rocks
            pts.push([along, 1.5 * a.cos() * inside, 1.0 * a.sin() * inside]);
        }
        let r = ring(&pts, Some([1.0, 0.0, 0.0])).unwrap();
        assert_eq!(r.style, Style::Spray);
        assert!(r.axis[0] > 0.99);
        let [l, rt, u, d] = r.lrud;
        assert!(
            (l - 1.5).abs() < 0.3
                && (rt - 1.5).abs() < 0.3
                && (u - 1.0).abs() < 0.25
                && (d - 1.0).abs() < 0.25,
            "{:?}",
            r.lrud
        );
    }

    #[test]
    fn a_level_sweep_round_a_level_passage_is_read_across_the_passage() {
        // shots round the room horizontally (a plan outline), passage running north
        let pts: Vec<V> = (0..60)
            .map(|i| {
                let a = i as f64 / 60.0 * std::f64::consts::TAU;
                [2.0 * a.cos(), 4.0 * a.sin(), 0.02 * ((i % 3) as f64 - 1.0)]
            })
            .collect();
        let r = ring(&pts, Some([0.0, 1.0, 0.0])).unwrap();
        assert_eq!(r.style, Style::Spray);
        assert!(
            r.axis[1] > 0.99,
            "across the passage, not a vertical shaft: {:?}",
            r.axis
        );
        assert!(
            (r.lrud[0] - 2.0).abs() < 0.3 && (r.lrud[1] - 2.0).abs() < 0.3,
            "{:?}",
            r.lrud
        );
    }

    #[test]
    fn too_little_to_go_on_is_none() {
        assert!(ring(&[[1.0, 0.0, 0.0]; 3], None).is_none());
        assert!(ring(&[[0.0, 0.0, 0.0]; 40], None).is_none());
    }

    #[test]
    fn a_real_x1_sweep_makes_a_sane_ring() {
        let path = format!(
            "{}/../cave-instrument/tests/captures/x1-2026-11-13-legs-and-two-sweeps.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let packets: Vec<Vec<u8>> = v["records"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["t"] == "notify")
            .map(|r| {
                let h = r["hex"].as_str().unwrap();
                (0..h.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
                    .collect()
            })
            .collect();
        // the decoding here is the X1's (crates/cave-instrument/src/cavway.rs), kept tiny for the test
        let shot = |b: &[u8]| {
            let d =
                f64::from(u32::from(b[2]) << 16 | u32::from(b[4]) << 8 | u32::from(b[3])) / 1000.0;
            let az = f64::from(u16::from_le_bytes([b[5], b[6]])) * 360.0 / 65535.0;
            let inc = f64::from(i16::from_le_bytes([b[7], b[8]])) * 360.0 / 65535.0;
            splay_vector(d, az, inc)
        };
        let mut sweeps: Vec<Vec<V>> = Vec::new();
        for b in &packets[52..] {
            if b[1] & 0x40 != 0 {
                continue;
            }
            if b[1] & 0x60 == 0 {
                sweeps.push(Vec::new());
            }
            sweeps.last_mut().unwrap().push(shot(b));
        }
        assert_eq!(
            sweeps.iter().map(Vec::len).collect::<Vec<_>>(),
            vec![51, 54]
        );
        for s in &sweeps {
            let r = ring(s, None).expect("a ring");
            let longest = s.iter().map(|p| len(*p)).fold(0.0, f64::max);
            assert!(
                r.radii
                    .iter()
                    .all(|x| x.is_finite() && *x > 0.0 && *x <= longest + 1e-9),
                "{:?}",
                r.radii
            );
            assert!(r.lrud.iter().all(|x| *x > 0.0));
            let kept = r.kept.iter().filter(|k| **k).count();
            assert!(
                kept * 10 >= s.len() * 6,
                "most shots used: {kept} of {}",
                s.len()
            );
        }
    }
}

#[cfg(test)]
mod look {
    #[test]
    #[ignore]
    fn print_real_rings() {
        let path = format!(
            "{}/../cave-instrument/tests/captures/x1-2026-11-13-legs-and-two-sweeps.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let mut sweeps: Vec<Vec<[f64; 3]>> = Vec::new();
        for (i, r) in v["records"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["t"] == "notify")
            .enumerate()
        {
            let h = r["hex"].as_str().unwrap();
            let b: Vec<u8> = (0..h.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
                .collect();
            if i < 52 || b[1] & 0x40 != 0 {
                continue;
            }
            if b[1] & 0x60 == 0 {
                sweeps.push(Vec::new());
            }
            let d =
                f64::from(u32::from(b[2]) << 16 | u32::from(b[4]) << 8 | u32::from(b[3])) / 1000.0;
            let az = f64::from(u16::from_le_bytes([b[5], b[6]])) * 360.0 / 65535.0;
            let inc = f64::from(i16::from_le_bytes([b[7], b[8]])) * 360.0 / 65535.0;
            sweeps
                .last_mut()
                .unwrap()
                .push(super::splay_vector(d, az, inc));
        }
        for s in &sweeps {
            let r = super::ring(s, None).unwrap();
            let ft = |m: f64| m * 3.2808;
            println!(
                "{:?} axis {:.2?} lrud ft {:.1} {:.1} {:.1} {:.1} kept {}/{} radii ft {:?}",
                r.style,
                r.axis,
                ft(r.lrud[0]),
                ft(r.lrud[1]),
                ft(r.lrud[2]),
                ft(r.lrud[3]),
                r.kept.iter().filter(|k| **k).count(),
                s.len(),
                r.radii
                    .iter()
                    .map(|x| (ft(*x) * 10.0).round() / 10.0)
                    .collect::<Vec<_>>()
            );
        }
    }
}
