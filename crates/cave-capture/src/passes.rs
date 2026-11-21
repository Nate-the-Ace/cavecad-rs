//! Several scanning passes in one shot: the pass record, adding a pass, the ghost of a scan, and
//! lining a pass up from the station cards. Pure: bytes and numbers in, data out.

use crate::bundle::{BundleError, CardSeen, Keyframe, KeyframeRef, Shot, ShotRef};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

/// One scanning pass of a shot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pass {
    /// When the pass was captured (milliseconds since 1970, from the phone).
    pub captured_at_ms: i64,
    /// How it was lined up with the first pass: `first`, `same_session` or `cards`.
    pub aligned_by: String,
    /// Column-major 4x4 that moved this pass's data into the shot's frame (identity unless `cards`).
    pub transform: [f64; 16],
    /// Index of this pass's first keyframe in the shot.
    pub first_keyframe: u32,
    /// How many keyframes this pass added.
    pub keyframe_count: u32,
    /// The cards as this pass placed them, in the shot's frame.
    #[serde(default)]
    pub cards: Vec<CardSeen>,
    /// Why/how the pass ran: `gimbal`, `tracking lost`, `gimbal lost`, `low battery`. Empty in old files.
    #[serde(default)]
    pub flags: Vec<String>,
}

/// What a new pass does to the shot's mesh.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassMesh {
    /// The pass's mesh is the whole room as ARKit now holds it (same session): it replaces the old.
    Replace,
    /// The pass's mesh is new geometry only (tracking was lost): it is added to the old.
    Add,
}

/// A pass to add to a shot, in its own capture frame.
#[derive(Clone, Debug, PartialEq)]
pub struct NewPass {
    /// When the pass was captured (milliseconds since 1970).
    pub captured_at_ms: i64,
    /// `same_session` or `cards`.
    pub aligned_by: String,
    /// Column-major 4x4 from this pass's frame to the shot's frame.
    pub transform: [f64; 16],
    /// Replace or add the mesh.
    pub mesh: PassMesh,
    /// Vertex positions, xyz per vertex.
    pub positions: Vec<f32>,
    /// Vertex normals, xyz per vertex.
    pub normals: Vec<f32>,
    /// Triangle indices, three per triangle.
    pub indices: Vec<u32>,
    /// The pass's kept photos.
    pub keyframes: Vec<Keyframe>,
    /// The cards the pass placed.
    pub cards: Vec<CardSeen>,
    /// How tracking went in this pass: `normal`, `limited` or `lost`.
    pub tracking: String,
    /// Flags for the pass: `gimbal`, `tracking lost`, `gimbal lost`, `low battery`.
    pub flags: Vec<String>,
}

/// [`NewPass`] with its arrays and photos borrowed (the phone's buffers), for [`add_pass_ref`].
#[derive(Clone, Debug, PartialEq)]
pub struct NewPassRef<'a> {
    /// When the pass was captured (milliseconds since 1970).
    pub captured_at_ms: i64,
    /// `same_session` or `cards`.
    pub aligned_by: String,
    /// Column-major 4x4 from this pass's frame to the shot's frame.
    pub transform: [f64; 16],
    /// Replace or add the mesh.
    pub mesh: PassMesh,
    /// Vertex positions, xyz per vertex.
    pub positions: &'a [f32],
    /// Vertex normals, xyz per vertex.
    pub normals: &'a [f32],
    /// Triangle indices, three per triangle.
    pub indices: &'a [u32],
    /// The pass's kept photos, borrowed.
    pub keyframes: Vec<KeyframeRef<'a>>,
    /// The cards the pass placed.
    pub cards: Vec<CardSeen>,
    /// How tracking went in this pass: `normal`, `limited` or `lost`.
    pub tracking: String,
    /// Flags for the pass.
    pub flags: Vec<String>,
}

/// The 4x4 identity, column-major.
pub const IDENTITY: [f64; 16] = [
    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
];

/// A point moved by a column-major 4x4.
pub fn apply_point(m: &[f64; 16], p: [f64; 3]) -> [f64; 3] {
    [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
    ]
}

