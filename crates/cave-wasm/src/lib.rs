//! The browser's door into the Rust engine: thin `wasm-bindgen` wrappers, no logic of their own.
//!
//! Each function here forwards to a core crate. Keep it that way: behaviour belongs in the
//! `cave-*` crates where it is tested natively; this crate only adapts types for JavaScript.

use cave_doc::{ActorId, Document};
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
            summary
        }
        Err(e) => {
            tracing::error!(source = "import", "could not open demo cave: {e}");
            format!("error: {e}")
        }
    }
}

/// The open cave as a scene for the map canvas (JSON). Empty scene when nothing is open.
#[wasm_bindgen]
pub fn scene(adjusted: bool) -> String {
    DOC.with(|d| {
        let scene = match d.borrow().as_ref() {
            Some(doc) => cave_scene::build(doc, adjusted),
            None => cave_scene::Scene::default(),
        };
        serde_json::to_string(&scene).unwrap_or_else(|_| "{}".to_string())
    })
}
