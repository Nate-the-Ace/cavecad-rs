//! C interface to `cave-capture` for the CaveCAD Capture iPhone app. Every function takes plain
//! pointers and returns a `CcBuf` the caller frees with `cc_free`. Inputs are never trusted: a
//! null or malformed argument gives an error JSON (or empty bytes), never a crash.
#![allow(unsafe_code)]

use cave_capture::{
    CardSeen, Keyframe, KeyframeRef, NewPass, PassMesh, Shot, ShotHeader, add_pass, card_offsets,
    cards_fit, check_header, decode, document_from_room, encode, encode_parts, from_compass, ghost,
    heading_from_tap, stations,
};
use cave_survey::Vec3;
use serde::Deserialize;
use std::ffi::{CStr, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};

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

/// A compass shot places from its station alone (the target half of `st` is ignored); an older tap
/// shot needs its target.
fn place(shot: &Shot, st: &[f64]) -> Option<cave_capture::Placement> {
    let station = Vec3 {
        x: st[0],
        y: st[1],
        z: st[2],
    };
    if shot.header.compass.is_some() {
        return from_compass(
            station,
            shot.header.start_camera,
            shot.header.instrument_height_m,
        );
    }
    let tap = shot.header.taps.first()?;
    let target = Vec3 {
        x: st[3],
        y: st[4],
        z: st[5],
    };
    heading_from_tap(
        station,
        target,
        tap.dir,
        shot.header.start_camera,
        shot.header.instrument_height_m,
    )
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
    let st = unsafe { slice(station_and_target, 6) };
    let Ok(shot) = decode(unsafe { slice(bundle, len) }) else {
        return give(Vec::new());
    };
    if st.len() != 6 {
        return give(Vec::new());
    }
    let Some((pts, _)) = placed_points(&shot, st) else {
        return give(Vec::new());
    };
    // Relative to the station, in f64 before narrowing: survey coordinates can be millions of
    // metres, where f32 keeps only about half a metre.
    let mut out = Vec::with_capacity(pts.len() * 12);
    for q in pts {
        for v in [
            (q.x - st[0]) as f32,
            (q.y - st[1]) as f32,
            (q.z - st[2]) as f32,
        ] {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    give(out)
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
                        "misfits": pairs(&f.misfits)
                    })
                })}),
            )
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
    fn tap_shot_summary_has_no_compass() {
        let b = bundle_of(BARE);
        let sum: serde_json::Value = serde_json::from_slice(&read(unsafe {
            cc_shot_summary(b.as_ptr(), b.len(), std::ptr::null())
        }))
        .unwrap();
        assert!(sum["compass"].is_null());
    }
}
