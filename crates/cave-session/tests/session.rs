//! A whole survey, end to end with everything but the screen: a simulated instrument's bytes, framing and
//! acks, decoding, grouping into legs, the session's rules, and the notebook writes, on a real document.
//! Ported from the web app's `x1session`, `devsim` and `surveywriter` checks.

use cave_doc::{Action, ActorId, Document, History};
use cave_id::{Id, IdSource, SeqIds};
use cave_instrument::legs::{LegEvent, Spread};
use cave_instrument::link::LinkOut;
use cave_instrument::sim::{
    DeviceKind, FlagPolicy, SimOptions, Simulator, Step, StepKind, TraverseOpts, devices, traverse,
};
use cave_instrument::{Mark, Reading, Side};
use cave_notebook::{Row, RowKind, new_cave, page, trips};
use cave_session::{Host, Incoming, Mode, PlanKind, Session};

struct Book {
    doc: Document,
    ids: SeqIds,
    history: History,
    trip: Id,
}

impl Book {
    fn new() -> Book {
        let mut ids = SeqIds::new(11);
        let doc = new_cave(ActorId(1), &mut ids, "Test", "Trip 1", "m").unwrap();
        let trip = trips(&doc)[0].id;
        Book {
            doc,
            ids,
            history: History::new(),
            trip,
        }
    }

    fn rows(&self) -> Vec<Row> {
        page(&self.doc, self.trip).unwrap().rows
    }

    /// The legs on the page as `(from, to, row)`.
    fn legs(&self) -> Vec<(String, String, Row)> {
        let rows = self.rows();
        let mut out = Vec::new();
        let mut from = String::new();
        for (i, r) in rows.iter().enumerate() {
            match r.kind {
                RowKind::Station if r.anchor => from = r.label.clone(),
                RowKind::Shot => {
                    let to = rows[i + 1..]
                        .iter()
                        .find(|x| x.kind == RowKind::Station)
                        .map(|x| x.label.clone())
                        .unwrap_or_default();
                    out.push((from.clone(), to.clone(), r.clone()));
                    from = to;
                }
                RowKind::Station => from = r.label.clone(),
                _ => {}
            }
        }
        out
    }

    /// The stations the splays start at, in page order.
    fn splays(&self) -> Vec<String> {
        self.rows()
            .iter()
            .filter(|r| r.kind == RowKind::Splay)
            .map(|r| r.label.split('.').next().unwrap().to_string())
            .collect()
    }

