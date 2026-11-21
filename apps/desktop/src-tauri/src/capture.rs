//! CaveCAD Capture on the same iPhone or iPad, offline (plan
//! `docs/superpowers/plans/2026-11-21-capture-cavecad-link.md`):
//! - the open cave's stations go to Capture through the keychain (`capture_share_stations`), via the
//!   `tauri-plugin-capture-link` Swift plugin;
//! - Capture's shots are read from its Files folder, linked once (`capture_link`), listed and placed
//!   the way Capture places them (card chaining first, `cave_capture::chains`), and their points
//!   handed to the web view thinned and relative to an anchor, as raw little-endian f32 bytes.
//!
//! Off iOS everything here answers "not available" and the plugin is a no-op.

use cave_capture::chain::{Rigid, Source, chains};
use cave_capture::{
    ShotHeader, StationRef, capture_to_survey, check_header, mesh_positions, station_for,
    thin_points,
};
use cave_survey::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;
use tauri::ipc::Response;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_capture_link::CaptureLink;

/// Largest shared-stations JSON accepted (a keychain item is meant to be small).
const MAX_STATIONS_JSON: usize = 1_000_000;

/// The hash of the last stations JSON written per cave, so an unchanged cave is not rewritten.
#[derive(Default)]
pub struct Shared(Mutex<HashMap<String, u64>>);

/// Capture's `shots` folder once linked (access held by the Swift plugin).
#[derive(Default)]
pub struct Linked(Mutex<Option<PathBuf>>);

/// A server cave id as Capture names its folder: 1 to 64 of `A-Z a-z 0-9 _ -`.
pub fn valid_cave_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// FNV-1a 64 of a text.
fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

/// Capture's `shots` folder from the folder the person picked: Capture's Documents ("Capture" in
/// Files), its `shots` folder, or one cave's folder inside it. `None` when it is none of those.
pub fn shots_root(picked: &Path) -> Option<PathBuf> {
    if picked.file_name().is_some_and(|n| n == "shots") && picked.is_dir() {
        return Some(picked.into());
    }
    let s = picked.join("shots");
    if s.is_dir() {
        return Some(s);
    }
    let up = picked.parent()?;
    (up.file_name().is_some_and(|n| n == "shots") && up.is_dir()).then(|| up.into())
}

fn link(app: &AppHandle) -> State<'_, CaptureLink<tauri::Wry>> {
    app.state::<CaptureLink<tauri::Wry>>()
}

/// Whether the Capture link exists in this build (iPhone and iPad only).
#[tauri::command]
pub fn capture_link_available() -> bool {
    cfg!(target_os = "ios")
}

/// Writes one cave's shared stations for Capture. `None` when it was unchanged since the last
/// write (nothing written), else the bytes written.
#[tauri::command]
pub async fn capture_share_stations(
    app: AppHandle,
    shared: State<'_, Shared>,
    cave_id: String,
    json: String,
) -> Result<Option<usize>, String> {
    if !valid_cave_id(&cave_id) {
        return Err("that is not a cave id".into());
    }
    if json.len() > MAX_STATIONS_JSON {
        return Err(format!(
            "stations too large to share ({} bytes)",
            json.len()
        ));
    }
    let h = fnv(&json);
    if shared.0.lock().unwrap().get(&cave_id) == Some(&h) {
        return Ok(None);
    }
    let handle = app.clone();
    let id = cave_id.clone();
    let n = tauri::async_runtime::spawn_blocking(move || link(&handle).write_stations(&id, &json))
        .await
        .map_err(|e| e.to_string())??;
    shared.0.lock().unwrap().insert(cave_id, h);
    Ok(Some(n))
}

/// Deletes one cave's shared stations (the cave was removed from this device).
#[tauri::command]
pub async fn capture_forget_stations(
    app: AppHandle,
    shared: State<'_, Shared>,
    cave_id: String,
) -> Result<(), String> {
    if !valid_cave_id(&cave_id) {
        return Err("that is not a cave id".into());
    }
    shared.0.lock().unwrap().remove(&cave_id);
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || link(&handle).delete_stations(&cave_id))
        .await
        .map_err(|e| e.to_string())?
}

/// Where the link to Capture's shots stands.
#[derive(Debug, Default, Serialize, PartialEq)]
pub struct LinkState {
    /// Capture's shots folder can be read.
    pub linked: bool,
    /// The remembered folder went stale or no longer opens.
    pub stale: bool,
    /// What a person should be told, or empty.
    pub problem: String,
}

