//! The tiling must match the old app's `CsSheetTile` on every case in `golden/tile.json`
//! (made by `tools/golden/gen_tile_golden.mjs`).

use cave_sheet::tile::*;
use serde_json::Value;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * (1.0 + b.abs())
}

fn bx(v: &Value) -> BBox {
    BBox {
        min_x: v["minX"].as_f64().unwrap(),
        min_y: v["minY"].as_f64().unwrap(),
        max_x: v["maxX"].as_f64().unwrap(),
        max_y: v["maxY"].as_f64().unwrap(),
    }
}

fn same(a: &BBox, b: &BBox, what: &str) {
    assert!(
        close(a.min_x, b.min_x)
            && close(a.max_x, b.max_x)
            && close(a.min_y, b.min_y)
            && close(a.max_y, b.max_y),
        "{what}: {a:?} vs {b:?}"
    );
}

#[test]
fn ids_and_axes_match() {
    let g: Value = serde_json::from_str(include_str!("golden/tile.json")).unwrap();
    for c in g["ids"].as_array().unwrap() {
        let row = c["row"].as_u64().unwrap() as usize;
        assert_eq!(row_letter(row), c["letter"].as_str().unwrap());
        assert_eq!(id_of(row, row % 7), c["id"].as_str().unwrap());
    }
    for c in g["axes"].as_array().unwrap() {
        let a = axis(
            c["c"].as_f64().unwrap(),
            c["m"].as_f64().unwrap(),
            c["o"].as_f64().unwrap(),
        );
        let o = &c["axis"];
        assert_eq!(a.count as u64, o["count"].as_u64().unwrap(), "{c}");
        assert!(close(a.core_size, o["coreSize"].as_f64().unwrap()), "{c}");
        assert!(close(a.expand, o["expand"].as_f64().unwrap()), "{c}");
        assert!(close(a.span, o["span"].as_f64().unwrap()), "{c}");
    }
}

fn caves() -> Vec<(BBox, Vec<BBox>)> {
    let b = |a, b, c, d| BBox {
        min_x: a,
        min_y: b,
        max_x: c,
        max_y: d,
    };
    vec![
        (b(0., 0., 100., 60.), vec![]),
        (b(0., 0., 4000., 700.), vec![]),
        (b(-500., -300., 900., 1500.), vec![]),
        (
            b(0., 0., 3000., 3000.),
            vec![b(0., 0., 3000., 300.), b(0., 0., 300., 3000.)],
        ),
        (
            b(0., 0., 9000., 4000.),
            vec![b(0., 0., 9000., 200.), b(4000., 0., 4300., 4000.)],
        ),
        (b(0., 0., 1e5, 1e5), vec![]),
        (b(5e5, 3.9e6, 5e5 + 2500., 3.9e6 + 1800.), vec![]),
    ]
}

#[test]
fn layouts_match_the_old_tiling() {
    let g: Value = serde_json::from_str(include_str!("golden/tile.json")).unwrap();
    let caves = caves();
    let mut checked = 0;
    for c in g["layouts"].as_array().unwrap() {
        let (cave, occupied) = &caves[c["case"].as_u64().unwrap() as usize];
        let sheet = c["sheet"].as_array().unwrap();
        let shift = c["shift"].as_array().unwrap();
        let input = TileInput {
            cave: *cave,
            sheet: (sheet[0].as_f64().unwrap(), sheet[1].as_f64().unwrap()),
            scale: c["scale"].as_f64().unwrap(),
            turned: false,
            footer_inches: c["footer"].as_f64().unwrap(),
            overlap_inches: None,
            occupied: occupied.clone(),
            shift_inches: (shift[0].as_f64().unwrap(), shift[1].as_f64().unwrap()),
        };
        let l = layout(&input, c["turned"].as_bool());
        let o = &c["layout"];
        let what = format!(
            "{} {} {} {}",
            c["case"], c["sheet"], c["scale"], c["turned"]
        );
        assert_eq!(l.too_many, o["tooMany"].as_bool().unwrap(), "{what}");
        if l.too_many {
            checked += 1;
            continue;
        }
        assert_eq!(l.tiled, o["tiled"].as_bool().unwrap(), "{what}");
        assert_eq!(l.rows as u64, o["rows"].as_u64().unwrap(), "{what}");
        assert_eq!(l.cols as u64, o["cols"].as_u64().unwrap(), "{what}");
        assert_eq!(l.turned, o["turned"].as_bool().unwrap(), "{what}");
        let ids: Vec<&str> = o["ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let mine: Vec<&str> = l.tiles.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(mine, ids, "{what}");
        assert!(close(l.overlap, o["overlap"].as_f64().unwrap()), "{what}");
        assert!(
            close(l.anchor[0], o["anchor"]["x"].as_f64().unwrap()),
            "{what}"
        );
        assert!(
            close(l.anchor[1], o["anchor"]["y"].as_f64().unwrap()),
            "{what}"
        );
        if o["big"].as_bool().unwrap() {
            checked += 1;
            continue;
        }
        same(&l.span.unwrap(), &bx(&o["span"]), &what);
        for (t, w) in l.tiles.iter().zip(o["tiles"].as_array().unwrap()) {
            assert_eq!(
                (t.title, t.primary),
                (
                    w["title"].as_bool().unwrap(),
                    w["primary"].as_bool().unwrap()
                ),
                "{what} {}",
                t.id
            );
            same(&t.core, &bx(&w["core"]), &what);
            same(&t.map, &bx(&w["map"]), &what);
            assert!(
                close(t.paper.width, w["paper"]["width"].as_f64().unwrap()),
                "{what}"
            );
            assert!(
                close(t.paper.height, w["paper"]["height"].as_f64().unwrap()),
                "{what}"
            );
            assert_eq!(
                t.matches.len(),
                w["matches"].as_array().unwrap().len(),
                "{what} {}",
                t.id
            );
            for (m, wm) in t.matches.iter().zip(w["matches"].as_array().unwrap()) {
                assert_eq!(
                    (m.edge, m.to.as_str()),
                    (wm["edge"].as_str().unwrap(), wm["to"].as_str().unwrap()),
                    "{what}"
                );
            }
        }
        assert_eq!(
            l.match_lines.len(),
            o["matchLines"].as_array().unwrap().len(),
            "{what}"
        );
        checked += 1;
    }
    assert_eq!(checked, 756);
}
