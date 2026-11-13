//! The test-build debug bridge: lets a developer's tools (the `cavecad-app` MCP server in `tools/cavecad-app-mcp`)
//! look inside a running test build without touching the mouse or the window: run script in the page, screenshot
//! the window (even when other windows cover it), and read the page's log.
//!
//! Compiled ONLY with the `debug-bridge` feature, which desktop.yml turns on for test builds (channel=dev) and never
//! for regular releases. It listens on 127.0.0.1 only, and every request must carry the key written, readable by
//! this user alone, to `<app data>/debug-bridge.json` together with the port. Anyone who can read that file can
//! already run anything as this user, so the key adds nothing for them and keeps everyone else out.
//!
//! Protocol: HTTP/1.1, `POST /<op>` with a JSON body and the header `X-Bridge-Key`. Ops: `status`, `eval`
//! (`{ "code": "...", "timeoutMs": 15000 }`: the code is a function body run in the page, `return` gives the
//! result), `screenshot` (`{ "path": "/abs/out.png" }`, macOS).

#![cfg_attr(not(feature = "debug-bridge"), allow(dead_code, unused_imports))]

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::Duration;
use tauri::{AppHandle, Manager};

/// The port tried first ("CAVE"+5; the old QCAD-era bridge used 42283). Taken: any free port, written to the file.
const PORT: u16 = 42285;

/// Answers to `eval` requests waiting for the page.
#[derive(Default)]
pub struct Waiting {
    next: AtomicU64,
    senders: Mutex<HashMap<u64, mpsc::Sender<(bool, String)>>>,
}

/// The page hands back an `eval` result. Registered in every build; without the bridge nothing is waiting, so a
/// call does nothing.
#[tauri::command]
pub fn bridge_result(waiting: tauri::State<'_, Waiting>, id: u64, ok: bool, value: String) {
    if let Some(tx) = waiting.senders.lock().unwrap().remove(&id) {
        let _ = tx.send((ok, value));
    }
}

/// Starts the bridge on its own thread and writes `debug-bridge.json`. Problems are printed, never fatal.
#[cfg(feature = "debug-bridge")]
pub fn start(app: &AppHandle) {
    let listener = TcpListener::bind(("127.0.0.1", PORT)).or_else(|_| TcpListener::bind(("127.0.0.1", 0)));
    let Ok(listener) = listener else {
        eprintln!("debug bridge: could not listen");
        return;
    };
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    let key = random_key();
    if let Err(e) = write_info(app, port, &key) {
        eprintln!("debug bridge: could not write its file: {e}");
        return;
    }
    eprintln!("debug bridge: listening on 127.0.0.1:{port}");
    let app = app.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (app, key) = (app.clone(), key.clone());
            std::thread::spawn(move || {
                if let Err(e) = serve(&app, &key, stream) {
                    eprintln!("debug bridge: {e}");
                }
            });
        }
    });
}

#[cfg(feature = "debug-bridge")]
fn random_key() -> String {
    // the standard library seeds its hash keys from the operating system's random source: four of them make the key
    use std::hash::{BuildHasher, Hasher};
    (0..4)
        .map(|i| {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u64(i);
            format!("{:016x}", h.finish())
        })
        .collect()
}

#[cfg(feature = "debug-bridge")]
fn write_info(app: &AppHandle, port: u16, key: &str) -> std::io::Result<()> {
    let dir = app.path().app_data_dir().map_err(std::io::Error::other)?;
    std::fs::create_dir_all(&dir)?;
    let file = dir.join("debug-bridge.json");
    let body = serde_json::json!({ "port": port, "key": key, "pid": std::process::id(), "version": app.package_info().version.to_string() });
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&file)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600))?;
    }
    f.write_all(body.to_string().as_bytes())
}

/// One request: read it, check the key, answer.
fn serve(app: &AppHandle, key: &str, stream: TcpStream) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
    let (mut length, mut given) = (0usize, String::new());
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h)? == 0 || h == "\r\n" || h == "\n" {
            break;
        }
        let (name, value) = h.split_once(':').unwrap_or((&h, ""));
        match name.trim().to_ascii_lowercase().as_str() {
            "content-length" => length = value.trim().parse().unwrap_or(0).min(8 << 20),
            "x-bridge-key" => given = value.trim().to_string(),
            _ => {}
        }
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    let (code, answer) = if !same(&given, key) {
        (401, serde_json::json!({ "error": "bad key" }))
    } else {
        let req: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::json!({}));
        match handle(app, path.trim_start_matches('/'), &req) {
            Ok(v) => (200, v),
            Err(e) => (400, serde_json::json!({ "error": e })),
        }
    };
    let text = answer.to_string();
    let mut out = stream;
    write!(
        out,
        "HTTP/1.1 {code} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        if code == 200 { "OK" } else { "Error" },
        text.len()
    )?;
    out.flush()
}

