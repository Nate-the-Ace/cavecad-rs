//! The live Survey session: ties what an instrument sends to the notebook. Readings come in (single shots
//! from a manual instrument, or the X1-style grouping done by [`LegAssembler`]); [`crate::rules`] decides
//! what each is; this writes it to the notebook as ONE undoable edit per shot, keeps the log, the held
//! readings and the walls waiting for a station to get its line, and says where the surveyor stands.
//!
//! Nothing here is stored in the cave: the session (where the surveyor stands, the mode, the log) is view
//! state rebuilt from the notebook page when a trip is opened. The notebook is reached only through
//! [`Host`], so the whole session runs natively in tests.

use crate::rules::{
    self, Decision, Mode, Plan, PlanKind, SessionState, advance, applied, decide, decide_event,
    next_step, skip_back, standing, start, with_mode,
};
use cave_doc::{Action, Document};
use cave_id::{Id, IdSource};
use cave_instrument::legs::{LegAssembler, LegEvent};
use cave_instrument::{Reading, Side};
use cave_notebook::{
    EditError, Measured, Reading as Cell, add_measured_shot, remove_shot, set_readings,
    set_station_wall, set_station_walls, station_walls, stations_in_use,
};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};

/// What the session needs of the open cave. The wasm layer fills it with the live document and its undo
/// history; tests fill it with a plain document.
pub trait Host {
    /// The open cave.
    fn doc(&self) -> &Document;
    /// The open cave and where new ids come from, together (the borrow checker will not lend them apart).
    fn doc_and_ids(&mut self) -> (&Document, &mut dyn IdSource);
    /// Applies `actions` as one undoable edit.
    fn commit(&mut self, actions: Vec<Action>) -> Result<(), String>;
}

/// Something to write: a single reading (the surveyor says what it is) or an event from the grouping.
#[derive(Clone, Debug, PartialEq)]
pub enum Incoming {
    /// One shot from a manual instrument.
    Single(Reading),
    /// What the leg grouping settled.
    Event(LegEvent),
}

/// Where a written thing is, so it can be taken back or refined.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// A shot on the page (a leg or splay it made, or the foresight a backsight completed).
    Shot(Id),
    /// A wall dimension of a station.
    Wall {
        /// The station.
        station: String,
        /// The wall.
        side: Side,
    },
}

/// What the session keeps about each thing written: enough to show it, to take it back, and to update it.
#[derive(Clone, Debug)]
pub struct Entry {
    /// A counter, for list keys.
    pub n: u32,
    /// What sort of thing.
    pub kind: PlanKind,
    /// Where it was written, as a surveyor would say it (`A3 to A4`, `splay at A4`, `backsight A4 to A3`).
    pub label: String,
    /// The reading written.
    pub reading: Reading,
    /// Where.
    pub target: Target,
    /// Worth a look.
    pub warnings: Vec<String>,
    /// Where the surveyor stood before it, so undoing it puts them back.
    pub before: SessionState,
    /// The instrument's group, for a leg that may still gain shots.
    pub group: Option<u32>,
}

/// An [`Entry`] as the screen shows it.
#[derive(Clone, Debug, Serialize)]
pub struct EntryView {
    /// List key.
    pub n: u32,
    /// What sort of thing.
    pub kind: PlanKind,
    /// Where it was written.
    pub label: String,
    /// The reading written.
    pub reading: Reading,
    /// Worth a look.
    pub warnings: Vec<String>,
}

/// A held thing as the screen shows it, for the Accept card.
#[derive(Clone, Debug, Serialize)]
pub struct HeldView {
    /// What accepting it would do.
    pub text: String,
    /// Worth a look.
    pub warnings: Vec<String>,
    /// Why it cannot be written at all.
    pub reject: Option<String>,
    /// The reading.
    pub reading: Option<Reading>,
}

/// The passage dimensions at the station the surveyor stands at.
#[derive(Clone, Debug, Serialize)]
pub struct StandingWalls {
    /// The station.
    pub station: String,
    /// Left, metres.
    pub left: Option<f64>,
    /// Right, metres.
    pub right: Option<f64>,
    /// Up, metres.
    pub up: Option<f64>,
    /// Down, metres.
    pub down: Option<f64>,
}

