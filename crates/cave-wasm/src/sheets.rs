//! Sheets for the browser: the stored sheet definitions, drawing one, and generating a grid of them
//! for a big cave. Thin wrappers; the logic is in `cave-sheet` (maths) and `cave-draw::blobs` (storage).

use crate::{DOC, record_edit};
use cave_doc::Document;
use cave_draw::blobs;
use cave_sheet::sheetset::{self, Change, Context, Gen, SheetDef};
use cave_sheet::tile::{self, BBox, TileInput};
use cave_sheet::title::{self, Notebook, Values};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

const PREFIX: &str = "sheet:";
const GEN_PREFIX: &str = "sheetgen:";

fn all(doc: &Document) -> Vec<SheetDef> {
    blobs::list(doc, PREFIX)
        .into_iter()
        .filter_map(|(_, v)| serde_json::from_str::<SheetDef>(&v).ok())
        .collect()
}

fn with_doc<T>(f: impl FnOnce(&mut Document) -> Result<T, String>) -> Result<T, String> {
    DOC.with(|d| {
        let mut slot = d.borrow_mut();
        let doc = slot.as_mut().ok_or("no cave is open")?;
        f(doc)
    })
}

fn apply(doc: &mut Document, actions: Vec<cave_doc::Action>) -> Result<(), String> {
    if actions.is_empty() {
        return Ok(());
    }
    let edit = doc.edit(actions).map_err(|e| format!("{e:?}"))?;
    record_edit("sheets", &edit);
    Ok(())
}

fn status(r: Result<(), String>) -> String {
    match r {
        Ok(()) => "ok".into(),
        Err(e) => format!("error: {e}"),
    }
}

fn feet_text(m: f64, unit: &str) -> String {
    let (v, u) = if unit == "ft" {
        (m / 0.3048, "FT")
    } else {
        (m, "M")
    };
    let n = v.round() as i64;
    let digits = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    format!("{}{out} {u}", if n < 0 { "-" } else { "" })
}

fn context(doc: &Document) -> Context {
    let s = cave_shelf::summarize(doc);
    let nb = Notebook {
        name: s.name.clone(),
        teams: s.trips.iter().map(|t| t.team.clone()).collect(),
        dates: s.trips.iter().map(|t| t.date.clone()).collect(),
        length: (s.length_m > 0.0).then(|| feet_text(s.length_m, &s.unit)),
        depth: s
            .depth_m
            .filter(|d| *d > 0.0)
            .map(|d| feet_text(d, &s.unit)),
        grade: None,
    };
    Context {
        filled: title::auto_fill(&nb),
        declination: s.trips.iter().rev().find_map(|t| t.declination),
    }
}

/// Every stored sheet, as a JSON array of sheet definitions.
#[wasm_bindgen]
pub fn sheets_list() -> String {
    DOC.with(|d| {
        let v = d.borrow().as_ref().map(all).unwrap_or_default();
        serde_json::to_string(&v).unwrap_or_else(|_| "[]".into())
    })
}

/// Stores one sheet (a JSON sheet definition) as an undoable edit. Returns `ok` or `error: ...`.
#[wasm_bindgen]
pub fn sheet_put(json: &str) -> String {
    status(with_doc(|doc| {
        let def = serde_json::from_str::<SheetDef>(json).map_err(|e| e.to_string())?;
        if def.id.is_empty() || sheetset::paper_size(&def).is_none() {
            return Err("the sheet needs an id and a known paper".into());
        }
        let text = serde_json::to_string(&def).map_err(|e| e.to_string())?;
        let actions = blobs::put(doc, &format!("{PREFIX}{}", def.id), &text);
        apply(doc, actions)
    }))
}

/// Deletes a sheet. Returns `ok` or `error: ...`.
#[wasm_bindgen]
pub fn sheet_remove(id: &str) -> String {
    status(with_doc(|doc| {
        let actions = blobs::remove(doc, &format!("{PREFIX}{id}"));
        apply(doc, actions)
    }))
}

