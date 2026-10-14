//! The Survey session's rules, with no screen and no engine in them: given what the surveyor has set (leg or
//! splay, backsights or not, where they stand) and a reading from the instrument, what should be written to
//! the notebook, and where do they stand afterwards.
//!
//! What the instrument sends is never changed on the way in: the compass reading stays what the compass said
//! (the notebook turns it into a true bearing with the trip's declination), a backsight keeps its own sign
//! and direction, and a steep shot keeps whatever bearing the instrument gave (the engine already ignores the
//! compass on a pitch, so nothing here invents or drops one).

use cave_id::Id;
use cave_instrument::legs::LegEvent;
use cave_instrument::{Mark, Reading, Side, angle_gap, mod360};
use cave_notebook::suggest_next;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// `Auto`: the instrument says what each shot is (an X1: three close shots are a leg). `Leg`, `Splay` and
/// `Lrud`: the surveyor says (`Lrud`: each shot measures a wall of the station).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// The instrument decides.
    Auto,
    /// Every shot is a leg.
    Leg,
    /// Every shot is a splay.
    Splay,
    /// Every shot is a wall dimension.
    Lrud,
}

/// A foresight has been written and its backsight is the next reading.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Awaiting {
    /// The foresight's shot.
    pub shot: Id,
    /// The foresight, to compare the backsight against.
    pub reading: Reading,
}

/// Where the surveyor stands and what they are doing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionState {
    /// How readings are told apart.
    pub mode: Mode,
    /// Take a backsight after each foresight.
    pub backsights: bool,
    /// The station the surveyor stands at (a leg starts here, a splay is taken from here).
    pub from: String,
    /// The station the next leg goes to.
    pub to: String,
    /// The foresight whose backsight is awaited.
    pub awaiting: Option<Awaiting>,
    /// The last leg written: what a backsight marked on the instrument belongs to when none is awaited.
    pub last: Option<Awaiting>,
}

/// What to write to the notebook.
#[derive(Clone, Debug, PartialEq)]
pub enum Plan {
    /// A leg between two stations.
    Leg {
        /// Where it starts.
        from: String,
        /// Where it ends.
        to: String,
        /// The shot.
        reading: Reading,
    },
    /// A splay from a station.
    Splay {
        /// Where it starts.
        from: String,
        /// The shot.
        reading: Reading,
    },
    /// A wall dimension of `station`: the shot's length, on the side it was aimed.
    Lrud {
        /// The station.
        station: String,
        /// The wall.
        side: Side,
        /// The shot.
        reading: Reading,
    },
    /// The backsight of an earlier foresight.
    Back {
        /// The foresight's shot.
        shot: Id,
        /// The backsight.
        reading: Reading,
    },
}

impl Plan {
    /// The reading the plan writes.
    pub fn reading(&self) -> &Reading {
        match self {
            Plan::Leg { reading, .. }
            | Plan::Splay { reading, .. }
            | Plan::Lrud { reading, .. }
            | Plan::Back { reading, .. } => reading,
        }
    }

    /// Which sort of plan, as the log names it.
    pub fn kind(&self) -> PlanKind {
        match self {
            Plan::Leg { .. } => PlanKind::Leg,
            Plan::Splay { .. } => PlanKind::Splay,
            Plan::Lrud { .. } => PlanKind::Lrud,
            Plan::Back { .. } => PlanKind::Back,
        }
    }
}

/// The sort of a [`Plan`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlanKind {
    /// A leg.
    Leg,
    /// A splay.
    Splay,
    /// A wall dimension.
    Lrud,
    /// A backsight.
    Back,
}

/// What [`decide`] makes of a reading.
#[derive(Clone, Debug, PartialEq)]
pub struct Decision {
    /// What to write, if anything.
    pub plan: Option<Plan>,
    /// Why the reading cannot be used at all (nothing is written).
    pub reject: Option<String>,
    /// Worth a look, but written anyway.
    pub warnings: Vec<String>,
}

impl Decision {
    fn reject(why: impl Into<String>, warnings: Vec<String>) -> Self {
        Decision {
            plan: None,
            reject: Some(why.into()),
            warnings,
        }
    }

    fn plan(plan: Plan, warnings: Vec<String>) -> Self {
        Decision {
            plan: Some(plan),
            reject: None,
            warnings,
        }
    }
}

