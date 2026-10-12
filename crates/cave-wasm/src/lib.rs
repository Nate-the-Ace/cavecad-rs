//! The browser's door into the Rust engine: thin `wasm-bindgen` wrappers, no logic of their own.
//!
//! Each function here forwards to a core crate. Keep it that way: behaviour belongs in the
//! `cave-*` crates where it is tested natively; this crate only adapts types for JavaScript.

use cave_doc::{ActorId, Document, History};
use cave_log::{BufferLayer, LogBuffer};
use serde::Serialize;
use std::cell::RefCell;
use std::sync::OnceLock;
use tracing_subscriber::layer::SubscriberExt;
use wasm_bindgen::prelude::*;

static CONSOLE: OnceLock<LogBuffer> = OnceLock::new();

thread_local! {
    /// The cave currently open. The browser is single-threaded, so this is the one document.
    static DOC: RefCell<Option<Document>> = const { RefCell::new(None) };
    /// Undo and redo for the person at this browser.
    static HISTORY: RefCell<History> = RefCell::new(History::new());
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
            HISTORY.with(|h| *h.borrow_mut() = History::new());
            summary
        }
        Err(e) => {
            tracing::error!(source = "import", "could not open demo cave: {e}");
            format!("error: {e}")
        }
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
pub fn add_profile_sketch(run: &str, points: &str, widths: &str, adjusted: bool) -> String {
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
            cave_draw::profile_sketch(doc, &mut random_ids(), run, &points, widths, &frames)
                .map_err(|e| format!("{e:?}"))?;
        let edit = doc.edit(actions).map_err(|e| format!("{e:?}"))?;
        HISTORY.with(|h| h.borrow_mut().record(&edit));
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

/// Undoes this person's last edit. Returns whether anything changed.
#[wasm_bindgen]
pub fn undo() -> bool {
    DOC.with(|d| {
        let mut slot = d.borrow_mut();
        let Some(doc) = slot.as_mut() else {
            return false;
        };
        let done = HISTORY
            .with(|h| h.borrow_mut().undo(doc))
            .ok()
            .flatten()
            .is_some();
        if done {
            tracing::info!(source = "edit", "undo");
        }
        done
    })
}

/// Redoes the last undone edit. Returns whether anything changed.
#[wasm_bindgen]
pub fn redo() -> bool {
    DOC.with(|d| {
        let mut slot = d.borrow_mut();
        let Some(doc) = slot.as_mut() else {
            return false;
        };
        let done = HISTORY
            .with(|h| h.borrow_mut().redo(doc))
            .ok()
            .flatten()
            .is_some();
        if done {
            tracing::info!(source = "edit", "redo");
        }
        done
    })
}

/// Whether undo and redo have anything to do, as JSON `{"undo":bool,"redo":bool}`.
#[wasm_bindgen]
pub fn edit_state() -> String {
    HISTORY.with(|h| {
        let h = h.borrow();
        format!("{{\"undo\":{},\"redo\":{}}}", h.can_undo(), h.can_redo())
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
