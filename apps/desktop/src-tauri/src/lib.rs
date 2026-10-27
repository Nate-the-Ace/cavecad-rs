//! The CaveCAD desktop app: the web app in a native window, plus what a browser is not allowed to
//! do. A browser never says where a picked folder lives and cannot open Finder or Explorer; here the
//! folder dialog gives the real path, pages are read straight from it, and Reveal opens the file
//! manager on the real file.

mod cavefolder;
mod files;

use std::path::PathBuf;
use std::sync::Mutex;
use tauri::ipc::{InvokeBody, Request, Response};
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
    let urls: Vec<String> = urls
        .into_iter()
        .filter(|u| u.starts_with("cavecad://"))
        .collect();
    if urls.is_empty() {
        return;
    }
    app.state::<Links>()
        .0
        .lock()
        .unwrap()
        .extend(urls.iter().cloned());
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.set_focus();
        for url in &urls {
            let quoted = serde_json::to_string(url).unwrap_or_default();
            let _ = window.eval(format!(
                "window.__cavecadLink && window.__cavecadLink({quoted})"
            ));
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

/// The person's Documents folder, where the New cave dialog starts.
#[tauri::command]
fn documents_folder(app: AppHandle) -> Option<String> {
    app.path()
        .document_dir()
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

/// Asks where a new cave's folder goes, starting at `start` (the last place chosen, or Documents).
#[tauri::command]
async fn pick_cave_place(app: AppHandle, start: Option<String>) -> Option<String> {
    let start = start
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .or_else(|| app.path().document_dir().ok());
    let mut dialog = app
        .dialog()
        .file()
        .set_title("Choose where the cave's folder goes");
    if let Some(dir) = start {
        dialog = dialog.set_directory(dir);
    }
    dialog
        .blocking_pick_folder()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
}

/// What the folder for a cave named `name` under `parent` would be, and what is there already.
#[tauri::command]
fn look_cave_place(parent: String, name: String) -> Result<cavefolder::Looked, String> {
    cavefolder::look(&PathBuf::from(parent), &name).map_err(|e| e.to_string())
}

/// Makes (or completes) a cave's folder with its standard subfolders. Gives back its path.
#[tauri::command]
fn make_cave_folder(parent: String, name: String) -> Result<String, String> {
    cavefolder::make(&PathBuf::from(parent), &name)
        .map(|(folder, _)| folder.to_string_lossy().into_owned())
        .map_err(|e| e.to_string())
}

/// True when a cave's folder is still where it was.
#[tauri::command]
fn folder_exists(folder: String) -> bool {
    PathBuf::from(folder).is_dir()
}

/// A header the page sent with raw bytes, `%`-escaped.
fn header(request: &Request<'_>, name: &str) -> Result<String, String> {
    request
        .headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(cavefolder::unescape)
        .ok_or_else(|| format!("missing {name}"))
}

fn body<'a>(request: &'a Request<'a>) -> Result<&'a [u8], String> {
    match request.body() {
        InvokeBody::Raw(bytes) => Ok(bytes),
        InvokeBody::Json(_) => Err("expected the file's bytes".into()),
    }
}

/// Writes the cave file into its folder (bytes in the body; `x-folder`, `x-file` and `x-stamp`,
/// the person's local time as `yyyy-mm-dd hhmm`, in headers), backing up the last copy first.
#[tauri::command]
fn write_cave(request: Request) -> Result<(), String> {
    let folder = header(&request, "x-folder")?;
    let file = header(&request, "x-file")?;
    let now = cave_folder::Stamp::parse(&header(&request, "x-stamp")?).ok_or("bad time")?;
    cavefolder::write(&PathBuf::from(folder), &file, body(&request)?, now)
        .map_err(|e| e.to_string())
}

/// The cave file's bytes, read from its folder.
#[tauri::command]
fn read_cave(folder: String, file: String) -> Result<Response, String> {
    cavefolder::read(&PathBuf::from(folder), &file)
        .map(Response::new)
        .map_err(|e| e.to_string())
}

/// Writes an export into the cave folder's `exports/` (bytes in the body; `x-folder` and `x-file`
/// in headers). Gives back where it went.
#[tauri::command]
fn write_export(request: Request) -> Result<String, String> {
    let folder = header(&request, "x-folder")?;
    let file = header(&request, "x-file")?;
    cavefolder::write_export(&PathBuf::from(folder), &file, body(&request)?)
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| e.to_string())
}

/// Starts the app.
pub fn run() {
    let builder = tauri::Builder::default();
    // A second launch (a link clicked while the app is open) hands its link to the app already running and goes away.
    #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
        hand_over(app, args)
    }));
    builder
        .manage(Links::default())
        .plugin(tauri_plugin_deep_link::init())
        .setup(|app| {
            #[cfg(any(windows, target_os = "linux"))]
            let _ = app.deep_link().register_all(); // lets a copy that was not installed (a portable run) take links too
            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                hand_over(&handle, event.urls().into_iter().map(|u| u.to_string()))
            });
            if let Ok(Some(urls)) = app.deep_link().get_current() {
                app.state::<Links>().0.lock().unwrap().extend(
                    urls.into_iter()
                        .map(|u| u.to_string())
                        .filter(|u| u.starts_with("cavecad://")),
                );
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
            take_links,
            documents_folder,
            pick_cave_place,
            look_cave_place,
            make_cave_folder,
            folder_exists,
            write_cave,
            read_cave,
            write_export
        ])
        .run(tauri::generate_context!())
        .expect("the CaveCAD desktop app could not start");
}
