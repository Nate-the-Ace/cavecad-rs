//! The notebook page on Pitfall Cave (the default test cave) and Plumbline Pit (the vertical guard):
//! what the page shows, and that every edit is one undoable change that does not touch anything else.

use cave_doc::{ActorId, Document, Field, History, Kind};
use cave_draw::Walls;
use cave_id::{Id, SeqIds};
use cave_notebook::{
    Reading, RowKind, add_shot, new_trip, page, remove_shot, rename_station, set_reading,
    set_trip_declination, set_walls, trips,
};

fn load(name: &str) -> Document {
    let path = format!("{}/tests/data/{name}.json", env!("CARGO_MANIFEST_DIR"));
    cave_fixture::from_shot_list(&std::fs::read_to_string(path).unwrap(), ActorId(1)).unwrap()
}

fn apply(doc: &mut Document, h: &mut History, actions: Vec<cave_doc::Action>) {
    let edit = doc.edit(actions).unwrap();
    h.record(&edit);
}

fn first_shot_row(doc: &Document, trip: Id) -> (usize, Id) {
    let p = page(doc, trip).unwrap();
    let i = p.rows.iter().position(|r| r.kind == RowKind::Shot).unwrap();
    (i, p.rows[i].shot.unwrap())
}

#[test]
fn every_shot_of_the_cave_lands_on_exactly_one_page() {
    let doc = load("PitfallCave");
    let choices = trips(&doc);
    assert_eq!(choices.len(), 4);
    let on_pages: usize = choices
        .iter()
        .map(|c| {
            let p = page(&doc, c.id).unwrap();
            p.rows
                .iter()
                .filter(|r| matches!(r.kind, RowKind::Shot | RowKind::Splay))
                .count()
        })
        .sum();
    assert_eq!(on_pages, doc.ids_of(Kind::Shot).len());
    assert_eq!(choices.iter().map(|c| c.shots).sum::<usize>(), on_pages);
}

#[test]
fn a_page_reads_like_notes_station_then_shot_then_station() {
    let doc = load("PitfallCave");
    let p = page(&doc, trips(&doc)[0].id).unwrap();
    assert_eq!(p.rows[0].kind, RowKind::Station);
    assert!(p.rows[0].anchor);
    assert_eq!(p.rows[0].label, "A1");
    assert_eq!(p.rows[1].kind, RowKind::Shot);
    assert_eq!(p.rows[2].kind, RowKind::Station);
    assert_eq!(p.rows[2].label, "A2");
    // Splays are named for the station they start at and have no station line.
    let splay = trips(&doc)
        .iter()
        .flat_map(|c| page(&doc, c.id).unwrap().rows)
        .find(|r| r.kind == RowKind::Splay)
        .unwrap();
    assert!(splay.label.contains('.'), "{}", splay.label);
    assert!(splay.station.is_none());
    // The first station carries the trip's opening walls and they are editable here.
    assert!(p.rows[0].lrud_editable);
    assert_eq!(p.unit, "ft");
    assert_eq!(p.declination, Some(-4.66));
}

#[test]
fn running_total_ends_at_the_page_length() {
    let doc = load("PitfallCave");
    for c in trips(&doc) {
        let p = page(&doc, c.id).unwrap();
        let last = p
            .rows
            .iter()
            .rev()
            .find(|r| r.kind == RowKind::Shot)
            .and_then(|r| r.total)
            .unwrap_or(0.0);
        assert!((last - p.totals.length).abs() < 1e-9);
    }
}

#[test]
fn closures_across_pages_are_all_the_loops_and_ties_of_the_cave() {
    let doc = load("PitfallCave");
    let ex = cave_survey::extract(&doc);
    let r = cave_survey::resolve(&ex.legs, &ex.fixed);
    let shown: usize = trips(&doc)
        .iter()
        .map(|c| page(&doc, c.id).unwrap().closures.len())
        .sum();
    assert_eq!(shown, r.closures.len());
    assert!(shown >= 5, "Pitfall Cave has five loops");
}

