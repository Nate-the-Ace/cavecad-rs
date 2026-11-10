//! Integration tests.
use cave_geo::frame::{LatLon, Placement};
use cave_geo::request::{Bounds, ELEVATION, PHOTO, dem_url, naip_url, pixel_size, window};

const BG: LatLon = LatLon {
    lat: 36.99,
    lon: -86.44,
};

#[test]
fn window_floor_and_margin() {
    let p = Placement::new([0.0, 0.0], BG);
    let w = window([0.0, 0.0, 100.0, 40.0], &p);
    assert!(w.width() >= 150.0 && w.height() >= 150.0);
    let w = window([-200.0, -100.0, 200.0, 100.0], &p);
    assert!(w.width() >= 500.0 && w.height() >= 250.0);
    for (x, y) in [
        (-250.0, -125.0),
        (250.0, -125.0),
        (250.0, 125.0),
        (-250.0, 125.0),
    ] {
        let (e, n) = p.cave_to_utm(x, y);
        assert!(
            e >= w.xmin - 1e-6 && e <= w.xmax + 1e-6 && n >= w.ymin - 1e-6 && n <= w.ymax + 1e-6
        );
    }
}

#[test]
fn pixels_keep_aspect() {
    let b = Bounds {
        xmin: 0.0,
        ymin: 0.0,
        xmax: 500.0,
        ymax: 250.0,
    };
    let (w, h) = pixel_size(&b, PHOTO);
    assert!(((w as f64 / h as f64) - 2.0).abs() < 0.02);
    assert!(w <= 4000 && h <= 4000);
    let tiny = Bounds {
        xmin: 0.0,
        ymin: 0.0,
        xmax: 10.0,
        ymax: 10.0,
    };
    assert_eq!(pixel_size(&tiny, PHOTO), (256, 256));
    let long = Bounds {
        xmin: 0.0,
        ymin: 0.0,
        xmax: 3750.0,
        ymax: 150.0,
    };
    assert_eq!(pixel_size(&long, PHOTO), (4000, 160));
    let dem = pixel_size(&b, ELEVATION);
    assert_eq!(dem, (500, 250));
}

#[test]
fn urls() {
    let b = Bounds {
        xmin: 550000.0,
        ymin: 4094000.0,
        xmax: 550200.5,
        ymax: 4094200.0,
    };
    let u = naip_url(&b, 16, (400, 400));
    assert!(u.starts_with("https://imagery.nationalmap.gov/arcgis/rest/services/USGSNAIPImagery/ImageServer/exportImage?"));
    assert!(u.contains("bbox=550000.000,4094000.000,550200.500,4094200.000"));
    assert!(u.contains("bboxSR=32616&imageSR=32616&size=400,400&format=jpg&f=image"));
    let d = dem_url(&b, 16, (200, 200));
    assert!(d.starts_with("https://elevation.nationalmap.gov/arcgis/rest/services/3DEPElevation/ImageServer/exportImage?"));
    assert!(d.contains("&format=tiff&pixelType=F32&f=image"));
}
