//! C interface to `cave-capture` for the CaveCAD Capture iPhone app. Every function takes plain
//! pointers and returns a `CcBuf` the caller frees with `cc_free`. Inputs are never trusted: a
//! null or malformed argument gives an error JSON (or empty bytes), never a crash.
#![allow(unsafe_code)]

use cave_capture::{
    CardSeen, Keyframe, KeyframeRef, NewPass, PassMesh, Shot, ShotHeader, StationRef, add_pass,
    card_offsets, cards_fit, chains, check_header, decode, display_fix, document_from_room, encode,
    encode_parts, ghost, mesh_positions, placement, station_for, stations, thin_points,
};
use cave_survey::Vec3;
use serde::Deserialize;
use std::ffi::{CStr, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};

mod gimbal;

/// Bytes owned by Rust.
#[repr(C)]
pub struct CcBuf {
    /// Start.
    pub ptr: *mut u8,
    /// Length.
    pub len: usize,
}

fn give(v: Vec<u8>) -> CcBuf {
    let mut b = v.into_boxed_slice();
    let out = CcBuf {
        ptr: b.as_mut_ptr(),
        len: b.len(),
    };
    std::mem::forget(b);
    out
}

fn give_json(v: serde_json::Value) -> CcBuf {
    give(v.to_string().into_bytes())
}

fn err(msg: &str) -> CcBuf {
    give_json(serde_json::json!({ "error": msg }))
}

/// `Ok(None)` for NULL, `Err` for text that is not UTF-8.
unsafe fn text<'a>(p: *const c_char) -> Result<Option<&'a str>, ()> {
    if p.is_null() {
        Ok(None)
    } else {
        unsafe { CStr::from_ptr(p) }
            .to_str()
            .map(Some)
            .map_err(|_| ())
    }
}

/// Runs a body so a panic never crosses the C boundary; `bad` is what the caller gets instead.
fn guard(body: impl FnOnce() -> CcBuf, bad: fn() -> CcBuf) -> CcBuf {
    catch_unwind(AssertUnwindSafe(body)).unwrap_or_else(|_| bad())
}

fn internal() -> CcBuf {
    err("internal")
}

fn empty() -> CcBuf {
    give(Vec::new())
}

unsafe fn slice<'a, T>(p: *const T, n: usize) -> &'a [T] {
    if p.is_null() || n == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(p, n) }
    }
}

/// Releases a buffer returned by any `cc_` function.
///
/// # Safety
/// `buf` must come from a `cc_` function and be freed at most once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cc_free(buf: CcBuf) {
    if !buf.ptr.is_null() {
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(buf.ptr, buf.len)) });
    }
}

/// See `capture_core.h`.
///
/// # Safety
/// Every pointer must be null or valid for its stated length (strings NUL-terminated).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cc_stations(
    snapshot: *const c_char,
    entries_json: *const c_char,
) -> CcBuf {
    guard(
        || {
            let Ok(snap) = (unsafe { text(snapshot) }) else {
                return err("snapshot is not valid UTF-8");
            };
            let entries: Vec<String> = match unsafe { text(entries_json) } {
                Err(()) => return err("entries are not valid UTF-8"),
                Ok(None) => Vec::new(),
                Ok(Some(t)) => match serde_json::from_str(t) {
                    Ok(v) => v,
                    Err(_) => return err("entries are not a JSON array of texts"),
                },
            };
            let refs: Vec<&str> = entries.iter().map(String::as_str).collect();
            match document_from_room(snap, &refs) {
                Err(e) => err(&e),
                Ok(doc) => give_json(serde_json::Value::Array(
                    stations(&doc)
                        .into_iter()
                        .map(|s| serde_json::json!({ "id": s.id.to_string(), "label": s.label, "x": s.pos.x, "y": s.pos.y, "z": s.pos.z }))
                        .collect(),
                )),
            }
        },
        internal,
    )
}

#[derive(Deserialize)]
struct KeyIn {
    pose: Vec<f32>,
    intrinsics: Vec<f32>,
    width: u32,
    height: u32,
    jpeg_len: usize,
}

#[derive(Deserialize)]
struct HeaderIn {
    #[serde(flatten)]
    header: ShotHeader,
    keyframes: Vec<KeyIn>,
}

/// See `capture_core.h`.
///
/// # Safety
/// Every pointer must be null or valid for its stated length (strings NUL-terminated).
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn cc_shot_encode(
    header_json: *const c_char,
    positions: *const f32,
    n_positions: usize,
    normals: *const f32,
    n_normals: usize,
    indices: *const u32,
    n_indices: usize,
    jpegs: *const u8,
    n_jpegs: usize,
) -> CcBuf {
    guard(
        || {
            // Pointer/count pairs: a NULL pointer with a count is a caller bug, refuse it.
            if (positions.is_null() && n_positions > 0)
                || (normals.is_null() && n_normals > 0)
                || (indices.is_null() && n_indices > 0)
                || (jpegs.is_null() && n_jpegs > 0)
            {
                return empty();
            }
            let Ok(Some(htext)) = (unsafe { text(header_json) }) else {
                return empty();
            };
            let Ok(h) = serde_json::from_str::<HeaderIn>(htext) else {
                return empty();
            };
            // Everything is borrowed from the caller's buffers: the only copy made is the bundle itself.
            let jpeg_bytes = unsafe { slice(jpegs, n_jpegs) };
            let mut at = 0usize;
            let mut keyframes = Vec::with_capacity(h.keyframes.len());
            for k in &h.keyframes {
                let (Ok(pose), Ok(intrinsics)) = (
                    <[f32; 16]>::try_from(k.pose.as_slice()),
                    <[f32; 9]>::try_from(k.intrinsics.as_slice()),
                ) else {
                    return empty();
                };
                let Some(end) = at.checked_add(k.jpeg_len) else {
                    return empty();
                };
                let Some(jpeg) = jpeg_bytes.get(at..end) else {
                    return empty();
                };
                at = end;
                keyframes.push(KeyframeRef {
                    pose,
                    intrinsics,
                    width: k.width,
                    height: k.height,
                    jpeg,
                });
            }
            let bytes = match encode_parts(
                &h.header,
                unsafe { slice(positions, n_positions) },
                unsafe { slice(normals, n_normals) },
                unsafe { slice(indices, n_indices) },
                &keyframes,
            ) {
                Ok(b) => b,
                Err(_) => return empty(),
            };
            // encode_parts already refused any mesh decode would; re-reading the header (no copies)
            // is the check that what was written reads back.
            if check_header(&bytes).is_err() {
                return empty();
            }
            give(bytes)
        },
        empty,
    )
}

/// The shot placed over `st[0..3]`; the target `st[3..6]` is used only by an older tap shot
/// ([`cave_capture::placement`]).
fn place(shot: &Shot, st: &[f64]) -> Option<cave_capture::Placement> {
    let station = Vec3 {
        x: st[0],
        y: st[1],
        z: st[2],
    };
    let target = Vec3 {
        x: st[3],
        y: st[4],
        z: st[5],
    };
    placement(&shot.header, station, Some(target))
}

fn placed_points(shot: &Shot, st: &[f64]) -> Option<(Vec<Vec3>, f64)> {
    let p = place(shot, st)?;
    let pts: Vec<Vec3> = shot
        .positions
        .as_chunks::<3>()
        .0
        .iter()
        .map(|c| p.apply([c[0] as f64, c[1] as f64, c[2] as f64]))
        .collect();
    if pts.is_empty() {
        return None;
    }
    Some((pts, p.turn_deg))
}

