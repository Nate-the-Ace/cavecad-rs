//! Live checks for a sweep while it is being taken: has it covered what it was meant to? Two kinds of sweep:
//!
//! - **LRUD ring** ([`Kind::Lrud`]): one slow ring across the passage at a station, for the cross-section and the
//!   station's left, right, up and down. Checked: enough shots, something on each of the four walls, no big gap
//!   round the ring, and most shots across the passage (not along it).
//! - **Room sweep** ([`Kind::Room`]): the laser held about level and turned right round, for the outline of a room
//!   or a wide passage. Checked: enough level shots, every compass direction reached, no big gap.
//!
//! Two **standards** set how much is asked for. **Digital** (the default) is for surveying with the instrument doing
//! the detail: the full checks above. **Manual** is for a survey that is mostly sketched by hand: an LRUD ring needs
//! only one shot on each of the four walls (the cardinal directions), and a room sweep a rough eight-way outline.
//!
//! The answer says, in plain words, what is still missing ("Nothing on the ceiling yet"), so the surveyor can sweep
//! there before moving on. Pure geometry: vectors from the station in metres (x east, y north, z up), as
//! [`crate::splay_vector`] makes them. Bearings stay in whatever frame the readings were (magnetic on the
//! instrument), which is what the surveyor sees on its screen.

use crate::{Style, V, cross, dot, len, ring, scale, sub, unit};
use serde::{Deserialize, Serialize};

/// What a sweep is meant to capture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A ring across the passage: the cross-section and LRUD at the station.
    #[default]
    Lrud,
    /// About level, all the way round: the outline of a room.
    Room,
}

impl Kind {
    /// From the word the screen uses.
    pub fn parse(s: &str) -> Option<Kind> {
        match s {
            "lrud" => Some(Kind::Lrud),
            "room" => Some(Kind::Room),
            _ => None,
        }
    }
}

/// How much a sweep is asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Standard {
    /// Mostly sketched by hand: the four walls for LRUD, a rough outline for a room.
    Manual,
    /// The instrument captures the detail: dense rings and full room outlines.
    #[default]
    Digital,
}

impl Standard {
    /// From the word the screen uses.
    pub fn parse(s: &str) -> Option<Standard> {
        match s {
            "manual" => Some(Standard::Manual),
            "digital" => Some(Standard::Digital),
            _ => None,
        }
    }

    /// The thresholds this standard checks against.
    pub fn rules(self) -> Rules {
        match self {
            Standard::Digital => Rules {
                lrud_min_shots: LRUD_MIN_SHOTS,
                lrud_min_per_wall: LRUD_MIN_PER_WALL,
                lrud_max_gap: LRUD_MAX_GAP,
                room_min_shots: ROOM_MIN_SHOTS,
                room_max_gap: ROOM_MAX_GAP,
                by_count: false,
            },
            Standard::Manual => Rules {
                lrud_min_shots: 4,
                lrud_min_per_wall: 1,
                lrud_max_gap: 360.0, // no gap check: the four walls are all that is wanted
                room_min_shots: 8,
                room_max_gap: 90.0,
                by_count: true,
            },
        }
    }
}

/// The thresholds a sweep is checked against (see [`Standard::rules`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rules {
    /// Fewest usable shots for an LRUD ring.
    pub lrud_min_shots: usize,
    /// Fewest shots on each of the four walls.
    pub lrud_min_per_wall: usize,
    /// The widest gap round the ring that still passes, degrees.
    pub lrud_max_gap: f64,
    /// Fewest level shots for a room sweep.
    pub room_min_shots: usize,
    /// The widest gap round a room that still passes, degrees.
    pub room_max_gap: f64,
    /// Ask for shots by count ("2 more shots wanted"). Off for Digital: there the shots are continuous, and what is
    /// missing is said as a place and a coverage percentage.
    pub by_count: bool,
}

