//! The iPad build has no self-update (Xcode, TestFlight or the App Store install new versions): the same commands as
//! `update.rs`, answering "up to date", so the page needs no special case.

use serde::Serialize;
use tauri::State;

/// Nothing is ever waiting here.
#[derive(Default)]
pub struct Pending;

/// Same shape as the desktop's answer; never sent.
#[allow(dead_code)]
#[derive(Serialize)]
pub struct Found {
    version: String,
    installable: bool,
}

/// Always up to date. (The arguments are named as the page sends them: Tauri matches them by name.)
#[allow(unused_variables)]
#[tauri::command]
pub async fn update_prepare(
    pending: State<'_, Pending>,
    base: String,
    token: String,
    channel: Option<String>,
) -> Result<Option<Found>, String> {
    Ok(None)
}

/// There is never an update to install.
#[allow(unused_variables)]
#[tauri::command]
pub async fn update_install(pending: State<'_, Pending>) -> Result<(), String> {
    Err("updates come from Xcode or the App Store on this device".into())
}
