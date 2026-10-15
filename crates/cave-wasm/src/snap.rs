//! Object snap in the browser: thin `wasm-bindgen` doors onto `cave-snap`. All the work is in that crate and tested
//! natively; everything crosses as JSON, and nothing is kept here between calls (the session of a pick in progress
//! lives in the page and is passed in and out).

use cave_snap::{Acquired, Guide, Mode, Scene, Session, Settings, along_guide, snap};
use serde::Serialize;
use wasm_bindgen::prelude::*;

/// What can be snapped to, near `cursor` (pixels): `scene`, `settings` and `session` are JSON. Answers a `SnapResult`
/// (JSON): the best candidate, the rest in Tab order, the guides to draw and what hovering here could acquire.
/// Anything unreadable answers an empty result.
#[wasm_bindgen]
pub fn snap_pick(
    scene: &str,
    cursor_x: f64,
    cursor_y: f64,
    settings: &str,
    session: &str,
) -> String {
    let (Ok(scene), Ok(settings), Ok(session)) = (
        serde_json::from_str::<Scene>(scene),
        serde_json::from_str::<Settings>(settings),
        serde_json::from_str::<Session>(session),
    ) else {
        return serde_json::to_string(&cave_snap::SnapResult::default()).unwrap_or_default();
    };
    serde_json::to_string(&snap(&scene, [cursor_x, cursor_y], &settings, &session))
        .unwrap_or_default()
}

/// A pick landed on world point `(x, y, z)`: what it came to, `{point, session}` (JSON). `point` is `null` when the pick
/// only set something up (a track point, a base point, the first point of Mid Between 2 Points).
#[wasm_bindgen]
pub fn snap_commit(session: &str, x: f64, y: f64, z: f64) -> String {
    let session: Session = serde_json::from_str(session).unwrap_or_default();
    serde_json::to_string(&session.pick([x, y, z])).unwrap_or_default()
}

/// Changes a session and answers it (JSON). `op` is `acquire` (`arg` an `Acquired`), `one_pick` (`arg` a mode name or
/// `null`), `cancel` or `last` (`arg` a world point `[x, y, z]`, the previous point of the pick in progress).
#[wasm_bindgen]
pub fn snap_session(op: &str, session: &str, arg: &str) -> String {
    let mut s: Session = serde_json::from_str(session).unwrap_or_default();
    match op {
        "acquire" => {
            if let Ok(a) = serde_json::from_str::<Acquired>(arg) {
                s.acquire(a);
            }
        }
        "one_pick" => s.one_pick(serde_json::from_str::<Option<Mode>>(arg).unwrap_or(None)),
        "cancel" => s.cancel(),
        "last" => s.last_point = serde_json::from_str::<Option<[f64; 3]>>(arg).unwrap_or(None),
        _ => {}
    }
    serde_json::to_string(&s).unwrap_or_default()
}

/// The point `distance` along a straight guide (JSON `Guide`) from its origin, or `null`.
#[wasm_bindgen]
pub fn snap_along(guide: &str, distance: f64) -> String {
    let g = serde_json::from_str::<Guide>(guide).ok();
    serde_json::to_string(&g.and_then(|g| along_guide(&g, distance)))
        .unwrap_or_else(|_| "null".into())
}

#[derive(Serialize)]
struct ModeInfo {
    id: Mode,
    name: &'static str,
    /// Offered in the one-pick override menu and the settings list (the rest are results of tracking, not choices).
    group: &'static str,
    /// On unless the person has chosen otherwise.
    default: bool,
}

/// The modes the settings list and the override menu offer, with the default settings (JSON `{settings, modes}`).
#[wasm_bindgen]
pub fn snap_catalog() -> String {
    let defaults = Mode::defaults();
    let groups: [(&str, &[Mode]); 3] = [
        (
            "point",
            &[
                Mode::Endpoint,
                Mode::Midpoint,
                Mode::Center,
                Mode::GeometricCenter,
                Mode::Quadrant,
                Mode::Insertion,
                Mode::Node,
                Mode::Station,
            ],
        ),
        (
            "relational",
            &[
                Mode::Intersection,
                Mode::ApparentIntersection,
                Mode::ExtendedIntersection,
                Mode::Extension,
                Mode::Perpendicular,
                Mode::Tangent,
                Mode::Parallel,
                Mode::Nearest,
            ],
        ),
        (
            "reference",
            &[Mode::From, Mode::MidBetween, Mode::TrackPoint],
        ),
    ];
    let modes: Vec<ModeInfo> = groups
        .iter()
        .flat_map(|(g, ms)| {
            ms.iter().map(|m| ModeInfo {
                id: *m,
                name: m.name(),
                group: g,
                default: defaults.contains(m),
            })
        })
        .collect();
    serde_json::json!({ "settings": Settings::default(), "modes": modes }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn the_catalog_lists_every_choosable_mode_and_the_defaults() {
        let c: Value = serde_json::from_str(&snap_catalog()).unwrap();
        assert_eq!(c["modes"].as_array().unwrap().len(), 19);
        assert_eq!(c["settings"]["aperture"], 10.0);
        let on: Vec<&str> = c["modes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["default"] == true)
            .map(|m| m["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            on,
            [
                "endpoint",
                "midpoint",
                "center",
                "quadrant",
                "insertion",
                "station",
                "intersection"
            ]
        );
    }

    #[test]
    fn a_pick_goes_through_json_and_back() {
        let scene = r#"{"camera":{"view_proj":[0.025,0,0,0,0,0.03333333333333333,0,0,0,0,-0.001,0,0,0,0,1],"width":800,"height":600},
          "things":[{"id":"x","kind":"line","frame":{"origin":[0,0,0],"u":[1,0,0],"v":[0,1,0]},"geom":{"type":"polyline","pts":[[0,0],[10,0]]}}],"plane":null}"#;
        let settings = serde_json::to_string(&Settings::default()).unwrap();
        let r: Value =
            serde_json::from_str(&snap_pick(scene, 502.0, 301.0, &settings, "{}")).unwrap();
        assert_eq!(r["best"]["mode"], "endpoint");
        assert_eq!(r["best"]["source"]["seg"], 1);
        // A one-pick override for Mid Between 2 Points, then two picks.
        let s = snap_session("one_pick", "{}", "\"mid_between\"");
        let first: Value = serde_json::from_str(&snap_commit(&s, 0.0, 0.0, 0.0)).unwrap();
        assert!(first["point"].is_null());
        let second: Value =
            serde_json::from_str(&snap_commit(&first["session"].to_string(), 10.0, 0.0, 0.0))
                .unwrap();
        assert_eq!(second["point"], serde_json::json!([5.0, 0.0, 0.0]));
        // Garbage in, empty result out.
        let bad: Value =
            serde_json::from_str(&snap_pick("nonsense", 0.0, 0.0, "{}", "{}")).unwrap();
        assert!(bad["best"].is_null());
    }
}
