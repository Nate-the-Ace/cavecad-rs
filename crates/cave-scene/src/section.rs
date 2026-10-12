//! The Cross Section panel: a cut across the passage anywhere along a surveyed leg.
//!
//! The outline is made from the same rings as the 3D tube (every measured LRUD tick and splay tip
//! at the two stations, projected across the passage), so a cut at a station IS the tube's ring
//! there. Between stations it is a straight blend of the two end rings and says so: the caption
//! states how far the cut is from the nearer station. Nothing is smoothed and nothing is invented;
//! a station with too few measurements gives a plain-words refusal, not a guess (unless "fill
//! gaps" is on, in which case the inferred ring is labelled as an inference).

use crate::{ViewFilter, solved::solve};
use cave_doc::{Document, Field};
use cave_id::Id;
use cave_mesh::{
    ColorMode, LegRings, LoftMode, MeshInput, RingPoint, build_with_rings, cut_outline, frame_at,
};
use cave_survey::{LegKind, Vec3};
use serde::Serialize;

/// A leg a cross section can be cut along.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SectionLeg {
    /// The shot (its id as text).
    pub shot: String,
    /// Label of the start station.
    pub from: String,
    /// Label of the end station.
    pub to: String,
    /// Length of the leg in metres.
    pub length: f64,
}

/// One cut. Points are `[across, up]` in metres from the axis, in the plane across the passage.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Section {
    /// The shot.
    pub shot: String,
    /// Start station label.
    pub from: String,
    /// End station label.
    pub to: String,
    /// Fraction of the way from start to end.
    pub t: f64,
    /// Length of the leg.
    pub length: f64,
    /// Metres from the start station.
    pub along: f64,
    /// The nearer end station.
    pub nearest: String,
    /// Metres from the nearer end station.
    pub nearest_distance: f64,
    /// True when the cut is at a station (its own measured ring, no blending).
    pub at_station: bool,
    /// The closed outline, in angle order. Empty when the cut is refused (see `note`).
    pub outline: Vec<[f64; 2]>,
    /// Measured points at the start station, drawn as evidence.
    pub from_points: Vec<[f64; 2]>,
    /// Measured points at the end station.
    pub to_points: Vec<[f64; 2]>,
    /// The start ring is an inference from neighbours.
    pub from_inferred: bool,
    /// The end ring is an inference from neighbours.
    pub to_inferred: bool,
    /// What "up the page" means: `up`, or `north` in a shaft where no direction is up.
    pub up_means: String,
    /// Why there is no outline, in words a caver can act on.
    pub note: Option<String>,
}

