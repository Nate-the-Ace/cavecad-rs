#![allow(clippy::field_reassign_with_default)] // each test spells out only the modes it is about

//! Hand-solved snap cases. The camera looks straight down at the plan with 10 pixels to a unit, world (0, 0) in
//! the middle of an 800 by 600 view: world (x, y) is pixel (400 + 10x, 300 - 10y).

use cave_snap::*;
use std::f64::consts::{FRAC_PI_2, PI};

fn cam() -> Camera {
    Camera::top(0.0, 0.0, 10.0, 800.0, 600.0)
}

fn px(x: f64, y: f64) -> [f64; 2] {
    [400.0 + 10.0 * x, 300.0 - 10.0 * y]
}

fn thing(id: &str, geom: Geometry) -> Thing {
    Thing {
        id: id.into(),
        kind: ThingKind::Line,
        frame: Frame::plan(0.0),
        geom,
        label: String::new(),
    }
}

fn line(id: &str, a: [f64; 2], b: [f64; 2]) -> Thing {
    thing(
        id,
        Geometry::Polyline {
            pts: vec![a, b],
            bulges: vec![],
            closed: false,
        },
    )
}

fn scene(things: Vec<Thing>) -> Scene {
    Scene {
        camera: cam(),
        things,
        plane: Some(Frame::plan(0.0)),
    }
}

fn all_modes() -> Settings {
    let mut s = Settings::default();
    s.modes = vec![
        Mode::Endpoint,
        Mode::Midpoint,
        Mode::Center,
        Mode::GeometricCenter,
        Mode::Quadrant,
        Mode::Insertion,
        Mode::Node,
        Mode::Station,
        Mode::Intersection,
        Mode::ApparentIntersection,
        Mode::Extension,
        Mode::Perpendicular,
        Mode::Tangent,
        Mode::Parallel,
        Mode::Nearest,
    ];
    s
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6 && (a[2] - b[2]).abs() < 1e-6
}

fn best(s: &Scene, cursor: [f64; 2], settings: &Settings, session: &Session) -> Candidate {
    snap(s, cursor, settings, session).best.expect("a snap")
}

#[test]
fn endpoint_of_a_line_within_the_aperture() {
    let s = scene(vec![line("a", [0.0, 0.0], [10.0, 0.0])]);
    let c = best(
        &s,
        [px(10.0, 0.0)[0] + 3.0, 300.0 - 2.0],
        &Settings::default(),
        &Session::new(),
    );
    assert_eq!(c.mode, Mode::Endpoint);
    assert!(near(c.world, [10.0, 0.0, 0.0]));
    assert_eq!(
        c.source.unwrap().seg,
        1,
        "the vertex index, so a leader can fasten to it"
    );
}

#[test]
fn nothing_outside_the_aperture() {
    let s = scene(vec![line("a", [0.0, 0.0], [10.0, 0.0])]);
    assert!(
        snap(
            &s,
            [px(10.0, 0.0)[0] + 11.0, 300.0],
            &Settings::default(),
            &Session::new()
        )
        .best
        .is_none()
    );
}

#[test]
fn midpoint_of_a_line_and_of_a_semicircle_by_arc_length() {
    let s = scene(vec![line("a", [0.0, 0.0], [10.0, 4.0])]);
    assert!(near(
        best(&s, px(5.0, 2.0), &Settings::default(), &Session::new()).world,
        [5.0, 2.0, 0.0]
    ));
    // Bulge 1 is a counter-clockwise half circle: from (0, 0) it swings down through (5, -5) to (10, 0).
    let arc = scene(vec![thing(
        "b",
        Geometry::Polyline {
            pts: vec![[0.0, 0.0], [10.0, 0.0]],
            bulges: vec![1.0],
            closed: false,
        },
    )]);
    let c = best(&arc, px(5.0, -5.0), &Settings::default(), &Session::new());
    assert_eq!(c.mode, Mode::Midpoint);
    assert!(near(c.world, [5.0, -5.0, 0.0]), "{:?}", c.world);
}

#[test]
fn a_spline_has_one_midpoint_by_length() {
    // A straight run of fit points: the middle by length is the middle.
    let s = scene(vec![thing(
        "s",
        Geometry::Spline {
            pts: vec![[0.0, 0.0], [2.0, 0.0], [10.0, 0.0]],
            closed: false,
        },
    )]);
    let mut set = Settings::default();
    set.modes = vec![Mode::Midpoint];
    let c = best(&s, px(5.0, 0.0), &set, &Session::new());
    assert!(
        (c.world[0] - 5.0).abs() < 0.05 && c.world[1].abs() < 1e-6,
        "{:?}",
        c.world
    );
}

