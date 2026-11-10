//! Prints the place, declination and grid convergence for a UTM point: `probe E N zone year alt_m`.
fn main() {
    let a: Vec<f64> = std::env::args()
        .skip(1)
        .map(|v| v.parse().unwrap())
        .collect();
    let (lat, lon) = cave_geomag::utm_to_latlon(a[0], a[1], a[2] as i32);
    let f = cave_geomag::field(lat, lon, a[3], a[4] / 1000.0).unwrap();
    println!(
        "lat {lat:.5} lon {lon:.5} decl {:.4} conv {:.4}",
        f.declination,
        cave_geomag::utm_convergence(lat, lon, a[2] as i32)
    );
}
