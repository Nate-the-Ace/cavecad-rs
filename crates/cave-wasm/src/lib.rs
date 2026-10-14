//! The browser's door into the Rust engine: thin `wasm-bindgen` wrappers, no logic of their own.
//!
//! Each function here forwards to a core crate. Keep it that way: behaviour belongs in the
//! `cave-*` crates where it is tested natively; this crate only adapts types for JavaScript.

use cave_doc::{ActorId, Document, History};
use cave_log::{BufferLayer, LogBuffer};
use serde::Serialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::OnceLock;
use tracing_subscriber::layer::SubscriberExt;
use wasm_bindgen::prelude::*;

static CONSOLE: OnceLock<LogBuffer> = OnceLock::new();

thread_local! {
    /// The cave currently open. The browser is single-threaded, so this is the one document.
    static DOC: RefCell<Option<Document>> = const { RefCell::new(None) };
    /// Undo and redo for the person at this browser.
    /// One undo history per panel (`profile`, `section`, `map`), so a panel's Undo and Redo only
    /// touch what was done in that panel.
    static HISTORY: RefCell<HashMap<String, History>> = RefCell::new(HashMap::new());
}

fn console() -> &'static LogBuffer {
    CONSOLE.get_or_init(|| {
        let buf = LogBuffer::new(10_000);
        let sub = tracing_subscriber::registry().with(BufferLayer::new(buf.clone()));
        // Already set only if a host installed its own subscriber first; the buffer still works.
        let _ = tracing::subscriber::set_global_default(sub);
        buf
    })
}

#[derive(Serialize)]
struct EventJs<'a> {
    seq: u64,
    level: &'a str,
    source: &'a str,
    message: &'a str,
}

/// Version of the engine this page is running, shown in the footer so a stale tab is obvious.
#[wasm_bindgen]
pub fn engine_version() -> String {
    format!("cave-wasm {}", env!("CARGO_PKG_VERSION"))
}

/// Starts the console and logs the first line. Safe to call more than once.
#[wasm_bindgen]
pub fn start() {
    let _ = console();
    tracing::info!(source = "app", "engine ready");
}

/// Writes one line to the console from the UI (`level`: debug, detail, info, warn, error).
#[wasm_bindgen]
pub fn log(level: &str, source: &str, message: &str) {
    let _ = console();
    match level {
        "error" => tracing::error!(source = source, "{message}"),
        "warn" => tracing::warn!(source = source, "{message}"),
        "detail" => tracing::debug!(source = source, "{message}"),
        "debug" => tracing::trace!(source = source, "{message}"),
        _ => tracing::info!(source = source, "{message}"),
    }
}

/// Console events with `seq >= since`, as a JSON array the panel appends to its view.
#[wasm_bindgen]
pub fn console_events(since: u64) -> String {
    let events = console().snapshot(since);
    let rows: Vec<EventJs> = events
        .iter()
        .map(|e| EventJs {
            seq: e.seq,
            level: match e.level {
                cave_log::Level::Debug => "debug",
                cave_log::Level::Detail => "detail",
                cave_log::Level::Info => "info",
                cave_log::Level::Warn => "warn",
                cave_log::Level::Error => "error",
            },
            source: &e.source,
            message: &e.message,
        })
        .collect();
    serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_string())
}

/// Empties the console.
#[wasm_bindgen]
pub fn console_clear() {
    console().clear();
}

/// Makes `doc` the open cave. This device edits it under its own actor (so edits made on two
/// devices never carry the same stamp), and what loading it did is not sent anywhere.
fn install(mut doc: Document) {
    doc.set_actor(random_actor());
    doc.take_outbox();
    DOC.with(|d| *d.borrow_mut() = Some(doc));
    HISTORY.with(|h| h.borrow_mut().clear());
}

/// A new editor identity for this device (53 bits, so it survives a trip through JavaScript).
#[cfg(target_arch = "wasm32")]
fn random_actor() -> ActorId {
    let hi = (js_sys::Math::random() * 2_097_152.0) as u64;
    let lo = (js_sys::Math::random() * 4_294_967_296.0) as u64;
    ActorId((hi << 32) | lo | 1)
}

/// Native builds (the tests) have no browser random source and edit alone.
#[cfg(not(target_arch = "wasm32"))]
fn random_actor() -> ActorId {
    ActorId(1)
}

/// Opens a demo cave from shot-list JSON (the synthetic test caves). Returns a one-line summary,
/// or an error message starting with `error:`.
#[wasm_bindgen]
pub fn open_demo(json: &str) -> String {
    let _ = console();
    match cave_fixture::from_shot_list(json, ActorId(1)) {
        Ok(doc) => {
            let scene = cave_scene::build(&doc, true);
            let summary = format!(
                "{} stations, {} legs, {} loop closures",
                scene.stations.len(),
                scene.legs.len(),
                scene.closures.len()
            );
            tracing::info!(source = "import", "opened demo cave: {summary}");
            install(doc);
            summary
        }
        Err(e) => {
            tracing::error!(source = "import", "could not open demo cave: {e}");
            format!("error: {e}")
        }
    }
}

#[derive(Serialize)]
struct OpenResult {
    ok: bool,
    /// One line for the status bar, or the reason nothing was opened.
    message: String,
    /// Notes about the file the person should see.
    warnings: Vec<String>,
}

/// Opens a cave from the text of a DXF file saved by CaveCAD. Nothing changes unless it opens
/// completely. Returns JSON `{ok, message, warnings}`.
#[wasm_bindgen]
pub fn open_dxf(text: &str) -> String {
    let _ = console();
    let result = match cave_dxf::import(text, ActorId(1)) {
        Ok(imp) => {
            let warnings: Vec<String> = imp
                .warnings
                .iter()
                .map(|w| match w {
                    cave_dxf::Warning::NoCaveData => "This drawing has no cave inside it (it was made in another program), so nothing was loaded.".to_string(),
                    cave_dxf::Warning::SavedElsewhere => "This file was saved by another program. The cave inside was used; changes made to the drawing itself are not applied.".to_string(),
                })
                .collect();
            match imp.document {
                Some(doc) => {
                    let scene = cave_scene::build(&doc, true);
                    let message = format!(
                        "{} stations, {} legs, {} loop closures",
                        scene.stations.len(),
                        scene.legs.len(),
                        scene.closures.len()
                    );
                    tracing::info!(source = "file", "opened a DXF: {message}");
                    install(doc);
                    OpenResult {
                        ok: true,
                        message,
                        warnings,
                    }
                }
                None => OpenResult {
                    ok: false,
                    message: warnings.first().cloned().unwrap_or_default(),
                    warnings,
                },
            }
        }
        Err(e) => {
            let message = match e {
                cave_dxf::ImportError::NotDxf => "This file is not a DXF drawing.".to_string(),
                cave_dxf::ImportError::Binary => {
                    "This is a binary DXF; only the text kind can be opened.".to_string()
                }
                cave_dxf::ImportError::Corrupt(why) => {
                    format!("The cave inside this file is damaged ({why}); nothing was opened.")
                }
                cave_dxf::ImportError::NewerVersion { .. } => {
                    "This file was saved by a newer version of CaveCAD; refresh the page to get it."
                        .to_string()
                }
            };
            OpenResult {
                ok: false,
                message,
                warnings: Vec::new(),
            }
        }
    };
    if !result.ok {
        tracing::warn!(
            source = "file",
            "could not open the DXF: {}",
            result.message
        );
    }
    serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string())
}

/// The open cave as DXF text (the whole cave is inside), or empty when nothing is open.
#[wasm_bindgen]
pub fn save_dxf(adjusted: bool) -> String {
    DOC.with(|d| match d.borrow().as_ref() {
        Some(doc) => {
            let text = cave_dxf::export(doc, &cave_dxf::ExportOptions { adjusted });
            tracing::info!(source = "file", "saved a DXF ({} KB)", text.len() / 1024);
            text
        }
        None => String::new(),
    })
}

/// The stations a cross section can be cut at (JSON array), in survey order.
#[wasm_bindgen]
pub fn section_stations(adjusted: bool, fill_gaps: bool, filter: &str) -> String {
    let filter = parse_filter(filter);
    DOC.with(|d| {
        let list = match d.borrow().as_ref() {
            Some(doc) => cave_scene::section_stations(doc, adjusted, fill_gaps, &filter),
            None => Vec::new(),
        };
        serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_string())
    })
}