/// Longest shot taken as believable, metres. A laser reading past this is a glitch.
pub const MAX_DISTANCE_M: f64 = 600.0;
/// Shots shorter than this are probably a mis-fire, metres.
pub const MIN_DISTANCE_M: f64 = 0.05;
/// The engine reads a shot steeper than this as a plumbed pitch and ignores its compass.
pub const PLUMB_DEG: f64 = 85.0;
/// Degrees by which foresight and backsight may disagree (the same figure the notebook flags at).
pub const BACKSIGHT_TOLERANCE_DEG: f64 = 3.0;
/// Tape difference between foresight and backsight that is worth saying: larger of 0.1 m and 2 %.
pub const TAPE_TOLERANCE_M: f64 = 0.1;
/// See [`TAPE_TOLERANCE_M`].
pub const TAPE_TOLERANCE_FRACTION: f64 = 0.02;
/// How far a leg may be from exactly reversed to be taken as the backsight of the leg before it, degrees
/// (compass and clino each).
pub const BACK_FIND_DEG: f64 = 25.0;
/// How far from square on a shot may be and still count as aimed at the left or right wall, degrees either side.
pub const WALL_AIM_DEG: f64 = 45.0;
/// Spread among the shots of a leg beyond which it is worth a word, metres.
const SPREAD_DISTANCE_M: f64 = 0.03;
/// Spread among the shots of a leg beyond which it is worth a word, degrees.
const SPREAD_ANGLE_DEG: f64 = 1.5;

/// A number written the way JavaScript's `toFixed` does: a tie rounds up, not to even.
pub fn to_fixed(v: f64, digits: usize) -> String {
    let exact = format!("{:.*}", digits + 40, v.abs());
    let (whole, frac) = exact.split_once('.').unwrap_or((&exact, ""));
    let keep = &frac[..digits];
    let round_up = frac.as_bytes()[digits] >= b'5';
    let mut text: Vec<u8> = format!("{whole}{keep}").into_bytes();
    if round_up {
        let mut i = text.len();
        loop {
            if i == 0 {
                text.insert(0, b'1');
                break;
            }
            i -= 1;
            if text[i] == b'9' {
                text[i] = b'0';
            } else {
                text[i] += 1;
                break;
            }
        }
    }
    let split = text.len() - digits;
    let (w, f) = text.split_at(split);
    let (w, f) = (String::from_utf8_lossy(w), String::from_utf8_lossy(f));
    let sign = if v < 0.0 && text.iter().any(|c| *c != b'0') {
        "-"
    } else {
        ""
    };
    if digits == 0 {
        format!("{sign}{w}")
    } else {
        format!("{sign}{w}.{f}")
    }
}

/// A reading that can be written as it is (with any warnings), or the reason it cannot. A shot steeper than
/// the plumb limit is flagged for a leg (the engine reads it as a pitch and ignores the compass), but not for
/// a splay: up and down splays are how a passage is measured, and say nothing the surveyor needs told.
pub fn check_reading(r: &Reading, splay: bool) -> Result<Vec<String>, String> {
    if ![r.distance, r.azimuth, r.inclination]
        .iter()
        .all(|v| v.is_finite())
    {
        return Err("The instrument sent something that is not a number.".into());
    }
    if r.distance <= 0.0 {
        return Err("The tape reading is zero or negative.".into());
    }
    if r.distance > MAX_DISTANCE_M {
        return Err("The tape reading is far too long to be right.".into());
    }
    if !(-90.0..=90.0).contains(&r.inclination) {
        return Err("The clino reading is past vertical.".into());
    }
    let mut warnings = Vec::new();
    if r.distance < MIN_DISTANCE_M {
        warnings.push("Very short shot: check it was not a mis-fire.".into());
    }
    if !splay && r.inclination.abs() > PLUMB_DEG {
        warnings.push(format!(
            "Steeper than {PLUMB_DEG}\u{b0}: read as a pitch, so the compass is ignored."
        ));
    }
    Ok(warnings)
}

/// What a backsight says about its foresight: the same checks the notebook flags a page with.
pub fn backsight_warnings(fore: &Reading, back: &Reading) -> Vec<String> {
    let mut out = Vec::new();
    // Steep shots have no meaningful compass, so only the clino is compared.
    if fore.inclination.abs() <= PLUMB_DEG && back.inclination.abs() <= PLUMB_DEG {
        let g = angle_gap(fore.azimuth, mod360(back.azimuth + 180.0));
        if g > BACKSIGHT_TOLERANCE_DEG {
            out.push(format!(
                "Compass foresight and backsight differ by {}\u{b0}.",
                to_fixed(g, 1)
            ));
        }
    }
    let sum = (fore.inclination + back.inclination).abs();
    if sum > BACKSIGHT_TOLERANCE_DEG {
        out.push(format!(
            "Clino foresight and backsight differ by {}\u{b0}.",
            to_fixed(sum, 1)
        ));
    }
    let d = (fore.distance - back.distance).abs();
    if d > TAPE_TOLERANCE_M.max(TAPE_TOLERANCE_FRACTION * fore.distance) {
        out.push(format!(
            "Tape differs by {} m between foresight and backsight.",
            to_fixed(d, 2)
        ));
    }
    out
}

