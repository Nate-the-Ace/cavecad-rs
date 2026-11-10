//! Integration tests.
use cave_geo::datum::{Datum, FixedFrame};
use cave_geo::frame::LatLon;

fn metres(a: LatLon, b: LatLon) -> f64 {
    let dy = (a.lat - b.lat) * 111_000.0;
    let dx = (a.lon - b.lon) * 111_000.0 * a.lat.to_radians().cos();
    (dx * dx + dy * dy).sqrt()
}

#[test]
fn names() {
    assert_eq!(Datum::parse("North American 1927"), Some(Datum::Nad27));
    assert_eq!(Datum::parse("nad27"), Some(Datum::Nad27));
    assert_eq!(Datum::parse("North American 1983"), Some(Datum::Nad83));
    assert_eq!(Datum::parse("NAD83"), Some(Datum::Nad83));
    assert_eq!(Datum::parse("WGS 1984"), Some(Datum::Wgs84));
    assert_eq!(Datum::parse("WGS84"), Some(Datum::Wgs84));
    assert_eq!(Datum::parse("Tokyo"), None);
}

#[test]
fn nad27_shift_is_tens_of_metres() {
    let p = LatLon {
        lat: 37.0,
        lon: -86.4,
    };
    let w = Datum::Nad27.to_wgs84(p);
    let d = metres(p, w);
    assert!(d > 5.0 && d < 60.0, "shift {d} m");
    assert_eq!(Datum::Nad83.to_wgs84(p), p);
    assert_eq!(Datum::Wgs84.to_wgs84(p), p);
}

#[test]
fn fixed_frame() {
    let f = FixedFrame {
        zone: 16,
        datum: Datum::Nad83,
    };
    let ll = f.to_latlon(550_000.0, 4_094_000.0);
    let (lat, lon) = cave_geomag::utm_to_latlon(550_000.0, 4_094_000.0, 16);
    assert_eq!((ll.lat, ll.lon), (lat, lon));
    let j = serde_json::to_string(&f).unwrap();
    assert_eq!(serde_json::from_str::<FixedFrame>(&j).unwrap(), f);
}

#[test]
fn nad27_shift_goes_north_and_east() {
    let p = LatLon {
        lat: 37.0,
        lon: -86.4,
    };
    let w = Datum::Nad27.to_wgs84(p);
    let dlat = (w.lat - p.lat) * 3600.0;
    let dlon = (w.lon - p.lon) * 3600.0;
    assert!(dlat > 0.2 && dlat < 0.35, "dlat {dlat}\"");
    assert!(dlon > 0.03 && dlon < 0.13, "dlon {dlon}\"");
}

#[test]
fn pole_is_not_nan() {
    for lat in [90.0, -90.0] {
        let w = Datum::Nad27.to_wgs84(LatLon { lat, lon: 10.0 });
        assert!(w.lat.is_finite() && w.lon.is_finite(), "{w:?}");
        assert!((w.lat - lat).abs() < 0.01, "{w:?}");
    }
}