/// Everything the screen shows of the session.
#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    /// The trip this session was set up for (an id), if any.
    pub trip: Option<Id>,
    /// Where the surveyor stands and what they are doing.
    pub state: SessionState,
    /// What to do next, in plain words.
    pub instruction: String,
    /// The station the surveyor is physically at (the new one while a backsight is awaited).
    pub standing: String,
    /// Write each thing at once (default, as TopoDroid does) or hold it for an Accept.
    pub auto: bool,
    /// The instrument sends single shots that must be grouped (an X1).
    pub groups_legs: bool,
    /// Things waiting for Accept (only when `auto` is off), oldest first.
    pub held: Vec<HeldView>,
    /// Close shots the grouping is holding while it waits to see if they become a leg (0, 1 or 2).
    pub waiting: usize,
    /// The latest reading, written or not, for the big display.
    pub last: Option<Reading>,
    /// The last 50 things written, oldest first.
    pub log: Vec<EntryView>,
    /// Why the last thing was refused (or the last edit failed). Empty when all is well.
    pub message: String,
    /// Worth a look about the last thing written.
    pub warnings: Vec<String>,
    /// The passage dimensions at the standing station, once the page has a line for it.
    pub standing_walls: Option<StandingWalls>,
    /// How many edits the session has made to the cave: the screen refreshes when it grows.
    pub edits: u64,
}

fn describe(e: EditError) -> String {
    match e {
        EditError::NoSuchTrip => "There is no such trip.".into(),
        EditError::NoSuchShot => "That shot is no longer on the page.".into(),
        EditError::NoSuchStation => "There is no such station.".into(),
        EditError::Invalid(why) => {
            let mut s = why.to_string();
            if let Some(c) = s.get_mut(0..1) {
                c.make_ascii_uppercase();
            }
            s
        }
        EditError::NameTaken => "Another station already has that name.".into(),
    }
}

fn nb_side(s: Side) -> cave_notebook::Side {
    match s {
        Side::Left => cave_notebook::Side::Left,
        Side::Right => cave_notebook::Side::Right,
        Side::Up => cave_notebook::Side::Up,
        Side::Down => cave_notebook::Side::Down,
    }
}

fn side_index(s: Side) -> usize {
    match s {
        Side::Left => 0,
        Side::Right => 1,
        Side::Up => 2,
        Side::Down => 3,
    }
}

const SIDES: [Side; 4] = [Side::Left, Side::Right, Side::Up, Side::Down];

/// How many things the log keeps.
const LOG_LIMIT: usize = 50;

/// The live session.
pub struct Session {
    trip: Option<Id>,
    state: SessionState,
    auto: bool,
    groups_legs: bool,
    held: Vec<Incoming>,
    assembler: LegAssembler,
    last: Option<Reading>,
    log: Vec<Entry>,
    counter: u32,
    message: String,
    warnings: Vec<String>,
    /// Wall dimensions shot at a station that has no line on the page yet (before its first leg), by station
    /// name: written once it has one. `None` inside is untouched, `Some(None)` a clear.
    pending: BTreeMap<(Id, String), [Option<Option<f64>>; 4]>,
    edits: u64,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

struct Written {
    target: Target,
    label: String,
}

impl Session {
    /// A session with no trip yet.
    pub fn new() -> Self {
        Session {
            trip: None,
            state: start("", &HashSet::new(), None),
            auto: true,
            groups_legs: false,
            held: Vec::new(),
            assembler: LegAssembler::new(),
            last: None,
            log: Vec::new(),
            counter: 0,
            message: String::new(),
            warnings: Vec::new(),
            pending: BTreeMap::new(),
            edits: 0,
        }
    }

    /// Where the surveyor stands and what they are doing.
    pub fn state(&self) -> &SessionState {
        &self.state
    }

    /// The trip this session was set up for.
    pub fn trip(&self) -> Option<Id> {
        self.trip
    }