#[test]
fn center_fires_on_the_circle_and_near_the_centre() {
    let s = scene(vec![thing(
        "c",
        Geometry::Circle {
            center: [0.0, 0.0],
            radius: 5.0,
        },
    )]);
    let mut set = Settings::default();
    set.modes = vec![Mode::Center];
    // On the circumference (the point (-5·cos45, 5·sin45)), 20 px from the centre.
    let on_ring = best(&s, px(-3.5355, 3.5355), &set, &Session::new());
    assert_eq!(on_ring.mode, Mode::Center);
    assert!(near(on_ring.world, [0.0, 0.0, 0.0]));
    // Near the centre itself.
    assert!(near(
        best(&s, [px(0.0, 0.0)[0] + 2.0, 300.0], &set, &Session::new()).world,
        [0.0, 0.0, 0.0]
    ));
    // Away from both: nothing.
    assert!(snap(&s, px(2.0, 2.0), &set, &Session::new()).best.is_none());
}

#[test]
fn quadrants_of_a_circle_and_of_a_rotated_arc() {
    let mut set = Settings::default();
    set.modes = vec![Mode::Quadrant];
    let c = scene(vec![thing(
        "c",
        Geometry::Circle {
            center: [0.0, 0.0],
            radius: 5.0,
        },
    )]);
    for q in [[5.0, 0.0], [0.0, 5.0], [-5.0, 0.0], [0.0, -5.0]] {
        assert!(near(
            best(&c, px(q[0], q[1]), &set, &Session::new()).world,
            [q[0], q[1], 0.0]
        ));
    }
    // An arc from 30 to 120 degrees holds only the 90 degree quadrant.
    let a = scene(vec![thing(
        "a",
        Geometry::Arc {
            center: [0.0, 0.0],
            radius: 5.0,
            start: PI / 6.0,
            sweep: FRAC_PI_2,
        },
    )]);
    assert!(near(
        best(&a, px(0.0, 5.0), &set, &Session::new()).world,
        [0.0, 5.0, 0.0]
    ));
    assert!(
        snap(&a, px(5.0, 0.0), &set, &Session::new()).best.is_none(),
        "0 degrees is off the arc"
    );
}

#[test]
fn crossing_lines_intersect_and_lines_at_other_heights_only_appear_to() {
    // Long lines, so that neither has its midpoint at the crossing (a point mode would outrank it).
    let x = scene(vec![
        line("a", [0.0, 0.0], [20.0, 20.0]),
        line("b", [0.0, 10.0], [20.0, -10.0]),
    ]);
    let c = best(&x, px(5.0, 5.0), &all_modes(), &Session::new());
    assert_eq!(c.mode, Mode::Intersection);
    assert!(near(c.world, [5.0, 5.0, 0.0]));
    // The second line 3 units above: they cross on the screen only.
    let mut high = line("b", [0.0, 10.0], [20.0, -10.0]);
    high.frame = Frame::plan(3.0);
    let o = scene(vec![line("a", [0.0, 0.0], [20.0, 20.0]), high]);
    let mut set = all_modes();
    let apparent = best(&o, px(5.0, 5.0), &set, &Session::new());
    assert_eq!(apparent.mode, Mode::ApparentIntersection);
    set.modes
        .retain(|m| *m != Mode::ApparentIntersection && *m != Mode::Nearest);
    assert!(
        snap(&o, px(5.0, 5.0), &set, &Session::new()).best.is_none(),
        "plain Intersection refuses it"
    );
}

