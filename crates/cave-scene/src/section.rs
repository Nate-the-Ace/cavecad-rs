//! The Cross Section panel: the passage cut across at a station, and the sketches drawn on it.
//!
//! Sections are cut at stations only (that is where they are sketched). The outline is the ring of
//! measured wall points at the station, built exactly as the 3D tube builds it (every LRUD tick and
//! splay tip, projected across the passage), so the panel and the tube always agree. It is a guide
//! to trace against, never the passage itself. A station with too few measurements gets a plain-words
//! note instead of an outline, and can still be sketched on (the field-book page is the evidence
//! then); where "fill gaps" supplies an inferred ring the section says so.
//!
//! Sketches are `section-sketch` features naming their station, with points in the section's own
//! plane (`[across, up]` metres from the station, the plane's orientation being the tube's frame at
//! that station), so they follow the station when the survey is adjusted.

use crate::{ViewFilter, solved::solve};
use cave_doc::{Document, Field, Kind, Value};
use cave_id::Id;
use cave_mesh::{
    ColorMode, LegRings, LoftMode, MeshInput, RingPoint, build_with_rings, cut_outline, frame_at,
};
use cave_survey::Vec3;
use serde::Serialize;
use std::collections::HashMap;

/// A station a section can be cut at.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SectionStation {
    /// The station's id as text.
    pub id: String,
    /// Its label.
    pub label: String,
    /// True when there are enough measured (or inferred) wall points for an outline.
    pub outlined: bool,
    /// Sketches already drawn on this station's section.
    pub sketches: usize,
}

/// A stroke sketched on a section, `[across, up]` metres from the station.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SectionSketch {
    /// The feature's id as text.
    pub id: String,
    /// The stroke.
    pub points: Vec<[f64; 2]>,
    /// Pen weight per point, when drawn with pressure.
    pub widths: Option<Vec<f64>>,
}

/// The section at one station. Points are `[across, up]` in metres from the station.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StationSection {
    /// The station's id as text.
    pub id: String,
    /// Its label.
    pub label: String,
    /// The closed outline in angle order; empty when there are too few measurements.
    pub outline: Vec<[f64; 2]>,
    /// The measured wall points the outline is made from.
    pub measured: Vec<[f64; 2]>,
    /// The ring is an inference from neighbouring stations, not a measurement.
    pub inferred: bool,
    /// What "up the page" means: `up`, or `north` in a shaft where no direction is up.
    pub up_means: String,
    /// Why there is no outline, in words a caver can act on.
    pub note: Option<String>,
    /// Sketches on this section.
    pub sketches: Vec<SectionSketch>,
}