/// A direction turned by a column-major 4x4 (no shift).
pub fn apply_dir(m: &[f64; 16], v: [f64; 3]) -> [f64; 3] {
    [
        m[0] * v[0] + m[4] * v[1] + m[8] * v[2],
        m[1] * v[0] + m[5] * v[1] + m[9] * v[2],
        m[2] * v[0] + m[6] * v[1] + m[10] * v[2],
    ]
}

/// a * b, column-major 4x4.
pub fn mul(a: &[f64; 16], b: &[f64; 16]) -> [f64; 16] {
    let mut o = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            o[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    o
}

fn move_card(m: &[f64; 16], c: &CardSeen) -> CardSeen {
    CardSeen {
        point: apply_point(m, c.point),
        corners: c.corners.iter().map(|p| apply_point(m, *p)).collect(),
        ..c.clone()
    }
}

fn worst(a: &str, b: &str) -> String {
    let rank = |t: &str| match t {
        "lost" => 2,
        "limited" => 1,
        _ => 0,
    };
    if rank(b) > rank(a) {
        b.into()
    } else {
        a.into()
    }
}

/// Sightings from which a card estimate counts as well established.
const SOLID_SIGHTINGS: u32 = 8;

/// Whether `new` is a better estimate of a card than `old`: a well-seen one that wandered no more,
/// or any one with more sightings while `old` is still thinly seen.
fn better(new: &CardSeen, old: &CardSeen) -> bool {
    (new.sightings >= SOLID_SIGHTINGS && new.spread_m <= old.spread_m)
        || (old.sightings < SOLID_SIGHTINGS && new.sightings > old.sightings)
}

/// The shot with `p` added: photos appended, mesh replaced or added, cards merged (a card the new
/// pass placed replaces the same id only if it is the better estimate; the rest are kept or added), and the pass recorded. A shot with no pass list gets its
/// first pass recorded too. `BadMesh` for ragged new arrays or an index past the new vertices.
pub fn add_pass(old: &Shot, p: NewPass) -> Result<Shot, BundleError> {
    let NewPass {
        captured_at_ms,
        aligned_by,
        transform,
        mesh,
        positions,
        normals,
        indices,
        keyframes,
        cards,
        tracking,
        flags,
    } = p;
    let r = add_pass_ref(
        old.borrowed(),
        NewPassRef {
            captured_at_ms,
            aligned_by,
            transform,
            mesh,
            positions: &positions,
            normals: &normals,
            indices: &indices,
            keyframes: keyframes.iter().map(KeyframeRef::from).collect(),
            cards,
            tracking,
            flags,
        },
    )?;
    Ok(r.into_shot())
}

/// [`add_pass`] without copying a photo: `old` is taken whole (its mesh and header are moved, not
/// cloned) and the result's photos borrow from `old`'s bytes and the new pass's. Write it with
/// [`crate::bundle::write_parts`].
pub fn add_pass_ref<'a>(old: ShotRef<'a>, p: NewPassRef<'a>) -> Result<ShotRef<'a>, BundleError> {
    if !p.positions.len().is_multiple_of(3)
        || p.normals.len() != p.positions.len()
        || !p.indices.len().is_multiple_of(3)
    {
        return Err(BundleError::BadMesh);
    }
    if !p.transform.iter().all(|v| v.is_finite()) {
        return Err(BundleError::BadMesh);
    }
    let n_new = (p.positions.len() / 3) as u32;
    if p.indices.iter().any(|&i| i >= n_new) {
        return Err(BundleError::BadMesh);
    }
    let m = p.transform;
    let mut positions = Vec::with_capacity(p.positions.len());
    let mut normals = Vec::with_capacity(p.normals.len());
    for v in p.positions.as_chunks::<3>().0 {
        let q = apply_point(&m, [v[0] as f64, v[1] as f64, v[2] as f64]);
        positions.extend(q.iter().map(|x| *x as f32));
    }
    for v in p.normals.as_chunks::<3>().0 {
        let q = apply_dir(&m, [v[0] as f64, v[1] as f64, v[2] as f64]);
        normals.extend(q.iter().map(|x| *x as f32));
    }
    let mut out = old;
    if out.header.passes.is_empty() {
        out.header.passes.push(Pass {
            captured_at_ms: out.header.captured_at_ms,
            aligned_by: "first".into(),
            transform: IDENTITY,
            first_keyframe: 0,
            keyframe_count: out.keyframes.len() as u32,
            cards: out.header.cards.clone(),
            flags: out.header.flags.clone(),
        });
    }
    match p.mesh {
        PassMesh::Replace => {
            out.positions = positions;
            out.normals = normals;
            out.indices = p.indices.to_vec();
        }
        PassMesh::Add => {
            let base = (out.positions.len() / 3) as u32;
            out.positions.extend(positions);
            out.normals.extend(normals);
            out.indices.extend(p.indices.iter().map(|i| i + base));
        }
    }
    let first_keyframe = out.keyframes.len() as u32;
    let keyframe_count = p.keyframes.len() as u32;
    out.keyframes.reserve_exact(p.keyframes.len());
    for k in p.keyframes {
        let mut pose64 = [0.0; 16];
        for (d, s) in pose64.iter_mut().zip(k.pose.iter()) {
            *d = *s as f64;
        }
        let moved = mul(&m, &pose64);
        let mut pose = [0f32; 16];
        for (d, s) in pose.iter_mut().zip(moved.iter()) {
            *d = *s as f32;
        }
        out.keyframes.push(KeyframeRef { pose, ..k });
    }
    let mut cards: Vec<CardSeen> = Vec::new();
    for c in p.cards.iter().rev() {
        if !cards.iter().any(|n| n.id == c.id) {
            cards.push(move_card(&m, c));
        }
    }
    cards.reverse();
    let mut merged: Vec<CardSeen> = std::mem::take(&mut out.header.cards);
    for n in &cards {
        match merged.iter_mut().find(|o| o.id == n.id) {
            Some(o) => {
                if better(n, o) {
                    *o = n.clone();
                }
            }
            None => merged.push(n.clone()),
        }
    }
    merged.sort_by_key(|c| c.id);
    out.header.cards = merged;
    out.header.tracking = worst(&out.header.tracking, &p.tracking);
    out.header.passes.push(Pass {
        captured_at_ms: p.captured_at_ms,
        aligned_by: p.aligned_by,
        transform: m,
        first_keyframe,
        keyframe_count,
        cards,
        flags: p.flags,
    });
    Ok(out)
}

/// A scan made light for drawing over a live view: a simplified mesh and its hole outlines.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Ghost {
    /// Vertex positions, xyz per vertex.
    pub positions: Vec<f32>,
    /// Triangle indices, three per triangle.
    pub indices: Vec<u32>,
    /// Open edges of the simplified mesh joined into polylines (a closed loop repeats its first
    /// point at the end); each at least `min_outline_m` long.
    pub outlines: Vec<Vec<[f32; 3]>>,
}

