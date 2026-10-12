//! What the 3D canvas draws: stations, legs, splays and loop closures, in metres (x east, y north,
//! z up, relative to the survey's anchor).
//!
//! Derived data only: [`build`] reads a [`Document`] and runs the survey math; nothing is stored
//! back. The canvas stays dumb: it receives a [`Scene`] and draws it.

mod filter;
mod mesh;
mod profile;
mod section;
mod solved;
mod under;

pub use filter::{TripInfo, ViewFilter};
pub use mesh::{ColorMode, Mesh, passage_mesh};
pub use profile::{ProfileScene, RunInfo, profile_frames, profile_scene, runs};
pub use section::{Section, SectionLeg, section_cut, section_legs};
pub use under::Shading;

use cave_doc::{Document, Field, Kind};
use cave_id::Id;
use cave_survey::{LegKind, LegResult, Params, Splay, adjust, extract, resolve};
use serde::Serialize;
use std::collections::HashMap;

/// A point in 3D: east, north, up.
pub type Pt = [f64; 3];

/// A surveyed station.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StationDot {
    /// Label shown to people.
    pub label: String,
    /// Position in metres.
    pub at: Pt,
    /// True for a station the network is pinned on.
    pub anchor: bool,
}

/// How a leg is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LegStyle {
    /// An ordinary leg.
    New,
    /// A leg that closes a loop.
    Closure,
    /// A leg joining two separately anchored sections.
    Tie,
}

/// A surveyed line between two stations.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SceneLeg {
    /// The shot this leg came from (its id as text).
    pub shot: String,
    /// Start.
    pub from: Pt,
    /// End.
    pub to: Pt,
    /// How to draw it.
    pub style: LegStyle,
}

/// Where a loop did not close.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SceneClosure {
    /// The station where the error shows.
    pub at: Pt,
    /// Plan error in metres, as surveyed.
    pub horizontal: f64,
    /// Vertical error in metres, as surveyed.
    pub vertical: f64,
    /// Error as a percent of the ring length; absent for a tie.
    pub percent: Option<f64>,
}

/// Everything to draw.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Scene {
    /// Stations in survey order.
    pub stations: Vec<StationDot>,
    /// Survey legs.
    pub legs: Vec<SceneLeg>,
    /// Splay rays as `[x1, y1, z1, x2, y2, z2]`.
    pub splays: Vec<[f64; 6]>,
    /// The shot of each splay, in the same order as `splays` (ids as text).
    pub splay_shots: Vec<String>,
    /// Passage wall lines (both sides), each a run of 3D points built from LRUD and splays.
    pub walls: Vec<Vec<Pt>>,
    /// The same lines split by whether another passage lies over them (drawn dim and dashed).
    pub shading: Shading,
    /// Loop and tie misclosures (as surveyed, never from adjusted coordinates).
    pub closures: Vec<SceneClosure>,
    /// `[min_x, min_y, min_z, max_x, max_y, max_z]`, or `None` when empty.
    pub bounds: Option<[f64; 6]>,
    /// True if positions are least-squares adjusted.
    pub adjusted: bool,
    /// Shots left out, for the console and Check tools.
    pub skipped: usize,
    /// Palette types that have something in this scene, ignoring the view filter (what the filter lists).
    pub types: Vec<String>,
    /// Every trip with shots in the cave, in trip order.
    pub trips: Vec<TripInfo>,
}

/// Splays aimed this close (degrees) to a leg at their station run along the passage and are not drawn.
pub const ALONG_PASSAGE_DEG: f64 = 20.0;

/// The directions of the legs at each station, to tell a splay that runs along the passage from
/// one that crosses it. Judged in 3D, so a shaft's own splays are treated the same as a level
/// passage's, and no bearing is ever invented for a pitch.
struct AlongPassage {
    at: HashMap<Id, Vec<Pt>>,
}

