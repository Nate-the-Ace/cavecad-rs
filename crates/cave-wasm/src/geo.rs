//! Doors for Surface Data and the entrance location. Coordinates cross this boundary as JSON for the
//! person's own screen; they are NEVER logged (cave-log redaction is only a safety net).

use crate::{DOC, record_edit};
use cave_doc::{Action, Document, Field, Kind};
use cave_geo::frame::{LatLon, Placement};
use cave_geo::grid::Grid;
use cave_geo::record::{self, Candidate, Location, SurfaceRecord};
use cave_geo::request::{self, Bounds, ELEVATION, PHOTO};
use serde::{Deserialize, Serialize};
use serde_json::json;
use wasm_bindgen::prelude::*;

fn err(e: impl std::fmt::Display) -> String {
    format!("error: {e}")
}

/// The entrance station's solved position, metres.
fn entrance(scene: &cave_scene::Scene, station: cave_id::Id) -> Option<[f64; 3]> {
    let id = station.to_string();
    scene.stations.iter().find(|s| s.id == id).map(|s| s.at)
}

type Placed = (Placement, Location, [f64; 3], cave_scene::Scene);

/// The placement of the cave on the Earth, the stored location, the entrance's solved position, the scene.
fn placement(doc: &Document, adjusted: bool) -> Result<Placed, String> {
    let loc = record::location(doc).ok_or("no location")?;
    let scene = cave_scene::build(doc, adjusted);
    let at = entrance(&scene, loc.station).ok_or("the entrance station has no position")?;
    Ok((Placement::new([at[0], at[1]], loc.at()), loc, at, scene))
}

type WithSurface = (Placement, Location, [f64; 3], SurfaceRecord);

/// Placement in the zone the surface was cut in (a stored surface keeps its zone).
fn surface_placement(doc: &Document, adjusted: bool) -> Result<WithSurface, String> {
    let (p, loc, at, _) = placement(doc, adjusted)?;
    let s = record::surface(doc).ok_or("no surface")?;
    Ok((Placement::in_zone(p.anchor(), loc.at(), s.zone), loc, at, s))
}

/// The panel histories an edit may be recorded under (the web app's `Scope` in `engine.ts`).
const SCOPES: &[&str] = &[
    "profile", "section", "map", "stations", "notebook", "scans", "settings",
];

/// `Err` naming the problem when `scope` is not a panel history.
fn check_scope(scope: &str) -> Result<(), String> {
    if SCOPES.contains(&scope) {
        Ok(())
    } else {
        Err(err(format!("unknown undo scope {scope:?}")))
    }
}

/// Applies `actions` as one edit on `scope`'s history. Returns `ok` or `error: ...`.
fn commit(doc: &mut Document, actions: Vec<Action>, scope: &str) -> String {
    if actions.is_empty() {
        return "ok".into();
    }
    match doc.edit(actions) {
        Ok(edit) => {
            record_edit(scope, &edit);
            "ok".into()
        }
        Err(e) => err(format!("{e:?}")),
    }
}

/// Everything the Surface Data dialog needs (JSON).
#[wasm_bindgen]
pub fn geo_status() -> String {
    DOC.with(|d| {
        let slot = d.borrow();
        let Some(doc) = slot.as_ref() else {
            return err("no cave is open");
        };
        let candidate = match record::candidate(doc) {
            Candidate::Stored(_) => json!({"kind": "stored"}),
            Candidate::Imported { station, at } => {
                json!({"kind": "imported", "station": station.to_string(), "lat": at.lat, "lon": at.lon})
            }
            Candidate::Ask { station } => {
                json!({"kind": "ask", "station": station.map(|s| s.to_string())})
            }
        };
        let mut stations: Vec<(String, String)> = doc
            .ids_of(Kind::Station)
            .into_iter()
            .filter_map(|id| {
                doc.entity(id)
                    .map(|e| (e.text(Field::StationLabel), id.to_string()))
            })
            .collect();
        stations.sort();
        let stations: Vec<_> = stations
            .into_iter()
            .map(|(label, id)| json!({"id": id, "label": label}))
            .collect();
        json!({
            "location": record::location(doc),
            "surface": record::surface(doc),
            "candidate": candidate,
            "interval_m": record::contour_interval_m(doc),
            "stations": stations,
        })
        .to_string()
    })
}

