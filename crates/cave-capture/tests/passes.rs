//! Passes in a shot: the pass record and `add_pass`.

use cave_capture::{
    CardSeen, Keyframe, NewPass, PassMesh, Shot, ShotHeader, add_pass, decode, encode,
};

fn card(id: u32, p: [f64; 3]) -> CardSeen {
    CardSeen {
        id,
        word: format!("#{id}"),
        point: p,
        corners: vec![],
        sightings: 9,
        spread_m: 0.005,
    }
}

fn header() -> ShotHeader {
    ShotHeader {
        shot_id: "s".into(),
        cave_id: "c".into(),
        station_id: "a".into(),
        station_label: "A".into(),
        captured_at_ms: 1000,
        device: "t".into(),
        instrument_height_m: 0.3,
        start_camera: [0.0; 3],
        taps: vec![],
        tracking: "normal".into(),
        arkit_version: "27".into(),
        gravity: [0.0, -1.0, 0.0],
        compass: None,
        cards: vec![card(2, [0.0, -1.0, -1.0]), card(12, [0.0, -1.0, -4.0])],
        passes: vec![],
    }
}

fn kf(tag: u8) -> Keyframe {
    let mut pose = [0f32; 16];
    pose[0] = 1.0;
    pose[5] = 1.0;
    pose[10] = 1.0;
    pose[15] = 1.0;
    Keyframe {
        pose,
        intrinsics: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        width: 2,
        height: 2,
        jpeg: vec![tag; 5],
    }
}

fn shot() -> Shot {
    Shot {
        header: header(),
        positions: vec![0.0, 0.0, -1.0, 1.0, 0.0, -1.0, 0.0, 1.0, -1.0],
        normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
        indices: vec![0, 1, 2],
        keyframes: vec![kf(1), kf(2)],
    }
}

/// Column-major: turn `yaw` about +y, then shift.
fn yaw_shift(yaw: f64, t: [f64; 3]) -> [f64; 16] {
    let (s, c) = yaw.sin_cos();
    [
        c, 0.0, -s, 0.0, 0.0, 1.0, 0.0, 0.0, s, 0.0, c, 0.0, t[0], t[1], t[2], 1.0,
    ]
}

fn identity() -> [f64; 16] {
    yaw_shift(0.0, [0.0; 3])
}

fn new_pass(aligned_by: &str, transform: [f64; 16], mesh: PassMesh) -> NewPass {
    NewPass {
        captured_at_ms: 2000,
        aligned_by: aligned_by.into(),
        transform,
        mesh,
        positions: vec![0.0, 0.0, -2.0, 1.0, 0.0, -2.0, 0.0, 1.0, -2.0],
        normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
        indices: vec![0, 1, 2],
        keyframes: vec![kf(9)],
        cards: vec![card(12, [0.0, -1.0, -4.1]), card(26, [0.0, -1.0, 2.0])],
        tracking: "limited".into(),
    }
}

#[test]
fn old_header_without_passes_still_reads() {
    let bytes = encode(&shot()).unwrap();
    assert!(decode(&bytes).unwrap().header.passes.is_empty());
}

#[test]
fn same_session_pass_replaces_the_mesh_and_appends_photos() {
    let out = add_pass(
        &shot(),
        new_pass("same_session", identity(), PassMesh::Replace),
    )
    .unwrap();
    assert_eq!(out.indices, vec![0, 1, 2]);
    assert_eq!(out.positions[2], -2.0);
    assert_eq!(out.keyframes.len(), 3);
    assert_eq!(out.keyframes[0].jpeg, vec![1; 5]);
    assert_eq!(out.keyframes[2].jpeg, vec![9; 5]);
    let p = &out.header.passes;
    assert_eq!(p.len(), 2);
    assert_eq!(
        (
            p[0].aligned_by.as_str(),
            p[0].first_keyframe,
            p[0].keyframe_count
        ),
        ("first", 0, 2)
    );
    assert_eq!(
        (
            p[1].aligned_by.as_str(),
            p[1].first_keyframe,
            p[1].keyframe_count
        ),
        ("same_session", 2, 1)
    );
    assert_eq!(p[0].cards.len(), 2);
    assert_eq!(out.header.tracking, "limited");
    // cards: 12 replaced by the new pass, 2 kept, 26 added; in id order
    let ids: Vec<u32> = out.header.cards.iter().map(|c| c.id).collect();
    assert_eq!(ids, vec![2, 12, 26]);
    assert!((out.header.cards[1].point[2] + 4.1).abs() < 1e-12);
    // and it round-trips through the file format
    assert_eq!(decode(&encode(&out).unwrap()).unwrap(), out);
}