const NO_SHOTS: &str =
    "That folder has no shots folder: pick Capture's folder (On My iPhone › Capture).";

fn state_of(path: Option<String>, stale: bool, linked: &Linked) -> LinkState {
    let root = path.as_deref().map(Path::new).and_then(shots_root);
    let problem = match (&path, &root) {
        (Some(_), None) => NO_SHOTS.to_string(),
        (None, _) if stale => {
            "Capture's folder can no longer be opened: link it again.".to_string()
        }
        _ => String::new(),
    };
    let ok = root.is_some();
    *linked.0.lock().unwrap() = root;
    LinkState {
        linked: ok,
        stale,
        problem,
    }
}

/// Resolves the remembered Capture folder (call on every app focus).
#[tauri::command]
pub async fn capture_link_state(
    app: AppHandle,
    linked: State<'_, Linked>,
) -> Result<LinkState, String> {
    if !cfg!(target_os = "ios") {
        return Ok(LinkState::default());
    }
    let handle = app.clone();
    let r = tauri::async_runtime::spawn_blocking(move || link(&handle).resolve_folder())
        .await
        .map_err(|e| e.to_string())??;
    Ok(state_of(r.path, r.stale, &linked))
}

/// Shows the Files picker to link Capture's folder. A cancel leaves the old link as it was.
#[tauri::command]
pub async fn capture_link(app: AppHandle, linked: State<'_, Linked>) -> Result<LinkState, String> {
    let handle = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || link(&handle).pick_folder())
        .await
        .map_err(|e| e.to_string())??;
    match picked {
        None => Ok(LinkState {
            linked: linked.0.lock().unwrap().is_some(),
            ..LinkState::default()
        }),
        Some(p) => Ok(state_of(Some(p), false, &linked)),
    }
}

/// A station as the web app passes it (`engine.captureStations().stations`).
#[derive(Clone, Debug, Deserialize)]
pub struct St {
    /// Id text.
    pub id: String,
    /// Label.
    pub label: String,
    /// East, metres.
    pub x: f64,
    /// North, metres.
    pub y: f64,
    /// Up, metres.
    pub z: f64,
}

/// One shot file in the cave's folder.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShotRow {
    /// File name inside the cave's folder.
    pub file: String,
    /// The shot's id (empty when unreadable).
    pub shot_id: String,
    /// Its station's label (as the cave has it, or as typed while waiting).
    pub label: String,
    /// When it was captured (ms since 1970).
    pub captured_at_ms: i64,
    /// File modified time (ms since 1970).
    pub modified_ms: u64,
    /// `placed`, `waiting` (for a station the cave does not have), `gone` (its station is gone or
    /// it cannot be placed) or `unreadable`.
    pub status: &'static str,
    /// Where its place came from (`chain`, `card+compass`, `compass`, `tap`, ...).
    pub source: &'static str,
    /// The station the room is drawn relative to, so it follows that station in the view; `None`
    /// for a chained shot whose own station the cave lacks (drawn at `anchor`).
    pub station_id: Option<String>,
    /// The anchor position its points are relative to (adjusted survey metres).
    pub anchor: Option<[f64; 3]>,
    /// Survey-axis capture points to survey: `[yaw rad, shift x, y, z]`.
    pub rigid: Option<[f64; 4]>,
    /// Mesh vertices.
    pub vertices: usize,
}

fn ms(t: std::io::Result<std::time::SystemTime>) -> u64 {
    t.ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as u64)
}