/// Sectors round the dial the screen draws (30 degrees each).
pub const SECTORS: usize = 12;
/// Fewest usable shots for an LRUD ring.
pub const LRUD_MIN_SHOTS: usize = 6;
/// Fewest shots on each of the four walls of an LRUD ring.
pub const LRUD_MIN_PER_WALL: usize = 1;
/// The widest gap round an LRUD ring that still passes, degrees.
pub const LRUD_MAX_GAP: f64 = 60.0;
/// Fewest usable (level) shots for a room sweep.
pub const ROOM_MIN_SHOTS: usize = 8;
/// The widest gap round a room sweep that still passes, degrees.
pub const ROOM_MAX_GAP: f64 = 30.0;
/// How far up or down a room-sweep shot may point and still count as level, degrees.
pub const ROOM_LEVEL: f64 = 20.0;
/// How far along the passage an LRUD shot may point and still count as across it, degrees.
pub const LRUD_ACROSS: f64 = 30.0;

/// How a sweep stands so far.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Check {
    /// What it is meant to capture.
    pub kind: Kind,
    /// How much is asked for.
    pub standard: Standard,
    /// Shots taken.
    pub shots: usize,
    /// Shots that count (across the passage for a ring, level for a room; misses never count).
    pub usable: usize,
    /// The fewest usable shots wanted.
    pub wanted: usize,
    /// Shots in each of [`SECTORS`] sectors round the dial. LRUD: looking along the passage, sector 0 starts at the
    /// right wall and they go anticlockwise (up, left, down). Room: sector 0 starts at north, going clockwise.
    pub sectors: Vec<usize>,
    /// The widest gap with no shot, degrees (360 with nothing yet).
    pub widest_gap: f64,
    /// How much of the way round is covered, percent: every gap wider than the standard allows counts as uncovered
    /// beyond that width. 100 exactly when no gap is too wide. What a Digital sweep shows instead of a shot count:
    /// to the surveyor its shots are continuous.
    pub coverage: f64,
    /// All the checks pass.
    pub complete: bool,
    /// What is still missing, in plain words, most important first. Empty when complete.
    pub todo: Vec<String>,
    /// Worth knowing, but not missing (shots that did not count, and why).
    pub notes: Vec<String>,
    /// LRUD ring: left, right, up and down read off the ring so far (metres), once there is enough for one.
    pub lrud: Option<[f64; 4]>,
    /// LRUD: the next wall still wanted, in the order a surveyor calls them (left, right, up, down), to aim at.
    pub aim: Option<Aim>,
}

/// A wall to aim the next shot at.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Aim {
    /// In words: "the left wall", "the ceiling".
    pub wall: String,
    /// Where it is round the ring, degrees (0 right wall, 90 ceiling, 180 left wall, 270 floor, looking along the
    /// passage).
    pub angle: f64,
}

/// Percent of the way round with no gap wider than `allowed` (degrees) between shots.
fn coverage(angles: &[f64], allowed: f64) -> f64 {
    if angles.is_empty() {
        return 0.0;
    }
    let mut a = angles.to_vec();
    a.sort_by(f64::total_cmp);
    let mut missing = (a[0] + 360.0 - a[a.len() - 1] - allowed).max(0.0);
    for w in a.windows(2) {
        missing += (w[1] - w[0] - allowed).max(0.0);
    }
    ((360.0 - missing) / 360.0 * 100.0).clamp(0.0, 100.0)
}

fn widest_gap(angles: &mut [f64]) -> f64 {
    if angles.is_empty() {
        return 360.0;
    }
    angles.sort_by(f64::total_cmp);
    let mut gap = angles[0] + 360.0 - angles[angles.len() - 1];
    for w in angles.windows(2) {
        gap = gap.max(w[1] - w[0]);
    }
    gap
}

/// Where the widest gap is: its middle, degrees on the dial.
fn gap_middle(angles: &[f64]) -> Option<f64> {
    if angles.len() < 2 {
        return None;
    }
    let mut best = (
        angles[0] + 360.0 - angles[angles.len() - 1],
        angles[angles.len() - 1],
    );
    for w in angles.windows(2) {
        if w[1] - w[0] > best.0 {
            best = (w[1] - w[0], w[0]);
        }
    }
    Some((best.1 + best.0 / 2.0).rem_euclid(360.0))
}

fn sector_of(deg: f64) -> usize {
    ((deg.rem_euclid(360.0) / (360.0 / SECTORS as f64)).floor() as usize).min(SECTORS - 1)
}

