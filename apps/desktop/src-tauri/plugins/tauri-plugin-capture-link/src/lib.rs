//! CaveCAD on iPhone and iPad, linked to CaveCAD Capture on the same device with no network:
//! - stations go to Capture through a keychain item in the shared access group
//!   (`<team>.app.cavecad.shared`, service `cavecad.stations`, account = server cave id);
//! - Capture's shots are read from its Files folder, picked once and remembered as a bookmark.
//!
//! The Swift side is `ios/Sources/CaptureLinkPlugin.swift`. There are no JS commands: the app's own
//! commands call [`CaptureLink`] (from `app.state()`). Off iOS every call fails with
//! [`NOT_HERE`], so the desktop builds are unaffected.

use serde::Deserialize;
use serde::de::DeserializeOwned;
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager, Runtime};

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_capture_link);

/// The error every call gives off iOS.
pub const NOT_HERE: &str = "only in CaveCAD on iPhone and iPad";

/// What resolving the remembered folder found.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Resolved {
    /// The folder's path, access held for the app's life; `None` when nothing is linked or the
    /// bookmark no longer resolves.
    pub path: Option<String>,
    /// The bookmark was stale (re-saved when it still resolved) or could not be resolved.
    #[serde(default)]
    pub stale: bool,
}

#[derive(Deserialize)]
struct Written {
    bytes: usize,
}

#[derive(Deserialize)]
struct Picked {
    path: Option<String>,
}

/// The plugin's handle, kept in the app's state.
pub struct CaptureLink<R: Runtime>(#[allow(dead_code)] Option<tauri::plugin::PluginHandle<R>>);

impl<R: Runtime> CaptureLink<R> {
    /// Writes (adds or replaces) one cave's shared stations JSON. Returns the bytes written.
    pub fn write_stations(&self, cave_id: &str, json: &str) -> Result<usize, String> {
        self.call::<Written>(
            "writeStations",
            serde_json::json!({ "caveId": cave_id, "json": json }),
        )
        .map(|w| w.bytes)
    }

    /// Deletes one cave's shared stations (no error when there were none).
    pub fn delete_stations(&self, cave_id: &str) -> Result<(), String> {
        self.call::<serde_json::Value>("deleteStations", serde_json::json!({ "caveId": cave_id }))
            .map(|_| ())
    }

    /// Shows the Files folder picker; remembers the pick as a bookmark. `None` when cancelled.
    /// Blocks until the person picks: never call it on the main thread.
    pub fn pick_folder(&self) -> Result<Option<String>, String> {
        self.call::<Picked>("pickFolder", serde_json::json!({}))
            .map(|p| p.path)
    }

    /// Resolves the remembered folder (and holds access to it).
    pub fn resolve_folder(&self) -> Result<Resolved, String> {
        self.call("resolveFolder", serde_json::json!({}))
    }

    fn call<T: DeserializeOwned>(&self, cmd: &str, args: serde_json::Value) -> Result<T, String> {
        match &self.0 {
            #[cfg(target_os = "ios")]
            Some(h) => h.run_mobile_plugin(cmd, args).map_err(|e| e.to_string()),
            #[cfg(not(target_os = "ios"))]
            Some(_) => {
                let _ = (cmd, args);
                Err(NOT_HERE.into())
            }
            None => {
                let _ = (cmd, args);
                Err(NOT_HERE.into())
            }
        }
    }
}

/// The plugin: registers the Swift side on iOS and puts a [`CaptureLink`] in the app's state.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("capture-link")
        .setup(|app, _api| {
            #[cfg(target_os = "ios")]
            let h = Some(_api.register_ios_plugin(init_plugin_capture_link)?);
            #[cfg(not(target_os = "ios"))]
            let h = None;
            app.manage(CaptureLink::<R>(h));
            Ok(())
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_ios_every_call_says_not_here() {
        let c: CaptureLink<tauri::Wry> = CaptureLink(None);
        assert_eq!(c.write_stations("a", "{}").unwrap_err(), NOT_HERE);
        assert_eq!(c.resolve_folder().unwrap_err(), NOT_HERE);
    }
}