fn mm(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn map_file(p: &Path) -> Result<memmap2::Mmap, String> {
    let f = fs::File::open(p).map_err(|e| e.to_string())?;
    if f.metadata().map_err(|e| e.to_string())?.len() == 0 {
        return Err("empty file".into());
    }
    // SAFETY: Capture writes a shot file whole (atomic replace); a file changed under us at worst
    // reads as a bad shot, which every reader here refuses.
    unsafe { memmap2::Mmap::map(&f) }.map_err(|e| e.to_string())
}

/// The cave's folder inside `root`, refusing an unsafe cave id.
fn cave_dir(root: &Path, cave_id: &str) -> Result<PathBuf, String> {
    if !valid_cave_id(cave_id) {
        return Err("that is not a cave id".into());
    }
    Ok(root.join(cave_id))
}

/// Every `.cvcap` in the cave's folder, placed as Capture places it: all of the cave's readable
/// shots chained together ([`chains`], no legs, as Capture's `cc_chains` is called), the rest
/// placed alone. Rows sorted by capture time.
pub fn list_shots(root: &Path, cave_id: &str, stations: &[St]) -> Result<Vec<ShotRow>, String> {
    let dir = cave_dir(root, cave_id)?;
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.to_string()),
    };
    // (file, created, modified)
    let mut files: Vec<(String, u64, u64)> = Vec::new();
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || !name.ends_with(".cvcap") {
            continue;
        }
        let Ok(md) = e.metadata() else { continue };
        if !md.is_file() {
            continue;
        }
        files.push((name, ms(md.created()), ms(md.modified())));
    }
    // Capture's order: newest made first, then by name (ShotFiles.list)
    files.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let refs: Vec<StationRef> = stations
        .iter()
        .map(|s| StationRef {
            id: &s.id,
            label: &s.label,
            pos: Vec3::new(s.x, s.y, s.z),
        })
        .collect();
    let heads: Vec<Result<(ShotHeader, usize), String>> = files
        .iter()
        .map(|(name, _, _)| {
            let m = map_file(&dir.join(name))?;
            let info = check_header(&m).map_err(|e| format!("{e:?}"))?;
            if info.header.cave_id != cave_id {
                return Err("another cave's shot".into());
            }
            Ok((info.header, info.vertices))
        })
        .collect();
    let readable: Vec<usize> = (0..heads.len()).filter(|&i| heads[i].is_ok()).collect();
    let hs: Vec<ShotHeader> = readable
        .iter()
        .map(|&i| heads[i].as_ref().expect("readable").0.clone())
        .collect();
    let c = chains(&hs, &refs, &[]);

    let mut rows: Vec<ShotRow> = files
        .iter()
        .zip(&heads)
        .map(|((name, _, modified), h)| ShotRow {
            file: name.clone(),
            shot_id: String::new(),
            label: String::new(),
            captured_at_ms: 0,
            modified_ms: *modified,
            status: "unreadable",
            source: "unplaced",
            station_id: None,
            anchor: None,
            rigid: None,
            vertices: h.as_ref().map_or(0, |x| x.1),
        })
        .collect();
    for (k, p) in c.shots.iter().enumerate() {
        let h = &hs[k];
        let row = &mut rows[readable[k]];
        let found = station_for(h, &refs).map(|(s, _)| s);
        row.shot_id = h.shot_id.clone();
        row.captured_at_ms = h.captured_at_ms;
        row.label = found.map_or(h.station_label.clone(), |s| refs[s].label.to_string());
        row.source = p.source.name();
        let placed = matches!(
            p.source,
            Source::Chain | Source::CardCompass | Source::Compass | Source::Tap
        );
        match (placed, p.to_survey) {
            (true, Some(m)) => {
                row.status = "placed";
                row.rigid = Some([m.yaw, m.shift.x, m.shift.y, m.shift.z]);
                match found {
                    Some(s) => {
                        let at = refs[s].pos;
                        row.station_id = Some(refs[s].id.to_string());
                        row.anchor = Some([at.x, at.y, at.z]);
                    }
                    None => {
                        // chained over a station the cave lacks: its placed spot, to the mm
                        let at = m.apply(capture_to_survey(p.station_point));
                        row.anchor = Some([mm(at.x), mm(at.y), mm(at.z)]);
                    }
                }
            }
            _ => row.status = if h.is_waiting() { "waiting" } else { "gone" },
        }
    }
    rows.sort_by(|a, b| {
        a.captured_at_ms
            .cmp(&b.captured_at_ms)
            .then_with(|| a.file.cmp(&b.file))
    });
    Ok(rows)
}

/// A placed shot's points: `rigid` (from its row) applied to the survey-axis capture points,
/// minus `anchor`, thinned on a `cell_m` grid; x y z f32 per point.
pub fn shot_points(
    root: &Path,
    cave_id: &str,
    file: &str,
    rigid: [f64; 4],
    anchor: [f64; 3],
    cell_m: f32,
) -> Result<Vec<f32>, String> {
    if !file.ends_with(".cvcap") || file.contains(['/', '\\']) {
        return Err("that is not a shot file".into());
    }
    let dir = cave_dir(root, cave_id)?;
    let path = crate::files::safe_join(&dir, file).ok_or("that is not a shot file")?;
    let m = map_file(&path)?;
    let (info, positions) = mesh_positions(&m).map_err(|e| format!("{e:?}"))?;
    if info.header.cave_id != cave_id {
        return Err("another cave's shot".into());
    }
    let r = Rigid {
        yaw: rigid[0],
        shift: Vec3::new(rigid[1], rigid[2], rigid[3]),
    };
    let mut rel = Vec::with_capacity(positions.len());
    for c in positions.as_chunks::<3>().0 {
        let q = r.apply(capture_to_survey([c[0] as f64, c[1] as f64, c[2] as f64]));
        rel.extend_from_slice(&[
            (q.x - anchor[0]) as f32,
            (q.y - anchor[1]) as f32,
            (q.z - anchor[2]) as f32,
        ]);
    }
    Ok(thin_points(&rel, cell_m))
}