#[derive(Deserialize)]
struct SetLocation {
    station: String,
    lat: f64,
    lon: f64,
    source: String,
    #[serde(default)]
    ground_m: Option<f64>,
}

/// Stores the cave's location (one undoable edit on `scope`'s history: `settings` from the Surface
/// Data dialog, `map` for a pick on the Map). JSON `{station, lat, lon, source, ground_m?}`.
#[wasm_bindgen]
pub fn geo_set_location(json_in: &str, scope: &str) -> String {
    if let Err(e) = check_scope(scope) {
        return e;
    }
    let req: SetLocation = match serde_json::from_str(json_in) {
        Ok(r) => r,
        Err(e) => return err(e),
    };
    if !(req.lat.abs() <= 90.0 && req.lon.abs() <= 180.0) {
        return err("that is not a latitude and longitude");
    }
    DOC.with(|d| {
        let mut slot = d.borrow_mut();
        let Some(doc) = slot.as_mut() else {
            return err("no cave is open");
        };
        let Ok(station) = req.station.parse::<cave_id::Id>() else {
            return err("unknown station");
        };
        if doc
            .entity(station)
            .is_none_or(|e| e.kind() != Kind::Station)
        {
            return err("unknown station");
        }
        let loc = Location {
            station,
            lat: req.lat,
            lon: req.lon,
            ground_m: req.ground_m.filter(|g| g.is_finite()),
            source: req.source,
        };
        let actions = record::set_location(doc, &loc);
        commit(doc, actions, scope)
    })
}

#[derive(Serialize, Deserialize)]
struct Part {
    w: u32,
    h: u32,
    url: String,
}

#[derive(Serialize, Deserialize)]
struct Plan {
    zone: i32,
    bounds: Bounds,
    for_at: LatLon,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    photo: Option<Part>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    elevation: Option<Part>,
}

/// What to download for the open cave (JSON plan), from its plan extent and stored location.
#[wasm_bindgen]
pub fn surface_plan(photo: bool, elevation: bool, adjusted: bool) -> String {
    DOC.with(|d| {
        let slot = d.borrow();
        let Some(doc) = slot.as_ref() else {
            return err("no cave is open");
        };
        let (place, loc, at, scene) = match placement(doc, adjusted) {
            Ok(v) => v,
            Err(e) => return err(e),
        };
        // Re-fetching one part of a surface cut for this very location keeps its window, so the part
        // already on disk still matches the new one.
        let kept = record::surface(doc).filter(|s| s.for_at == loc.at() && !(photo && elevation));
        let (b, zone) = match kept {
            Some(s) => (s.bounds, s.zone),
            None => {
                let cave = scene
                    .bounds
                    .map_or([at[0], at[1], at[0], at[1]], |b| [b[0], b[1], b[3], b[4]]);
                (request::window(cave, &place), place.zone())
            }
        };
        let part = |s, url: fn(&Bounds, i32, (u32, u32)) -> String| {
            let (w, h) = request::pixel_size(&b, s);
            Part {
                w,
                h,
                url: url(&b, zone, (w, h)),
            }
        };
        let plan = Plan {
            zone,
            bounds: b,
            for_at: loc.at(),
            photo: photo.then(|| part(PHOTO, request::naip_url)),
            elevation: elevation.then(|| part(ELEVATION, request::dem_url)),
        };
        serde_json::to_string(&plan).unwrap_or_else(err)
    })
}

/// The plan that downloads again exactly what the stored surface record lists (its zone, window,
/// pixel sizes and `for_at`), for a device that lacks the files. Never a new window, so the files
/// match the record; same JSON shape as [`surface_plan`]. `adjusted` is unused: the record decides.
#[wasm_bindgen]
pub fn surface_plan_for_record(adjusted: bool) -> String {
    let _ = adjusted;
    DOC.with(|d| {
        let slot = d.borrow();
        let Some(doc) = slot.as_ref() else {
            return err("no cave is open");
        };
        let Some(s) = record::surface(doc) else {
            return err("no surface");
        };
        let part = |size: Option<[u32; 2]>, url: fn(&Bounds, i32, (u32, u32)) -> String| {
            size.map(|[w, h]| Part {
                w,
                h,
                url: url(&s.bounds, s.zone, (w, h)),
            })
        };
        let plan = Plan {
            zone: s.zone,
            bounds: s.bounds,
            for_at: s.for_at,
            photo: part(s.photo, request::naip_url),
            elevation: part(s.elevation, request::dem_url),
        };
        serde_json::to_string(&plan).unwrap_or_else(err)
    })
}

