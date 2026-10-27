//! What to download: the UTM window around a cave, its pixel size, and the USGS request URLs.
//! Ported from the old CsGeoProject (groundExtent, pixelSize, naipUrl, demUrl), with UTM in place of
//! Web Mercator so ground pixels are true metres.

use crate::frame::{Placement, epsg};
use serde::{Deserialize, Serialize};

/// A box in UTM metres (one zone).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    /// West edge, easting.
    pub xmin: f64,
    /// South edge, northing.
    pub ymin: f64,
    /// East edge.
    pub xmax: f64,
    /// North edge.
    pub ymax: f64,
}

impl Bounds {
    /// East-west size, metres.
    pub fn width(&self) -> f64 {
        self.xmax - self.xmin
    }
    /// North-south size, metres.
    pub fn height(&self) -> f64 {
        self.ymax - self.ymin
    }
}

/// Margin added around the cave (0.25 = 25%).
pub const MARGIN: f64 = 0.25;
/// Smallest window per axis, metres.
pub const FLOOR_M: f64 = 150.0;

/// A service's pixel rules.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Service {
    /// Native metres per pixel.
    pub native_m: f64,
    /// Largest side, pixels.
    pub max_px: f64,
    /// Smallest side, pixels (yields to aspect).
    pub min_px: f64,
}

/// NAIP: 0.3 m native, 4000 px service limit.
pub const PHOTO: Service = Service {
    native_m: 0.3,
    max_px: 4000.0,
    min_px: 256.0,
};
/// 3DEP: ~1 m native; 512 px is plenty for contours and terrain.
pub const ELEVATION: Service = Service {
    native_m: 1.0,
    max_px: 512.0,
    min_px: 64.0,
};

/// The UTM window for a cave whose plan extent is `[min_x, min_y, max_x, max_y]` (cave metres):
/// padded by [`MARGIN`], at least [`FLOOR_M`] each way, then the UTM box holding the padded corners.
pub fn window(cave: [f64; 4], place: &Placement) -> Bounds {
    let (cx, cy) = ((cave[0] + cave[2]) / 2.0, (cave[1] + cave[3]) / 2.0);
    let hw = ((cave[2] - cave[0]).abs() * (1.0 + MARGIN)).max(FLOOR_M) / 2.0;
    let hh = ((cave[3] - cave[1]).abs() * (1.0 + MARGIN)).max(FLOOR_M) / 2.0;
    let mut b = Bounds {
        xmin: f64::MAX,
        ymin: f64::MAX,
        xmax: f64::MIN,
        ymax: f64::MIN,
    };
    for (x, y) in [
        (cx - hw, cy - hh),
        (cx + hw, cy - hh),
        (cx + hw, cy + hh),
        (cx - hw, cy + hh),
    ] {
        let (e, n) = place.cave_to_utm(x, y);
        b.xmin = b.xmin.min(e);
        b.ymin = b.ymin.min(n);
        b.xmax = b.xmax.max(e);
        b.ymax = b.ymax.max(n);
    }
    b
}

/// Pixel size for a window: native where it fits, one shared factor for both axes so the aspect is
/// always the window's; where the cap and the floor clash, the floor yields (old pixelSize bug).
pub fn pixel_size(b: &Bounds, s: Service) -> (u32, u32) {
    let (mut w, mut h) = (b.width() / s.native_m, b.height() / s.native_m);
    let down = 1f64.min(s.max_px / w).min(s.max_px / h);
    w *= down;
    h *= down;
    let up = 1f64
        .max(s.min_px / w)
        .max(s.min_px / h)
        .min(s.max_px / w.max(h));
    w *= up;
    h *= up;
    ((w.round() as u32).max(1), (h.round() as u32).max(1))
}

const NAIP: &str =
    "https://imagery.nationalmap.gov/arcgis/rest/services/USGSNAIPImagery/ImageServer/exportImage";
const DEM: &str =
    "https://elevation.nationalmap.gov/arcgis/rest/services/3DEPElevation/ImageServer/exportImage";

fn query(b: &Bounds, zone: i32, size: (u32, u32)) -> String {
    let sr = epsg(zone);
    format!(
        "bbox={:.3},{:.3},{:.3},{:.3}&bboxSR={sr}&imageSR={sr}&size={},{}",
        b.xmin, b.ymin, b.xmax, b.ymax, size.0, size.1
    )
}

/// The NAIP photograph request (JPEG).
pub fn naip_url(b: &Bounds, zone: i32, size: (u32, u32)) -> String {
    format!("{NAIP}?{}&format=jpg&f=image", query(b, zone, size))
}

/// The 3DEP elevation request (float32 TIFF, metres NAVD88).
pub fn dem_url(b: &Bounds, zone: i32, size: (u32, u32)) -> String {
    format!(
        "{DEM}?{}&format=tiff&pixelType=F32&f=image",
        query(b, zone, size)
    )
}
