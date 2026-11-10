//! The title block maths must match the old app's `CsSheetSetup` on every case in `golden/title.json`
//! (made by `tools/golden/gen_title_golden.mjs`).

use cave_sheet::title::*;
use serde_json::Value as J;
use std::collections::BTreeMap;

fn strs(v: &J) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn wrapping_matches() {
    let g: J = serde_json::from_str(include_str!("golden/title.json")).unwrap();
    for c in g["wrap"].as_array().unwrap() {
        assert_eq!(
            wrap_text(c["t"].as_str().unwrap(), c["b"].as_u64().unwrap() as usize),
            strs(&c["r"]),
            "{c}"
        );
    }
}

#[test]
fn title_lines_match() {
    let g: J = serde_json::from_str(include_str!("golden/title.json")).unwrap();
    for c in g["lines"].as_array().unwrap() {
        let values: BTreeMap<String, String> = c["v"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
            .collect();
        let mine = title_lines(&values);
        let want = c["r"].as_array().unwrap();
        assert_eq!(mine.len(), want.len(), "{c}");
        for (m, w) in mine.iter().zip(want) {
            assert_eq!(m.text, w["text"].as_str().unwrap());
            assert!((m.inches - w["inches"].as_f64().unwrap()).abs() < 1e-12);
            assert_eq!(m.field_id, w["fieldId"].as_str().unwrap());
        }
    }
}

#[test]
fn names_dates_and_crews_match() {
    let g: J = serde_json::from_str(include_str!("golden/title.json")).unwrap();
    for c in g["dedupe"].as_array().unwrap() {
        assert_eq!(dedupe_names(&strs(&c["n"])), strs(&c["r"]), "{c}");
    }
    for c in g["dates"].as_array().unwrap() {
        assert_eq!(dates_for(&strs(&c["d"])), c["r"].as_str().unwrap(), "{c}");
    }
    for c in g["crew"].as_array().unwrap() {
        assert_eq!(
            surveyed_by_for(&strs(&c["t"])),
            c["r"].as_str().unwrap(),
            "{c}"
        );
    }
}

#[test]
fn linked_fields_follow_until_typed_over() {
    let mut v = Values::new();
    let mut filled = BTreeMap::new();
    filled.insert("caveName".to_string(), "Truitt".to_string());
    filled.insert("length".to_string(), "100 ft".to_string());
    assert_eq!(sync(&mut v, &filled).len(), 2);
    filled.insert("length".to_string(), "120 ft".to_string());
    assert_eq!(sync(&mut v, &filled), vec!["length".to_string()]);
    assert_eq!(v["length"].text, "120 ft");
    edit(&mut v, "length", "about 120 ft");
    filled.insert("length".to_string(), "130 ft".to_string());
    assert!(sync(&mut v, &filled).is_empty());
    assert_eq!(v["length"].text, "about 120 ft");
    // the notebook having no answer never blanks a field
    filled.clear();
    sync(&mut v, &filled);
    assert_eq!(v["caveName"].text, "Truitt");
    relink(&mut v, "length");
    filled.insert("length".to_string(), "130 ft".to_string());
    sync(&mut v, &filled);
    assert_eq!(
        v["length"],
        Value {
            text: "130 ft".into(),
            link: Link::Auto
        }
    );
}

#[test]
fn location_is_never_filled_in() {
    let nb = Notebook {
        name: "X".into(),
        teams: vec!["A B".into()],
        dates: vec!["2024-01-01".into()],
        length: Some("1 m".into()),
        depth: Some("1 m".into()),
        grade: Some("g".into()),
    };
    assert!(!auto_fill(&nb).contains_key("location"));
}