/// True when `r` looks like the backsight of `fore`: it points back along it (the compass about 180 degrees
/// round, the clino about opposite) and is about as long. Only the X1 can mark a leg as a backsight on the
/// instrument; the others (SAP5, SAP6, BRIC) cannot, so after a leg with backsights switched on the
/// surveyor's next leg is taken as its backsight if it comes back, and as the next leg if it does not.
pub fn looks_like_back(fore: &Reading, r: &Reading) -> bool {
    let steep = fore.inclination.abs() > PLUMB_DEG && r.inclination.abs() > PLUMB_DEG;
    if !steep && angle_gap(fore.azimuth, mod360(r.azimuth + 180.0)) > BACK_FIND_DEG {
        return false;
    }
    if (fore.inclination + r.inclination).abs() > BACK_FIND_DEG {
        return false;
    }
    (fore.distance - r.distance).abs() <= 1.0f64.max(0.25 * fore.distance)
}

/// Which wall a shot measures, from the way the passage runs (`heading`, the compass of the last leg): up or
/// down when it points well up or down, else left or right of the heading. `None` when it points along the
/// passage, or when there is no heading yet to say left from right.
pub fn side_of(r: &Reading, heading: Option<f64>) -> Option<Side> {
    if r.inclination >= WALL_AIM_DEG {
        return Some(Side::Up);
    }
    if r.inclination <= -WALL_AIM_DEG {
        return Some(Side::Down);
    }
    let rel = mod360(r.azimuth - heading?);
    if (rel - 90.0).abs() <= WALL_AIM_DEG {
        return Some(Side::Right);
    }
    if (rel - 270.0).abs() <= WALL_AIM_DEG {
        return Some(Side::Left);
    }
    None
}

const NO_SIDE: &str = "That shot points along the passage, not at a wall. Aim left, right, up or down (and shoot a leg first, so there is a direction to say left from).";

/// An LRUD shot at `station`: the side it says, or the side its aim gives.
fn lrud_plan(
    station: &str,
    reading: &Reading,
    heading: Option<f64>,
    warnings: Vec<String>,
) -> Decision {
    let Some(side) = reading.lrud.or_else(|| side_of(reading, heading)) else {
        return Decision::reject(NO_SIDE, warnings);
    };
    if station.is_empty() {
        return Decision::reject("Say which station you are standing at.", warnings);
    }
    Decision::plan(
        Plan::Lrud {
            station: station.into(),
            side,
            reading: reading.clone(),
        },
        warnings,
    )
}

/// Decides what a reading is: the next leg, a splay, a wall dimension, or the backsight being waited for.
pub fn decide(state: &SessionState, reading: &Reading) -> Decision {
    let lrud = state.mode == Mode::Lrud || reading.lrud.is_some();
    let splay = (state.mode == Mode::Splay && state.awaiting.is_none()) || lrud;
    let mut warnings = match check_reading(reading, splay) {
        Ok(w) => w,
        Err(why) => return Decision::reject(why, Vec::new()),
    };
    if lrud {
        let heading = state.last.as_ref().map(|l| l.reading.azimuth);
        return lrud_plan(&standing(state), reading, heading, warnings);
    }
    if let Some(a) = &state.awaiting {
        warnings.extend(backsight_warnings(&a.reading, reading));
        return Decision::plan(
            Plan::Back {
                shot: a.shot,
                reading: reading.clone(),
            },
            warnings,
        );
    }
    let (from, to) = (state.from.trim(), state.to.trim());
    if from.is_empty() {
        return Decision::reject("Say which station you are standing at.", warnings);
    }
    if state.mode == Mode::Splay {
        return Decision::plan(
            Plan::Splay {
                from: from.into(),
                reading: reading.clone(),
            },
            warnings,
        );
    }
    if to.is_empty() {
        return Decision::reject("Name the station this shot goes to.", warnings);
    }
    if from == to {
        return Decision::reject("A shot joins two different stations.", warnings);
    }
    Decision::plan(
        Plan::Leg {
            from: from.into(),
            to: to.into(),
            reading: reading.clone(),
        },
        warnings,
    )
}

