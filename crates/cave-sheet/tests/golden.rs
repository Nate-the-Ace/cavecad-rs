//! The sheet maths must match the old app's `CsSheetSetup` on every case in `golden/sheet.json`
//! (made by `tools/golden/gen_sheet_golden.mjs`).

use cave_sheet::*;
use serde_json::Value;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn papers_and_scales_are_the_old_lists() {
    let g: Value = serde_json::from_str(include_str!("golden/sheet.json")).unwrap();
    let ps = papers();
    let gs = g["sheets"].as_array().unwrap();
    assert_eq!(ps.len(), gs.len());
    for (p, o) in ps.iter().zip(gs) {
        assert_eq!(p.name, o["name"].as_str().unwrap());
        assert!(close(p.w, o["w"].as_f64().unwrap()) && close(p.h, o["h"].as_f64().unwrap()));
    }
    let ss = scales();
    let gs = g["scales"].as_array().unwrap();
    assert_eq!(ss.len(), gs.len());
    for (s, o) in ss.iter().zip(gs) {
        assert_eq!(s.label, o["label"].as_str().unwrap());
        assert!(close(s.feet_per_inch, o["feetPerInch"].as_f64().unwrap()));
        assert_eq!(s.metric, o["metric"].as_bool().unwrap());
    }
}

#[test]
fn scale_bars_and_captions_match() {
    let g: Value = serde_json::from_str(include_str!("golden/sheet.json")).unwrap();
    for c in g["bars"].as_array().unwrap() {
        let fpi = c["fpi"].as_f64().unwrap();
        let b = bar_for(fpi);
        let o = &c["bar"];
        assert!(close(b.per_block, o["perBlock"].as_f64().unwrap()), "{c}");
        assert_eq!(f64::from(b.blocks), o["blocks"].as_f64().unwrap(), "{c}");
        assert_eq!(b.unit, o["unit"].as_str().unwrap());
        assert!(close(b.per_block_feet, o["perBlockFeet"].as_f64().unwrap()));
        assert!(close(b.total_feet, o["totalFeet"].as_f64().unwrap()));
        assert_eq!(scale_text(fpi), c["text"].as_str().unwrap());
    }
}

#[test]
fn fit_matches() {
    let g: Value = serde_json::from_str(include_str!("golden/sheet.json")).unwrap();
    let ps = papers();
    for c in g["fits"].as_array().unwrap() {
        let paper = ps
            .iter()
            .find(|p| p.name == c["sheet"].as_str().unwrap())
            .unwrap();
        let force = c["force"].as_bool();
        let f = fit(
            c["w"].as_f64().unwrap(),
            c["h"].as_f64().unwrap(),
            paper,
            c["footer"].as_f64().unwrap(),
            force,
        );
        let o = &c["fit"];
        assert!(close(f.scale, o["scale"].as_f64().unwrap()), "{c}");
        assert_eq!(f.turned, o["turned"].as_bool().unwrap(), "{c}");
        assert_eq!(f.fits, o["fits"].as_bool().unwrap(), "{c}");
    }
}

#[test]
fn furniture_matches() {
    let g: Value = serde_json::from_str(include_str!("golden/sheet.json")).unwrap();
    let ps = papers();
    let wants = Wants {
        title: true,
        bar: true,
        north: true,
    };
    for c in g["furniture"].as_array().unwrap() {
        let paper = ps
            .iter()
            .find(|p| p.name == c["sheet"].as_str().unwrap())
            .unwrap();
        let w = if c["turned"].as_bool().unwrap() {
            paper.h
        } else {
            paper.w
        };
        let f = furniture(w, wants, c["titleHeight"].as_f64().unwrap());
        let o = &c["f"];
        assert!(close(f.footer, o["footer"].as_f64().unwrap()), "{c}");
        assert_eq!(f.rows as f64, o["rows"].as_f64().unwrap(), "{c}");
        for p in &f.pieces {
            let q = &o["pieces"][p.kind];
            assert!(
                close(p.x, q["x"].as_f64().unwrap()) && close(p.y, q["y"].as_f64().unwrap()),
                "{} {c}",
                p.kind
            );
            assert!(
                close(p.w, q["w"].as_f64().unwrap()) && close(p.h, q["h"].as_f64().unwrap()),
                "{} {c}",
                p.kind
            );
        }
    }
}

#[test]
fn nothing_sticks_out_of_the_margin() {
    let wants = Wants {
        title: true,
        bar: true,
        north: true,
    };
    let title = Title {
        name: "Pitfall Cave",
        lines: &["Surveyed by the team".to_string()],
    };
    for paper in papers() {
        for turned in [false, true] {
            let s = layout(&paper, turned, 50.0, wants, &title);
            assert!(
                s.map[2] > s.map[0] && s.map[3] > s.map[1],
                "{} leaves room for a map",
                paper.name
            );
            for sh in &s.shapes {
                if let Shape::Rect { b, .. } = sh {
                    assert!(b[0] <= s.w && b[1] <= s.h);
                }
            }
        }
    }
}