/// The cross section at one station (an id as text) with its sketches (JSON), or `null`.
#[wasm_bindgen]
pub fn section_at(adjusted: bool, station: &str, fill_gaps: bool, filter: &str) -> String {
    let filter = parse_filter(filter);
    DOC.with(|d| {
        let section = d
            .borrow()
            .as_ref()
            .and_then(|doc| cave_scene::section_at(doc, adjusted, station, fill_gaps, &filter));
        serde_json::to_string(&section).unwrap_or_else(|_| "null".to_string())
    })
}

/// The cross-section sketches to show on the map, next to their stations (JSON array).
#[wasm_bindgen]
pub fn map_sections(adjusted: bool, filter: &str) -> String {
    let filter = parse_filter(filter);
    DOC.with(|d| {
        let list = match d.borrow().as_ref() {
            Some(doc) => cave_scene::map_sections(doc, adjusted, &filter),
            None => Vec::new(),
        };
        serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_string())
    })
}

/// The profile sketches placed along their runs in 3D, for the map (JSON array).
#[wasm_bindgen]
pub fn map_profile_sketches(adjusted: bool) -> String {
    DOC.with(|d| {
        let list = match d.borrow().as_ref() {
            Some(doc) => cave_scene::profile_sketches_3d(doc, adjusted),
            None => Vec::new(),
        };
        serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_string())
    })
}

/// Adds a stroke of `kind` drawn in plan on the map. `points` are `[east, north]` metres. It is
/// anchored to the survey's legs, goes through the one edit path and is undoable. Returns `ok` or
/// `error: ...`.
#[wasm_bindgen]
pub fn add_map_line(
    kind: &str,
    points: &str,
    widths: &str,
    closed: bool,
    adjusted: bool,
    text: &str,
) -> String {
    let Ok(points) = serde_json::from_str::<Vec<[f64; 2]>>(points) else {
        return "error: unreadable stroke".into();
    };
    let widths: Option<Vec<f64>> = if widths.is_empty() {
        None
    } else {
        serde_json::from_str(widths).ok()
    };
    let result = DOC.with(|d| {
        let mut slot = d.borrow_mut();
        let doc = slot.as_mut().ok_or("no cave is open")?;
        let frames = cave_scene::map_frames(doc, adjusted);
        let (id, actions) =
            cave_draw::map_line(doc, &mut random_ids(), kind, &points, widths, &frames)
                .map_err(|e| format!("{e:?}"))?;
        let actions = cave_draw::with_closed(actions, closed).map_err(|e| format!("{e:?}"))?;
        let actions = cave_draw::with_text(actions, id, text);
        let edit = doc.edit(actions).map_err(|e| format!("{e:?}"))?;
        record_edit("map", &edit);
        Ok::<(), String>(())
    });
    match result {
        Ok(()) => {
            tracing::info!(source = "sketch", "added a {kind} line to the map");
            "ok".into()
        }
        Err(e) => format!("error: {e}"),
    }
}

/// Moves the fit points of a line already drawn (`points` in the space it was drawn in: band
/// metres, plan metres or section metres). Profile and map lines are re-anchored to the survey.
/// One undoable edit. Returns `ok` or `error: ...`.
#[wasm_bindgen]
pub fn edit_line(id: &str, points: &str, widths: &str, adjusted: bool) -> String {
    let Ok(points) = serde_json::from_str::<Vec<[f64; 2]>>(points) else {
        return "error: unreadable stroke".into();
    };
    let widths: Option<Vec<f64>> = if widths.is_empty() {
        None
    } else {
        serde_json::from_str(widths).ok()
    };
    let Ok(line) = id.parse::<cave_id::Id>() else {
        return "error: no such line".into();
    };
    let result = DOC.with(|d| {
        let mut slot = d.borrow_mut();
        let doc = slot.as_mut().ok_or("no cave is open")?;
        let e = doc.entity(line).ok_or("no such line")?;
        let scope = e.text(cave_doc::Field::FeatureView);
        let frames = match scope.as_str() {
            "profile" => {
                cave_scene::profile_frames(doc, &e.text(cave_doc::Field::FeatureRun), adjusted)
                    .unwrap_or_default()
            }
            "map" => cave_scene::map_frames(doc, adjusted),
            _ => Vec::new(),
        };
        let actions = cave_draw::edit_stroke(doc, line, &points, widths, &frames)
            .map_err(|e| format!("{e:?}"))?;
        let edit = doc.edit(actions).map_err(|e| format!("{e:?}"))?;
        record_edit(&scope, &edit);
        Ok::<(), String>(())
    });
    match result {
        Ok(()) => "ok".into(),
        Err(e) => format!("error: {e}"),
    }
}

/// Closes or opens a line already drawn (a closed one loops back to its first point). One
/// undoable edit. Returns `ok` or `error: ...`.
#[wasm_bindgen]
pub fn set_line_closed(id: &str, closed: bool) -> String {
    change_line(id, |doc, line| {
        cave_draw::set_stroke_closed(doc, line, closed)
    })
}

/// Which side of a shaped line its ornament lies on. One undoable edit. Returns `ok` or `error: ...`.
#[wasm_bindgen]
pub fn set_line_flip(id: &str, flip: bool) -> String {
    change_line(id, |doc, line| cave_draw::set_stroke_flip(doc, line, flip))
}

/// The ornament of a shaped line: `curve` is `[[x, y], ...]` (the smooth curve, in metres), `flip`
/// puts it on the other side, `id` seeds anything random (wall glyphs) and `survey` is the survey's
/// stations in the curve's own coordinates (wall glyphs go on the side away from them). Returns
/// `{spine, lines}` as JSON: whether the curve itself is drawn, and the ornament polylines (none for
/// a type with no shape).
#[wasm_bindgen]
pub fn ornament(
    kind: &str,
    curve: &str,
    closed: bool,
    flip: bool,
    id: &str,
    survey: &str,
) -> String {
    let pts = serde_json::from_str::<Vec<[f64; 2]>>(curve).unwrap_or_default();
    let survey = serde_json::from_str::<Vec<[f64; 2]>>(survey).unwrap_or_default();
    let spine = cave_shape::shape(kind).is_none_or(|s| !s.hide_spine);
    let dress = cave_shape::Dress {
        flip,
        seed: cave_fill::seed_of(id),
        survey: &survey,
    };
    let lines = cave_shape::ornament(kind, &pts, closed, dress);
    serde_json::json!({"spine": spine, "lines": lines}).to_string()
}

/// The drawing of a callout: dragged from `tip` to `at`, with a note of `chars` characters.
/// `{height, side, note:[x1,y1,x2,y2], leader:[[x,y]..], head:[[x,y]..]}` as JSON.
#[wasm_bindgen]
pub fn callout(tip_x: f64, tip_y: f64, at_x: f64, at_y: f64, chars: usize, curved: bool) -> String {
    let l = cave_shape::callout::layout([tip_x, tip_y], [at_x, at_y], chars, curved);
    serde_json::to_string(&l).unwrap_or_default()
}

/// The papers and plot scales a sheet offers: `{papers:[{name,w,h}], scales:[{label,feet_per_inch,metric}]}`
/// as JSON (inches, feet per inch).
#[wasm_bindgen]
pub fn sheet_options() -> String {
    serde_json::json!({"papers": cave_sheet::papers(), "scales": cave_sheet::scales()}).to_string()
}

