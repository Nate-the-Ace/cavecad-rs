//! Sheets as data: generating a grid, drawing a sheet, and what regenerating may touch.

use cave_sheet::sheetset::*;
use cave_sheet::tile::{self, BBox, TileInput};
use cave_sheet::title::Values;
use cave_sheet::{MARGIN_INCHES, Shape, papers};

const FT: f64 = 0.3048;

fn grid(cave: BBox, fpi: f64, turned: Option<bool>) -> tile::Layout {
    let paper = papers()
        .into_iter()
        .find(|p| p.name.starts_with("ISO A3"))
        .unwrap();
    tile::layout(
        &TileInput {
            cave,
            sheet: (paper.w, paper.h),
            scale: fpi * FT,
            turned: false,
            footer_inches: 0.0,
            overlap_inches: None,
            occupied: vec![],
            shift_inches: (0.0, 0.0),
        },
        turned,
    )
}

fn a3() -> String {
    papers()
        .into_iter()
        .find(|p| p.name.starts_with("ISO A3"))
        .unwrap()
        .name
        .to_string()
}

fn make(l: &tile::Layout, fpi: f64) -> Vec<SheetDef> {
    let values = Values::new();
    generate(&Gen {
        layout: l,
        paper: &a3(),
        feet_per_inch: fpi,
        group: "plan",
        values: &values,
        bar: true,
        north: true,
        title: true,
        footer_inches: 0.0,
    })
}

#[test]
fn each_sheets_frame_lands_on_its_tiles_map() {
    let l = grid(
        BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 900.0,
            max_y: 400.0,
        },
        50.0,
        Some(false),
    );
    assert!(l.tiled);
    let sheets = make(&l, 50.0);
    assert_eq!(sheets.len(), l.tiles.len());
    for (s, t) in sheets.iter().zip(&l.tiles) {
        let r = render(s, &Context::default()).unwrap();
        let f = &r.frames[0];
        // the tile's map corners land on the frame's corners
        let sw = f.to_paper(t.map.min_x, t.map.min_y);
        let ne = f.to_paper(t.map.max_x, t.map.max_y);
        assert!(
            (sw[0] - f.rect[0]).abs() < 1e-6 && (sw[1] - f.rect[1]).abs() < 1e-6,
            "{}",
            t.id
        );
        assert!(
            (ne[0] - f.rect[2]).abs() < 1e-6 && (ne[1] - f.rect[3]).abs() < 1e-6,
            "{}",
            t.id
        );
    }
    // only the first sheet has the title block; every sheet has a bar and an arrow
    assert_eq!(
        sheets
            .iter()
            .filter(|s| s.pieces.iter().any(|p| p.kind == "title"))
            .count(),
        1
    );
    assert!(
        sheets
            .iter()
            .all(|s| s.pieces.iter().any(|p| p.kind == "bar")
                && s.pieces.iter().any(|p| p.kind == "north"))
    );
}