/// The compass point nearest a bearing, as a word.
pub fn compass_word(deg: f64) -> &'static str {
    const NAMES: [&str; 8] = [
        "north",
        "north-east",
        "east",
        "south-east",
        "south",
        "south-west",
        "west",
        "north-west",
    ];
    NAMES[((deg.rem_euclid(360.0) + 22.5) / 45.0).floor() as usize % 8]
}

/// A place round an LRUD ring (0 = right wall, anticlockwise looking along the passage), as words.
fn ring_word(deg: f64) -> &'static str {
    const NAMES: [&str; 8] = [
        "the right wall",
        "up to the right",
        "the ceiling",
        "up to the left",
        "the left wall",
        "down to the left",
        "the floor",
        "down to the right",
    ];
    NAMES[((deg.rem_euclid(360.0) + 22.5) / 45.0).floor() as usize % 8]
}

/// Checks a sweep so far. `splays` are the shots (vectors from the station, metres), `passage` which way the
/// passage runs here (from the legs), if known.
pub fn check(kind: Kind, standard: Standard, splays: &[V], passage: Option<V>) -> Check {
    let rules = standard.rules();
    let mut c = match kind {
        Kind::Lrud => check_lrud(splays, passage, rules),
        Kind::Room => check_room(splays, rules),
    };
    c.standard = standard;
    c
}

fn hits(splays: &[V]) -> Vec<V> {
    splays
        .iter()
        .copied()
        .filter(|p| p.iter().all(|x| x.is_finite()) && len(*p) >= 0.05)
        .collect()
}

