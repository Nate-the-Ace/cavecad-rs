//! The Profile panel's scene: a run unrolled onto one horizontal axis, with the sketches drawn on it.
//!
//! Coordinates are `(s, z)` in metres: `s` is distance along the run, `z` is elevation.

use crate::solved::{Solved, solve};
use cave_doc::{Document, Field, Frame, Kind, Value};
use cave_id::Id;
use cave_profile::{Graph, LegInfo, group_runs, hierarchy, unroll_band};
use cave_survey::LegKind;
use serde::Serialize;
use std::collections::HashMap;

/// A run offered in the panel's dropdown.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunInfo {
    /// The run key (`A`, `B1`, `A13a`).
    pub key: String,
    /// Stations on the band.
    pub stations: usize,
    /// Length of the band in metres.
    pub length: f64,
    /// Label of the parent station this run ties in at, if any.
    pub tie: Option<String>,
}

/// A station on the band.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BandDot {
    /// Label.
    pub label: String,
    /// Distance along the run.
    pub s: f64,
    /// Elevation.
    pub z: f64,
    /// Measured ceiling height above the station.
    pub up: Option<f64>,
    /// Measured floor depth below the station.
    pub down: Option<f64>,
}

/// A stroke sketched on the band.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Sketch {
    /// The feature's id, as text.
    pub id: String,
    /// What it is (`wall`, `ledge`, `sketch`, ...): the palette type that styles it.
    pub kind: String,
    /// Points `(s, z)`.
    pub points: Vec<[f64; 2]>,
    /// Pen weight per point, when drawn with pressure.
    pub widths: Option<Vec<f64>>,
    /// True when the curve loops back from the last point to the first.
    pub closed: bool,
}

/// A station's cross-section sketches as the band shows them, next to the station.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BandSection {
    /// The station's id as text.
    pub id: String,
    /// Its label.
    pub label: String,
    /// Where the station is on the band.
    pub s: f64,
    /// Its elevation.
    pub z: f64,
    /// The strokes, `[across, up]` metres from the station.
    pub sketches: Vec<crate::SectionSketch>,
    /// Where they were placed by hand: offset from the station (along the band, elevation);
    /// `None` until placed, when the band draws them beside the station.
    pub placement: Option<[f64; 2]>,
}

/// Everything the Profile panel draws for one run.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProfileScene {
    /// The run key.
    pub key: String,
    /// Parent tie station label, if the band starts at one.
    pub tie: Option<String>,
    /// Stations in band order.
    pub stations: Vec<BandDot>,
    /// Surveyed legs as `[s1, z1, s2, z2]`.
    pub legs: Vec<[f64; 4]>,
    /// Ceiling lines.
    pub ceiling: Vec<Vec<[f64; 2]>>,
    /// Floor lines.
    pub floor: Vec<Vec<[f64; 2]>>,
    /// Strokes sketched on this band, oldest first.
    pub sketches: Vec<Sketch>,
    /// Cross-section sketches of the stations on this band.
    pub sections: Vec<BandSection>,
    /// Sketches that cannot be placed because a station they hang on is not on this band.
    pub unplaced: usize,
    /// How many run stations are not on the band (side branches).
    pub omitted: usize,
    /// Plain-language note when the band stopped short.
    pub note: Option<String>,
    /// `[min_s, min_z, max_s, max_z]` around everything, or `None` when empty.
    pub bounds: Option<[f64; 4]>,
}

fn leg_infos(s: &Solved) -> Vec<LegInfo> {
    let plan: HashMap<Id, f64> =
        s.ex.legs
            .iter()
            .map(|l| (l.shot, l.offset.horizontal()))
            .collect();
    let infos: Vec<LegInfo> = s
        .resolved
        .legs
        .iter()
        .filter_map(|l| {
            plan.get(&l.shot).map(|p| LegInfo {
                shot: l.shot,
                from: l.from,
                to: l.to,
                kind: l.kind,
                plan: *p,
            })
        })
        .collect();
    infos
}

fn graph(s: &Solved) -> Graph {
    Graph::new(&leg_infos(s))
}