    /// Sets the session up for `trip`: standing at the last station on its page (or A1 on an empty one),
    /// keeping the surveyor's mode and backsight choice. Does nothing when it is already set up for it.
    pub fn sync_to_trip(&mut self, host: &dyn Host, trip: Id) {
        if self.trip == Some(trip) {
            return;
        }
        self.trip = Some(trip);
        let doc = host.doc();
        let last = cave_notebook::page(doc, trip).and_then(|p| {
            p.rows
                .into_iter()
                .rev()
                .find(|r| r.kind == cave_notebook::RowKind::Station)
                .map(|r| r.label)
        });
        self.state = start(
            last.as_deref().unwrap_or("A1"),
            &stations_in_use(doc),
            Some((self.state.mode, self.state.backsights)),
        );
        self.log.clear();
        self.held.clear();
        self.message.clear();
        self.warnings.clear();
        self.assembler.reset();
    }

    /// The surveyor typed new station names: where they stand and where the next leg goes.
    pub fn set_names(&mut self, from: &str, to: &str) {
        self.state.from = from.to_string();
        self.state.to = to.to_string();
    }

    /// Chooses between writing each thing at once and holding it for Accept.
    pub fn set_auto(&mut self, auto: bool) {
        self.auto = auto;
    }

    /// A link came up. An instrument that groups its own legs (an X1) says what each shot is; any other is
    /// told by the surveyor.
    pub fn connected(&mut self, groups_legs: bool) {
        self.pending.clear();
        self.groups_legs = groups_legs;
        self.state.mode = if groups_legs {
            Mode::Auto
        } else if self.state.mode == Mode::Auto {
            Mode::Leg
        } else {
            self.state.mode
        };
        self.assembler.reset();
    }

    /// The link went down (idle, error, unsupported): nothing held survives it.
    pub fn link_down(&mut self) {
        self.held.clear();
    }

    /// Disconnecting: what the grouping was waiting on is settled first.
    pub fn disconnected(&mut self, host: &mut dyn Host) {
        if self.groups_legs {
            self.settle_now(host);
        }
        self.held.clear();
    }

    fn fail(&mut self, m: &str) -> bool {
        if !m.is_empty() {
            self.message = m.to_string();
        }
        false
    }

    fn names(host: &dyn Host) -> HashSet<String> {
        stations_in_use(host.doc())
    }

    fn commit_actions(&mut self, host: &mut dyn Host, actions: Vec<Action>) -> Result<(), String> {
        if actions.is_empty() {
            return Ok(());
        }
        host.commit(actions)?;
        self.edits += 1;
        Ok(())
    }

    /// Sets one wall dimension of a station. At a station with no line yet it waits ([`Self::flush_walls`]).
    fn set_wall(
        &mut self,
        host: &mut dyn Host,
        trip: Id,
        station: &str,
        side: Side,
        value: Option<f64>,
    ) -> Result<(), String> {
        match set_station_wall(host.doc(), trip, station, nb_side(side), value).map_err(describe)? {
            Some(actions) => self.commit_actions(host, actions),
            None => {
                self.pending.entry((trip, station.to_string())).or_default()[side_index(side)] =
                    Some(value);
                Ok(())
            }
        }
    }

    /// Writes the dimensions that were waiting for a station to get its line, all of a station's in one edit.
    fn flush_walls(&mut self, host: &mut dyn Host, trip: Id) {
        let waiting: Vec<(String, [Option<Option<f64>>; 4])> = self
            .pending
            .iter()
            .filter(|((t, _), _)| *t == trip)
            .map(|((_, s), p)| (s.clone(), *p))
            .collect();
        for (station, patch) in waiting {
            let changes: Vec<(cave_notebook::Side, Option<f64>)> = SIDES
                .iter()
                .filter_map(|s| patch[side_index(*s)].map(|v| (nb_side(*s), v)))
                .collect();
            let Ok(Some(actions)) = set_station_walls(host.doc(), trip, &station, &changes) else {
                continue; // still no line for it
            };
            if self.commit_actions(host, actions).is_ok() {
                self.pending.remove(&(trip, station));
            }
        }
    }