/// A sheet laid out for a cave `width` x `height` metres: `paper` is an index into the papers,
/// `turned` 0 landscape / 1 portrait / 2 whichever fits, `feet_per_inch` 0 for the most detailed scale
/// that fits. `lines` is a JSON array of title block lines. Returns `{w, h, map, feet_per_inch, shapes,
/// turned, fits}` as JSON (inches from the paper's lower left corner).
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn sheet_plan(
    width: f64,
    height: f64,
    paper: usize,
    turned: u8,
    feet_per_inch: f64,
    name: &str,
    lines: &str,
    title: bool,
    bar: bool,
    north: bool,
) -> String {
    let papers = cave_sheet::papers();
    let Some(paper) = papers.get(paper).or(papers.first()) else {
        return String::new();
    };
    let lines = serde_json::from_str::<Vec<String>>(lines).unwrap_or_default();
    let words = cave_sheet::Title {
        name,
        lines: &lines,
    };
    let wants = cave_sheet::Wants { title, bar, north };
    let (w_ft, h_ft) = (width * cave_sheet::FT_PER_M, height * cave_sheet::FT_PER_M);
    let footer = |t| cave_sheet::footer_for(paper, t, wants, &words);
    let (mut is_turned, mut fits) = (turned == 1, true);
    let mut scale = feet_per_inch;
    if feet_per_inch <= 0.0 || turned == 2 {
        let land = cave_sheet::fit(w_ft, h_ft, paper, footer(false), Some(false));
        let port = cave_sheet::fit(w_ft, h_ft, paper, footer(true), Some(true));
        let best = if turned == 0 || (turned == 2 && !land.fits && !port.fits) {
            land.clone()
        } else if turned == 1 {
            port.clone()
        } else if land.fits && (!port.fits || land.scale <= port.scale) {
            land.clone()
        } else {
            port.clone()
        };
        if turned == 2 {
            is_turned = best.turned;
        }
        if feet_per_inch <= 0.0 {
            scale = best.scale;
            fits = best.fits;
        }
    }
    let sheet = cave_sheet::layout(paper, is_turned, scale, wants, &words);
    let mut v = serde_json::to_value(&sheet).unwrap_or_default();
    v["turned"] = is_turned.into();
    v["fits"] = fits.into();
    v.to_string()
}

/// Ids of the shaped lines whose ornament is flipped to the left of travel (JSON array).
#[wasm_bindgen]
pub fn flipped_lines() -> String {
    DOC.with(|d| {
        let ids: Vec<String> = d
            .borrow()
            .as_ref()
            .map(|doc| {
                doc.ids_of(cave_doc::Kind::Feature)
                    .into_iter()
                    .filter(|id| {
                        doc.entity(*id)
                            .is_some_and(|e| e.flag(cave_doc::Field::FeatureFlip))
                    })
                    .map(|id| id.to_string())
                    .collect()
            })
            .unwrap_or_default();
        serde_json::to_string(&ids).unwrap_or_else(|_| "[]".into())
    })
}

/// Changes the words of a line already drawn (a symbol's label, or a text stroke's text). One
/// undoable edit. Returns `ok` or `error: ...`.
#[wasm_bindgen]
pub fn set_line_text(id: &str, text: &str) -> String {
    change_line(id, |doc, line| cave_draw::set_stroke_text(doc, line, text))
}

/// Sets how one line is drawn, apart from its type's look: `linetype;scale;density` (`dashed;1.50;1.00`; empty for the
/// type's own). One undoable edit. Returns `ok` or `error: ...`.
#[wasm_bindgen]
pub fn set_line_style(id: &str, style: &str) -> String {
    change_line(id, |doc, line| {
        cave_draw::set_stroke_style(doc, line, style)
    })
}

/// Changes what a line already drawn is (`wall`, `ledge`, ...). One undoable edit. Returns `ok`
/// or `error: ...`.
#[wasm_bindgen]
pub fn set_line_kind(id: &str, kind: &str) -> String {
    change_line(id, |doc, line| cave_draw::set_stroke_kind(doc, line, kind))
}

/// Applies `make` to a drawn line as one edit on the history of the view it was drawn in.
fn change_line(
    id: &str,
    make: impl FnOnce(&Document, cave_id::Id) -> Result<Vec<cave_doc::Action>, cave_draw::DrawError>,
) -> String {
    let Ok(line) = id.parse::<cave_id::Id>() else {
        return "error: no such line".into();
    };
    let result = DOC.with(|d| {
        let mut slot = d.borrow_mut();
        let doc = slot.as_mut().ok_or("no cave is open")?;
        let scope = doc
            .entity(line)
            .map(|e| e.text(cave_doc::Field::FeatureView))
            .unwrap_or_default();
        let actions = make(doc, line).map_err(|e| format!("{e:?}"))?;
        let edit = doc.edit(actions).map_err(|e| format!("{e:?}"))?;
        record_edit(&scope, &edit);
        Ok::<(), String>(())
    });
    match result {
        Ok(()) => "ok".into(),
        Err(e) => format!("error: {e}"),
    }
}

/// What a drawn line is: JSON `{"kind","view","closed"}`, or `null` when `id` is not a drawn line.
#[wasm_bindgen]
pub fn line_info(id: &str) -> String {
    let Ok(line) = id.parse::<cave_id::Id>() else {
        return "null".into();
    };
    DOC.with(|d| {
        let slot = d.borrow();
        let info = slot.as_ref().and_then(|doc| {
            let e = doc.entity(line).filter(|e| e.kind() == cave_doc::Kind::Feature)?;
            let view = e.text(cave_doc::Field::FeatureView);
            if !["profile", "section", "map"].contains(&view.as_str()) {
                return None;
            }
            let closed = match (
                e.get(cave_doc::Field::FeaturePath),
                e.get(cave_doc::Field::FeatureAnchored),
            ) {
                (cave_doc::Value::Path(Some(cave_geom::Path::Spline { closed, .. })), _) => closed,
                (_, cave_doc::Value::Anchored(Some(a))) => a.closed,
                _ => false,
            };
            Some(serde_json::json!({"kind": e.text(cave_doc::Field::FeatureType), "view": view, "closed": closed, "flip": e.flag(cave_doc::Field::FeatureFlip), "text": e.text(cave_doc::Field::FeatureText), "style": e.text(cave_doc::Field::FeatureStyle)}))
        });
        info.map_or_else(|| "null".to_string(), |v| v.to_string())
    })
}

/// Deletes a line drawn in a view. One undoable edit. Returns `ok` or `error: ...`.
#[wasm_bindgen]
pub fn delete_line(id: &str) -> String {
    let Ok(line) = id.parse::<cave_id::Id>() else {
        return "error: no such line".into();
    };
    let result = DOC.with(|d| {
        let mut slot = d.borrow_mut();
        let doc = slot.as_mut().ok_or("no cave is open")?;
        let scope = doc
            .entity(line)
            .map(|e| e.text(cave_doc::Field::FeatureView))
            .unwrap_or_default();
        let actions = cave_draw::remove_stroke(doc, line).map_err(|e| format!("{e:?}"))?;
        let edit = doc.edit(actions).map_err(|e| format!("{e:?}"))?;
        record_edit(&scope, &edit);
        Ok::<(), String>(())
    });
    match result {
        Ok(()) => "ok".into(),
        Err(e) => format!("error: {e}"),
    }
}

/// The linework drawn in plan on the map, lifted to the passage's elevation (JSON array).
#[wasm_bindgen]
pub fn map_lines(adjusted: bool) -> String {
    DOC.with(|d| {
        let list = match d.borrow().as_ref() {
            Some(doc) => cave_scene::map_lines_3d(doc, adjusted),
            None => Vec::new(),
        };
        serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_string())
    })
}

/// The station table (JSON array of rows): one per station, with what kind of place it is, its
/// notes, elevation (never a position) and the team's mark. `keyword` is the word a note must contain
/// to be a lead (empty: "lead").
#[wasm_bindgen]
pub fn station_rows(adjusted: bool, keyword: &str) -> String {
    DOC.with(|d| {
        let rows = d
            .borrow()
            .as_ref()
            .map(|doc| cave_scene::station_rows(doc, adjusted, keyword))
            .unwrap_or_default();
        serde_json::to_string(&rows).unwrap_or_else(|_| "[]".into())
    })
}

/// Puts the team's mark on a station: its status, the team's note and who has it (one undoable edit
/// in the Stations panel's own history). Returns `ok` or `error: ...`.
#[wasm_bindgen]
pub fn set_station_mark(id: &str, status: &str, team: &str, who: &str) -> String {
    let Ok(station) = id.parse::<cave_id::Id>() else {
        return "error: no such station".into();
    };
    let result = DOC.with(|d| {
        let mut slot = d.borrow_mut();
        let doc = slot.as_mut().ok_or("no cave is open")?;
        let actions = cave_draw::set_station_mark(doc, station, status, team, who)
            .map_err(|e| format!("{e:?}"))?;
        if actions.is_empty() {
            return Ok::<(), String>(());
        }
        let edit = doc.edit(actions).map_err(|e| format!("{e:?}"))?;
        record_edit("stations", &edit);
        Ok(())
    });
    match result {
        Ok(()) => "ok".into(),
        Err(e) => format!("error: {e}"),
    }
}

