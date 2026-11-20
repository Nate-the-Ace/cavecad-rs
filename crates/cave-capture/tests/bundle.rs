//! Round-trip and refusal tests for the capture bundle.

use cave_capture::bundle::{
    BundleError, CardSeen, HeadingTap, Keyframe, MAGIC, Shot, ShotHeader, decode, encode,
};

fn sample() -> Shot {
    Shot {
        header: ShotHeader {
            shot_id: "s1".into(),
            cave_id: "cave12345".into(),
            station_id: "st-a".into(),
            station_label: "A3".into(),
            captured_at_ms: 1_763_400_000_000,
            device: "iPhone18,2".into(),
            instrument_height_m: 0.0,
            start_camera: [0.0, 0.0, 0.0],
            taps: vec![HeadingTap {
                target_station_id: "st-b".into(),
                target_label: "A4".into(),
                origin: [0.0, 0.0, 0.0],
                dir: [0.0, 0.0, -1.0],
            }],
            tracking: "normal".into(),
            arkit_version: "26.1".into(),
            gravity: [0.0, -1.0, 0.0],
            compass: None,
            cards: vec![],
        },
        positions: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
        indices: vec![0, 1, 2],
        keyframes: vec![
            Keyframe {
                pose: [0.0; 16],
                intrinsics: [0.0; 9],
                width: 4,
                height: 3,
                jpeg: vec![0xff, 0xd8, 1, 2],
            },
            Keyframe {
                pose: [1.0; 16],
                intrinsics: [1.0; 9],
                width: 4,
                height: 3,
                jpeg: vec![0xff, 0xd8, 3],
            },
        ],
    }
}

#[test]
fn round_trip() {
    let s = sample();
    assert_eq!(decode(&encode(&s).unwrap()), Ok(s));
}

#[test]
fn starts_with_magic() {
    assert!(encode(&sample()).unwrap().starts_with(MAGIC));
}

#[test]
fn wrong_magic() {
    let mut b = encode(&sample()).unwrap();
    b[0] = b'X';
    assert_eq!(decode(&b), Err(BundleError::NotACapture));
}

#[test]
fn newer_version() {
    let mut b = encode(&sample()).unwrap();
    b[MAGIC.len()] = 99;
    assert_eq!(decode(&b), Err(BundleError::NewerFormat(99)));
}

#[test]
fn truncated() {
    let b = encode(&sample()).unwrap();
    assert_eq!(decode(&b[..b.len() - 1]), Err(BundleError::Truncated));
}

/// Where the mesh starts in a bundle (after magic, format, header length and header).
fn mesh_start(b: &[u8]) -> usize {
    let at = MAGIC.len() + 2;
    at + 4 + u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize
}

// encode refuses these, so the bad bytes are patched into a good file: decode must refuse them too.
#[test]
fn index_out_of_range() {
    let mut b = encode(&sample()).unwrap();
    let third_index = mesh_start(&b) + 9 * 4 * 2 + 2 * 4;
    b[third_index..third_index + 4].copy_from_slice(&3u32.to_le_bytes());
    assert_eq!(decode(&b), Err(BundleError::BadMesh));
}

#[test]
fn non_finite_vertex() {
    let mut b = encode(&sample()).unwrap();
    let fifth_position = mesh_start(&b) + 4 * 4;
    b[fifth_position..fifth_position + 4].copy_from_slice(&f32::NAN.to_le_bytes());
    assert_eq!(decode(&b), Err(BundleError::BadMesh));
}

#[test]
fn lying_header_counts_do_not_panic() {
    // Rewrite the header JSON so it claims u32::MAX vertices.
    let b = encode(&sample()).unwrap();
    let at = MAGIC.len() + 2;
    let len = u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize;
    let json = std::str::from_utf8(&b[at + 4..at + 4 + len]).unwrap();
    let lie = json
        .replace("\"vertex_count\":3", "\"vertex_count\":4294967295")
        .replace("\"normal_count\":3", "\"normal_count\":4294967295");
    let mut out = b[..at].to_vec();
    out.extend_from_slice(&(lie.len() as u32).to_le_bytes());
    out.extend_from_slice(lie.as_bytes());
    out.extend_from_slice(&b[at + 4 + len..]);
    assert!(decode(&out).is_err());
}

#[test]
fn ragged_input_refused_on_encode() {
    let mut s = sample();
    s.positions.push(1.0);
    assert_eq!(encode(&s), Err(BundleError::BadMesh));
    let mut s = sample();
    s.normals.pop();
    s.normals.pop();
    s.normals.pop();
    assert_eq!(encode(&s), Err(BundleError::BadMesh));
    let mut s = sample();
    s.indices.push(0);
    assert_eq!(encode(&s), Err(BundleError::BadMesh));
}