/// The station each run ties in at, decided by the shot graph (the station on the parent's side
/// of the leg that placed the run), as the old app does. Only stations with a position count.
fn tie_stations(s: &Solved, runs: &cave_profile::Runs) -> HashMap<String, Id> {
    let seq: HashMap<Id, usize> = s.order.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    hierarchy(runs, &leg_infos(s), &seq)
        .ties
        .into_iter()
        .filter_map(|(k, t)| t.filter(|t| s.positions.contains_key(t)).map(|t| (k, t)))
        .collect()
}

/// Every run in the cave with its band length, in survey order.
pub fn runs(doc: &Document, use_adjustment: bool) -> Vec<RunInfo> {
    let s = solve(doc, use_adjustment);
    let g = graph(&s);
    let z_of = |id: Id| s.positions.get(&id).map(|p| p.z);
    let grouped = group_runs(&s.labels, &s.order);
    let ties = tie_stations(&s, &grouped);
    grouped
        .runs
        .iter()
        .map(|run| {
            let tie = ties.get(&run.key).copied();
            let band = unroll_band(run, tie, &g, &s.labels, &z_of, &s.ex.lruds);
            RunInfo {
                key: run.key.clone(),
                stations: band.stations.len(),
                length: band.stations.last().map_or(0.0, |x| x.s),
                tie: band.tie.map(|t| s.labels[&t].clone()),
            }
        })
        .collect()
}

/// The band for one run with its sketches, or `None` if the cave has no such run.
pub fn profile_scene(doc: &Document, key: &str, use_adjustment: bool) -> Option<ProfileScene> {
    let s = solve(doc, use_adjustment);
    let g = graph(&s);
    let z_of = |id: Id| s.positions.get(&id).map(|p| p.z);
    let grouped = group_runs(&s.labels, &s.order);
    let run = grouped.runs.iter().find(|r| r.key == key)?;
    let tie = tie_stations(&s, &grouped).get(key).copied();
    let band = unroll_band(run, tie, &g, &s.labels, &z_of, &s.ex.lruds);

    // Sketches are anchored to stations: place them from where the stations are NOW.
    let at: HashMap<Id, [f64; 2]> = band.stations.iter().map(|x| (x.id, [x.s, x.z])).collect();
    let mut unplaced = 0;
    let sketches: Vec<Sketch> = doc
        .ids_of(Kind::Feature)
        .into_iter()
        .filter_map(|id| {
            let e = doc.entity(id)?;
            if e.text(Field::FeatureView) != cave_draw::VIEW_PROFILE
                || e.text(Field::FeatureRun) != key
            {
                return None;
            }
            let Value::Anchored(Some(anchored)) = e.get(Field::FeatureAnchored) else {
                return None;
            };
            let Some(points) = anchored.place(&|st| at.get(&st).copied()) else {
                unplaced += 1;
                return None;
            };
            Some(Sketch {
                id: id.to_string(),
                kind: e.text(Field::FeatureType),
                points,
                widths: anchored.widths.clone(),
                closed: anchored.closed,
            })
        })
        .collect();

    let all_sketches = crate::sketches_by_station(doc, &crate::ViewFilter::default());
    let placed = crate::placements(doc, "profile");
    let sections: Vec<BandSection> = band
        .stations
        .iter()
        .filter_map(|x| {
            Some(BandSection {
                id: x.id.to_string(),
                label: x.label.clone(),
                s: x.s,
                z: x.z,
                sketches: all_sketches.get(&x.id)?.clone(),
                placement: placed.get(&x.id).map(|p| [p[0], p[1]]),
            })
        })
        .collect();
    let ceiling = band.ceiling_runs();
    let floor = band.floor_runs();
    let mut pts: Vec<[f64; 2]> = band.stations.iter().map(|x| [x.s, x.z]).collect();
    pts.extend(ceiling.iter().chain(&floor).flatten().copied());
    pts.extend(sketches.iter().flat_map(|k| k.points.iter().copied()));
    let bounds = pts.iter().fold(None, |b: Option<[f64; 4]>, p| {
        Some(match b {
            None => [p[0], p[1], p[0], p[1]],
            Some(b) => [
                b[0].min(p[0]),
                b[1].min(p[1]),
                b[2].max(p[0]),
                b[3].max(p[1]),
            ],
        })
    });
    let note = band.stopped.map(|(id, _)| {
        format!(
            "The band stops at {}: no usable connection beyond it.",
            s.labels[&id]
        )
    });
    Some(ProfileScene {
        key: band.key,
        tie: band.tie.map(|t| s.labels[&t].clone()),
        stations: band
            .stations
            .iter()
            .map(|x| BandDot {
                label: x.label.clone(),
                s: x.s,
                z: x.z,
                up: x.up,
                down: x.down,
            })
            .collect(),
        legs: band
            .legs
            .iter()
            .filter(|l| l.kind != LegKind::Closure)
            .map(|l| [l.from_s, l.from_z, l.to_s, l.to_z])
            .collect(),
        ceiling,
        floor,
        sketches,
        sections,
        unplaced,
        omitted: band.omitted.len(),
        note,
        bounds,
    })
}

