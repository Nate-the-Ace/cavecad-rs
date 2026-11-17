//! Bluetooth for the desktop app. The app's web view has no Web Bluetooth (Safari's engine on the Mac, WebView2 on
//! Windows), so the page gets a stand-in for `navigator.bluetooth` (`apps/web/src/lib/native-bluetooth.ts`) that
//! calls these commands. The instrument code in the page (`webble.ts`) and the Rust link behind it run unchanged, so
//! a surveying instrument works the same in Chrome and in the app.
//!
//! Only what the instruments use: scan, connect and list services, read, write (with or without response), turn on
//! notifications, disconnect. Notifications and dropped connections are sent to the page by calling
//! `window.__cavecadBle(...)` with a small JSON message: `{"t":"notify","id","char","bytes"}` or `{"t":"gone","id"}`.

use btleplug::api::{
    Central, CentralEvent, CharPropFlags, Characteristic, Manager as _, Peripheral as _, ScanFilter, WriteType,
};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures::StreamExt;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager as _, State};
use tokio::sync::OnceCell;

/// The Bluetooth adapter (found once) and the devices seen, by id.
#[derive(Default)]
pub struct Ble {
    adapter: OnceCell<Adapter>,
    seen: Mutex<HashMap<String, Peripheral>>,
    /// Devices whose notifications are already being passed to the page.
    listening: Mutex<HashSet<String>>,
    /// Devices whose connection dropped (not by the page): closed properly before they are connected again.
    dropped: Mutex<HashSet<String>>,
}

/// A device found by a scan.
#[derive(Serialize)]
pub struct Found {
    id: String,
    name: Option<String>,
    services: Vec<String>,
    rssi: Option<i16>,
}

/// A characteristic of a connected device: its uuid and what it can do.
#[derive(Serialize)]
pub struct Char {
    uuid: String,
    props: Vec<&'static str>,
}

/// A service of a connected device and its characteristics.
#[derive(Serialize)]
pub struct Service {
    uuid: String,
    chars: Vec<Char>,
}

/// Appends lines to the diagnostic log, `cavecad-debug.log` in the app's data folder (kept under 2 MB: the older
/// half goes). Read off an iPhone with `xcrun devicectl device copy from --domain-type appDataContainer`.
pub fn debug_append(app: &AppHandle, lines: &[String]) {
    let Ok(dir) = app.path().app_data_dir() else { return };
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("cavecad-debug.log");
    let mut text = std::fs::read_to_string(&path).unwrap_or_default();
    for l in lines {
        text.push_str(l);
        text.push('\n');
    }
    if text.len() > 2_000_000 {
        let cut = text.len() / 2;
        let at = text[cut..].find('\n').map_or(cut, |i| cut + i + 1);
        text = text[at..].to_string();
    }
    let _ = std::fs::write(&path, text);
}

fn trace(app: &AppHandle, line: String) {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    debug_append(app, &[format!("{t} native {line}")]);
}

/// Lines from the page for the diagnostic log.
#[tauri::command]
pub fn debug_log(app: AppHandle, lines: Vec<String>) {
    debug_append(&app, &lines);
}

/// Sends a message to the page.
fn tell(app: &AppHandle, msg: serde_json::Value) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.eval(format!("window.__cavecadBle && window.__cavecadBle({msg})"));
    }
}

/// The first Bluetooth adapter; found once, and from then on its dropped connections are told to the page.
async fn adapter<'a>(app: &AppHandle, ble: &'a Ble) -> Result<&'a Adapter, String> {
    ble.adapter
        .get_or_try_init(|| async {
            let manager = Manager::new()
                .await
                .map_err(|e| format!("Bluetooth is not available: {e}"))?;
            let adapter = manager
                .adapters()
                .await
                .map_err(|e| format!("Bluetooth is not available: {e}"))?
                .into_iter()
                .next()
                .ok_or("This computer has no Bluetooth (or it is turned off).")?;
            let mut events = adapter
                .events()
                .await
                .map_err(|e| format!("Bluetooth is not available: {e}"))?;
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                while let Some(event) = events.next().await {
                    match &event {
                        CentralEvent::DeviceConnected(id) => trace(&app, format!("event connected id={id}")),
                        CentralEvent::DeviceDisconnected(id) => trace(&app, format!("event disconnected id={id}")),
                        _ => {}
                    }
                    if let CentralEvent::DeviceDisconnected(id) = event {
                        let id = id.to_string();
                        app.state::<Ble>().listening.lock().unwrap().remove(&id);
                        app.state::<Ble>().dropped.lock().unwrap().insert(id.clone());
                        tell(&app, serde_json::json!({ "t": "gone", "id": id }));
                    }
                }
            });
            Ok::<Adapter, String>(adapter)
        })
        .await
}

fn device(ble: &Ble, id: &str) -> Result<Peripheral, String> {
    ble.seen
        .lock()
        .unwrap()
        .get(id)
        .cloned()
        .ok_or_else(|| "That device is no longer in range. Scan again.".into())
}

fn characteristic(p: &Peripheral, service: &str, uuid: &str) -> Result<Characteristic, String> {
    p.characteristics()
        .into_iter()
        .find(|c| {
            c.uuid.to_string().eq_ignore_ascii_case(uuid)
                && (service.is_empty() || c.service_uuid.to_string().eq_ignore_ascii_case(service))
        })
        .ok_or_else(|| format!("The instrument has no characteristic {uuid}."))
}