/// See `capture_core.h`.
///
/// # Safety
/// Every pointer must be null or valid for its stated length (strings NUL-terminated).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cc_shot_summary(
    bundle: *const u8,
    len: usize,
    station_and_target: *const f64,
) -> CcBuf {
    guard(|| summary(bundle, len, station_and_target), internal)
}

fn summary(bundle: *const u8, len: usize, station_and_target: *const f64) -> CcBuf {
    let bytes = unsafe { slice(bundle, len) };
    // The header alone answers the counts and which stations the shot was taken from.
    let info = match check_header(bytes) {
        Ok(i) => i,
        Err(e) => return err(&format!("{e:?}")),
    };
    let mut out = serde_json::json!({
        "vertices": info.vertices,
        "triangles": info.triangles,
        "keyframes": info.keyframes,
        "passes": info.header.passes.len(),
        "placed": false,
        "station_label": info.header.station_label,
        // typed label the cave did not have: placed once Swift finds the label among the stations
        "waiting": info.header.is_waiting(),
        "station_id": info.header.station_id,
        "target_station_id": info.header.taps.first().map(|t| t.target_station_id.clone()),
        "compass": info.header.compass.map(|c| serde_json::json!({ "true_north": c.true_north, "accuracy_deg": c.accuracy_deg })),
    });
    let st = unsafe { slice(station_and_target, 6) };
    if st.len() != 6 {
        return give_json(out);
    }
    let shot = match decode(bytes) {
        Ok(s) => s,
        Err(e) => return err(&format!("{e:?}")),
    };
    if let Some((pts, turn)) = placed_points(&shot, st) {
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for q in &pts {
            for (i, v) in [q.x, q.y, q.z].into_iter().enumerate() {
                min[i] = min[i].min(v);
                max[i] = max[i].max(v);
            }
        }
        out["placed"] = true.into();
        out["min"] = serde_json::json!(min);
        out["max"] = serde_json::json!(max);
        out["turn_deg"] = turn.into();
        // Cards seen, placed like the mesh; points RELATIVE TO THE STATION (as cc_shot_placed_positions).
        if let Some(p) = place(&shot, st) {
            let cards: Vec<_> = shot
                .header
                .cards
                .iter()
                .map(|c| {
                    let q = p.apply(c.point);
                    serde_json::json!({
                        "id": c.id, "word": c.word,
                        "point": [q.x - st[0], q.y - st[1], q.z - st[2]],
                        "sightings": c.sightings, "spread_m": c.spread_m,
                    })
                })
                .collect();
            out["cards"] = cards.into();
        }
    }
    give_json(out)
}

/// See `capture_core.h`.
///
/// # Safety
/// Every pointer must be null or valid for its stated length (strings NUL-terminated).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cc_shot_placed_positions(
    bundle: *const u8,
    len: usize,
    station_and_target: *const f64,
) -> CcBuf {
    guard(|| positions_of(bundle, len, station_and_target), empty)
}

fn positions_of(bundle: *const u8, len: usize, station_and_target: *const f64) -> CcBuf {
    give(le_bytes(&relative_points(bundle, len, station_and_target)))
}

/// The placed mesh points relative to the station (`station_and_target[0..3]`), x y z per point;
/// empty when the shot cannot be read or placed. Reads positions only (no photo is copied).
fn relative_points(bundle: *const u8, len: usize, station_and_target: *const f64) -> Vec<f32> {
    let st = unsafe { slice(station_and_target, 6) };
    if st.len() != 6 {
        return Vec::new();
    }
    let Ok((info, positions)) = mesh_positions(unsafe { slice(bundle, len) }) else {
        return Vec::new();
    };
    if positions.is_empty() {
        return Vec::new();
    }
    let station = Vec3 {
        x: st[0],
        y: st[1],
        z: st[2],
    };
    let target = Vec3 {
        x: st[3],
        y: st[4],
        z: st[5],
    };
    let Some(p) = placement(&info.header, station, Some(target)) else {
        return Vec::new();
    };
    cave_capture::relative_points(&positions, &p, station)
}

fn le_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

/// See `capture_core.h`.
///
/// # Safety
/// Every pointer must be null or valid for its stated length (strings NUL-terminated).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cc_shot_placed_points_thin(
    bundle: *const u8,
    len: usize,
    station_and_target: *const f64,
    cell_m: f32,
) -> CcBuf {
    guard(
        || {
            let pts = relative_points(bundle, len, station_and_target);
            give(le_bytes(&thin_points(&pts, cell_m)))
        },
        empty,
    )
}

#[derive(Deserialize)]
struct PassIn {
    captured_at_ms: i64,
    aligned_by: String,
    transform: Vec<f64>,
    replace: bool,
    tracking: String,
    #[serde(default)]
    cards: Vec<CardSeen>,
    #[serde(default)]
    flags: Vec<String>,
    keyframes: Vec<KeyIn>,
}

/// See `capture_core.h`.
///
/// # Safety
/// Every pointer must be null or valid for its stated length (strings NUL-terminated).
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn cc_shot_add_pass(
    bundle: *const u8,
    len: usize,
    pass_json: *const c_char,
    positions: *const f32,
    n_positions: usize,
    normals: *const f32,
    n_normals: usize,
    indices: *const u32,
    n_indices: usize,
    jpegs: *const u8,
    n_jpegs: usize,
) -> CcBuf {
    guard(
        || {
            if (positions.is_null() && n_positions > 0)
                || (normals.is_null() && n_normals > 0)
                || (indices.is_null() && n_indices > 0)
                || (jpegs.is_null() && n_jpegs > 0)
            {
                return empty();
            }
            let Ok(old) = decode(unsafe { slice(bundle, len) }) else {
                return empty();
            };
            let Ok(Some(t)) = (unsafe { text(pass_json) }) else {
                return empty();
            };
            let Ok(p) = serde_json::from_str::<PassIn>(t) else {
                return empty();
            };
            let Ok(transform) = <[f64; 16]>::try_from(p.transform.as_slice()) else {
                return empty();
            };
            let jpeg_bytes = unsafe { slice(jpegs, n_jpegs) };
            let mut at = 0usize;
            let mut keyframes = Vec::with_capacity(p.keyframes.len());
            for k in &p.keyframes {
                let (Ok(pose), Ok(intrinsics)) = (
                    <[f32; 16]>::try_from(k.pose.as_slice()),
                    <[f32; 9]>::try_from(k.intrinsics.as_slice()),
                ) else {
                    return empty();
                };
                let Some(end) = at.checked_add(k.jpeg_len) else {
                    return empty();
                };
                let Some(jpeg) = jpeg_bytes.get(at..end) else {
                    return empty();
                };
                at = end;
                keyframes.push(Keyframe {
                    pose,
                    intrinsics,
                    width: k.width,
                    height: k.height,
                    jpeg: jpeg.to_vec(),
                });
            }
            let new = NewPass {
                captured_at_ms: p.captured_at_ms,
                aligned_by: p.aligned_by,
                transform,
                mesh: if p.replace {
                    PassMesh::Replace
                } else {
                    PassMesh::Add
                },
                positions: unsafe { slice(positions, n_positions) }.to_vec(),
                normals: unsafe { slice(normals, n_normals) }.to_vec(),
                indices: unsafe { slice(indices, n_indices) }.to_vec(),
                keyframes,
                cards: p.cards,
                tracking: p.tracking,
                flags: p.flags,
            };
            let Ok(shot) = add_pass(&old, new) else {
                return empty();
            };
            match encode(&shot) {
                Ok(b) if check_header(&b).is_ok() => give(b),
                _ => empty(),
            }
        },
        empty,
    )
}

