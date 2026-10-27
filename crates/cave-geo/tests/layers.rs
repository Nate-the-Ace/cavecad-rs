//! Photo matrix, contours and terrain in the cave frame, and the datum offset rule.

use cave_geo::frame::{LatLon, Placement};
use cave_geo::grid::Grid;
use cave_geo::layers::{ENTRANCE_DEPTH_M, contours, datum_offset, photo_matrix, terrain};
use cave_geo::request::Bounds;

const BG: LatLon = LatLon {
    lat: 36.99,
    lon: -86.44,
};

#[test]
fn matrix_corners() {
    let p = Placement::new([5.0, 5.0], BG);
    let (e, n) = p.cave_to_utm(5.0, 5.0);
    let b = Bounds {
        xmin: e - 100.0,
        ymin: n - 50.0,
        xmax: e + 100.0,
        ymax: n + 50.0,
    };
    let m = photo_matrix(&b, (400, 200), &p);
    let apply = |px: f64, py: f64| [m[0] * px + m[1] * py + m[2], m[3] * px + m[4] * py + m[5]];
    let nw = p.utm_to_cave(b.xmin, b.ymax);
    let se = p.utm_to_cave(b.xmax, b.ymin);
    let a = apply(0.0, 0.0);
    let z = apply(400.0, 200.0);
    assert!((a[0] - nw[0]).abs() < 1e-3 && (a[1] - nw[1]).abs() < 1e-3);
    assert!((z[0] - se[0]).abs() < 1e-3 && (z[1] - se[1]).abs() < 1e-3);
}

#[test]
fn offset_rule() {
    assert!((ENTRANCE_DEPTH_M - 1.524).abs() < 1e-12);
    assert!((datum_offset(Some(200.0), -30.0).unwrap() - 228.476).abs() < 1e-9);
    assert_eq!(datum_offset(None, -30.0), None);
}

#[test]
fn majors_every_fifth() {
    let g = Grid {
        w: 3,
        h: 3,
        values: vec![0., 30., 60., 0., 30., 60., 0., 30., 60.],
    };
    let p = Placement::new([0.0, 0.0], BG);
    let (e, n) = p.cave_to_utm(0.0, 0.0);
    let b = Bounds {
        xmin: e,
        ymin: n,
        xmax: e + 3.0,
        ymax: n + 3.0,
    };
    let lines = contours(&g, &b, &p, 10.0, false, None, -42.0);
    let lv: Vec<(f64, bool)> = lines.iter().map(|l| (l.level, l.major)).collect();
    assert!(lv.contains(&(50.0, true)) && lv.contains(&(10.0, false)));
    // feet: 10 ft interval, levels in feet
    let ft = contours(&g, &b, &p, 10.0, true, None, -42.0);
    assert!(ft.iter().any(|l| l.level == 190.0)); // 60 m = 196.85 ft, so 190 ft is the top level
}

#[test]
fn terrain_needs_a_datum() {
    let g = Grid {
        w: 2,
        h: 2,
        values: vec![1., 2., 3., 4.],
    };
    let p = Placement::new([0.0, 0.0], BG);
    let (e, n) = p.cave_to_utm(0.0, 0.0);
    let b = Bounds {
        xmin: e,
        ymin: n,
        xmax: e + 2.0,
        ymax: n + 2.0,
    };
    assert!(terrain(&g, &b, &p, None).is_none());
    let t = terrain(&g, &b, &p, Some(-100.0)).unwrap();
    assert_eq!(t.xyz.len(), 12);
    assert_eq!(t.xyz[2], 1.0 + 100.0);
}

#[test]
fn contours_lie_flat_at_the_given_z_without_an_offset() {
    let g = Grid {
        w: 3,
        h: 3,
        values: vec![0., 30., 60., 0., 30., 60., 0., 30., 60.],
    };
    let p = Placement::new([0.0, 0.0], BG);
    let (e, n) = p.cave_to_utm(0.0, 0.0);
    let b = Bounds {
        xmin: e,
        ymin: n,
        xmax: e + 3.0,
        ymax: n + 3.0,
    };
    let flat = contours(&g, &b, &p, 10.0, false, None, -42.0);
    assert!(!flat.is_empty());
    assert!(flat.iter().all(|c| c.xyz.chunks(3).all(|v| v[2] == -42.0)));
    let up = contours(&g, &b, &p, 10.0, false, Some(100.0), -42.0);
    assert!(
        up.iter()
            .all(|c| c.xyz.chunks(3).all(|v| v[2] == c.level - 100.0))
    );
}

#[test]
fn negative_levels_have_majors_too() {
    let g = Grid {
        w: 3,
        h: 3,
        values: vec![-60., -30., 0., -60., -30., 0., -60., -30., 0.],
    };
    let p = Placement::new([0.0, 0.0], BG);
    let (e, n) = p.cave_to_utm(0.0, 0.0);
    let b = Bounds {
        xmin: e,
        ymin: n,
        xmax: e + 3.0,
        ymax: n + 3.0,
    };
    let lv: Vec<(f64, bool)> = contours(&g, &b, &p, 10.0, false, None, 0.0)
        .iter()
        .map(|l| (l.level, l.major))
        .collect();
    assert!(lv.contains(&(-50.0, true)), "{lv:?}");
    assert!(lv.contains(&(-40.0, false)), "{lv:?}");
}

#[test]
fn terrain_keeps_a_hole_as_nan() {
    let g = Grid {
        w: 2,
        h: 2,
        values: vec![1., f32::NAN, 3., 4.],
    };
    let p = Placement::new([0.0, 0.0], BG);
    let (e, n) = p.cave_to_utm(0.0, 0.0);
    let b = Bounds {
        xmin: e,
        ymin: n,
        xmax: e + 2.0,
        ymax: n + 2.0,
    };
    let t = terrain(&g, &b, &p, Some(10.0)).unwrap();
    assert_eq!(t.xyz.len(), 12);
    assert_eq!(t.xyz[2], -9.0);
    assert!(t.xyz[5].is_nan(), "the hole");
    assert_eq!(t.xyz[8], -7.0);
}

#[test]
fn a_zero_sized_photo_does_not_divide_by_zero() {
    let p = Placement::new([0.0, 0.0], BG);
    let (e, n) = p.cave_to_utm(0.0, 0.0);
    let b = Bounds {
        xmin: e,
        ymin: n,
        xmax: e + 10.0,
        ymax: n + 10.0,
    };
    assert!(photo_matrix(&b, (0, 0), &p).iter().all(|v| v.is_finite()));
}

#[test]
fn the_entrance_ground_sits_one_entrance_depth_above_the_entrance() {
    // Ground 166 m NAVD88 at the entrance; the entrance station is at survey z = -3 m.
    let g = Grid {
        w: 1,
        h: 1,
        values: vec![166.0],
    };
    let p = Placement::new([0.0, 0.0], BG);
    let (e, n) = p.cave_to_utm(0.0, 0.0);
    let b = Bounds {
        xmin: e - 0.5,
        ymin: n - 0.5,
        xmax: e + 0.5,
        ymax: n + 0.5,
    };
    let off = datum_offset(Some(166.0), -3.0);
    let t = terrain(&g, &b, &p, off).unwrap();
    assert!(
        (t.xyz[2] - (-3.0 + ENTRANCE_DEPTH_M)).abs() < 1e-9,
        "z = {}",
        t.xyz[2]
    );
}