#[test]
fn normal_count_mismatch_refused() {
    let b = encode(&sample()).unwrap();
    let at = MAGIC.len() + 2;
    let len = u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize;
    let json = std::str::from_utf8(&b[at + 4..at + 4 + len]).unwrap();
    assert!(json.contains("\"normal_count\":3"));
    let lie = json.replace("\"normal_count\":3", "\"normal_count\":2");
    let mut out = b[..at].to_vec();
    out.extend_from_slice(&(lie.len() as u32).to_le_bytes());
    out.extend_from_slice(lie.as_bytes());
    out.extend_from_slice(&b[at + 4 + len..]);
    assert_eq!(decode(&out), Err(BundleError::BadMesh));
}

#[test]
fn encode_refuses_what_decode_would() {
    let mut s = sample();
    s.indices = vec![0, 1, 3];
    assert_eq!(encode(&s), Err(BundleError::BadMesh));
    let mut s = sample();
    s.normals[4] = f32::INFINITY;
    assert_eq!(encode(&s), Err(BundleError::BadMesh));
}

#[test]
fn encode_never_reallocates() {
    let b = encode(&sample()).unwrap();
    assert_eq!(b.capacity(), b.len());
}

#[test]
fn encode_parts_matches_encode() {
    use cave_capture::bundle::{KeyframeRef, encode_parts};
    let s = sample();
    let refs: Vec<KeyframeRef> = s
        .keyframes
        .iter()
        .map(|k| KeyframeRef {
            pose: k.pose,
            intrinsics: k.intrinsics,
            width: k.width,
            height: k.height,
            jpeg: &k.jpeg,
        })
        .collect();
    assert_eq!(
        encode_parts(&s.header, &s.positions, &s.normals, &s.indices, &refs),
        encode(&s)
    );
}

#[test]
fn check_header_reads_header_and_counts_only() {
    use cave_capture::bundle::check_header;
    let s = sample();
    let b = encode(&s).unwrap();
    let h = check_header(&b).unwrap();
    assert_eq!(h.header, s.header);
    assert_eq!((h.vertices, h.triangles, h.keyframes), (3, 1, 2));
    assert_eq!(check_header(&b[..b.len() - 1]), Err(BundleError::Truncated));
    let mut longer = b.clone();
    longer.push(0);
    assert_eq!(check_header(&longer), Err(BundleError::Truncated));
    assert_eq!(check_header(b"nope"), Err(BundleError::NotACapture));
}

#[test]
fn compass_round_trips() {
    use cave_capture::bundle::Compass;
    let mut s = sample();
    s.header.taps.clear();
    s.header.compass = Some(Compass {
        true_north: true,
        accuracy_deg: 7.5,
    });
    assert_eq!(decode(&encode(&s).unwrap()), Ok(s));
}

/// The two shots already on the phone were written before `compass` existed (and a compass shot
/// writes no taps): both must still read.
#[test]
fn header_without_compass_or_taps_still_reads() {
    let h: ShotHeader = serde_json::from_str(
        r#"{"shot_id":"s","cave_id":"c","station_id":"a","station_label":"A","captured_at_ms":0,
            "device":"t","instrument_height_m":1.0,"start_camera":[0,0,0],"tracking":"normal",
            "arkit_version":"26.1","gravity":[0,-1,0]}"#,
    )
    .unwrap();
    assert!(h.taps.is_empty());
    assert_eq!(h.compass, None);
}

/// A card seen during the shot round-trips, corners included.
#[test]
fn cards_round_trip() {
    let mut s = sample();
    s.header.cards = vec![
        CardSeen {
            id: 2,
            word: "CURRENT".into(),
            point: [0.5, -0.3, -2.0],
            corners: vec![
                [0.4, -0.2, -2.0],
                [0.6, -0.2, -2.0],
                [0.6, -0.4, -2.0],
                [0.4, -0.4, -2.0],
            ],
            sightings: 14,
            spread_m: 0.004,
        },
        CardSeen {
            id: 12,
            word: "FORWARD".into(),
            point: [3.0, -0.3, -1.0],
            corners: vec![],
            sightings: 3,
            spread_m: 0.02,
        },
    ];
    assert_eq!(decode(&encode(&s).unwrap()), Ok(s));
}

/// Shots written before `cards` existed must still read, with none.
#[test]
fn header_without_cards_still_reads() {
    let h: ShotHeader = serde_json::from_str(
        r#"{"shot_id":"s","cave_id":"c","station_id":"a","station_label":"A","captured_at_ms":0,
            "device":"t","instrument_height_m":1.0,"start_camera":[0,0,0],"tracking":"normal",
            "arkit_version":"26.1","gravity":[0,-1,0],"compass":{"true_north":true,"accuracy_deg":5.0}}"#,
    )
    .unwrap();
    assert!(h.cards.is_empty());
}
