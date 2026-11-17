//! Guided calibration: follows an instrument's calibration shots (the X1 sends each one as it is taken) and says
//! which directions are covered and where to point next.
//!
//! The X1 does the calibration itself; this only guides the surveyor, the way the X1's own screen does with its two
//! circles, but in words ("level, facing east") and with a list. It works like the X1's grouping (manual Rev. C):
//! shots in about the same direction, the instrument rolled 90 degrees between them, make a group; four complete it
//! (more: the last four count); a shot pointing elsewhere starts a new group, and an unfinished group before it is
//! dropped.
//!
//! Coverage: fourteen target directions spread evenly round the sphere (the six axis directions and the eight
//! between them), the "at least 14 groups covering all directions" the manual asks for. A finished group within
//! [`COVER_DEG`] of a target covers it. Directions are the instrument's own (magnetic azimuth, degrees, clino up
//! positive); an uncalibrated instrument is a few degrees out, which is far inside these tolerances.

use serde::Serialize;

/// A shot this far (degrees) from the current group's direction starts a new group.
pub const GROUP_DEG: f64 = 8.0;
/// Shots that make a group.
pub const GROUP_SHOTS: usize = 4;
/// A finished group this close (degrees) to a target covers it.
pub const COVER_DEG: f64 = 30.0;
/// Groups the X1 needs before it can compute.
pub const MIN_GROUPS: usize = 8;
/// Groups the manual asks for, for a good calibration.
pub const GOOD_GROUPS: usize = 14;
/// The X1 takes at most this many groups.
pub const MAX_GROUPS: usize = 25;

/// A target direction, in the order a surveyor is asked for them.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Target {
    /// Where to point, in words: "level, facing north", "35° up, facing south-east", "straight up".
    pub label: String,
    /// Azimuth, degrees (meaningless straight up or down).
    pub azimuth: f64,
    /// Clino, degrees, up positive.
    pub inclination: f64,
    /// A finished group covers it.
    pub covered: bool,
}

/// A finished group.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Group {
    /// Its direction (the mean of its shots): azimuth, degrees.
    pub azimuth: f64,
    /// Clino, degrees, up positive.
    pub inclination: f64,
}

/// The guide as the screen shows it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GuideView {
    /// The fourteen directions to cover, in the order they are asked for.
    pub targets: Vec<Target>,
    /// The finished groups.
    pub groups: Vec<Group>,
    /// Shots in the group being taken (0 to 4).
    pub current_shots: usize,
    /// The direction of the group being taken, once it has a shot.
    pub current: Option<Group>,
    /// The next target not yet covered, if any.
    pub next: Option<Target>,
    /// Targets covered.
    pub covered: usize,
    /// Enough groups for the X1 to compute (8).
    pub can_compute: bool,
    /// Every target covered and 14 groups or more: compute on the X1 (M), then apply (M).
    pub done: bool,
    /// What to do now, in words.
    pub say: String,
}

fn unit(az: f64, inc: f64) -> [f64; 3] {
    let (a, i) = (az.to_radians(), inc.to_radians());
    [i.cos() * a.sin(), i.cos() * a.cos(), i.sin()]
}

fn angle(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] * b[0] + a[1] * b[1] + a[2] * b[2])
        .clamp(-1.0, 1.0)
        .acos()
        .to_degrees()
}

fn direction(v: [f64; 3]) -> Group {
    let flat = v[0].hypot(v[1]);
    Group {
        azimuth: v[0].atan2(v[1]).to_degrees().rem_euclid(360.0),
        inclination: v[2].atan2(flat).to_degrees(),
    }
}

const COMPASS: [&str; 8] = [
    "north",
    "north-east",
    "east",
    "south-east",
    "south",
    "south-west",
    "west",
    "north-west",
];

/// The fourteen targets, in the order they are asked for: level all round, then up between, then down between,
/// then straight up and straight down.
pub fn targets() -> Vec<Target> {
    // the eight between the axes sit at 35.26 degrees (a cube's corners): call it 35
    let mut t = Vec::new();
    for k in [0, 2, 4, 6] {
        t.push(target(
            format!("level, facing {}", COMPASS[k]),
            k as f64 * 45.0,
            0.0,
        ));
    }
    for k in [1, 3, 5, 7] {
        t.push(target(
            format!("35° up, facing {}", COMPASS[k]),
            k as f64 * 45.0,
            35.26,
        ));
    }
    for k in [1, 3, 5, 7] {
        t.push(target(
            format!("35° down, facing {}", COMPASS[k]),
            k as f64 * 45.0,
            -35.26,
        ));
    }
    t.push(target("straight up".into(), 0.0, 90.0));
    t.push(target("straight down".into(), 0.0, -90.0));
    t
}

fn target(label: String, azimuth: f64, inclination: f64) -> Target {
    Target {
        label,
        azimuth,
        inclination,
        covered: false,
    }
}

/// Follows the calibration shots.
#[derive(Clone, Debug, Default)]
pub struct Guide {
    groups: Vec<[f64; 3]>,
    current: Vec<[f64; 3]>,
}

impl Guide {
    /// A guide with nothing shot yet.
    pub fn new() -> Self {
        Guide::default()
    }

    /// Starts again (a new calibration on the X1).
    pub fn reset(&mut self) {
        *self = Guide::default();
    }

    /// The group being taken was reset on the X1 (its > button): its shots are forgotten.
    pub fn reset_group(&mut self) {
        self.current.clear();
    }

    /// A calibration shot: its azimuth and clino (degrees).
    pub fn shot(&mut self, azimuth: f64, inclination: f64) {
        let v = unit(azimuth, inclination);
        if !v.iter().all(|x| x.is_finite()) {
            return;
        }
        if let Some(mean) = self.current_mean()
            && angle(mean, v) > GROUP_DEG
        {
            // pointing elsewhere: a finished group is kept, an unfinished one dropped (as the X1 does)
            self.close();
            self.current.clear();
        }
        self.current.push(v);
        if self.current.len() > GROUP_SHOTS {
            self.current.remove(0); // the last four count
        }
    }