/// See `capture_core.h`.
///
/// # Safety
/// `bundle` must be null or valid for `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cc_shot_ghost(
    bundle: *const u8,
    len: usize,
    cell_m: f32,
    min_outline_m: f32,
) -> CcBuf {
    guard(
        || {
            let Ok(shot) = decode(unsafe { slice(bundle, len) }) else {
                return empty();
            };
            let g = ghost(&shot.positions, &shot.indices, cell_m, min_outline_m);
            let mut out = Vec::new();
            for w in [
                g.positions.len() as u32,
                g.indices.len() as u32,
                g.outlines.len() as u32,
            ] {
                out.extend_from_slice(&w.to_le_bytes());
            }
            for f in &g.positions {
                out.extend_from_slice(&f.to_le_bytes());
            }
            for i in &g.indices {
                out.extend_from_slice(&i.to_le_bytes());
            }
            for o in &g.outlines {
                out.extend_from_slice(&(o.len() as u32).to_le_bytes());
                for p in o {
                    for f in p {
                        out.extend_from_slice(&f.to_le_bytes());
                    }
                }
            }
            give(out)
        },
        empty,
    )
}

/// See `capture_core.h`.
///
/// # Safety
/// `bundle` valid for `len` bytes; `cards_json` NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cc_cards_fit(
    bundle: *const u8,
    len: usize,
    cards_json: *const c_char,
    same_session: u8,
) -> CcBuf {
    guard(
        || {
            let Ok(shot) = decode(unsafe { slice(bundle, len) }) else {
                return err("unreadable shot");
            };
            let Ok(Some(t)) = (unsafe { text(cards_json) }) else {
                return err("cards are not text");
            };
            let Ok(new) = serde_json::from_str::<Vec<CardSeen>>(t) else {
                return err("cards are not JSON");
            };
            let pairs = |v: &[(u32, f64)]| {
                v.iter()
                    .map(|(id, m)| serde_json::json!({"id": id, "m": m}))
                    .collect::<Vec<_>>()
            };
            if same_session != 0 {
                return give_json(
                    serde_json::json!({"offsets": pairs(&card_offsets(&shot.header.cards, &new))}),
                );
            }
            give_json(
                serde_json::json!({"fit": cards_fit(&shot.header.cards, &new).map(|f| {
                    serde_json::json!({
                        "transform": f.transform, "yaw_rad": f.yaw_rad, "worst_m": f.worst_m,
                        "ok": f.ok, "worst_ratio": f.worst_ratio,
                        "misfits": pairs(&f.misfits), "limits": pairs(&f.limits)
                    })
                })}),
            )
        },
        internal,
    )
}

#[derive(Deserialize)]
struct StationIn {
    id: String,
    label: String,
    x: f64,
    y: f64,
    z: f64,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StationsIn {
    Plain(Vec<StationIn>),
    WithLegs {
        stations: Vec<StationIn>,
        #[serde(default)]
        legs: Vec<(String, String)>,
    },
}

fn r3(v: Vec3) -> [f64; 3] {
    [v.x, v.y, v.z]
}

/// The chain result as JSON (see `capture_core.h`, `cc_chains`). `heads[i]` is `None` for a shot
/// that could not be read.
fn chains_json(
    heads: &[Result<ShotHeader, String>],
    stations: &[StationIn],
    legs: &[(String, String)],
) -> serde_json::Value {
    use cave_capture::chain::{Flag, Source};
    let refs: Vec<StationRef> = stations
        .iter()
        .map(|s| StationRef {
            id: &s.id,
            label: &s.label,
            pos: Vec3::new(s.x, s.y, s.z),
        })
        .collect();
    // chain the readable shots; map back to input order
    let readable: Vec<usize> = (0..heads.len()).filter(|&i| heads[i].is_ok()).collect();
    let hs: Vec<ShotHeader> = readable
        .iter()
        .map(|&i| heads[i].clone().expect("readable"))
        .collect();
    let c = chains(&hs, &refs, legs);
    let label_of = |k: usize| -> String {
        station_for(&hs[k], &refs)
            .map(|(s, _)| refs[s].label.to_string())
            .unwrap_or_else(|| hs[k].station_label.clone())
    };
    let zero = Vec3::new(0.0, 0.0, 0.0);
    let mut out: Vec<serde_json::Value> = heads
        .iter()
        .enumerate()
        .map(|(i, h)| match h {
            Err(e) => {
                serde_json::json!({ "index": i, "error": e, "source": "unplaced", "placed": false })
            }
            Ok(_) => serde_json::Value::Null,
        })
        .collect();
    let (mut by_chain, mut flagged) = (0usize, 0usize);
    for (k, p) in c.shots.iter().enumerate() {
        let h = &hs[k];
        let found = station_for(h, &refs);
        // the pair a viewer places with the old way: its station (and target), or the origin
        let station = found.map_or(zero, |(s, _)| refs[s].pos);
        let target = match found.and_then(|(_, t)| t) {
            Some(t) => refs[t].pos,
            None if h.compass.is_some() => station,
            // a tap shot with no target: any sideways point lets the old placement run; the fix corrects it
            None => station.plus(Vec3::new(1.0, 0.0, 0.0)),
        };
        let usable = placement(h, station, Some(target)).is_some();
        let (pair, fix, spot) = match (p.to_survey, usable) {
            (Some(m), true) => {
                let fix = display_fix(h, &m, station, Some(target));
                let spot = m
                    .apply(cave_capture::capture_to_survey(p.station_point))
                    .minus(station);
                (true, fix, Some(spot))
            }
            // waiting compass shot in no chain: drawn around its own spot, as before
            (None, true) if p.source == Source::Waiting && h.compass.is_some() => {
                (true, Some(cave_capture::Rigid::IDENTITY), Some(zero))
            }
            _ => (false, None, None),
        };
        if p.source == Source::Chain {
            by_chain += 1;
        }
        if !p.flags.is_empty() {
            flagged += 1;
        }
        let flags: Vec<serde_json::Value> = p
            .flags
            .iter()
            .map(|f| match *f {
                Flag::LegMisfit { other, lidar_m, survey_m, off_m, limit_m } => serde_json::json!({
                    "kind": "leg_misfit", "with": label_of(other),
                    "lidar_m": lidar_m, "survey_m": survey_m, "off_m": off_m, "limit_m": limit_m }),
                Flag::CardsMisfit { other, residual_m, limit_m } => serde_json::json!({
                    "kind": "cards_misfit", "with": label_of(other), "off_m": residual_m, "limit_m": limit_m }),
                Flag::OnePoint { other } => serde_json::json!({ "kind": "one_point", "with": label_of(other) }),
            })
            .collect();
        out[readable[k]] = serde_json::json!({
            "index": readable[k],
            "shot_id": h.shot_id,
            "label": label_of(k),
            "source": p.source.name(),
            "placed": matches!(p.source, Source::Chain | Source::CardCompass | Source::Compass | Source::Tap) && p.to_survey.is_some(),
            "chain": p.chain,
            "pair": pair.then_some([station.x, station.y, station.z, target.x, target.y, target.z]),
            "fix": fix.map(|f| serde_json::json!({ "yaw_deg": f.yaw_deg(), "shift": r3(f.shift) })),
            "spot": spot.map(r3),
            "turn_deg": p.to_survey.map(|m| m.placement([0.0; 3]).turn_deg),
            "joined": p.joined.iter().map(|&j| label_of(j)).collect::<Vec<_>>(),
            "one_point": p.one_point(),
            "misfit": p.misfit(),
            "flags": flags,
        });
    }
    let chain_list: Vec<serde_json::Value> = c
        .chains
        .iter()
        .enumerate()
        .map(|(k, ch)| {
            serde_json::json!({
                "k": k,
                "shots": ch.shots.iter().map(|&m| readable[m]).collect::<Vec<_>>(),
                "labels": ch.shots.iter().map(|&m| label_of(m)).collect::<Vec<_>>(),
                "placed": ch.placed,
            })
        })
        .collect();
    serde_json::json!({
        "shots": out,
        "chains": chain_list,
        "counts": { "shots": heads.len(), "chains": c.chains.len(), "by_chain": by_chain, "flagged": flagged },
    })
}

/// See `capture_core.h`.
///
/// # Safety
/// `stations_json` NUL-terminated or null; `bundles` and `lens` null or valid for `n` entries, each
/// bundle pointer null or valid for its length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cc_chains(
    stations_json: *const c_char,
    bundles: *const *const u8,
    lens: *const usize,
    n: usize,
) -> CcBuf {
    guard(
        || {
            let (stations, legs) = match unsafe { text(stations_json) } {
                Err(()) => return err("stations are not valid UTF-8"),
                Ok(None) => (Vec::new(), Vec::new()),
                Ok(Some(t)) => match serde_json::from_str::<StationsIn>(t) {
                    Ok(StationsIn::Plain(s)) => (s, Vec::new()),
                    Ok(StationsIn::WithLegs { stations, legs }) => (stations, legs),
                    Err(_) => return err("stations are not JSON"),
                },
            };
            if (bundles.is_null() || lens.is_null()) && n > 0 {
                return err("no bundles");
            }
            let ptrs = unsafe { slice(bundles, n) };
            let lens = unsafe { slice(lens, n) };
            // headers only: no mesh or photo is copied
            let heads: Vec<Result<ShotHeader, String>> = ptrs
                .iter()
                .zip(lens)
                .map(|(&p, &l)| {
                    if p.is_null() && l > 0 {
                        return Err("null bundle".to_string());
                    }
                    check_header(unsafe { slice(p, l) })
                        .map(|i| i.header)
                        .map_err(|e| format!("{e:?}"))
                })
                .collect();
            give_json(chains_json(&heads, &stations, &legs))
        },
        internal,
    )
}