fn check_room(splays: &[V], rules: Rules) -> Check {
    let want = rules.room_min_shots;
    let pts = hits(splays);
    let mut level: Vec<f64> = Vec::new();
    let mut off = 0;
    for p in &pts {
        let flat = p[0].hypot(p[1]);
        let incl = p[2].atan2(flat).to_degrees();
        if incl.abs() <= ROOM_LEVEL && flat > 0.02 {
            level.push(p[0].atan2(p[1]).to_degrees().rem_euclid(360.0));
        } else {
            off += 1;
        }
    }
    let mut sectors = vec![0; SECTORS];
    for a in &level {
        sectors[sector_of(*a)] += 1;
    }
    let mut sorted = level.clone();
    let gap = widest_gap(&mut sorted);
    let covered = coverage(&level, rules.room_max_gap.min(360.0));
    let enough = level.len() >= rules.room_min_shots;
    let mut todo = Vec::new();
    if rules.by_count && level.len() < rules.room_min_shots {
        todo.push(format!(
            "{} more level shot{} wanted ({} of {want}).",
            rules.room_min_shots - level.len(),
            if rules.room_min_shots - level.len() == 1 {
                ""
            } else {
                "s"
            },
            level.len()
        ));
    }
    if level.is_empty() {
        todo.push("Hold the laser about level and turn right round.".into());
    } else if gap > rules.room_max_gap {
        let towards = gap_middle(&sorted).map(compass_word).unwrap_or("round");
        todo.push(format!(
            "Nothing towards the {towards} yet ({gap:.0}° gap): sweep there."
        ));
    }
    let mut notes = Vec::new();
    let missed = splays.len() - pts.len();
    if missed > 0 {
        notes.push(format!(
            "{missed} shot{} with no return, not counted.",
            plural(missed)
        ));
    }
    if off > 0 {
        notes.push(format!(
            "{off} shot{} more than {ROOM_LEVEL:.0}° up or down, not counted: keep the laser level.",
            plural(off)
        ));
    }
    Check {
        kind: Kind::Room,
        standard: Standard::Digital,
        shots: splays.len(),
        usable: level.len(),
        wanted: rules.room_min_shots,
        sectors,
        widest_gap: gap,
        coverage: covered,
        complete: todo.is_empty() && enough,
        todo,
        notes,
        lrud: None,
        aim: None,
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

fn check_lrud(splays: &[V], passage: Option<V>, rules: Rules) -> Check {
    let want = rules.lrud_min_shots;
    let pts = hits(splays);
    // which way the passage runs: the ring's own plane once there is one, else the legs, else the sweep's spread
    let made = ring(&pts, passage);
    let axis = made
        .as_ref()
        .filter(|r| r.style == Style::Ring)
        .map(|r| r.axis)
        .or_else(|| passage.and_then(unit))
        .or_else(|| (pts.len() >= 3).then(|| crate::principal(&pts).1[0]));
    let mut todo = Vec::new();
    let mut notes = Vec::new();
    let mut sectors = vec![0; SECTORS];
    let mut across: Vec<f64> = Vec::new();
    let mut along = 0;
    if let Some(mut axis) = axis {
        let reference = passage.and_then(unit).unwrap_or([0.0, 1.0, 0.0]);
        if dot(axis, reference) < 0.0 {
            axis = scale(axis, -1.0);
        }
        let z = [0.0, 0.0, 1.0];
        let up = unit(sub(z, scale(axis, dot(z, axis))))
            .or_else(|| unit(sub([0.0, 1.0, 0.0], scale(axis, axis[1]))));
        if let Some(up) = up {
            let right = cross(axis, up);
            let limit = LRUD_ACROSS.to_radians().sin();
            for p in &pts {
                let l = len(*p);
                if (dot(*p, axis) / l).abs() > limit {
                    along += 1;
                    continue;
                }
                let a = dot(*p, up)
                    .atan2(dot(*p, right))
                    .to_degrees()
                    .rem_euclid(360.0);
                across.push(a);
                sectors[sector_of(a)] += 1;
            }
        }
    }
    let usable = across.len();
    if rules.by_count && usable < rules.lrud_min_shots {
        todo.push(format!(
            "{} more shot{} across the passage wanted ({usable} of {want}).",
            rules.lrud_min_shots - usable,
            plural(rules.lrud_min_shots - usable)
        ));
    }
    // the four walls: right 315-45, up 45-135, left 135-225, down 225-315
    let wall = |lo: f64| {
        across
            .iter()
            .filter(|a| (*a - lo).rem_euclid(360.0) < 90.0)
            .count()
    };
    let mut aim = None;
    for (lo, name, wall_name) in [
        (135.0, "Nothing on the left wall yet.", "the left wall"),
        (315.0, "Nothing on the right wall yet.", "the right wall"),
        (45.0, "Nothing on the ceiling yet.", "the ceiling"),
        (225.0, "Nothing on the floor yet.", "the floor"),
    ] {
        let n = wall(lo);
        if n < rules.lrud_min_per_wall && aim.is_none() {
            aim = Some(Aim {
                wall: wall_name.into(),
                angle: (lo + 45.0) % 360.0,
            });
        }
        if n == 0 {
            todo.push(name.into());
        } else if n < rules.lrud_min_per_wall {
            todo.push(
                name.replace("Nothing on", "Only one shot on")
                    .replace(" yet.", ": one more."),
            );
        }
    }
    let mut sorted = across.clone();
    let gap = widest_gap(&mut sorted);
    let covered = coverage(&across, rules.lrud_max_gap.min(360.0));
    let enough = usable >= rules.lrud_min_shots;
    if usable >= 2 && gap > rules.lrud_max_gap {
        let at = gap_middle(&sorted)
            .map(ring_word)
            .unwrap_or("round the ring");
        // name it only when the wall checks have not already said so
        let said = todo.iter().any(|t| {
            (at == "the ceiling" && t.contains("ceiling"))
                || (at == "the floor" && t.contains("floor"))
                || (at == "the left wall" && t.contains("left wall"))
                || (at == "the right wall" && t.contains("right wall"))
        });
        if !said {
            todo.push(format!("A gap at {at} ({gap:.0}°): sweep there."));
        }
    }
    let missed = splays.len() - pts.len();
    if missed > 0 {
        notes.push(format!(
            "{missed} shot{} with no return, not counted.",
            plural(missed)
        ));
    }
    if along > 0 {
        notes.push(format!(
            "{along} shot{} along the passage, not across it: not counted for the ring.",
            plural(along)
        ));
    }
    Check {
        kind: Kind::Lrud,
        standard: Standard::Digital,
        shots: splays.len(),
        usable,
        wanted: rules.lrud_min_shots,
        sectors,
        widest_gap: gap,
        coverage: covered,
        complete: todo.is_empty() && enough,
        todo,
        notes,
        lrud: made.map(|r| r.lrud),
        aim,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::splay_vector;

    /// A ring across a north-running passage: `n` shots round an ellipse (half-width `w`, half-height `h`),
    /// skipping angles (degrees, 0 = east wall, anticlockwise looking north) in `skip`.
    fn ring_shots(n: usize, w: f64, h: f64, skip: Option<(f64, f64)>) -> Vec<V> {
        (0..n)
            .filter_map(|i| {
                let a = i as f64 / n as f64 * 360.0;
                if let Some((lo, hi)) = skip
                    && a >= lo
                    && a <= hi
                {
                    return None;
                }
                let t = a.to_radians();
                Some([w * t.cos(), 0.05 * (i % 3) as f64, h * t.sin()])
            })
            .collect()
    }

    #[test]
    fn a_full_ring_passes_with_lrud() {
        let c = check(
            Kind::Lrud,
            Standard::Digital,
            &ring_shots(24, 2.0, 1.0, None),
            Some([0.0, 1.0, 0.0]),
        );
        assert!(c.complete, "{:?}", c.todo);
        assert_eq!(c.usable, 24);
        let [l, r, u, d] = c.lrud.expect("lrud");
        assert!((l - 2.0).abs() < 0.3 && (r - 2.0).abs() < 0.3, "{l} {r}");
        assert!((u - 1.0).abs() < 0.3 && (d - 1.0).abs() < 0.3, "{u} {d}");
    }

    #[test]
    fn a_ring_missing_the_ceiling_says_so() {
        let c = check(
            Kind::Lrud,
            Standard::Digital,
            &ring_shots(30, 2.0, 1.0, Some((40.0, 140.0))),
            Some([0.0, 1.0, 0.0]),
        );
        assert!(!c.complete);
        assert_eq!(c.todo, vec!["Nothing on the ceiling yet.".to_string()]);
    }

    #[test]
    fn a_ring_with_a_side_gap_names_it() {
        // gap down to the right (about 290-340): the right wall and floor still have shots
        let c = check(
            Kind::Lrud,
            Standard::Digital,
            &ring_shots(36, 2.0, 1.0, Some((280.0, 345.0))),
            Some([0.0, 1.0, 0.0]),
        );
        assert!(!c.complete);
        assert!(
            c.todo
                .iter()
                .any(|t| t.starts_with("A gap at down to the right")),
            "{:?}",
            c.todo
        );
    }

    #[test]
    fn too_few_shots_asks_for_more() {
        let s = [[2.0, 0.0, 0.0], [-1.8, 0.0, 0.0]];
        let c = check(Kind::Lrud, Standard::Manual, &s, Some([0.0, 1.0, 0.0]));
        assert!(!c.complete);
        assert_eq!(
            c.todo[0],
            "2 more shots across the passage wanted (2 of 4)."
        );
    }

    #[test]
    fn coverage_is_a_percentage_round() {
        let none = check(Kind::Lrud, Standard::Digital, &[], None);
        assert_eq!(none.coverage, 0.0);
        let full = check(
            Kind::Lrud,
            Standard::Digital,
            &ring_shots(24, 2.0, 1.0, None),
            Some([0.0, 1.0, 0.0]),
        );
        assert_eq!(full.coverage, 100.0);
        // the ceiling missing (40-140 skipped on a 2 x 1 m ellipse): one wide gap, about a quarter uncovered
        let part = check(
            Kind::Lrud,
            Standard::Digital,
            &ring_shots(36, 2.0, 1.0, Some((40.0, 140.0))),
            Some([0.0, 1.0, 0.0]),
        );
        assert!(
            part.coverage > 70.0 && part.coverage < 90.0,
            "{}",
            part.coverage
        );
        assert!(!part.complete);
    }

    #[test]
    fn shots_along_the_passage_do_not_count() {
        let mut s = ring_shots(16, 2.0, 1.0, None);
        s.push(splay_vector(5.0, 0.0, 0.0)); // straight up the passage
        s.push(splay_vector(5.0, 180.0, 5.0)); // straight back
        let c = check(Kind::Lrud, Standard::Digital, &s, Some([0.0, 1.0, 0.0]));
        assert_eq!(c.usable, 16);
        assert!(
            c.notes
                .iter()
                .any(|n| n.starts_with("2 shots along the passage"))
        );
        assert!(c.complete, "{:?}", c.todo);
    }

    #[test]
    fn nothing_yet() {
        let c = check(Kind::Lrud, Standard::Digital, &[], None);
        assert!(!c.complete);
        assert_eq!(c.widest_gap, 360.0);
        assert_eq!(c.usable, 0);
        let r = check(Kind::Room, Standard::Digital, &[], None);
        assert!(!r.complete);
        assert!(r.todo.iter().any(|t| t.contains("level")));
    }

    #[test]
    fn a_full_room_sweep_passes() {
        let s: Vec<V> = (0..36)
            .map(|i| splay_vector(6.0, 5.0 + i as f64 * 10.0, 3.0))
            .collect();
        let c = check(Kind::Room, Standard::Digital, &s, None);
        assert!(c.complete, "{:?}", c.todo);
        assert!(c.sectors.iter().all(|&n| n == 3));
    }

    #[test]
    fn a_room_sweep_with_a_gap_names_the_direction() {
        // nothing between 100 and 170: the gap is towards the south-east
        let s: Vec<V> = (0..36)
            .map(|i| i as f64 * 10.0)
            .filter(|a| !(100.0..=170.0).contains(a))
            .map(|a| splay_vector(6.0, a, 0.0))
            .collect();
        let c = check(Kind::Room, Standard::Digital, &s, None);
        assert!(!c.complete);
        assert!(
            c.todo
                .iter()
                .any(|t| t.starts_with("Nothing towards the south-east")),
            "{:?}",
            c.todo
        );
    }

    #[test]
    fn steep_room_shots_are_noted_not_counted() {
        let mut s: Vec<V> = (0..30)
            .map(|i| splay_vector(6.0, i as f64 * 12.0, 0.0))
            .collect();
        s.push(splay_vector(3.0, 0.0, 60.0));
        s.push(splay_vector(3.0, 0.0, -45.0));
        s.push(splay_vector(0.0, 0.0, 0.0)); // a miss
        let c = check(Kind::Room, Standard::Digital, &s, None);
        assert_eq!(c.usable, 30);
        assert_eq!(c.shots, 33);
        assert!(c.complete);
        assert_eq!(c.notes.len(), 2, "{:?}", c.notes);
    }

    #[test]
    fn manual_wants_only_the_four_walls() {
        // one shot on each wall, nothing between: Digital wants more, Manual is done
        let s = [
            [2.0, 0.0, 0.0],
            [0.0, 0.0, 1.2],
            [-1.8, 0.0, 0.0],
            [0.0, 0.0, -0.9],
        ];
        let d = check(Kind::Lrud, Standard::Digital, &s, Some([0.0, 1.0, 0.0]));
        assert!(!d.complete);
        let m = check(Kind::Lrud, Standard::Manual, &s, Some([0.0, 1.0, 0.0]));
        assert!(m.complete, "{:?}", m.todo);
        assert_eq!(m.standard, Standard::Manual);
        assert_eq!(m.wanted, 4);
        // still asks for a missing wall
        let m = check(Kind::Lrud, Standard::Manual, &s[..3], Some([0.0, 1.0, 0.0]));
        assert!(m.todo.iter().any(|t| t.contains("floor")), "{:?}", m.todo);
        assert_eq!(
            m.aim.as_ref().map(|a| (a.wall.as_str(), a.angle)),
            Some(("the floor", 270.0))
        );
        // nothing yet: the left wall first, as LRUD is called
        let none = check(Kind::Lrud, Standard::Manual, &[], None);
        assert_eq!(none.aim.unwrap().wall, "the left wall");
    }

    #[test]
    fn manual_room_is_a_rough_outline() {
        let s: Vec<V> = (0..8)
            .map(|i| splay_vector(6.0, 10.0 + i as f64 * 45.0, 0.0))
            .collect();
        assert!(!check(Kind::Room, Standard::Digital, &s, None).complete);
        assert!(check(Kind::Room, Standard::Manual, &s, None).complete);
    }

    #[test]
    fn compass_words() {
        assert_eq!(compass_word(0.0), "north");
        assert_eq!(compass_word(359.0), "north");
        assert_eq!(compass_word(135.0), "south-east");
        assert_eq!(compass_word(-90.0), "west");
    }
}
