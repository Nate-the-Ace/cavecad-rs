//! What the 3D canvas draws: stations, legs, splays and loop closures, in metres (x east, y north,
//! z up, relative to the survey's anchor).
//!
//! Derived data only: [`build`] reads a [`Document`] and runs the survey math; nothing is stored
//! back. The canvas stays dumb: it receives a [`Scene`] and draws it.

mod profile;

pub use profile::{ProfileScene, RunInfo, profile_scene, runs};

use cave_doc::Document;
use cave_id::Id;
use cave_survey::{LegKind, Params, adjust, extract, resolve};
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
    /// Passage wall lines (both sides), each a run of 3D points built from LRUD and splays.
    pub walls: Vec<Vec<Pt>>,
    /// Loop and tie misclosures (as surveyed, never from adjusted coordinates).
    pub closures: Vec<SceneClosure>,
    /// `[min_x, min_y, min_z, max_x, max_y, max_z]`, or `None` when empty.
    pub bounds: Option<[f64; 6]>,
    /// True if positions are least-squares adjusted.
    pub adjusted: bool,
    /// Shots left out, for the console and Check tools.
    pub skipped: usize,
}

/// Builds the scene, adjusting loops when `use_adjustment` is set and the solver converges.
pub fn build(doc: &Document, use_adjustment: bool) -> Scene {
    let ex = extract(doc);
    let r = resolve(&ex.legs, &ex.fixed);
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
    for (id, _) in &ordered {
        if let Some(p) = pos(**id) {
            scene.stations.push(StationDot {
                label: labels[*id].clone(),
                at: [p.x, p.y, p.z],
                anchor: r.anchors.contains(*id),
            });
        }
    }
    for l in &r.legs {
        if let (Some(a), Some(b)) = (pos(l.from), pos(l.to)) {
            let style = match l.kind {
                LegKind::New => LegStyle::New,
                LegKind::Closure => LegStyle::Closure,
                LegKind::Tie => LegStyle::Tie,
            };
            scene.legs.push(SceneLeg {
                from: [a.x, a.y, a.z],
                to: [b.x, b.y, b.z],
                style,
            });
        }
    }
    for s in ex.splays.iter().filter(|s| !s.exclude_from_plot) {
        if let Some(a) = pos(s.from) {
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
        legs: &r.legs,
        readings: &ex.readings,
        lruds: &ex.lruds,
        start_lruds: &ex.start_lruds,
        splays: &ex.splays,
    });
    for run in walls.left.iter().chain(&walls.right) {
        scene
            .walls
            .push(run.points.iter().map(|p| [p.x, p.y, p.z]).collect());
    }
    for c in &r.closures {
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
    fn empty_document_has_empty_scene() {
        let s = build(&Document::new(ActorId(1)), true);
        assert!(s.stations.is_empty() && s.bounds.is_none());
    }
}
