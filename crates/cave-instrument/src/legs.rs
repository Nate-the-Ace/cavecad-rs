//! Turns single shots into the things a survey is made of. The X1 does not send "a leg": it sends every
//! shot, and calls three or more consecutive close shots a leg (its screen says LEG, and the packets carry a
//! leg flag). So: shots that are close to the one before join a group; a group of three (or one the device
//! flags as a leg) is ONE leg, whose reading is the average of its shots; any other shot is a splay.
//!
//! Two things the real instrument has to settle (docs/CAVWAY-X1.md, UNVERIFIED): whether all three shots of
//! a leg carry the flag or only one, and how close "close" is for the device. This code does not depend on the
//! first: it decides by closeness and treats the flag as an extra vote (a flagged shot makes a leg at once,
//! whatever our count). The second is [`is_close`], one function with its numbers named, to be adjusted.

use crate::{Mark, Reading, angle_gap, mod360, tidy};
use serde::{Deserialize, Serialize};

/// Two shots of one leg agree to this much: tape 5 cm or 0.5 %, whichever is larger; compass and clino 2
/// degrees (a guess until the unit shows what its LEG decision uses).
pub const CLOSE_DISTANCE_M: f64 = 0.05;
/// See [`CLOSE_DISTANCE_M`].
pub const CLOSE_DISTANCE_FRACTION: f64 = 0.005;
/// See [`CLOSE_DISTANCE_M`].
pub const CLOSE_ANGLE_DEG: f64 = 2.0;
/// Beyond this the compass means nothing (a pitch): only the tape and clino are compared.
pub const STEEP_DEG: f64 = 85.0;
/// A leg is three close shots.
pub const SHOTS_PER_LEG: usize = 3;

/// True when `b` could be the next shot of the same leg as `a`.
pub fn is_close(a: &Reading, b: &Reading) -> bool {
    if (a.distance - b.distance).abs()
        > CLOSE_DISTANCE_M.max(CLOSE_DISTANCE_FRACTION * a.distance.max(b.distance))
    {
        return false;
    }
    if (a.inclination - b.inclination).abs() > CLOSE_ANGLE_DEG {
        return false;
    }
    let steep = a.inclination.abs() > STEEP_DEG && b.inclination.abs() > STEEP_DEG;
    steep || angle_gap(a.azimuth, b.azimuth) <= CLOSE_ANGLE_DEG
}

/// How far apart the shots of a leg were: worth a warning when large.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spread {
    /// Tape, metres.
    pub distance: f64,
    /// Compass, degrees.
    pub azimuth: f64,
    /// Clino, degrees.
    pub inclination: f64,
}

/// What the assembler reports.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum LegEvent {
    /// A shot that is not part of a leg.
    Splay {
        /// The shot.
        reading: Reading,
    },
    /// A shot that measures a wall of the station (its `lrud` says which): never part of a leg, never a splay.
    Lrud {
        /// The shot.
        reading: Reading,
    },
    /// A leg is complete: the average of its shots. `group` names it for later updates.
    Leg {
        /// The leg's group.
        group: u32,
        /// The average of its shots.
        reading: Reading,
        /// How many shots.
        shots: usize,
        /// The mark.
        mark: Mark,
        /// How far apart the shots were.
        spread: Spread,
        /// A shot of it carried the device's leg flag.
        flagged: bool,
    },
    /// Another close shot joined a leg already reported: the new average, to replace the old.
    LegUpdate {
        /// The leg's group.
        group: u32,
        /// The new average.
        reading: Reading,
        /// How many shots.
        shots: usize,
        /// The mark.
        mark: Mark,
        /// How far apart the shots were.
        spread: Spread,
        /// A shot of it carried the device's leg flag.
        flagged: bool,
    },
}

#[derive(Clone, Debug)]
struct Group {
    id: u32,
    shots: Vec<Reading>,
    /// The leg has been reported.
    leg: bool,
}

/// The mark of a leg: the last real mark any of its shots carries (the surveyor sets it while the laser is
/// on, on whichever shot).
fn mark_of(shots: &[Reading]) -> Mark {
    shots
        .iter()
        .rev()
        .filter_map(|s| s.mark)
        .find(|m| *m != Mark::None)
        .unwrap_or(Mark::None)
}

