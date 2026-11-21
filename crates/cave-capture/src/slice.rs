//! Slicing a LiDAR room into wall lines: the mesh cut by a plane, the cut segments joined into
//! polylines, slivers dropped and the lines simplified. Derived on the fly, never stored (spec
//! `docs/superpowers/specs/2026-11-17-lidar-room-capture-design.md`, "Walls and sections").
//! Pure: numbers in, numbers out; builds for wasm32.
//!
//! Positions are survey axes (x east, y north, z up), in metres, relative to any origin the caller
//! picks (a station). A closed polyline repeats its first point at its end; gaps in the scan stay
//! gaps (an open polyline), nothing is invented.

use crate::bundle::{BundleError, HeaderInfo, MAGIC, check_header};
use std::collections::HashMap;

/// Cut points closer than this (metres) are the same point when segments are joined.
pub const JOIN_M: f64 = 0.001;
/// A polyline shorter than this in total (metres) is a sliver and is dropped.
pub const MIN_LENGTH_M: f64 = 0.05;
/// Douglas-Peucker tolerance (metres) for the kept polylines.
pub const SIMPLIFY_M: f64 = 0.01;

type P3 = [f64; 3];

fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: P3, b: P3) -> P3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn len(a: P3) -> f64 {
    dot(a, a).sqrt()
}
fn unit(a: P3) -> Option<P3> {
    let l = len(a);
    (l.is_finite() && l > 1e-12).then(|| [a[0] / l, a[1] / l, a[2] / l])
}

/// Joins cut points within [`JOIN_M`] of each other into one node (a 1 mm grid, neighbours searched).
struct Nodes {
    cells: HashMap<[i64; 3], Vec<usize>>,
    at: Vec<P3>,
}

impl Nodes {
    fn id(&mut self, p: P3) -> usize {
        let key = p.map(|x| (x / JOIN_M).floor() as i64);
        let mut best: Option<(f64, usize)> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let k = [key[0] + dx, key[1] + dy, key[2] + dz];
                    for &i in self.cells.get(&k).into_iter().flatten() {
                        let d = len(sub(self.at[i], p));
                        if d <= JOIN_M && best.is_none_or(|(b, _)| d < b) {
                            best = Some((d, i));
                        }
                    }
                }
            }
        }
        if let Some((_, i)) = best {
            return i;
        }
        self.at.push(p);
        let i = self.at.len() - 1;
        self.cells.entry(key).or_default().push(i);
        i
    }
}

fn lex(a: P3, b: P3) -> std::cmp::Ordering {
    a[0].total_cmp(&b[0])
        .then(a[1].total_cmp(&b[1]))
        .then(a[2].total_cmp(&b[2]))
}

