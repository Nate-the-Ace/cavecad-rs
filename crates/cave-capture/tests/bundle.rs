//! Round-trip and refusal tests for the capture bundle.

use cave_capture::bundle::{
    BundleError, HeadingTap, Keyframe, MAGIC, Shot, ShotHeader, decode, encode,
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

#[test]
fn index_out_of_range() {
    let mut s = sample();
    s.indices = vec![0, 1, 3];
    assert_eq!(decode(&encode(&s).unwrap()), Err(BundleError::BadMesh));
}

#[test]
fn non_finite_vertex() {
    let mut s = sample();
    s.positions[4] = f32::NAN;
    assert_eq!(decode(&encode(&s).unwrap()), Err(BundleError::BadMesh));
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
