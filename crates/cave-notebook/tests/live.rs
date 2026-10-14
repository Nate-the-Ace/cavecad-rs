//! Shots taken as they are measured: one edit per shot, the next station's name, and a station's
//! passage dimensions found by name. On Pitfall Cave (the default test cave) and Plumbline Pit (vertical).

use cave_doc::{ActorId, Document, History};
use cave_id::SeqIds;
use cave_notebook::{
    Measured, Reading, RowKind, Side, add_measured_shot, new_cave, page, set_readings,
    set_station_wall, station_walls, stations_in_use, suggest_next, trips,
};
use std::collections::HashSet;

fn set(names: &[&str]) -> HashSet<String> {
    names.iter().map(|s| s.to_string()).collect()
}

#[test]
fn the_next_name_follows_the_number() {
    let none = HashSet::new();
    assert_eq!(suggest_next("A93", &none), "A94");
    assert_eq!(suggest_next("B9", &none), "B10");
    assert_eq!(suggest_next("12", &none), "13");
    assert_eq!(
        suggest_next("A93", &set(&["A94", "A95"])),
        "A96",
        "a taken name is skipped"
    );
    assert_eq!(suggest_next("D07", &none), "D08");
    assert_eq!(suggest_next("C09", &none), "C10", "zero padding is kept");
    assert_eq!(suggest_next("A1B2", &none), "A1B3");
    assert_eq!(suggest_next("ENT", &none), "");
    assert_eq!(suggest_next("", &none), "");
    assert_eq!(suggest_next("  A3  ", &none), "A4");
}

fn fresh() -> (Document, cave_id::Id, SeqIds) {
    let mut ids = SeqIds::new(7);
    let doc = new_cave(ActorId(1), &mut ids, "Test", "Trip 1", "m").unwrap();
    let trip = trips(&doc)[0].id;
    (doc, trip, ids)
}

fn apply(doc: &mut Document, h: &mut History, actions: Vec<cave_doc::Action>) {
    let edit = doc.edit(actions).unwrap();
    h.record(&edit);
}

#[test]
fn a_measured_shot_is_one_undoable_edit() {
    let (mut doc, trip, mut ids) = fresh();
    let mut h = History::new();
    let m = Measured {
        distance: Some(8.84),
        azimuth: Some(123.4),
        inclination: Some(-12.5),
        back_azimuth: Some(303.5),
        back_inclination: Some(12.4),
    };
    let (_, actions) = add_measured_shot(&doc, &mut ids, trip, "A1", "A2", false, m).unwrap();
    apply(&mut doc, &mut h, actions);
    let p = page(&doc, trip).unwrap();
    let shot = p.rows.iter().find(|r| r.kind == RowKind::Shot).unwrap();
    assert_eq!(shot.distance, Some(8.84));
    assert_eq!(shot.az_fs, Some(123.4));
    assert_eq!(shot.inc_fs, Some(-12.5));
    assert_eq!(shot.az_bs, Some(303.5));
    assert_eq!(shot.inc_bs, Some(12.4));
    // One undo takes the whole shot back, not a cell at a time.
    h.undo(&mut doc).unwrap();
    assert_eq!(page(&doc, trip).unwrap().totals.shots, 0);
}

#[test]
fn a_compass_reading_goes_through_the_trips_declination() {
    let (mut doc, trip, mut ids) = fresh();
    let mut h = History::new();
    let a = cave_notebook::set_trip_declination(&doc, trip, 10.0).unwrap();
    apply(&mut doc, &mut h, a);
    let m = Measured {
        distance: Some(5.0),
        azimuth: Some(355.0),
        inclination: Some(0.0),
        ..Measured::default()
    };
    let (shot, actions) = add_measured_shot(&doc, &mut ids, trip, "A1", "A2", false, m).unwrap();
    apply(&mut doc, &mut h, actions);
    // The page shows what the compass read; the survey keeps the true bearing (355 + 10 = 5).
    let p = page(&doc, trip).unwrap();
    assert_eq!(
        p.rows
            .iter()
            .find(|r| r.shot == Some(shot) && r.kind == RowKind::Shot)
            .unwrap()
            .az_fs,
        Some(355.0)
    );
    let stored = doc
        .entity(shot)
        .unwrap()
        .number(cave_doc::Field::ShotAzimuth)
        .unwrap();
    assert!((stored - 5.0).abs() < 1e-9, "{stored}");
}