/// The mesh `positions` (xyz per vertex) and `indices` (three per triangle) cut by the plane through
/// `origin` with `normal`: joined polylines (cut points within 1 mm are one point), closed loops
/// repeating their first point at the end, slivers under 5 cm dropped, simplified to 1 cm. Triangles
/// with an index out of range, a non-finite corner or no area are ignored. The result does not
/// depend on the triangle order. Empty when the plane misses the mesh or `normal` is zero.
pub fn slice_plane(
    positions: &[f32],
    indices: &[u32],
    origin: [f64; 3],
    normal: [f64; 3],
) -> Vec<Vec<[f64; 3]>> {
    let Some(n) = unit(normal) else {
        return Vec::new();
    };
    let nv = positions.len() / 3;
    let vert = |i: u32| -> Option<P3> {
        let i = i as usize;
        (i < nv).then(|| {
            [
                f64::from(positions[i * 3]),
                f64::from(positions[i * 3 + 1]),
                f64::from(positions[i * 3 + 2]),
            ]
        })
    };
    let mut nodes = Nodes {
        cells: HashMap::new(),
        at: Vec::new(),
    };
    let mut edges: Vec<(usize, usize)> = Vec::new();
    let mut seen: HashMap<(usize, usize), ()> = HashMap::new();
    for t in indices.as_chunks::<3>().0 {
        let Some(c) = t.iter().map(|&i| vert(i)).collect::<Option<Vec<P3>>>() else {
            continue;
        };
        if c.iter().flatten().any(|x| !x.is_finite())
            || len(cross(sub(c[1], c[0]), sub(c[2], c[0]))) < 1e-12
        {
            continue;
        }
        // a corner on the plane counts as above it, so every crossing is on exactly two edges
        let d: Vec<f64> = c.iter().map(|p| dot(sub(*p, origin), n)).collect();
        let up: Vec<bool> = d.iter().map(|&x| x >= 0.0).collect();
        if up[0] == up[1] && up[1] == up[2] {
            continue;
        }
        let mut cut: Vec<P3> = Vec::with_capacity(2);
        for (a, b) in [(0, 1), (1, 2), (2, 0)] {
            if up[a] == up[b] {
                continue;
            }
            // the same edge gives the same point from either triangle: always from its lower index
            let (a, b) = if t[a] <= t[b] { (a, b) } else { (b, a) };
            let s = d[a] / (d[a] - d[b]);
            let e = sub(c[b], c[a]);
            cut.push([c[a][0] + s * e[0], c[a][1] + s * e[1], c[a][2] + s * e[2]]);
        }
        let (i, j) = (nodes.id(cut[0]), nodes.id(cut[1]));
        if i == j {
            continue;
        }
        let k = (i.min(j), i.max(j));
        if seen.insert(k, ()).is_none() {
            edges.push(k);
        }
    }
    let lines = join(&nodes.at, &edges);
    lines
        .into_iter()
        .filter(|l| l.windows(2).map(|w| len(sub(w[1], w[0]))).sum::<f64>() >= MIN_LENGTH_M)
        .map(|l| simplify(&l, SIMPLIFY_M))
        .collect()
}

/// Walks the segment graph into polylines: open runs first (from their ends, then from branch
/// points), then the loops left. Starts and directions are chosen by position so the walk does not
/// depend on the order the segments were found; the lines come back sorted by their first point.
fn join(at: &[P3], edges: &[(usize, usize)]) -> Vec<Vec<P3>> {
    let mut adj: Vec<Vec<(usize, usize)>> = vec![Vec::new(); at.len()];
    for (e, &(a, b)) in edges.iter().enumerate() {
        adj[a].push((b, e));
        adj[b].push((a, e));
    }
    for v in &mut adj {
        v.sort_by(|x, y| lex(at[x.0], at[y.0]));
    }
    let mut used = vec![false; edges.len()];
    let mut order: Vec<usize> = (0..at.len()).filter(|&i| !adj[i].is_empty()).collect();
    order.sort_by(|&a, &b| lex(at[a], at[b]));
    let mut out: Vec<Vec<P3>> = Vec::new();
    let walk = |start: usize, used: &mut Vec<bool>| -> Vec<usize> {
        let mut path = vec![start];
        let mut here = start;
        while let Some(&(next, e)) = adj[here].iter().find(|(_, e)| !used[*e]) {
            used[e] = true;
            path.push(next);
            here = next;
            if adj[here].len() != 2 {
                break;
            }
        }
        path
    };
    // ends and branches
    for &s in &order {
        if adj[s].len() == 2 {
            continue;
        }
        while adj[s].iter().any(|(_, e)| !used[*e]) {
            let mut p = walk(s, &mut used);
            if lex(at[*p.last().expect("walked")], at[p[0]]).is_lt() {
                p.reverse();
            }
            out.push(p.iter().map(|&i| at[i]).collect());
        }
    }
    // loops: from their lowest point, towards its lower neighbour
    for &s in &order {
        if adj[s].iter().any(|(_, e)| !used[*e]) {
            let p = walk(s, &mut used);
            out.push(p.iter().map(|&i| at[i]).collect());
        }
    }
    out.sort_by(|a, b| lex(a[0], b[0]).then(a.len().cmp(&b.len())));
    out
}

