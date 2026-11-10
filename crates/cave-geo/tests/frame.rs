//! Frame tests: zone choice, entrance identity, true north as cave y, round trips.

use cave_geo::frame::{LatLon, Placement, utm_zone};

const BG: LatLon = LatLon {
    lat: 36.99,
    lon: -86.44,
}; // a town block, not a cave

#[test]
fn zone_choice() {
    assert_eq!(utm_zone(BG), 16);
    assert_eq!(
        utm_zone(LatLon {
            lat: -33.0,
            lon: -86.44
        }),
        -16
    );
    assert_eq!(cave_geo::frame::epsg(16), 32616);
    assert_eq!(cave_geo::frame::epsg(-16), 32716);
}

#[test]
fn entrance_is_its_own_coordinate() {
    let p = Placement::new([12.0, -7.0], BG);
    let ll = p.cave_to_latlon(12.0, -7.0);
    assert!((ll.lat - BG.lat).abs() < 1e-9 && (ll.lon - BG.lon).abs() < 1e-9);
}

#[test]
fn true_north_is_cave_y() {
    // Bowling Green is east of zone 16's central meridian (-87), so grid north is east of true north.
    let p = Placement::new([0.0, 0.0], BG);
    assert!(p.convergence_deg() > 0.0);
    let north = LatLon {
        lat: BG.lat + 0.001,
        lon: BG.lon,
    };
    let (e, n) = p.latlon_to_utm(north);
    let xy = p.utm_to_cave(e, n);
    assert!(xy[0].abs() < 1e-3, "x = {}", xy[0]);
    assert!(xy[1] > 110.0 && xy[1] < 112.0, "y = {}", xy[1]);
}

#[test]
fn round_trip() {
    let p = Placement::new([100.0, 50.0], BG);
    for (x, y) in [(0.0, 0.0), (350.0, -120.0), (-800.0, 900.0)] {
        let (e, n) = p.cave_to_utm(x, y);
        let back = p.utm_to_cave(e, n);
        assert!((back[0] - x).abs() < 1e-3 && (back[1] - y).abs() < 1e-3);
        let ll = p.cave_to_latlon(x, y);
        let (e2, n2) = p.latlon_to_utm(ll);
        assert!((e2 - e).abs() < 1e-3 && (n2 - n).abs() < 1e-3);
    }
}