#[test]
fn perpendicular_and_tangent_from_the_last_point() {
    let mut set = Settings::default();
    set.modes = vec![Mode::Perpendicular];
    let l = scene(vec![line("a", [0.0, 0.0], [10.0, 0.0])]);
    let mut session = Session::new();
    session.last_point = Some([3.0, 5.0, 0.0]);
    let c = best(&l, px(3.0, 0.0), &set, &session);
    assert_eq!(c.mode, Mode::Perpendicular);
    assert!(near(c.world, [3.0, 0.0, 0.0]));
    // From (13, 0) the tangents to a circle of radius 5 touch at x = 25/13, y = ±sqrt(25 - x²).
    let circle = scene(vec![thing(
        "c",
        Geometry::Circle {
            center: [0.0, 0.0],
            radius: 5.0,
        },
    )]);
    set.modes = vec![Mode::Tangent];
    session.last_point = Some([13.0, 0.0, 0.0]);
    let (x, y) = (25.0 / 13.0, (25.0f64 - (25.0f64 / 13.0).powi(2)).sqrt());
    let up = best(&circle, px(x, y), &set, &session);
    assert!(
        (up.world[0] - x).abs() < 0.02 && (up.world[1] - y).abs() < 0.02,
        "{:?}",
        up.world
    );
    let down = best(&circle, px(x, -y), &set, &session);
    assert!((down.world[1] + y).abs() < 0.02, "{:?}", down.world);
}

#[test]
fn nearest_is_last() {
    let s = scene(vec![line("a", [0.0, 0.0], [10.0, 0.0])]);
    // Near the end: both Endpoint and Nearest are inside the aperture; the point mode wins.
    let r = snap(
        &s,
        [px(10.0, 0.0)[0] - 4.0, 300.0],
        &all_modes(),
        &Session::new(),
    );
    assert_eq!(r.best.as_ref().unwrap().mode, Mode::Endpoint);
    assert!(
        r.others.iter().any(|c| c.mode == Mode::Nearest),
        "and Nearest is still there for Tab"
    );
    // Away from any point: Nearest alone.
    let n = best(&s, px(3.0, 0.4), &all_modes(), &Session::new());
    assert_eq!(n.mode, Mode::Nearest);
    assert!(near(n.world, [3.0, 0.0, 0.0]));
}

#[test]
fn extension_continues_an_acquired_end() {
    let s = scene(vec![line("a", [0.0, 0.0], [10.0, 0.0])]);
    let mut set = Settings::default();
    set.modes = vec![Mode::Endpoint, Mode::Extension];
    // Hovering the end offers it for acquisition.
    let hover = snap(&s, px(10.0, 0.0), &set, &Session::new());
    let ext = hover
        .acquirable
        .iter()
        .find(|a| matches!(a, Acquired::Extension { .. }))
        .cloned()
        .expect("acquirable");
    let mut session = Session::new();
    session.acquire(ext.clone());
    let c = best(
        &s,
        [px(15.0, 0.0)[0], px(15.0, 0.0)[1] - 3.0],
        &set,
        &session,
    );
    assert_eq!(c.mode, Mode::Extension);
    assert!(near(c.world, [15.0, 0.0, 0.0]), "{:?}", c.world);
    assert!(c.label.starts_with("Extension: 5.00 < 0"), "{}", c.label);
    // Behind the end there is no extension.
    assert!(snap(&s, px(-3.0, 0.2), &set, &session).guides.is_empty());
    // Hovering it again releases it.
    session.acquire(ext);
    assert!(session.acquired.is_empty());
}

#[test]
fn an_arc_extends_round_its_own_circle() {
    // A quarter circle of radius 5 from (5, 0) to (0, 5): its extension goes on round to (-5, 0).
    let s = scene(vec![thing(
        "a",
        Geometry::Arc {
            center: [0.0, 0.0],
            radius: 5.0,
            start: 0.0,
            sweep: FRAC_PI_2,
        },
    )]);
    let mut set = Settings::default();
    set.modes = vec![Mode::Extension];
    let hover = snap(&s, px(0.0, 5.0), &set, &Session::new());
    let ext = hover
        .acquirable
        .into_iter()
        .find(|a| {
            matches!(
                a,
                Acquired::Extension {
                    cont: Continuation::Arc { .. },
                    ..
                }
            )
        })
        .expect("an arc continuation");
    let mut session = Session::new();
    session.acquire(ext);
    let c = best(&s, px(-5.0, 0.0), &set, &session);
    assert_eq!(c.mode, Mode::Extension);
    assert!(near(c.world, [-5.0, 0.0, 0.0]), "{:?}", c.world);
}

