//! Several scanning passes in one shot: the pass record, adding a pass, the ghost of a scan, and
//! lining a pass up from the station cards. Pure: bytes and numbers in, data out.

use crate::bundle::{BundleError, CardSeen, Keyframe, Shot};
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

/// The shot with `p` added: photos appended, mesh replaced or added, cards merged (a card the new
/// pass placed replaces the same id), and the pass recorded. A shot with no pass list gets its
/// first pass recorded too. `BadMesh` for ragged new arrays or an index past the new vertices.
pub fn add_pass(old: &Shot, p: NewPass) -> Result<Shot, BundleError> {
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
    let mut out = old.clone();
    if out.header.passes.is_empty() {
        out.header.passes.push(Pass {
            captured_at_ms: old.header.captured_at_ms,
            aligned_by: "first".into(),
            transform: IDENTITY,
            first_keyframe: 0,
            keyframe_count: old.keyframes.len() as u32,
            cards: old.header.cards.clone(),
        });
    }
    match p.mesh {
        PassMesh::Replace => {
            out.positions = positions;
            out.normals = normals;
            out.indices = p.indices.clone();
        }
        PassMesh::Add => {
            let base = (out.positions.len() / 3) as u32;
            out.positions.extend(positions);
            out.normals.extend(normals);
            out.indices.extend(p.indices.iter().map(|i| i + base));
        }
    }
    let first_keyframe = out.keyframes.len() as u32;
    for k in &p.keyframes {
        let mut pose64 = [0.0; 16];
        for (d, s) in pose64.iter_mut().zip(k.pose.iter()) {
            *d = *s as f64;
        }
        let moved = mul(&m, &pose64);
        let mut pose = [0f32; 16];
        for (d, s) in pose.iter_mut().zip(moved.iter()) {
            *d = *s as f32;
        }
        out.keyframes.push(Keyframe { pose, ..k.clone() });
    }
    let mut cards: Vec<CardSeen> = Vec::new();
    for c in p.cards.iter().rev() {
        if !cards.iter().any(|n| n.id == c.id) {
            cards.push(move_card(&m, c));
        }
    }
    cards.reverse();
    let mut merged: Vec<CardSeen> = out
        .header
        .cards
        .iter()
        .filter(|c| !cards.iter().any(|n| n.id == c.id))
        .cloned()
        .collect();
    merged.extend(cards.iter().cloned());
    merged.sort_by_key(|c| c.id);
    out.header.cards = merged;
    out.header.tracking = worst(&out.header.tracking, &p.tracking);
    out.header.passes.push(Pass {
        captured_at_ms: p.captured_at_ms,
        aligned_by: p.aligned_by,
        transform: m,
        first_keyframe,
        keyframe_count: p.keyframes.len() as u32,
        cards,
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
    let mut map = vec![0u32; n];
    for (v, p) in positions.as_chunks::<3>().0.iter().enumerate() {
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
    let at = |v: u32| {
        let i = v as usize * 3;
        [out.positions[i], out.positions[i + 1], out.positions[i + 2]]
    };
    let dist = |a: [f32; 3], b: [f32; 3]| {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    };
    let mut outlines = Vec::new();
    while let Some((&start, _)) = next.iter().find(|(_, v)| !v.is_empty()) {
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
        if len >= min_outline_m {
            outlines.push(line);
        }
    }
    out.outlines = outlines;
    out
}