    fn write(
        &mut self,
        host: &mut dyn Host,
        trip: Id,
        plan: &Plan,
        base: &SessionState,
    ) -> Result<Written, String> {
        match plan {
            Plan::Back { shot, reading } => {
                let actions = set_readings(
                    host.doc(),
                    *shot,
                    &[
                        (Cell::AzimuthBs, Some(reading.azimuth)),
                        (Cell::InclinationBs, Some(reading.inclination)),
                    ],
                )
                .map_err(describe)?;
                self.commit_actions(host, actions)?;
                Ok(Written {
                    target: Target::Shot(*shot),
                    label: format!("backsight {} to {}", base.to.trim(), base.from.trim()),
                })
            }
            Plan::Lrud {
                station,
                side,
                reading,
            } => {
                self.set_wall(host, trip, station, *side, Some(reading.distance))?;
                Ok(Written {
                    target: Target::Wall {
                        station: station.clone(),
                        side: *side,
                    },
                    label: format!("{} wall at {station}", side_name(*side)),
                })
            }
            Plan::Leg { from, to, reading } => self.add_shot(host, trip, from, to, false, reading),
            Plan::Splay { from, reading } => self.add_shot(host, trip, from, "", true, reading),
        }
    }

    fn add_shot(
        &mut self,
        host: &mut dyn Host,
        trip: Id,
        from: &str,
        to: &str,
        splay: bool,
        r: &Reading,
    ) -> Result<Written, String> {
        let measured = Measured {
            distance: Some(r.distance),
            azimuth: Some(r.azimuth),
            inclination: Some(r.inclination),
            ..Measured::default()
        };
        let (doc, ids) = host.doc_and_ids();
        let (shot, actions) =
            add_measured_shot(doc, ids, trip, from, to, splay, measured).map_err(describe)?;
        self.commit_actions(host, actions)?;
        Ok(Written {
            target: Target::Shot(shot),
            label: if splay {
                format!("splay at {from}")
            } else {
                format!("{from} to {to}")
            },
        })
    }

    /// Writes one thing to the notebook. Returns true when it was written (or an earlier leg was refined).
    pub fn commit(&mut self, host: &mut dyn Host, incoming: &Incoming) -> bool {
        let Some(trip) = self.trip else {
            return self.fail("Open a cave first.");
        };
        if let Incoming::Event(LegEvent::LegUpdate { group, reading, .. }) = incoming {
            return self.refine(host, trip, *group, reading);
        }
        let taken = Self::names(host);
        let (base, decision) = match incoming {
            Incoming::Single(r) => (self.state.clone(), decide(&self.state, r)),
            Incoming::Event(e) => decide_event(&self.state, e, &taken),
        };
        let Decision {
            plan,
            reject,
            warnings,
        } = decision;
        let Some(plan) = plan else {
            return self.fail(&reject.unwrap_or_else(|| "That reading cannot be used.".into()));
        };
        let written = match self.write(host, trip, &plan, &base) {
            Ok(w) => w,
            Err(e) => return self.fail(&e),
        };
        let group = match incoming {
            Incoming::Event(LegEvent::Leg { group, .. }) => Some(*group),
            _ => None,
        };
        self.counter += 1;
        let entry = Entry {
            n: self.counter,
            kind: plan.kind(),
            label: written.label,
            reading: plan.reading().clone(),
            target: written.target.clone(),
            warnings: warnings.clone(),
            before: self.state.clone(),
            group,
        };
        let shot = match &written.target {
            Target::Shot(id) => Some(*id),
            Target::Wall { .. } => None,
        };
        self.state = applied(&base, &plan, shot, &Self::names(host));
        self.log.push(entry);
        if self.log.len() > LOG_LIMIT {
            self.log.remove(0);
        }
        self.warnings = warnings;
        self.message.clear();
        self.flush_walls(host, trip);
        true
    }

    /// A leg gained a close shot: its average moved, so the readings already written are replaced.
    fn refine(&mut self, host: &mut dyn Host, trip: Id, group: u32, reading: &Reading) -> bool {
        let Some(i) = self.log.iter().position(|e| e.group == Some(group)) else {
            return false; // taken back since: nothing to update
        };
        let Target::Shot(shot) = self.log[i].target.clone() else {
            return false;
        };
        let cells = if self.log[i].kind == PlanKind::Back {
            vec![
                (Cell::AzimuthBs, Some(reading.azimuth)),
                (Cell::InclinationBs, Some(reading.inclination)),
            ]
        } else {
            vec![
                (Cell::Distance, Some(reading.distance)),
                (Cell::AzimuthFs, Some(reading.azimuth)),
                (Cell::InclinationFs, Some(reading.inclination)),
            ]
        };
        let result = set_readings(host.doc(), shot, &cells)
            .map_err(describe)
            .and_then(|a| self.commit_actions(host, a));
        if let Err(e) = result {
            return self.fail(&e);
        }
        self.log[i].reading = reading.clone();
        let same = |a: &Option<rules::Awaiting>| a.as_ref().is_some_and(|a| a.shot == shot);
        if same(&self.state.last)
            && let Some(l) = self.state.last.as_mut()
        {
            l.reading = reading.clone();
        }
        if same(&self.state.awaiting)
            && let Some(a) = self.state.awaiting.as_mut()
        {
            a.reading = reading.clone();
        }
        self.message.clear();
        self.flush_walls(host, trip);
        true
    }