#[test]
fn card_pass_is_moved_and_added() {
    let t = yaw_shift(std::f64::consts::FRAC_PI_2, [1.0, 0.0, 0.0]);
    let out = add_pass(&shot(), new_pass("cards", t, PassMesh::Add)).unwrap();
    assert_eq!(out.indices, vec![0, 1, 2, 3, 4, 5]);
    // new vertex (0,0,-2) turned 90 deg about +y -> (-2,0,0), shifted -> (-1,0,0)
    assert!((out.positions[9] + 1.0).abs() < 1e-6 && out.positions[11].abs() < 1e-6);
    // its normal (0,0,1) turns to (1,0,0)
    assert!((out.normals[9] - 1.0).abs() < 1e-6);
    // the new photo's pose is moved too: translation column becomes (1,0,0)
    assert!((out.keyframes[2].pose[12] - 1.0).abs() < 1e-6);
    // the new pass's card 26 at (0,-1,2) -> (2,-1,0) + (1,0,0) = (3,-1,0)
    let c26 = out.header.cards.iter().find(|c| c.id == 26).unwrap();
    assert!((c26.point[0] - 3.0).abs() < 1e-9 && c26.point[2].abs() < 1e-9);
    assert_eq!(out.header.passes[1].transform, t);
}

#[test]
fn third_pass_extends_the_list() {
    let two = add_pass(
        &shot(),
        new_pass("same_session", identity(), PassMesh::Replace),
    )
    .unwrap();
    let three = add_pass(
        &two,
        new_pass("same_session", identity(), PassMesh::Replace),
    )
    .unwrap();
    assert_eq!(three.header.passes.len(), 3);
    assert_eq!(three.header.passes[2].first_keyframe, 3);
}

#[test]
fn nan_transform_is_refused() {
    let mut t = identity();
    t[12] = f64::NAN;
    let r = add_pass(&shot(), new_pass("cards", t, PassMesh::Add));
    assert!(matches!(r, Err(cave_capture::BundleError::BadMesh)));
}

#[test]
fn encode_refuses_nan_pose_and_nan_card() {
    let mut s = shot();
    s.keyframes[0].pose[3] = f32::NAN;
    assert!(matches!(
        encode(&s),
        Err(cave_capture::BundleError::BadMesh)
    ));
    let mut s = shot();
    s.header.cards[0].point[1] = f64::NAN;
    assert!(matches!(
        encode(&s),
        Err(cave_capture::BundleError::BadMesh)
    ));
}

#[test]
fn duplicate_new_card_ids_keep_the_last() {
    let mut np = new_pass("same_session", identity(), PassMesh::Replace);
    np.cards = vec![card(26, [1.0, 0.0, 0.0]), card(26, [2.0, 0.0, 0.0])];
    let out = add_pass(&shot(), np).unwrap();
    let c26: Vec<_> = out.header.cards.iter().filter(|c| c.id == 26).collect();
    assert_eq!(c26.len(), 1);
    assert_eq!(c26[0].point[0], 2.0);
    assert_eq!(out.header.passes[1].cards.len(), 1);
}

use cave_capture::ghost;

