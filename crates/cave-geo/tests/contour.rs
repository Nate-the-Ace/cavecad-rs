//! Integration tests.
use cave_geo::contour::{levels, lines};
use cave_geo::grid::Grid;

#[test]
fn level_list() {
    assert_eq!(
        levels(186.6, 232.0, 10.0),
        vec![190.0, 200.0, 210.0, 220.0, 230.0]
    );
    assert!(levels(5.0, 5.0, 10.0).is_empty());
    assert!(levels(0.0, 100.0, 0.0).is_empty());
}

#[test]
fn ramp() {
    let g = Grid {
        w: 3,
        h: 3,
        values: vec![0., 1., 2., 0., 1., 2., 0., 1., 2.],
    };
    let runs = lines(&g, 0.5);
    assert_eq!(runs.len(), 1);
    assert!(!runs[0].closed);
    assert_eq!(runs[0].points.len(), 3);
    assert!(runs[0].points.iter().all(|p| (p[0] - 0.5).abs() < 1e-9));
    let ys: Vec<f64> = runs[0].points.iter().map(|p| p[1]).collect();
    assert_eq!(ys.iter().cloned().fold(f64::MAX, f64::min), 0.0);
    assert_eq!(ys.iter().cloned().fold(f64::MIN, f64::max), 2.0);
}

#[test]
fn peak_closes_and_holes_are_safe() {
    let mut v = vec![
        0., 0., 0., 0., 0., 0., 1., 1., 1., 0., 0., 1., 4., 1., 0., 0., 1., 1., 1., 0., 0., 0., 0.,
        0., 0.,
    ];
    let g = Grid {
        w: 5,
        h: 5,
        values: v.clone(),
    };
    let runs = lines(&g, 2.0);
    assert_eq!(runs.len(), 1);
    assert!(runs[0].closed);
    v[0] = -999_999.0;
    let holed = Grid {
        w: 5,
        h: 5,
        values: v,
    };
    assert!(!lines(&holed, 0.5).is_empty());
}

#[test]
fn real_grid() {
    let g = Grid::parse_tiff(include_bytes!("data/bowling-green-200.tif")).unwrap();
    let (lo, hi) = g.range().unwrap();
    let lv = levels(lo, hi, 2.0);
    assert!(!lv.is_empty());
    let runs: Vec<_> = lv.iter().flat_map(|l| lines(&g, *l)).collect();
    assert!(!runs.is_empty());
    for r in runs {
        for p in r.points {
            assert!(p[0] >= 0.0 && p[0] <= 199.0 && p[1] >= 0.0 && p[1] <= 199.0);
        }
    }
}
