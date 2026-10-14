//! The in-cave Survey in the browser: thin `wasm-bindgen` doors onto `cave-instrument` (what an instrument
//! sends), `cave-session` (what to write and where the surveyor stands) and `cave-sketch` (the sketchpad's
//! views). The logic is in those crates and tested natively; this holds the one live session, the link to the
//! instrument that is up, and the simulator, and answers every call with one JSON "step": the log lines, the
//! bytes to write back to the instrument, and the session as the screen shows it.

use crate::{DOC, record_edit};
use cave_doc::{Action, Document};
use cave_id::IdSource;
use cave_instrument::identify::{NAMES, identify};
use cave_instrument::link::{BricKind, BricLink, LegLink, LinkOut, Sap5Link, X1Link};
use cave_instrument::sim::{
    DeviceKind, FlagPolicy, SimOptions, Simulator, TraverseOpts, device_of, devices, traverse,
};
use cave_instrument::{Side, bric, caveble, cavway, sap5};
use cave_session::{Host, Mode, Session, Snapshot};
use serde::Serialize;
use std::cell::RefCell;
use wasm_bindgen::prelude::*;

/// The link to the instrument that is up.
enum Link {
    X1(X1Link),
    Sap6(LegLink),
    Sap5(Sap5Link),
    Bric(BricLink),
}

thread_local! {
    static SESSION: RefCell<Session> = RefCell::new(Session::new());
    static LINK: RefCell<Option<Link>> = const { RefCell::new(None) };
    static SIM: RefCell<Option<Simulator>> = const { RefCell::new(None) };
}

#[cfg(target_arch = "wasm32")]
fn fresh_ids() -> Box<dyn IdSource> {
    Box::new(crate::random_ids())
}

/// Native builds (the tests) have no browser random source: a counting sequence will do.
#[cfg(not(target_arch = "wasm32"))]
fn fresh_ids() -> Box<dyn IdSource> {
    thread_local!(static NEXT: RefCell<u64> = const { RefCell::new(1) });
    Box::new(cave_id::SeqIds::new(NEXT.with(|n| {
        let mut n = n.borrow_mut();
        *n += 1;
        *n
    })))
}

/// The open cave as the session sees it: edits go in as one undoable step of the notebook's history.
struct WasmHost<'a> {
    doc: &'a mut Document,
    ids: Box<dyn IdSource>,
}

impl Host for WasmHost<'_> {
    fn doc(&self) -> &Document {
        self.doc
    }

    fn doc_and_ids(&mut self) -> (&Document, &mut dyn IdSource) {
        (self.doc, &mut *self.ids)
    }

    fn commit(&mut self, actions: Vec<Action>) -> Result<(), String> {
        let edit = self.doc.edit(actions).map_err(|e| format!("{e:?}"))?;
        record_edit("notebook", &edit);
        Ok(())
    }
}

#[derive(Serialize)]
struct FrameLine {
    hex: String,
    from: String,
}

/// What every call answers.
#[derive(Serialize, Default)]
struct Step {
    /// Lines for the capture log.
    frames: Vec<FrameLine>,
    /// Bytes to write back to the instrument, in order.
    sends: Vec<Vec<u8>>,
    /// The session as the screen shows it.
    snapshot: Option<Snapshot>,
    /// Why the call could not be made at all (no cave open).
    error: Option<String>,
    /// False when the call was refused (the reason is in the snapshot's message).
    ok: bool,
    /// A shot is waiting for the rest of its indications: flush the link after this many ms.
    settle_ms: Option<u32>,
    /// The simulator has more shots to give.
    sim_left: bool,
    /// What the simulator is pretending, for the panel's note.
    sim_note: Option<String>,
}

