//! What is drawn in the Map, the Profile and the Cross Section reaches the DXF in its own look:
//! symbols as drawings, text as text, fills with their marks, shaped lines with their ornament, and
//! plain lines as smooth curves. The file still reads back as the same cave.

use cave_doc::{ActorId, Document, Field, Kind, Value};
use cave_dxf::{ExportOptions, export, import};
use cave_id::SeqIds;

fn pitfall() -> Document {
    let path = format!(
        "{}/../cave-survey/tests/golden/PitfallCave.json",
        env!("CARGO_MANIFEST_DIR")
    );
    cave_fixture::from_shot_list(&std::fs::read_to_string(path).unwrap(), ActorId(1)).unwrap()
}

/// A drawing with one of everything in the plan (map) and on the first run's profile.
fn drawn() -> Document {
    let mut doc = pitfall();
    let mut ids = SeqIds::new(500);
    let frames = cave_scene::map_frames(&doc, true);
    let run = cave_scene::runs(&doc, true)[0].key.clone();
    let band = cave_scene::profile_frames(&doc, &run, true).unwrap();
    let at = frames[0].pa;
    let p = |dx: f64, dy: f64| [at[0] + dx, at[1] + dy];
    let sym_line = [p(0.0, 0.0), p(0.0, 2.0)];
    let loop_pts = [p(0.0, 0.0), p(8.0, 0.0), p(8.0, 8.0), p(0.0, 8.0)];
    let wavy = [p(0.0, 0.0), p(3.0, 2.0), p(6.0, -1.0), p(9.0, 1.0)];
    for (kind, pts, text) in [
        ("sym-pit", sym_line.to_vec(), "P 19"),
        ("text", sym_line.to_vec(), "Big room"),
        ("callout", vec![p(0.0, 0.0), p(5.0, 3.0)], "Bad air"),
        ("fill-sand", loop_pts.to_vec(), ""),
        ("ledge", wavy.to_vec(), ""),
        ("wall", wavy.to_vec(), ""),
    ] {
        let (id, actions) = cave_draw::map_line(&doc, &mut ids, kind, &pts, None, &frames).unwrap();
        let actions = cave_draw::with_closed(actions, kind.starts_with("fill-")).unwrap();
        doc.edit(cave_draw::with_text(actions, id, text)).unwrap();
    }
    let (id, actions) = cave_draw::profile_sketch(
        &doc,
        &mut ids,
        "slope",
        &run,
        &[[0.0, 0.0], [5.0, 3.0], [10.0, 0.0]],
        None,
        &band,
    )
    .unwrap();
    doc.edit(actions).unwrap();
    let _ = id;
    // a cross section at the first station, drawn in its own plane
    let first = cave_scene::section_stations(&doc, true, false, &cave_scene::ViewFilter::default())
        [0]
    .id
    .clone();
    let station: cave_id::Id = first.parse().unwrap();
    let trip = cave_scene::section_trip(&doc, true, &first);
    let (_, actions) = cave_draw::section_sketch(
        &doc,
        &mut ids,
        "wall",
        station,
        &[[-1.0, 0.0], [0.0, 1.5], [1.0, 0.0]],
        None,
        trip,
    )
    .unwrap();
    doc.edit(actions).unwrap();
    doc
}

#[test]
fn every_kind_of_drawn_thing_is_written_in_its_own_look() {
    let text = export(&drawn(), &ExportOptions::default());
    for (what, needle) in [
        ("a symbol's drawing, on its layer", "ARCHAEOLOGY"),
        ("the text of a text stroke", "Big room"),
        ("the label of a symbol", "P 19"),
        ("a fill's outline layer", "CAVE-FILL-SAND-MAP"),
        ("a fill's dots as circles", "AcDbCircle"),
        ("a shaped line's layer", "CAVE-LEDGES-MAP"),
        ("a plain line on the map", "CAVE-WALLS-MAP"),
        ("a shaped line on the profile", "CAVE-SLOPES-PROFILE"),
        ("a cross section beside its station", "CAVE-WALLS-SECTION"),
    ] {
        // the layer of a symbol is its catalogue layer plus the view
        let found = if what.starts_with("a symbol's drawing") {
            text.contains("PITS-DOMES-MAP")
        } else {
            text.contains(needle)
        };
        assert!(found, "{what}: no {needle}");
    }
}

#[test]
fn the_drawn_cave_still_reads_back_exactly() {
    let doc = drawn();
    let text = export(&doc, &ExportOptions::default());
    let back = import(&text, ActorId(2))
        .unwrap()
        .document
        .expect("the cave is inside the file");
    assert_eq!(back, doc);
    assert!(back.ids_of(Kind::Feature).len() >= 6);
    let kinds: Vec<String> = back
        .ids_of(Kind::Feature)
        .iter()
        .map(|id| back.entity(*id).unwrap().text(Field::FeatureType))
        .collect();
    assert!(kinds.iter().any(|k| k == "fill-sand") && kinds.iter().any(|k| k == "text"));
    assert!(
        text.contains("CAVE-CALLOUTS-MAP") && text.contains("Bad air"),
        "the callout is exported with its leader"
    );
    let _ = Value::Flag(true);
}

