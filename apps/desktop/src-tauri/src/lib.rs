//! The CaveCAD desktop app: the web app in a native window, plus what a browser is not allowed to
//! do. A browser never says where a picked folder lives and cannot open Finder or Explorer; here the
//! folder dialog gives the real path, pages are read straight from it, and Reveal opens the file
//! manager on the real file.

mod files;

use std::path::PathBuf;
use std::sync::Mutex;
use tauri::ipc::Response;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_dialog::DialogExt;

/// Join links (`cavecad://join?...`) that arrived and have not been picked up by the page yet (the app can be
/// started by a link before its page is ready to hear about it).
#[derive(Default)]
struct Links(Mutex<Vec<String>>);

/// The links waiting for the page, handed over once.
#[tauri::command]
fn take_links(links: State<Links>) -> Vec<String> {
    std::mem::take(&mut *links.0.lock().unwrap())
}

/// A join link arrived (the app was opened by it, or was already running): the window comes to the front and the page
/// is told. Only our own scheme is accepted, and the page decides what the link means.
fn hand_over(app: &AppHandle, urls: impl IntoIterator<Item = String>) {
    let urls: Vec<String> = urls.into_iter().filter(|u| u.starts_with("cavecad://")).collect();
    if urls.is_empty() {
        return;
    }
    app.state::<Links>().0.lock().unwrap().extend(urls.iter().cloned());
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.set_focus();
        for url in &urls {
            let quoted = serde_json::to_string(url).unwrap_or_default();
            let _ = window.eval(format!("window.__cavecadLink && window.__cavecadLink({quoted})"));
        }
    }
}

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

/// Opens an address in the system browser (the desktop sign-in). Only https addresses.
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("only https addresses open".into());
    }
    tauri_plugin_opener::open_url(url, None::<&str>).map_err(|e| e.to_string())
}

/// Starts the app.
pub fn run() {
    let builder = tauri::Builder::default();
    // A second launch (a link clicked while the app is open) hands its link to the app already running and goes away.
    #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| hand_over(app, args)));
    builder
        .manage(Links::default())
        .plugin(tauri_plugin_deep_link::init())
        .setup(|app| {
            #[cfg(any(windows, target_os = "linux"))]
            let _ = app.deep_link().register_all(); // lets a copy that was not installed (a portable run) take links too
            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| hand_over(&handle, event.urls().into_iter().map(|u| u.to_string())));
            if let Ok(Some(urls)) = app.deep_link().get_current() {
                app.state::<Links>().0.lock().unwrap().extend(urls.into_iter().map(|u| u.to_string()).filter(|u| u.starts_with("cavecad://")));
            }
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            pick_folder,
            list_scans,
            read_scan,
            reveal,
            open_url,
            take_links
        ])
        .run(tauri::generate_context!())
        .expect("the CaveCAD desktop app could not start");
}