fn root_of(linked: &Linked) -> Result<PathBuf, String> {
    linked
        .0
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "Capture's folder is not linked".to_string())
}

/// Lists and places the open cave's shots (see [`list_shots`]).
#[tauri::command]
pub async fn capture_shots(
    linked: State<'_, Linked>,
    cave_id: String,
    stations: Vec<St>,
) -> Result<Vec<ShotRow>, String> {
    let root = root_of(&linked)?;
    tauri::async_runtime::spawn_blocking(move || list_shots(&root, &cave_id, &stations))
        .await
        .map_err(|e| e.to_string())?
}

/// One placed shot's thinned points as little-endian f32 bytes (see [`shot_points`]).
#[tauri::command]
pub async fn capture_shot_points(
    linked: State<'_, Linked>,
    cave_id: String,
    file: String,
    rigid: [f64; 4],
    anchor: [f64; 3],
    cell_m: f32,
) -> Result<Response, String> {
    let root = root_of(&linked)?;
    let pts = tauri::async_runtime::spawn_blocking(move || {
        shot_points(&root, &cave_id, &file, rigid, anchor, cell_m)
    })
    .await
    .map_err(|e| e.to_string())??;
    let mut bytes = Vec::with_capacity(pts.len() * 4);
    for v in pts {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    Ok(Response::new(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cave_capture::{Shot, encode, placement, relative_points};

    const COMPASS: &str = r#"{"shot_id":"s1","cave_id":"cave1","station_id":"a","station_label":"A1","captured_at_ms":2000,"device":"t",
        "instrument_height_m":0.3,"start_camera":[0,0,0],"taps":[],
        "compass":{"true_north":true,"accuracy_deg":6.0},
        "tracking":"normal","arkit_version":"26.1","gravity":[0,-1,0]}"#;

    const POS: [f32; 12] = [
        0.0, 0.0, -5.0, 1.0, 0.0, -5.0, 0.0, 1.0, -5.0, 0.01, 0.0, -5.0,
    ];

    fn shot(edit: impl FnOnce(&mut ShotHeader)) -> Vec<u8> {
        let mut header: ShotHeader = serde_json::from_str(COMPASS).unwrap();
        edit(&mut header);
        encode(&Shot {
            header,
            positions: POS.to_vec(),
            normals: vec![0.0; POS.len()],
            indices: vec![0, 1, 2],
            keyframes: vec![],
        })
        .unwrap()
    }

    fn stations() -> Vec<St> {
        vec![
            St {
                id: "a".into(),
                label: "A1".into(),
                x: 500_000.0,
                y: 4_000_000.0,
                z: 300.0,
            },
            St {
                id: "b".into(),
                label: "A2".into(),
                x: 500_010.0,
                y: 4_000_000.0,
                z: 300.0,
            },
        ]
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cavecad-capture-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d.join("cave1")).unwrap();
        d
    }

    #[test]
    fn capture_cave_ids_are_checked() {
        assert!(valid_cave_id("01JCAVE_x-9"));
        assert!(!valid_cave_id(""));
        assert!(!valid_cave_id("../x"));
        assert!(!valid_cave_id("a/b"));
        assert!(!valid_cave_id(&"a".repeat(65)));
        let root = tmp("ids");
        assert!(list_shots(&root, "../x", &stations()).is_err());
    }

    #[test]
    fn capture_list_places_waits_and_flags() {
        let root = tmp("list");
        let dir = root.join("cave1");
        fs::write(dir.join("s1.cvcap"), shot(|_| {})).unwrap();
        fs::write(
            dir.join("s2.cvcap"),
            shot(|h| {
                h.shot_id = "s2".into();
                h.station_id.clear();
                h.station_label = " a 2".into();
                h.captured_at_ms = 1000;
            }),
        )
        .unwrap();
        fs::write(
            dir.join("s3.cvcap"),
            shot(|h| {
                h.shot_id = "s3".into();
                h.station_id.clear();
                h.station_label = "Z9".into();
                h.captured_at_ms = 3000;
            }),
        )
        .unwrap();
        fs::write(
            dir.join("s4.cvcap"),
            shot(|h| {
                h.shot_id = "s4".into();
                h.station_id = "gone".into();
                h.captured_at_ms = 4000;
            }),
        )
        .unwrap();
        fs::write(dir.join("junk.cvcap"), b"not a shot").unwrap();
        fs::write(dir.join("s1.thumb.png"), b"png").unwrap();
        let rows = list_shots(&root, "cave1", &stations()).unwrap();
        let got: Vec<_> = rows
            .iter()
            .map(|r| (r.shot_id.as_str(), r.status))
            .collect();
        assert_eq!(
            got,
            [
                ("", "unreadable"),
                ("s2", "placed"),
                ("s1", "placed"),
                ("s3", "waiting"),
                ("s4", "gone")
            ]
        );
        let s2 = &rows[1];
        assert_eq!(s2.label, "A2");
        assert_eq!(s2.station_id.as_deref(), Some("b"));
        assert_eq!(s2.anchor, Some([500_010.0, 4_000_000.0, 300.0]));
        assert_eq!(s2.vertices, 4);
        assert_eq!(s2.source, "compass");
        // another cave's folder is empty, not an error
        assert!(list_shots(&root, "cave2", &stations()).unwrap().is_empty());
    }

    #[test]
    fn capture_points_match_capture_placement() {
        let root = tmp("points");
        fs::write(root.join("cave1/s1.cvcap"), shot(|_| {})).unwrap();
        let rows = list_shots(&root, "cave1", &stations()).unwrap();
        let r = &rows[0];
        let (rigid, anchor) = (r.rigid.unwrap(), r.anchor.unwrap());
        let pts = shot_points(&root, "cave1", "s1.cvcap", rigid, anchor, 0.0).unwrap();
        assert_eq!(pts.len() % 3, 0);
        let h: ShotHeader = serde_json::from_str(COMPASS).unwrap();
        let st = Vec3::new(anchor[0], anchor[1], anchor[2]);
        let want = relative_points(&POS, &placement(&h, st, None).unwrap(), st);
        assert_eq!(pts.len(), want.len());
        for (a, b) in pts.iter().zip(&want) {
            assert!((a - b).abs() < 1e-4, "{pts:?} vs {want:?}");
        }
        // thinned: the two points 1 cm apart share a cell
        let thin = shot_points(&root, "cave1", "s1.cvcap", rigid, anchor, 0.08).unwrap();
        assert_eq!(thin, thin_points(&pts, 0.08));
        assert_eq!(thin.len(), 9);
    }

    #[test]
    fn capture_points_refuse_other_files() {
        let root = tmp("refuse");
        let r = [0.0; 4];
        let a = [0.0; 3];
        assert!(shot_points(&root, "cave1", "a/b.cvcap", r, a, 0.1).is_err());
        assert!(shot_points(&root, "cave1", "../x.cvcap", r, a, 0.1).is_err());
        assert!(shot_points(&root, "cave1", "s1.thumb.png", r, a, 0.1).is_err());
        assert!(shot_points(&root, "..", "s1.cvcap", r, a, 0.1).is_err());
    }

    #[test]
    fn shots_root_finds_capture_folder() {
        let root = tmp("root");
        fs::create_dir_all(root.join("shots/cave1")).unwrap();
        assert_eq!(shots_root(&root), Some(root.join("shots")));
        assert_eq!(shots_root(&root.join("shots")), Some(root.join("shots")));
        assert_eq!(
            shots_root(&root.join("shots/cave1")),
            Some(root.join("shots"))
        );
        assert_eq!(shots_root(&root.join("cave1")), None);
        let linked = Linked::default();
        let s = state_of(
            Some(root.join("cave1").to_string_lossy().into()),
            false,
            &linked,
        );
        assert!(!s.linked && s.problem.contains("no shots folder"));
        let s = state_of(Some(root.to_string_lossy().into()), false, &linked);
        assert!(s.linked && s.problem.is_empty());
        assert!(linked.0.lock().unwrap().is_some());
        let s = state_of(None, true, &linked);
        assert!(!s.linked && s.stale && s.problem.contains("link it again"));
    }
}
