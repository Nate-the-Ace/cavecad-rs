//! The app updating itself: ask the CaveCAD server whether a newer version exists, download it and check its
//! signature in the background, and install it only when the person says so (then restart). Spec:
//! docs/superpowers/specs/2026-11-09-desktop-self-update-design.md.
//!
//! The server answers only signed-in members, so the page hands over the API base and the person's bearer token.
//! Updates are signed with the CaveCAD update key; the plugin refuses one whose signature does not match the public
//! key in tauri.conf.json.

use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, State};
use tauri_plugin_updater::{Update, UpdaterExt};

/// A downloaded, verified update waiting for the person to say "Restart now".
#[derive(Default)]
pub struct Pending(Mutex<Option<(Update, Vec<u8>)>>);

/// What the page is told about a newer version.
#[derive(Serialize)]
pub struct Found {
    /// Its version, e.g. `0.1.42`.
    version: String,
    /// Whether this copy can install it itself. False for Linux installed from a .deb or .rpm (only the AppImage
    /// replaces itself): the page points at the Downloads list instead, and nothing is downloaded.
    installable: bool,
}

/// Whether this copy of the app can replace itself.
fn can_install() -> bool {
    if cfg!(target_os = "linux") {
        std::env::var_os("APPIMAGE").is_some()
    } else {
        true
    }
}

/// Asks `base` (the account API, `https://api.cavecad.com`) for a version newer than this one. `channel` `"dev"`
/// (admins' test builds) asks for the newer of the test build and the regular one; the server ignores it for
/// anyone who is not an admin. When there is one and
/// this copy can install it, downloads and verifies it, and keeps it until [`update_install`]. `None`: up to date.
#[tauri::command]
pub async fn update_prepare(
    app: AppHandle,
    pending: State<'_, Pending>,
    base: String,
    token: String,
    channel: Option<String>,
) -> Result<Option<Found>, String> {
    if !base.starts_with("https://") && !base.starts_with("http://localhost") {
        return Err("the update address must be https".into());
    }
    let query = if channel.as_deref() == Some("dev") {
        "?channel=dev"
    } else {
        ""
    };
    let endpoint = format!(
        "{}/api/desktop-update/{{{{target}}}}/{{{{arch}}}}/{{{{current_version}}}}{query}",
        base.trim_end_matches('/')
    )
    .parse()
    .map_err(|e| format!("bad update address: {e}"))?;
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(|e| e.to_string())?
        .header("Authorization", format!("Bearer {token}"))
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?;
    let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let version = update.version.clone();
    if !can_install() {
        return Ok(Some(Found {
            version,
            installable: false,
        }));
    }
    // the plugin checks the signature as the download completes; a bad one is an error here, never kept
    let bytes = update
        .download(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    *pending.0.lock().unwrap() = Some((update, bytes));
    Ok(Some(Found {
        version,
        installable: true,
    }))
}

/// Installs the update [`update_prepare`] kept, then restarts into it. On Windows the installer closes the app itself.
#[tauri::command]
pub async fn update_install(app: AppHandle, pending: State<'_, Pending>) -> Result<(), String> {
    let Some((update, bytes)) = pending.0.lock().unwrap().take() else {
        return Err("no update is ready".into());
    };
    update.install(bytes).map_err(|e| e.to_string())?;
    app.restart();
}