fn dot(a: Vec3, b: Vec3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

/// Runs the tube's own ring building and hands back the rings of every ordinary leg.
fn leg_rings(
    doc: &Document,
    use_adjustment: bool,
    fill_gaps: bool,
    filter: &ViewFilter,
) -> Vec<LegRings> {
    let s = solve(doc, use_adjustment);
    let trip_shown =
        |shot| filter.trip_shown(doc.entity(shot).and_then(|e| e.reference(Field::ShotTrip)));
    let legs: Vec<_> = s
        .resolved
        .legs
        .iter()
        .copied()
        .filter(|l| trip_shown(l.shot))
        .collect();
    let splays: Vec<_> =
        s.ex.splays
            .iter()
            .filter(|sp| trip_shown(sp.shot))
            .cloned()
            .collect();
    let mut out = Vec::new();
    build_with_rings(
        &MeshInput {
            stations: &s.positions,
            legs: &legs,
            splays: &splays,
            first_arrival_lruds: &s.ex.first_arrival_lruds,
            start_lruds: &s.ex.start_lruds,
            station_trip: &s.ex.station_trip,
            color: ColorMode::Trip,
            loft: LoftMode::Full,
            fill_gaps,
        },
        &mut |r| out.push(r.clone()),
    );
    out
}

fn label(doc: &Document, id: Id) -> String {
    doc.entity(id)
        .map(|e| e.text(Field::StationLabel))
        .unwrap_or_default()
}

/// Every leg a section can be cut along, in survey order.
pub fn section_legs(doc: &Document, use_adjustment: bool, filter: &ViewFilter) -> Vec<SectionLeg> {
    let s = solve(doc, use_adjustment);
    s.resolved
        .legs
        .iter()
        .filter(|l| l.kind == LegKind::New)
        .filter(|l| {
            filter.trip_shown(
                doc.entity(l.shot)
                    .and_then(|e| e.reference(Field::ShotTrip)),
            )
        })
        .filter_map(|l| {
            let (a, b) = (s.positions.get(&l.from)?, s.positions.get(&l.to)?);
            let length = b.minus(*a).norm();
            (length > 1e-9).then(|| SectionLeg {
                shot: l.shot.to_string(),
                from: label(doc, l.from),
                to: label(doc, l.to),
                length,
            })
        })
        .collect()
}

/// Why a station's ring cannot make an outline, in words a caver can act on.
fn thin_reason(name: &str, measured: usize) -> String {
    if measured == 0 {
        format!(
            "Station {name} has no measured wall points of its own, so there is no outline to cut. \
             Walls are recorded with the shot INTO a station, so the first station of a survey chain has none."
        )
    } else {
        format!(
            "Station {name} has only {measured} measured wall point{}; 3 are needed to make an outline.",
            if measured == 1 { "" } else { "s" }
        )
    }
}

/// The cut along `shot`, `t` (0 to 1) of the way from its start station to its end. `None` when
/// the shot is not a leg that can be cut. A cut that cannot be made still returns a [`Section`]
/// with an empty outline and a `note`.
pub fn section_cut(
    doc: &Document,
    use_adjustment: bool,
    shot: &str,
    t: f64,
    fill_gaps: bool,
    filter: &ViewFilter,
) -> Option<Section> {
    let shot_id: Id = shot.parse().ok()?;
    let t = if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let rings = leg_rings(doc, use_adjustment, fill_gaps, filter)
        .into_iter()
        .find(|r| r.shot == shot_id)?;
    let along_vec = rings.b.minus(rings.a);
    let length = along_vec.norm();
    let frame = frame_at(along_vec)?;
    let axis = Vec3::new(
        rings.a.x + along_vec.x * t,
        rings.a.y + along_vec.y * t,
        rings.a.z + along_vec.z * t,
    );
    let flat = |origin: Vec3, p: Vec3| {
        let w = p.minus(origin);
        [dot(w, frame.right), dot(w, frame.up)]
    };
    let ring_pts = |r: &[RingPoint], origin: Vec3| -> Vec<[f64; 2]> {
        r.iter().map(|p| flat(origin, p.p)).collect()
    };
    let (from, to) = (label(doc, rings.from), label(doc, rings.to));
    let (near_from, nearest_distance) = if t <= 0.5 {
        (true, length * t)
    } else {
        (false, length * (1.0 - t))
    };
    let mut section = Section {
        shot: shot.to_string(),
        from: from.clone(),
        to: to.clone(),
        t,
        length,
        along: length * t,
        nearest: if near_from { from.clone() } else { to.clone() },
        nearest_distance,
        at_station: t <= 1e-6 || t >= 1.0 - 1e-6,
        outline: Vec::new(),
        from_points: ring_pts(&rings.ring_a, rings.a),
        to_points: ring_pts(&rings.ring_b, rings.b),
        from_inferred: rings.inferred_a,
        to_inferred: rings.inferred_b,
        up_means: if frame.forward.z.abs() > 0.98 {
            "north"
        } else {
            "up"
        }
        .to_string(),
        note: None,
    };
    match cut_outline(&rings, t) {
        Some(o) => section.outline = o.iter().map(|(_, p)| flat(axis, *p)).collect(),
        None => {
            let thin = if t >= 1.0 - 1e-6 || (t > 1e-6 && rings.ring_a.len() >= 3) {
                (&to, rings.ring_b.len())
            } else {
                (&from, rings.ring_a.len())
            };
            // Count directions, not points: two measurements the same way make one wall point.
            let ring = if std::ptr::eq(thin.0, &from) {
                &rings.ring_a
            } else {
                &rings.ring_b
            };
            let mut angles: Vec<f64> = ring.iter().map(|p| p.angle).collect();
            angles.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
            section.note = Some(thin_reason(thin.0, angles.len().min(thin.1)));
        }
    }
    Some(section)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cave_doc::ActorId;

    const BOX: &str = r#"{"unit":"m","name":"b","trips":[{"name":"b","startLrud":{"left":1,"right":1,"up":1,"down":1}}],"shots":[
        {"from":"A1","to":"A2","distance":10,"azimuth":0,"inclination":0,"left":1,"right":1,"up":1,"down":1},
        {"from":"A2","to":"A3","distance":10,"azimuth":0,"inclination":0,"left":3,"right":3,"up":3,"down":3}]}"#;

    fn doc(text: &str) -> Document {
        cave_fixture::from_shot_list(text, ActorId(1)).unwrap()
    }

    fn legs(d: &Document) -> Vec<SectionLeg> {
        section_legs(d, false, &ViewFilter::default())
    }

    fn cut(d: &Document, leg: usize, t: f64) -> Section {
        let l = &legs(d)[leg];
        section_cut(d, false, &l.shot, t, false, &ViewFilter::default()).unwrap()
    }

    fn has(points: &[[f64; 2]], u: f64, v: f64) -> bool {
        points
            .iter()
            .any(|p| (p[0] - u).abs() < 1e-9 && (p[1] - v).abs() < 1e-9)
    }

    #[test]
    fn the_legs_are_listed_with_labels_and_lengths() {
        let l = legs(&doc(BOX));
        assert_eq!(l.len(), 2);
        assert_eq!((l[0].from.as_str(), l[0].to.as_str()), ("A1", "A2"));
        assert!((l[0].length - 10.0).abs() < 1e-9);
    }

    #[test]
    fn a_cut_at_a_station_is_that_stations_measured_ring() {
        let d = doc(BOX);
        let s = cut(&d, 0, 1.0);
        assert!(s.at_station && s.note.is_none());
        assert_eq!((s.nearest.as_str(), s.nearest_distance), ("A2", 0.0));
        assert_eq!(
            s.outline.len(),
            4,
            "four measured walls, no invented vertices"
        );
        for (u, v) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            assert!(has(&s.outline, u, v), "{u},{v}");
        }
    }

    #[test]
    fn halfway_between_stations_is_the_blend_and_says_how_far_from_the_nearer() {
        let d = doc(BOX);
        // Leg 1 runs A2 (1 m walls: it arrived by the first shot) to A3 (3 m walls).
        let s = cut(&d, 1, 0.5);
        assert!(!s.at_station);
        assert!((s.nearest_distance - 5.0).abs() < 1e-9);
        for (u, v) in [(2.0, 0.0), (-2.0, 0.0), (0.0, 2.0), (0.0, -2.0)] {
            assert!(has(&s.outline, u, v), "{u},{v}: {:?}", s.outline);
        }
        assert!(s.from_points.len() == 4 && s.to_points.len() == 4);
        assert_eq!(s.up_means, "up");
    }

    #[test]
    fn a_station_with_no_measurements_is_refused_in_plain_words() {
        let text = r#"{"unit":"m","name":"n","shots":[
            {"from":"A1","to":"A2","distance":10,"azimuth":0,"inclination":0}]}"#;
        let d = doc(text);
        let s = cut(&d, 0, 0.0);
        assert!(s.outline.is_empty());
        let note = s.note.unwrap();
        assert!(
            note.contains("A1") && note.contains("no measured wall points"),
            "{note}"
        );
    }

    #[test]
    fn a_shaft_cuts_with_north_up_the_page() {
        let text = r#"{"unit":"m","name":"s","trips":[{"name":"s","startLrud":{"left":1,"right":1,"up":1,"down":1}}],"shots":[
            {"from":"A1","to":"A2","distance":10,"azimuth":0,"inclination":-90,"left":2,"right":2,"up":2,"down":2}]}"#;
        let d = doc(text);
        let s = cut(&d, 0, 0.5);
        assert_eq!(s.up_means, "north");
        assert!(s.outline.iter().flatten().all(|v| v.is_finite()) && s.outline.len() >= 3);
    }

    #[test]
    fn every_leg_of_the_test_caves_cuts_to_finite_geometry_or_says_why_not() {
        for name in [
            "PitfallCave.json",
            "PlumblinePit.json",
            "StairstepCave.json",
        ] {
            let path = format!(
                "{}/../cave-survey/tests/golden/{name}",
                env!("CARGO_MANIFEST_DIR")
            );
            let d = doc(&std::fs::read_to_string(path).unwrap());
            let all = ViewFilter::default();
            let (mut cut_ok, mut refused) = (0, 0);
            for l in section_legs(&d, true, &all) {
                for t in [0.0, 0.3, 0.5, 1.0] {
                    let s = section_cut(&d, true, &l.shot, t, true, &all).unwrap();
                    if s.outline.is_empty() {
                        assert!(s.note.is_some(), "{name} {}-{} at {t}", l.from, l.to);
                        refused += 1;
                    } else {
                        assert!(
                            s.outline.len() >= 3,
                            "{name} {}-{} at {t}: {:?}",
                            l.from,
                            l.to,
                            s.outline
                        );
                        assert!(s.outline.iter().flatten().all(|v| v.is_finite()), "{name}");
                        cut_ok += 1;
                    }
                }
            }
            assert!(
                cut_ok > 0,
                "{name}: something can be cut ({refused} refused)"
            );
        }
    }

    #[test]
    fn unknown_shots_and_odd_fractions_are_safe() {
        let d = doc(BOX);
        let all = ViewFilter::default();
        assert!(section_cut(&d, false, "not-an-id", 0.5, false, &all).is_none());
        let l = &legs(&d)[0];
        let s = section_cut(&d, false, &l.shot, f64::NAN, false, &all).unwrap();
        assert_eq!(s.t, 0.0);
        assert_eq!(
            section_cut(&d, false, &l.shot, 7.0, false, &all).unwrap().t,
            1.0
        );
    }
}