/// Runs `f` on the session and the open cave, and answers with the step it filled in.
fn with_session(f: impl FnOnce(&mut Session, &mut WasmHost, &mut Step)) -> String {
    let mut step = Step {
        ok: true,
        ..Step::default()
    };
    DOC.with(|d| {
        let mut slot = d.borrow_mut();
        let Some(doc) = slot.as_mut() else {
            step.ok = false;
            step.error = Some("Open a cave first.".into());
            return;
        };
        let mut host = WasmHost {
            doc,
            ids: fresh_ids(),
        };
        SESSION.with(|s| {
            let mut session = s.borrow_mut();
            f(&mut session, &mut host, &mut step);
            step.snapshot = Some(session.snapshot(&host));
        });
        LINK.with(|l| {
            if let Some(Link::Bric(b)) = l.borrow().as_ref()
                && b.has_pending()
            {
                step.settle_ms = Some(bric::SETTLE_MS);
            }
        });
        SIM.with(|s| {
            if let Some(sim) = s.borrow().as_ref() {
                step.sim_left = sim.upcoming().is_some();
                if sim.has_pending() {
                    step.settle_ms = Some(bric::SETTLE_MS);
                }
            }
        });
    });
    serde_json::to_string(&step).unwrap_or_else(|_| "{}".into())
}

/// Sends what a link did where it belongs: the log, the instrument, the session.
fn route(outs: Vec<LinkOut>, session: &mut Session, host: &mut WasmHost, step: &mut Step) {
    for out in outs {
        match out {
            LinkOut::Frame { hex, from } => step.frames.push(FrameLine { hex, from }),
            LinkOut::Send { bytes } => step.sends.push(bytes),
            LinkOut::Reading { reading } => session.reading(host, reading),
        }
    }
}

/// Makes the session ready for this trip (an id): standing at the last station on its page.
#[wasm_bindgen]
pub fn survey_sync_trip(trip: &str) -> String {
    let Ok(trip) = trip.parse() else {
        return with_session(|_, _, step| step.ok = false);
    };
    with_session(|s, host, _| s.sync_to_trip(host, trip))
}

