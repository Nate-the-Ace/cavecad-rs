//! The port against the old app's CsGeomag.js over a grid of places and dates
//! (tools/golden/gen_geomag_golden.mjs).

#[test]
fn matches_the_old_igrf_everywhere() {
    let path = format!("{}/tests/golden.json", env!("CARGO_MANIFEST_DIR"));
    let cases: Vec<[f64; 7]> =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert!(cases.len() > 500);
    for [lat, lon, year, decl, incl, h, total] in cases {
        let f = cave_geomag::field(lat, lon, year, 0.0).unwrap();
        let close = |a: f64, b: f64| (a - b).abs() <= 1e-9 * b.abs().max(1.0);
        assert!(
            close(f.declination, decl) && close(f.inclination, incl),
            "{lat},{lon} {year}: {f:?} vs {decl} {incl}"
        );
        assert!(
            close(f.horizontal_nt, h) && close(f.total_nt, total),
            "{lat},{lon} {year}: intensity"
        );
    }
}
