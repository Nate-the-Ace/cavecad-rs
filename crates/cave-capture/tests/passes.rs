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

use cave_capture::{apply_point, card_offsets, cards_fit};

#[test]
fn cards_fit_recovers_a_known_turn_and_shift() {
    let old = vec![
        card(2, [0.3, -1.0, -1.0]),
        card(12, [1.5, -0.8, -4.0]),
        card(26, [-0.4, -1.1, 2.0]),
    ];
    let t = yaw_shift(0.7, [1.0, -0.2, 3.0]);
    // the new pass sees the same cards in its own frame: new = inverse(t) * old
    let inv = yaw_shift(-0.7, [0.0; 3]);
    let back = |p: [f64; 3]| apply_point(&inv, [p[0] - 1.0, p[1] + 0.2, p[2] - 3.0]);
    let new: Vec<_> = old
        .iter()
        .take(2)
        .map(|c| card(c.id, back(c.point)))
        .collect();
    let f = cards_fit(&old, &new).unwrap();
    assert!((f.yaw_rad - 0.7).abs() < 1e-9, "{}", f.yaw_rad);
    assert!(f.worst_m < 1e-9);
    for (a, b) in f.transform.iter().zip(t.iter()) {
        assert!((a - b).abs() < 1e-9);
    }
    assert_eq!(f.misfits.len(), 2);
}

#[test]
fn cards_fit_names_a_moved_card() {
    let old = vec![
        card(2, [0.0, -1.0, -1.0]),
        card(12, [0.0, -1.0, -4.0]),
        card(26, [0.0, -1.0, 2.0]),
    ];
    let mut new = old.clone();
    // along the line of cards: a sideways nudge of an end card would be soaked up by the turn
    new[2].point[2] += 0.12;
    let f = cards_fit(&old, &new).unwrap();
    assert!(f.worst_m > 0.05);
    let (id, m) = f
        .misfits
        .iter()
        .copied()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap();
    assert_eq!(id, 26);
    assert!(m > 0.05);
    assert!(cards_fit(&old, &new[..1]).is_none());
}

#[test]
fn card_offsets_are_plain_distances() {
    let old = vec![card(2, [0.0, 0.0, 0.0]), card(12, [1.0, 0.0, 0.0])];
    let new = vec![card(2, [0.07, 0.0, 0.0]), card(26, [5.0, 0.0, 0.0])];
    let o = card_offsets(&old, &new);
    assert_eq!(o.len(), 1);
    assert_eq!(o[0].0, 2);
    assert!((o[0].1 - 0.07).abs() < 1e-12);
}

#[test]
fn cards_fit_refuses_cards_stacked_on_one_spot() {
    // 0.1 m apart horizontally (one above the other is no help either): yaw is undetermined
    let old = vec![card(2, [0.0, -1.0, -1.0]), card(12, [0.1, -0.2, -1.0])];
    let new = vec![card(2, [3.0, -1.0, 0.0]), card(12, [3.1, -0.2, 0.0])];
    assert!(cards_fit(&old, &new).is_none());
    let tall = vec![card(2, [0.0, -1.0, -1.0]), card(12, [0.0, 1.0, -1.0])];
    assert!(cards_fit(&tall, &tall).is_none());
}

#[test]
fn cards_fit_refuses_non_finite_numbers() {
    let old = vec![card(2, [0.0, -1.0, -1.0]), card(12, [0.0, -1.0, -4.0])];
    for bad in [f64::NAN, f64::INFINITY] {
        let mut new = old.clone();
        new[1].point[2] = bad;
        assert!(cards_fit(&old, &new).is_none());
        assert!(cards_fit(&new, &old).is_none());
    }
    // a pair so large the arithmetic overflows must not produce a NaN transform either
    let huge = vec![
        card(2, [1e200, 0.0, 1e200]),
        card(12, [-1e200, 0.0, -1e200]),
    ];
    assert!(cards_fit(&huge, &huge).is_none_or(|f| f.transform.iter().all(|v| v.is_finite())));
}

fn seen(id: u32, x: f64, sightings: u32, spread_m: f64) -> CardSeen {
    CardSeen {
        sightings,
        spread_m,
        ..card(id, [x, 0.0, 0.0])
    }
}

/// The shot's card 2 as `old`, one new card 2 in the same frame; returns the stored card 2.
fn merged_card(old: CardSeen, new: CardSeen) -> CardSeen {
    let mut s = shot();
    s.header.cards = vec![old];
    let mut np = new_pass("same_session", identity(), PassMesh::Replace);
    np.cards = vec![new];
    let out = add_pass(&s, np).unwrap();
    assert_eq!(out.header.cards.len(), 1);
    // the pass record keeps the pass's own card either way
    assert_eq!(out.header.passes[1].cards.len(), 1);
    out.header.cards[0].clone()
}

#[test]
fn worse_new_card_does_not_replace_the_old() {
    // fewer sightings, and a wider spread at the same count
    assert_eq!(
        merged_card(seen(2, 1.0, 4, 0.008), seen(2, 2.0, 1, 0.0)).point[0],
        1.0
    );
    assert_eq!(
        merged_card(seen(2, 1.0, 9, 0.005), seen(2, 2.0, 9, 0.019)).point[0],
        1.0
    );
    assert_eq!(
        merged_card(seen(2, 1.0, 9, 0.005), seen(2, 2.0, 4, 0.001)).point[0],
        1.0
    );
}

#[test]
fn better_new_card_replaces_the_old() {
    assert_eq!(
        merged_card(seen(2, 1.0, 4, 0.008), seen(2, 2.0, 5, 0.02)).point[0],
        2.0
    );
    assert_eq!(
        merged_card(seen(2, 1.0, 9, 0.010), seen(2, 2.0, 12, 0.004)).point[0],
        2.0
    );
    assert_eq!(
        merged_card(seen(2, 1.0, 9, 0.010), seen(2, 2.0, 9, 0.010)).point[0],
        2.0
    );
}

#[test]
fn new_card_ids_are_added_and_old_only_kept() {
    let mut np = new_pass("same_session", identity(), PassMesh::Replace);
    np.cards = vec![card(26, [1.0, 0.0, 0.0])];
    let out = add_pass(&shot(), np).unwrap();
    let ids: Vec<u32> = out.header.cards.iter().map(|c| c.id).collect();
    assert_eq!(ids, vec![2, 12, 26]);
}

#[test]
fn far_card_gets_a_wider_limit() {
    // two cards 4.8 m apart, the line-up stretched 12 cm: 6 cm misfit each, limit 0.072
    let old = vec![card(2, [0.0, 0.0, 0.0]), card(12, [0.0, 0.0, -4.8])];
    let mut new = old.clone();
    new[1].point[2] -= 0.12;
    let f = cards_fit(&old, &new).unwrap();
    assert!(f.worst_m > 0.05);
    assert!(f.ok, "{f:?}");
    assert!(
        f.worst_ratio < 1.0 && f.worst_ratio > 0.7,
        "{}",
        f.worst_ratio
    );
    assert!(f.limits.iter().all(|(_, l)| (l - 0.072).abs() < 1e-9));
}

#[test]
fn near_card_keeps_the_floor_limit() {
    let old = vec![card(2, [0.0, 0.0, 0.0]), card(12, [0.0, 0.0, -1.0])];
    let mut new = old.clone();
    new[1].point[2] -= 0.12;
    let f = cards_fit(&old, &new).unwrap();
    assert!(!f.ok, "{f:?}");
    assert!(f.worst_ratio > 1.0);
    assert!(f.limits.iter().all(|(_, l)| (l - 0.05).abs() < 1e-9));
}