// ---- the gimbal (one runner; see gimbal.rs). The doors only queue commands or flip flags and return at once.

/// Looks for the gimbal for `timeout_ms` and connects; poll `cc_gimbal_status`.
#[unsafe(no_mangle)]
pub extern "C" fn cc_gimbal_connect(timeout_ms: u32) {
    let _ = catch_unwind(|| gimbal::connect(timeout_ms));
}

/// Queues a plan: `[[{"yaw","roll","pitch","seconds"}, ...], ...]`, each list 1 to 15 targets. `{"ok":true}` or
/// `{"error"}` for a plan that is refused.
///
/// # Safety
/// `plan_json` is NUL-terminated UTF-8 or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cc_gimbal_run(plan_json: *const c_char) -> CcBuf {
    guard(
        || match unsafe { text(plan_json) } {
            Ok(Some(t)) => match gimbal::run_plan(t) {
                Ok(()) => give_json(serde_json::json!({"ok": true})),
                Err(e) => err(&e),
            },
            _ => err("plan is not text"),
        },
        internal,
    )
}

/// Pauses the running plan on its next reached target (target mode kept alive).
#[unsafe(no_mangle)]
pub extern "C" fn cc_gimbal_hold() {
    let _ = catch_unwind(gimbal::hold);
}

/// Ends a hold.
#[unsafe(no_mangle)]
pub extern "C" fn cc_gimbal_go_on() {
    let _ = catch_unwind(gimbal::go_on);
}

/// Stops the running plan at its next reached target and sends the gimbal home.
#[unsafe(no_mangle)]
pub extern "C" fn cc_gimbal_stop_home() {
    let _ = catch_unwind(gimbal::stop_home);
}

/// Disconnects the gimbal (after a running plan ends).
#[unsafe(no_mangle)]
pub extern "C" fn cc_gimbal_disconnect() {
    let _ = catch_unwind(gimbal::disconnect);
}

/// The gimbal status JSON.
#[unsafe(no_mangle)]
pub extern "C" fn cc_gimbal_status() -> CcBuf {
    guard(|| give(gimbal::status().into_bytes()), internal)
}

/// Target lists as the JSON `cc_gimbal_run` takes.
fn lists_json(lists: Vec<Vec<cave_osmo::targets::Target>>) -> serde_json::Value {
    serde_json::Value::Array(
        lists
            .into_iter()
            .map(|l| {
                serde_json::Value::Array(
                    l.into_iter()
                        .map(|t| serde_json::json!({ "yaw": t.yaw, "roll": t.roll, "pitch": t.pitch, "seconds": t.seconds }))
                        .collect(),
                )
            })
            .collect(),
    )
}

/// The slow sweep of the whole camera window at `speed` degrees per second, as a gimbal plan (empty list for a
/// non-positive speed).
#[unsafe(no_mangle)]
pub extern "C" fn cc_camera_sweep(speed: f64) -> CcBuf {
    guard(
        || {
            give_json(lists_json(cave_osmo::camera::sweep(
                cave_osmo::camera::OM7_CAMERA,
                speed,
            )))
        },
        internal,
    )
}