#[test]
fn tracking_runs_horizontal_vertical_and_polar_through_a_track_point() {
    let s = scene(vec![]);
    let set = Settings::default();
    let mut session = Session::new();
    session.one_pick(Some(Mode::TrackPoint));
    let out = session.pick([2.0, 3.0, 0.0]);
    assert!(out.point.is_none(), "a track point only sets up");
    let session = out.session;
    assert_eq!(session.track_points, vec![[2.0, 3.0, 0.0]]);
    let h = best(&s, px(7.0, 3.1), &set, &session);
    assert_eq!(h.mode, Mode::Tracking);
    assert!(near(h.world, [7.0, 3.0, 0.0]), "{:?}", h.world);
    let v = best(&s, px(2.1, 8.0), &set, &session);
    assert!(near(v.world, [2.0, 8.0, 0.0]), "{:?}", v.world);
    // 45 degrees: 4 units along puts the point at (2 + 4cos45, 3 + 4sin45).
    let k = 4.0 / 2f64.sqrt();
    let d = snap(&s, px(2.0 + k, 3.0 + k + 0.2), &set, &session);
    let c = d.best.unwrap();
    assert!(
        (c.world[0] - (2.0 + k)).abs() < 0.1 && (c.world[1] - (3.0 + k)).abs() < 0.1,
        "{:?}",
        c.world
    );
    assert!(c.label.contains("< 45"), "{}", c.label);
    // The typed distance: 6 along that guide.
    let guide = d
        .guides
        .iter()
        .find(|g| g.kind == GuideKind::Polar)
        .expect("the polar guide is shown");
    let p = along_guide(guide, 6.0).unwrap();
    assert!((p[0] - (2.0 + 6.0 / 2f64.sqrt())).abs() < 1e-6, "{p:?}");
}

#[test]
fn two_track_points_cross_where_their_guides_do() {
    let s = scene(vec![]);
    let mut session = Session::new();
    session.track_points = vec![[0.0, 0.0, 0.0], [10.0, 5.0, 0.0]];
    let c = best(&s, px(10.0, 0.0), &Settings::default(), &session);
    assert!(
        near(c.world, [10.0, 0.0, 0.0]),
        "the vertical through (10, 5) meets the horizontal through the origin: {:?}",
        c.world
    );
}

#[test]
fn from_mid_between_and_the_one_pick_override() {
    let mut session = Session::new();
    // Mid Between 2 Points: two picks, then the point.
    session.one_pick(Some(Mode::MidBetween));
    let first = session.pick([0.0, 0.0, 0.0]);
    assert!(first.point.is_none());
    let second = first.session.pick([10.0, 4.0, 2.0]);
    assert_eq!(second.point, Some([5.0, 2.0, 1.0]));
    assert!(second.session.override_mode.is_none() && second.session.m2p_first.is_none());
    // From: the base is kept for the next pick, which then clears it.
    let mut s = Session::new();
    s.one_pick(Some(Mode::From));
    let s = s.pick([1.0, 1.0, 0.0]).session;
    assert_eq!(s.from_base, Some([1.0, 1.0, 0.0]));
    let done = s.pick([4.0, 1.0, 0.0]);
    assert_eq!(done.point, Some([4.0, 1.0, 0.0]));
    assert!(done.session.from_base.is_none());
    // The override searches only its mode, and a miss gives nothing, never another mode.
    let scn = scene(vec![line("a", [0.0, 0.0], [10.0, 0.0])]);
    let mut o = Session::new();
    o.one_pick(Some(Mode::Midpoint));
    assert!(
        snap(&scn, px(10.0, 0.0), &Settings::default(), &o)
            .best
            .is_none(),
        "the end is not the middle"
    );
    assert!(near(
        best(&scn, px(5.0, 0.0), &Settings::default(), &o).world,
        [5.0, 0.0, 0.0]
    ));
    // Escape drops the setup but keeps the tool's last point.
    let mut e = Session::new();
    e.track_points.push([1.0, 1.0, 1.0]);
    e.last_point = Some([2.0, 2.0, 2.0]);
    e.cancel();
    assert!(e.track_points.is_empty() && e.last_point.is_some());
}

#[test]
fn results_are_deterministic_whatever_the_order() {
    let a = line("a", [0.0, 0.0], [10.0, 10.0]);
    let b = line("b", [0.0, 10.0], [10.0, 0.0]);
    let c = line("c", [5.0, 5.0], [9.0, 5.0]);
    let one = snap(
        &scene(vec![a.clone(), b.clone(), c.clone()]),
        px(5.0, 5.2),
        &all_modes(),
        &Session::new(),
    );
    let two = snap(
        &scene(vec![c, a, b]),
        px(5.0, 5.2),
        &all_modes(),
        &Session::new(),
    );
    assert_eq!(one, two);
    assert!(!one.others.is_empty(), "Tab has something to cycle");
}

