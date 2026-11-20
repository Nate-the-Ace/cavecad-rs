//! Several scanning passes in one shot: the pass record, adding a pass, the ghost of a scan, and
//! lining a pass up from the station cards. Pure: bytes and numbers in, data out.

use crate::bundle::{BundleError, CardSeen, Keyframe, Shot};
use serde::{Deserialize, Serialize};

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
    let cards: Vec<CardSeen> = p.cards.iter().map(|c| move_card(&m, c)).collect();
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