    fn current_mean(&self) -> Option<[f64; 3]> {
        if self.current.is_empty() {
            return None;
        }
        let s = self
            .current
            .iter()
            .fold([0.0; 3], |a, v| [a[0] + v[0], a[1] + v[1], a[2] + v[2]]);
        let n = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt();
        (n > 1e-9).then(|| [s[0] / n, s[1] / n, s[2] / n])
    }

    /// Keeps the group being taken if it is complete.
    fn close(&mut self) {
        if self.current.len() >= GROUP_SHOTS
            && let Some(m) = self.current_mean()
            && self.groups.len() < MAX_GROUPS
        {
            self.groups.push(m);
        }
    }

    /// Where things stand. A complete group still being shot counts as finished.
    pub fn view(&self) -> GuideView {
        let mut groups = self.groups.clone();
        let complete_now = self.current.len() >= GROUP_SHOTS;
        if complete_now && let Some(m) = self.current_mean() {
            groups.push(m);
        }
        let mut t = targets();
        for x in &mut t {
            let u = unit(x.azimuth, x.inclination);
            x.covered = groups.iter().any(|g| angle(*g, u) <= COVER_DEG);
        }
        let covered = t.iter().filter(|x| x.covered).count();
        let next = t.iter().find(|x| !x.covered).cloned();
        let n = groups.len();
        let done = next.is_none() && n >= GOOD_GROUPS;
        let shots = self.current.len().min(GROUP_SHOTS);
        let say = if done {
            "Every direction is covered. On the X1: M to compute, then M to apply if the average error is small.".into()
        } else if n >= MAX_GROUPS {
            "The X1 takes no more groups (25). Compute on the X1 with M.".into()
        } else if (1..GROUP_SHOTS).contains(&shots) {
            format!(
                "Roll the X1 90° about the laser, keep the dot on the target, and shoot ({shots} of {GROUP_SHOTS} in this group)."
            )
        } else if let Some(x) = &next {
            let more = if covered == t.len() {
                format!(" ({} more groups to reach {GOOD_GROUPS})", GOOD_GROUPS - n)
            } else {
                String::new()
            };
            format!(
                "Next: point {}, and take 4 shots rolling 90° between them.{more}",
                x.label
            )
        } else {
            format!(
                "All directions covered. {} more groups anywhere to reach {GOOD_GROUPS}.",
                GOOD_GROUPS - n
            )
        };
        GuideView {
            targets: t,
            groups: groups.iter().map(|g| direction(*g)).collect(),
            current_shots: shots,
            current: self.current_mean().map(direction),
            next,
            covered,
            can_compute: n >= MIN_GROUPS,
            done,
            say,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Four shots at a direction, a little noise from rolling.
    fn group(g: &mut Guide, az: f64, inc: f64) {
        for k in 0..4 {
            g.shot(az + (k as f64 - 1.5) * 0.4, inc + (k as f64 - 1.5) * 0.3);
        }
    }

    #[test]
    fn starts_by_asking_for_level_north() {
        let v = Guide::new().view();
        assert_eq!(v.targets.len(), 14);
        assert_eq!(v.next.unwrap().label, "level, facing north");
        assert!(v.say.starts_with("Next: point level, facing north"));
        assert!(!v.can_compute && !v.done);
    }

    #[test]
    fn a_group_of_four_covers_its_target_and_moves_on() {
        let mut g = Guide::new();
        g.shot(2.0, 1.0);
        let v = g.view();
        assert_eq!(v.current_shots, 1);
        assert!(v.say.contains("Roll the X1 90°"), "{}", v.say);
        for _ in 0..3 {
            g.shot(2.5, 1.2);
        }
        let v = g.view();
        assert!(v.targets[0].covered);
        assert_eq!(v.next.unwrap().label, "level, facing east");
        assert_eq!(v.groups.len(), 1);
    }

    #[test]
    fn an_unfinished_group_is_dropped_when_the_x1_points_elsewhere() {
        let mut g = Guide::new();
        g.shot(0.0, 0.0);
        g.shot(0.3, 0.1);
        g.shot(90.0, 0.0); // a new direction after two shots
        let v = g.view();
        assert_eq!(v.groups.len(), 0);
        assert_eq!(v.current_shots, 1);
        assert!(!v.targets[0].covered);
    }

    #[test]
    fn fourteen_groups_at_the_targets_finish_it() {
        let mut g = Guide::new();
        for (i, t) in targets().iter().enumerate() {
            group(&mut g, t.azimuth, t.inclination);
            if i == 7 {
                assert!(g.view().can_compute, "8 groups: the X1 can compute");
            }
        }
        let v = g.view();
        assert_eq!(v.covered, 14);
        assert_eq!(v.groups.len(), 14);
        assert!(v.done, "{}", v.say);
        assert!(v.say.contains("compute"));
    }

    #[test]
    fn straight_up_is_covered_whatever_the_azimuth_says() {
        let mut g = Guide::new();
        group(&mut g, 217.0, 88.0); // the compass wanders when pointing up
        assert!(
            g.view()
                .targets
                .iter()
                .find(|t| t.label == "straight up")
                .unwrap()
                .covered
        );
    }

    #[test]
    fn more_than_four_shots_keep_the_last_four() {
        let mut g = Guide::new();
        for _ in 0..6 {
            g.shot(180.0, 0.0);
        }
        let v = g.view();
        assert_eq!(v.current_shots, 4);
        assert!(v.targets[2].covered, "level south");
    }
}
