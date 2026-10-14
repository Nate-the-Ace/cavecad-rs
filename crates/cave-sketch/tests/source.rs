//! The sketchpad draws what the notebook holds: Pitfall Cave (the default test cave) and Plumbline Pit (the
//! vertical guard: a pitch has no bearing, so nothing may invent one).

use cave_doc::{ActorId, Document};
use cave_notebook::{RowKind, page, trips};
use cave_sketch::*;

fn load(name: &str) -> Document {
    let path = format!(
        "{}/../cave-notebook/tests/data/{name}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    cave_fixture::from_shot_list(&std::fs::read_to_string(path).unwrap(), ActorId(1)).unwrap()
}

#[test]
fn every_shot_of_a_page_becomes_a_survey_shot() {
    let doc = load("PitfallCave");
    for t in trips(&doc) {
        let p = page(&doc, t.id).unwrap();
        let src = source(&p);
        let on_page = p
            .rows
            .iter()
            .filter(|r| matches!(r.kind, RowKind::Shot | RowKind::Splay))
            .count();
        assert_eq!(src.shots.len(), on_page, "{}", t.name);
        assert!(
            src.shots.iter().all(|s| !s.from.is_empty()),
            "every shot starts at a named station"
        );
        assert!(
            src.shots
                .iter()
                .filter(|s| s.to.is_some())
                .all(|s| s.to.as_deref().is_some_and(|t| !t.is_empty()))
        );
        let l = layout(&src.shots);
        assert!(!l.stations.is_empty());
        assert_eq!(l.chain.first(), l.stations.first().map(|s| &s.label));
        // The views never produce a number that is not one.
        let plan = serde_json::to_string(&plan_view(&l, &src.dims)).unwrap();
        let profile = serde_json::to_string(&profile_view(&l, &src.dims)).unwrap();
        assert!(
            !plan.contains("null") && !profile.contains("null"),
            "{}",
            t.name
        );
    }
}

#[test]
fn the_trips_declination_turns_the_page_into_true_bearings() {
    let doc = load("PitfallCave");
    let t = trips(&doc)[0].id;
    let mut p = page(&doc, t).unwrap();
    let was = p.declination.unwrap_or(0.0);
    let plain = source(&p);
    p.declination = Some(was + 10.0);
    let turned = source(&p);
    let mut compared = 0;
    for (a, b) in plain.shots.iter().zip(&turned.shots) {
        if let (Some(x), Some(y)) = (a.az, b.az) {
            assert!(((x + 10.0).rem_euclid(360.0) - y).abs() < 1e-9);
            compared += 1;
        }
    }
    assert!(compared > 3);
}

#[test]
fn a_vertical_cave_goes_straight_down_and_no_bearing_is_invented() {
    let doc = load("PlumblinePit");
    for (i, t) in trips(&doc).iter().enumerate() {
        let p = page(&doc, t.id).unwrap();
        let src = source(&p);
        let l = layout(&src.shots);
        if i == 0 {
            // The entrance drop goes down the pit: the profile drops with it.
            let z: Vec<f64> = l.chain.iter().map(|c| l.station(c).unwrap().z).collect();
            assert!(
                z.last().unwrap() < &-50.0 && z.windows(2).all(|w| w[1] <= w[0] + 1e-9),
                "the chain drops: {z:?}"
            );
        }
        assert!(
            l.stations
                .iter()
                .all(|s| s.x.is_finite() && s.y.is_finite() && s.z.is_finite())
        );
        let profile = profile_view(&l, &src.dims);
        assert_eq!(profile.stations.len(), l.chain.len());
        for s in &l.stations {
            assert!(section_view(&l, &src.dims, &s.label).is_some());
        }
    }
    // A shot with no compass that is not a pitch is left unplaced rather than given a bearing.
    let flat = SurveyShot {
        id: "x".into(),
        from: "A".into(),
        to: Some("B".into()),
        distance: Some(5.0),
        az: None,
        inc: Some(10.0),
        az_back: None,
        inc_back: None,
    };
    assert!(shot_vector(&flat).is_none());
    let pitch = SurveyShot {
        inc: Some(-89.0),
        ..flat
    };
    let v = shot_vector(&pitch).unwrap();
    assert!(v[0].abs() < 0.1 && v[1].abs() < 0.1 && v[2] < -4.9, "{v:?}");
}
