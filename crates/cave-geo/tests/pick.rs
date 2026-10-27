//! Integration tests.
use cave_geo::frame::{LatLon, Placement};
use cave_geo::grid::Grid;
use cave_geo::pick::lowest_in_circle;
use cave_geo::request::Bounds;

const BG: LatLon = LatLon {
    lat: 36.99,
    lon: -86.44,
};

fn setup(values: impl Fn(usize, usize) -> f32) -> (Grid, Bounds, Placement) {
    let place = Placement::new([0.0, 0.0], BG);
    let (e0, n0) = place.cave_to_utm(0.0, 0.0);
    let b = Bounds {
        xmin: e0 - 10.0,
        ymin: n0 - 10.0,
        xmax: e0 + 10.0,
        ymax: n0 + 10.0,
    };
    let mut v = Vec::new();
    for r in 0..20 {
        for c in 0..20 {
            v.push(values(c, r));
        }
    }
    (
        Grid {
            w: 20,
            h: 20,
            values: v,
        },
        b,
        place,
    )
}

fn cave_of(g: &Grid, b: &Bounds, p: &Placement, c: f64, r: f64) -> [f64; 2] {
    let (e, n) = g.cell_utm(b, c, r);
    p.utm_to_cave(e, n)
}

#[test]
fn finds_the_bowl() {
    let (g, b, p) = setup(|c, r| ((c as f32 - 6.0).powi(2) + (r as f32 - 4.0).powi(2)) + 100.0);
    let at = cave_of(&g, &b, &p, 6.0, 4.0);
    let hit = lowest_in_circle(&g, &b, &p, [at[0] + 1.5, at[1] - 1.0], 4.0).unwrap();
    assert!((hit.point[0] - at[0]).abs() < 1e-6 && (hit.point[1] - at[1]).abs() < 1e-6);
    assert_eq!(hit.ground_m, 100.0);
    let ll = p.cave_to_latlon(hit.point[0], hit.point[1]);
    assert_eq!((hit.at.lat, hit.at.lon), (ll.lat, ll.lon));
}

#[test]
fn stays_inside_the_circle() {
    let (g, b, p) = setup(|c, r| ((c as f32 - 6.0).powi(2) + (r as f32 - 4.0).powi(2)) + 100.0);
    let far = cave_of(&g, &b, &p, 15.0, 15.0);
    let hit = lowest_in_circle(&g, &b, &p, far, 2.0).unwrap();
    let d = ((hit.point[0] - far[0]).powi(2) + (hit.point[1] - far[1]).powi(2)).sqrt();
    assert!(d <= 2.0 + 1e-9);
    assert!(hit.ground_m > 100.0);
}

#[test]
fn ties_go_to_the_centre() {
    let (g, b, p) = setup(|c, r| {
        if (c == 3 && r == 3) || (c == 8 && r == 3) {
            50.0
        } else {
            90.0
        }
    });
    let near8 = cave_of(&g, &b, &p, 7.0, 3.0);
    let hit = lowest_in_circle(&g, &b, &p, near8, 6.0).unwrap();
    let want = cave_of(&g, &b, &p, 8.0, 3.0);
    assert!((hit.point[0] - want[0]).abs() < 1e-6);
}

#[test]
fn holes_and_off_grid() {
    let (g, b, p) = setup(|_, _| -1.0e30);
    assert!(lowest_in_circle(&g, &b, &p, [0.0, 0.0], 5.0).is_none());
    let (g, b, p) = setup(|_, _| 10.0);
    assert!(lowest_in_circle(&g, &b, &p, [500.0, 500.0], 5.0).is_none());
}