fn dot(a: Vec3, b: Vec3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

/// The tube's rings for every ordinary leg of what the filter leaves visible.
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

/// Where a station's section comes from: the first ordinary leg arriving at it (walls are recorded
/// with the shot into a station), else the first leaving it. `(rings, at the leg's end, t)`.
fn pick(rings: &[LegRings], station: Id) -> Option<(&LegRings, bool)> {
    rings
        .iter()
        .find(|r| r.to == station)
        .map(|r| (r, true))
        .or_else(|| rings.iter().find(|r| r.from == station).map(|r| (r, false)))
}

fn label(doc: &Document, id: Id) -> String {
    doc.entity(id)
        .map(|e| e.text(Field::StationLabel))
        .unwrap_or_default()
}

/// The sketches on `station`'s section that the trip filter leaves visible.
fn sketches_of(doc: &Document, station: Id, filter: &ViewFilter) -> Vec<SectionSketch> {
    doc.ids_of(Kind::Feature)
        .into_iter()
        .filter_map(|id| {
            let e = doc.entity(id)?;
            if e.text(Field::FeatureType) != cave_draw::SECTION_SKETCH
                || e.reference(Field::FeatureStation) != Some(station)
                || !filter.trip_shown(e.reference(Field::FeatureTrip))
            {
                return None;
            }
            let Value::Path(Some(path)) = e.get(Field::FeaturePath) else {
                return None;
            };
            Some(SectionSketch {
                id: id.to_string(),
                points: path.points().iter().map(|p| [p.x, p.y]).collect(),
                widths: path.widths().map(<[f64]>::to_vec),
            })
        })
        .collect()
}

/// Why a station's ring cannot make an outline, in words a caver can act on.
fn thin_reason(name: &str, directions: usize) -> String {
    if directions == 0 {
        format!(
            "Station {name} has no measured wall points of its own, so there is no outline to trace against. \
             Walls are recorded with the shot INTO a station, so the first station of a survey chain has none. \
             You can still sketch here."
        )
    } else {
        format!(
            "Station {name} has only {directions} measured wall direction{}; 3 are needed for an outline. You can still sketch here.",
            if directions == 1 { "" } else { "s" }
        )
    }
}

/// Every station a section can be cut at, in survey order, with whether it has an outline.
pub fn section_stations(
    doc: &Document,
    use_adjustment: bool,
    fill_gaps: bool,
    filter: &ViewFilter,
) -> Vec<SectionStation> {
    let rings = leg_rings(doc, use_adjustment, fill_gaps, filter);
    let mut seen: Vec<Id> = Vec::new();
    for r in &rings {
        for s in [r.from, r.to] {
            if !seen.contains(&s) {
                seen.push(s);
            }
        }
    }
    let mut counts: HashMap<Id, usize> = HashMap::new();
    for id in doc.ids_of(Kind::Feature) {
        if let Some(e) = doc.entity(id)
            && e.text(Field::FeatureType) == cave_draw::SECTION_SKETCH
            && filter.trip_shown(e.reference(Field::FeatureTrip))
            && let Some(st) = e.reference(Field::FeatureStation)
        {
            *counts.entry(st).or_default() += 1;
        }
    }
    seen.into_iter()
        .map(|id| SectionStation {
            id: id.to_string(),
            label: label(doc, id),
            outlined: pick(&rings, id)
                .is_some_and(|(r, end)| cut_outline(r, if end { 1.0 } else { 0.0 }).is_some()),
            sketches: counts.get(&id).copied().unwrap_or(0),
        })
        .collect()
}

/// The trip that surveyed `station` (the trip of the first shot touching it), for filing a new
/// sketch under it so the trip filter hides the sketch with its passage.
pub fn section_trip(doc: &Document, use_adjustment: bool, station: &str) -> Option<Id> {
    let station: Id = station.parse().ok()?;
    let s = solve(doc, use_adjustment);
    s.resolved
        .legs
        .iter()
        .find(|l| l.from == station || l.to == station)
        .and_then(|l| doc.entity(l.shot)?.reference(Field::ShotTrip))
}

/// The section at `station` (an id as text), or `None` when it is not a station on any leg.
pub fn section_at(
    doc: &Document,
    use_adjustment: bool,
    station: &str,
    fill_gaps: bool,
    filter: &ViewFilter,
) -> Option<StationSection> {
    let id: Id = station.parse().ok()?;
    let rings = leg_rings(doc, use_adjustment, fill_gaps, filter);
    let (leg, at_end) = pick(&rings, id)?;
    let (centre, dir, ring, inferred): (Vec3, Vec3, &[RingPoint], bool) = if at_end {
        (leg.b, leg.dir_b, &leg.ring_b, leg.inferred_b)
    } else {
        (leg.a, leg.dir_a, &leg.ring_a, leg.inferred_a)
    };
    let frame = frame_at(dir)?;
    let flat = |p: Vec3| {
        let w = p.minus(centre);
        [dot(w, frame.right), dot(w, frame.up)]
    };
    let name = label(doc, id);
    let outline: Vec<[f64; 2]> = cut_outline(leg, if at_end { 1.0 } else { 0.0 })
        .map(|o| o.iter().map(|(_, p)| flat(*p)).collect())
        .unwrap_or_default();
    let note = outline.is_empty().then(|| {
        // Count directions, not points: two measurements the same way make one wall point.
        let mut angles: Vec<f64> = ring.iter().map(|p| p.angle).collect();
        angles.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
        thin_reason(&name, angles.len())
    });
    Some(StationSection {
        id: station.to_string(),
        label: name,
        measured: ring.iter().map(|p| flat(p.p)).collect(),
        outline,
        inferred,
        up_means: if frame.forward.z.abs() > 0.98 {
            "north"
        } else {
            "up"
        }
        .to_string(),
        note,
        sketches: sketches_of(doc, id, filter),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cave_doc::ActorId;
    use cave_id::SeqIds;

    const BOX: &str = r#"{"unit":"m","name":"b","trips":[{"name":"b","startLrud":{"left":1,"right":1,"up":1,"down":1}}],"shots":[
        {"from":"A1","to":"A2","distance":10,"azimuth":0,"inclination":0,"left":1,"right":1,"up":1,"down":1},
        {"from":"A2","to":"A3","distance":10,"azimuth":0,"inclination":0,"left":3,"right":3,"up":3,"down":3}]}"#;

    fn doc(text: &str) -> Document {
        cave_fixture::from_shot_list(text, ActorId(1)).unwrap()
    }

    fn all() -> ViewFilter {
        ViewFilter::default()
    }

    fn station(d: &Document, name: &str) -> String {
        section_stations(d, false, false, &all())
            .into_iter()
            .find(|s| s.label == name)
            .unwrap_or_else(|| panic!("no station {name}"))
            .id
    }

    fn at(d: &Document, name: &str) -> StationSection {
        section_at(d, false, &station(d, name), false, &all()).unwrap()
    }

    fn has(points: &[[f64; 2]], u: f64, v: f64) -> bool {
        points
            .iter()
            .any(|p| (p[0] - u).abs() < 1e-9 && (p[1] - v).abs() < 1e-9)
    }

    #[test]
    fn stations_are_listed_in_survey_order_with_whether_they_have_an_outline() {
        let list = section_stations(&doc(BOX), false, false, &all());
        let labels: Vec<_> = list.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, ["A1", "A2", "A3"]);
        assert!(list.iter().all(|s| s.outlined && s.sketches == 0));
    }

    #[test]
    fn a_stations_section_is_its_measured_ring() {
        let s = at(&doc(BOX), "A3");
        assert!(s.note.is_none() && !s.inferred);
        assert_eq!(s.outline.len(), 4, "four measured walls, nothing invented");
        for (u, v) in [(3.0, 0.0), (-3.0, 0.0), (0.0, 3.0), (0.0, -3.0)] {
            assert!(has(&s.outline, u, v), "{u},{v}");
        }
        assert_eq!(s.measured.len(), 4);
        assert_eq!(s.up_means, "up");
    }

    #[test]
    fn a_station_with_no_measurements_says_why_and_can_still_be_sketched_on() {
        let text = r#"{"unit":"m","name":"n","shots":[
            {"from":"A1","to":"A2","distance":10,"azimuth":0,"inclination":0}]}"#;
        let d = doc(text);
        assert!(
            section_stations(&d, false, false, &all())
                .iter()
                .all(|s| !s.outlined)
        );
        let s = at(&d, "A1");
        assert!(s.outline.is_empty());
        let note = s.note.unwrap();
        assert!(
            note.contains("A1")
                && note.contains("no measured wall points")
                && note.contains("sketch")
        );
    }

    #[test]
    fn a_shaft_has_north_up_the_page_and_finite_geometry() {
        let text = r#"{"unit":"m","name":"s","trips":[{"name":"s","startLrud":{"left":1,"right":1,"up":1,"down":1}}],"shots":[
            {"from":"A1","to":"A2","distance":10,"azimuth":0,"inclination":-90,"left":2,"right":2,"up":2,"down":2}]}"#;
        let s = at(&doc(text), "A2");
        assert_eq!(s.up_means, "north");
        assert!(s.outline.len() >= 3 && s.outline.iter().flatten().all(|v| v.is_finite()));
    }

    #[test]
    fn sketches_come_back_on_their_own_station_and_follow_the_trip_filter() {
        let mut d = doc(BOX);
        let a2: Id = station(&d, "A2").parse().unwrap();
        let trip = section_trip(&d, false, &a2.to_string());
        assert!(trip.is_some());
        let (_, actions) = cave_draw::section_sketch(
            &d,
            &mut SeqIds::new(9),
            a2,
            &[[0.0, 0.0], [1.0, 0.5], [2.0, 0.0]],
            Some(vec![0.5, 1.0, 1.5]),
            trip,
        )
        .unwrap();
        d.edit(actions).unwrap();
        let s = at(&d, "A2");
        assert_eq!(s.sketches.len(), 1);
        assert_eq!(
            s.sketches[0].points,
            vec![[0.0, 0.0], [1.0, 0.5], [2.0, 0.0]]
        );
        assert_eq!(s.sketches[0].widths.as_deref(), Some(&[0.5, 1.0, 1.5][..]));
        assert!(
            at(&d, "A3").sketches.is_empty(),
            "sketches belong to their station"
        );
        let counted = section_stations(&d, false, false, &all());
        assert_eq!(counted.iter().map(|s| s.sketches).sum::<usize>(), 1);
        let hidden = ViewFilter {
            hidden_trips: [trip.unwrap().to_string()].into(),
            ..ViewFilter::default()
        };
        assert!(
            section_at(&d, false, &a2.to_string(), false, &hidden).is_none(),
            "with the trip hidden the station is not on any visible leg"
        );
    }

    #[test]
    fn unknown_stations_are_none() {
        let d = doc(BOX);
        assert!(section_at(&d, false, "nope", false, &all()).is_none());
        assert!(section_at(&d, false, &Id::from_name("x").to_string(), false, &all()).is_none());
    }

    #[test]
    fn every_station_of_the_test_caves_gives_finite_geometry_or_says_why_not() {
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
            let (mut outlined, mut noted) = (0, 0);
            for st in section_stations(&d, true, true, &all()) {
                let s = section_at(&d, true, &st.id, true, &all()).unwrap();
                assert_eq!(s.outline.is_empty(), !st.outlined, "{name} {}", st.label);
                if s.outline.is_empty() {
                    assert!(s.note.is_some(), "{name} {}", st.label);
                    noted += 1;
                } else {
                    assert!(s.outline.len() >= 3);
                    assert!(
                        s.outline
                            .iter()
                            .chain(&s.measured)
                            .flatten()
                            .all(|v| v.is_finite())
                    );
                    outlined += 1;
                }
            }
            assert!(
                outlined > 0,
                "{name}: something has an outline ({noted} noted)"
            );
        }
    }
}