/// Everything the station tab shows about one station (an id as text), as JSON, or `null`.
#[wasm_bindgen]
pub fn station_detail(id: &str, adjusted: bool) -> String {
    let Ok(station) = id.parse::<cave_id::Id>() else {
        return "null".into();
    };
    DOC.with(|d| {
        let detail = d
            .borrow()
            .as_ref()
            .and_then(|doc| cave_scene::station_detail(doc, adjusted, station));
        serde_json::to_string(&detail).unwrap_or_else(|_| "null".into())
    })
}

/// Runs an edit as one undoable edit in the history of `scope`: `build` makes the actions.
fn scoped_edit<E: std::fmt::Debug>(
    scope: &str,
    build: impl FnOnce(&cave_doc::Document) -> Result<Vec<cave_doc::Action>, E>,
) -> String {
    let result = DOC.with(|d| {
        let mut slot = d.borrow_mut();
        let doc = slot.as_mut().ok_or("no cave is open")?;
        let actions = build(doc).map_err(|e| format!("{e:?}"))?;
        if actions.is_empty() {
            return Ok::<(), String>(());
        }
        let edit = doc.edit(actions).map_err(|e| format!("{e:?}"))?;
        record_edit(scope, &edit);
        Ok(())
    });
    match result {
        Ok(()) => "ok".into(),
        Err(e) => format!("error: {e}"),
    }
}

/// Runs a station edit as one undoable edit in the Stations scope: `build` makes the actions.
fn station_edit(
    build: impl FnOnce(&cave_doc::Document) -> Result<Vec<cave_doc::Action>, cave_draw::DrawError>,
) -> String {
    scoped_edit("stations", build)
}

/// Everything the shot tab shows about one shot (an id as text), as JSON, or `null`.
#[wasm_bindgen]
pub fn shot_detail(id: &str) -> String {
    let Ok(shot) = id.parse::<cave_id::Id>() else {
        return "null".into();
    };
    DOC.with(|d| {
        let detail = d
            .borrow()
            .as_ref()
            .and_then(|doc| cave_scene::shot_detail(doc, shot));
        serde_json::to_string(&detail).unwrap_or_else(|_| "null".into())
    })
}

/// Changes what is measured on a shot. `edit` is JSON `{distance, azimuth, inclination, back_azimuth,
/// back_inclination, exclude_from_plot, exclude_from_all, no_adjust}` (metres, degrees, null for not
/// measured). Writes only what changed, as one undoable edit.
#[wasm_bindgen]
pub fn set_shot(id: &str, edit: &str) -> String {
    #[derive(serde::Deserialize)]
    struct E {
        distance: Option<f64>,
        azimuth: Option<f64>,
        inclination: Option<f64>,
        back_azimuth: Option<f64>,
        back_inclination: Option<f64>,
        #[serde(default)]
        exclude_from_plot: bool,
        #[serde(default)]
        exclude_from_all: bool,
        #[serde(default)]
        no_adjust: bool,
    }
    let (Ok(shot), Ok(e)) = (id.parse::<cave_id::Id>(), serde_json::from_str::<E>(edit)) else {
        return "error: bad shot or values".into();
    };
    let edit = cave_draw::ShotEdit {
        distance: e.distance,
        azimuth: e.azimuth,
        inclination: e.inclination,
        back_azimuth: e.back_azimuth,
        back_inclination: e.back_inclination,
        exclude_from_plot: e.exclude_from_plot,
        exclude_from_all: e.exclude_from_all,
        no_adjust: e.no_adjust,
    };
    station_edit(|doc| cave_draw::set_shot(doc, shot, edit))
}

/// Sets a station's passage dimensions (metres; a negative or missing number is not measured unless
/// the open flag is set). `walls` is JSON `{left, right, up, down, left_open, right_open, up_open,
/// down_open}`. Writes only what changed.
#[wasm_bindgen]
pub fn set_station_walls(id: &str, walls: &str) -> String {
    #[derive(serde::Deserialize)]
    struct W {
        left: Option<f64>,
        right: Option<f64>,
        up: Option<f64>,
        down: Option<f64>,
        #[serde(default)]
        left_open: bool,
        #[serde(default)]
        right_open: bool,
        #[serde(default)]
        up_open: bool,
        #[serde(default)]
        down_open: bool,
    }
    let (Ok(station), Ok(w)) = (id.parse::<cave_id::Id>(), serde_json::from_str::<W>(walls)) else {
        return "error: bad station or walls".into();
    };
    let walls = cave_draw::Walls {
        left: w.left,
        right: w.right,
        up: w.up,
        down: w.down,
        left_open: w.left_open,
        right_open: w.right_open,
        up_open: w.up_open,
        down_open: w.down_open,
    };
    station_edit(|doc| cave_draw::set_station_walls(doc, station, walls))
}

/// Changes a note written at a station: `source` is the id the station detail gave for it.
#[wasm_bindgen]
pub fn set_station_note(source: &str, text: &str) -> String {
    let Ok(source) = source.parse::<cave_id::Id>() else {
        return "error: no such note".into();
    };
    station_edit(|doc| cave_draw::set_note(doc, source, text))
}

/// Marks a station as an agreed benchmark, or not.
#[wasm_bindgen]
pub fn set_station_benchmark(id: &str, on: bool) -> String {
    let Ok(station) = id.parse::<cave_id::Id>() else {
        return "error: no such station".into();
    };
    station_edit(|doc| cave_draw::set_station_benchmark(doc, station, on))
}

/// Every point symbol the palette offers, with its drawing (JSON array).
#[wasm_bindgen]
pub fn symbols() -> String {
    serde_json::to_string(cave_symbols::catalogue()).unwrap_or_else(|_| "[]".to_string())
}

/// The kinds of area fill, with what each means (JSON array of `{key, label, help}`).
#[wasm_bindgen]
pub fn fills() -> String {
    let list: Vec<serde_json::Value> = cave_fill::FILLS
        .iter()
        .map(|f| serde_json::json!({"key": f.key, "label": f.label, "help": f.help}))
        .collect();
    serde_json::Value::Array(list).to_string()
}

/// The marks that fill an outline: `kind` is a fill type, `curve` the outline as `[[x, y], ...]` in
/// metres, `id` the stroke's id (it seeds the scatter, so every device draws the same marks).
/// `amount` is how dense (1 as authored, 2 twice as many marks). Returns `{lines, circles, tint}` as JSON.
#[wasm_bindgen]
pub fn fill_marks(kind: &str, curve: &str, id: &str, amount: f64) -> String {
    let pts = serde_json::from_str::<Vec<[f64; 2]>>(curve).unwrap_or_default();
    serde_json::to_string(&cave_fill::marks_with(
        kind,
        &pts,
        cave_fill::seed_of(id),
        amount,
    ))
    .unwrap_or_else(|_| "{}".into())
}

/// How the Draw tab groups the drawing tools (JSON array of `{name, tools}`, each tool a palette style).
#[wasm_bindgen]
pub fn draw_tool_groups() -> String {
    let groups: Vec<serde_json::Value> = cave_style::TOOL_GROUPS
        .iter()
        .map(|(name, tools)| {
            let styles: Vec<&cave_style::Style> =
                tools.iter().map(|k| cave_style::style(k)).collect();
            serde_json::json!({"name": name, "tools": styles})
        })
        .collect();
    serde_json::Value::Array(groups).to_string()
}

/// The drawing tools the Draw tab offers (JSON array of palette styles).
#[wasm_bindgen]
pub fn draw_tools() -> String {
    serde_json::to_string(&cave_style::draw_tools()).unwrap_or_else(|_| "[]".to_string())
}

/// Places a station's section sketches by hand in a view (`map` or `profile`), as an offset from
/// the station in that view's metres. One undoable edit. Returns `ok` or `error: ...`.
#[wasm_bindgen]
pub fn place_section(station: &str, view: &str, a: f64, b: f64, c: f64) -> String {
    let Ok(station_id) = station.parse::<cave_id::Id>() else {
        return "error: no such station".into();
    };
    let result = DOC.with(|d| {
        let mut slot = d.borrow_mut();
        let doc = slot.as_mut().ok_or("no cave is open")?;
        let (_, actions) =
            cave_draw::place_section(doc, &mut random_ids(), station_id, view, [a, b, c])
                .map_err(|e| format!("{e:?}"))?;
        let edit = doc.edit(actions).map_err(|e| format!("{e:?}"))?;
        record_edit(view, &edit);
        Ok::<(), String>(())
    });
    match result {
        Ok(()) => "ok".into(),
        Err(e) => format!("error: {e}"),
    }
}