/// Simplifies a mesh by vertex clustering on a `cell_m` grid (each cell's vertices become their
/// mean; triangles that collapse or repeat are dropped), then outlines its holes: edges used by one
/// triangle only, chained, dropping chains shorter than `min_outline_m`. Clustering also welds the
/// seams between ARKit's mesh anchors, so only real gaps and the scan's edge are outlined.
/// Deterministic: outlines come out in the same order every run.
pub fn ghost(positions: &[f32], indices: &[u32], cell_m: f32, min_outline_m: f32) -> Ghost {
    let n = positions.len() / 3;
    if cell_m <= 0.0 || n == 0 {
        return Ghost::default();
    }
    // Lookup only; ids are assigned in vertex order so the map's order never reaches the output.
    let mut cell_of: HashMap<(i32, i32, i32), u32> = HashMap::new();
    let mut sums: Vec<([f64; 3], u32)> = Vec::new();
    let mut map = vec![u32::MAX; n];
    for (v, p) in positions.as_chunks::<3>().0.iter().enumerate() {
        if !p.iter().all(|x| x.is_finite()) {
            continue; // left unclustered; triangles touching it are skipped
        }
        let key = (
            (p[0] / cell_m).floor() as i32,
            (p[1] / cell_m).floor() as i32,
            (p[2] / cell_m).floor() as i32,
        );
        let id = *cell_of.entry(key).or_insert_with(|| {
            sums.push(([0.0; 3], 0));
            (sums.len() - 1) as u32
        });
        let s = &mut sums[id as usize];
        for (acc, x) in s.0.iter_mut().zip(p) {
            *acc += *x as f64;
        }
        s.1 += 1;
        map[v] = id;
    }
    let mut out = Ghost::default();
    for (s, c) in &sums {
        out.positions
            .extend(s.iter().map(|x| (*x / *c as f64) as f32));
    }
    let mut seen = HashSet::new();
    for t in indices.as_chunks::<3>().0 {
        if t.iter().any(|&i| i as usize >= n) {
            continue;
        }
        let (a, b, c) = (map[t[0] as usize], map[t[1] as usize], map[t[2] as usize]);
        if a == u32::MAX || b == u32::MAX || c == u32::MAX {
            continue;
        }
        if a == b || b == c || a == c {
            continue;
        }
        let mut key = [a, b, c];
        key.sort_unstable();
        if !seen.insert(key) {
            continue;
        }
        out.indices.extend([a, b, c]);
    }
    // Open edges: used by exactly one triangle.
    let mut uses: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for t in out.indices.as_chunks::<3>().0 {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            *uses.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }
    let mut next: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (&(a, b), &u) in &uses {
        if u == 1 {
            next.entry(a).or_default().push(b);
            next.entry(b).or_default().push(a);
        }
    }
    out.outlines = chain(next, &out.positions, min_outline_m);
    out
}