fn seg_dist(p: P3, a: P3, b: P3) -> f64 {
    let ab = sub(b, a);
    let l2 = dot(ab, ab);
    if l2 <= 0.0 {
        return len(sub(p, a));
    }
    let t = (dot(sub(p, a), ab) / l2).clamp(0.0, 1.0);
    len(sub(
        p,
        [a[0] + t * ab[0], a[1] + t * ab[1], a[2] + t * ab[2]],
    ))
}

/// Douglas-Peucker: keeps the ends (a closed polyline keeps its repeated first point) and every
/// point further than `tol` from the simplified line.
fn simplify(pts: &[P3], tol: f64) -> Vec<P3> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let closed = pts.len() > 3 && pts[0] == pts[pts.len() - 1];
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut stack = vec![(0, pts.len() - 1)];
    if closed {
        // a loop has no ends: split at the point furthest from the start first
        let far = (1..pts.len() - 1)
            .max_by(|&a, &b| len(sub(pts[a], pts[0])).total_cmp(&len(sub(pts[b], pts[0]))))
            .expect("a loop has inner points");
        keep[far] = true;
        stack = vec![(0, far), (far, pts.len() - 1)];
    }
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let (mut worst, mut at) = (0.0, a);
        for i in a + 1..b {
            let d = seg_dist(pts[i], pts[a], pts[b]);
            if d > worst {
                worst = d;
                at = i;
            }
        }
        if worst > tol {
            keep[at] = true;
            stack.push((a, at));
            stack.push((at, b));
        }
    }
    pts.iter()
        .zip(&keep)
        .filter(|(_, k)| **k)
        .map(|(p, _)| *p)
        .collect()
}

/// Plan walls: the mesh cut by the level plane at `height` (metres, in the positions' frame), as
/// `[east, north]` polylines (see [`slice_plane`]).
pub fn plan_walls(positions: &[f32], indices: &[u32], height: f64) -> Vec<Vec<[f64; 2]>> {
    slice_plane(positions, indices, [0.0, 0.0, height], [0.0, 0.0, 1.0])
        .into_iter()
        .map(|l| l.into_iter().map(|p| [p[0], p[1]]).collect())
        .collect()
}

/// A cross section: the mesh cut by the plane through the origin (the station) spanned by `right`
/// and `up` (a section frame, east-north-up), as `[across, up]` polylines: each point's distance
/// along `right` and along `up` from the origin. Empty when the two directions are parallel.
pub fn section(
    positions: &[f32],
    indices: &[u32],
    right: [f64; 3],
    up: [f64; 3],
) -> Vec<Vec<[f64; 2]>> {
    let (Some(r), Some(u)) = (unit(right), unit(up)) else {
        return Vec::new();
    };
    slice_plane(positions, indices, [0.0; 3], cross(r, u))
        .into_iter()
        .map(|l| l.into_iter().map(|p| [dot(p, r), dot(p, u)]).collect())
        .collect()
}

/// Profile walls (the floor and ceiling along the passage): the mesh cut by the vertical plane
/// through the origin (the station) that contains the horizontal direction `along` (its vertical
/// part is ignored), as `[s, z]` polylines: distance along `along` from the origin, and height.
/// Empty when `along` is vertical or zero.
pub fn profile_walls(positions: &[f32], indices: &[u32], along: [f64; 3]) -> Vec<Vec<[f64; 2]>> {
    let Some(a) = unit([along[0], along[1], 0.0]) else {
        return Vec::new();
    };
    slice_plane(positions, indices, [0.0; 3], cross(a, [0.0, 0.0, 1.0]))
        .into_iter()
        .map(|l| l.into_iter().map(|p| [dot(p, a), p[2]]).collect())
        .collect()
}

