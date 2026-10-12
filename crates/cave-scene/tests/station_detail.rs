//! The station tab's data and edits, on Pitfall Cave: walls and notes are read from, and written back to,
//! the shot (or trip row) that holds them; benchmark is a station flag; each is one undoable edit that
//! changes only what differs.

use cave_doc::{ActorId, Document, Field, History, Kind};
use cave_draw::{
    Walls, set_note, set_station_benchmark, set_station_walls, walls_at, walls_source,
};
use cave_id::Id;
use cave_scene::station_detail;

fn pitfall() -> Document {
    let path = format!(
        "{}/../cave-survey/tests/golden/PitfallCave.json",
        env!("CARGO_MANIFEST_DIR")
    );
    cave_fixture::from_shot_list(&std::fs::read_to_string(path).unwrap(), ActorId(1)).unwrap()
}

fn station(doc: &Document, label: &str) -> Id {
    doc.ids_of(Kind::Station)
        .into_iter()
        .find(|id| doc.entity(*id).unwrap().text(Field::StationLabel) == label)
        .unwrap()
}

#[test]
fn walls_edit_in_metres_go_to_the_shot_that_holds_them_and_undo() {
    let mut doc = pitfall();
    let a5 = station(&doc, "A5");
    let d = station_detail(&doc, true, a5).unwrap();
    let w = d.walls.expect("A5 has walls");
    assert!(w.on.starts_with("shot "), "kept on {}", w.on);
    let mut want = Walls {
        left: w.left,
        right: w.right,
        up: w.up,
        down: w.down,
        left_open: w.left_open,
        right_open: w.right_open,
        up_open: w.up_open,
        down_open: w.down_open,
    };
    assert!(
        set_station_walls(&doc, a5, want).unwrap().is_empty(),
        "nothing changed, nothing written"
    );
    want.left = Some(2.5);
    want.up_open = true;
    let actions = set_station_walls(&doc, a5, want).unwrap();
    assert_eq!(actions.len(), 2);
    let mut h = History::new();
    h.record(&doc.edit(actions).unwrap());
    let src = walls_source(&doc, a5).unwrap();
    let now = walls_at(&doc, src);
    assert!((now.left.unwrap() - 2.5).abs() < 1e-9 && now.up_open);
    assert!(
        (station_detail(&doc, true, a5)
            .unwrap()
            .walls
            .unwrap()
            .left
            .unwrap()
            - 2.5)
            .abs()
            < 1e-9
    );
    h.undo(&mut doc).unwrap();
    assert!((walls_at(&doc, src).left.unwrap() - 2.5).abs() > 1e-6);
    want.left = Some(-1.0);
    assert!(
        set_station_walls(&doc, a5, want).is_err(),
        "no negative distance"
    );
}

#[test]
fn the_first_station_keeps_its_walls_on_the_trips_opening_row() {
    let mut doc = pitfall();
    let a1 = station(&doc, "A1");
    let src = walls_source(&doc, a1).expect("a shot leaves A1");
    assert!(src.trip, "nothing arrives at the first station");
    let mut w = walls_at(&doc, src);
    w.left = Some(1.25);
    doc.edit(set_station_walls(&doc, a1, w).unwrap()).unwrap();
    let d = station_detail(&doc, true, a1).unwrap();
    assert_eq!(d.walls.as_ref().unwrap().on, "trip opening row");
    assert!((d.walls.unwrap().left.unwrap() - 1.25).abs() < 1e-9);
}

#[test]
fn notes_and_benchmark_edit_where_they_live() {
    let mut doc = pitfall();
    let a5 = station(&doc, "A5");
    let d = station_detail(&doc, true, a5).unwrap();
    let (source, _) = cave_draw::note_sources(&doc, a5)
        .pop()
        .expect("a shot arrives at A5");
    doc.edit(set_note(&doc, source, "  low crawl, LEAD east  ").unwrap())
        .unwrap();
    let d2 = station_detail(&doc, true, a5).unwrap();
    assert_eq!(d2.notes.last().unwrap().text, "low crawl, LEAD east");
    assert!(
        d2.kinds.contains(&"lead"),
        "a note that says LEAD makes it a lead"
    );
    assert!(
        set_note(&doc, source, "low crawl, LEAD east")
            .unwrap()
            .is_empty()
    );
    assert!(!d.benchmark);
    doc.edit(set_station_benchmark(&doc, a5, true).unwrap())
        .unwrap();
    assert!(station_detail(&doc, true, a5).unwrap().benchmark);
    assert!(set_station_benchmark(&doc, a5, true).unwrap().is_empty());
    assert!(
        set_note(&doc, a5, "x").is_err(),
        "a station holds no note itself"
    );
}