    fn leg_names(&self) -> String {
        self.legs()
            .iter()
            .map(|(a, b, _)| format!("{a}-{b}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn edits(&self) -> usize {
        self.doc.ids_of(cave_doc::Kind::Shot).len()
    }
}

impl Host for Book {
    fn doc(&self) -> &Document {
        &self.doc
    }
    fn doc_and_ids(&mut self) -> (&Document, &mut dyn IdSource) {
        (&self.doc, &mut self.ids)
    }
    fn commit(&mut self, actions: Vec<Action>) -> Result<(), String> {
        let edit = self.doc.edit(actions).map_err(|e| format!("{e:?}"))?;
        self.history.record(&edit);
        Ok(())
    }
}

fn session(book: &Book, groups_legs: bool, backsights: bool) -> Session {
    let mut s = Session::new();
    s.sync_to_trip(book, book.trip);
    s.connected(groups_legs);
    s.set_backsights(backsights);
    s
}

fn feed(s: &mut Session, book: &mut Book, outs: Vec<LinkOut>) {
    for o in outs {
        if let LinkOut::Reading { reading } = o {
            s.reading(book, reading);
        }
    }
}

fn near(a: Option<f64>, b: f64, e: f64) -> bool {
    a.is_some_and(|a| (a - b).abs() <= e)
}

/// Fires every shot of the script one by one through the simulator, the grouping and the session.
fn shoot_all(
    steps: &[Step],
    policy: FlagPolicy,
    backsights: bool,
    quirks: &[&str],
) -> (Book, Session) {
    let mut book = Book::new();
    let mut s = session(&book, true, backsights);
    let mut sim = Simulator::new(SimOptions {
        kind: DeviceKind::X1,
        steps: steps.to_vec(),
        policy,
        quirks: quirks.iter().map(|q| q.to_string()).collect(),
    });
    sim.connect();
    while sim.upcoming().is_some() {
        let outs = sim.take_shot();
        feed(&mut s, &mut book, outs);
    }
    s.settle_now(&mut book);
    (book, s)
}

fn with_backs() -> Vec<Step> {
    traverse(&TraverseOpts {
        seed: 5,
        legs: 6,
        backsights: true,
        splays: 2,
        start_splays: false,
        lrud: false,
    })
}

#[test]
fn a_whole_survey_from_a_simulated_x1() {
    let steps = with_backs();
    let leg_steps: Vec<&Step> = steps.iter().filter(|s| s.kind == StepKind::Leg).collect();
    let back_steps: Vec<&Step> = steps.iter().filter(|s| s.kind == StepKind::Back).collect();
    for policy in [FlagPolicy::All, FlagPolicy::Third, FlagPolicy::None] {
        let (book, s) = shoot_all(&steps, policy, true, &[]);
        let snap = s.snapshot(&book);
        assert_eq!(snap.message, "", "{policy:?}: nothing refused");
        assert_eq!(
            book.leg_names(),
            "A1-A2 A2-A3 A3-A4 A4-A5 A5-A6 A6-A7",
            "{policy:?}"
        );
        let splays = book.splays();
        assert_eq!(splays.len(), 12, "{policy:?}");
        assert!(
            splays
                .iter()
                .enumerate()
                .all(|(i, f)| *f == format!("A{}", i / 2 + 2)),
            "{policy:?}: two splays at each new station: {splays:?}"
        );
        let legs = book.legs();
        for (i, (_, _, r)) in legs.iter().enumerate() {
            assert!(
                near(r.distance, leg_steps[i].distance, 0.004)
                    && near(r.az_fs, leg_steps[i].azimuth, 0.3)
                    && near(r.inc_fs, leg_steps[i].inclination, 0.3),
                "{policy:?}: foresight {i}"
            );
            assert!(
                near(r.az_bs, back_steps[i].azimuth, 0.3)
                    && near(r.inc_bs, back_steps[i].inclination, 0.3),
                "{policy:?}: backsight {i}"
            );
        }
        assert_eq!(snap.state.from, "A7", "{policy:?}");
        assert!(snap.state.awaiting.is_none());
        assert_eq!(
            snap.log.len(),
            6 + 6 + 12,
            "{policy:?}: 24 things in the log"
        );
    }
}

type Cells = Vec<(
    String,
    String,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
)>;

fn notebook_cells(book: &Book) -> Cells {
    book.legs()
        .into_iter()
        .map(|(a, b, r)| (a, b, r.distance, r.az_fs, r.inc_fs, r.az_bs, r.inc_bs))
        .collect()
}

#[test]
fn a_noisy_link_gives_the_identical_notebook() {
    let steps = with_backs();
    let (clean, _) = shoot_all(&steps, FlagPolicy::Third, true, &[]);
    let (noisy, _) = shoot_all(&steps, FlagPolicy::Third, true, &["lost-acks", "short"]);
    assert_eq!(notebook_cells(&clean), notebook_cells(&noisy));
    assert_eq!(clean.splays(), noisy.splays());
}

#[test]
fn legs_run_on_without_backsights_and_skipped_ones_are_forgiven() {
    let steps = traverse(&TraverseOpts {
        seed: 8,
        legs: 4,
        backsights: false,
        splays: 2,
        start_splays: false,
        lrud: false,
    });
    for backsights in [false, true] {
        let (book, s) = shoot_all(&steps, FlagPolicy::Third, backsights, &[]);
        assert_eq!(s.snapshot(&book).message, "", "backsights {backsights}");
        assert_eq!(book.leg_names(), "A1-A2 A2-A3 A3-A4 A4-A5");
        assert!(
            book.splays()
                .iter()
                .enumerate()
                .all(|(i, f)| *f == format!("A{}", i / 2 + 2)),
            "splays are at the stations they were taken at"
        );
        assert!(book.legs().iter().all(|(_, _, r)| r.az_bs.is_none()));
    }
}

#[test]
fn a_backsight_marked_on_the_instrument_finds_its_leg_without_the_switch() {
    let (book, s) = shoot_all(&with_backs(), FlagPolicy::Third, false, &[]);
    assert_eq!(s.snapshot(&book).message, "");
    let legs = book.legs();
    assert_eq!(legs.len(), 6);
    assert!(legs.iter().all(|(_, _, r)| r.az_bs.is_some()));
    assert!(
        book.splays()
            .iter()
            .enumerate()
            .all(|(i, f)| *f == format!("A{}", i / 2 + 2))
    );
}

fn r(distance: f64, azimuth: f64, inclination: f64) -> Reading {
    Reading::new(distance, azimuth, inclination, "t")
}

fn leg_event(group: u32, reading: Reading, mark: Mark, spread: Spread) -> LegEvent {
    LegEvent::Leg {
        group,
        reading,
        shots: 3,
        mark,
        spread,
        flagged: true,
    }
}

const NO_SPREAD: Spread = Spread {
    distance: 0.0,
    azimuth: 0.0,
    inclination: 0.0,
};

#[test]
fn refusals_write_nothing() {
    let mut book = Book::new();
    let mut s = session(&book, true, false);
    // A backsight with nothing before it.
    let ev = leg_event(
        1,
        Reading {
            mark: Some(Mark::Backsight),
            ..r(5.0, 280.0, 3.0)
        },
        Mark::Backsight,
        NO_SPREAD,
    );
    assert!(!s.commit(&mut book, &Incoming::Event(ev)));
    assert!(s.snapshot(&book).message.contains("needs a leg"));
    assert_eq!(book.edits(), 0);
    // A zero tape.
    assert!(!s.commit(
        &mut book,
        &Incoming::Event(LegEvent::Splay {
            reading: r(0.0, 280.0, 3.0)
        })
    ));
    assert_eq!(book.edits(), 0);
    // A single reading in manual mode is the next leg.
    s.connected(false);
    assert_eq!(s.state().mode, Mode::Leg);
    assert!(s.commit(&mut book, &Incoming::Single(r(4.0, 10.0, 0.0))));
    assert_eq!(s.snapshot(&book).log[0].label, "A1 to A2");
    assert_eq!(book.edits(), 1);
    // An update for a leg that was taken back is quietly dropped.
    let msg = s.snapshot(&book).message.clone();
    let gone = LegEvent::LegUpdate {
        group: 99,
        reading: r(5.0, 1.0, 1.0),
        shots: 4,
        mark: Mark::None,
        spread: NO_SPREAD,
        flagged: false,
    };
    assert!(!s.commit(&mut book, &Incoming::Event(gone)));
    assert_eq!(s.snapshot(&book).message, msg);
}

#[test]
fn an_accuracy_error_and_a_loose_leg_are_both_written_and_both_said() {
    let mut book = Book::new();
    let mut s = session(&book, true, false);
    let reading = Reading {
        error: Some("accuracy error: absG".into()),
        ..r(5.0, 100.0, 3.0)
    };
    let ev = leg_event(
        1,
        reading,
        Mark::None,
        Spread {
            distance: 0.05,
            azimuth: 1.9,
            inclination: 0.2,
        },
    );
    assert!(s.commit(&mut book, &Incoming::Event(ev)));
    let snap = s.snapshot(&book);
    assert!(snap.warnings.iter().any(|w| w.contains("accuracy error")));
    assert!(snap.warnings.iter().any(|w| w.contains("differ by")));
    assert_eq!(book.edits(), 1);
}

#[test]
fn a_leg_that_gains_a_shot_is_refined_in_place() {
    let mut book = Book::new();
    let mut s = session(&book, true, false);
    let first = leg_event(4, r(5.0, 100.0, 3.0), Mark::None, NO_SPREAD);
    assert!(s.commit(&mut book, &Incoming::Event(first)));
    let update = LegEvent::LegUpdate {
        group: 4,
        reading: r(5.02, 100.4, 3.1),
        shots: 4,
        mark: Mark::None,
        spread: NO_SPREAD,
        flagged: true,
    };
    assert!(s.commit(&mut book, &Incoming::Event(update)));
    let legs = book.legs();
    assert_eq!(legs.len(), 1);
    assert!(
        near(legs[0].2.distance, 5.02, 1e-9)
            && near(legs[0].2.az_fs, 100.4, 1e-9)
            && near(legs[0].2.inc_fs, 3.1, 1e-9)
    );
    assert!(near(
        s.snapshot(&book).log[0].reading.distance.into(),
        5.02,
        1e-9
    ));
}

fn stations(steps: &[Step], backsights: bool, wall_shots: bool) -> (Book, Session, Vec<usize>) {
    let mut book = Book::new();
    let mut s = session(&book, true, backsights);
    let mut sim = Simulator::new(SimOptions {
        kind: DeviceKind::X1,
        steps: steps.to_vec(),
        policy: FlagPolicy::Third,
        quirks: vec![],
    });
    sim.connect();
    let mut fired = Vec::new();
    for _ in 0..4 {
        let (n, outs) = sim.take_station(backsights, wall_shots);
        feed(&mut s, &mut book, outs);
        s.settle_now(&mut book); // the surveyor stops: what waits is settled
        fired.push(n);
    }
    (book, s, fired)
}

#[test]
fn one_press_shoots_a_whole_station() {
    let steps = traverse(&TraverseOpts {
        seed: 31,
        legs: 3,
        backsights: true,
        splays: 4,
        start_splays: false,
        lrud: false,
    });
    let (book, s, fired) = stations(&steps, true, true);
    assert_eq!(fired, [10, 10, 10, 0]);
    assert_eq!(book.leg_names(), "A1-A2 A2-A3 A3-A4");
    assert_eq!(book.splays().len(), 12);
    assert!(book.legs().iter().all(|(_, _, r)| r.az_bs.is_some()));
    assert_eq!(s.snapshot(&book).state.from, "A4");
    let (book, s, fired) = stations(&steps, false, true);
    assert_eq!(fired, [7, 7, 7, 0]);
    assert!(book.legs().iter().all(|(_, _, r)| r.az_bs.is_none()));
    assert_eq!(
        book.rows()
            .iter()
            .filter(|r| r.kind == RowKind::Splay)
            .count(),
        12
    );
    assert_eq!(s.snapshot(&book).state.from, "A4");
}

#[test]
fn wall_shots_become_dimensions_and_never_splays() {
    let steps = traverse(&TraverseOpts {
        seed: 31,
        legs: 3,
        backsights: true,
        splays: 4,
        start_splays: true,
        lrud: true,
    });
    let (book, s, fired) = stations(&steps, true, true);
    assert_eq!(
        fired,
        [14, 10, 10, 0],
        "the first click is four wall shots, a leg, its backsight and four wall shots"
    );
    assert_eq!(s.snapshot(&book).message, "");
    assert_eq!(book.leg_names(), "A1-A2 A2-A3 A3-A4");
    assert_eq!(book.splays().len(), 0, "no splay rows at all");
    // Sixteen dimensions: left, right, up and down at each of the four stations.
    let mut count = 0;
    for st in ["A1", "A2", "A3", "A4"] {
        let w = cave_notebook::station_walls(&book.doc, book.trip, st).unwrap();
        for side in cave_notebook::Side::ALL {
            assert!(side.of(&w).is_some_and(|v| v > 0.0), "{st} {side:?}");
            count += 1;
        }
    }
    assert_eq!(count, 16);
}

#[test]
fn walls_shot_before_the_first_leg_wait_for_it() {
    let steps = traverse(&TraverseOpts {
        seed: 31,
        legs: 1,
        backsights: false,
        splays: 4,
        start_splays: true,
        lrud: true,
    });
    let mut book = Book::new();
    let mut s = session(&book, true, false);
    let mut sim = Simulator::new(SimOptions {
        kind: DeviceKind::X1,
        steps: steps.clone(),
        policy: FlagPolicy::Third,
        quirks: vec![],
    });
    sim.connect();
    // Four wall shots only: the station has no line yet.
    for _ in 0..4 {
        let outs = sim.take_shot();
        feed(&mut s, &mut book, outs);
    }
    s.settle_now(&mut book);
    assert_eq!(s.snapshot(&book).message, "");
    assert_eq!(book.edits(), 0, "nothing is on the page yet");
    assert_eq!(s.snapshot(&book).log.len(), 4);
    // The leg gives A1 its line; the waiting dimensions are written.
    for _ in 0..3 {
        let outs = sim.take_shot();
        feed(&mut s, &mut book, outs);
    }
    s.settle_now(&mut book);
    let w = cave_notebook::station_walls(&book.doc, book.trip, "A1").unwrap();
    assert!(
        w.left.is_some() && w.right.is_some() && w.up.is_some() && w.down.is_some(),
        "{w:?}"
    );
    assert!((w.left.unwrap() - steps[4].behind.unwrap().left).abs() < 0.04);
}

#[test]
fn all_four_devices_write_the_same_survey() {
    let steps = traverse(&TraverseOpts {
        seed: 31,
        legs: 3,
        backsights: true,
        splays: 4,
        start_splays: true,
        lrud: true,
    });
    let survey = |kind: DeviceKind| {
        let mut book = Book::new();
        let mut s = session(&book, true, true);
        let mut sim = Simulator::new(SimOptions {
            kind,
            steps: steps.clone(),
            policy: FlagPolicy::Third,
            quirks: vec![],
        });
        sim.connect();
        for _ in 0..4 {
            let (_, outs) = sim.take_station(true, true);
            feed(&mut s, &mut book, outs);
            s.settle_now(&mut book);
        }
        assert_eq!(s.snapshot(&book).message, "", "{kind:?}");
        (book, s)
    };
    let (x1, _) = survey(DeviceKind::X1);
    for kind in [DeviceKind::Sap6, DeviceKind::Sap5, DeviceKind::Bric] {
        let (b, _) = survey(kind);
        assert_eq!(b.leg_names(), "A1-A2 A2-A3 A3-A4", "{kind:?}");
        for ((_, _, a), (_, _, e)) in b.legs().iter().zip(x1.legs().iter()) {
            assert!(
                near(a.distance, e.distance.unwrap(), 0.01),
                "{kind:?} distance"
            );
            for (p, q) in [
                (a.az_fs, e.az_fs),
                (a.inc_fs, e.inc_fs),
                (a.az_bs, e.az_bs),
                (a.inc_bs, e.inc_bs),
            ] {
                assert!(near(p, q.unwrap(), 0.05), "{kind:?} angle {p:?} {q:?}");
            }
        }
        for st in ["A1", "A2", "A3", "A4"] {
            let (wb, wx) = (
                cave_notebook::station_walls(&b.doc, b.trip, st).unwrap(),
                cave_notebook::station_walls(&x1.doc, x1.trip, st).unwrap(),
            );
            for side in cave_notebook::Side::ALL {
                assert!(
                    near(side.of(&wb), side.of(&wx).unwrap(), 0.01),
                    "{kind:?} {st} {side:?}"
                );
            }
        }
    }
}

#[test]
fn every_quirk_of_every_device_is_coped_with() {
    let steps = traverse(&TraverseOpts {
        seed: 31,
        legs: 3,
        backsights: true,
        splays: 4,
        start_splays: true,
        lrud: true,
    });
    let run = |kind: DeviceKind, quirk: &str| {
        let mut book = Book::new();
        let mut s = session(&book, true, true);
        let quirks = if quirk == "none" {
            vec![]
        } else {
            vec![quirk.to_string()]
        };
        let mut sim = Simulator::new(SimOptions {
            kind,
            steps: steps.clone(),
            policy: FlagPolicy::Third,
            quirks,
        });
        sim.connect();
        for _ in 0..4 {
            let (_, outs) = sim.take_station(true, true);
            feed(&mut s, &mut book, outs);
            s.settle_now(&mut book);
        }
        let snap = s.snapshot(&book);
        (
            notebook_cells(&book),
            snap.message,
            snap.log
                .iter()
                .flat_map(|e| e.warnings.clone())
                .collect::<Vec<_>>(),
        )
    };
    for d in devices() {
        let (clean, message, _) = run(d.kind, "none");
        assert_eq!(message, "", "{:?}", d.kind);
        assert_eq!(clean.len(), 3);
        for q in d.quirks.iter().filter(|q| q.id != "none") {
            let (cells, message, warnings) = run(d.kind, q.id);
            assert_eq!(message, "", "{:?} / {}", d.kind, q.id);
            match (d.kind, q.id) {
                // These change nothing about the survey, or only add a warning.
                (DeviceKind::Sap5, "fault32") => assert_eq!(
                    cells.len(),
                    3,
                    "the long legs still read 8 times as far, not 65 m too far"
                ),
                (DeviceKind::X1, "accuracy") => assert!(
                    warnings.iter().any(|w| w.contains("accuracy error")),
                    "{warnings:?}"
                ),
                (DeviceKind::Bric, "errors") => assert!(
                    warnings.iter().any(|w| w.contains("the target moved")),
                    "{warnings:?}"
                ),
                _ => assert_eq!(
                    cells, clean,
                    "{:?} / {}: the survey is unchanged",
                    d.kind, q.id
                ),
            }
        }
    }
}

#[test]
fn undo_takes_back_one_thing_in_one_step_and_puts_the_surveyor_back() {
    let mut book = Book::new();
    let mut s = session(&book, false, true);
    s.connected(false);
    s.set_mode(&book, Mode::Leg);
    let before = s.snapshot(&book).state;
    assert!(s.commit(&mut book, &Incoming::Single(r(5.0, 100.0, 2.0))));
    assert!(
        s.snapshot(&book).state.awaiting.is_some(),
        "now waiting for the backsight"
    );
    assert!(s.commit(&mut book, &Incoming::Single(r(5.02, 280.5, -2.1))));
    assert_eq!(
        s.snapshot(&book).state.from,
        "A2",
        "the backsight moved the surveyor on"
    );
    assert!(book.legs()[0].2.az_bs.is_some());
    // Undo the backsight: its cells clear, the foresight stays, the surveyor is waiting for it again.
    assert!(s.undo_last(&mut book));
    assert!(book.legs()[0].2.az_bs.is_none() && book.legs()[0].2.distance.is_some());
    assert!(s.snapshot(&book).state.awaiting.is_some());
    // Undo the leg: the shot is gone and the surveyor stands where they began.
    assert!(s.undo_last(&mut book));
    assert_eq!(book.edits(), 0);
    assert_eq!(s.snapshot(&book).state, before);
    assert!(!s.undo_last(&mut book), "nothing more to take back");
    // A splay is one history step: the notebook's own undo agrees.
    s.set_backsights(false);
    s.set_mode(&book, Mode::Splay);
    assert!(s.commit(&mut book, &Incoming::Single(r(3.0, 10.0, 0.0))));
    assert_eq!(book.splays().len(), 1);
    book.history.undo(&mut book.doc).unwrap();
    assert_eq!(
        book.splays().len(),
        0,
        "one notebook undo removes the whole splay"
    );
}

#[test]
fn a_held_thing_waits_for_accept_or_discard() {
    let mut book = Book::new();
    let mut s = session(&book, false, false);
    s.set_auto(false);
    s.set_mode(&book, Mode::Leg);
    s.reading(&mut book, r(5.0, 100.0, 2.0));
    s.reading(&mut book, r(6.0, 110.0, 2.0));
    let snap = s.snapshot(&book);
    assert_eq!(snap.held.len(), 2);
    assert_eq!(snap.held[0].text, "A1 to A2");
    assert_eq!(book.edits(), 0, "nothing is written until it is accepted");
    s.accept(&mut book);
    assert_eq!(book.edits(), 1);
    // The second was previewed as A1 to A2 too; now the surveyor has moved on, so it is A2 to A3.
    assert_eq!(s.snapshot(&book).held[0].text, "A2 to A3");
    s.discard();
    assert!(s.snapshot(&book).held.is_empty());
    assert_eq!(book.edits(), 1);
}

#[test]
fn a_manual_instrument_takes_legs_splays_and_walls_by_mode() {
    let mut book = Book::new();
    let mut s = session(&book, false, false);
    s.set_mode(&book, Mode::Leg);
    s.reading(&mut book, r(5.0, 100.0, 2.0));
    s.set_mode(&book, Mode::Splay);
    s.reading(&mut book, r(2.0, 190.0, 0.0));
    s.set_mode(&book, Mode::Lrud);
    s.reading(&mut book, r(1.5, 190.0, 0.0)); // 90 degrees left of the last leg's heading... square to it
    let snap = s.snapshot(&book);
    assert_eq!(snap.message, "");
    assert_eq!(
        snap.log.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [PlanKind::Leg, PlanKind::Splay, PlanKind::Lrud]
    );
    assert_eq!(book.splays(), ["A2"]);
    let w = cave_notebook::station_walls(&book.doc, book.trip, "A2").unwrap();
    assert!(w.left.is_some() || w.right.is_some(), "{w:?}");
    // Along the passage is refused.
    s.reading(&mut book, r(3.0, 100.0, 0.0));
    assert!(s.snapshot(&book).message.contains("along the passage"));
}

#[test]
fn the_standing_station_takes_dimensions_by_hand() {
    let mut book = Book::new();
    let mut s = session(&book, false, false);
    s.set_mode(&book, Mode::Leg);
    assert!(
        !s.set_standing_wall(&mut book, Side::Left, Some(1.0)),
        "no line yet"
    );
    assert!(s.snapshot(&book).message.contains("no line on the page"));
    s.reading(&mut book, r(5.0, 100.0, 2.0));
    assert!(s.set_standing_wall(&mut book, Side::Left, Some(1.0)));
    let w = s.snapshot(&book).standing_walls.unwrap();
    assert_eq!((w.station.as_str(), w.left), ("A2", Some(1.0)));
}

#[test]
fn the_session_follows_a_real_cave_and_never_reuses_a_name() {
    // Pitfall Cave: open its last trip and carry on from where the page ends.
    let path = format!(
        "{}/../cave-notebook/tests/data/PitfallCave.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let doc =
        cave_fixture::from_shot_list(&std::fs::read_to_string(path).unwrap(), ActorId(1)).unwrap();
    let trip = trips(&doc).last().unwrap().id;
    let last = page(&doc, trip)
        .unwrap()
        .rows
        .into_iter()
        .rev()
        .find(|r| r.kind == RowKind::Station)
        .unwrap()
        .label;
    let mut book = Book {
        doc,
        ids: SeqIds::new(99),
        history: History::new(),
        trip,
    };
    let mut s = Session::new();
    s.sync_to_trip(&book, trip);
    assert_eq!(s.state().from, last);
    let next = s.state().to.clone();
    assert!(!next.is_empty());
    assert!(
        !cave_notebook::stations_in_use(&book.doc).contains(&next),
        "{next} is already a station"
    );
    s.connected(false);
    s.set_mode(&book, Mode::Leg);
    s.reading(&mut book, r(4.0, 45.0, 1.0));
    assert_eq!(s.snapshot(&book).message, "");
    assert_eq!(s.snapshot(&book).log[0].label, format!("{last} to {next}"));
}
