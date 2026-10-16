//! Magnetic declination from latitude, longitude and date (the IGRF model), and UTM to and from
//! latitude and longitude.
//!
//! A port of the old app's CsGeomag.js, held to it by golden tests. Old survey notes rarely record
//! the declination in force on the day; survey programs compute it from the survey's date and place
//! (Compass's `!A`, Walls' `.REF`, Survex's `*declination auto`), and IGRF covers 1900 to the present
//! from published public-domain coefficients. Its error -- a fraction of a degree over most of the
//! century -- is noise next to a hand compass. Pure and wasm32-safe.
//!
//! The synthesis is the standard one: geodetic to geocentric on the WGS84 ellipsoid, Schmidt
//! semi-normalised Legendre functions by recurrence, coefficients interpolated linearly between
//! 5-year epochs (the published secular variation after the last), and the field vector rotated back
//! to geodetic north, east and down.

mod coeffs;

use coeffs::{EPOCHS, GH};

/// IGRF's reference sphere radius, km.
const EARTH_REFERENCE_RADIUS_KM: f64 = 6371.2;
const WGS84_A_KM: f64 = 6378.137;
const WGS84_B_KM: f64 = 6356.7523142;
const NMAX: usize = 13;

/// The earliest date the model covers.
pub const MIN_YEAR: f64 = 1900.0;

/// A date as a decimal year (month 1-12, day 1-31).
pub fn decimal_year(year: i32, month: u32, day: u32) -> f64 {
    const DAYS: [u32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let m = (month.clamp(1, 12) - 1) as usize;
    let doy = DAYS[m] + day + u32::from(leap && month > 2);
    f64::from(year) + f64::from(doy - 1) / if leap { 366.0 } else { 365.0 }
}

/// A "YYYY-MM-DD" date as a decimal year.
pub fn decimal_year_of(date: &str) -> Option<f64> {
    let mut parts = date.trim().splitn(3, '-');
    let y = parts.next()?.parse().ok()?;
    let m = parts.next()?.parse().ok()?;
    let d: String = parts
        .next()?
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    let d = d.parse().ok()?;
    Some(decimal_year(y, m, d))
}

/// The field at a place and time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Field {
    /// Degrees, east positive (true = magnetic + declination).
    pub declination: f64,
    /// Dip, degrees, down positive.
    pub inclination: f64,
    /// Horizontal intensity, nT.
    pub horizontal_nt: f64,
    /// Total intensity, nT.
    pub total_nt: f64,
}

/// The g/h coefficients at a decimal year, or `None` before 1900.
fn coefficients_at(year: f64) -> Option<Vec<(bool, usize, usize, f64)>> {
    if year < MIN_YEAR {
        return None;
    }
    let last = EPOCHS.len() - 1;
    let (i, frac) = if year >= EPOCHS[last] {
        (last, year - EPOCHS[last])
    } else {
        let i = ((year - EPOCHS[0]) / 5.0).floor() as usize;
        (i, (year - EPOCHS[i]) / (EPOCHS[i + 1] - EPOCHS[i]))
    };
    Some(
        GH.iter()
            .map(|(is_h, n, m, row)| {
                let base = row[i];
                let v = if i == last {
                    base + row[row.len() - 1] * frac
                } else {
                    base + (row[i + 1] - base) * frac
                };
                (*is_h, *n, *m, v)
            })
            .collect(),
    )
}