/// The surveyor moves on: they stand at `to`, and the next leg goes to the next name in the sequence.
pub fn advance(state: &SessionState, taken: &HashSet<String>) -> SessionState {
    let stand = state.to.trim().to_string();
    let mut names = taken.clone();
    names.insert(stand.clone());
    SessionState {
        from: stand.clone(),
        to: suggest_next(&stand, &names),
        awaiting: None,
        ..state.clone()
    }
}

/// The state once `plan` has been written to the notebook (`shot` is the id of the shot it created or changed).
pub fn applied(
    state: &SessionState,
    plan: &Plan,
    shot: Option<Id>,
    taken: &HashSet<String>,
) -> SessionState {
    match plan {
        Plan::Leg { reading, .. } => {
            let made = shot.map(|shot| Awaiting {
                shot,
                reading: reading.clone(),
            });
            // With backsights the surveyor has not moved yet: the backsight is taken from the new station
            // looking back. They move on once it is in (or skipped).
            if state.backsights {
                SessionState {
                    awaiting: made.clone(),
                    last: made,
                    ..state.clone()
                }
            } else {
                SessionState {
                    last: made,
                    ..advance(state, taken)
                }
            }
        }
        // A backsight that was awaited moves them on; one marked on the instrument for a leg already left behind does not.
        Plan::Back { .. } if state.awaiting.is_some() => advance(state, taken),
        Plan::Back { .. } | Plan::Splay { .. } | Plan::Lrud { .. } => state.clone(),
    }
}

/// Gives up on the backsight being waited for and moves on.
pub fn skip_back(state: &SessionState, taken: &HashSet<String>) -> SessionState {
    if state.awaiting.is_some() {
        advance(state, taken)
    } else {
        state.clone()
    }
}

/// Switches mode. A surveyor who changes mode while a backsight is awaited has given it up and is already at
/// the new station, so they move on (a splay taken now belongs to that station).
pub fn with_mode(state: &SessionState, mode: Mode, taken: &HashSet<String>) -> SessionState {
    if state.mode == mode {
        return state.clone();
    }
    let base = if state.awaiting.is_some() {
        advance(state, taken)
    } else {
        state.clone()
    };
    SessionState { mode, ..base }
}

/// The station the surveyor is physically at: the new one while its backsight is awaited.
pub fn standing(state: &SessionState) -> String {
    (if state.awaiting.is_some() {
        &state.to
    } else {
        &state.from
    })
    .trim()
    .to_string()
}

/// What to tell the surveyor to do next, in plain words.
pub fn next_step(state: &SessionState) -> String {
    let (from, to) = (state.from.trim(), state.to.trim());
    if state.mode == Mode::Lrud {
        let at = standing(state);
        return format!(
            "Shoot the walls at {}: left, right, up, down. Each shot is a dimension of the station, not a splay.",
            if at.is_empty() {
                "(name the station)"
            } else {
                &at
            }
        );
    }
    if state.mode == Mode::Auto {
        if state.awaiting.is_some() {
            return format!(
                "Backsight: shoot back from {to} to {from} (three shots; an X1 can mark them Backsight with M). Or carry on without."
            );
        }
        if from.is_empty() || to.is_empty() {
            return "Name the two stations, then shoot the leg: three close shots.".into();
        }
        return format!(
            "Shoot {from} to {to}: three close shots make a leg. A single shot is a splay at {from}."
        );
    }
    if state.awaiting.is_some() {
        return format!("Take the backsight from {to} back to {from}.");
    }
    if state.mode == Mode::Splay {
        return format!(
            "Take a splay from {}.",
            if from.is_empty() {
                "(name the station)"
            } else {
                from
            }
        );
    }
    if from.is_empty() || to.is_empty() {
        return "Name the two stations, then take the shot.".into();
    }
    format!("Take the shot {from} to {to}.")
}

/// A fresh state standing at `from` (the last station of the page, or the first of a new one), keeping the
/// surveyor's mode and backsight choice when given.
pub fn start(from: &str, taken: &HashSet<String>, keep: Option<(Mode, bool)>) -> SessionState {
    let mut names = taken.clone();
    names.insert(from.to_string());
    SessionState {
        mode: keep.map_or(Mode::Leg, |k| k.0),
        backsights: keep.is_some_and(|k| k.1),
        from: from.into(),
        to: suggest_next(from, &names),
        awaiting: None,
        last: None,
    }
}