/// Average of the shots of a leg: tape and clino by arithmetic mean, compass by circular mean.
pub fn average(shots: &[Reading]) -> (Reading, Spread) {
    let n = shots.len() as f64;
    let mean = |f: &dyn Fn(&Reading) -> f64| shots.iter().map(f).sum::<f64>() / n;
    let sin = mean(&|r| r.azimuth.to_radians().sin());
    let cos = mean(&|r| r.azimuth.to_radians().cos());
    let azimuth = mod360(sin.atan2(cos).to_degrees());
    let last = &shots[shots.len() - 1];
    let range = |f: &dyn Fn(&Reading) -> f64| {
        shots.iter().map(f).fold(f64::NEG_INFINITY, f64::max)
            - shots.iter().map(f).fold(f64::INFINITY, f64::min)
    };
    let reading = Reading {
        distance: tidy(mean(&|r| r.distance), 4),
        azimuth: tidy(azimuth, 3),
        inclination: tidy(mean(&|r| r.inclination), 3),
        roll: last.roll,
        leg: Some(true),
        mark: Some(mark_of(shots)),
        error: shots
            .iter()
            .filter_map(|s| s.error.clone())
            .find(|e| !e.is_empty()),
        ..last.clone()
    };
    let steep = reading.inclination.abs() > STEEP_DEG;
    let spread = Spread {
        distance: tidy(range(&|r| r.distance), 4),
        inclination: tidy(range(&|r| r.inclination), 3),
        azimuth: if steep {
            0.0
        } else {
            tidy(
                shots
                    .iter()
                    .map(|r| angle_gap(r.azimuth, azimuth))
                    .fold(f64::NEG_INFINITY, f64::max)
                    * 2.0,
                3,
            )
        },
    };
    (reading, spread)
}

/// Groups single shots, in the order the instrument sent them.
#[derive(Clone, Debug, Default)]
pub struct LegAssembler {
    group: Option<Group>,
    counter: u32,
}

impl LegAssembler {
    /// A fresh assembler.
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes the next shot. Returns what that settled.
    pub fn push(&mut self, shot: Reading) -> Vec<LegEvent> {
        let mut out = Vec::new();
        if shot.lrud.is_some() {
            out.extend(self.settle());
            out.push(LegEvent::Lrud { reading: shot });
            return out;
        }
        if let Some(g) = self.group.as_mut()
            && is_close(&g.shots[g.shots.len() - 1], &shot)
        {
            let flagged_now = shot.leg == Some(true);
            g.shots.push(shot);
            if g.leg {
                out.push(Self::event(g, true));
            } else if g.shots.len() >= SHOTS_PER_LEG || flagged_now {
                g.leg = true;
                out.push(Self::event(g, false));
            }
            return out;
        }
        out.extend(self.settle());
        self.counter += 1;
        let mut fresh = Group {
            id: self.counter,
            shots: vec![shot],
            leg: false,
        };
        // A shot the device itself flags as a leg is one, even alone: the device knows what it counted.
        if fresh.shots[0].leg == Some(true) {
            fresh.leg = true;
            out.push(Self::event(&fresh, false));
        }
        self.group = Some(fresh);
        out
    }

    /// Nothing more is coming for now (the surveyor paused): what is waiting is splays.
    pub fn flush(&mut self) -> Vec<LegEvent> {
        let out = self.settle();
        self.group = None;
        out
    }

    /// Forgets everything (a new connection, another cave).
    pub fn reset(&mut self) {
        self.group = None;
    }

    /// Shots waiting on a decision (one or two close shots that may yet become a leg).
    pub fn waiting(&self) -> usize {
        self.group
            .as_ref()
            .filter(|g| !g.leg)
            .map_or(0, |g| g.shots.len())
    }

    fn settle(&mut self) -> Vec<LegEvent> {
        match self.group.take() {
            Some(g) if !g.leg => g
                .shots
                .into_iter()
                .map(|reading| LegEvent::Splay { reading })
                .collect(),
            _ => Vec::new(),
        }
    }

    fn event(g: &Group, update: bool) -> LegEvent {
        let (reading, spread) = average(&g.shots);
        let mark = reading.mark.unwrap_or(Mark::None);
        let flagged = g.shots.iter().any(|s| s.leg == Some(true));
        let (group, shots) = (g.id, g.shots.len());
        if update {
            LegEvent::LegUpdate {
                group,
                reading,
                shots,
                mark,
                spread,
                flagged,
            }
        } else {
            LegEvent::Leg {
                group,
                reading,
                shots,
                mark,
                spread,
                flagged,
            }
        }
    }
}