/// The field at a geodetic latitude and longitude (degrees, north and east positive), a decimal
/// year, and a height above the ellipsoid in km (0 is near enough for a cave). `None` before 1900.
pub fn field(lat_deg: f64, lon_deg: f64, year: f64, alt_km: f64) -> Option<Field> {
    let gh = coefficients_at(year)?;
    let deg = std::f64::consts::PI / 180.0;

    // geodetic -> geocentric
    let colat = (90.0 - lat_deg) * deg;
    let (mut ct, mut st) = (colat.cos(), colat.sin());
    let (a2, b2) = (WGS84_A_KM * WGS84_A_KM, WGS84_B_KM * WGS84_B_KM);
    let one = a2 * st * st;
    let two = b2 * ct * ct;
    let three = one + two;
    let rho = three.sqrt();
    let r = (alt_km * (alt_km + 2.0 * rho) + (a2 * one + b2 * two) / three).sqrt();
    let cd = (alt_km + rho) / r;
    let sd = (a2 - b2) / rho * ct * st / r;
    let ct_old = ct;
    ct = ct * cd - st * sd;
    st = st * cd + ct_old * sd;
    let phi = lon_deg * deg;

    // Schmidt semi-normalised Legendre functions and their colatitude derivatives
    let mut p = [[0.0f64; NMAX + 1]; NMAX + 1];
    let mut dp = [[0.0f64; NMAX + 1]; NMAX + 1];
    p[0][0] = 1.0;
    for n in 1..=NMAX {
        for m in 0..=n {
            let (nf, mf) = (n as f64, m as f64);
            if n == m {
                let k = if n == 1 {
                    1.0
                } else {
                    (1.0 - 1.0 / (2.0 * nf)).sqrt()
                };
                p[n][n] = k * st * p[n - 1][n - 1];
                dp[n][n] = k * (st * dp[n - 1][n - 1] + ct * p[n - 1][n - 1]);
            } else {
                let f1 = (2.0 * nf - 1.0) / (nf * nf - mf * mf).sqrt();
                let f2 = if (n as i64) - 1 - (m as i64) < 0 || n + m < 2 {
                    0.0
                } else {
                    (((nf - 1.0) * (nf - 1.0) - mf * mf) / (nf * nf - mf * mf)).sqrt()
                };
                let (pp2, dpp2) = if n >= 2 && m + 2 <= n {
                    (p[n - 2][m], dp[n - 2][m])
                } else {
                    (0.0, 0.0)
                };
                p[n][m] = f1 * ct * p[n - 1][m] - f2 * pp2;
                dp[n][m] = f1 * (ct * dp[n - 1][m] - st * p[n - 1][m]) - f2 * dpp2;
            }
        }
    }

    let mut g = [[0.0f64; NMAX + 1]; NMAX + 1];
    let mut h = [[0.0f64; NMAX + 1]; NMAX + 1];
    for (is_h, n, m, v) in gh {
        if is_h {
            h[n][m] = v;
        } else {
            g[n][m] = v;
        }
    }

    let a_over_r = EARTH_REFERENCE_RADIUS_KM / r;
    let (mut br, mut bt, mut bp) = (0.0, 0.0, 0.0);
    let mut ratio = a_over_r * a_over_r;
    for n in 1..=NMAX {
        ratio *= a_over_r;
        for m in 0..=n {
            let (cosm, sinm) = ((m as f64 * phi).cos(), (m as f64 * phi).sin());
            let gc = g[n][m] * cosm + h[n][m] * sinm;
            br += (n as f64 + 1.0) * ratio * gc * p[n][m];
            bt -= ratio * gc * dp[n][m];
            if m > 0 {
                let gs = -g[n][m] * sinm + h[n][m] * cosm;
                bp -= ratio * m as f64 * gs * p[n][m] / st;
            }
        }
    }
    let (x, y, z) = (-bt, bp, -br);
    let x_geo = x * cd + z * sd;
    let z_geo = z * cd - x * sd;
    let horiz = x_geo.hypot(y);
    Some(Field {
        declination: y.atan2(x_geo) / deg,
        inclination: z_geo.atan2(horiz) / deg,
        horizontal_nt: horiz,
        total_nt: horiz.hypot(z_geo),
    })
}

/// The declination (degrees, east positive) at a place and decimal year, or `None` before 1900.
pub fn declination(lat_deg: f64, lon_deg: f64, year: f64) -> Option<f64> {
    field(lat_deg, lon_deg, year, 0.0).map(|f| f.declination)
}

// ---- UTM ---------------------------------------------------------------------------------------
//
// Transverse Mercator on the WGS84 ellipsoid (Krüger's series to the third order, good to well
// under a millimetre in the zone). NAD83 differs from WGS84 by about a metre, which changes a
// declination by nothing anyone could measure.

const A: f64 = 6_378_137.0;
const F: f64 = 1.0 / 298.257_223_563;
const K0: f64 = 0.9996;

fn series() -> (f64, [f64; 3], [f64; 3]) {
    let n = F / (2.0 - F);
    let a_big = A / (1.0 + n) * (1.0 + n * n / 4.0 + n.powi(4) / 64.0);
    let alpha = [
        n / 2.0 - 2.0 * n * n / 3.0 + 5.0 * n.powi(3) / 16.0,
        13.0 * n * n / 48.0 - 3.0 * n.powi(3) / 5.0,
        61.0 * n.powi(3) / 240.0,
    ];
    let beta = [
        n / 2.0 - 2.0 * n * n / 3.0 + 37.0 * n.powi(3) / 96.0,
        n * n / 48.0 + n.powi(3) / 15.0,
        17.0 * n.powi(3) / 480.0,
    ];
    (a_big, alpha, beta)
}