#[test]
fn bad_readings_are_refused_and_nothing_is_added() {
    let (doc, trip, mut ids) = fresh();
    for m in [
        Measured {
            distance: Some(-1.0),
            ..Measured::default()
        },
        Measured {
            inclination: Some(91.0),
            ..Measured::default()
        },
        Measured {
            azimuth: Some(f64::NAN),
            ..Measured::default()
        },
    ] {
        assert!(add_measured_shot(&doc, &mut ids, trip, "A1", "A2", false, m).is_err());
    }
    assert!(
        add_measured_shot(&doc, &mut ids, trip, "A1", "A1", false, Measured::default()).is_err()
    );
}

#[test]
fn a_backsight_fills_compass_and_clino_in_one_edit() {
    let (mut doc, trip, mut ids) = fresh();
    let mut h = History::new();
    let m = Measured {
        distance: Some(5.0),
        azimuth: Some(100.0),
        inclination: Some(3.0),
        ..Measured::default()
    };
    let (shot, actions) = add_measured_shot(&doc, &mut ids, trip, "A1", "A2", false, m).unwrap();
    apply(&mut doc, &mut h, actions);
    let back = set_readings(
        &doc,
        shot,
        &[
            (Reading::AzimuthBs, Some(280.5)),
            (Reading::InclinationBs, Some(-3.1)),
        ],
    )
    .unwrap();
    apply(&mut doc, &mut h, back);
    let p = page(&doc, trip).unwrap();
    let row = p.rows.iter().find(|r| r.kind == RowKind::Shot).unwrap();
    assert_eq!((row.az_bs, row.inc_bs), (Some(280.5), Some(-3.1)));
    h.undo(&mut doc).unwrap();
    let p = page(&doc, trip).unwrap();
    let row = p.rows.iter().find(|r| r.kind == RowKind::Shot).unwrap();
    assert_eq!(
        (row.az_bs, row.inc_bs),
        (None, None),
        "one undo clears both"
    );
    assert_eq!(row.distance, Some(5.0), "and leaves the foresight alone");
}

#[test]
fn walls_wait_for_a_line_then_land_on_the_right_row() {
    let (mut doc, trip, mut ids) = fresh();
    let mut h = History::new();
    // Before any shot: no station, so nowhere to write.
    assert!(
        set_station_wall(&doc, trip, "A1", Side::Left, Some(1.0))
            .unwrap()
            .is_none()
    );
    // A splay at A1 does not give it a line either.
    let (_, a) = add_measured_shot(
        &doc,
        &mut ids,
        trip,
        "A1",
        "",
        true,
        Measured {
            distance: Some(1.0),
            azimuth: Some(10.0),
            inclination: Some(0.0),
            ..Measured::default()
        },
    )
    .unwrap();
    apply(&mut doc, &mut h, a);
    assert!(station_walls(&doc, trip, "A1").is_none());
    // The first leg gives A1 its opening row and A2 an arrival row.
    let (_, a) = add_measured_shot(
        &doc,
        &mut ids,
        trip,
        "A1",
        "A2",
        false,
        Measured {
            distance: Some(5.0),
            azimuth: Some(100.0),
            inclination: Some(0.0),
            ..Measured::default()
        },
    )
    .unwrap();
    apply(&mut doc, &mut h, a);
    for (station, side, v) in [
        ("A1", Side::Left, 0.8),
        ("A1", Side::Up, 2.5),
        ("A2", Side::Right, 1.2),
        ("A2", Side::Down, 0.4),
    ] {
        let a = set_station_wall(&doc, trip, station, side, Some(v))
            .unwrap()
            .expect("has a line now");
        apply(&mut doc, &mut h, a);
    }
    let a1 = station_walls(&doc, trip, "A1").unwrap();
    let a2 = station_walls(&doc, trip, "A2").unwrap();
    assert_eq!((a1.left, a1.up, a1.right), (Some(0.8), Some(2.5), None));
    assert_eq!((a2.right, a2.down, a2.left), (Some(1.2), Some(0.4), None));
    // The page agrees.
    let p = page(&doc, trip).unwrap();
    let row = p
        .rows
        .iter()
        .find(|r| r.kind == RowKind::Station && r.label == "A2")
        .unwrap();
    assert_eq!(row.lrud.as_ref().unwrap().right, Some(1.2));
    // Clearing a wall is an edit too.
    let a = set_station_wall(&doc, trip, "A2", Side::Right, None)
        .unwrap()
        .unwrap();
    apply(&mut doc, &mut h, a);
    assert_eq!(station_walls(&doc, trip, "A2").unwrap().right, None);
}

#[test]
fn stations_in_use_are_the_ones_a_shot_touches() {
    let (mut doc, trip, mut ids) = fresh();
    let mut h = History::new();
    let (_, a) =
        add_measured_shot(&doc, &mut ids, trip, "A1", "A2", false, Measured::default()).unwrap();
    apply(&mut doc, &mut h, a);
    assert_eq!(stations_in_use(&doc), set(&["A1", "A2"]));
}