    /// What to do with something that has arrived: write it, or hold it for Accept.
    fn handle(&mut self, host: &mut dyn Host, incoming: Incoming) {
        if self.auto {
            self.commit(host, &incoming);
            return;
        }
        if let Incoming::Event(LegEvent::LegUpdate {
            group,
            reading,
            shots,
            mark,
            spread,
            flagged,
        }) = &incoming
        {
            // A held leg gained a shot: the held one is replaced. One already written is refined without asking.
            let held = self.held.iter().position(
                |h| matches!(h, Incoming::Event(LegEvent::Leg { group: g, .. }) if g == group),
            );
            match held {
                Some(i) => {
                    self.held[i] = Incoming::Event(LegEvent::Leg {
                        group: *group,
                        reading: reading.clone(),
                        shots: *shots,
                        mark: *mark,
                        spread: *spread,
                        flagged: *flagged,
                    })
                }
                None => {
                    self.commit(host, &incoming);
                }
            }
            return;
        }
        self.held.push(incoming);
        self.message.clear();
    }

    /// A shot arrived from the instrument.
    pub fn reading(&mut self, host: &mut dyn Host, reading: Reading) {
        self.last = Some(reading.clone());
        if !self.groups_legs {
            self.handle(host, Incoming::Single(reading));
            return;
        }
        for e in self.assembler.push(reading) {
            self.handle(host, Incoming::Event(e));
        }
    }

    /// Settle what is waiting as splays now, without waiting for the pause (the surveyor means them to be splays).
    pub fn settle_now(&mut self, host: &mut dyn Host) {
        for e in self.assembler.flush() {
            self.handle(host, Incoming::Event(e));
        }
    }

    /// Writes the oldest held thing (the Accept button).
    pub fn accept(&mut self, host: &mut dyn Host) {
        if self.held.is_empty() {
            return;
        }
        let next = self.held.remove(0);
        self.commit(host, &next);
    }

    /// Throws the oldest held thing away (the Discard button).
    pub fn discard(&mut self) {
        if !self.held.is_empty() {
            self.held.remove(0);
        }
    }

    /// Takes back the last thing written: the shot is removed (or its backsight cleared) and the surveyor
    /// stands where they did.
    pub fn undo_last(&mut self, host: &mut dyn Host) -> bool {
        let Some(entry) = self.log.last().cloned() else {
            return false;
        };
        let trip = self.trip;
        let result: Result<(), String> = match &entry.target {
            Target::Wall { station, side } => match trip {
                Some(t) => self.set_wall(host, t, station, *side, None),
                None => Err("Open a cave first.".into()),
            },
            Target::Shot(shot) if entry.kind == PlanKind::Back => set_readings(
                host.doc(),
                *shot,
                &[(Cell::AzimuthBs, None), (Cell::InclinationBs, None)],
            )
            .map_err(describe)
            .and_then(|a| self.commit_actions(host, a)),
            Target::Shot(shot) => remove_shot(host.doc(), *shot)
                .map_err(describe)
                .and_then(|a| self.commit_actions(host, a)),
        };
        if let Err(e) = result {
            return self.fail(&e);
        }
        self.log.pop();
        self.state = entry.before;
        self.message.clear();
        self.warnings.clear();
        // So that shooting the same leg again right away is a new leg, not a refinement of the one just taken back.
        self.assembler.reset();
        true
    }

    /// Switches between the modes.
    pub fn set_mode(&mut self, host: &dyn Host, mode: Mode) {
        self.state = with_mode(&self.state, mode, &Self::names(host));
    }

    /// Takes backsights, or not.
    pub fn set_backsights(&mut self, on: bool) {
        self.state.backsights = on;
        if !on {
            self.state.awaiting = None;
        }
    }