#[test]
fn neighbours_get_match_lines_with_captions() {
    let l = grid(
        BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 900.0,
            max_y: 400.0,
        },
        50.0,
        Some(false),
    );
    let sheets = make(&l, 50.0);
    let first = render(&sheets[0], &Context::default()).unwrap();
    assert!(first.shapes.iter().any(|s| matches!(s, Shape::Dash { .. })));
    let text = |r: &Rendered| {
        r.shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Text { s, .. } | Shape::Turned { s, .. } => Some(s.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert!(
        text(&first)
            .iter()
            .any(|t| t.starts_with("MATCH LINE - SEE SHEET "))
    );
    assert!(text(&first).contains(&format!("SHEET {}", sheets[0].name)));
}

#[test]
fn a_twisted_frame_turns_the_map_and_the_arrow() {
    let mut s = blank("a", "Plan", &a3(), false, [10.0, 20.0], 50.0).unwrap();
    s.frames[0].twist = 90.0;
    let r = render(&s, &Context::default()).unwrap();
    let f = &r.frames[0];
    // north (up the map) points left on the paper after a quarter turn counter-clockwise
    let o = f.to_paper(10.0, 20.0);
    let n = f.to_paper(10.0, 21.0);
    assert!(n[0] < o[0] - 1e-9 && (n[1] - o[1]).abs() < 1e-9);
    // and the arrow's shaft is horizontal
    let shaft = r.shapes.iter().find_map(|s| match s {
        Shape::Line { a, b } if (a[1] - b[1]).abs() < 1e-9 && (a[0] - b[0]).abs() > 1.0 => Some(()),
        _ => None,
    });
    assert!(shaft.is_some());
}

#[test]
fn declination_adds_a_magnetic_arm() {
    let s = blank("a", "Plan", &a3(), false, [0.0, 0.0], 50.0).unwrap();
    let words = |c: &Context| {
        render(&s, c)
            .unwrap()
            .shapes
            .iter()
            .filter_map(|x| match x {
                Shape::Text { s, .. } => Some(s.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert!(
        !words(&Context::default())
            .iter()
            .any(|t| t.starts_with("MN "))
    );
    let w = words(&Context {
        declination: Some(-4.5),
        ..Context::default()
    });
    assert!(w.contains(&"MN 4.5\u{b0}W".to_string()), "{w:?}");
}

#[test]
fn the_sheet_number_line_is_the_sheets_name_and_linked_fields_follow_the_notebook() {
    let mut s = blank("a", "B2", &a3(), false, [0.0, 0.0], 50.0).unwrap();
    let mut ctx = Context::default();
    ctx.filled.insert("caveName".into(), "Truitt Cave".into());
    let v = effective_values(&s, &ctx.filled);
    assert_eq!(v["sheetNumber"], "B2");
    assert_eq!(v["caveName"], "Truitt Cave");
    cave_sheet::title::edit(&mut s.values, "caveName", "Truitt");
    assert_eq!(effective_values(&s, &ctx.filled)["caveName"], "Truitt");
}

#[test]
fn regenerating_never_touches_a_sheet_a_person_changed() {
    let l = grid(
        BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 900.0,
            max_y: 400.0,
        },
        50.0,
        Some(false),
    );
    let mut have = make(&l, 50.0);
    // nothing to do when nothing changed
    let (ch, kept) = merge(&have, make(&l, 50.0), "plan");
    assert!(ch.is_empty() && kept.is_empty());
    // a hand edit is kept
    have[1].auto.as_mut().unwrap().edited = true;
    have[1].frames[0].twist = 12.0;
    let finer = grid(
        BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 900.0,
            max_y: 400.0,
        },
        30.0,
        Some(false),
    );
    let (ch, kept) = merge(&have, make(&finer, 30.0), "plan");
    assert!(kept.contains(&have[1].id));
    assert!(
        !ch.iter()
            .any(|c| matches!(c, Change::Put(s) if s.id == have[1].id))
    );
    assert!(ch.iter().any(|c| matches!(c, Change::Put(_))));
    // other groups are never removed
    let mut other = have[0].clone();
    other.id = "x-a1".into();
    other.auto.as_mut().unwrap().group = "x".into();
    let (ch, _) = merge(&[other], make(&l, 50.0), "plan");
    assert!(!ch.iter().any(|c| matches!(c, Change::Remove(_))));
}

#[test]
fn a_sheet_survives_storage() {
    let l = grid(
        BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 900.0,
            max_y: 400.0,
        },
        50.0,
        Some(false),
    );
    for s in make(&l, 50.0) {
        let j = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<SheetDef>(&j).unwrap(), s);
    }
    // an old record missing newer fields still reads
    let s: SheetDef = serde_json::from_str(r#"{"id":"a","name":"A","paper":"x"}"#).unwrap();
    assert!(s.band && s.frames.is_empty());
}

#[test]
fn furniture_stays_inside_the_margin_on_every_paper() {
    for p in papers() {
        for turned in [false, true] {
            let mut s = blank("a", "Plan", p.name, turned, [0.0, 0.0], 50.0).unwrap();
            s.values.insert(
                "surveyedBy".into(),
                cave_sheet::title::Value {
                    text: "A B, C D, E F, G H".into(),
                    link: cave_sheet::title::Link::Manual,
                },
            );
            let r = render(&s, &Context::default()).unwrap();
            assert!(
                r.warnings.is_empty(),
                "{} {turned}: {:?}",
                p.name,
                r.warnings
            );
            let m = MARGIN_INCHES;
            assert!(r.frames[0].rect[0] >= m - 1e-9 && r.frames[0].rect[3] <= r.h - m + 1e-9);
        }
    }
}