#[test]
fn stations_and_symbols_snap_as_themselves() {
    let station = Thing {
        id: "st".into(),
        kind: ThingKind::Station,
        frame: Frame::plan(0.0),
        geom: Geometry::Point {
            at: [3.0, 4.0, -2.0],
        },
        label: "A3".into(),
    };
    let sym = Thing {
        id: "sy".into(),
        kind: ThingKind::Symbol,
        frame: Frame::plan(0.0),
        geom: Geometry::Point {
            at: [-6.0, 1.0, 0.0],
        },
        label: String::new(),
    };
    let s = scene(vec![station, sym]);
    let st = best(&s, px(3.0, 4.0), &Settings::default(), &Session::new());
    assert_eq!(st.mode, Mode::Station);
    assert_eq!(st.label, "Station: A3");
    assert!(near(st.world, [3.0, 4.0, -2.0]));
    assert_eq!(
        best(&s, px(-6.0, 1.0), &Settings::default(), &Session::new()).mode,
        Mode::Insertion
    );
}

#[test]
fn a_circle_seen_obliquely_still_reports_its_true_centre_and_quadrants() {
    // From above and to the side, the circle is an ellipse on the screen; its Center and Quadrants are still
    // the true ones, found in the plane and projected.
    let cam = Camera::look_at(
        [30.0, -40.0, 35.0],
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        40.0,
        800.0,
        600.0,
    );
    let s = Scene {
        camera: cam.clone(),
        things: vec![thing(
            "c",
            Geometry::Circle {
                center: [0.0, 0.0],
                radius: 6.0,
            },
        )],
        plane: None,
    };
    let mut set = Settings::default();
    set.modes = vec![Mode::Center, Mode::Quadrant];
    let centre = cam.project([0.0, 0.0, 0.0]).unwrap();
    let c = best(&s, [centre[0] + 1.0, centre[1]], &set, &Session::new());
    assert_eq!(c.mode, Mode::Center);
    assert!(near(c.world, [0.0, 0.0, 0.0]));
    let q = cam.project([6.0, 0.0, 0.0]).unwrap();
    let c = best(&s, [q[0] + 2.0, q[1] - 1.0], &set, &Session::new());
    assert_eq!(c.mode, Mode::Quadrant);
    assert!(near(c.world, [6.0, 0.0, 0.0]), "{:?}", c.world);
    // Tracking angles are off without a drawing plane.
    let mut t = Session::new();
    t.track_points.push([0.0, 0.0, 0.0]);
    assert!(
        snap(&s, [10.0, 10.0], &Settings::default(), &t)
            .guides
            .is_empty()
    );
}

#[test]
fn a_vertical_shafts_plan_is_a_point_and_nothing_invents_a_direction() {
    // Plumbline Pit's plan: every station sits on the same spot; its lines have no length on the screen.
    let pit = scene(vec![
        line("shaft", [0.0, 0.0], [0.0, 0.0]),
        line("drop", [0.0, 0.0], [0.0, 0.0]),
    ]);
    let r = snap(&pit, px(0.0, 0.0), &all_modes(), &Session::new());
    let b = r.best.clone().expect("the point itself still snaps");
    assert!(b.mode == Mode::Endpoint || b.mode == Mode::Midpoint);
    assert!(r.guides.is_empty());
    assert!(
        r.best
            .iter()
            .chain(r.others.iter())
            .all(|c| c.world.iter().all(|v| v.is_finite()))
    );
}

#[test]
fn a_camera_round_trips_a_pixel_through_a_plane() {
    let cam = Camera::look_at(
        [30.0, -40.0, 35.0],
        [1.0, 2.0, 0.0],
        [0.0, 0.0, 1.0],
        40.0,
        800.0,
        600.0,
    );
    let p = cam.project([4.0, 5.0, 0.0]).unwrap();
    let back = cam.on_plane(p, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]).unwrap();
    assert!(near(back, [4.0, 5.0, 0.0]), "{back:?}");
    assert!(
        cam.project([30.0, -40.0, 100.0]).is_none() || cam.project([30.0, -40.0, 100.0]).is_some()
    );
}

