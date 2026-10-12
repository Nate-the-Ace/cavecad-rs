//! Resolves shots into station coordinates, finds loops, and measures how well they close.
//!
//! Ported from the old `CsNetwork`. Resolution is by repeated passes over the legs until nothing
//! more resolves, so shots may come in any order, branches work, and a leg whose TO is known
//! places its FROM backwards. A leg whose two ends are both already placed is a closure.
//!
//! Anchors: every fixed station that appears in a leg is pinned at its fixed position; if a
//! component has none, the FROM of its first leg is pinned at the origin.
//!
//! Known difference from the old app: the old code classified a closure joining two *different
//! fixed* anchors on one ring as a loop "via control"; here any closure joining two differently
//! anchored sections is a [`ClosureKind::Tie`] (no percent). The explicit-anchor control-frame
//! reconciliation is not ported yet.

use crate::vec3::Vec3;
use cave_id::Id;
use std::collections::{BTreeMap, HashMap, HashSet};

/// One surveyed leg between two stations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leg {
    /// The shot this leg came from.
    pub shot: Id,
    /// Station the leg starts at.
    pub from: Id,
    /// Station the leg ends at.
    pub to: Id,
    /// Measured displacement from `from` to `to`, in metres.
    pub offset: Vec3,
    /// Tape length in metres (used for weights and loop length).
    pub distance: f64,
    /// Hold this leg rigid when adjusting.
    pub no_adjust: bool,
    /// The bearing was omitted (a plumbed pitch).
    pub azimuth_omitted: bool,
}

/// A station with a known position (surveyed control).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fixed {
    /// The station.
    pub station: Id,
    /// Its position in metres.
    pub pos: Vec3,
}

/// A placed station.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    /// Position in metres.
    pub pos: Vec3,
    /// Order of placement (the survey order labels and walls rely on).
    pub seq: usize,
}

/// How a leg took part in resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegKind {
    /// It placed a new station.
    New,
    /// It closed a loop.
    Closure,
    /// It joined two separately anchored sections.
    Tie,
}

/// One leg, in resolution order: what is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LegResult {
    /// The shot.
    pub shot: Id,
    /// Start station.
    pub from: Id,
    /// End station.
    pub to: Id,
    /// How it took part.
    pub kind: LegKind,
}

/// Whether a closure completes a ring or joins two anchored sections.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClosureKind {
    /// A ring: both ends hang off the same anchor.
    Loop,
    /// A control tie: the ends hang off different anchors.
    Tie,
}

/// The misclosure of one closing leg, as surveyed.
#[derive(Clone, Debug, PartialEq)]
pub struct Closure {
    /// The closing shot.
    pub shot: Id,
    /// The station where the error shows (the leg's TO).
    pub at: Id,
    /// Computed minus already-known position.
    pub delta: Vec3,
    /// Plan error.
    pub horizontal: f64,
    /// Vertical error (absolute).
    pub vertical: f64,
    /// 3D error.
    pub distance: f64,
    /// Loop or tie.
    pub kind: ClosureKind,
    /// Stations around the ring from the closing leg's FROM to its TO (just the two ends for a tie).
    pub path: Vec<Id>,
    /// Length of the ring including the closing leg.
    pub traverse_length: f64,
    /// Error as a percent of ring length; `None` for a tie.
    pub percent: Option<f64>,
}

/// Everything resolution produced.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    /// Every placed station.
    pub stations: BTreeMap<Id, Placed>,
    /// Every leg in resolution order.
    pub legs: Vec<LegResult>,
    /// Every closing leg's misclosure, in resolution order.
    pub closures: Vec<Closure>,
    /// Stations placed with no parent, in placement order. Adjustment pins these.
    pub anchors: Vec<Id>,
}

impl Resolved {
    /// The ring closures only.
    pub fn loops(&self) -> impl Iterator<Item = &Closure> {
        self.closures.iter().filter(|c| c.kind == ClosureKind::Loop)
    }

    /// The control ties only.
    pub fn ties(&self) -> impl Iterator<Item = &Closure> {
        self.closures.iter().filter(|c| c.kind == ClosureKind::Tie)
    }
}

struct Walk {
    parent: HashMap<Id, (Id, usize)>,
    root: HashMap<Id, Id>,
}

