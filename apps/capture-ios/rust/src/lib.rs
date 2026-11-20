//! C interface to `cave-capture` for the CaveCAD Capture iPhone app. Every function takes plain
//! pointers and returns a `CcBuf` the caller frees with `cc_free`. Inputs are never trusted: a
//! null or malformed argument gives an error JSON (or empty bytes), never a crash.
#![allow(unsafe_code)]

use cave_capture::{
    Keyframe, Shot, ShotHeader, decode, document_from_room, encode, heading_from_tap, stations,
};
use cave_survey::Vec3;
use serde::Deserialize;
use std::ffi::{CStr, c_char};

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

unsafe fn text<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        None
    } else {
        unsafe { CStr::from_ptr(p) }.to_str().ok()
    }
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
    let snap = unsafe { text(snapshot) };
    let entries: Vec<String> = match unsafe { text(entries_json) } {
        None => Vec::new(),
        Some(t) => match serde_json::from_str(t) {
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
    let Some(h) =
        (unsafe { text(header_json) }).and_then(|t| serde_json::from_str::<HeaderIn>(t).ok())
    else {
        return give(Vec::new());
    };
    let jpeg_bytes = unsafe { slice(jpegs, n_jpegs) };
    let mut at = 0usize;
    let mut keyframes = Vec::new();
    for k in h.keyframes {
        let (Ok(pose), Ok(intrinsics)) = (
            <[f32; 16]>::try_from(k.pose),
            <[f32; 9]>::try_from(k.intrinsics),
        ) else {
            return give(Vec::new());
        };
        let Some(jpeg) = jpeg_bytes.get(at..at + k.jpeg_len) else {
            return give(Vec::new());
        };
        at += k.jpeg_len;
        keyframes.push(Keyframe {
            pose,
            intrinsics,
            width: k.width,
            height: k.height,
            jpeg: jpeg.to_vec(),
        });
    }
    let shot = Shot {
        header: h.header,
        positions: unsafe { slice(positions, n_positions) }.to_vec(),
        normals: unsafe { slice(normals, n_normals) }.to_vec(),
        indices: unsafe { slice(indices, n_indices) }.to_vec(),
        keyframes,
    };
    let bytes = encode(&shot);
    if decode(&bytes).is_err() {
        give(Vec::new())
    } else {
        give(bytes)
    }
}

fn place(shot: &Shot, st: &[f64]) -> Option<cave_capture::Placement> {
    let tap = shot.header.taps.first()?;
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
    let pts = shot
        .positions
        .as_chunks::<3>()
        .0
        .iter()
        .map(|c| p.apply([c[0] as f64, c[1] as f64, c[2] as f64]))
        .collect();
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
    let shot = match decode(unsafe { slice(bundle, len) }) {
        Ok(s) => s,
        Err(e) => return err(&format!("{e:?}")),
    };
    let st = unsafe { slice(station_and_target, 6) };
    let mut out = serde_json::json!({
        "vertices": shot.positions.len() / 3,
        "triangles": shot.indices.len() / 3,
        "keyframes": shot.keyframes.len(),
        "placed": false,
    });
    if st.len() == 6
        && let Some((pts, turn)) = placed_points(&shot, st)
    {
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
    let mut out = Vec::with_capacity(pts.len() * 12);
    for q in pts {
        for v in [q.x as f32, q.y as f32, q.z as f32] {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    give(out)
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
               "tracking":"normal","keyframes":[{"pose":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"intrinsics":[0,0,0,0,0,0,0,0,0],"width":1,"height":1,"jpeg_len":2}]}"#,
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
}