/// A new blank sheet over the cave's middle `(cx, cy)` metres, stored. Returns its id, or `error: ...`.
#[wasm_bindgen]
pub fn sheet_new(
    name: &str,
    paper: &str,
    turned: bool,
    cx: f64,
    cy: f64,
    feet_per_inch: f64,
) -> String {
    let r = with_doc(|doc| {
        let taken: Vec<String> = all(doc).into_iter().map(|s| s.id).collect();
        let id = (1..)
            .map(|n| format!("sheet-{n}"))
            .find(|i| !taken.contains(i))
            .unwrap_or_default();
        let def = sheetset::blank(&id, name, paper, turned, [cx, cy], feet_per_inch)
            .ok_or("unknown paper")?;
        let text = serde_json::to_string(&def).map_err(|e| e.to_string())?;
        let actions = blobs::put(doc, &format!("{PREFIX}{id}"), &text);
        apply(doc, actions)?;
        Ok(id)
    });
    r.unwrap_or_else(|e| format!("error: {e}"))
}

/// A stored sheet drawn: `{w, h, frames, shapes, warnings}` (inches), or `null`. The title block follows the notebook.
#[wasm_bindgen]
pub fn sheet_render(id: &str) -> String {
    DOC.with(|d| {
        let slot = d.borrow();
        let out = slot.as_ref().and_then(|doc| {
            let def = all(doc).into_iter().find(|s| s.id == id)?;
            sheetset::render(&def, &context(doc))
        });
        serde_json::to_string(&out).unwrap_or_else(|_| "null".into())
    })
}

/// What a sheet's title block shows now for each field, as `{field id: text}` (typed words and the notebook's).
#[wasm_bindgen]
pub fn sheet_title_values(id: &str) -> String {
    DOC.with(|d| {
        let slot = d.borrow();
        let out = slot.as_ref().and_then(|doc| {
            let def = all(doc).into_iter().find(|s| s.id == id)?;
            Some(sheetset::effective_values(&def, &context(doc).filled))
        });
        serde_json::to_string(&out).unwrap_or_else(|_| "null".into())
    })
}

/// How a grid of sheets is made; kept in the cave so regenerating repeats it.
#[derive(Clone, Serialize, Deserialize)]
struct GenParams {
    group: String,
    paper: String,
    /// 0 landscape, 1 portrait, 2 whichever needs fewer sheets.
    turned: u8,
    /// Feet of cave per inch; 0 for the most detailed scale that fits one sheet.
    feet_per_inch: f64,
    /// The viewframe slid by hand, inches.
    #[serde(default)]
    shift: [f64; 2],
    title: bool,
    bar: bool,
    north: bool,
    /// A band under the map kept clear, inches.
    #[serde(default)]
    footer: f64,
    #[serde(default)]
    values: Values,
    /// The cave's box in metres, `[minx, miny, maxx, maxy]` (given on every call, not kept).
    #[serde(default, skip_serializing)]
    bounds: [f64; 4],
    /// Boxes the cave is made of (not kept).
    #[serde(default, skip_serializing)]
    occupied: Vec<[f64; 4]>,
}

fn bbox(b: [f64; 4]) -> BBox {
    BBox {
        min_x: b[0],
        min_y: b[1],
        max_x: b[2],
        max_y: b[3],
    }
}

/// The settings a group of sheets was last generated with, or `null`.
#[wasm_bindgen]
pub fn sheet_gen_params(group: &str) -> String {
    DOC.with(|d| {
        let v = d
            .borrow()
            .as_ref()
            .and_then(|doc| blobs::get(doc, &format!("{GEN_PREFIX}{group}")));
        v.unwrap_or_else(|| "null".into())
    })
}