impl Walk {
    /// Stations from `s` up to its anchor, inclusive.
    fn chain(&self, s: Id) -> Vec<Id> {
        let mut out = vec![s];
        let mut cur = s;
        while let Some(&(p, _)) = self.parent.get(&cur) {
            out.push(p);
            cur = p;
        }
        out
    }
}

/// Places every station reachable from the anchors and measures every closure.
pub fn resolve(legs: &[Leg], fixed: &[Fixed]) -> Resolved {
    let mut stations: BTreeMap<Id, Placed> = BTreeMap::new();
    let mut walk = Walk {
        parent: HashMap::new(),
        root: HashMap::new(),
    };
    let mut anchors = Vec::new();
    let mut results = Vec::new();
    let mut closures = Vec::new();
    // A shot from a station to itself is skipped, never scored as a closure of its own length.
    let mut used: Vec<bool> = legs.iter().map(|l| l.from == l.to).collect();

    let in_network: HashSet<Id> = legs
        .iter()
        .filter(|l| l.from != l.to)
        .flat_map(|l| [l.from, l.to])
        .collect();
    let place = |stations: &mut BTreeMap<Id, Placed>, id: Id, pos: Vec3| {
        let seq = stations.len();
        stations.insert(id, Placed { pos, seq });
    };
    for f in fixed {
        if in_network.contains(&f.station) && !stations.contains_key(&f.station) {
            place(&mut stations, f.station, f.pos);
            walk.root.insert(f.station, f.station);
            anchors.push(f.station);
        }
    }

    loop {
        let mut progress = false;
        for (i, leg) in legs.iter().enumerate() {
            if used[i] {
                continue;
            }
            match (
                stations.get(&leg.from).copied(),
                stations.get(&leg.to).copied(),
            ) {
                (Some(a), None) => {
                    place(&mut stations, leg.to, a.pos.plus(leg.offset));
                    walk.parent.insert(leg.to, (leg.from, i));
                    walk.root.insert(leg.to, walk.root[&leg.from]);
                    results.push(LegResult {
                        shot: leg.shot,
                        from: leg.from,
                        to: leg.to,
                        kind: LegKind::New,
                    });
                }
                (None, Some(b)) => {
                    place(&mut stations, leg.from, b.pos.minus(leg.offset));
                    walk.parent.insert(leg.from, (leg.to, i));
                    walk.root.insert(leg.from, walk.root[&leg.to]);
                    results.push(LegResult {
                        shot: leg.shot,
                        from: leg.from,
                        to: leg.to,
                        kind: LegKind::New,
                    });
                }
                (Some(a), Some(b)) => {
                    let closure = measure(legs, leg, a.pos, b.pos, &walk);
                    let kind = if closure.kind == ClosureKind::Loop {
                        LegKind::Closure
                    } else {
                        LegKind::Tie
                    };
                    results.push(LegResult {
                        shot: leg.shot,
                        from: leg.from,
                        to: leg.to,
                        kind,
                    });
                    closures.push(closure);
                }
                (None, None) => continue,
            }
            used[i] = true;
            progress = true;
        }
        if !progress {
            match used.iter().position(|u| !u) {
                Some(i) => {
                    let s = legs[i].from;
                    place(&mut stations, s, Vec3::ZERO);
                    walk.root.insert(s, s);
                    anchors.push(s);
                }
                None => break,
            }
        }
    }

    Resolved {
        stations,
        legs: results,
        closures,
        anchors,
    }
}