#[test]
fn compass_cells_are_magnetic_and_the_survey_keeps_the_true_bearing() {
    let mut doc = load("PitfallCave");
    let mut h = History::new();
    let trip = trips(&doc)[0].id;
    let (row, shot) = first_shot_row(&doc, trip);
    let before = page(&doc, trip).unwrap();
    let actions = set_reading(&doc, shot, Reading::AzimuthFs, Some(100.0)).unwrap();
    apply(&mut doc, &mut h, actions);
    let p = page(&doc, trip).unwrap();
    assert!((p.rows[row].az_fs.unwrap() - 100.0).abs() < 1e-9);
    let stored = doc
        .entity(shot)
        .unwrap()
        .number(Field::ShotAzimuth)
        .unwrap();
    assert!(
        (stored - 95.34).abs() < 1e-9,
        "true = magnetic + declination, got {stored}"
    );
    h.undo(&mut doc).unwrap();
    assert_eq!(
        page(&doc, trip).unwrap(),
        before,
        "undo puts everything back"
    );
}

#[test]
fn tape_is_typed_in_metres_and_kept_in_the_trips_unit() {
    let mut doc = load("PitfallCave");
    let mut h = History::new();
    let trip = trips(&doc)[0].id;
    let (row, shot) = first_shot_row(&doc, trip);
    let actions = set_reading(&doc, shot, Reading::Distance, Some(3.048)).unwrap();
    apply(&mut doc, &mut h, actions);
    assert_eq!(
        doc.entity(shot).unwrap().number(Field::ShotDistance),
        Some(10.0)
    );
    assert!((page(&doc, trip).unwrap().rows[row].distance.unwrap() - 3.048).abs() < 1e-9);
    assert!(set_reading(&doc, shot, Reading::Distance, Some(-1.0)).is_err());
    assert!(set_reading(&doc, shot, Reading::InclinationFs, Some(95.0)).is_err());
}

#[test]
fn backsights_that_disagree_are_flagged() {
    let mut doc = load("PitfallCave");
    let mut h = History::new();
    let trip = trips(&doc)[0].id;
    let (row, shot) = first_shot_row(&doc, trip);
    let fs = page(&doc, trip).unwrap().rows[row].az_fs.unwrap();
    let ok = set_reading(
        &doc,
        shot,
        Reading::AzimuthBs,
        Some((fs + 180.0).rem_euclid(360.0)),
    )
    .unwrap();
    apply(&mut doc, &mut h, ok);
    assert!(!page(&doc, trip).unwrap().rows[row].warn.contains("differ"));
    let bad = set_reading(
        &doc,
        shot,
        Reading::AzimuthBs,
        Some((fs + 200.0).rem_euclid(360.0)),
    )
    .unwrap();
    apply(&mut doc, &mut h, bad);
    assert!(page(&doc, trip).unwrap().rows[row].warn.contains("compass"));
}

#[test]
fn changing_declination_swings_the_survey_and_keeps_what_the_compass_read() {
    let mut doc = load("PitfallCave");
    let mut h = History::new();
    let trip = trips(&doc)[0].id;
    let before = page(&doc, trip).unwrap();
    let (_, shot) = first_shot_row(&doc, trip);
    let true_before = doc
        .entity(shot)
        .unwrap()
        .number(Field::ShotAzimuth)
        .unwrap();
    let a = set_trip_declination(&doc, trip, -2.66).unwrap();
    apply(&mut doc, &mut h, a);
    let after = page(&doc, trip).unwrap();
    assert_eq!(after.declination, Some(-2.66));
    let true_after = doc
        .entity(shot)
        .unwrap()
        .number(Field::ShotAzimuth)
        .unwrap();
    assert!((true_after - true_before - 2.0).abs() < 1e-9);
    for (b, a) in before.rows.iter().zip(&after.rows) {
        match (b.az_fs, a.az_fs) {
            (Some(x), Some(y)) => assert!((x - y).abs() < 1e-9, "compass cells hold what was read"),
            (None, None) => {}
            _ => panic!("a reading appeared or vanished"),
        }
    }
    h.undo(&mut doc).unwrap();
    assert_eq!(page(&doc, trip).unwrap(), before);
}

