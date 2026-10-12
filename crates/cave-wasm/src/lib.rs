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
            DOC.with(|d| *d.borrow_mut() = Some(doc));
            HISTORY.with(|h| h.borrow_mut().clear());
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
                    DOC.with(|d| *d.borrow_mut() = Some(doc));
                    HISTORY.with(|h| h.borrow_mut().clear());
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
        let (_, actions) =
            cave_draw::map_line(doc, &mut random_ids(), kind, &points, widths, &frames)
                .map_err(|e| format!("{e:?}"))?;
        let actions = cave_draw::with_closed(actions, closed).map_err(|e| format!("{e:?}"))?;
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
/// puts it on the left of travel. Returns `{spine, lines}` as JSON: whether the curve itself is drawn, and the ornament polylines (none for a type with no shape).
#[wasm_bindgen]
pub fn ornament(kind: &str, curve: &str, closed: bool, flip: bool) -> String {
    let pts = serde_json::from_str::<Vec<[f64; 2]>>(curve).unwrap_or_default();
    let side = if flip { -1.0 } else { 1.0 };
    let spine = cave_shape::shape(kind).is_none_or(|s| !s.hide_spine);
    let lines = cave_shape::ornament(kind, &pts, closed, side, 1.0);
    serde_json::json!({"spine": spine, "lines": lines}).to_string()
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
            Some(serde_json::json!({"kind": e.text(cave_doc::Field::FeatureType), "view": view, "closed": closed, "flip": e.flag(cave_doc::Field::FeatureFlip)}))
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
        let (_, actions) = cave_draw::section_sketch(
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
        let (_, actions) =
            cave_draw::profile_sketch(doc, &mut random_ids(), kind, run, &points, widths, &frames)
                .map_err(|e| format!("{e:?}"))?;
        let actions = cave_draw::with_closed(actions, closed).map_err(|e| format!("{e:?}"))?;
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
            add_section_sketch("nope", "wall", "[[0,0],[1,1]]", "", false, false)
                .starts_with("error")
        );
        assert!(add_section_sketch(&id, "wall", "garbage", "", false, false).starts_with("error"));
    }
}