impl AlongPassage {
    fn new(legs: &[cave_survey::LegResult], pos: &dyn Fn(Id) -> Option<cave_survey::Vec3>) -> Self {
        let mut at: HashMap<Id, Vec<Pt>> = HashMap::new();
        for l in legs {
            if let (Some(a), Some(b)) = (pos(l.from), pos(l.to)) {
                let d = [b.x - a.x, b.y - a.y, b.z - a.z];
                at.entry(l.from).or_default().push(d);
                at.entry(l.to).or_default().push(d);
            }
        }
        AlongPassage { at }
    }

    /// True if the splay `offset` from `station` lies within [`ALONG_PASSAGE_DEG`] of a leg there
    /// (either way along it).
    fn hides(&self, station: Id, offset: cave_survey::Vec3) -> bool {
        let o = [offset.x, offset.y, offset.z];
        let lo = (o[0] * o[0] + o[1] * o[1] + o[2] * o[2]).sqrt();
        let min_cos = ALONG_PASSAGE_DEG.to_radians().cos();
        lo > 1e-9
            && self.at.get(&station).is_some_and(|ds| {
                ds.iter().any(|d| {
                    let ld = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                    ld > 1e-9
                        && ((o[0] * d[0] + o[1] * d[1] + o[2] * d[2]) / (lo * ld)).abs() >= min_cos
                })
            })
    }
}

/// Builds the scene, adjusting loops when `use_adjustment` is set and the solver converges.
pub fn build(doc: &Document, use_adjustment: bool) -> Scene {
    build_seen_from(
        doc,
        use_adjustment,
        under::TOP,
        None,
        &ViewFilter::default(),
    )
}

/// [`build`] showing only what `filter` leaves visible.
pub fn build_filtered(doc: &Document, use_adjustment: bool, filter: &ViewFilter) -> Scene {
    build_seen_from(doc, use_adjustment, under::TOP, None, filter)
}

/// Only the over/under split of the lines, for a camera looking from `view` (the direction from
/// the cave toward the camera), or from the point `eye` for a perspective camera. Cheap to ask again whenever the camera settles.
pub fn shading(
    doc: &Document,
    use_adjustment: bool,
    view: Pt,
    eye: Option<Pt>,
    filter: &ViewFilter,
) -> Shading {
    build_seen_from(doc, use_adjustment, view, eye, filter).shading
}