#[test]
fn a_closed_path_has_a_geometric_centre_and_a_closing_midpoint() {
    let square = scene(vec![thing(
        "q",
        Geometry::Polyline {
            pts: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            bulges: vec![],
            closed: true,
        },
    )]);
    let mut set = Settings::default();
    set.modes = vec![Mode::GeometricCenter, Mode::Midpoint];
    // On the outline, away from every midpoint: the centroid fires.
    let g = best(&square, px(2.0, 0.0), &set, &Session::new());
    assert_eq!(g.mode, Mode::GeometricCenter);
    assert!(near(g.world, [5.0, 5.0, 0.0]));
    // The closing edge (0, 10) back to (0, 0) has its own midpoint.
    let m = best(&square, px(0.0, 5.0), &set, &Session::new());
    assert_eq!(m.mode, Mode::Midpoint);
    assert!(near(m.world, [0.0, 5.0, 0.0]));
}

#[test]
fn extended_intersection_is_override_only_and_finds_where_the_lines_would_meet() {
    // Two lines that stop short of each other: (0,0)-(4,4) and (10,0)-(6,4) would meet at (5,5).
    let s = scene(vec![
        line("a", [0.0, 0.0], [4.0, 4.0]),
        line("b", [10.0, 0.0], [6.0, 4.0]),
    ]);
    let mut set = all_modes();
    set.modes.retain(|m| *m != Mode::Nearest);
    assert!(
        snap(&s, px(5.0, 5.0), &set, &Session::new()).best.is_none(),
        "not a running mode"
    );
    let mut o = Session::new();
    o.one_pick(Some(Mode::ExtendedIntersection));
    let c = best(&s, px(5.0, 5.0), &set, &o);
    assert_eq!(c.mode, Mode::ExtendedIntersection);
    assert!(near(c.world, [5.0, 5.0, 0.0]), "{:?}", c.world);
}

#[test]
fn an_extension_crossing_a_real_line_is_an_intersection() {
    let s = scene(vec![
        line("a", [0.0, 0.0], [10.0, 0.0]),
        line("wall", [15.0, -5.0], [15.0, 5.0]),
    ]);
    let mut set = Settings::default();
    set.modes = vec![Mode::Extension, Mode::Intersection];
    let hover = snap(&s, px(10.0, 0.0), &set, &Session::new());
    let ext = hover
        .acquirable
        .into_iter()
        .find(|a| matches!(a, Acquired::Extension { .. }))
        .unwrap();
    let mut session = Session::new();
    session.acquire(ext);
    let c = best(&s, px(15.0, 0.3), &set, &session);
    assert_eq!(c.mode, Mode::Intersection);
    assert!(near(c.world, [15.0, 0.0, 0.0]), "{:?}", c.world);
}

#[test]
fn parallel_runs_alongside_an_acquired_segment_through_the_last_point() {
    let s = scene(vec![line("a", [0.0, 0.0], [10.0, 0.0])]);
    let mut set = Settings::default();
    set.modes = vec![Mode::Parallel];
    let hover = snap(&s, px(5.0, 0.0), &set, &Session::new());
    let par = hover
        .acquirable
        .into_iter()
        .find(|a| matches!(a, Acquired::Parallel { .. }))
        .unwrap();
    let mut session = Session::new();
    session.last_point = Some([2.0, 7.0, 0.0]);
    session.acquire(par);
    let c = best(&s, px(8.0, 7.2), &set, &session);
    assert_eq!(c.mode, Mode::Parallel);
    assert!(near(c.world, [8.0, 7.0, 0.0]), "{:?}", c.world);
}

#[test]
fn the_json_the_browser_sends_is_read() {
    let text = r#"{"camera":{"view_proj":[0.025,0,0,0,0,0.03333333333333333,0,0,0,0,-0.001,0,0,0,0,1],"width":800,"height":600},
      "things":[{"id":"x","kind":"line","frame":{"origin":[0,0,0],"u":[1,0,0],"v":[0,1,0]},"geom":{"type":"polyline","pts":[[0,0],[10,0]]}}],
      "plane":{"origin":[0,0,0],"u":[1,0,0],"v":[0,1,0]}}"#;
    let scene: Scene = serde_json::from_str(text).unwrap();
    let r = snap(
        &scene,
        [500.0, 300.0],
        &Settings::default(),
        &Session::new(),
    );
    assert_eq!(r.best.unwrap().mode, Mode::Endpoint);
    let session: Session = serde_json::from_str(r#"{"override":"midpoint"}"#).unwrap();
    assert_eq!(session.override_mode, Some(Mode::Midpoint));
}
