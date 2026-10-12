//! The CaveCAD desktop app: the web app in a native window, plus what a browser is not allowed to
//! do. A browser never says where a picked folder lives and cannot open Finder or Explorer; here the
//! folder dialog gives the real path, pages are read straight from it, and Reveal opens the file
//! manager on the real file.

mod files;

use std::path::PathBuf;
use tauri::ipc::Response;
use tauri_plugin_dialog::DialogExt;

/// Asks which folder to use, and gives back its real path (`None` if the person cancelled).
#[tauri::command]
async fn pick_folder(app: tauri::AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .set_title("Choose the folder your scans are in")
        .blocking_pick_folder()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
}

/// The pictures and PDFs under a folder, with sizes.
#[tauri::command]
async fn list_scans(root: String) -> Result<Vec<files::Entry>, String> {
    files::list(&PathBuf::from(root)).map_err(|e| e.to_string())
}

/// One page's bytes, straight from the folder (never from outside it).
#[tauri::command]
async fn read_scan(root: String, rel: String) -> Result<Response, String> {
    files::read(&PathBuf::from(root), &rel)
        .map(Response::new)
        .map_err(|e| e.to_string())
}

/// Opens Finder or Explorer on the real file, selected (an empty `rel` is the folder itself).
#[tauri::command]
fn reveal(root: String, rel: String) -> Result<(), String> {
    let path = files::safe_join(&PathBuf::from(root), &rel)
        .ok_or("that path is outside the scans folder")?;
    if !path.exists() {
        return Err("that file is not there any more".into());
    }
    tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| e.to_string())
}

/// Starts the app.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            pick_folder,
            list_scans,
            read_scan,
            reveal
        ])
        .run(tauri::generate_context!())
        .expect("the CaveCAD desktop app could not start");
}