/// Aims at missing directions: `missing_json` is `[[yaw, pitch], ...]` degrees relative to home. The plan keeps
/// the reachable ones, ending home (empty list when none).
///
/// # Safety
/// `missing_json` is NUL-terminated UTF-8 or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cc_camera_fill(missing_json: *const c_char, speed: f64) -> CcBuf {
    guard(
        || {
            let dirs: Vec<(f64, f64)> = match unsafe { text(missing_json) } {
                Ok(None) => Vec::new(),
                Ok(Some(t)) => match serde_json::from_str(t) {
                    Ok(v) => v,
                    Err(_) => return err("missing is not a JSON array of [yaw, pitch]"),
                },
                Err(()) => return err("missing is not valid UTF-8"),
            };
            give_json(lists_json(cave_osmo::camera::fill(
                &dirs,
                cave_osmo::camera::OM7_CAMERA,
                speed,
            )))
        },
        internal,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    fn read(b: CcBuf) -> Vec<u8> {
        let v = unsafe { std::slice::from_raw_parts(b.ptr, b.len) }.to_vec();
        unsafe { cc_free(b) };
        v
    }

    #[test]
    fn stations_json() {
        let doc = cave_fixture::from_shot_list(
            r#"{"unit":"m","name":"T","shots":[{"from":"A","to":"B","distance":10,"azimuth":90,"inclination":0}]}"#,
            cave_doc::ActorId(1),
        )
        .unwrap();
        let snap = CString::new(format!(
            r#"{{"upTo":1,"ops":{}}}"#,
            serde_json::to_string(&doc.snapshot()).unwrap()
        ))
        .unwrap();
        let out: serde_json::Value = serde_json::from_slice(&read(unsafe {
            cc_stations(snap.as_ptr(), std::ptr::null())
        }))
        .unwrap();
        assert_eq!(out[1]["label"], "B");
        assert!((out[1]["x"].as_f64().unwrap() - 10.0).abs() < 1e-6);
    }

    #[test]
    fn bad_entries_give_error_json() {
        let bad = CString::new("nope").unwrap();
        let out: serde_json::Value = serde_json::from_slice(&read(unsafe {
            cc_stations(std::ptr::null(), bad.as_ptr())
        }))
        .unwrap();
        assert!(out["error"].is_string());
    }

    #[test]
    fn encode_then_summarize() {
        let header = CString::new(
            r#"{"shot_id":"s","cave_id":"c","station_id":"a","station_label":"A","captured_at_ms":0,"device":"t",
               "instrument_height_m":0.0,"start_camera":[0,0,0],
               "taps":[{"target_station_id":"b","target_label":"B","origin":[0,0,0],"dir":[0,0,-1]}],
               "tracking":"normal","arkit_version":"26.1","gravity":[0,-1,0],"keyframes":[{"pose":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"intrinsics":[0,0,0,0,0,0,0,0,0],"width":1,"height":1,"jpeg_len":2}]}"#,
        )
        .unwrap();
        let pos = [0f32, 0.0, -5.0, 1.0, 0.0, -5.0, 0.0, 1.0, -5.0];
        let nor = [0f32; 9];
        let idx = [0u32, 1, 2];
        let jpg = [0xffu8, 0xd8];
        let bundle = read(unsafe {
            cc_shot_encode(
                header.as_ptr(),
                pos.as_ptr(),
                9,
                nor.as_ptr(),
                9,
                idx.as_ptr(),
                3,
                jpg.as_ptr(),
                2,
            )
        });
        assert!(!bundle.is_empty());
        let st = [0.0, 0.0, 0.0, 10.0, 0.0, 0.0]; // target due east
        let sum: serde_json::Value = serde_json::from_slice(&read(unsafe {
            cc_shot_summary(bundle.as_ptr(), bundle.len(), st.as_ptr())
        }))
        .unwrap();
        assert_eq!(sum["vertices"], 3);
        assert_eq!(sum["placed"], true);
        assert!(
            (sum["min"][0].as_f64().unwrap() - 5.0).abs() < 1e-5,
            "{sum}"
        );
    }

    #[test]
    fn garbage_bundle_is_error_not_crash() {
        let junk = [1u8, 2, 3];
        let out: serde_json::Value = serde_json::from_slice(&read(unsafe {
            cc_shot_summary(junk.as_ptr(), 3, std::ptr::null())
        }))
        .unwrap();
        assert!(out["error"].is_string());
    }

    #[test]
    fn invalid_utf8_is_refused() {
        let bad = c"\xff\xfe";
        let out: serde_json::Value = serde_json::from_slice(&read(unsafe {
            cc_stations(bad.as_ptr(), std::ptr::null())
        }))
        .unwrap();
        assert!(out["error"].is_string());
        let out: serde_json::Value = serde_json::from_slice(&read(unsafe {
            cc_stations(std::ptr::null(), bad.as_ptr())
        }))
        .unwrap();
        assert!(out["error"].is_string());
        let none = std::ptr::null();
        assert!(
            read(unsafe {
                cc_shot_encode(
                    bad.as_ptr(),
                    none,
                    0,
                    none,
                    0,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    0,
                )
            })
            .is_empty()
        );
    }

    const BARE: &str = r#"{"shot_id":"s","cave_id":"c","station_id":"a","station_label":"A","captured_at_ms":0,"device":"t",
        "instrument_height_m":0.0,"start_camera":[0,0,0],
        "taps":[{"target_station_id":"b","target_label":"B","origin":[0,0,0],"dir":[0,0,-1]}],
        "tracking":"normal","arkit_version":"26.1","gravity":[0,-1,0],"keyframes":[]}"#;

    #[test]
    fn null_pointer_with_count_is_empty() {
        let header = CString::new(BARE).unwrap();
        let n = std::ptr::null();
        let out = read(unsafe {
            cc_shot_encode(
                header.as_ptr(),
                n,
                9,
                n,
                9,
                std::ptr::null(),
                0,
                std::ptr::null(),
                0,
            )
        });
        assert!(out.is_empty());
    }

    #[test]
    fn empty_mesh_is_not_placed() {
        let header = CString::new(BARE).unwrap();
        let n = std::ptr::null();
        let bundle = read(unsafe {
            cc_shot_encode(
                header.as_ptr(),
                n,
                0,
                n,
                0,
                std::ptr::null(),
                0,
                std::ptr::null(),
                0,
            )
        });
        assert!(!bundle.is_empty());
        let st = [0.0, 0.0, 0.0, 10.0, 0.0, 0.0];
        let sum: serde_json::Value = serde_json::from_slice(&read(unsafe {
            cc_shot_summary(bundle.as_ptr(), bundle.len(), st.as_ptr())
        }))
        .unwrap();
        assert_eq!(sum["placed"], false);
        assert!(
            read(unsafe { cc_shot_placed_positions(bundle.as_ptr(), bundle.len(), st.as_ptr()) })
                .is_empty()
        );
    }

    fn sample_bundle() -> Vec<u8> {
        let header = CString::new(BARE).unwrap();
        let pos = [0f32, 0.0, -5.0, 1.0, 0.0, -5.0, 0.0, 1.0, -5.0];
        let nor = [0f32; 9];
        let idx = [0u32, 1, 2];
        read(unsafe {
            cc_shot_encode(
                header.as_ptr(),
                pos.as_ptr(),
                9,
                nor.as_ptr(),
                9,
                idx.as_ptr(),
                3,
                std::ptr::null(),
                0,
            )
        })
    }

    #[test]
    fn add_pass_ghost_and_fit_through_the_doors() {
        let b = sample_bundle();
        let pass = CString::new(
            r#"{"captured_at_ms":5,"aligned_by":"same_session","transform":[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1],
               "replace":true,"tracking":"normal","cards":[],
               "keyframes":[{"pose":[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1],"intrinsics":[1,0,0,0,1,0,0,0,1],"width":1,"height":1,"jpeg_len":3}]}"#,
        )
        .unwrap();
        let pos = [0f32, 0.0, -1.0, 1.0, 0.0, -1.0, 0.0, 1.0, -1.0];
        let nor = [0f32; 9];
        let idx = [0u32, 1, 2];
        let jpg = [7u8, 7, 7];
        let add = |old: &[u8], n: usize| {
            read(unsafe {
                cc_shot_add_pass(
                    old.as_ptr(),
                    n,
                    pass.as_ptr(),
                    pos.as_ptr(),
                    9,
                    nor.as_ptr(),
                    9,
                    idx.as_ptr(),
                    3,
                    jpg.as_ptr(),
                    3,
                )
            })
        };
        let out = add(&b, b.len());
        let shot = decode(&out).unwrap();
        assert_eq!(shot.header.passes.len(), 2);
        assert_eq!(shot.keyframes.last().unwrap().jpeg, vec![7, 7, 7]);
        assert!(add(&b, 3).is_empty());

        let sum: serde_json::Value = serde_json::from_slice(&read(unsafe {
            cc_shot_summary(out.as_ptr(), out.len(), std::ptr::null())
        }))
        .unwrap();
        assert_eq!(sum["passes"], 2);

        let g = read(unsafe { cc_shot_ghost(out.as_ptr(), out.len(), 0.05, 0.0) });
        let word = |i: usize| u32::from_le_bytes(g[i * 4..i * 4 + 4].try_into().unwrap());
        assert_eq!((word(0), word(1)), (9, 3));
        assert_eq!(word(2), 1); // the lone triangle's edge is one closed outline
        assert!(read(unsafe { cc_shot_ghost(out.as_ptr(), 3, 0.05, 0.0) }).is_empty());

        let cards = CString::new(r#"[{"id":2,"word":"CURRENT","point":[0,0,0],"corners":[],"sightings":9,"spread_m":0.0}]"#).unwrap();
        let fit = |same: u8| -> serde_json::Value {
            serde_json::from_slice(&read(unsafe {
                cc_cards_fit(out.as_ptr(), out.len(), cards.as_ptr(), same)
            }))
            .unwrap()
        };
        assert!(fit(0)["fit"].is_null());
        assert!(fit(1)["offsets"].is_array());
    }

    #[test]
    fn cards_fit_reports_ok_ratio_and_limits() {
        let header = CString::new(
            r#"{"shot_id":"s","cave_id":"c","station_id":"a","station_label":"A","captured_at_ms":0,"device":"t",
               "instrument_height_m":0.3,"start_camera":[0,0,0],"compass":{"true_north":true,"accuracy_deg":5.0},
               "tracking":"normal","arkit_version":"26.1","gravity":[0,-1,0],"keyframes":[],
               "cards":[{"id":2,"word":"CURRENT","point":[0,0,0],"sightings":9,"spread_m":0.01},
                        {"id":12,"word":"FORWARD","point":[0,0,-4.8],"sightings":9,"spread_m":0.01}]}"#,
        )
        .unwrap();
        let pos = [0f32, 0.0, -1.0, 1.0, 0.0, -1.0, 0.0, 1.0, -1.0];
        let nor = [0f32; 9];
        let idx = [0u32, 1, 2];
        let b = read(unsafe {
            cc_shot_encode(
                header.as_ptr(),
                pos.as_ptr(),
                9,
                nor.as_ptr(),
                9,
                idx.as_ptr(),
                3,
                std::ptr::null(),
                0,
            )
        });
        let cards = CString::new(
            r#"[{"id":2,"word":"CURRENT","point":[0,0,0],"corners":[],"sightings":9,"spread_m":0.0},
                {"id":12,"word":"FORWARD","point":[0,0,-4.92],"corners":[],"sightings":9,"spread_m":0.0}]"#,
        )
        .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&read(unsafe {
            cc_cards_fit(b.as_ptr(), b.len(), cards.as_ptr(), 0)
        }))
        .unwrap();
        let f = &v["fit"];
        assert_eq!(f["ok"], true, "{f}");
        assert!(f["worst_m"].as_f64().unwrap() > 0.05);
        let r = f["worst_ratio"].as_f64().unwrap();
        assert!(r > 0.7 && r < 1.0, "{r}");
        let l = f["limits"].as_array().unwrap();
        assert_eq!(l.len(), 2);
        assert!((l[0]["m"].as_f64().unwrap() - 0.072).abs() < 1e-9);
        assert_eq!(f["misfits"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn summary_places_cards_relative_to_the_station() {
        let header = CString::new(
            r#"{"shot_id":"s","cave_id":"c","station_id":"a","station_label":"A","captured_at_ms":0,"device":"t",
               "instrument_height_m":0.3,"start_camera":[0,0,0],"compass":{"true_north":true,"accuracy_deg":5.0},
               "tracking":"normal","arkit_version":"26.1","gravity":[0,-1,0],"keyframes":[],
               "cards":[{"id":12,"word":"FORWARD","point":[0,0,-2],"sightings":4,"spread_m":0.01}]}"#,
        )
        .unwrap();
        let pos = [0f32, 0.0, -1.0, 1.0, 0.0, -1.0, 0.0, 1.0, -1.0];
        let nor = [0f32; 9];
        let idx = [0u32, 1, 2];
        let b = read(unsafe {
            cc_shot_encode(
                header.as_ptr(),
                pos.as_ptr(),
                9,
                nor.as_ptr(),
                9,
                idx.as_ptr(),
                3,
                std::ptr::null(),
                0,
            )
        });
        assert!(!b.is_empty());
        let st = [100.0, 200.0, 50.0, 100.0, 200.0, 50.0];
        let sum: serde_json::Value = serde_json::from_slice(&read(unsafe {
            cc_shot_summary(b.as_ptr(), b.len(), st.as_ptr())
        }))
        .unwrap();
        let c = &sum["cards"][0];
        assert_eq!(c["word"], "FORWARD");
        // 2 m north of the camera, which is 0.3 m above the station
        assert!((c["point"][0].as_f64().unwrap()).abs() < 1e-9);
        assert!((c["point"][1].as_f64().unwrap() - 2.0).abs() < 1e-9);
        assert!((c["point"][2].as_f64().unwrap() - 0.3).abs() < 1e-9);
        assert_eq!(c["sightings"], 4);
    }

    #[test]
    fn summary_names_its_stations_without_a_pair() {
        let b = sample_bundle();
        let sum: serde_json::Value = serde_json::from_slice(&read(unsafe {
            cc_shot_summary(b.as_ptr(), b.len(), std::ptr::null())
        }))
        .unwrap();
        assert_eq!(sum["station_id"], "a");
        assert_eq!(sum["target_station_id"], "b");
        assert_eq!(sum["vertices"], 3);
        assert_eq!(sum["triangles"], 1);
        assert_eq!(sum["placed"], false);
    }

    #[test]
    fn placed_positions_are_relative_to_the_station() {
        let b = sample_bundle();
        // a station far from the origin: absolute f32 would lose millimetres
        let st = [500_000.0, 4_000_000.0, 300.0, 500_010.0, 4_000_000.0, 300.0];
        let raw = read(unsafe { cc_shot_placed_positions(b.as_ptr(), b.len(), st.as_ptr()) });
        let pts: Vec<f32> = raw
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes(*c))
            .collect();
        assert_eq!(pts.len(), 9);
        // the first vertex is 5 m along the tap, which points at the target due east
        assert!((pts[0] - 5.0).abs() < 1e-4, "{pts:?}");
        assert!(pts[1].abs() < 1e-4 && pts[2].abs() < 1e-4, "{pts:?}");
    }

    fn floats(raw: &[u8]) -> Vec<f32> {
        raw.as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes(*c))
            .collect()
    }

    #[test]
    fn thin_points_door_thins_relative_to_the_station() {
        let b = sample_bundle();
        let st = [500_000.0, 4_000_000.0, 300.0, 500_010.0, 4_000_000.0, 300.0];
        let all = floats(&read(unsafe {
            cc_shot_placed_positions(b.as_ptr(), b.len(), st.as_ptr())
        }));
        // a fine grid keeps all three vertices, in order, exactly as the plain door gives them
        let fine = floats(&read(unsafe {
            cc_shot_placed_points_thin(b.as_ptr(), b.len(), st.as_ptr(), 0.05)
        }));
        assert_eq!(fine, all);
        // placed: (5,0,0), (5,-1,0), (5,0,1). A 10 m grid merges the two with y >= 0 into their
        // mean, first; the one south of the station (y < 0) is a cell of its own.
        let coarse = floats(&read(unsafe {
            cc_shot_placed_points_thin(b.as_ptr(), b.len(), st.as_ptr(), 10.0)
        }));
        let want = [5.0, 0.0, 0.5, 5.0, -1.0, 0.0];
        assert_eq!(coarse.len(), 6, "{coarse:?}");
        assert!(
            coarse.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-4),
            "{coarse:?}"
        );
    }

    #[test]
    fn thin_points_door_is_empty_when_it_cannot_place() {
        let b = sample_bundle();
        let junk = [1u8, 2, 3];
        let st = [0.0, 0.0, 0.0, 10.0, 0.0, 0.0];
        assert!(
            read(unsafe { cc_shot_placed_points_thin(junk.as_ptr(), 3, st.as_ptr(), 0.05) })
                .is_empty()
        );
        assert!(
            read(unsafe {
                cc_shot_placed_points_thin(b.as_ptr(), b.len(), std::ptr::null(), 0.05)
            })
            .is_empty()
        );
        assert!(
            read(unsafe { cc_shot_placed_points_thin(std::ptr::null(), 99, st.as_ptr(), 0.05) })
                .is_empty()
        );
    }

    const COMPASS: &str = r#"{"shot_id":"s","cave_id":"c","station_id":"a","station_label":"A","captured_at_ms":0,"device":"t",
        "instrument_height_m":0.3,"start_camera":[0,0,0],"taps":[],
        "compass":{"true_north":true,"accuracy_deg":6.0},
        "tracking":"normal","arkit_version":"26.1","gravity":[0,-1,0],"keyframes":[]}"#;

    fn bundle_of(header: &str) -> Vec<u8> {
        let header = CString::new(header).unwrap();
        let pos = [0f32, 0.0, -5.0, 1.0, 0.0, -5.0, 0.0, 1.0, -5.0];
        let nor = [0f32; 9];
        let idx = [0u32, 1, 2];
        read(unsafe {
            cc_shot_encode(
                header.as_ptr(),
                pos.as_ptr(),
                9,
                nor.as_ptr(),
                9,
                idx.as_ptr(),
                3,
                std::ptr::null(),
                0,
            )
        })
    }

    #[test]
    fn compass_shot_places_from_its_station_alone() {
        let b = bundle_of(COMPASS);
        assert!(!b.is_empty());
        // the target half is ignored for a compass shot: give it the station itself
        let st = [100.0, 200.0, 50.0, 100.0, 200.0, 50.0];
        let sum: serde_json::Value = serde_json::from_slice(&read(unsafe {
            cc_shot_summary(b.as_ptr(), b.len(), st.as_ptr())
        }))
        .unwrap();
        assert_eq!(sum["placed"], true, "{sum}");
        assert_eq!(sum["turn_deg"], 0.0);
        assert_eq!(sum["compass"]["true_north"], true);
        assert_eq!(sum["compass"]["accuracy_deg"], 6.0);
        assert!(sum["target_station_id"].is_null());
        let raw = read(unsafe { cc_shot_placed_positions(b.as_ptr(), b.len(), st.as_ptr()) });
        let pts: Vec<f32> = raw
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes(*c))
            .collect();
        // (0,0,-5) is 5 m north of the start camera, which sits 0.3 m above the station
        assert!(
            pts[0].abs() < 1e-5 && (pts[1] - 5.0).abs() < 1e-5 && (pts[2] - 0.3).abs() < 1e-5,
            "{pts:?}"
        );
    }

    #[test]
    fn waiting_shot_summary_works_unplaced_and_places_by_pair() {
        // station typed by hand, not in the cave yet: empty station id, the typed label kept
        let b = bundle_of(&COMPASS.replace(
            r#""station_id":"a","station_label":"A""#,
            r#""station_id":"","station_label":"A7""#,
        ));
        assert!(!b.is_empty());
        let sum: serde_json::Value = serde_json::from_slice(&read(unsafe {
            cc_shot_summary(b.as_ptr(), b.len(), std::ptr::null())
        }))
        .unwrap();
        assert!(sum["error"].is_null(), "{sum}");
        assert_eq!(sum["placed"], false);
        assert_eq!(sum["waiting"], true);
        assert_eq!(sum["station_id"], "");
        assert_eq!(sum["station_label"], "A7");
        assert_eq!(sum["vertices"], 3);
        // once the label is found, Swift passes that station's xyz: placed like any compass shot
        let st = [10.0, 20.0, 5.0, 10.0, 20.0, 5.0];
        let sum: serde_json::Value = serde_json::from_slice(&read(unsafe {
            cc_shot_summary(b.as_ptr(), b.len(), st.as_ptr())
        }))
        .unwrap();
        assert_eq!(sum["placed"], true, "{sum}");
        assert_eq!(
            sum["waiting"], true,
            "placement is derived: the file still says waiting"
        );
    }

    #[test]
    fn shot_with_a_station_is_not_waiting() {
        let b = bundle_of(COMPASS);
        let sum: serde_json::Value = serde_json::from_slice(&read(unsafe {
            cc_shot_summary(b.as_ptr(), b.len(), std::ptr::null())
        }))
        .unwrap();
        assert_eq!(sum["waiting"], false);
        assert_eq!(sum["station_label"], "A");
    }

    #[test]
    fn tap_shot_summary_has_no_compass() {
        let b = bundle_of(BARE);
        let sum: serde_json::Value = serde_json::from_slice(&read(unsafe {
            cc_shot_summary(b.as_ptr(), b.len(), std::ptr::null())
        }))
        .unwrap();
        assert!(sum["compass"].is_null());
    }

    /// A compass shot whose frame is turned `yaw_deg` (counter-clockwise) and shifted `at` from the
    /// survey, seeing `cards` (id, true survey point); its mesh is three points near the camera.
    fn chain_shot(
        label: &str,
        id: &str,
        yaw_deg: f64,
        at: [f64; 3],
        cards: &[(u32, [f64; 3])],
    ) -> Vec<u8> {
        let (s, c) = yaw_deg.to_radians().sin_cos();
        // survey → survey-axis capture: undo the shift, turn back; then to capture axes (x, z, -y)
        let cap = |p: [f64; 3]| {
            let (x, y, z) = (p[0] - at[0], p[1] - at[1], p[2] - at[2]);
            let (ex, ny) = (c * x + s * y, -s * x + c * y);
            [ex, z, -ny]
        };
        let cards: Vec<serde_json::Value> = cards
            .iter()
            .map(|&(cid, p)| serde_json::json!({"id": cid, "word": "", "point": cap(p), "corners": [], "sightings": 9, "spread_m": 0.01}))
            .collect();
        let h = serde_json::json!({
            "shot_id": format!("shot-{label}"), "cave_id": "c", "station_id": id, "station_label": label,
            "captured_at_ms": 0, "device": "t", "instrument_height_m": 0.3, "start_camera": cap([at[0], at[1], at[2]]),
            "taps": [], "compass": {"true_north": true, "accuracy_deg": 5.0},
            "tracking": "normal", "arkit_version": "26.1", "gravity": [0, -1, 0], "keyframes": [], "cards": cards,
        });
        bundle_of(&h.to_string())
    }

    fn run_chains(stations: &str, shots: &[&[u8]]) -> serde_json::Value {
        let st = CString::new(stations).unwrap();
        let ptrs: Vec<*const u8> = shots.iter().map(|b| b.as_ptr()).collect();
        let lens: Vec<usize> = shots.iter().map(|b| b.len()).collect();
        serde_json::from_slice(&read(unsafe {
            cc_chains(st.as_ptr(), ptrs.as_ptr(), lens.as_ptr(), shots.len())
        }))
        .unwrap()
    }

    const A3: [f64; 3] = [10.0, 20.0, 5.0];
    const A4: [f64; 3] = [14.0, 23.0, 4.5];

    #[test]
    fn chains_place_neighbours_and_correct_the_compass() {
        // T's compass was 10° out; A3 and A4 are both placed, so the chain gives the true heading
        let s = chain_shot("A3", "a3", 0.0, [10.0, 20.0, 5.3], &[(2, A3), (12, A4)]);
        let t = chain_shot("A4", "a4", 10.0, [14.0, 23.0, 4.8], &[(2, A4), (26, A3)]);
        let junk = [1u8, 2, 3];
        let st = r#"[{"id":"a3","label":"A3","x":10,"y":20,"z":5},{"id":"a4","label":"A4","x":14,"y":23,"z":4.5}]"#;
        let v = run_chains(st, &[&s, &junk, &t]);
        assert_eq!(v["counts"]["shots"], 3);
        assert_eq!(v["counts"]["chains"], 1);
        assert_eq!(v["counts"]["by_chain"], 2);
        assert_eq!(v["counts"]["flagged"], 0);
        assert!(v["shots"][1]["error"].is_string(), "{v}");
        let t0 = &v["shots"][2];
        assert_eq!(t0["source"], "chain", "{t0}");
        assert_eq!(t0["placed"], true);
        assert_eq!(t0["joined"][0], "A3");
        assert_eq!(t0["pair"][0], 14.0);
        let yaw = t0["fix"]["yaw_deg"].as_f64().unwrap();
        assert!((yaw - 10.0).abs() < 0.5, "{t0}");
        // its station point (the CURRENT card) lands on A4
        for k in 0..3 {
            assert!(t0["spot"][k].as_f64().unwrap().abs() < 0.01, "{t0}");
        }
        assert_eq!(v["chains"][0]["shots"], serde_json::json!([0, 2]));
        assert_eq!(v["chains"][0]["labels"], serde_json::json!(["A3", "A4"]));
    }

    #[test]
    fn waiting_chain_floats_round_its_first_shot() {
        let a7 = [100.0, 50.0, -3.0];
        let a8 = [103.0, 54.0, -3.5];
        let s = chain_shot("A7", "", 0.0, [100.0, 50.0, -2.7], &[(2, a7), (12, a8)]);
        let t = chain_shot("A8", "", 0.0, [103.0, 54.0, -3.2], &[(2, a8), (26, a7)]);
        let v = run_chains(
            r#"{"stations":[{"id":"a3","label":"A3","x":10,"y":20,"z":5}],"legs":[]}"#,
            &[&s, &t],
        );
        assert_eq!(v["chains"][0]["placed"], false);
        let t0 = &v["shots"][1];
        assert_eq!(t0["source"], "floating");
        assert_eq!(t0["placed"], false);
        assert_eq!(
            t0["pair"],
            serde_json::json!([0.0, 0.0, 0.0, 0.0, 0.0, 0.0])
        );
        let want = [3.0, 4.0, -0.5];
        for k in 0..3 {
            assert!(
                (t0["spot"][k].as_f64().unwrap() - want[k]).abs() < 0.01,
                "{t0}"
            );
        }
        // A7 arrives: placed through it
        let v = run_chains(
            r#"[{"id":"x","label":"a7","x":100,"y":50,"z":-3}]"#,
            &[&s, &t],
        );
        assert_eq!(v["shots"][1]["source"], "chain");
        assert_eq!(
            v["shots"][1]["pair"][0], 0.0,
            "A8 itself is still not in the cave"
        );
        assert!((v["shots"][1]["spot"][0].as_f64().unwrap() - 103.0).abs() < 0.01);
    }

    #[test]
    fn chains_flag_a_leg_misfit_and_refuse_junk() {
        let s = chain_shot("A3", "a3", 0.0, [10.0, 20.0, 5.3], &[(2, A3), (12, A4)]);
        let t = chain_shot("A4", "a4", 0.0, [14.0, 23.0, 4.8], &[(2, A4), (26, A3)]);
        let st = r#"[{"id":"a3","label":"A3","x":10,"y":20,"z":5},{"id":"a4","label":"A4","x":14.4,"y":23.3,"z":4.5}]"#;
        let v = run_chains(st, &[&s, &t]);
        assert_eq!(v["counts"]["chains"], 0);
        assert_eq!(v["counts"]["flagged"], 2);
        let f = &v["shots"][0]["flags"][0];
        assert_eq!(f["kind"], "leg_misfit");
        assert_eq!(f["with"], "A4");
        assert!((f["off_m"].as_f64().unwrap() - 0.5).abs() < 0.01, "{f}");
        assert_eq!(v["shots"][0]["source"], "card+compass");
        assert_eq!(v["shots"][0]["misfit"], true);
        let bad = CString::new("nope").unwrap();
        let out: serde_json::Value = serde_json::from_slice(&read(unsafe {
            cc_chains(bad.as_ptr(), std::ptr::null(), std::ptr::null(), 0)
        }))
        .unwrap();
        assert!(out["error"].is_string());
        let out: serde_json::Value = serde_json::from_slice(&read(unsafe {
            cc_chains(std::ptr::null(), std::ptr::null(), std::ptr::null(), 2)
        }))
        .unwrap();
        assert!(out["error"].is_string());
    }

    #[test]
    fn camera_sweep_round_trips_through_the_gimbal_plan() {
        let json = String::from_utf8(read(cc_camera_sweep(10.0))).unwrap();
        let lists = gimbal::parse_plan(&json).unwrap();
        let last = lists.last().unwrap().last().unwrap();
        assert_eq!((last.yaw, last.pitch), (0.0, 0.0), "ends home");
        assert!(read(cc_camera_sweep(0.0)).starts_with(b"[]"));
    }

    #[test]
    fn camera_fill_keeps_reachable_and_refuses_junk() {
        let m = CString::new("[[10,5],[0,85]]").unwrap();
        let json = String::from_utf8(read(unsafe { cc_camera_fill(m.as_ptr(), 10.0) })).unwrap();
        assert!(gimbal::parse_plan(&json).is_ok());
        let none = CString::new("[[0,85]]").unwrap();
        assert_eq!(read(unsafe { cc_camera_fill(none.as_ptr(), 10.0) }), b"[]");
        let bad = CString::new("nope").unwrap();
        let out: serde_json::Value =
            serde_json::from_slice(&read(unsafe { cc_camera_fill(bad.as_ptr(), 10.0) })).unwrap();
        assert!(out["error"].is_string());
    }
}