/// Adds a stroke sketched on the cross section at `station` (an id as text). `points` are
/// `[across, up]` metres in the section's plane; `widths` is pen weight per point or empty. Goes
/// through the one edit path and is undoable. Returns `ok` or `error: ...`.
#[wasm_bindgen]
pub fn add_section_sketch(
    station: &str,
    kind: &str,
    points: &str,
    widths: &str,
    closed: bool,
    adjusted: bool,
    text: &str,
) -> String {
    let Ok(points) = serde_json::from_str::<Vec<[f64; 2]>>(points) else {
        return "error: unreadable stroke".into();
    };
    let widths: Option<Vec<f64>> = if widths.is_empty() {
        None
    } else {
        serde_json::from_str(widths).ok()
    };
    let Ok(station_id) = station.parse::<cave_id::Id>() else {
        return "error: no such station".into();
    };
    let result = DOC.with(|d| {
        let mut slot = d.borrow_mut();
        let doc = slot.as_mut().ok_or("no cave is open")?;
        let trip = cave_scene::section_trip(doc, adjusted, station);
        let (id, actions) = cave_draw::section_sketch(
            doc,
            &mut random_ids(),
            kind,
            station_id,
            &points,
            widths,
            trip,
        )
        .map_err(|e| format!("{e:?}"))?;
        let actions = cave_draw::with_closed(actions, closed).map_err(|e| format!("{e:?}"))?;
        let actions = cave_draw::with_text(actions, id, text);
        let edit = doc.edit(actions).map_err(|e| format!("{e:?}"))?;
        record_edit("section", &edit);
        Ok::<(), String>(())
    });
    match result {
        Ok(()) => {
            tracing::info!(source = "sketch", "added a stroke to a cross section");
            "ok".into()
        }
        Err(e) => format!("error: {e}"),
    }
}

/// The view filter the viewer sends (JSON); anything unreadable hides nothing.
fn parse_filter(json: &str) -> cave_scene::ViewFilter {
    serde_json::from_str(json).unwrap_or_default()
}

/// The open cave as a scene for the map canvas (JSON). Empty scene when nothing is open.
#[wasm_bindgen]
pub fn scene(adjusted: bool, filter: &str) -> String {
    let filter = parse_filter(filter);
    DOC.with(|d| {
        let scene = match d.borrow().as_ref() {
            Some(doc) => cave_scene::build_filtered(doc, adjusted, &filter),
            None => cave_scene::Scene::default(),
        };
        serde_json::to_string(&scene).unwrap_or_else(|_| "{}".to_string())
    })
}

/// The type palette (colour role, weight, linetype per type) as JSON, for the viewer.
#[wasm_bindgen]
pub fn palette() -> String {
    serde_json::to_string(cave_style::PALETTE).unwrap_or_else(|_| "[]".to_string())
}

/// The over/under split of the drawn lines for a camera looking from `(x, y, z)` toward the cave
/// (z up); for a perspective camera `(x, y, z)` is the eye's position instead. Asked again whenever the camera settles, so dashes follow the view.
#[wasm_bindgen]
pub fn shading(adjusted: bool, x: f64, y: f64, z: f64, perspective: bool, filter: &str) -> String {
    let filter = parse_filter(filter);
    DOC.with(|d| {
        let sh = match d.borrow().as_ref() {
            Some(doc) => cave_scene::shading(
                doc,
                adjusted,
                [x, y, z],
                perspective.then_some([x, y, z]),
                &filter,
            ),
            None => cave_scene::Shading::default(),
        };
        serde_json::to_string(&sh).unwrap_or_else(|_| "{}".to_string())
    })
}

/// Random ids for new things, from the browser's own random source.
fn random_ids() -> cave_id::EntropyIds<impl FnMut() -> u64> {
    cave_id::EntropyIds::new(|| {
        let word = || (js_sys::Math::random() * 4_294_967_296.0) as u64;
        (word() << 32) | word()
    })
}

/// Runs of the open cave for the Profile panel's dropdown (JSON array).
#[wasm_bindgen]
pub fn profile_runs(adjusted: bool) -> String {
    DOC.with(|d| {
        let runs = d
            .borrow()
            .as_ref()
            .map(|doc| cave_scene::runs(doc, adjusted))
            .unwrap_or_default();
        serde_json::to_string(&runs).unwrap_or_else(|_| "[]".to_string())
    })
}

/// The band of one run with its sketches (JSON), or `null` if there is no such run.
#[wasm_bindgen]
pub fn profile_band(run: &str, adjusted: bool) -> String {
    DOC.with(|d| {
        let band = d
            .borrow()
            .as_ref()
            .and_then(|doc| cave_scene::profile_scene(doc, run, adjusted));
        serde_json::to_string(&band).unwrap_or_else(|_| "null".to_string())
    })
}

/// Adds a stroke sketched on a run's band. `points` is JSON `[[s, z], ...]`; `widths` is JSON
/// `[w, ...]` (pen pressure) or an empty string; `adjusted` says which positions the band was
/// drawn from. The stroke is anchored to the band's stations. Goes through the document's one edit path and
/// is undoable. Returns `ok` or `error: ...`.
#[wasm_bindgen]
pub fn add_profile_sketch(
    run: &str,
    kind: &str,
    points: &str,
    widths: &str,
    closed: bool,
    adjusted: bool,
    text: &str,
) -> String {
    let Ok(points) = serde_json::from_str::<Vec<[f64; 2]>>(points) else {
        return "error: unreadable stroke".into();
    };
    let widths: Option<Vec<f64>> = if widths.is_empty() {
        None
    } else {
        serde_json::from_str(widths).ok()
    };
    let result = DOC.with(|d| {
        let mut slot = d.borrow_mut();
        let doc = slot.as_mut().ok_or("no cave is open")?;
        let frames = cave_scene::profile_frames(doc, run, adjusted).ok_or("no such run")?;
        let (id, actions) =
            cave_draw::profile_sketch(doc, &mut random_ids(), kind, run, &points, widths, &frames)
                .map_err(|e| format!("{e:?}"))?;
        let actions = cave_draw::with_closed(actions, closed).map_err(|e| format!("{e:?}"))?;
        let actions = cave_draw::with_text(actions, id, text);
        let edit = doc.edit(actions).map_err(|e| format!("{e:?}"))?;
        record_edit("profile", &edit);
        Ok::<(), String>(())
    });
    match result {
        Ok(()) => {
            tracing::info!(source = "sketch", "added a stroke to run {run}");
            "ok".into()
        }
        Err(e) => format!("error: {e}"),
    }
}

/// Puts an edit on a panel's undo history (`profile`, `section` or `map`).
fn record_edit(scope: &str, edit: &cave_doc::Edit) {
    HISTORY.with(|h| {
        h.borrow_mut()
            .entry(scope.to_string())
            .or_default()
            .record(edit)
    });
}

/// Undoes this person's last edit in a panel (`profile`, `section` or `map`). Returns whether
/// anything changed.
#[wasm_bindgen]
pub fn undo(scope: &str) -> bool {
    DOC.with(|d| {
        let mut slot = d.borrow_mut();
        let Some(doc) = slot.as_mut() else {
            return false;
        };
        let done = HISTORY
            .with(|h| {
                h.borrow_mut()
                    .entry(scope.to_string())
                    .or_default()
                    .undo(doc)
            })
            .ok()
            .flatten()
            .is_some();
        if done {
            tracing::info!(source = "edit", "undo in {scope}");
        }
        done
    })
}

/// Redoes the last undone edit in a panel. Returns whether anything changed.
#[wasm_bindgen]
pub fn redo(scope: &str) -> bool {
    DOC.with(|d| {
        let mut slot = d.borrow_mut();
        let Some(doc) = slot.as_mut() else {
            return false;
        };
        let done = HISTORY
            .with(|h| {
                h.borrow_mut()
                    .entry(scope.to_string())
                    .or_default()
                    .redo(doc)
            })
            .ok()
            .flatten()
            .is_some();
        if done {
            tracing::info!(source = "edit", "redo in {scope}");
        }
        done
    })
}

/// Whether a panel's undo and redo have anything to do, as JSON `{"undo":bool,"redo":bool}`.
#[wasm_bindgen]
pub fn edit_state(scope: &str) -> String {
    HISTORY.with(|h| {
        let h = h.borrow();
        let (undo, redo) = h
            .get(scope)
            .map_or((false, false), |h| (h.can_undo(), h.can_redo()));
        format!("{{\"undo\":{undo},\"redo\":{redo}}}")
    })
}

