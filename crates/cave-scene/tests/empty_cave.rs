//! A cave started from nothing: one empty trip and no shots. Every view must answer sensibly
//! (empty, never a crash), and the cave must build up shot by shot from there.

use cave_doc::{ActorId, Document, Kind};
use cave_id::SeqIds;
use cave_notebook::{RowKind, add_shot, new_cave, page, trips};
use cave_scene::{ColorMode, ViewFilter};

fn fresh() -> Document {
    new_cave(
        ActorId(1),
        &mut SeqIds::new(9),
        "New Cave",
        "First trip",
        "ft",
    )
    .unwrap()
}

#[test]
fn a_new_cave_has_one_empty_trip_in_the_chosen_unit() {
    let doc = fresh();
    let t = trips(&doc);
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].name, "First trip");
    let p = page(&doc, t[0].id).unwrap();
    assert!(p.rows.is_empty() && p.closures.is_empty());
    assert_eq!(p.unit, "ft");
    assert_eq!(
        doc.ids_of(Kind::Shot).len() + doc.ids_of(Kind::Station).len(),
        0
    );
}

#[test]
fn every_view_of_an_empty_cave_answers_with_nothing() {
    let doc = fresh();
    for adjusted in [false, true] {
        let scene = cave_scene::build(&doc, adjusted);
        assert!(scene.stations.is_empty() && scene.legs.is_empty() && scene.closures.is_empty());
        assert!(scene.bounds.is_none());
        let _ = cave_scene::build_filtered(&doc, adjusted, &ViewFilter::default());
        assert!(cave_scene::runs(&doc, adjusted).is_empty());
        assert!(cave_scene::profile_scene(&doc, "A", adjusted).is_none());
        assert!(
            cave_scene::section_stations(&doc, adjusted, true, &ViewFilter::default()).is_empty()
        );
        assert!(
            cave_scene::section_at(&doc, adjusted, "none", true, &ViewFilter::default()).is_none()
        );
        assert!(cave_scene::station_rows(&doc, adjusted, "").is_empty());
        assert!(cave_scene::map_frames(&doc, adjusted).is_empty());
        assert!(cave_scene::map_lines_3d(&doc, adjusted).is_empty());
        assert!(cave_scene::profile_sketches_3d(&doc, adjusted).is_empty());
        assert!(cave_scene::map_sections(&doc, adjusted, &ViewFilter::default()).is_empty());
        let mesh = cave_scene::passage_mesh(
            &doc,
            adjusted,
            ColorMode::Trip,
            true,
            &ViewFilter::default(),
        );
        assert_eq!(mesh.positions.len(), 0);
    }
}

#[test]
fn a_cave_builds_up_from_nothing_one_shot_at_a_time() {
    let mut doc = fresh();
    let mut ids = SeqIds::new(77);
    let trip = trips(&doc)[0].id;
    for (from, to) in [("A1", "A2"), ("A2", "A3"), ("A3", "A4")] {
        let (_, a) = add_shot(&doc, &mut ids, trip, from, to, false).unwrap();
        doc.edit(a).unwrap();
    }
    let p = page(&doc, trip).unwrap();
    let labels: Vec<_> = p
        .rows
        .iter()
        .filter(|r| r.kind == RowKind::Station)
        .map(|r| r.label.as_str())
        .collect();
    assert_eq!(labels, ["A1", "A2", "A3", "A4"]);
    // Shots with no readings yet are on the page, flagged, and cause no trouble elsewhere.
    assert_eq!(p.totals.shots, 3);
    assert!(
        p.rows
            .iter()
            .filter(|r| r.kind == RowKind::Shot)
            .all(|r| !r.warn.is_empty())
    );
    let scene = cave_scene::build(&doc, true);
    assert!(scene.legs.is_empty(), "unmeasured shots are not drawn");
    assert_eq!(
        cave_scene::station_rows(&doc, true, "").len(),
        4,
        "the table lists the stations as they are named"
    );
}
