//! The 3D passage surface for the canvas.

use crate::ViewFilter;
use crate::solved::solve;
use cave_doc::{Document, Field};
pub use cave_mesh::{ColorMode, Mesh};
use cave_mesh::{LoftMode, MeshInput, build};

/// Builds the passage mesh from the open cave (adjusted positions when asked and available).
/// `fill_gaps` draws a paler inferred section through stations with no measurements. Shots of
/// trips the `filter` hides leave no surface.
pub fn passage_mesh(
    doc: &Document,
    use_adjustment: bool,
    color: ColorMode,
    fill_gaps: bool,
    filter: &ViewFilter,
) -> Mesh {
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
    build(&MeshInput {
        stations: &s.positions,
        legs: &legs,
        splays: &splays,
        first_arrival_lruds: &s.ex.first_arrival_lruds,
        start_lruds: &s.ex.start_lruds,
        station_trip: &s.ex.station_trip,
        color,
        loft: LoftMode::Full,
        fill_gaps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cave_doc::ActorId;

    #[test]
    fn a_measured_passage_becomes_triangles() {
        let text = r#"{"unit":"m","name":"m","trips":[{"name":"m","startLrud":{"left":1,"right":1,"up":1,"down":1}}],"shots":[
            {"from":"A1","to":"A2","distance":10,"azimuth":0,"inclination":0,"left":1,"right":1,"up":1,"down":1}]}"#;
        let doc = cave_fixture::from_shot_list(text, ActorId(1)).unwrap();
        let all = ViewFilter::default();
        let m = passage_mesh(&doc, true, ColorMode::Trip, false, &all);
        assert_eq!(m.triangles(), 8);
        let trip = doc.ids_of(cave_doc::Kind::Trip)[0].to_string();
        let hidden = ViewFilter {
            hidden_trips: [trip].into(),
            ..ViewFilter::default()
        };
        assert_eq!(
            passage_mesh(&doc, true, ColorMode::Trip, false, &hidden).triangles(),
            0,
            "a hidden trip leaves no surface"
        );
        assert!(
            passage_mesh(
                &Document::new(ActorId(1)),
                true,
                ColorMode::Depth,
                true,
                &all
            )
            .triangles()
                == 0
        );
    }
}