    /// Gives up the backsight being waited for and moves on to the next station.
    pub fn skip_backsight(&mut self, host: &dyn Host) {
        self.state = skip_back(&self.state, &Self::names(host));
    }

    /// Moves on to the next station without a shot (a station reached some other way).
    pub fn next_station(&mut self, host: &dyn Host) {
        let to = if self.state.to.is_empty() {
            self.state.from.clone()
        } else {
            self.state.to.clone()
        };
        self.state = advance(
            &SessionState {
                to,
                ..self.state.clone()
            },
            &Self::names(host),
        );
    }

    /// The passage dimensions at the standing station, or `None` before the page has a line for it.
    pub fn standing_walls(&self, host: &dyn Host) -> Option<StandingWalls> {
        let trip = self.trip?;
        let station = standing(&self.state);
        let w = station_walls(host.doc(), trip, &station)?;
        Some(StandingWalls {
            station,
            left: w.left,
            right: w.right,
            up: w.up,
            down: w.down,
        })
    }

    /// Writes one dimension at the standing station.
    pub fn set_standing_wall(
        &mut self,
        host: &mut dyn Host,
        side: Side,
        value: Option<f64>,
    ) -> bool {
        let Some(trip) = self.trip else {
            return self.fail("Open a cave first.");
        };
        let station = standing(&self.state);
        let planned = match set_station_wall(host.doc(), trip, &station, nb_side(side), value) {
            Ok(Some(a)) => a,
            Ok(None) => {
                return self.fail("This station has no line on the page yet: take its shot first.");
            }
            Err(e) => return self.fail(&describe(e)),
        };
        if let Err(e) = self.commit_actions(host, planned) {
            return self.fail(&e);
        }
        self.message.clear();
        true
    }

    /// What a held thing would do when accepted, for the preview card.
    fn preview(&self, host: &dyn Host, incoming: &Incoming) -> HeldView {
        let taken = Self::names(host);
        let decision = match incoming {
            Incoming::Single(r) => decide(&self.state, r),
            Incoming::Event(LegEvent::LegUpdate { .. }) => {
                return HeldView {
                    text: "a refined leg".into(),
                    warnings: Vec::new(),
                    reject: None,
                    reading: None,
                };
            }
            Incoming::Event(e) => decide_event(&self.state, e, &taken).1,
        };
        let text = match &decision.plan {
            None => String::new(),
            Some(Plan::Leg { from, to, .. }) => format!("{from} to {to}"),
            Some(Plan::Splay { from, .. }) => format!("a splay at {from}"),
            Some(Plan::Lrud { side, station, .. }) => {
                format!("the {} wall at {station}", side_name(*side))
            }
            Some(Plan::Back { .. }) => "the backsight".into(),
        };
        let reading = match incoming {
            Incoming::Single(r) => Some(r.clone()),
            Incoming::Event(
                LegEvent::Leg { reading, .. }
                | LegEvent::Splay { reading }
                | LegEvent::Lrud { reading }
                | LegEvent::LegUpdate { reading, .. },
            ) => Some(reading.clone()),
        };
        HeldView {
            text,
            warnings: decision.warnings,
            reject: decision.reject,
            reading,
        }
    }

    /// Everything the screen shows.
    pub fn snapshot(&self, host: &dyn Host) -> Snapshot {
        Snapshot {
            trip: self.trip,
            state: self.state.clone(),
            instruction: next_step(&self.state),
            standing: standing(&self.state),
            auto: self.auto,
            groups_legs: self.groups_legs,
            held: self.held.iter().map(|h| self.preview(host, h)).collect(),
            waiting: self.assembler.waiting(),
            last: self.last.clone(),
            log: self
                .log
                .iter()
                .map(|e| EntryView {
                    n: e.n,
                    kind: e.kind,
                    label: e.label.clone(),
                    reading: e.reading.clone(),
                    warnings: e.warnings.clone(),
                })
                .collect(),
            message: self.message.clone(),
            warnings: self.warnings.clone(),
            standing_walls: self.standing_walls(host),
            edits: self.edits,
        }
    }
}

fn side_name(s: Side) -> &'static str {
    match s {
        Side::Left => "left",
        Side::Right => "right",
        Side::Up => "up",
        Side::Down => "down",
    }
}
