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
    assert_eq!(decode(&encode(&s)), Ok(s));
}

#[test]
fn starts_with_magic() {
    assert!(encode(&sample()).starts_with(MAGIC));
}

#[test]
fn wrong_magic() {
    let mut b = encode(&sample());
    b[0] = b'X';
    assert_eq!(decode(&b), Err(BundleError::NotACapture));
}

#[test]
fn newer_version() {
    let mut b = encode(&sample());
    b[MAGIC.len()] = 99;
    assert_eq!(decode(&b), Err(BundleError::NewerFormat(99)));
}

#[test]
fn truncated() {
    let b = encode(&sample());
    assert_eq!(decode(&b[..b.len() - 1]), Err(BundleError::Truncated));
}

#[test]
fn index_out_of_range() {
    let mut s = sample();
    s.indices = vec![0, 1, 3];
    assert_eq!(decode(&encode(&s)), Err(BundleError::BadMesh));
}

#[test]
fn non_finite_vertex() {
    let mut s = sample();
    s.positions[4] = f32::NAN;
    assert_eq!(decode(&encode(&s)), Err(BundleError::BadMesh));
}