/// The instrument's own events (what the X1 says is a leg or a splay), made into a plan. `base` is where the
/// surveyor stands when it is written (they have moved on if a backsight was awaited and something else came).
/// `event` must be a leg, splay or lrud event; a leg update is handled by the caller.
pub fn decide_event(
    state: &SessionState,
    event: &LegEvent,
    taken: &HashSet<String>,
) -> (SessionState, Decision) {
    let (reading, is_leg, spread, mark) = match event {
        LegEvent::Leg {
            reading,
            spread,
            mark,
            ..
        } => (reading, true, Some(spread), *mark),
        LegEvent::Splay { reading } | LegEvent::Lrud { reading } => {
            (reading, false, None, Mark::None)
        }
        LegEvent::LegUpdate { .. } => {
            return (
                state.clone(),
                Decision::reject("A refined leg is not a new shot.", Vec::new()),
            );
        }
    };
    let as_wall = matches!(event, LegEvent::Lrud { .. })
        || (matches!(event, LegEvent::Splay { .. }) && state.mode == Mode::Lrud);
    let mut warnings = match check_reading(reading, !is_leg || as_wall) {
        Ok(w) => w,
        Err(why) => return (state.clone(), Decision::reject(why, Vec::new())),
    };
    if let Some(e) = reading.error.as_deref().filter(|e| !e.is_empty()) {
        warnings.push(format!("The instrument reports an {e}."));
    }
    let moved = |s: &SessionState| {
        if s.awaiting.is_some() {
            advance(s, taken)
        } else {
            s.clone()
        }
    };
    if as_wall {
        let base = moved(state);
        let heading = base.last.as_ref().map(|l| l.reading.azimuth);
        let d = lrud_plan(base.from.trim(), reading, heading, warnings);
        return (base, d);
    }
    if !is_leg {
        let base = moved(state);
        let from = base.from.trim().to_string();
        if from.is_empty() {
            return (
                base,
                Decision::reject("Say which station you are standing at.", warnings),
            );
        }
        return (
            base,
            Decision::plan(
                Plan::Splay {
                    from,
                    reading: reading.clone(),
                },
                warnings,
            ),
        );
    }
    if let Some(spread) = spread
        && (spread.distance > SPREAD_DISTANCE_M
            || spread.azimuth > SPREAD_ANGLE_DEG
            || spread.inclination > SPREAD_ANGLE_DEG)
    {
        warnings.push(format!(
            "The shots of this leg differ by {} m, {}\u{b0} compass, {}\u{b0} clino.",
            to_fixed(spread.distance, 3),
            to_fixed(spread.azimuth, 1),
            to_fixed(spread.inclination, 1)
        ));
    }
    if mark == Mark::Backsight {
        let Some(target) = state.awaiting.as_ref().or(state.last.as_ref()) else {
            return (
                state.clone(),
                Decision::reject("A backsight needs a leg before it.", warnings),
            );
        };
        warnings.extend(backsight_warnings(&target.reading, reading));
        return (
            state.clone(),
            Decision::plan(
                Plan::Back {
                    shot: target.shot,
                    reading: reading.clone(),
                },
                warnings,
            ),
        );
    }
    // An instrument that cannot mark a backsight: a leg that comes back along the one before is its backsight.
    if let Some(a) = &state.awaiting
        && looks_like_back(&a.reading, reading)
    {
        warnings.extend(backsight_warnings(&a.reading, reading));
        return (
            state.clone(),
            Decision::plan(
                Plan::Back {
                    shot: a.shot,
                    reading: reading.clone(),
                },
                warnings,
            ),
        );
    }
    let base = moved(state);
    let (from, to) = (base.from.trim().to_string(), base.to.trim().to_string());
    let refuse = |base: SessionState, why: &str, warnings: Vec<String>| {
        (base, Decision::reject(why, warnings))
    };
    if from.is_empty() {
        return refuse(base, "Say which station you are standing at.", warnings);
    }
    if to.is_empty() {
        return refuse(base, "Name the station this leg goes to.", warnings);
    }
    if from == to {
        return refuse(base, "A leg joins two different stations.", warnings);
    }
    (
        base,
        Decision::plan(
            Plan::Leg {
                from,
                to,
                reading: reading.clone(),
            },
            warnings,
        ),
    )
}