/// Ground (metres NAVD88) at the stored location from an elevation TIFF cut to `plan_json`'s bounds, or NaN.
#[wasm_bindgen]
pub fn surface_ground(plan_json: &str, tiff: &[u8]) -> f64 {
    let (Ok(plan), Ok(grid)) = (
        serde_json::from_str::<Plan>(plan_json),
        Grid::parse_tiff(tiff),
    ) else {
        return f64::NAN;
    };
    let place = Placement::in_zone([0.0, 0.0], plan.for_at, plan.zone);
    let (e, n) = place.latlon_to_utm(plan.for_at);
    let (c, r) = grid.utm_cell(&plan.bounds, e, n);
    grid.sample_at(c, r).unwrap_or(f64::NAN)
}

/// Records a finished download (one edit on `scope`'s history): the surface request and, when finite,
/// the ground at the entrance.
#[wasm_bindgen]
pub fn surface_commit(plan_json: &str, ground_m: f64, today: &str, scope: &str) -> String {
    if let Err(e) = check_scope(scope) {
        return e;
    }
    let plan: Plan = match serde_json::from_str(plan_json) {
        Ok(p) => p,
        Err(e) => return err(e),
    };
    DOC.with(|d| {
        let mut slot = d.borrow_mut();
        let Some(doc) = slot.as_mut() else {
            return err("no cave is open");
        };
        // A stored part is kept only if it was cut to exactly this window; otherwise it is dropped.
        let old = record::surface(doc).filter(|s| s.zone == plan.zone && s.bounds == plan.bounds);
        let rec = SurfaceRecord {
            zone: plan.zone,
            bounds: plan.bounds,
            photo: plan
                .photo
                .as_ref()
                .map(|p| [p.w, p.h])
                .or_else(|| old.as_ref().and_then(|s| s.photo)),
            elevation: plan
                .elevation
                .as_ref()
                .map(|p| [p.w, p.h])
                .or_else(|| old.as_ref().and_then(|s| s.elevation)),
            for_at: plan.for_at,
            fetched: today.to_string(),
        };
        let mut actions = record::set_surface(doc, &rec);
        if ground_m.is_finite()
            && let Some(mut loc) = record::location(doc)
        {
            loc.ground_m = Some(ground_m);
            actions.extend(record::set_location(doc, &loc));
        }
        commit(doc, actions, scope)
    })
}

/// Sets the contour spacing (metres). One edit.
#[wasm_bindgen]
pub fn set_contour_interval(metres: f64) -> String {
    if !(metres.is_finite() && metres > 0.0) {
        return err("the spacing must be a positive length");
    }
    DOC.with(|d| {
        let mut slot = d.borrow_mut();
        let Some(doc) = slot.as_mut() else {
            return err("no cave is open");
        };
        let actions = record::set_contour_interval_m(doc, metres);
        commit(doc, actions, "settings")
    })
}

/// What the views draw (JSON `{photo_matrix?, photo_size?, contours, stale}`). `tiff` may be empty (photo only).
#[wasm_bindgen]
pub fn surface_layers(tiff: &[u8], interval: f64, feet: bool, adjusted: bool) -> String {
    DOC.with(|d| {
        let slot = d.borrow();
        let Some(doc) = slot.as_ref() else {
            return err("no cave is open");
        };
        let (place, loc, at, s) = match surface_placement(doc, adjusted) {
            Ok(v) => v,
            Err(e) => return err(e),
        };
        let offset = cave_geo::layers::datum_offset(loc.ground_m, at[2]);
        let photo = s
            .photo
            .map(|[w, h]| cave_geo::layers::photo_matrix(&s.bounds, (w, h), &place));
        let contours = match Grid::parse_tiff(tiff) {
            Ok(g) if interval.is_finite() && interval > 0.0 => {
                cave_geo::layers::contours(&g, &s.bounds, &place, interval, feet, offset, at[2])
            }
            _ => Vec::new(),
        };
        json!({"photo_matrix": photo, "photo_size": s.photo, "contours": contours, "stale": s.for_at != loc.at()}).to_string()
    })
}