/// The session as the screen shows it.
#[wasm_bindgen]
pub fn survey_snapshot() -> String {
    with_session(|_, _, _| {})
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct SimChoice {
    device: Option<DeviceKind>,
    quirk: String,
}

/// Brings a link up. `kind` is `x1`, `sap6`, `sap5` or `bric` (the instrument has been told apart),
/// `capture` (frames only, nothing is decoded) or `sim`, a pretend instrument; for `sim`, `options` is JSON
/// `{device, quirk}`.
#[wasm_bindgen]
pub fn survey_connect(kind: &str, options: &str) -> String {
    let kind = kind.to_string();
    let choice: SimChoice = serde_json::from_str(options).unwrap_or_default();
    with_session(move |s, host, step| {
        LINK.with(|l| *l.borrow_mut() = None);
        SIM.with(|m| *m.borrow_mut() = None);
        let groups = kind != "capture";
        match kind.as_str() {
            "x1" => LINK.with(|l| *l.borrow_mut() = Some(Link::X1(X1Link::new("x1")))),
            "sap6" => LINK.with(|l| *l.borrow_mut() = Some(Link::Sap6(LegLink::new("sap6")))),
            "sap5" => LINK.with(|l| *l.borrow_mut() = Some(Link::Sap5(Sap5Link::new("sap5")))),
            "bric" => LINK.with(|l| *l.borrow_mut() = Some(Link::Bric(BricLink::new("bric")))),
            "sim" => {
                let device = choice.device.unwrap_or(DeviceKind::X1);
                // A fresh page starts with the splays at the first station; a survey already under way
                // carries on from where the surveyor stands.
                let fresh = s
                    .trip()
                    .and_then(|t| cave_notebook::page(host.doc(), t))
                    .is_none_or(|p| p.totals.shots == 0);
                let steps = traverse(&TraverseOpts {
                    seed: 7,
                    legs: 40,
                    backsights: true,
                    splays: 4,
                    start_splays: fresh,
                    lrud: true,
                });
                let quirks = if choice.quirk.is_empty() || choice.quirk == "none" {
                    vec![]
                } else {
                    vec![choice.quirk.clone()]
                };
                let dev = device_of(device);
                step.sim_note = Some(
                    match dev
                        .quirks
                        .iter()
                        .find(|q| q.id == choice.quirk && q.id != "none")
                    {
                        Some(q) => format!("{}, {}: {}", dev.label, q.label, q.expect),
                        None => dev.label.to_string(),
                    },
                );
                let mut sim = Simulator::new(SimOptions {
                    kind: device,
                    steps,
                    policy: FlagPolicy::Third,
                    quirks,
                });
                sim.connect();
                SIM.with(|m| *m.borrow_mut() = Some(sim));
            }
            _ => {}
        }
        s.connected(groups);
    })
}

/// A frame arrived from the instrument. `kind` is `frame` (an X1, SAP6 or SAP5 packet) or, for a BRIC,
/// `primary`, `meta` or `errors`.
#[wasm_bindgen]
pub fn survey_frame(kind: &str, bytes: &[u8]) -> String {
    let kind = kind.to_string();
    let bytes = bytes.to_vec();
    with_session(move |s, host, step| {
        let outs = LINK.with(|l| match l.borrow_mut().as_mut() {
            Some(Link::X1(x)) => x.on_frame(&bytes, "x1"),
            Some(Link::Sap6(x)) => x.on_frame(&bytes, "leg"),
            Some(Link::Sap5(x)) => x.on_frame(&bytes, "sap5"),
            Some(Link::Bric(x)) => BricKind::parse(&kind)
                .map(|k| x.on_frame(k, &bytes))
                .unwrap_or_default(),
            None => Vec::new(),
        });
        route(outs, s, host, step);
    })
}

/// The settling time of a BRIC shot has passed: write what has arrived of it.
#[wasm_bindgen]
pub fn survey_flush_link() -> String {
    with_session(|s, host, step| {
        let outs = LINK.with(|l| match l.borrow_mut().as_mut() {
            Some(Link::Bric(b)) => b.flush(),
            _ => Vec::new(),
        });
        let sim = SIM.with(|m| {
            m.borrow_mut()
                .as_mut()
                .map(Simulator::flush_pending)
                .unwrap_or_default()
        });
        route(outs, s, host, step);
        route(sim, s, host, step);
    })
}

/// Takes the link down: a BRIC shot still waiting for its errors is written first, and what the grouping
/// was waiting on is settled.
#[wasm_bindgen]
pub fn survey_disconnect() -> String {
    with_session(|s, host, step| {
        let outs = LINK.with(|l| match l.borrow_mut().take() {
            Some(Link::Bric(mut b)) => b.reset(),
            _ => Vec::new(),
        });
        let sim = SIM.with(|m| {
            m.borrow_mut()
                .take()
                .map(|mut sim| sim.disconnect())
                .unwrap_or_default()
        });
        route(outs, s, host, step);
        route(sim, s, host, step);
        s.disconnected(host);
    })
}

/// The link went down without being asked: whatever was held is dropped.
#[wasm_bindgen]
pub fn survey_link_down() -> String {
    with_session(|s, _, _| s.link_down())
}

/// The simulator shoots a whole station for one press: the leg, its backsight, the splays and the walls.
/// `walls` says whether the wall shots (LRUD) are taken.
#[wasm_bindgen]
pub fn survey_sim_station(walls: bool) -> String {
    with_session(move |s, host, step| {
        let backsights = s.state().backsights;
        let outs = SIM.with(|m| {
            m.borrow_mut()
                .as_mut()
                .map(|sim| sim.take_station(backsights, walls).1)
                .unwrap_or_default()
        });
        route(outs, s, host, step);
        s.settle_now(host); // the surveyor stops: nothing more is coming, so what is waiting is settled
    })
}

/// Settle what is waiting as splays now, without waiting for the pause.
#[wasm_bindgen]
pub fn survey_settle_now() -> String {
    with_session(|s, host, _| s.settle_now(host))
}

/// Writes the oldest held thing.
#[wasm_bindgen]
pub fn survey_accept() -> String {
    with_session(|s, host, _| s.accept(host))
}

/// Throws the oldest held thing away.
#[wasm_bindgen]
pub fn survey_discard() -> String {
    with_session(|s, _, _| s.discard())
}

/// Takes back the last thing written.
#[wasm_bindgen]
pub fn survey_undo_last() -> String {
    with_session(|s, host, step| step.ok = s.undo_last(host))
}

/// Chooses what the next shots are: `auto`, `leg`, `splay` or `lrud`.
#[wasm_bindgen]
pub fn survey_set_mode(mode: &str) -> String {
    let mode = match mode {
        "auto" => Mode::Auto,
        "splay" => Mode::Splay,
        "lrud" => Mode::Lrud,
        _ => Mode::Leg,
    };
    with_session(move |s, host, _| s.set_mode(host, mode))
}

/// Takes backsights after each leg, or not.
#[wasm_bindgen]
pub fn survey_set_backsights(on: bool) -> String {
    with_session(move |s, _, _| s.set_backsights(on))
}

/// Writes each thing at once, or holds it for Accept.
#[wasm_bindgen]
pub fn survey_set_auto(auto: bool) -> String {
    with_session(move |s, _, _| s.set_auto(auto))
}

/// Gives up the backsight being waited for.
#[wasm_bindgen]
pub fn survey_skip_backsight() -> String {
    with_session(|s, host, _| s.skip_backsight(host))
}

/// Moves on to the next station without a shot.
#[wasm_bindgen]
pub fn survey_next_station() -> String {
    with_session(|s, host, _| s.next_station(host))
}

/// The surveyor typed station names.
#[wasm_bindgen]
pub fn survey_set_names(from: &str, to: &str) -> String {
    let (from, to) = (from.to_string(), to.to_string());
    with_session(move |s, _, _| s.set_names(&from, &to))
}

/// Writes one passage dimension at the standing station (`side`: left, right, up or down; `value` metres, empty to clear).
#[wasm_bindgen]
pub fn survey_set_standing_wall(side: &str, value: &str) -> String {
    let side = match side {
        "left" => Side::Left,
        "right" => Side::Right,
        "up" => Side::Up,
        "down" => Side::Down,
        _ => return with_session(|_, _, step| step.ok = false),
    };
    let value = value.trim().parse::<f64>().ok();
    with_session(move |s, host, step| step.ok = s.set_standing_wall(host, side, value))
}

fn bytes_of(s: &str) -> Vec<u8> {
    s.bytes().map(|b| b & 0x7f).collect()
}

/// Everything the browser's Bluetooth transport needs to know about the instruments, and the simulator's
/// devices and their quirks, as JSON.
#[wasm_bindgen]
pub fn instrument_catalog() -> String {
    let catalog = serde_json::json!({
        "devices": devices(),
        "names": NAMES.iter().map(|n| serde_json::json!({"prefix": n.prefix, "kind": n.kind, "label": n.label})).collect::<Vec<_>>(),
        "x1": {"service": cavway::SERVICE, "write": cavway::WRITE, "notify": cavway::NOTIFY, "takeShot": cavway::command(cavway::CMD_TRIGGER)},
        "sap5": {"service": sap5::SERVICE, "write": sap5::WRITE, "notify": sap5::NOTIFY},
        "sap6": {"service": caveble::SERVICE, "name": caveble::NAME_CHARACTERISTIC, "command": caveble::COMMAND_CHARACTERISTIC, "leg": caveble::LEG_CHARACTERISTIC, "takeShot": [caveble::CMD_TAKE_SHOT]},
        "bric": {
            "measureService": bric::measure_service(), "primary": bric::primary(), "meta": bric::meta(), "errors": bric::errors(),
            "controlService": bric::control_service(), "control": bric::control(), "takeShot": bytes_of(bric::CMD_SHOT),
            "batteryService": "0000180f-0000-1000-8000-00805f9b34fb", "deviceInfoService": "0000180a-0000-1000-8000-00805f9b34fb",
            "settleMs": bric::SETTLE_MS,
        },
    });
    catalog.to_string()
}

/// Which instrument is this, from its advertised name and the services it offers? JSON `{kind, label, how}` or
/// `null`; the Nordic UART service alone does not say (`"nus"` is answered for it when asked via `has_nordic_uart`).
#[wasm_bindgen]
pub fn instrument_identify(name: &str, services: &str) -> String {
    let services: Vec<String> = serde_json::from_str(services).unwrap_or_default();
    let found = identify(name, &services);
    serde_json::json!({
        "identified": found,
        "nordicUart": cave_instrument::identify::has_nordic_uart(&services),
    })
    .to_string()
}

// ---- the sketchpad ----

/// The shots and passage dimensions of a trip's page, as the sketchpad's views are made of them (JSON), or `null`.
#[wasm_bindgen]
pub fn sketch_source(trip: &str) -> String {
    let Ok(trip) = trip.parse() else {
        return "null".into();
    };
    DOC.with(|d| {
        let src = d
            .borrow()
            .as_ref()
            .and_then(|doc| cave_notebook::page(doc, trip))
            .map(|p| cave_sketch::source(&p));
        serde_json::to_string(&src).unwrap_or_else(|_| "null".into())
    })
}

/// The sketchpad's views of a source (JSON): `{layout, plan, profile, section}`; `section` is for `station`
/// (empty for none).
#[wasm_bindgen]
pub fn sketch_views(source: &str, station: &str) -> String {
    let Ok(src) = serde_json::from_str::<cave_sketch::Source>(source) else {
        return "null".into();
    };
    let l = cave_sketch::layout(&src.shots);
    serde_json::json!({
        "layout": l,
        "plan": cave_sketch::plan_view(&l, &src.dims),
        "profile": cave_sketch::profile_view(&l, &src.dims),
        "section": (!station.is_empty()).then(|| cave_sketch::section_view(&l, &src.dims, station)).flatten(),
    })
    .to_string()
}

/// Hangs a stroke on the nearest station: `stations` is JSON `[{label, p}]`, `pts` JSON `[[x, y]]`, `widths`
/// JSON `[w]` or empty. Answers the stored stroke (JSON) or `null`.
#[wasm_bindgen]
pub fn sketch_anchor(stations: &str, pts: &str, kind: &str, widths: &str) -> String {
    let (Ok(stations), Ok(pts)) = (
        serde_json::from_str::<Vec<cave_sketch::Mark>>(stations),
        serde_json::from_str::<Vec<cave_sketch::Pt>>(pts),
    ) else {
        return "null".into();
    };
    let widths = serde_json::from_str::<Vec<f64>>(widths).ok();
    serde_json::to_string(&cave_sketch::anchor_stroke(&stations, &pts, kind, widths))
        .unwrap_or_else(|_| "null".into())
}

/// Where a stored stroke is now: the stroke's points in the view's metres (JSON), or `null` when its station is gone.
#[wasm_bindgen]
pub fn sketch_place(stations: &str, stroke: &str) -> String {
    let (Ok(stations), Ok(stroke)) = (
        serde_json::from_str::<Vec<cave_sketch::Mark>>(stations),
        serde_json::from_str::<cave_sketch::Anchored>(stroke),
    ) else {
        return "null".into();
    };
    serde_json::to_string(&cave_sketch::place_stroke(&stations, &stroke))
        .unwrap_or_else(|_| "null".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cave_doc::ActorId;
    use serde_json::Value;

    fn new_cave() -> String {
        let mut ids = cave_id::SeqIds::new(1);
        let doc = cave_notebook::new_cave(ActorId(1), &mut ids, "Test", "Trip 1", "m").unwrap();
        let trip = cave_notebook::trips(&doc)[0].id.to_string();
        crate::install(doc);
        trip
    }

    fn json(s: String) -> Value {
        serde_json::from_str(&s).unwrap()
    }

    #[test]
    fn nothing_works_without_a_cave() {
        crate::DOC.with(|d| *d.borrow_mut() = None);
        let v = json(survey_snapshot());
        assert_eq!(v["ok"], false);
        assert!(v["error"].as_str().unwrap().contains("Open a cave"));
    }

    #[test]
    fn a_simulated_survey_lands_in_the_notebook_and_the_sketch() {
        let trip = new_cave();
        let v = json(survey_sync_trip(&trip));
        assert_eq!(v["snapshot"]["state"]["from"], "A1");
        let v = json(survey_connect("sim", r#"{"device":"x1","quirk":"none"}"#));
        assert_eq!(v["snapshot"]["groups_legs"], true);
        assert_eq!(v["snapshot"]["state"]["mode"], "auto");
        assert!(v["sim_note"].as_str().unwrap().contains("Cavway X1"));
        json(survey_set_backsights(true));
        let mut v = Value::Null;
        for _ in 0..3 {
            v = json(survey_sim_station(true));
        }
        assert_eq!(v["snapshot"]["message"], "");
        assert_eq!(v["snapshot"]["state"]["from"], "A4");
        assert!(v["snapshot"]["log"].as_array().unwrap().len() > 10);
        assert_eq!(v["sim_left"], true);
        // The notebook has it, as ordinary undoable edits.
        let page = cave_wasm_page(&trip);
        assert!(page["totals"]["shots"].as_u64().unwrap() >= 3, "{page}");
        // The sketchpad reads it straight from the document.
        let src = crate::survey::sketch_source(&trip);
        let views = json(sketch_views(&src, "A3"));
        assert!(views["layout"]["chain"].as_array().unwrap().len() >= 4);
        assert!(
            !views["plan"]["walls"].as_array().unwrap().is_empty(),
            "the walls came from the passage dimensions"
        );
        assert!(views["section"]["outline"].is_array());
        // Undo takes the last thing back.
        let before = v["snapshot"]["log"].as_array().unwrap().len();
        let v = json(survey_undo_last());
        assert_eq!(v["ok"], true);
        assert_eq!(v["snapshot"]["log"].as_array().unwrap().len(), before - 1);
        json(survey_disconnect());
    }

    fn cave_wasm_page(trip: &str) -> Value {
        json(crate::notebook_page(trip))
    }

    #[test]
    fn a_real_packet_is_decoded_acknowledged_and_written() {
        let trip = new_cave();
        json(survey_sync_trip(&trip));
        json(survey_connect("x1", "{}"));
        let packet = cavway::encode_packet(&cavway::PacketSpec {
            distance: 6.5,
            azimuth: 120.0,
            inclination: -4.0,
            leg: true,
            time: 5,
            ..cavway::PacketSpec::default()
        });
        let v = json(survey_frame("frame", &packet));
        assert_eq!(
            v["sends"].as_array().unwrap().len(),
            1,
            "the packet is acknowledged"
        );
        assert!(v["frames"].as_array().unwrap().len() >= 1);
        // A flagged shot is a leg at once.
        assert_eq!(v["snapshot"]["log"][0]["label"], "A1 to A2");
        // The same packet again is acknowledged again but written once.
        let v = json(survey_frame("frame", &packet));
        assert_eq!(v["sends"].as_array().unwrap().len(), 1);
        assert_eq!(v["snapshot"]["log"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn a_bric_shot_waits_for_its_flush() {
        let trip = new_cave();
        json(survey_sync_trip(&trip));
        json(survey_connect("bric", "{}"));
        let (p, m, _) = bric::encode_shot(&bric::ShotSpec {
            distance: 5.0,
            azimuth: 100.0,
            inclination: 2.0,
            index: 1,
            dip: 60.0,
            time: bric::Time {
                year: 2026,
                month: 10,
                day: 14,
                ..Default::default()
            },
            ..Default::default()
        });
        let v = json(survey_frame("primary", &p));
        assert_eq!(
            v["settle_ms"],
            bric::SETTLE_MS,
            "the transport is asked to flush after the settling time"
        );
        json(survey_frame("meta", &m));
        let v = json(survey_flush_link());
        assert!(v["settle_ms"].is_null());
        assert_eq!(
            v["snapshot"]["waiting"], 1,
            "one shot so far: it may yet become a leg"
        );
    }

    #[test]
    fn the_catalog_names_every_instrument() {
        let c = json(instrument_catalog());
        for k in ["x1", "sap5", "sap6", "bric"] {
            assert!(c[k].is_object(), "{k}");
        }
        assert_eq!(c["devices"].as_array().unwrap().len(), 4);
        assert!(
            c["names"]
                .as_array()
                .unwrap()
                .iter()
                .any(|n| n["prefix"] == "CavwayX1-")
        );
        assert_eq!(
            c["bric"]["takeShot"],
            serde_json::json!([115, 104, 111, 116])
        );
        let id = json(instrument_identify("CavwayX1-0042", "[]"));
        assert_eq!(id["identified"]["kind"], "x1");
        let id = json(instrument_identify(
            "",
            r#"["6E400001-B5A3-F393-E0A9-E50E24DCCA9E"]"#,
        ));
        assert!(id["identified"].is_null() && id["nordicUart"] == true);
    }

    #[test]
    fn strokes_are_anchored_and_placed_through_json() {
        let stations = r#"[{"label":"A","p":[0,0]},{"label":"B","p":[10,0]}]"#;
        let a = sketch_anchor(stations, "[[9,1],[10,2]]", "wall", "[1,1.2]");
        let v = json(a.clone());
        assert_eq!(v["anchor"], "B");
        let moved = r#"[{"label":"A","p":[0,0]},{"label":"B","p":[20,5]}]"#;
        let placed = json(sketch_place(moved, &a));
        assert_eq!(placed[0], serde_json::json!([19.0, 6.0]));
        assert_eq!(sketch_place(r#"[{"label":"A","p":[0,0]}]"#, &a), "null");
    }
}