/// A shot file's header, mesh positions and triangle indices, nothing else: no normals or photos are
/// copied (a sibling of [`crate::bundle::mesh_positions`] for slicing). Checks the file as
/// [`check_header`] does, that every position is finite and every index names a vertex.
pub fn mesh_triangles(bytes: &[u8]) -> Result<(HeaderInfo, Vec<f32>, Vec<u32>), BundleError> {
    let info = check_header(bytes)?;
    // MAGIC, u16 format, u32 header length, header, positions, normals, indices, photos
    let head = MAGIC.len() + 2;
    let len = bytes
        .get(head..head + 4)
        .map(|b| u32::from_le_bytes(b.try_into().expect("4 bytes")) as usize)
        .ok_or(BundleError::Truncated)?;
    let n = info.vertices * 3;
    let at = head + 4 + len;
    let words = |from: usize, count: usize| -> Result<&[u8], BundleError> {
        bytes
            .get(from..from + count * 4)
            .ok_or(BundleError::Truncated)
    };
    let positions: Vec<f32> = words(at, n)?
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect();
    let indices: Vec<u32> = words(at + n * 8, info.triangles * 3)?
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect();
    if !positions.iter().all(|v| v.is_finite())
        || !indices.iter().all(|&i| (i as usize) < info.vertices)
    {
        return Err(BundleError::BadMesh);
    }
    Ok((info, positions, indices))
}

