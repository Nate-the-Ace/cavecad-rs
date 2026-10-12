//! The scatter must place exactly what the old app placed (same seed, same outline): same count,
//! same positions, sizes and turns, in the same order.

use cave_fill::{Fill, Pattern, Rng, scatter};
use serde_json::Value;

fn element_name(old_block: &str) -> &'static str {
    match old_block {
        "SYM_BREAKDOWN" => "sym-breakdown",
        "SYM_BREAKDOWN_B" => "sym-breakdown-b",
        "SYM_BREAKDOWN_C" => "sym-breakdown-c",
        "AREA_PEBBLE" => "pebble",
        "AREA_STIPPLE" => "stipple",
        "AREA_DASH" => "dash",
        other => panic!("unmapped block {other}"),
    }
}

fn fill_of(key: &str) -> &'static Fill {
    let k = match key {
        "BLOCKS" => "fill-blocks",
        "DEBRIS" => "fill-debris",
        "PEBBLES" => "fill-pebbles",
        "SAND" => "fill-sand",
        "GUANO" => "fill-guano",
        other => panic!("{other}"),
    };
    cave_fill::fill(k).unwrap()
}

#[test]
fn scatter_matches_the_old_app() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("golden/scatter.json")).unwrap();
    assert_eq!(cases.len(), 30);
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        let key = c["key"].as_str().unwrap();
        let seed = c["seed"].as_u64().unwrap();
        let poly: Vec<[f64; 2]> = c["pts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()])
            .collect();
        let Pattern::Scatter {
            elements,
            density,
            scale,
            rotate,
            spacing,
        } = fill_of(key).pattern
        else {
            panic!()
        };
        let got = scatter(
            &poly,
            elements,
            density,
            scale,
            rotate,
            spacing,
            &mut Rng::new(seed),
            1.0,
            1.0,
        );
        let label = format!("{name} {key} seed {seed}");
        assert_eq!(
            got.len() as u64,
            c["count"].as_u64().unwrap(),
            "{label}: how many"
        );
        for (i, want) in c["first"].as_array().unwrap().iter().enumerate() {
            let g = &got[i];
            assert!(
                (g.at[0] - want[0].as_f64().unwrap()).abs() < 1e-9,
                "{label} #{i} x"
            );
            assert!(
                (g.at[1] - want[1].as_f64().unwrap()).abs() < 1e-9,
                "{label} #{i} y"
            );
            assert_eq!(
                g.element,
                element_name(want[2].as_str().unwrap()),
                "{label} #{i} element"
            );
            assert!(
                (g.scale - want[3].as_f64().unwrap()).abs() < 1e-9,
                "{label} #{i} scale"
            );
            assert!(
                (g.angle - want[4].as_f64().unwrap()).abs() < 1e-9,
                "{label} #{i} angle"
            );
        }
    }
}