/// Compares in constant time.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn handle(app: &AppHandle, op: &str, req: &serde_json::Value) -> Result<serde_json::Value, String> {
    let window = app.get_webview_window("main").ok_or("no main window")?;
    match op {
        "status" => Ok(serde_json::json!({
            "version": app.package_info().version.to_string(),
            "pid": std::process::id(),
            "url": window.url().map(|u| u.to_string()).unwrap_or_default(),
            "visible": window.is_visible().unwrap_or(false),
            "focused": window.is_focused().unwrap_or(false),
            "minimized": window.is_minimized().unwrap_or(false),
        })),
        "eval" => {
            let code = req.get("code").and_then(|c| c.as_str()).ok_or("eval needs `code`")?;
            let timeout = req.get("timeoutMs").and_then(|t| t.as_u64()).unwrap_or(15_000).clamp(100, 120_000);
            let waiting = app.state::<Waiting>();
            let id = waiting.next.fetch_add(1, Ordering::Relaxed);
            let (tx, rx) = mpsc::channel();
            waiting.senders.lock().unwrap().insert(id, tx);
            window.eval(wrap(id, code)).map_err(|e| e.to_string())?;
            let got = rx.recv_timeout(Duration::from_millis(timeout));
            waiting.senders.lock().unwrap().remove(&id);
            match got {
                Ok((true, v)) => Ok(serde_json::json!({ "ok": true, "value": v })),
                Ok((false, e)) => Ok(serde_json::json!({ "ok": false, "error": e })),
                Err(_) => Err(format!("no answer from the page within {timeout} ms")),
            }
        }
        "screenshot" => {
            let path = req.get("path").and_then(|p| p.as_str()).ok_or("screenshot needs `path`")?;
            screenshot(&window, path)?;
            Ok(serde_json::json!({ "path": path }))
        }
        _ => Err(format!("unknown op `{op}` (status, eval, screenshot)")),
    }
}

/// The code as an async function body, its result (or error) sent back through `bridge_result`. Code with no
/// `return` is taken as one expression.
fn wrap(id: u64, code: &str) -> String {
    let body = if code.contains("return") { code.to_string() } else { format!("return ({code});") };
    format!(
        r#"(async () => {{
  const send = (ok, value) => window.__TAURI_INTERNALS__.invoke('bridge_result', {{ id: {id}, ok, value }});
  try {{
    const v = await (async () => {{ {body}
    }})();
    let s;
    try {{ s = v === undefined ? 'undefined' : JSON.stringify(v, (k, x) => typeof x === 'bigint' ? String(x) : x); }} catch {{ s = String(v); }}
    await send(true, s ?? String(v));
  }} catch (e) {{
    await send(false, String((e && e.stack) || e));
  }}
}})();"#
    )
}

/// Captures the window itself (covered or not) to a PNG.
#[cfg(target_os = "macos")]
fn screenshot(window: &tauri::WebviewWindow, path: &str) -> Result<(), String> {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    let ns = window.ns_window().map_err(|e| e.to_string())? as *mut AnyObject;
    if ns.is_null() {
        return Err("no native window".into());
    }
    // windowNumber is read on the main thread (AppKit), handed back through a channel
    let (tx, rx) = mpsc::channel();
    let ns_addr = ns as usize;
    window
        .run_on_main_thread(move || {
            let n: isize = unsafe { msg_send![ns_addr as *mut AnyObject, windowNumber] };
            let _ = tx.send(n);
        })
        .map_err(|e| e.to_string())?;
    let number = rx.recv_timeout(Duration::from_secs(5)).map_err(|_| "window number timed out")?;
    let status = std::process::Command::new("/usr/sbin/screencapture")
        .args(["-x", "-o", "-l", &number.to_string(), path])
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err(format!("screencapture failed: {status}")) }
}

#[cfg(not(target_os = "macos"))]
fn screenshot(_window: &tauri::WebviewWindow, _path: &str) -> Result<(), String> {
    Err("screenshots are macOS only for now".into())
}