#[test]
fn walls_edit_on_the_arriving_shot_only() {
    let mut doc = load("PitfallCave");
    let mut h = History::new();
    let trip = trips(&doc)[0].id;
    let (_, shot) = first_shot_row(&doc, trip);
    let p = page(&doc, trip).unwrap();
    let r = p
        .rows
        .iter()
        .find(|r| r.shot == Some(shot) && r.kind == RowKind::Station)
        .unwrap();
    let mut w = r.lrud.clone().unwrap();
    w.left = Some(1.5);
    let walls = Walls {
        left: w.left,
        right: w.right,
        up: w.up,
        down: w.down,
        left_open: w.left_open,
        right_open: w.right_open,
        up_open: w.up_open,
        down_open: w.down_open,
    };
    let a = set_walls(&doc, shot, walls).unwrap();
    assert_eq!(a.len(), 1, "only the changed side is written");
    apply(&mut doc, &mut h, a);
    let p = page(&doc, trip).unwrap();
    let r = p
        .rows
        .iter()
        .find(|r| r.shot == Some(shot) && r.kind == RowKind::Station)
        .unwrap();
    assert!((r.lrud.as_ref().unwrap().left.unwrap() - 1.5).abs() < 1e-9);
}

#[test]
fn adding_a_shot_appends_and_branching_makes_a_gap() {
    let mut doc = load("PitfallCave");
    let mut h = History::new();
    let trip = trips(&doc)[3].id;
    let rows = page(&doc, trip).unwrap().rows.len();
    let mut ids = SeqIds::new(0xfeed);
    let (_, a) = add_shot(&doc, &mut ids, trip, "A1", "NEW1", false).unwrap();
    apply(&mut doc, &mut h, a);
    let p = page(&doc, trip).unwrap();
    // A1 is not where the chain stood, so the page breaks the chain: gap, A1 again, shot, NEW1.
    assert_eq!(p.rows.len(), rows + 4);
    assert_eq!(p.rows[rows].kind, RowKind::Gap);
    assert_eq!(p.rows.last().unwrap().label, "NEW1");
    assert!(add_shot(&doc, &mut ids, trip, "A1", "A1", false).is_err());
    let (id, a) = add_shot(&doc, &mut ids, trip, "NEW1", "", true).unwrap();
    apply(&mut doc, &mut h, a);
    let p = page(&doc, trip).unwrap();
    assert_eq!(p.rows.last().unwrap().label, "NEW1.1");
    let a = remove_shot(&doc, id).unwrap();
    apply(&mut doc, &mut h, a);
    assert_eq!(page(&doc, trip).unwrap().rows.last().unwrap().label, "NEW1");
}

#[test]
fn renaming_a_station_refuses_a_name_already_taken() {
    let doc = load("PitfallCave");
    let trip = trips(&doc)[0].id;
    let p = page(&doc, trip).unwrap();
    let a1 = p.rows[0].station.unwrap();
    assert!(rename_station(&doc, a1, "A2").is_err());
    assert!(rename_station(&doc, a1, " ").is_err());
    assert!(rename_station(&doc, a1, "A1").unwrap().is_empty());
    assert_eq!(rename_station(&doc, a1, "ENT").unwrap().len(), 1);
}

#[test]
fn a_new_trip_starts_with_an_empty_page_in_the_same_unit() {
    let mut doc = load("PitfallCave");
    let mut h = History::new();
    let mut ids = SeqIds::new(0xbee);
    let (id, a) = new_trip(&doc, &mut ids, "SECOND LOOK").unwrap();
    apply(&mut doc, &mut h, a);
    let p = page(&doc, id).unwrap();
    assert!(p.rows.is_empty());
    assert_eq!(p.unit, "ft");
    assert_eq!(trips(&doc).len(), 5);
}

#[test]
fn plumbline_pit_never_gains_a_bearing_it_was_not_given() {
    let doc = load("PlumblinePit");
    for c in trips(&doc) {
        let p = page(&doc, c.id).unwrap();
        for r in p.rows.iter().filter(|r| r.kind == RowKind::Shot) {
            let shot = doc.entity(r.shot.unwrap()).unwrap();
            if shot.number(Field::ShotAzimuth).is_none() {
                assert!(
                    r.az_fs.is_none(),
                    "a pitch has no bearing: nothing may invent one"
                );
            }
        }
    }
}