/// Chains open edges (vertex to neighbours) into polylines of at least `min_len` length. Open
/// chains are walked first, from odd-degree vertices in id order, so each comes out whole; the
/// remaining closed loops follow, from the lowest id. A closed loop repeats its first point.
fn chain(mut next: BTreeMap<u32, Vec<u32>>, positions: &[f32], min_len: f32) -> Vec<Vec<[f32; 3]>> {
    let at = |v: u32| {
        let i = v as usize * 3;
        [positions[i], positions[i + 1], positions[i + 2]]
    };
    let dist = |a: [f32; 3], b: [f32; 3]| {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    };
    let keys: Vec<u32> = next.keys().copied().collect();
    let odd: Vec<u32> = keys
        .iter()
        .copied()
        .filter(|k| next[k].len() % 2 == 1)
        .collect();
    let mut outlines = Vec::new();
    for &start in odd.iter().chain(&keys) {
        while !next[&start].is_empty() {
            let mut line = vec![at(start)];
            let mut len = 0.0f32;
            let mut cur = start;
            while let Some(nb) = next.get_mut(&cur).and_then(|v| v.pop()) {
                if let Some(back) = next.get_mut(&nb)
                    && let Some(k) = back.iter().position(|&x| x == cur)
                {
                    back.swap_remove(k);
                }
                let p = at(nb);
                len += dist(*line.last().unwrap(), p);
                line.push(p);
                cur = nb;
                if cur == start {
                    break;
                }
            }
            if len >= min_len {
                outlines.push(line);
            }
        }
    }
    outlines
}

/// How a pass lines up from its cards.
#[derive(Clone, Debug, PartialEq)]
pub struct CardFit {
    /// Turn about +y (up), radians.
    pub yaw_rad: f64,
    /// Column-major 4x4 from the pass's frame to the shot's (turn, then shift).
    pub transform: [f64; 16],
    /// Each matched card's distance from where the shot has it after the fit: (id, metres).
    pub misfits: Vec<(u32, f64)>,
    /// The largest misfit, metres.
    pub worst_m: f64,
    /// The misfit each matched card is allowed: (id, metres) = max(0.05, 0.015 x its largest
    /// horizontal distance to another matched card in the shot).
    pub limits: Vec<(u32, f64)>,
    /// Every misfit is within its limit.
    pub ok: bool,
    /// The largest misfit / limit; above 1 means a card is off by more than allowed.
    pub worst_ratio: f64,
}

fn matched<'a>(old: &'a [CardSeen], new: &'a [CardSeen]) -> Vec<(&'a CardSeen, &'a CardSeen)> {
    new.iter()
        .filter_map(|n| old.iter().find(|o| o.id == n.id).map(|o| (o, n)))
        .collect()
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Largest horizontal (x/z) separation between any two of the points.
fn horizontal_spread(pts: &[[f64; 3]]) -> f64 {
    let mut best = 0.0_f64;
    for (i, a) in pts.iter().enumerate() {
        for b in &pts[i + 1..] {
            best = best.max((a[0] - b[0]).hypot(a[2] - b[2]));
        }
    }
    best
}