fn props(f: CharPropFlags) -> Vec<&'static str> {
    let mut v = Vec::new();
    for (flag, name) in [
        (CharPropFlags::READ, "read"),
        (CharPropFlags::WRITE, "write"),
        (CharPropFlags::WRITE_WITHOUT_RESPONSE, "writeWithoutResponse"),
        (CharPropFlags::NOTIFY, "notify"),
        (CharPropFlags::INDICATE, "indicate"),
    ] {
        if f.contains(flag) {
            v.push(name);
        }
    }
    v
}

/// Looks for devices for `ms` milliseconds (at most 15 s) and lists what was found, strongest signal first.
#[tauri::command]
pub async fn ble_scan(app: AppHandle, ble: State<'_, Ble>, ms: u64) -> Result<Vec<Found>, String> {
    let a = adapter(&app, &ble).await?;
    a.start_scan(ScanFilter::default())
        .await
        .map_err(|e| format!("Could not look for devices: {e}"))?;
    tokio::time::sleep(Duration::from_millis(ms.clamp(500, 15_000))).await;
    let _ = a.stop_scan().await;
    let mut out = Vec::new();
    for p in a.peripherals().await.map_err(|e| e.to_string())? {
        let Ok(Some(props)) = p.properties().await else { continue };
        let id = p.id().to_string();
        out.push(Found {
            id: id.clone(),
            name: props.local_name.clone(),
            services: props.services.iter().map(|u| u.to_string()).collect(),
            rssi: props.rssi,
        });
        ble.seen.lock().unwrap().insert(id, p);
    }
    out.sort_by_key(|f| std::cmp::Reverse(f.rssi.unwrap_or(i16::MIN)));
    Ok(out)
}

/// Connects to a device found by a scan and lists its services; from now on its notifications reach the page.
#[tauri::command]
pub async fn ble_connect(app: AppHandle, ble: State<'_, Ble>, id: String) -> Result<Vec<Service>, String> {
    adapter(&app, &ble).await?;
    let p = device(&ble, &id).inspect_err(|e| trace(&app, format!("connect id={id}: {e}")))?;
    let fail = |e: String| {
        trace(&app, format!("connect id={id}: {e}"));
        e
    };
    // After a drop, close what the last connection left behind before a fresh connect (as TopoDroid does: closeGatt,
    // then connectGatt). No "is it connected?" question first: on iOS that call once never answered, and the page's
    // reconnecting stopped with it. Every step has a time limit, so a stuck call can only cost one try.
    let dropped = ble.dropped.lock().unwrap().remove(&id);
    if dropped {
        trace(&app, format!("connect id={id}: closing the old connection first"));
        let _ = tokio::time::timeout(Duration::from_secs(3), p.disconnect()).await;
    }
    trace(&app, format!("connect id={id}: connecting"));
    let wait = Duration::from_secs(10);
    tokio::time::timeout(wait, p.connect())
        .await
        .map_err(|_| fail("The instrument did not answer. Is it on and close by?".into()))?
        .map_err(|e| fail(format!("Could not connect: {e}")))?;
    trace(&app, format!("connect id={id}: connected, listing services"));
    tokio::time::timeout(wait, p.discover_services())
        .await
        .map_err(|_| fail("The instrument did not list its services in time.".into()))?
        .map_err(|e| fail(format!("Could not read the instrument's services: {e}")))?;
    if ble.listening.lock().unwrap().insert(id.clone()) {
        let mut stream = p
            .notifications()
            .await
            .map_err(|e| format!("Could not listen to the instrument: {e}"))?;
        let app = app.clone();
        let id = id.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(n) = stream.next().await {
                tell(
                    &app,
                    serde_json::json!({ "t": "notify", "id": id, "char": n.uuid.to_string(), "bytes": n.value }),
                );
            }
        });
    }
    trace(&app, format!("connect id={id}: ready"));
    Ok(p.services()
        .into_iter()
        .map(|s| Service {
            uuid: s.uuid.to_string(),
            chars: s
                .characteristics
                .iter()
                .map(|c| Char { uuid: c.uuid.to_string(), props: props(c.properties) })
                .collect(),
        })
        .collect())
}

/// Disconnects a device (nothing to do if it already went).
#[tauri::command]
pub async fn ble_disconnect(ble: State<'_, Ble>, id: String) -> Result<(), String> {
    if let Ok(p) = device(&ble, &id) {
        let _ = p.disconnect().await;
    }
    ble.listening.lock().unwrap().remove(&id);
    Ok(())
}

/// Reads a characteristic's value.
#[tauri::command]
pub async fn ble_read(ble: State<'_, Ble>, id: String, service: String, uuid: String) -> Result<Vec<u8>, String> {
    let p = device(&ble, &id)?;
    let c = characteristic(&p, &service, &uuid)?;
    p.read(&c).await.map_err(|e| format!("Could not read: {e}"))
}

/// Writes to a characteristic, with or without a response.
#[tauri::command]
pub async fn ble_write(
    ble: State<'_, Ble>,
    id: String,
    service: String,
    uuid: String,
    bytes: Vec<u8>,
    with_response: bool,
) -> Result<(), String> {
    let p = device(&ble, &id)?;
    let c = characteristic(&p, &service, &uuid)?;
    let kind = if with_response { WriteType::WithResponse } else { WriteType::WithoutResponse };
    p.write(&c, &bytes, kind).await.map_err(|e| format!("Could not write: {e}"))
}

/// Turns on notifications (or indications) from a characteristic.
#[tauri::command]
pub async fn ble_subscribe(ble: State<'_, Ble>, id: String, service: String, uuid: String) -> Result<(), String> {
    let p = device(&ble, &id)?;
    let c = characteristic(&p, &service, &uuid)?;
    p.subscribe(&c).await.map_err(|e| format!("Could not listen: {e}"))
}
