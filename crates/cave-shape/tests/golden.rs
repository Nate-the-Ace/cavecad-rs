//! The shaped-line geometry must match what the old app computed for the same spines: stations,
//! hachures, fans, scallop chains, automatic edging and glyph placements.

use cave_fill::Rng;
use cave_shape::{
    Edging, auto_sides, fans, glyph_placements, inward_side, scallop_chain, stations, ticks,
};
use serde_json::Value;

fn pts(v: &Value) -> Vec<[f64; 2]> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()])
        .collect()
}

fn near(a: f64, b: f64, what: &str) {
    assert!((a - b).abs() < 1e-9, "{what}: {a} vs {b}");
}

fn lines(v: &Value) -> Vec<Vec<[f64; 2]>> {
    v.as_array().unwrap().iter().map(pts).collect()
}

fn same_lines(got: &[Vec<[f64; 2]>], want: &[Vec<[f64; 2]>], what: &str) {
    assert_eq!(got.len(), want.len(), "{what}: how many");
    for (i, (g, w)) in got.iter().zip(want).enumerate() {
        assert_eq!(g.len(), w.len());
        for (a, b) in g.iter().zip(w) {
            near(a[0], b[0], &format!("{what} #{i} x"));
            near(a[1], b[1], &format!("{what} #{i} y"));
        }
    }
}

#[test]
fn shaped_lines_match_the_old_app() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("golden/shape.json")).unwrap();
    assert_eq!(cases.len(), 4);
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        let spine = pts(&c["pts"]);
        let closed = c["closed"].as_bool().unwrap();
        let survey = pts(&c["survey"]);

        let st = stations(&spine, closed, 4.0);
        let want = c["stations"].as_array().unwrap();
        assert_eq!(st.len(), want.len(), "{name} stations");
        for (s, w) in st.iter().zip(want) {
            near(s.x, w[0].as_f64().unwrap(), "station x");
            near(s.y, w[1].as_f64().unwrap(), "station y");
            near(s.tx, w[2].as_f64().unwrap(), "tangent x");
            near(s.ty, w[3].as_f64().unwrap(), "tangent y");
        }
        same_lines(
            &ticks(&spine, closed, 4.0, 2.0, 1.0),
            &lines(&c["ticks"]),
            &format!("{name} ticks"),
        );
        same_lines(
            &fans(&spine, closed, 6.0, 3.0, -1.0, 22.0),
            &lines(&c["fans"]),
            &format!("{name} fans"),
        );

        let chain = scallop_chain(&spine, closed, 3.0, 0.5, 1.0);
        let wc = &c["chain"];
        let wp = pts(&wc["points"]);
        assert_eq!(chain.points.len(), wp.len(), "{name} chain points");
        for (g, w) in chain.points.iter().zip(&wp) {
            near(g[0], w[0], "chain x");
            near(g[1], w[1], "chain y");
        }
        let wb: Vec<f64> = wc["bulges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b.as_f64().unwrap())
            .collect();
        assert_eq!(chain.bulges, wb, "{name} bulges");

        let sides = auto_sides(&spine, closed, 8.0, &survey, 1.2);
        let ws = c["sides"].as_array().unwrap();
        assert_eq!(sides.len(), ws.len(), "{name} sides");
        for (i, (g, w)) in sides.iter().zip(ws).enumerate() {
            assert_eq!(
                (g.side, g.skip, g.sure),
                (
                    w[0].as_f64().unwrap(),
                    w[1].as_bool().unwrap(),
                    w[2].as_bool().unwrap()
                ),
                "{name} edging #{i}"
            );
        }

        for gl in c["glyphs"].as_array().unwrap() {
            let seed = gl["seed"].as_u64().unwrap();
            let sides: Vec<Edging> = auto_sides(&spine, closed, 8.0, &survey, 1.2);
            let got = glyph_placements(
                &spine,
                closed,
                8.0,
                1.2,
                &sides,
                0.4,
                20.0_f64.to_radians(),
                0.25,
                &mut Rng::new(seed),
            );
            let want = gl["placed"].as_array().unwrap();
            assert_eq!(got.len(), want.len(), "{name} seed {seed}: how many glyphs");
            for (g, w) in got.iter().zip(want) {
                near(g.x, w[0].as_f64().unwrap(), "glyph x");
                near(g.y, w[1].as_f64().unwrap(), "glyph y");
                near(g.angle, w[2].as_f64().unwrap(), "glyph angle");
                near(g.scale_mul, w[3].as_f64().unwrap(), "glyph size");
            }
        }
        if closed {
            near(
                inward_side(&spine),
                c["inward"].as_f64().unwrap(),
                "inward side",
            );
        }
    }
}
