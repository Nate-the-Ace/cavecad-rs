//! Placement math tests.

use cave_capture::frame::{Placement, bearing_deg, capture_to_survey, heading_from_tap};
use cave_survey::Vec3;

fn close(a: Vec3, b: Vec3) -> bool {
    (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9 && (a.z - b.z).abs() < 1e-9
}

#[test]
fn axes_map() {
    assert!(close(
        capture_to_survey([1.0, 2.0, 3.0]),
        Vec3 {
            x: 1.0,
            y: -3.0,
            z: 2.0
        }
    ));
}

#[test]
fn bearings() {
    assert!((bearing_deg(0.0, 1.0) - 0.0).abs() < 1e-9);
    assert!((bearing_deg(1.0, 0.0) - 90.0).abs() < 1e-9);
    assert!((bearing_deg(0.0, -1.0) - 180.0).abs() < 1e-9);
    assert!((bearing_deg(-1.0, 0.0) - 270.0).abs() < 1e-9);
}

#[test]
fn tap_east_places_forward_east() {
    let station = Vec3 {
        x: 10.0,
        y: 20.0,
        z: 5.0,
    };
    let target = Vec3 {
        x: 30.0,
        y: 20.0,
        z: 5.0,
    }; // due east
    let p = heading_from_tap(station, target, [0.0, 0.0, -1.0], [0.0, 0.0, 0.0], 0.0).unwrap();
    let q = p.apply([0.0, 0.0, -5.0]);
    assert!(
        close(
            q,
            Vec3 {
                x: 15.0,
                y: 20.0,
                z: 5.0
            }
        ),
        "{q:?}"
    );
}

#[test]
fn instrument_height_lifts() {
    let s = Vec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    let t = Vec3 {
        x: 0.0,
        y: 10.0,
        z: 0.0,
    };
    let p = heading_from_tap(s, t, [0.0, 0.0, -1.0], [0.0, 0.0, 0.0], 1.5).unwrap();
    assert!(close(
        p.apply([0.0, 0.0, 0.0]),
        Vec3 {
            x: 0.0,
            y: 0.0,
            z: 1.5
        }
    ));
}

#[test]
fn start_camera_offset_removed() {
    let s = Vec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    let t = Vec3 {
        x: 0.0,
        y: 10.0,
        z: 0.0,
    };
    let p = heading_from_tap(s, t, [0.0, 0.0, -1.0], [0.2, 0.0, 0.0], 0.0).unwrap();
    assert!(close(
        p.apply([0.2, 0.0, 0.0]),
        Vec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0
        }
    ));
}

#[test]
fn vertical_tap_refused() {
    let s = Vec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    let t = Vec3 {
        x: 0.0,
        y: 10.0,
        z: 0.0,
    };
    assert_eq!(
        heading_from_tap(s, t, [0.0, -1.0, 0.01], [0.0; 3], 0.0),
        None::<Placement>
    );
}

#[test]
fn target_straight_above_refused() {
    let s = Vec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    let t = Vec3 {
        x: 0.001,
        y: 0.0,
        z: 8.0,
    };
    assert_eq!(
        heading_from_tap(s, t, [0.0, 0.0, -1.0], [0.0; 3], 0.0),
        None::<Placement>
    );
}

#[test]
fn non_finite_input_refused() {
    let ok = Vec3 {
        x: 0.0,
        y: 10.0,
        z: 0.0,
    };
    let nan = Vec3 {
        x: f64::NAN,
        y: 0.0,
        z: 0.0,
    };
    let o = Vec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    let d = [0.0, 0.0, -1.0];
    assert_eq!(heading_from_tap(nan, ok, d, [0.0; 3], 0.0), None);
    assert_eq!(heading_from_tap(o, nan, d, [0.0; 3], 0.0), None);
    assert_eq!(
        heading_from_tap(o, ok, [f64::NAN, 0.0, -1.0], [0.0; 3], 0.0),
        None
    );
    assert_eq!(
        heading_from_tap(o, ok, d, [0.0, f64::INFINITY, 0.0], 0.0),
        None
    );
    assert_eq!(heading_from_tap(o, ok, d, [0.0; 3], f64::NAN), None);
}

#[test]
fn compass_shot_lands_north_of_the_station() {
    use cave_capture::frame::{PIVOT_HEIGHT_M, from_compass};
    let st = cave_survey::Vec3 {
        x: 100.0,
        y: 200.0,
        z: 50.0,
    };
    let p = from_compass(st, [1.0, 0.5, 2.0], 0.0).unwrap();
    assert_eq!(p.turn_deg, 0.0);
    // 5 m along -z from the start camera is 5 m north (ARKit .gravityAndHeading: x east, y up, z south)
    let q = p.apply([1.0, 0.5, 2.0 - 5.0]);
    assert!((q.x - 100.0).abs() < 1e-9 && (q.y - 205.0).abs() < 1e-9 && (q.z - 50.0).abs() < 1e-9);
    // 3 m along +x is east
    let e = p.apply([4.0, 0.5, 2.0]);
    assert!((e.x - 103.0).abs() < 1e-9 && (e.y - 200.0).abs() < 1e-9);
    // the pivot height lifts everything
    let h = from_compass(st, [0.0; 3], PIVOT_HEIGHT_M).unwrap();
    assert!((h.apply([0.0; 3]).z - 50.30).abs() < 1e-9);
}

#[test]
fn compass_refuses_non_finite() {
    use cave_capture::frame::from_compass;
    let st = cave_survey::Vec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    assert!(from_compass(st, [f64::NAN, 0.0, 0.0], 0.3).is_none());
    assert!(from_compass(st, [0.0; 3], f64::INFINITY).is_none());
    let bad = cave_survey::Vec3 { x: f64::NAN, ..st };
    assert!(from_compass(bad, [0.0; 3], 0.3).is_none());
}