/// A UTM position to latitude and longitude (degrees). `zone` 1..=60, negative for the southern
/// hemisphere (as Compass and Survex write it).
pub fn utm_to_latlon(easting: f64, northing: f64, zone: i32) -> (f64, f64) {
    let (a_big, _, beta) = series();
    let south = zone < 0;
    let zone = zone.abs();
    let lon0 = (f64::from(zone) * 6.0 - 183.0).to_radians();
    let xi = (northing - if south { 10_000_000.0 } else { 0.0 }) / (K0 * a_big);
    let eta = (easting - 500_000.0) / (K0 * a_big);
    let (mut xi_p, mut eta_p) = (xi, eta);
    for (j, b) in beta.iter().enumerate() {
        let k = 2.0 * (j as f64 + 1.0);
        xi_p -= b * (k * xi).sin() * (k * eta).cosh();
        eta_p -= b * (k * xi).cos() * (k * eta).sinh();
    }
    let chi = (xi_p.sin() / eta_p.cosh()).asin();
    let e2 = F * (2.0 - F);
    // conformal latitude to geodetic latitude, by iteration
    let mut phi = chi;
    for _ in 0..8 {
        let e_sin = e2.sqrt() * phi.sin();
        phi = 2.0
            * (((1.0 + chi.sin()) / (1.0 - chi.sin())).sqrt()
                * ((1.0 + e_sin) / (1.0 - e_sin)).powf(e2.sqrt() / 2.0))
            .atan()
            - std::f64::consts::FRAC_PI_2;
    }
    let lon = lon0 + eta_p.sinh().atan2(xi_p.cos());
    (phi.to_degrees(), lon.to_degrees())
}

/// Latitude and longitude (degrees) to UTM (easting, northing) in a zone (negative: southern).
pub fn latlon_to_utm(lat: f64, lon: f64, zone: i32) -> (f64, f64) {
    let (a_big, alpha, _) = series();
    let south = zone < 0;
    let lon0 = (f64::from(zone.abs()) * 6.0 - 183.0).to_radians();
    let (phi, dl) = (lat.to_radians(), lon.to_radians() - lon0);
    let e = (F * (2.0 - F)).sqrt();
    let t = (phi.sin().atanh() - e * (e * phi.sin()).atanh()).sinh();
    let xi_p = t.atan2(dl.cos());
    let eta_p = (dl.sin() / (1.0 + t * t).sqrt()).atanh();
    let (mut xi, mut eta) = (xi_p, eta_p);
    for (j, a) in alpha.iter().enumerate() {
        let k = 2.0 * (j as f64 + 1.0);
        xi += a * (k * xi_p).sin() * (k * eta_p).cosh();
        eta += a * (k * xi_p).cos() * (k * eta_p).sinh();
    }
    (
        500_000.0 + K0 * a_big * eta,
        K0 * a_big * xi + if south { 10_000_000.0 } else { 0.0 },
    )
}

/// The grid convergence (degrees) at a place: grid north minus true north, positive when grid
/// north lies east of true north. A true bearing minus this is a grid bearing.
pub fn utm_convergence(lat: f64, lon: f64, zone: i32) -> f64 {
    let lon0 = f64::from(zone.abs()) * 6.0 - 183.0;
    ((lon - lon0).to_radians().tan() * lat.to_radians().sin())
        .atan()
        .to_degrees()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_old_apps_igrf_cases() {
        // tests/js_unit.js GEOMAG_CASES: (lat, lon, y, m, d, declination)
        for (lat, lon, y, m, d, want) in [
            (39.696, -86.309, 1962, 7, 14, 1.1905),
            (39.696, -86.309, 2026, 8, 19, -4.8895),
            (36.13, -86.05, 1975, 3, 1, 0.5240),
            (44.4, 6.6, 2000, 1, 1, -0.1981),
            (-31.9, 116.0, 1990, 6, 15, -2.8094),
            (64.8, -147.7, 2010, 1, 1, 20.9519),
            (37.2, -80.4, 1920, 5, 1, -2.3052),
        ] {
            let got = declination(lat, lon, decimal_year(y, m, d)).unwrap();
            assert!(
                (got - want).abs() < 0.01,
                "{lat},{lon} {y}: {got} vs {want}"
            );
        }
        assert!(
            declination(39.0, -86.0, 1890.0).is_none(),
            "before 1900 is outside the model"
        );
    }

    #[test]
    fn utm_round_trips() {
        for (lat, lon, zone) in [
            (37.162, -86.013, 16),
            (-41.2784, 174.7767, -60),
            (64.8, -147.7, 6),
            (0.5, 3.0, 31),
        ] {
            let (e, n) = latlon_to_utm(lat, lon, zone);
            let (la, lo) = utm_to_latlon(e, n, zone);
            assert!(
                (la - lat).abs() < 1e-8 && (lo - lon).abs() < 1e-8,
                "{lat},{lon}: {la},{lo}"
            );
        }
        // on the central meridian: easting 500 km; at 45 N the northing is 0.9996 of the WGS84
        // meridian arc (4 984 944.38 m)
        let (e, n) = latlon_to_utm(45.0, -87.0, 16);
        assert!(
            (e - 500_000.0).abs() < 1e-6 && (n - 0.9996 * 4_984_944.378).abs() < 0.01,
            "{e},{n}"
        );
        assert!(utm_convergence(45.0, -87.0, 16).abs() < 1e-12);
    }

    #[test]
    fn dates_read() {
        assert_eq!(decimal_year(2024, 1, 1), 2024.0);
        assert!((decimal_year_of("2024-07-10").unwrap() - decimal_year(2024, 7, 10)).abs() < 1e-12);
        assert!(decimal_year_of("").is_none());
    }
}