fn measure(legs: &[Leg], leg: &Leg, from_pos: Vec3, to_pos: Vec3, walk: &Walk) -> Closure {
    let delta = from_pos.plus(leg.offset).minus(to_pos);
    let same = walk.root.get(&leg.from) == walk.root.get(&leg.to);
    let (kind, path, length) = if same {
        let (fc, tc) = (walk.chain(leg.from), walk.chain(leg.to));
        let on_to: HashSet<Id> = tc.iter().copied().collect();
        let join = fc
            .iter()
            .position(|s| on_to.contains(s))
            .unwrap_or(fc.len() - 1);
        let join_id = fc[join];
        let tj = tc
            .iter()
            .position(|s| *s == join_id)
            .unwrap_or(tc.len() - 1);
        let mut path: Vec<Id> = fc[..=join].to_vec();
        path.extend(tc[..tj].iter().rev());
        let tree_len = |chain: &[Id]| -> f64 {
            chain
                .windows(2)
                .map(|w| legs[walk.parent[&w[0]].1].distance)
                .sum()
        };
        (
            ClosureKind::Loop,
            path,
            tree_len(&fc[..=join]) + tree_len(&tc[..=tj]) + leg.distance,
        )
    } else {
        (ClosureKind::Tie, vec![leg.from, leg.to], leg.distance)
    };
    let distance = delta.norm();
    let percent = (kind == ClosureKind::Loop && length > 0.0).then(|| distance / length * 100.0);
    Closure {
        shot: leg.shot,
        at: leg.to,
        delta,
        horizontal: delta.horizontal(),
        vertical: delta.z.abs(),
        distance,
        kind,
        path,
        traverse_length: length,
        percent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traverse::{Reading, TapeMode, offset};
    use cave_id::{IdSource, SeqIds};

    struct Net {
        ids: SeqIds,
        names: HashMap<&'static str, Id>,
        legs: Vec<Leg>,
    }

    impl Net {
        fn new() -> Self {
            Net {
                ids: SeqIds::new(1),
                names: HashMap::new(),
                legs: Vec::new(),
            }
        }
        fn id(&mut self, n: &'static str) -> Id {
            *self.names.entry(n).or_insert_with(|| self.ids.next_id())
        }
        fn shot(&mut self, from: &'static str, to: &'static str, d: f64, az: f64, inc: f64) {
            let (f, t, s) = (self.id(from), self.id(to), self.ids.next_id());
            let r = Reading {
                distance: d,
                azimuth: Some(az),
                inclination: Some(inc),
                ..Default::default()
            };
            let o = offset(&r, TapeMode::Slope).unwrap();
            self.legs.push(Leg {
                shot: s,
                from: f,
                to: t,
                offset: o.v,
                distance: d,
                no_adjust: false,
                azimuth_omitted: false,
            });
        }
    }

    fn near(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() < tol
    }

    /// The old unit test's square: out of order, a 0.5 misclosure, plus a branch.
    fn square() -> Net {
        let mut n = Net::new();
        n.shot("A4", "A1", 10.5, 270.0, 0.0);
        n.shot("A1", "A2", 10.0, 0.0, 0.0);
        n.shot("A2", "A3", 10.0, 90.0, 0.0);
        n.shot("A3", "A4", 10.0, 180.0, 0.0);
        n.shot("A3", "B1", 5.0, 90.0, 0.0);
        n
    }

    #[test]
    fn square_resolves_relative_to_a1() {
        let mut n = square();
        let r = resolve(&n.legs, &[]);
        let p = |n: &mut Net, s| r.stations[&n.id(s)].pos;
        let a1 = p(&mut n, "A1");
        assert!(
            near(p(&mut n, "A2").x - a1.x, 0.0, 1e-9) && near(p(&mut n, "A2").y - a1.y, 10.0, 1e-9)
        );
        assert!(near(p(&mut n, "A3").x - a1.x, 10.0, 1e-9));
        assert!(near(p(&mut n, "A4").y - a1.y, 0.0, 1e-9));
        assert!(near(p(&mut n, "B1").x - a1.x, 15.0, 1e-9));
        assert_eq!(
            r.anchors,
            vec![n.id("A4")],
            "first usable shot's FROM anchors"
        );
    }

    #[test]
    fn square_has_one_loop_with_the_old_numbers() {
        let n = square();
        let r = resolve(&n.legs, &[]);
        let loops: Vec<_> = r.loops().collect();
        assert_eq!(loops.len(), 1);
        let l = loops[0];
        assert!(
            near(l.distance, 0.5, 1e-9)
                && near(l.horizontal, 0.5, 1e-9)
                && near(l.vertical, 0.0, 1e-9)
        );
        assert!(near(l.traverse_length, 40.5, 1e-9));
        assert!(near(l.percent.unwrap(), 0.5 / 40.5 * 100.0, 1e-6));
        assert!(l.path.len() >= 4, "path walks the ring");
        assert_eq!(r.ties().count(), 0);
    }

    #[test]
    fn resolves_backwards_from_a_fixed_to() {
        let mut n = Net::new();
        n.shot("X2", "X1", 10.0, 0.0, 0.0);
        let x1 = n.id("X1");
        let r = resolve(
            &n.legs,
            &[Fixed {
                station: x1,
                pos: Vec3::new(100.0, 100.0, 0.0),
            }],
        );
        assert!(near(r.stations[&n.id("X2")].pos.y, 90.0, 1e-9));
    }

    #[test]
    fn disconnected_components_anchor_on_their_own_fixed_point() {
        let mut n = Net::new();
        n.shot("P1", "P2", 10.0, 90.0, 0.0);
        n.shot("Q1", "Q2", 10.0, 90.0, 0.0);
        let (p1, q1) = (n.id("P1"), n.id("Q1"));
        let r = resolve(
            &n.legs,
            &[
                Fixed {
                    station: p1,
                    pos: Vec3::ZERO,
                },
                Fixed {
                    station: q1,
                    pos: Vec3::new(1000.0, 0.0, 0.0),
                },
            ],
        );
        assert!(near(r.stations[&n.id("Q2")].pos.x, 1010.0, 1e-9));
        assert_eq!(r.stations.len(), 4);
    }

    #[test]
    fn disconnected_components_without_fixes_each_get_an_anchor() {
        let mut n = Net::new();
        n.shot("P1", "P2", 10.0, 90.0, 0.0);
        n.shot("Q1", "Q2", 10.0, 90.0, 0.0);
        let r = resolve(&n.legs, &[]);
        assert_eq!(r.anchors.len(), 2);
        assert_eq!(r.stations.len(), 4);
    }

    #[test]
    fn vertical_bust_lands_in_vertical() {
        let mut n = Net::new();
        n.shot("V1", "V2", 10.0, 0.0, 0.0);
        n.shot("V2", "V3", 10.0, 90.0, 0.0);
        n.shot("V3", "V1", 10.0 * 2f64.sqrt(), 225.0, 2.0);
        let r = resolve(&n.legs, &[]);
        let l = r.loops().next().unwrap();
        let tilt = 10.0 * 2f64.sqrt() * (1.0 - 2f64.to_radians().cos());
        assert!(near(l.horizontal, tilt, 1e-9));
        assert!(l.vertical > 0.4);
    }

    #[test]
    fn a_leg_joining_two_fixed_sections_is_a_tie_not_a_loop() {
        let mut n = Net::new();
        n.shot("P1", "P2", 10.0, 0.0, 0.0);
        n.shot("Q1", "Q2", 10.0, 0.0, 0.0);
        n.shot("P2", "Q1", 10.0, 90.0, 0.0);
        let (p1, q1) = (n.id("P1"), n.id("Q1"));
        let r = resolve(
            &n.legs,
            &[
                Fixed {
                    station: p1,
                    pos: Vec3::ZERO,
                },
                Fixed {
                    station: q1,
                    pos: Vec3::new(10.4, 10.0, 0.0),
                },
            ],
        );
        assert_eq!(r.loops().count(), 0);
        let ties: Vec<_> = r.ties().collect();
        assert_eq!(ties.len(), 1);
        assert!(near(ties[0].distance, 0.4, 1e-9) && ties[0].percent.is_none());
        assert!(r.legs.iter().any(|l| l.kind == LegKind::Tie));
    }

    #[test]
    fn order_of_shots_does_not_change_tree_positions() {
        let n = square();
        let fwd = resolve(&n.legs, &[]);
        let mut rev_legs = n.legs.clone();
        rev_legs.reverse();
        let rev = resolve(&rev_legs, &[]);
        // Positions are relative to the anchor, which differs; compare a distance that must agree.
        let d = |r: &Resolved, a: Id, b: Id| r.stations[&a].pos.minus(r.stations[&b].pos).norm();
        let mut n2 = Net {
            ids: SeqIds::new(1),
            names: n.names.clone(),
            legs: vec![],
        };
        let (a1, a2) = (n2.id("A1"), n2.id("A2"));
        assert!(near(d(&fwd, a1, a2), 10.0, 1e-9) && near(d(&rev, a1, a2), 10.0, 1e-9));
    }

    #[test]
    fn a_leg_from_a_station_to_itself_is_skipped_not_scored() {
        let mut n = Net::new();
        n.shot("S1", "S2", 5.0, 0.0, 0.0);
        n.shot("S2", "S2", 3.0, 90.0, 0.0);
        let r = resolve(&n.legs, &[]);
        assert!(r.closures.is_empty());
        assert_eq!(r.stations.len(), 2);
    }
}