/// The passage surface as flat triangle buffers for the 3D view. Every triangle has its own three
/// vertices, so no index buffer is needed (vertices are drawn in order).
#[wasm_bindgen]
pub struct PassageMesh {
    positions: Vec<f32>,
    normals: Vec<f32>,
    colors: Vec<f32>,
    filled: usize,
}

#[wasm_bindgen]
impl PassageMesh {
    /// x, y, z per vertex.
    #[wasm_bindgen(getter)]
    pub fn positions(&self) -> Vec<f32> {
        self.positions.clone()
    }

    /// Unit normal per vertex.
    #[wasm_bindgen(getter)]
    pub fn normals(&self) -> Vec<f32> {
        self.normals.clone()
    }

    /// Red, green, blue per vertex.
    #[wasm_bindgen(getter)]
    pub fn colors(&self) -> Vec<f32> {
        self.colors.clone()
    }

    /// How many unmeasured stations were given an inferred (paler) section.
    #[wasm_bindgen(getter)]
    pub fn filled(&self) -> usize {
        self.filled
    }
}

/// Builds the open cave's passage mesh. `mode` is `depth` or `trip`; `fill_gaps` draws a paler
/// inferred section through stations with no measurements.
#[wasm_bindgen]
pub fn passage_mesh(adjusted: bool, mode: &str, fill_gaps: bool, filter: &str) -> PassageMesh {
    let filter = parse_filter(filter);
    let color = if mode == "depth" {
        cave_scene::ColorMode::Depth
    } else {
        cave_scene::ColorMode::Trip
    };
    let mesh = DOC.with(|d| {
        d.borrow()
            .as_ref()
            .map(|doc| cave_scene::passage_mesh(doc, adjusted, color, fill_gaps, &filter))
            .unwrap_or_default()
    });
    let f32s = |v: &[f64]| v.iter().map(|x| *x as f32).collect::<Vec<f32>>();
    PassageMesh {
        positions: f32s(&mesh.positions),
        normals: f32s(&mesh.normals),
        colors: f32s(&mesh.colors),
        filled: mesh.filled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAVE: &str = r#"{"unit":"m","name":"t","shots":[
        {"from":"A","to":"B","distance":10,"azimuth":0,"inclination":0,"left":1,"right":1},
        {"from":"B","to":"C","distance":8,"azimuth":90,"inclination":-5}]}"#;

    fn open_result(text: &str) -> serde_json::Value {
        serde_json::from_str(&open_dxf(text)).unwrap()
    }

    #[test]
    fn a_saved_dxf_opens_as_the_same_cave() {
        assert!(!open_demo(CAVE).starts_with("error"));
        let before = scene(false, "{}");
        let file = save_dxf(true);
        assert!(file.contains("SECTION"), "it is a DXF");
        assert!(open_demo(r#"{"unit":"m","name":"other","shots":[]}"#).contains("0 stations"));
        let r = open_result(&file);
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(scene(false, "{}"), before);
    }

    #[test]
    fn a_bad_file_changes_nothing() {
        open_demo(CAVE);
        let before = scene(false, "{}");
        for junk in [
            "",
            "not a dxf at all",
            "0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n",
        ] {
            let r = open_result(junk);
            assert_eq!(r["ok"], false, "{junk:?}");
            assert!(!r["message"].as_str().unwrap().is_empty());
        }
        assert_eq!(scene(false, "{}"), before, "the open cave is untouched");
    }

    #[test]
    fn stations_and_sections_come_back_as_json() {
        // (Adding a sketch draws ids from the browser's random source, so it is tested in the
        // crates under it; here only the reading side.)
        open_demo(CAVE);
        let list: serde_json::Value =
            serde_json::from_str(&section_stations(false, true, "{}")).unwrap();
        let b = list
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["label"] == "B")
            .unwrap();
        let id = b["id"].as_str().unwrap().to_string();
        let sec: serde_json::Value =
            serde_json::from_str(&section_at(false, &id, true, "{}")).unwrap();
        assert_eq!(sec["label"], "B");
        assert!(sec["sketches"].as_array().unwrap().is_empty());
        assert_eq!(section_at(false, "nope", true, "{}"), "null");
        assert!(
            add_section_sketch("nope", "wall", "[[0,0],[1,1]]", "", false, false, "")
                .starts_with("error")
        );
        assert!(
            add_section_sketch(&id, "wall", "garbage", "", false, false, "").starts_with("error")
        );
    }
}

/// A number that goes up whenever the open cave really changes (an edit, undo, or an op from
/// another device that was news). The app compares it with its value at the last open or save to
/// know whether anything is unsaved. Zero when no cave is open.
#[wasm_bindgen]
pub fn revision() -> f64 {
    DOC.with(|d| d.borrow().as_ref().map_or(0.0, |doc| doc.revision() as f64))
}

/// Local edits made since the last call, as JSON ops (`[]` when there are none). Includes undo
/// and redo. A sync layer sends these to the other devices.
#[wasm_bindgen]
pub fn sync_outbox() -> String {
    DOC.with(|d| {
        let ops = d
            .borrow_mut()
            .as_mut()
            .map(Document::take_outbox)
            .unwrap_or_default();
        serde_json::to_string(&ops).unwrap_or_else(|_| "[]".into())
    })
}

/// Applies ops made on another device. Returns how many changed anything it could apply; ones
/// that make no sense are skipped, never half-applied.
#[wasm_bindgen]
pub fn sync_apply(ops: &str) -> u32 {
    let Ok(ops) = serde_json::from_str::<Vec<cave_doc::Op>>(ops) else {
        return 0;
    };
    DOC.with(|d| {
        let mut slot = d.borrow_mut();
        let Some(doc) = slot.as_mut() else {
            return 0;
        };
        ops.iter().filter(|op| doc.apply(op).is_ok()).count() as u32
    })
}

/// The whole open cave as JSON ops, to hand to a device that is joining.
#[wasm_bindgen]
pub fn sync_snapshot() -> String {
    DOC.with(|d| {
        let ops = d
            .borrow()
            .as_ref()
            .map(Document::snapshot)
            .unwrap_or_default();
        serde_json::to_string(&ops).unwrap_or_else(|_| "[]".into())
    })
}

/// Replaces the open cave with one rebuilt from a snapshot (or a room's whole log). Returns
/// `ok` or `error: ...`; on an error the open cave is left alone.
#[wasm_bindgen]
pub fn sync_load(ops: &str) -> String {
    let Ok(ops) = serde_json::from_str::<Vec<cave_doc::Op>>(ops) else {
        return "error: unreadable cave from the room".into();
    };
    let mut doc = Document::new(ActorId(1));
    for op in &ops {
        if doc.apply(op).is_err() {
            return "error: the room's cave does not fit this version of CaveCAD".into();
        }
    }
    install(doc);
    "ok".into()
}

// ---- Survey Notebook ------------------------------------------------------------------------

fn parse_id(id: &str) -> Option<cave_id::Id> {
    id.parse().ok()
}

/// The trips, for the notebook's page chooser (JSON array).
#[wasm_bindgen]
pub fn notebook_trips() -> String {
    DOC.with(|d| {
        let v = d
            .borrow()
            .as_ref()
            .map(cave_notebook::trips)
            .unwrap_or_default();
        serde_json::to_string(&v).unwrap_or_else(|_| "[]".into())
    })
}

/// One trip's notebook page (JSON), or `null` when there is no such trip.
#[wasm_bindgen]
pub fn notebook_page(trip: &str) -> String {
    DOC.with(|d| {
        let p = parse_id(trip)
            .zip(d.borrow().as_ref())
            .and_then(|(t, doc)| cave_notebook::page(doc, t));
        serde_json::to_string(&p).unwrap_or_else(|_| "null".into())
    })
}

/// Sets one reading of a shot. `which` is `distance`, `az_fs`, `az_bs`, `inc_fs` or `inc_bs`;
/// `value` is metres or degrees (compass cells are magnetic), empty for not measured.
#[wasm_bindgen]
pub fn notebook_set_reading(shot: &str, which: &str, value: &str) -> String {
    use cave_notebook::Reading;
    let which = match which {
        "distance" => Reading::Distance,
        "az_fs" => Reading::AzimuthFs,
        "az_bs" => Reading::AzimuthBs,
        "inc_fs" => Reading::InclinationFs,
        "inc_bs" => Reading::InclinationBs,
        _ => return "error: unknown reading".into(),
    };
    let value = if value.trim().is_empty() {
        None
    } else {
        match value.trim().parse::<f64>() {
            Ok(v) => Some(v),
            Err(_) => return "error: not a number".into(),
        }
    };
    let Some(shot) = parse_id(shot) else {
        return "error: no such shot".into();
    };
    scoped_edit("notebook", |doc| {
        cave_notebook::set_reading(doc, shot, which, value)
    })
}