/// [`build`] with the over/under split worked out for a camera at `view`.
pub fn build_seen_from(
    doc: &Document,
    use_adjustment: bool,
    view: Pt,
    eye: Option<Pt>,
    filter: &ViewFilter,
) -> Scene {
    let ex = extract(doc);
    let r = resolve(&ex.legs, &ex.fixed);
    let trip_of = |shot: Id| doc.entity(shot).and_then(|e| e.reference(Field::ShotTrip));
    // What the view filter leaves: shots of hidden trips take no part in the picture at all, so
    // they neither draw nor shade, and walls are built only from what stays.
    let legs: Vec<LegResult> = r
        .legs
        .iter()
        .copied()
        .filter(|l| filter.trip_shown(trip_of(l.shot)))
        .collect();
    let vis_splays: Vec<Splay> = ex
        .splays
        .iter()
        .filter(|s| filter.trip_shown(trip_of(s.shot)))
        .cloned()
        .collect();
    let touched = |legs: &[LegResult], splays: &[Splay]| -> std::collections::HashSet<Id> {
        legs.iter()
            .flat_map(|l| [l.from, l.to])
            .chain(splays.iter().map(|s| s.from))
            .collect()
    };
    let touched_any = touched(&r.legs, &ex.splays);
    let touched_vis = touched(&legs, &vis_splays);
    let adj = use_adjustment
        .then(|| adjust(&ex.legs, &r, &Params::default()))
        .filter(|a| a.adjusted);
    let pos = |id: Id| -> Option<cave_survey::Vec3> {
        match &adj {
            Some(a) => a.stations.get(&id).copied(),
            None => r.stations.get(&id).map(|p| p.pos),
        }
    };

    let mut ordered: Vec<(&Id, usize)> = r.stations.iter().map(|(id, p)| (id, p.seq)).collect();
    ordered.sort_by_key(|(_, seq)| *seq);
    let labels: HashMap<Id, String> = ordered
        .iter()
        .map(|(id, _)| {
            (
                **id,
                doc.entity(**id)
                    .map(|e| e.text(cave_doc::Field::StationLabel))
                    .unwrap_or_default(),
            )
        })
        .collect();

    let mut scene = Scene {
        adjusted: adj.is_some(),
        skipped: ex.skipped.len(),
        ..Scene::default()
    };
    let has_leg = |k: LegKind| r.legs.iter().any(|l| l.kind == k);
    let has_walls = !ex.lruds.is_empty() || !ex.start_lruds.is_empty() || !ex.splays.is_empty();
    scene.types = [
        ("wall", has_walls),
        ("centerline", has_leg(LegKind::New)),
        ("splay", ex.splays.iter().any(|s| !s.exclude_from_plot)),
        ("station", !r.stations.is_empty()),
        ("label", !r.stations.is_empty()),
        ("closure", has_leg(LegKind::Closure)),
        ("tie", has_leg(LegKind::Tie)),
        ("closure-note", !r.closures.is_empty()),
    ]
    .into_iter()
    .filter(|(_, present)| *present)
    .map(|(k, _)| k.to_string())
    .collect();
    let mut trips: Vec<(f64, Id, String)> = doc
        .ids_of(Kind::Trip)
        .into_iter()
        .filter_map(|t| {
            let e = doc.entity(t)?;
            Some((
                e.number(Field::TripOrder).unwrap_or(0.0),
                t,
                e.text(Field::TripName),
            ))
        })
        .collect();
    trips.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    scene.trips = trips
        .into_iter()
        .map(|(_, id, name)| TripInfo {
            id: id.to_string(),
            name,
        })
        .collect();
    for (id, _) in ordered
        .iter()
        .filter(|(id, _)| touched_vis.contains(*id) || !touched_any.contains(*id))
        .filter(|_| filter.type_shown("station"))
    {
        if let Some(p) = pos(**id) {
            scene.stations.push(StationDot {
                label: labels[*id].clone(),
                at: [p.x, p.y, p.z],
                anchor: r.anchors.contains(*id),
            });
        }
    }
    for l in &legs {
        if let (Some(a), Some(b)) = (pos(l.from), pos(l.to)) {
            let style = match l.kind {
                LegKind::New => LegStyle::New,
                LegKind::Closure => LegStyle::Closure,
                LegKind::Tie => LegStyle::Tie,
            };
            let key = match style {
                LegStyle::New => "centerline",
                LegStyle::Closure => "closure",
                LegStyle::Tie => "tie",
            };
            if !filter.type_shown(key) {
                continue;
            }
            scene.legs.push(SceneLeg {
                shot: l.shot.to_string(),
                from: [a.x, a.y, a.z],
                to: [b.x, b.y, b.z],
                style,
            });
        }
    }
    let along = AlongPassage::new(&r.legs, &pos);
    let shown: Vec<_> = vis_splays
        .iter()
        .filter(|_| filter.type_shown("splay"))
        .filter(|s| !s.exclude_from_plot && !along.hides(s.from, s.offset))
        .collect();
    for s in shown.iter().copied() {
        if let Some(a) = pos(s.from) {
            scene.splay_shots.push(s.shot.to_string());
            scene.splays.push([
                a.x,
                a.y,
                a.z,
                a.x + s.offset.x,
                a.y + s.offset.y,
                a.z + s.offset.z,
            ]);
        }
    }
    let placed: std::collections::BTreeMap<Id, cave_survey::Vec3> = r
        .stations
        .keys()
        .filter_map(|id| pos(*id).map(|p| (*id, p)))
        .collect();
    let walls = cave_walls::wall_runs(&cave_walls::WallInput {
        stations: &placed,
        legs: &legs,
        readings: &ex.readings,
        lruds: &ex.lruds,
        start_lruds: &ex.start_lruds,
        splays: &vis_splays,
    });
    let feet = under::Footprints::new(&legs, &pos, &ex.lruds, view, eye);
    let drawn_walls = if filter.type_shown("wall") {
        walls.left.iter().chain(&walls.right).collect()
    } else {
        Vec::new()
    };
    for run in drawn_walls {
        // Only the outside of the shell: loops where the line doubles back are dropped.
        let shell = cave_walls::outer_shell(&run.points);
        let line: Vec<Pt> = shell.iter().map(|p| [p.x, p.y, p.z]).collect();
        let own: std::collections::HashSet<Id> = run.stations.iter().copied().collect();
        let sh = &mut scene.shading;
        feet.split(&line, &own, &mut sh.walls_over, &mut sh.walls_under);
        scene.walls.push(line);
    }
    for (sp, ray) in shown
        .iter()
        .copied()
        .filter(|s| pos(s.from).is_some())
        .zip(&scene.splays)
    {
        let own = std::collections::HashSet::from([sp.from]);
        let line = [[ray[0], ray[1], ray[2]], [ray[3], ray[4], ray[5]]];
        let sh = &mut scene.shading;
        feet.split(&line, &own, &mut sh.splays_over, &mut sh.splays_under);
    }
    for l in legs
        .iter()
        .filter(|l| l.kind == LegKind::New && filter.type_shown("centerline"))
    {
        if let (Some(a), Some(b)) = (pos(l.from), pos(l.to)) {
            let own = std::collections::HashSet::from([l.from, l.to]);
            let sh = &mut scene.shading;
            feet.split(
                &[[a.x, a.y, a.z], [b.x, b.y, b.z]],
                &own,
                &mut sh.legs_over,
                &mut sh.legs_under,
            );
        }
    }
    for c in r.closures.iter().filter(|c| {
        filter.type_shown("closure-note")
            && (touched_vis.contains(&c.at) || !touched_any.contains(&c.at))
    }) {
        if let Some(p) = pos(c.at) {
            scene.closures.push(SceneClosure {
                at: [p.x, p.y, p.z],
                horizontal: c.horizontal,
                vertical: c.vertical,
                percent: c.percent,
            });
        }
    }
    let mut pts: Vec<Pt> = scene.stations.iter().map(|s| s.at).collect();
    pts.extend(scene.splays.iter().map(|s| [s[3], s[4], s[5]]));
    pts.extend(scene.walls.iter().flatten().copied());
    scene.bounds = pts.iter().fold(None, |b: Option<[f64; 6]>, p| {
        Some(match b {
            None => [p[0], p[1], p[2], p[0], p[1], p[2]],
            Some(b) => [
                b[0].min(p[0]),
                b[1].min(p[1]),
                b[2].min(p[2]),
                b[3].max(p[0]),
                b[4].max(p[1]),
                b[5].max(p[2]),
            ],
        })
    });
    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use cave_doc::ActorId;

    const SQUARE: &str = r#"{"unit":"m","name":"sq","shots":[
        {"from":"A4","to":"A1","distance":10.5,"azimuth":270,"inclination":0},
        {"from":"A1","to":"A2","distance":10,"azimuth":0,"inclination":0},
        {"from":"A2","to":"A3","distance":10,"azimuth":90,"inclination":0},
        {"from":"A3","to":"A4","distance":10,"azimuth":180,"inclination":0},
        {"from":"A2","to":"","distance":3,"azimuth":45,"inclination":0,"splay":true}]}"#;

    fn square() -> Document {
        cave_fixture::from_shot_list(SQUARE, ActorId(1)).unwrap()
    }

    #[test]
    fn square_scene_has_stations_legs_splay_and_one_loop() {
        let s = build(&square(), false);
        assert_eq!(s.stations.len(), 4);
        assert_eq!(s.legs.len(), 4);
        assert_eq!(s.splays.len(), 1);
        assert_eq!(s.closures.len(), 1);
        assert!(!s.adjusted);
        assert!((s.closures[0].horizontal - 0.5).abs() < 1e-9);
        assert_eq!(
            s.legs
                .iter()
                .filter(|l| l.style == LegStyle::Closure)
                .count(),
            1
        );
        assert!(s.bounds.is_some());
    }

    #[test]
    fn adjusting_moves_stations_but_not_the_reported_closure() {
        let raw = build(&square(), false);
        let adj = build(&square(), true);
        assert!(adj.adjusted);
        assert_ne!(raw.stations[2].at, adj.stations[2].at);
        assert_eq!(raw.closures[0].horizontal, adj.closures[0].horizontal);
    }

    #[test]
    fn elevation_is_kept_in_stations_legs_and_bounds() {
        let text = r#"{"unit":"m","name":"up","shots":[
            {"from":"A","to":"B","distance":10,"azimuth":0,"inclination":30},
            {"from":"B","to":"","distance":2,"azimuth":90,"inclination":-90,"splay":true}]}"#;
        let doc = cave_fixture::from_shot_list(text, ActorId(1)).unwrap();
        let s = build(&doc, false);
        assert!(
            (s.stations[1].at[2] - 5.0).abs() < 1e-9,
            "10 m at 30 degrees rises 5 m"
        );
        assert!((s.legs[0].to[2] - 5.0).abs() < 1e-9);
        assert!(
            (s.splays[0][5] - 3.0).abs() < 1e-9,
            "a splay straight down drops 2 m from z=5"
        );
        let b = s.bounds.unwrap();
        assert!(b[2] <= 0.0 + 1e-9 && b[5] >= 5.0 - 1e-9);
    }

    #[test]
    fn measured_walls_become_wall_lines_in_3d() {
        let text = r#"{"unit":"m","name":"w","shots":[
            {"from":"A","to":"B","distance":10,"azimuth":0,"inclination":0,"left":1,"right":2},
            {"from":"B","to":"C","distance":10,"azimuth":0,"inclination":10,"left":1,"right":2}]}"#;
        let doc = cave_fixture::from_shot_list(text, ActorId(1)).unwrap();
        let s = build(&doc, false);
        assert_eq!(s.walls.len(), 2, "one run per side");
        let left = s
            .walls
            .iter()
            .find(|w| w[0][0] < 0.0)
            .expect("left wall is west of a north-running passage");
        assert!((left[0][0] + 1.0).abs() < 1e-9 && (left[1][0] + 1.0).abs() < 1e-9);
        assert!(left[1][2] > left[0][2], "the wall climbs with the passage");
    }

    #[test]
    fn splays_along_the_passage_are_hidden_and_crossing_ones_kept() {
        for (name, incl) in [("level", 0.0), ("shaft", -90.0)] {
            let text = format!(
                r#"{{"unit":"m","name":"{name}","shots":[
                {{"from":"A","to":"B","distance":10,"azimuth":0,"inclination":{incl}}},
                {{"from":"B","to":"","distance":30,"azimuth":0,"inclination":{incl},"splay":true}},
                {{"from":"B","to":"","distance":4,"azimuth":0,"inclination":{inc2},"splay":true}}]}}"#,
                inc2 = if incl == 0.0 { 90.0 } else { 0.0 },
            );
            let doc = cave_fixture::from_shot_list(&text, ActorId(1)).unwrap();
            let s = build(&doc, false);
            assert_eq!(
                s.splays.len(),
                1,
                "{name}: the along-passage splay is hidden"
            );
            assert_eq!(s.splay_shots.len(), 1);
            let r = s.splays[0];
            let len =
                ((r[3] - r[0]).powi(2) + (r[4] - r[1]).powi(2) + (r[5] - r[2]).powi(2)).sqrt();
            assert!((len - 4.0).abs() < 1e-9, "{name}: the crossing splay stays");
        }
    }

    /// Two trips: T1 surveys A-B (walls measured), T2 surveys B-C.
    const TWO_TRIPS: &str = r#"{"unit":"m","name":"two","trips":[{"name":"T1"},{"name":"T2"}],"shots":[
        {"from":"A","to":"B","distance":10,"azimuth":0,"inclination":0,"left":1,"right":1,"trip":0},
        {"from":"B","to":"","distance":3,"azimuth":45,"inclination":0,"splay":true,"trip":0},
        {"from":"B","to":"C","distance":10,"azimuth":90,"inclination":0,"left":1,"right":1,"trip":1}]}"#;

    fn filter(types: &[&str], trips: &[&str]) -> ViewFilter {
        ViewFilter {
            hidden_types: types.iter().map(|s| s.to_string()).collect(),
            hidden_trips: trips.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn an_empty_filter_changes_nothing() {
        let doc = cave_fixture::from_shot_list(TWO_TRIPS, ActorId(1)).unwrap();
        assert_eq!(
            build(&doc, false),
            build_filtered(&doc, false, &ViewFilter::default())
        );
    }

    #[test]
    fn the_scene_lists_the_types_and_trips_the_filter_can_hide() {
        let doc = cave_fixture::from_shot_list(TWO_TRIPS, ActorId(1)).unwrap();
        let s = build(&doc, false);
        assert_eq!(
            s.trips.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            ["T1", "T2"]
        );
        for k in ["wall", "centerline", "splay", "station", "label"] {
            assert!(s.types.iter().any(|t| t == k), "{k}");
        }
        assert!(!s.types.iter().any(|t| t == "tie"));
    }

    #[test]
    fn hiding_a_type_removes_only_that_type() {
        let doc = cave_fixture::from_shot_list(TWO_TRIPS, ActorId(1)).unwrap();
        let all = build(&doc, false);
        let s = build_filtered(&doc, false, &filter(&["splay"], &[]));
        assert!(all.splays.len() == 1 && s.splays.is_empty() && s.shading.splays_over.is_empty());
        assert_eq!(s.legs.len(), all.legs.len());
        assert_eq!(s.walls, all.walls);
        let s = build_filtered(&doc, false, &filter(&["wall"], &[]));
        assert!(
            s.walls.is_empty()
                && s.shading.walls_over.is_empty()
                && s.shading.walls_under.is_empty()
        );
        assert_eq!(s.splays.len(), 1);
        let s = build_filtered(&doc, false, &filter(&["centerline", "station"], &[]));
        assert!(s.legs.is_empty() && s.stations.is_empty() && !s.walls.is_empty());
    }

    #[test]
    fn hiding_a_trip_hides_its_lines_splays_walls_and_lonely_stations() {
        let doc = cave_fixture::from_shot_list(TWO_TRIPS, ActorId(1)).unwrap();
        let all = build(&doc, false);
        let t1 = all.trips[0].id.clone();
        let s = build_filtered(&doc, false, &filter(&[], &[&t1]));
        assert_eq!(s.legs.len(), 1, "only T2's leg B-C is left");
        assert!(s.splays.is_empty(), "the splay belongs to T1");
        let labels: Vec<_> = s.stations.iter().map(|d| d.label.as_str()).collect();
        assert_eq!(labels, ["B", "C"], "A is only on T1; B is shared and stays");
        assert!(
            !s.walls.is_empty() && s.walls.iter().flatten().all(|p| p[1] > 5.0),
            "no wall points down at station A, which only T1 surveyed"
        );
        assert_eq!(s.trips.len(), 2, "the list of trips is never filtered");
        let none = build_filtered(&doc, false, &filter(&[], &[&t1, &all.trips[1].id]));
        assert!(none.legs.is_empty() && none.stations.is_empty() && none.walls.is_empty());
    }

    #[test]
    fn a_hidden_trip_does_not_shade_other_passages() {
        // T2 runs over T1 (higher), so T1's lines dim and dash; with T2 hidden, T1 is in the open.
        let text = r#"{"unit":"m","name":"o","trips":[{"name":"T1"},{"name":"T2"}],"shots":[
            {"from":"A","to":"B","distance":20,"azimuth":90,"inclination":0,"trip":0},
            {"from":"B","to":"C","distance":10,"azimuth":0,"inclination":90,"trip":1},
            {"from":"C","to":"D","distance":20,"azimuth":270,"inclination":0,"trip":1}]}"#;
        let doc = cave_fixture::from_shot_list(text, ActorId(1)).unwrap();
        let all = build(&doc, false);
        assert!(!all.shading.legs_under.is_empty());
        let s = build_filtered(&doc, false, &filter(&[], &[&all.trips[1].id]));
        assert!(s.shading.legs_under.is_empty() && !s.shading.legs_over.is_empty());
    }

    #[test]
    fn empty_document_has_empty_scene() {
        let s = build(&Document::new(ActorId(1)), true);
        assert!(s.stations.is_empty() && s.bounds.is_none());
    }
}