/// Least-squares turn about +y and shift taking the new pass's cards onto the shot's (gravity is
/// shared, so 4 unknowns). Needs 2 matched cards; a third shows a moved card as a misfit.
/// `None` if fewer than 2 match, if the matched cards are all within 0.2 m of each other
/// horizontally (the turn is undetermined), or if any number is, or comes out, non-finite.
pub fn cards_fit(old: &[CardSeen], new: &[CardSeen]) -> Option<CardFit> {
    let m = matched(old, new);
    if m.len() < 2 {
        return None;
    }
    if m.iter()
        .any(|(o, n)| o.point.iter().chain(n.point.iter()).any(|v| !v.is_finite()))
    {
        return None;
    }
    let op: Vec<[f64; 3]> = m.iter().map(|(o, _)| o.point).collect();
    let np: Vec<[f64; 3]> = m.iter().map(|(_, n)| n.point).collect();
    if horizontal_spread(&op) < 0.2 || horizontal_spread(&np) < 0.2 {
        return None;
    }
    let k = m.len() as f64;
    let mut co = [0.0; 3];
    let mut cn = [0.0; 3];
    for (o, n) in &m {
        for i in 0..3 {
            co[i] += o.point[i] / k;
            cn[i] += n.point[i] / k;
        }
    }
    // R_y(t) u = (c ux + s uz, uy, -s ux + c uz); maximise sum v . R u
    let (mut sx, mut sc) = (0.0, 0.0);
    for (o, n) in &m {
        let (ux, uz) = (n.point[0] - cn[0], n.point[2] - cn[2]);
        let (vx, vz) = (o.point[0] - co[0], o.point[2] - co[2]);
        sc += vx * ux + vz * uz;
        sx += vx * uz - vz * ux;
    }
    let yaw = sx.atan2(sc);
    let (s, c) = yaw.sin_cos();
    let rot = [
        c, 0.0, -s, 0.0, 0.0, 1.0, 0.0, 0.0, s, 0.0, c, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let rc = apply_dir(&rot, cn);
    let mut transform = rot;
    transform[12] = co[0] - rc[0];
    transform[13] = co[1] - rc[1];
    transform[14] = co[2] - rc[2];
    let misfits: Vec<(u32, f64)> = m
        .iter()
        .map(|(o, n)| (n.id, dist(apply_point(&transform, n.point), o.point)))
        .collect();
    let worst_m = misfits.iter().map(|x| x.1).fold(0.0, f64::max);
    if !yaw.is_finite() || !worst_m.is_finite() || transform.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let limits: Vec<(u32, f64)> = m
        .iter()
        .map(|(o, n)| {
            let far = op
                .iter()
                .map(|q| (o.point[0] - q[0]).hypot(o.point[2] - q[2]))
                .fold(0.0, f64::max);
            (n.id, (0.015 * far).max(0.05))
        })
        .collect();
    let ok = misfits.iter().zip(&limits).all(|(a, l)| a.1 <= l.1);
    let worst_ratio = misfits
        .iter()
        .zip(&limits)
        .map(|(a, l)| a.1 / l.1)
        .fold(0.0, f64::max);
    Some(CardFit {
        yaw_rad: yaw,
        transform,
        misfits,
        worst_m,
        limits,
        ok,
        worst_ratio,
    })
}

/// Each card seen in both, with how far apart they are as they stand (no fit): the same-session
/// check that nothing moved.
pub fn card_offsets(old: &[CardSeen], new: &[CardSeen]) -> Vec<(u32, f64)> {
    matched(old, new)
        .iter()
        .map(|(o, n)| (n.id, dist(n.point, o.point)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_chain_is_one_outline_from_its_end() {
        // a=1, b=0 (lowest id), c=2: chain a-b-c must not split at b.
        let pos = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0, 0.0, 0.0];
        let mut next = BTreeMap::new();
        next.insert(0, vec![1, 2]);
        next.insert(1, vec![0]);
        next.insert(2, vec![0]);
        let o = chain(next, &pos, 0.0);
        assert_eq!(o.len(), 1, "{o:?}");
        assert_eq!(o[0].len(), 3);
        let ends = [o[0][0][0], o[0][2][0]];
        assert!(ends == [1.0, 2.0] || ends == [2.0, 1.0], "{ends:?}");
    }
}