/// Whether any vertex of `positions` is within `reach` metres of `at`.
pub fn comes_within(positions: &[f32], at: [f64; 3], reach: f64) -> bool {
    positions.as_chunks::<3>().0.iter().any(|c| {
        let d = sub([f64::from(c[0]), f64::from(c[1]), f64::from(c[2])], at);
        dot(d, d) <= reach * reach
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A closed box, corners `lo` to `hi`, two triangles per face.
    fn box_mesh(lo: P3, hi: P3) -> (Vec<f32>, Vec<u32>) {
        let mut pos = Vec::new();
        for i in 0..8 {
            let p = [
                if i & 1 == 0 { lo[0] } else { hi[0] },
                if i & 2 == 0 { lo[1] } else { hi[1] },
                if i & 4 == 0 { lo[2] } else { hi[2] },
            ];
            pos.extend(p.map(|x| x as f32));
        }
        let quads = [
            [0, 1, 3, 2],
            [4, 6, 7, 5],
            [0, 4, 5, 1],
            [2, 3, 7, 6],
            [0, 2, 6, 4],
            [1, 5, 7, 3],
        ];
        let mut idx = Vec::new();
        for q in quads {
            idx.extend([q[0], q[1], q[2], q[0], q[2], q[3]]);
        }
        (pos, idx)
    }

    /// An open-ended cylinder about the z axis.
    fn cylinder(r: f64, z0: f64, z1: f64, n: u32) -> (Vec<f32>, Vec<u32>) {
        let mut pos = Vec::new();
        for i in 0..n {
            let a = f64::from(i) / f64::from(n) * std::f64::consts::TAU;
            for z in [z0, z1] {
                pos.extend([(r * a.cos()) as f32, (r * a.sin()) as f32, z as f32]);
            }
        }
        let mut idx = Vec::new();
        for i in 0..n {
            let (a, b) = (2 * i, 2 * ((i + 1) % n));
            idx.extend([a, b, a + 1, b, b + 1, a + 1]);
        }
        (pos, idx)
    }

    fn bbox2(l: &[[f64; 2]]) -> [f64; 4] {
        let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for p in l {
            b = [
                b[0].min(p[0]),
                b[1].min(p[1]),
                b[2].max(p[0]),
                b[3].max(p[1]),
            ];
        }
        b
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn box_sliced_level_is_one_closed_rectangle() {
        let (pos, idx) = box_mesh([-2.0, -1.0, -1.0], [2.0, 1.0, 2.0]);
        let w = plan_walls(&pos, &idx, 0.5);
        assert_eq!(w.len(), 1, "{w:?}");
        let l = &w[0];
        assert_eq!(l.first(), l.last(), "closed");
        assert_eq!(l.len(), 5, "four corners and the repeat: {l:?}");
        let b = bbox2(l);
        assert!(
            near(b[0], -2.0) && near(b[1], -1.0) && near(b[2], 2.0) && near(b[3], 1.0),
            "{b:?}"
        );
        // the 3-D slice sits on the plane
        for p in &slice_plane(&pos, &idx, [0.0, 0.0, 0.5], [0.0, 0.0, 1.0])[0] {
            assert!(near(p[2], 0.5));
        }
    }

    #[test]
    fn cylinder_slices_to_a_circle() {
        let (pos, idx) = cylinder(1.5, -1.0, 2.0, 64);
        let w = plan_walls(&pos, &idx, 0.3);
        assert_eq!(w.len(), 1);
        let l = &w[0];
        assert_eq!(l.first(), l.last());
        assert!(l.len() > 16);
        for p in l {
            let r = (p[0] * p[0] + p[1] * p[1]).sqrt();
            assert!((r - 1.5).abs() < 0.01, "{r}");
        }
    }

    #[test]
    fn plane_missing_the_mesh_gives_nothing() {
        let (pos, idx) = box_mesh([-2.0, -1.0, -1.0], [2.0, 1.0, 2.0]);
        assert!(plan_walls(&pos, &idx, 5.0).is_empty());
        assert!(plan_walls(&pos, &idx, -1.5).is_empty());
        assert!(slice_plane(&pos, &idx, [0.0; 3], [0.0; 3]).is_empty());
        assert!(plan_walls(&[], &[], 0.0).is_empty());
    }

    #[test]
    fn section_of_a_box_is_its_cross_rectangle() {
        // a passage running north: across is east, up is up
        let (pos, idx) = box_mesh([-1.5, -4.0, -0.5], [1.0, 4.0, 2.5]);
        let s = section(&pos, &idx, [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].first(), s[0].last());
        let b = bbox2(&s[0]);
        assert!(
            near(b[0], -1.5) && near(b[1], -0.5) && near(b[2], 1.0) && near(b[3], 2.5),
            "{b:?}"
        );
        // a tilted frame: right turned 90 degrees, so across is minus north
        let (pos, idx) = box_mesh([-4.0, -1.0, -0.5], [4.0, 2.0, 2.5]);
        let s = section(&pos, &idx, [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]);
        let b = bbox2(&s[0]);
        assert!(near(b[0], -2.0) && near(b[2], 1.0), "{b:?}");
        assert!(section(&pos, &idx, [0.0, 0.0, 1.0], [0.0, 0.0, 2.0]).is_empty());
    }

    #[test]
    fn profile_of_an_east_west_box_gives_floor_and_ceiling() {
        // a room 6 m long east-west, floor 0.8 m below the station, ceiling 1.7 m above
        let (pos, idx) = box_mesh([-2.0, -1.0, -0.8], [4.0, 1.0, 1.7]);
        let p = profile_walls(&pos, &idx, [1.0, 0.0, 0.3]);
        assert_eq!(p.len(), 1, "closed room: one loop");
        let b = bbox2(&p[0]);
        assert!(near(b[0], -2.0) && near(b[2], 4.0), "length {b:?}");
        assert!(
            near(b[1], -0.8) && near(b[3], 1.7),
            "floor and ceiling {b:?}"
        );
        let floor: Vec<_> = p[0].iter().filter(|q| near(q[1], -0.8)).collect();
        let ceiling: Vec<_> = p[0].iter().filter(|q| near(q[1], 1.7)).collect();
        assert!(floor.len() >= 2 && ceiling.len() >= 2);
        // looking west the run is reversed
        let w = profile_walls(&pos, &idx, [-1.0, 0.0, 0.0]);
        let b = bbox2(&w[0]);
        assert!(near(b[0], -4.0) && near(b[2], 2.0), "{b:?}");
        assert!(profile_walls(&pos, &idx, [0.0, 0.0, 1.0]).is_empty());
    }

    #[test]
    fn open_scan_stays_open() {
        // a box without its east and west ends: two walls, not a loop
        let (pos, idx) = box_mesh([-2.0, -1.0, -1.0], [2.0, 1.0, 2.0]);
        let idx: Vec<u32> = idx
            .as_chunks::<3>()
            .0
            .iter()
            .enumerate()
            .filter(|(i, _)| *i < 8)
            .flat_map(|(_, t)| *t)
            .collect();
        let w = plan_walls(&pos, &idx, 0.5);
        assert!(!w.is_empty());
        assert!(w.iter().all(|l| l.first() != l.last()), "{w:?}");
    }

    #[test]
    fn degenerate_and_bad_triangles_are_ignored() {
        let (mut pos, mut idx) = box_mesh([-2.0, -1.0, -1.0], [2.0, 1.0, 2.0]);
        let clean = plan_walls(&pos, &idx, 0.5);
        // a sliver with no area across the plane, one out of range, one with a NaN corner
        pos.extend([0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.5]);
        pos.extend([f32::NAN, 0.0, 0.0]);
        idx.extend([8, 9, 10, 0, 1, 999, 8, 9, 11, 0, 0, 0]);
        assert_eq!(plan_walls(&pos, &idx, 0.5), clean);
        // a tiny fragment (a 2 cm triangle) is dropped
        let tiny = [0.0f32, 0.0, -0.01, 0.02, 0.0, 0.01, 0.0, 0.02, 0.01];
        assert!(plan_walls(&tiny, &[0, 1, 2], 0.0).is_empty());
    }

    #[test]
    fn triangle_order_does_not_matter() {
        let (pos, idx) = cylinder(1.2, -1.0, 2.0, 40);
        let a = plan_walls(&pos, &idx, 0.7);
        let mut tris: Vec<[u32; 3]> = idx.as_chunks::<3>().0.to_vec();
        tris.reverse();
        tris.rotate_left(17);
        for t in &mut tris {
            t.rotate_left(1);
        }
        let shuffled: Vec<u32> = tris.into_iter().flatten().collect();
        let b = plan_walls(&pos, &shuffled, 0.7);
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x.len(), y.len());
            for (p, q) in x.iter().zip(y) {
                assert!(near(p[0], q[0]) && near(p[1], q[1]), "{p:?} {q:?}");
            }
        }
    }

    #[test]
    fn mesh_triangles_reads_the_mesh_only() {
        use crate::bundle::{Shot, ShotHeader, encode};
        let header: ShotHeader = serde_json::from_str(
            r#"{"shot_id":"s1","cave_id":"c","station_id":"a","station_label":"A1","captured_at_ms":1,"device":"t",
            "instrument_height_m":0.3,"start_camera":[0,0,0],"taps":[],"tracking":"normal","arkit_version":"26","gravity":[0,-1,0]}"#,
        )
        .unwrap();
        let (positions, indices) = box_mesh([-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]);
        let shot = Shot {
            header,
            normals: vec![0.5; positions.len()],
            positions: positions.clone(),
            indices: indices.clone(),
            keyframes: vec![],
        };
        let bytes = encode(&shot).unwrap();
        let (info, p, i) = mesh_triangles(&bytes).unwrap();
        assert_eq!((info.vertices, info.triangles), (8, 12));
        assert_eq!((p, i), (positions, indices));
        assert!(mesh_triangles(&bytes[..bytes.len() - 1]).is_err());
        assert!(mesh_triangles(b"nope").is_err());
    }

    #[test]
    fn comes_within_checks_reach() {
        let (pos, _) = box_mesh([-2.0, -1.0, -1.0], [2.0, 1.0, 2.0]);
        assert!(comes_within(&pos, [4.0, 0.0, 0.0], 2.5));
        assert!(!comes_within(&pos, [10.0, 0.0, 0.0], 3.0));
    }
}