/// Sets the dimensions beside a station line: `target` is the arriving shot or the station;
/// `walls` is JSON as for `set_station_walls`.
#[wasm_bindgen]
pub fn notebook_set_walls(target: &str, walls: &str) -> String {
    #[derive(serde::Deserialize)]
    struct W {
        left: Option<f64>,
        right: Option<f64>,
        up: Option<f64>,
        down: Option<f64>,
        #[serde(default)]
        left_open: bool,
        #[serde(default)]
        right_open: bool,
        #[serde(default)]
        up_open: bool,
        #[serde(default)]
        down_open: bool,
    }
    let (Some(target), Ok(w)) = (parse_id(target), serde_json::from_str::<W>(walls)) else {
        return "error: bad target or walls".into();
    };
    let walls = cave_draw::Walls {
        left: w.left,
        right: w.right,
        up: w.up,
        down: w.down,
        left_open: w.left_open,
        right_open: w.right_open,
        up_open: w.up_open,
        down_open: w.down_open,
    };
    scoped_edit("notebook", |doc| {
        cave_notebook::set_walls(doc, target, walls)
    })
}

/// Sets the note on a notebook line (`source` is the id the page gave for it).
#[wasm_bindgen]
pub fn notebook_set_note(source: &str, text: &str) -> String {
    let Some(source) = parse_id(source) else {
        return "error: no such note".into();
    };
    scoped_edit("notebook", |doc| {
        cave_notebook::set_notes(doc, source, text)
    })
}

/// Sets a header field of a trip: `name`, `date`, `team` or `instruments`.
#[wasm_bindgen]
pub fn notebook_set_trip_field(trip: &str, field: &str, text: &str) -> String {
    use cave_notebook::TripField;
    let field = match field {
        "name" => TripField::Name,
        "date" => TripField::Date,
        "team" => TripField::Team,
        "instruments" => TripField::Instruments,
        _ => return "error: unknown field".into(),
    };
    let Some(trip) = parse_id(trip) else {
        return "error: no such trip".into();
    };
    scoped_edit("notebook", |doc| {
        cave_notebook::set_trip_field(doc, trip, field, text)
    })
}

/// Changes a trip's declination; every bearing in the trip turns by the difference.
#[wasm_bindgen]
pub fn notebook_set_declination(trip: &str, degrees: f64) -> String {
    let Some(trip) = parse_id(trip) else {
        return "error: no such trip".into();
    };
    scoped_edit("notebook", |doc| {
        cave_notebook::set_trip_declination(doc, trip, degrees)
    })
}

/// Adds a shot at the end of a trip's page; `to` is empty for a splay. Returns `ok` or `error: ...`.
#[wasm_bindgen]
pub fn notebook_add_shot(trip: &str, from: &str, to: &str, splay: bool) -> String {
    let Some(trip) = parse_id(trip) else {
        return "error: no such trip".into();
    };
    scoped_edit("notebook", |doc| {
        cave_notebook::add_shot(doc, &mut random_ids(), trip, from, to, splay).map(|r| r.1)
    })
}

/// Takes a shot off the page.
#[wasm_bindgen]
pub fn notebook_remove_shot(shot: &str) -> String {
    let Some(shot) = parse_id(shot) else {
        return "error: no such shot".into();
    };
    scoped_edit("notebook", |doc| cave_notebook::remove_shot(doc, shot))
}

/// Renames a station.
#[wasm_bindgen]
pub fn notebook_rename_station(station: &str, name: &str) -> String {
    let Some(station) = parse_id(station) else {
        return "error: no such station".into();
    };
    scoped_edit("notebook", |doc| {
        cave_notebook::rename_station(doc, station, name)
    })
}

/// Starts a new trip. Returns its id, or `error: ...`.
#[wasm_bindgen]
pub fn notebook_new_trip(name: &str) -> String {
    let mut made = None;
    let r = scoped_edit("notebook", |doc| {
        cave_notebook::new_trip(doc, &mut random_ids(), name).map(|(id, a)| {
            made = Some(id);
            a
        })
    });
    match (r.as_str(), made) {
        ("ok", Some(id)) => id.to_string(),
        _ => r,
    }
}

// ---- Scans -----------------------------------------------------------------------------------

/// Every scan linked to a trip, as JSON `[{trip, path}]`.
#[wasm_bindgen]
pub fn scan_links() -> String {
    DOC.with(|d| {
        let v = d
            .borrow()
            .as_ref()
            .map(cave_scans::links)
            .unwrap_or_default();
        serde_json::to_string(&v).unwrap_or_else(|_| "[]".into())
    })
}

/// Links a scan (a path relative to the scans folder) to a trip. One undoable edit in the Scans history.
#[wasm_bindgen]
pub fn scan_link(trip: &str, path: &str) -> String {
    let Some(trip) = parse_id(trip) else {
        return "error: no such trip".into();
    };
    scoped_edit("scans", |doc| cave_scans::link(doc, trip, path))
}

/// Takes a scan off a trip.
#[wasm_bindgen]
pub fn scan_unlink(trip: &str, path: &str) -> String {
    let Some(trip) = parse_id(trip) else {
        return "error: no such trip".into();
    };
    scoped_edit("scans", |doc| cave_scans::unlink(doc, trip, path))
}

/// The stations a view shows, with where each is in that view's own frame (`map`: east, north;
/// `profile:<run>`: along the run, elevation; `section:<station id>`: the station at the origin).
fn view_positions(
    doc: &cave_doc::Document,
    view: &str,
    adjusted: bool,
) -> Vec<(cave_id::Id, String, (f64, f64))> {
    if let Some(run) = view.strip_prefix("profile:") {
        let by_label: std::collections::HashMap<String, cave_id::Id> = doc
            .ids_of(cave_doc::Kind::Station)
            .into_iter()
            .filter_map(|id| Some((doc.entity(id)?.text(cave_doc::Field::StationLabel), id)))
            .collect();
        return cave_scene::profile_scene(doc, run, adjusted)
            .map(|band| {
                band.stations
                    .iter()
                    .filter_map(|s| Some((*by_label.get(&s.label)?, s.label.clone(), (s.s, s.z))))
                    .collect()
            })
            .unwrap_or_default();
    }
    if let Some(station) = view.strip_prefix("section:") {
        return station
            .parse::<cave_id::Id>()
            .ok()
            .and_then(|id| {
                Some((
                    id,
                    doc.entity(id)?.text(cave_doc::Field::StationLabel),
                    (0.0, 0.0),
                ))
            })
            .into_iter()
            .collect();
    }
    cave_scene::build(doc, adjusted)
        .stations
        .iter()
        .filter_map(|s| Some((s.id.parse().ok()?, s.label.clone(), (s.at[0], s.at[1]))))
        .collect()
}

/// Where each scan placed in `view` sits now (JSON `[{trip, path, matrix, used, rms, mode, trim}]`),
/// from the stations' positions in that view (adjusted when asked).
#[wasm_bindgen]
pub fn scan_placements(view: &str, adjusted: bool) -> String {
    DOC.with(|d| {
        let slot = d.borrow();
        let Some(doc) = slot.as_ref() else {
            return "[]".into();
        };
        let positions: cave_scans::Positions = view_positions(doc, view, adjusted)
            .into_iter()
            .map(|(id, _, at)| (id, at))
            .collect();
        serde_json::to_string(&cave_scans::placements(doc, view, &positions))
            .unwrap_or_else(|_| "[]".into())
    })
}

/// The stations a view shows, as JSON `[{id, label}]`: what a spot on a scan can be matched to there.
#[wasm_bindgen]
pub fn scan_view_stations(view: &str, adjusted: bool) -> String {
    #[derive(Serialize)]
    struct S {
        id: String,
        label: String,
    }
    DOC.with(|d| {
        let v: Vec<S> = d
            .borrow()
            .as_ref()
            .map(|doc| {
                view_positions(doc, view, adjusted)
                    .into_iter()
                    .map(|(id, label, _)| S {
                        id: id.to_string(),
                        label,
                    })
                    .collect()
            })
            .unwrap_or_default();
        serde_json::to_string(&v).unwrap_or_else(|_| "[]".into())
    })
}

