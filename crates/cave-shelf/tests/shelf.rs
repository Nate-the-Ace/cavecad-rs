//! The shelf's summary of a cave: Pitfall Cave (the default test cave), a cave with nothing in it, and
//! Plumbline Pit (the vertical guard).

use cave_doc::ActorId;
use cave_id::SeqIds;
use cave_shelf::summarize;

fn load(name: &str) -> cave_doc::Document {
    let path = format!(
        "{}/../cave-notebook/tests/data/{name}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    cave_fixture::from_shot_list(&std::fs::read_to_string(path).unwrap(), ActorId(1)).unwrap()
}

#[test]
fn pitfall_cave_is_summarised_trip_by_trip() {
    let s = summarize(&load("PitfallCave"));
    assert_eq!(s.trips.len(), 4);
    assert_eq!(s.unit, "ft");
    assert!(!s.empty && s.shots > 50 && s.stations > 50);
    assert_eq!(s.trips.iter().map(|t| t.shots).sum::<usize>(), s.shots);
    let total: f64 = s.trips.iter().map(|t| t.length_m).sum();
    assert!((s.length_m - total).abs() < 1e-9);
    assert_eq!(s.trips[0].name, "ENTRANCE SERIES");
    assert_eq!(s.trips[0].start, "A1", "a trip says where it started");
    assert!(
        !s.trips[0].end.is_empty() && !s.trips[0].date.is_empty() && !s.trips[0].team.is_empty()
    );
    assert!(
        s.loops >= 5 && s.worst_percent.is_some(),
        "Pitfall Cave has its five loops"
    );
    assert!(s.depth_m.is_some_and(|d| d > 0.0));
}

#[test]
fn open_ends_are_listed_newest_trip_first_and_belong_to_trips() {
    let s = summarize(&load("PitfallCave"));
    assert!(!s.ends.is_empty());
    assert!(
        s.ends.windows(2).all(|w| w[0].trip >= w[1].trip),
        "newest trip first"
    );
    for e in &s.ends {
        assert!(
            s.trips[e.trip].ends.contains(&e.station),
            "{} is on its trip's line",
            e.station
        );
    }
}

#[test]
fn a_cave_with_nothing_in_it_says_so_and_has_a_name() {
    let doc = cave_notebook::new_cave(
        ActorId(1),
        &mut SeqIds::new(3),
        "Fresh Hole",
        "First trip",
        "m",
    )
    .unwrap();
    let s = summarize(&doc);
    assert_eq!(s.name, "Fresh Hole");
    assert!(
        s.empty
            && s.stations == 0
            && s.ends.is_empty()
            && s.depth_m.is_none()
            && s.worst_percent.is_none()
    );
    assert_eq!(
        (s.trips.len(), s.trips[0].shots, s.trips[0].start.as_str()),
        (1, 0, "")
    );
}

#[test]
fn plumbline_pit_summarises_without_inventing_anything() {
    let s = summarize(&load("PlumblinePit"));
    assert!(
        s.depth_m.is_some_and(|d| d > 5.0),
        "a shaft has depth: {:?}",
        s.depth_m
    );
    assert_eq!(s.trips.len(), 4);
}