/// The ground as a mesh, as numbers not text: `[w, h, x0, y0, z0, x1, ...]` (cave metres, NaN z for holes).
/// Empty when there is no surface, no datum offset, or the TIFF does not parse.
#[wasm_bindgen]
pub fn surface_terrain(tiff: &[u8], adjusted: bool) -> Vec<f32> {
    DOC.with(|d| {
        let slot = d.borrow();
        let Some(doc) = slot.as_ref() else {
            return Vec::new();
        };
        let Ok((place, loc, at, s)) = surface_placement(doc, adjusted) else {
            return Vec::new();
        };
        let offset = cave_geo::layers::datum_offset(loc.ground_m, at[2]);
        let Ok(g) = Grid::parse_tiff(tiff) else {
            return Vec::new();
        };
        let Some(t) = cave_geo::layers::terrain(&g, &s.bounds, &place, offset) else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(2 + t.xyz.len());
        out.push(t.w as f32);
        out.push(t.h as f32);
        out.extend(t.xyz.iter().map(|v| *v as f32));
        out
    })
}

/// The entrance pick: lowest ground inside the circle (cave metres). JSON `{x, y, lat, lon, ground_m}`.
#[wasm_bindgen]
pub fn surface_pick(tiff: &[u8], cx: f64, cy: f64, radius: f64, adjusted: bool) -> String {
    DOC.with(|d| {
        let slot = d.borrow();
        let Some(doc) = slot.as_ref() else {
            return err("no cave is open");
        };
        let (place, _, _, s) = match surface_placement(doc, adjusted) {
            Ok(v) => v,
            Err(e) => return err(e),
        };
        let grid = match Grid::parse_tiff(tiff) {
            Ok(g) => g,
            Err(e) => return err(e),
        };
        match cave_geo::pick::lowest_in_circle(&grid, &s.bounds, &place, [cx, cy], radius) {
            Some(p) => json!({"x": p.point[0], "y": p.point[1], "lat": p.at.lat, "lon": p.at.lon, "ground_m": p.ground_m}).to_string(),
            None => err("no ground inside the circle"),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Surface Data dialog's way in: edits on the settings history.
    fn geo_set_location(json_in: &str) -> String {
        super::geo_set_location(json_in, "settings")
    }

    fn surface_commit(plan_json: &str, ground_m: f64, today: &str) -> String {
        super::surface_commit(plan_json, ground_m, today, "settings")
    }

    const CAVE: &str = r#"{"unit":"m","name":"t","shots":[
        {"from":"A1","to":"B","distance":10,"azimuth":0,"inclination":0},
        {"from":"B","to":"C","distance":8,"azimuth":90,"inclination":-5}]}"#;

    fn status() -> serde_json::Value {
        serde_json::from_str(&geo_status()).unwrap()
    }

    fn station(label: &str) -> String {
        status()["stations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["label"] == label)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn set_loc(extra: &str) -> String {
        super::geo_set_location(
            &format!(
                r#"{{"station":"{}","lat":36.9,"lon":-86.4,"source":"typed"{extra}}}"#,
                station("A1")
            ),
            "settings",
        )
    }

    #[test]
    fn location_plan_and_commit_are_edits() {
        crate::open_demo(CAVE);
        assert!(geo_set_location("nope").starts_with("error"));
        assert!(
            geo_set_location(r#"{"station":"x","lat":99,"lon":0,"source":"typed"}"#)
                .starts_with("error")
        );
        assert!(
            geo_set_location(r#"{"station":"x","lat":1,"lon":0,"source":"typed"}"#)
                .starts_with("error: unknown station")
        );
        assert_eq!(surface_plan(true, true, false), "error: no location");
        assert_eq!(status()["candidate"]["kind"], "ask");
        assert_eq!(set_loc(""), "ok");
        assert_eq!(status()["candidate"]["kind"], "stored");
        assert_eq!(status()["location"]["lat"], 36.9);

        let plan: serde_json::Value =
            serde_json::from_str(&surface_plan(true, true, false)).unwrap();
        assert!(plan["zone"].as_i64().unwrap() == 16);
        assert!(plan["photo"]["url"].as_str().unwrap().starts_with("http"));
        let only: serde_json::Value =
            serde_json::from_str(&surface_plan(false, true, false)).unwrap();
        assert!(only.get("photo").is_none() && only.get("elevation").is_some());

        assert_eq!(
            surface_commit(&plan.to_string(), f64::NAN, "2026-10-27"),
            "ok"
        );
        let s = status();
        assert_eq!(s["surface"]["fetched"], "2026-10-27");
        assert!(
            s["location"]["ground_m"].is_null(),
            "NaN leaves ground alone"
        );
        assert_eq!(surface_commit(&plan.to_string(), 210.5, "2026-10-28"), "ok");
        assert_eq!(status()["location"]["ground_m"], 210.5);
        assert!(surface_commit("junk", 1.0, "x").starts_with("error"));

        // Layers with no elevation file: photo only, no contours.
        let l: serde_json::Value =
            serde_json::from_str(&surface_layers(&[], 5.0, false, false)).unwrap();
        assert_eq!(l["contours"].as_array().unwrap().len(), 0);
        assert!(l["photo_matrix"].is_array());
        assert_eq!(l["stale"], false);
        assert!(surface_ground(&plan.to_string(), &[]).is_nan());
        assert!(surface_pick(&[], 0.0, 0.0, 5.0, false).starts_with("error"));

        assert_eq!(set_contour_interval(5.0), "ok");
        assert_eq!(status()["interval_m"], 5.0);
        assert!(set_contour_interval(0.0).starts_with("error"));
    }

    fn plan(photo: bool, elevation: bool) -> serde_json::Value {
        serde_json::from_str(&surface_plan(photo, elevation, false)).unwrap()
    }

    #[test]
    fn refetching_one_part_keeps_the_window_and_a_moved_cave_drops_the_old_part() {
        crate::open_demo(CAVE);
        assert_eq!(set_loc(""), "ok");
        let both = plan(true, true);
        assert_eq!(surface_commit(&both.to_string(), f64::NAN, "d1"), "ok");
        // (i) same location, one part: the stored window is reused.
        let one = plan(false, true);
        assert_eq!(one["bounds"], both["bounds"]);
        assert_eq!(one["zone"], both["zone"]);
        assert_eq!(surface_commit(&one.to_string(), f64::NAN, "d2"), "ok");
        let s = status();
        assert!(s["surface"]["photo"].is_array() && s["surface"]["elevation"].is_array());

        // (ii) moved cave, one part: a new window, and the old photo goes.
        let moved = format!(
            r#"{{"station":"{}","lat":36.95,"lon":-86.4,"source":"typed"}}"#,
            station("A1")
        );
        assert_eq!(geo_set_location(&moved), "ok");
        let one = plan(false, true);
        assert_ne!(one["bounds"], both["bounds"]);
        assert_eq!(surface_commit(&one.to_string(), f64::NAN, "d3"), "ok");
        let s = status();
        assert!(s["surface"]["photo"].is_null(), "the old photo is dropped");
        assert!(s["surface"]["elevation"].is_array());
    }

    #[test]
    fn the_location_station_must_be_a_station() {
        crate::open_demo(CAVE);
        let shot = crate::DOC.with(|d| {
            let slot = d.borrow();
            slot.as_ref().unwrap().ids_of(Kind::Shot)[0].to_string()
        });
        let r = geo_set_location(&format!(
            r#"{{"station":"{shot}","lat":36.9,"lon":-86.4,"source":"typed"}}"#
        ));
        assert_eq!(r, "error: unknown station");
        assert_eq!(set_loc(""), "ok");
    }

    #[test]
    fn real_3dep_fixture_gives_contours_terrain_and_a_pick() {
        const TIF: &[u8] = include_bytes!("../../cave-geo/tests/data/bowling-green-200.tif");
        crate::open_demo(CAVE);
        let (lat, lon) = cave_geomag::utm_to_latlon(550100.0, 4094100.0, 16);
        let r = geo_set_location(&format!(
            r#"{{"station":"{}","lat":{lat},"lon":{lon},"source":"typed"}}"#,
            station("A1")
        ));
        assert_eq!(r, "ok");
        let plan = serde_json::json!({
            "zone": 16,
            "bounds": {"xmin": 550000.0, "ymin": 4094000.0, "xmax": 550200.0, "ymax": 4094200.0},
            "for_at": {"lat": lat, "lon": lon},
            "elevation": {"w": 200, "h": 200, "url": "x"},
        })
        .to_string();
        let ground = surface_ground(&plan, TIF);
        assert!(ground.is_finite());
        assert_eq!(surface_commit(&plan, ground, "2026-10-27"), "ok");

        let l: serde_json::Value =
            serde_json::from_str(&surface_layers(TIF, 2.0, false, false)).unwrap();
        assert!(!l["contours"].as_array().unwrap().is_empty());
        assert!(l.get("terrain").is_none());

        let g = Grid::parse_tiff(TIF).unwrap();
        let vals: Vec<f64> = (0..g.h)
            .flat_map(|r| (0..g.w).map(move |c| (c, r)))
            .filter_map(|(c, r)| g.at(c, r))
            .collect();
        let (lo, hi) = vals
            .iter()
            .fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
        let p: serde_json::Value =
            serde_json::from_str(&surface_pick(TIF, 0.0, 0.0, 30.0, false)).unwrap();
        let gm = p["ground_m"].as_f64().unwrap();
        assert!(gm >= lo && gm <= hi, "{gm} not in {lo}..{hi}");

        let t = surface_terrain(TIF, false);
        assert_eq!(t.len(), 2 + 200 * 200 * 3);
        assert_eq!((t[0], t[1]), (200.0, 200.0));
        assert!(surface_terrain(&[], false).is_empty());
    }

    #[test]
    fn plan_for_record_asks_again_for_exactly_what_is_recorded() {
        crate::open_demo(CAVE);
        assert_eq!(surface_plan_for_record(false), "error: no surface");
        assert_eq!(set_loc(""), "ok");
        let both = plan(true, true);
        assert_eq!(surface_commit(&both.to_string(), f64::NAN, "d1"), "ok");
        let again: serde_json::Value =
            serde_json::from_str(&surface_plan_for_record(false)).unwrap();
        assert_eq!(again, both, "same zone, bounds, for_at, sizes and URLs");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&surface_plan_for_record(true)).unwrap(),
            both
        );

        // The cave moves: a fresh plan gets a new window, the record's plan does not.
        let moved = format!(
            r#"{{"station":"{}","lat":36.95,"lon":-86.4,"source":"typed"}}"#,
            station("A1")
        );
        assert_eq!(geo_set_location(&moved), "ok");
        assert_ne!(plan(true, true)["bounds"], both["bounds"]);
        let again: serde_json::Value =
            serde_json::from_str(&surface_plan_for_record(false)).unwrap();
        assert_eq!(again, both);

        // Only the parts the record lists are asked for.
        let one = plan(false, true);
        assert_eq!(surface_commit(&one.to_string(), f64::NAN, "d2"), "ok");
        let again: serde_json::Value =
            serde_json::from_str(&surface_plan_for_record(false)).unwrap();
        assert!(again.get("photo").is_none());
        assert_eq!(again["elevation"], one["elevation"]);
        assert_eq!(again["bounds"], one["bounds"]);
    }

    #[test]
    fn location_and_surface_edits_land_on_the_named_history() {
        crate::open_demo(CAVE);
        let id = station("A1");
        let at =
            |lat: f64| format!(r#"{{"station":"{id}","lat":{lat},"lon":-86.4,"source":"typed"}}"#);
        assert!(
            super::geo_set_location(&at(36.9), "nowhere").starts_with("error: unknown undo scope")
        );
        assert!(
            status()["location"].is_null(),
            "a bad scope changes nothing"
        );
        assert_eq!(super::geo_set_location(&at(36.9), "settings"), "ok");
        assert_eq!(super::geo_set_location(&at(36.95), "map"), "ok");
        let p = plan(true, true);
        assert!(
            super::surface_commit(&p.to_string(), f64::NAN, "d", "nowhere").starts_with("error")
        );
        assert!(status()["surface"].is_null());
        assert_eq!(
            super::surface_commit(&p.to_string(), f64::NAN, "d", "map"),
            "ok"
        );

        // The settings history cannot reach the map's edits; the map's undo takes them back in order.
        assert!(crate::undo("map"));
        assert!(
            status()["surface"].is_null(),
            "the download is undone first"
        );
        assert_eq!(status()["location"]["lat"], 36.95);
        assert!(crate::undo("map"));
        assert_eq!(status()["location"]["lat"], 36.9, "then the move");
        assert!(!crate::undo("map"));
        assert!(crate::undo("settings"));
        assert!(status()["location"].is_null());
    }
}