/// A scan's matches on a trip in a view: JSON `{mode, anchors: [{px, py, station, label, x, y}]}`,
/// or `null`. A match to a station has its `station` id and `label`; a match to a point has `x`, `y`.
#[wasm_bindgen]
pub fn scan_alignment(trip: &str, view: &str, path: &str) -> String {
    #[derive(Serialize)]
    struct A {
        px: f64,
        py: f64,
        station: String,
        label: String,
        x: Option<f64>,
        y: Option<f64>,
    }
    #[derive(Serialize)]
    struct Out {
        mode: cave_scans::FitMode,
        anchors: Vec<A>,
    }
    DOC.with(|d| {
        let slot = d.borrow();
        let out = parse_id(trip).zip(slot.as_ref()).and_then(|(t, doc)| {
            cave_scans::alignment(doc, t, view, path).map(|al| Out {
                mode: al.mode,
                anchors: al
                    .anchors
                    .iter()
                    .map(|a| match a.target {
                        cave_scans::Target::Station { id } => A {
                            px: a.px,
                            py: a.py,
                            station: id.to_string(),
                            label: doc
                                .entity(id)
                                .map(|e| e.text(cave_doc::Field::StationLabel))
                                .unwrap_or_default(),
                            x: None,
                            y: None,
                        },
                        cave_scans::Target::Point { x, y } => A {
                            px: a.px,
                            py: a.py,
                            station: String::new(),
                            label: String::new(),
                            x: Some(x),
                            y: Some(y),
                        },
                    })
                    .collect(),
            })
        });
        serde_json::to_string(&out).unwrap_or_else(|_| "null".into())
    })
}

/// Matches a spot on a scan (pixels from its top left) with what it is in `view`: a station id, or a
/// point written `x:y` (metres in the view's frame).
#[wasm_bindgen]
pub fn scan_add_anchor(
    trip: &str,
    view: &str,
    path: &str,
    px: f64,
    py: f64,
    target: &str,
) -> String {
    let Some(trip) = parse_id(trip) else {
        return "error: no such trip".into();
    };
    let target = match target.split_once(':') {
        Some((x, y)) => match (x.parse::<f64>(), y.parse::<f64>()) {
            (Ok(x), Ok(y)) => cave_scans::Target::Point { x, y },
            _ => return "error: not a point".into(),
        },
        None => match parse_id(target) {
            Some(id) => cave_scans::Target::Station { id },
            None => return "error: no such station".into(),
        },
    };
    scoped_edit("scans", |doc| {
        cave_scans::add_anchor(doc, trip, view, path, px, py, target)
    })
}

/// Takes away a scan's match (by its place in the list).
#[wasm_bindgen]
pub fn scan_remove_anchor(trip: &str, view: &str, path: &str, index: usize) -> String {
    let Some(trip) = parse_id(trip) else {
        return "error: no such trip".into();
    };
    scoped_edit("scans", |doc| {
        cave_scans::remove_anchor(doc, trip, view, path, index)
    })
}

/// Chooses how a scan's matches are fitted: `rigid`, `warp` (stretch and skew) or `rubber` (a rubber sheet).
#[wasm_bindgen]
pub fn scan_set_mode(trip: &str, view: &str, path: &str, mode: &str) -> String {
    let Some(trip) = parse_id(trip) else {
        return "error: no such trip".into();
    };
    let mode = match mode {
        "rigid" => cave_scans::FitMode::Rigid,
        "rubber" => cave_scans::FitMode::Rubber,
        _ => cave_scans::FitMode::Warp,
    };
    scoped_edit("scans", |doc| {
        cave_scans::set_mode(doc, trip, view, path, mode)
    })
}

/// The outline a scan is trimmed to on a trip in a view: JSON `[[px, py], ...]`, or `null`.
#[wasm_bindgen]
pub fn scan_trim(trip: &str, view: &str, path: &str) -> String {
    DOC.with(|d| {
        let out = parse_id(trip)
            .zip(d.borrow().as_ref())
            .and_then(|(t, doc)| cave_scans::trim(doc, t, view, path));
        serde_json::to_string(&out).unwrap_or_else(|_| "null".into())
    })
}

/// Trims a scan to an outline in a view (JSON `[[px, py], ...]`, scan pixels), thinned first so a
/// hand-traced loop stays within `tolerance` pixels of what was drawn. An empty outline takes the trim off.
#[wasm_bindgen]
pub fn scan_set_trim(trip: &str, view: &str, path: &str, outline: &str, tolerance: f64) -> String {
    let (Some(trip), Ok(points)) = (
        parse_id(trip),
        serde_json::from_str::<Vec<[f64; 2]>>(outline),
    ) else {
        return "error: bad trip or outline".into();
    };
    let want = (!points.is_empty()).then(|| cave_scans::simplify(&points, tolerance));
    scoped_edit("scans", |doc| {
        cave_scans::set_trim(doc, trip, view, path, want)
    })
}

/// Starts a brand new cave: one empty trip called `trip`, written in `unit` (`ft` or `m`), with the
/// cave's `name`. Replaces the open cave. Returns `ok` or `error: ...`.
#[wasm_bindgen]
pub fn new_cave(name: &str, trip: &str, unit: &str) -> String {
    let _ = console();
    match cave_notebook::new_cave(ActorId(1), &mut random_ids(), name, trip, unit) {
        Ok(doc) => {
            tracing::info!(source = "import", "started a new cave: {name}");
            install(doc);
            "ok".into()
        }
        Err(e) => format!("error: {e:?}"),
    }
}

/// Every scanned page marked complete (JSON array of paths).
#[wasm_bindgen]
pub fn scan_completed() -> String {
    DOC.with(|d| {
        let v = d
            .borrow()
            .as_ref()
            .map(cave_scans::completed)
            .unwrap_or_default();
        serde_json::to_string(&v).unwrap_or_else(|_| "[]".into())
    })
}

/// Marks pages (JSON array of paths) complete or incomplete: one undoable edit in the Scans history.
#[wasm_bindgen]
pub fn scan_set_complete(paths: &str, complete: bool) -> String {
    let Ok(paths) = serde_json::from_str::<Vec<String>>(paths) else {
        return "error: unreadable list of pages".into();
    };
    scoped_edit("scans", |doc| {
        cave_scans::set_complete(doc, &paths, complete)
    })
}

// ---- Cave Shelf -----------------------------------------------------------------------------

/// What the shelf shows about the open cave (JSON), or `null` when none is open.
#[wasm_bindgen]
pub fn shelf_summary() -> String {
    DOC.with(|d| {
        let v = d.borrow().as_ref().map(cave_shelf::summarize);
        serde_json::to_string(&v).unwrap_or_else(|_| "null".into())
    })
}

/// What the shelf shows about a cave kept as DXF text, without opening it (the open cave is not
/// touched). `null` when the text is not a cave.
#[wasm_bindgen]
pub fn shelf_summarize(dxf: &str) -> String {
    let doc = cave_dxf::import(dxf, ActorId(1))
        .ok()
        .and_then(|i| i.document);
    serde_json::to_string(&doc.as_ref().map(cave_shelf::summarize))
        .unwrap_or_else(|_| "null".into())
}

/// What the shelf shows about a cave kept in CaveCAD's own format (its ops as JSON), without opening it.
#[wasm_bindgen]
pub fn shelf_summarize_native(ops: &str) -> String {
    let doc = serde_json::from_str::<Vec<cave_doc::Op>>(ops)
        .ok()
        .and_then(|ops| {
            let mut doc = Document::new(ActorId(1));
            ops.iter().all(|op| doc.apply(op).is_ok()).then_some(doc)
        });
    serde_json::to_string(&doc.as_ref().map(cave_shelf::summarize))
        .unwrap_or_else(|_| "null".into())
}

/// What the shelf shows about a sample cave given as shot-list JSON, without opening it.
#[wasm_bindgen]
pub fn shelf_summarize_demo(json: &str) -> String {
    let doc = cave_fixture::from_shot_list(json, ActorId(1)).ok();
    serde_json::to_string(&doc.as_ref().map(cave_shelf::summarize))
        .unwrap_or_else(|_| "null".into())
}