/// A flat sheet in x/z at y = 0, `n` x `n` cells of `step` m, skipping cells whose centre falls in
/// the square hole `[h0, h1)` on both axes. Starts at `x0`.
fn sheet(n: usize, step: f32, x0: f32, hole: Option<(f32, f32)>) -> (Vec<f32>, Vec<u32>) {
    let mut pos = Vec::new();
    for j in 0..=n {
        for i in 0..=n {
            pos.extend([x0 + i as f32 * step, 0.0, j as f32 * step]);
        }
    }
    let mut idx = Vec::new();
    let w = (n + 1) as u32;
    for j in 0..n {
        for i in 0..n {
            let (cx, cz) = (x0 + (i as f32 + 0.5) * step, (j as f32 + 0.5) * step);
            if let Some((a, b)) = hole
                && cx >= a
                && cx < b
                && cz >= a
                && cz < b
            {
                continue;
            }
            let v = j as u32 * w + i as u32;
            idx.extend([v, v + w, v + 1, v + 1, v + w, v + w + 1]);
        }
    }
    (pos, idx)
}

fn length(o: &[[f32; 3]]) -> f32 {
    o.windows(2)
        .map(|w| {
            ((w[1][0] - w[0][0]).powi(2)
                + (w[1][1] - w[0][1]).powi(2)
                + (w[1][2] - w[0][2]).powi(2))
            .sqrt()
        })
        .sum()
}

#[test]
fn ghost_outlines_the_hole_and_the_edge() {
    let (p, i) = sheet(100, 0.01, 0.0, Some((0.35, 0.65)));
    let g = ghost(&p, &i, 0.05, 0.15);
    assert!(
        g.indices.len() / 3 <= 2 * 21 * 21,
        "{} triangles",
        g.indices.len() / 3
    );
    let mut lens: Vec<f32> = g.outlines.iter().map(|o| length(o)).collect();
    lens.sort_by(f32::total_cmp);
    println!("outline lengths: {lens:?}");
    assert_eq!(lens.len(), 2, "{lens:?}");
    assert!(
        (lens[0] - 1.2).abs() < 0.2 && (lens[1] - 4.0).abs() < 0.15,
        "{lens:?}"
    );
}

#[test]
fn ghost_welds_anchor_seams() {
    let (mut p, mut i) = sheet(50, 0.01, 0.0, None);
    let (p2, i2) = sheet(50, 0.01, 0.5, None);
    let base = (p.len() / 3) as u32;
    p.extend(p2);
    i.extend(i2.iter().map(|v| v + base));
    let g = ghost(&p, &i, 0.05, 0.15);
    assert_eq!(g.outlines.len(), 1);
    assert!((length(&g.outlines[0]) - 3.0).abs() < 0.15);
}

#[test]
fn closed_box_has_no_outline() {
    let p: Vec<f32> = [
        [0., 0., 0.],
        [1., 0., 0.],
        [1., 1., 0.],
        [0., 1., 0.],
        [0., 0., 1.],
        [1., 0., 1.],
        [1., 1., 1.],
        [0., 1., 1.],
    ]
    .iter()
    .flatten()
    .copied()
    .collect();
    let i = vec![
        0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0, 7, 3,
        1, 2, 6, 1, 6, 5,
    ];
    assert!(ghost(&p, &i, 0.05, 0.15).outlines.is_empty());
}

#[test]
fn ghost_is_deterministic() {
    let (p, i) = sheet(100, 0.01, 0.0, Some((0.35, 0.65)));
    assert_eq!(ghost(&p, &i, 0.05, 0.15), ghost(&p, &i, 0.05, 0.15));
}

#[test]
fn ghost_ignores_non_finite_vertices() {
    let (mut p, i) = sheet(20, 0.05, 0.0, None);
    p[3 * 50 + 1] = f32::NAN;
    p[3 * 60] = f32::INFINITY;
    let g = ghost(&p, &i, 0.1, 0.1);
    assert!(!g.indices.is_empty());
    assert!(g.positions.iter().all(|x| x.is_finite()));
    assert!(g.outlines.iter().flatten().flatten().all(|x| x.is_finite()));
}
