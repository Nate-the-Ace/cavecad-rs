//! The curve exported and used for ornament must be the one the web app draws.

use cave_geom::spline::smooth;
use serde_json::Value;

#[test]
fn smooth_matches_the_web_apps_spline() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("golden/spline.json")).unwrap();
    assert_eq!(cases.len(), 5);
    for c in &cases {
        let pts = |v: &Value| -> Vec<[f64; 2]> {
            v.as_array()
                .unwrap()
                .iter()
                .map(|p| [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()])
                .collect()
        };
        let name = c["name"].as_str().unwrap();
        let got = smooth(
            &pts(&c["pts"]),
            c["closed"].as_bool().unwrap(),
            c["per"].as_u64().unwrap() as usize,
        );
        let want = pts(&c["out"]);
        assert_eq!(got.len(), want.len(), "{name}: how many samples");
        for (i, (g, w)) in got.iter().zip(&want).enumerate() {
            assert!(
                (g[0] - w[0]).abs() < 1e-9 && (g[1] - w[1]).abs() < 1e-9,
                "{name} sample {i}: {g:?} vs {w:?}"
            );
        }
    }
}