#[test]
fn a_curve_is_written_smooth_not_as_its_fit_points() {
    let doc = drawn();
    let text = export(&doc, &ExportOptions::default());
    // the wavy wall has 4 fit points; written as a curve it has 3 spans of 10 samples
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let count = (0..lines.len())
        .filter(|&i| {
            lines[i] == "LWPOLYLINE"
                && lines[i..(i + 14).min(lines.len())].contains(&"CAVE-WALLS-MAP")
        })
        .find_map(|i| {
            let k = lines[i..].iter().position(|l| *l == "90")?;
            lines[i + k + 1].parse::<usize>().ok()
        })
        .expect("a map wall polyline");
    assert!(count >= 31, "{count} vertices");
}

#[test]
fn exporting_twice_gives_the_same_file() {
    let doc = drawn();
    assert_eq!(
        export(&doc, &ExportOptions::default()),
        export(&doc, &ExportOptions::default())
    );
}

/// A cave with a linetype and a fill of the caver's own, and a wall and an area that use them.
fn with_patterns() -> Document {
    use cave_pattern::{Element, FillDef, Layer, LineDef};
    let mut doc = pitfall();
    let mut ids = SeqIds::new(900);
    let line = LineDef {
        name: "Cave ticks".into(),
        elements: vec![
            Element::Dash { mm: 6.0 },
            Element::Motif {
                motif: "Tick up".into(),
                size_mm: 1.6,
                turn_deg: 0.0,
                along_mm: -3.0,
                across_mm: 0.0,
            },
            Element::Gap { mm: 2.0 },
        ],
    };
    let fill = FillDef {
        name: "Hatchy".into(),
        layers: vec![
            Layer::Hatch {
                angle_deg: 30.0,
                spacing_mm: 2.5,
            },
            Layer::Dots {
                spacing_mm: 2.5,
                dot_mm: 0.4,
            },
        ],
        tint: false,
    };
    let a = cave_draw::patterns::save_line(&doc, &line).unwrap();
    doc.edit(a).unwrap();
    let a = cave_draw::patterns::save_fill(&doc, &fill).unwrap();
    doc.edit(a).unwrap();
    let frames = cave_scene::map_frames(&doc, true);
    let at = frames[0].pa;
    let p = |dx: f64, dy: f64| [at[0] + dx, at[1] + dy];
    for (kind, pts, style) in [
        (
            "wall",
            vec![p(0.0, 0.0), p(30.0, 6.0), p(60.0, -3.0)],
            "pat:Cave ticks",
        ),
        (
            "fill-custom",
            vec![p(0.0, -10.0), p(40.0, -10.0), p(40.0, -50.0), p(0.0, -50.0)],
            "pat:Hatchy",
        ),
    ] {
        let (id, actions) = cave_draw::map_line(&doc, &mut ids, kind, &pts, None, &frames).unwrap();
        let actions = cave_draw::with_closed(actions, kind.starts_with("fill-")).unwrap();
        let actions = cave_draw::with_style(actions, id, style).unwrap();
        doc.edit(actions).unwrap();
    }
    doc
}

#[test]
fn a_linetype_and_a_fill_of_the_cavers_own_are_written_as_their_marks() {
    let plain = export(&pitfall(), &ExportOptions::default());
    let text = export(&with_patterns(), &ExportOptions::default());
    assert!(
        text.contains("CAVE-FILL-CUSTOM"),
        "the custom fill has its own layer"
    );
    // the wall is many short pieces, not one line: far more LWPOLYLINEs than a plain cave
    let count = |t: &str| t.matches("LWPOLYLINE").count();
    assert!(
        count(&text) > count(&plain) + 40,
        "dashes, ticks and hatch lines became geometry"
    );
    assert!(
        text.matches("\nCIRCLE\n").count() > plain.matches("\nCIRCLE\n").count() + 20,
        "the fill's dots became circles"
    );
    // and the cave itself, with its patterns, reads back unchanged
    let back = import(&text, ActorId(2))
        .unwrap()
        .document
        .expect("the cave is inside the file");
    assert_eq!(cave_draw::patterns::own_library(&back).lines.len(), 1);
    assert_eq!(cave_draw::patterns::own_library(&back).fills.len(), 1);
}

#[test]
fn a_turned_pattern_is_written_turned_inside_the_same_outline() {
    let mut doc = with_patterns();
    let fill_id = doc
        .ids_of(cave_doc::Kind::Feature)
        .into_iter()
        .find(|id| doc.entity(*id).is_some_and(|e| e.text(Field::FeatureType) == "fill-custom"))
        .unwrap();
    let before = export(&doc, &ExportOptions::default());
    let actions = cave_draw::set_stroke_style(&doc, fill_id, "pat:Hatchy;1;1;;;;1;45").unwrap();
    doc.edit(actions).unwrap();
    let after = export(&doc, &ExportOptions::default());
    assert_ne!(before, after, "the pattern turned");
    assert_eq!(
        cave_draw::patterns::own_library(&doc).fills.len(),
        1,
        "turning is a property of the area, not of the fill"
    );
}