/// A profile sketch placed in the cave itself, in the vertical plane of its run.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Sketch3d {
    /// The feature's id as text.
    pub id: String,
    /// What it is (`wall`, `ledge`, ...): the palette type that styles it.
    pub kind: String,
    /// The run (key) the stroke was drawn on (empty for a line drawn in plan).
    pub run: String,
    /// The stroke in the map's metres (east, north, up).
    pub points: Vec<[f64; 3]>,
    /// Pen weight per point, when drawn with pressure.
    pub widths: Option<Vec<f64>>,
    /// True when the curve loops back from the last point to the first.
    pub closed: bool,
}

/// Every profile sketch placed along its run in 3D, so the Map can show it where it belongs: each
/// point sits in the vertical plane through the leg it hangs on, `s` along the leg's plan
/// direction and `z` up, relative to the leg's start station. A sketch whose stations are gone is
/// left out. Seen from straight above or below such a drawing collapses onto the centreline, which
/// is why the viewer hides it there.
pub fn profile_sketches_3d(doc: &Document, use_adjustment: bool) -> Vec<Sketch3d> {
    let s = solve(doc, use_adjustment);
    let g = graph(&s);
    let z_of = |id: Id| s.positions.get(&id).map(|p| p.z);
    let grouped = group_runs(&s.labels, &s.order);
    let ties = tie_stations(&s, &grouped);
    let mut bands: HashMap<String, HashMap<Id, [f64; 2]>> = HashMap::new();
    let mut out = Vec::new();
    for id in doc.ids_of(Kind::Feature) {
        let Some(e) = doc.entity(id) else { continue };
        if e.text(Field::FeatureView) != cave_draw::VIEW_PROFILE {
            continue;
        }
        let key = e.text(Field::FeatureRun);
        let Value::Anchored(Some(anchored)) = e.get(Field::FeatureAnchored) else {
            continue;
        };
        let at = bands.entry(key.clone()).or_insert_with(|| {
            grouped
                .runs
                .iter()
                .find(|r| r.key == key)
                .map(|run| {
                    unroll_band(
                        run,
                        ties.get(&key).copied(),
                        &g,
                        &s.labels,
                        &z_of,
                        &s.ex.lruds,
                    )
                    .stations
                    .iter()
                    .map(|x| (x.id, [x.s, x.z]))
                    .collect()
                })
                .unwrap_or_default()
        });
        let Some(placed) = anchored.place_with_frames(&|st| at.get(&st).copied()) else {
            continue;
        };
        let mut points = Vec::with_capacity(placed.len());
        for (frame, p) in &placed {
            let (a, b) = anchored.frames[*frame];
            let (Some(a3), Some(b3), Some(pa)) =
                (s.positions.get(&a), s.positions.get(&b), at.get(&a))
            else {
                points.clear();
                break;
            };
            // The leg's direction on the ground; a leg straight up or down has none, so north.
            let (hx, hy) = (b3.x - a3.x, b3.y - a3.y);
            let hl = hx.hypot(hy);
            let (hx, hy) = if hl > 1e-6 {
                (hx / hl, hy / hl)
            } else {
                (0.0, 1.0)
            };
            let (ds, dz) = (p[0] - pa[0], p[1] - pa[1]);
            points.push([a3.x + hx * ds, a3.y + hy * ds, a3.z + dz]);
        }
        if points.len() == placed.len() {
            out.push(Sketch3d {
                id: id.to_string(),
                kind: e.text(Field::FeatureType),
                run: key,
                points,
                widths: anchored.widths.clone(),
                closed: anchored.closed,
            });
        }
    }
    out
}