/// Lays the cave over sheets and stores them (see `GenParams`, JSON). Sheets a person changed by hand are
/// kept. Returns `{sheets, description, too_many, kept, scale, turned}` or `error: ...`.
#[wasm_bindgen]
pub fn sheet_generate(params: &str) -> String {
    let r = with_doc(|doc| {
        let p = serde_json::from_str::<GenParams>(params).map_err(|e| e.to_string())?;
        let paper = cave_sheet::papers()
            .into_iter()
            .find(|x| x.name == p.paper)
            .ok_or("unknown paper")?;
        let cave = bbox(p.bounds);
        let force = (p.turned < 2).then_some(p.turned == 1);
        let mut fpi = p.feet_per_inch;
        if fpi <= 0.0 {
            fpi = cave_sheet::fit(
                cave.w() * cave_sheet::FT_PER_M,
                cave.h() * cave_sheet::FT_PER_M,
                &paper,
                p.footer,
                force,
            )
            .scale;
        }
        let layout = tile::layout(
            &TileInput {
                cave,
                sheet: (paper.w, paper.h),
                scale: fpi / cave_sheet::FT_PER_M,
                turned: false,
                footer_inches: p.footer,
                overlap_inches: None,
                occupied: p.occupied.iter().map(|b| bbox(*b)).collect(),
                shift_inches: (p.shift[0], p.shift[1]),
            },
            force,
        );
        if layout.too_many {
            return Ok(
                serde_json::json!({"sheets": 0, "description": tile::describe(&layout), "too_many": true, "kept": [], "scale": fpi, "turned": layout.turned}),
            );
        }
        let made = sheetset::generate(&Gen {
            layout: &layout,
            paper: paper.name,
            feet_per_inch: fpi,
            group: &p.group,
            values: &p.values,
            bar: p.bar,
            north: p.north,
            title: p.title,
            footer_inches: p.footer,
        });
        let count = made.len();
        let (changes, kept) = sheetset::merge(&all(doc), made, &p.group);
        let mut actions = Vec::new();
        for c in changes {
            match c {
                Change::Put(s) => {
                    let text = serde_json::to_string(&s).map_err(|e| e.to_string())?;
                    actions.extend(blobs::put(doc, &format!("{PREFIX}{}", s.id), &text));
                }
                Change::Remove(id) => actions.extend(blobs::remove(doc, &format!("{PREFIX}{id}"))),
            }
        }
        let kept_params = serde_json::to_string(&p).map_err(|e| e.to_string())?;
        actions.extend(blobs::put(
            doc,
            &format!("{GEN_PREFIX}{}", p.group),
            &kept_params,
        ));
        apply(doc, actions)?;
        Ok(
            serde_json::json!({"sheets": count, "description": tile::describe(&layout), "too_many": false, "kept": kept, "scale": fpi, "turned": layout.turned}),
        )
    });
    match r {
        Ok(v) => v.to_string(),
        Err(e) => format!("error: {e}"),
    }
}

/// Sets the title block's words on every sheet of a group (and in the settings it regenerates from).
/// `values` is `{field id: {text, link}}` as JSON. Returns `ok` or `error: ...`.
#[wasm_bindgen]
pub fn sheet_set_values(group: &str, values: &str) -> String {
    status(with_doc(|doc| {
        let v = serde_json::from_str::<Values>(values).map_err(|e| e.to_string())?;
        let mut actions = Vec::new();
        for mut s in all(doc)
            .into_iter()
            .filter(|s| s.auto.as_ref().is_some_and(|a| a.group == group))
        {
            s.values = v.clone();
            let text = serde_json::to_string(&s).map_err(|e| e.to_string())?;
            actions.extend(blobs::put(doc, &format!("{PREFIX}{}", s.id), &text));
        }
        if let Some(raw) = blobs::get(doc, &format!("{GEN_PREFIX}{group}")) {
            if let Ok(mut p) = serde_json::from_str::<serde_json::Value>(&raw) {
                p["values"] = serde_json::to_value(&v).map_err(|e| e.to_string())?;
                actions.extend(blobs::put(
                    doc,
                    &format!("{GEN_PREFIX}{group}"),
                    &p.to_string(),
                ));
            }
        }
        apply(doc, actions)
    }))
}