/// The band's consecutive station pairs with their band positions: what a new stroke anchors to.
pub fn profile_frames(doc: &Document, key: &str, use_adjustment: bool) -> Option<Vec<Frame>> {
    let s = solve(doc, use_adjustment);
    let g = graph(&s);
    let z_of = |id: Id| s.positions.get(&id).map(|p| p.z);
    let grouped = group_runs(&s.labels, &s.order);
    let run = grouped.runs.iter().find(|r| r.key == key)?;
    let tie = tie_stations(&s, &grouped).get(key).copied();
    let band = unroll_band(run, tie, &g, &s.labels, &z_of, &s.ex.lruds);
    Some(
        band.stations
            .windows(2)
            .map(|w| Frame {
                a: w[0].id,
                b: w[1].id,
                pa: [w[0].s, w[0].z],
                pb: [w[1].s, w[1].z],
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use cave_doc::ActorId;
    use cave_id::SeqIds;

    const CAVE: &str = r#"{"unit":"m","name":"p","shots":[
        {"from":"A1","to":"A2","distance":10,"azimuth":0,"inclination":0,"up":3,"down":1},
        {"from":"A2","to":"A3","distance":10,"azimuth":90,"inclination":-30,"up":4,"down":2},
        {"from":"A3","to":"B1","distance":5,"azimuth":90,"inclination":0},
        {"from":"B1","to":"B2","distance":6,"azimuth":90,"inclination":10}]}"#;

    fn cave() -> Document {
        cave_fixture::from_shot_list(CAVE, ActorId(1)).unwrap()
    }

    #[test]
    fn runs_are_listed_with_their_lengths() {
        let r = runs(&cave(), false);
        let keys: Vec<&str> = r.iter().map(|x| x.key.as_str()).collect();
        assert_eq!(keys, vec!["A", "B"]);
        assert!(
            (r[0].length - (10.0 + 10.0 * 30f64.to_radians().cos())).abs() < 1e-9,
            "plan length, not slope"
        );
        assert_eq!(r[0].stations, 3);
    }

    #[test]
    fn a_profile_sketch_is_placed_in_the_vertical_plane_of_its_run() {
        let mut doc = cave();
        let frames = profile_frames(&doc, "A", false).unwrap();
        // Band coordinates: the middle of A1-A2 raised 1 m, and the middle of the sloping A2-A3.
        let run = 10.0 + 10.0 * 30f64.to_radians().cos() / 2.0;
        let (_, actions) = cave_draw::profile_sketch(
            &doc,
            &mut SeqIds::new(3),
            "wall",
            "A",
            &[[5.0, 1.0], [run, -2.5]],
            Some(vec![0.5, 1.5]),
            &frames,
        )
        .unwrap();
        doc.edit(actions).unwrap();
        let all = profile_sketches_3d(&doc, false);
        assert_eq!(all.len(), 1);
        assert_eq!(
            (all[0].run.as_str(), all[0].widths.as_deref()),
            ("A", Some(&[0.5, 1.5][..]))
        );
        let near = |p: [f64; 3], x: f64, y: f64, z: f64| {
            (p[0] - x).abs() < 1e-6 && (p[1] - y).abs() < 1e-6 && (p[2] - z).abs() < 1e-6
        };
        // A1-A2 runs north (so along-the-run is north); A2-A3 runs east.
        assert!(near(all[0].points[0], 0.0, 5.0, 1.0), "{:?}", all[0].points);
        assert!(
            near(
                all[0].points[1],
                10.0 * 30f64.to_radians().cos() / 2.0,
                10.0,
                -2.5
            ),
            "{:?}",
            all[0].points
        );
    }

    #[test]
    fn section_sketches_show_on_the_band_beside_their_station() {
        let mut doc = cave();
        let a2 = *doc
            .ids_of(Kind::Station)
            .iter()
            .find(|id| doc.entity(**id).unwrap().text(Field::StationLabel) == "A2")
            .unwrap();
        let (_, actions) = cave_draw::section_sketch(
            &doc,
            &mut SeqIds::new(4),
            "wall",
            a2,
            &[[0.0, 0.0], [1.0, 0.5]],
            None,
            None,
        )
        .unwrap();
        doc.edit(actions).unwrap();
        let p = profile_scene(&doc, "A", false).unwrap();
        assert_eq!(p.sections.len(), 1);
        let sec = &p.sections[0];
        assert_eq!(sec.label, "A2");
        assert!((sec.s - 10.0).abs() < 1e-9);
        assert_eq!(sec.placement, None);
        let (_, actions) =
            cave_draw::place_section(&doc, &mut SeqIds::new(5), a2, "profile", [3.0, 2.0, 0.0])
                .unwrap();
        doc.edit(actions).unwrap();
        let sec = &profile_scene(&doc, "A", false).unwrap().sections[0];
        assert_eq!(sec.placement, Some([3.0, 2.0]));
        assert!(
            profile_scene(&doc, "B", false).unwrap().sections.is_empty(),
            "only stations on the band"
        );
    }

    #[test]
    fn a_letter_run_starts_at_the_station_it_ties_in_at() {
        // B's name says nothing about A3; the shot that placed B1 does.
        let b = profile_scene(&cave(), "B", false).unwrap();
        assert_eq!(b.tie.as_deref(), Some("A3"));
        let labels: Vec<&str> = b.stations.iter().map(|x| x.label.as_str()).collect();
        assert_eq!(labels, ["A3", "B1", "B2"], "the tie-in station comes first");
        assert!(
            b.stations[0].s.abs() < 1e-9,
            "and sits at the start of the band"
        );
        let a = profile_scene(&cave(), "A", false).unwrap();
        assert_eq!(a.tie, None, "the main run has nothing to tie into");
    }

    #[test]
    fn the_band_unrolls_with_elevation_ceiling_and_floor() {
        let p = profile_scene(&cave(), "A", false).unwrap();
        assert_eq!(p.stations.len(), 3);
        assert!(
            (p.stations[2].z - (-5.0)).abs() < 1e-9,
            "10 m at -30 degrees drops 5 m"
        );
        assert_eq!(p.ceiling.len(), 1);
        assert!(
            (p.ceiling[0][0][1] - 3.0).abs() < 1e-9 && (p.floor[0][0][1] - (-1.0)).abs() < 1e-9
        );
        assert!(p.bounds.is_some());
    }

    #[test]
    fn unknown_runs_are_none_and_sketches_come_back_on_their_own_run() {
        let mut doc = cave();
        assert!(profile_scene(&doc, "Z", false).is_none());
        let frames = profile_frames(&doc, "A", false).unwrap();
        assert_eq!(frames.len(), 2, "three stations, two frames");
        let (_, actions) = cave_draw::profile_sketch(
            &doc,
            &mut SeqIds::new(7),
            "wall",
            "A",
            &[[0.0, 0.0], [5.0, 1.0]],
            Some(vec![0.5, 1.5]),
            &frames,
        )
        .unwrap();
        doc.edit(actions).unwrap();
        let a = profile_scene(&doc, "A", false).unwrap();
        assert_eq!(a.sketches.len(), 1);
        assert_eq!(a.sketches[0].widths.as_deref(), Some(&[0.5, 1.5][..]));
        assert_eq!(a.unplaced, 0);
        for (got, want) in a.sketches[0].points.iter().zip([[0.0, 0.0], [5.0, 1.0]]) {
            assert!(
                (got[0] - want[0]).abs() < 1e-9 && (got[1] - want[1]).abs() < 1e-9,
                "placed where it was drawn"
            );
        }
        assert!(
            profile_scene(&doc, "B", false).unwrap().sketches.is_empty(),
            "sketches belong to their run"
        );
    }

    #[test]
    fn a_sketch_stays_attached_to_its_station_when_adjustment_moves_it() {
        // A loop with a vertical bust: adjusting moves the stations' elevations.
        let text = r#"{"unit":"m","name":"loop","shots":[
            {"from":"A1","to":"A2","distance":10,"azimuth":0,"inclination":0},
            {"from":"A2","to":"A3","distance":10,"azimuth":90,"inclination":0},
            {"from":"A3","to":"A4","distance":10,"azimuth":180,"inclination":0},
            {"from":"A4","to":"A1","distance":10,"azimuth":270,"inclination":5}]}"#;
        let mut doc = cave_fixture::from_shot_list(text, ActorId(1)).unwrap();
        let raw = profile_scene(&doc, "A", false).unwrap();
        let last = raw.stations.last().unwrap();
        let frames = profile_frames(&doc, "A", false).unwrap();
        let stroke = [[last.s + 0.5, last.z + 1.0], [last.s + 1.5, last.z + 1.0]];
        let (_, actions) = cave_draw::profile_sketch(
            &doc,
            &mut SeqIds::new(3),
            "wall",
            "A",
            &stroke,
            None,
            &frames,
        )
        .unwrap();
        doc.edit(actions).unwrap();

        let (before, after) = (
            profile_scene(&doc, "A", false).unwrap(),
            profile_scene(&doc, "A", true).unwrap(),
        );
        let (st0, st1) = (
            before.stations.last().unwrap(),
            after.stations.last().unwrap(),
        );
        let moved = (st1.z - st0.z).abs();
        assert!(
            moved > 0.05,
            "adjustment really moved the station ({moved:.3} m)"
        );
        let (p0, p1) = (before.sketches[0].points[0], after.sketches[0].points[0]);
        // The sketch moves with its station (turning slightly with the leg it hangs on) instead of
        // staying where it was on the page.
        let (dz_station, dz_sketch) = (st1.z - st0.z, p1[1] - p0[1]);
        assert!(
            (dz_station - dz_sketch).abs() < 0.1 * moved.max(0.05),
            "sketch moved {dz_sketch:.3}, station moved {dz_station:.3}"
        );
        let dist = |p: [f64; 2], st: &BandDot| (p[0] - st.s).hypot(p[1] - st.z);
        assert!(
            (dist(p0, st0) - dist(p1, st1)).abs() < 0.02,
            "same distance from its station"
        );
    }

    #[test]
    fn a_sketch_whose_stations_are_gone_is_counted_not_drawn() {
        let mut doc = cave();
        let frames = profile_frames(&doc, "A", false).unwrap();
        let (_, actions) = cave_draw::profile_sketch(
            &doc,
            &mut SeqIds::new(5),
            "wall",
            "A",
            &[[1.0, 0.0], [2.0, 0.0]],
            None,
            &frames,
        )
        .unwrap();
        doc.edit(actions).unwrap();
        let first = profile_scene(&doc, "A", false).unwrap();
        assert_eq!(first.sketches.len(), 1);
        // Remove every shot touching the first station, leaving the stroke hanging on a station
        // that is no longer on the band.
        let victim = doc
            .ids_of(Kind::Station)
            .into_iter()
            .find(|id| {
                doc.entity(*id)
                    .is_some_and(|e| e.text(Field::StationLabel) == "A1")
            })
            .unwrap();
        doc.edit(vec![cave_doc::Action::Remove {
            id: victim,
            kind: Kind::Station,
        }])
        .unwrap();
        let after = profile_scene(&doc, "A", false).unwrap();
        assert!(
            after.sketches.is_empty() && after.unplaced == 1,
            "not placed, and counted"
        );
    }
}
