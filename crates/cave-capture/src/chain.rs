//! Card chaining: shots at neighbouring stations joined to each other through the station cards,
//! with no survey positions needed (spec, "Card chaining").
//!
//! At A3 the CURRENT card marks A3 and FORWARD marks A4; at A4 CURRENT marks A4 and REAR marks A3.
//! Each shot measures both station points, so two shots share two points. Gravity is shared and
//! scale is locked, so those fix the turn about the vertical and the shift between them. One shared
//! point plus the compass is the weaker fallback, flagged.
//!
//! Everything here works in **survey axes**: a capture-frame point `p` is first taken through
//! [`capture_to_survey`] (x east, y north, z up for a compass shot; any heading for a tap shot), so
//! a turn is about z. Pure and derived: no file is rewritten, the chains are worked out each time.

use crate::bundle::{CardSeen, ShotHeader};
use crate::frame::{Placement, capture_to_survey};
use crate::place::{StationRef, placement, station_for};
use crate::stations::label_key;
use cave_survey::Vec3;

/// Glyph id of the CURRENT card (on the station the rig stands over).
pub const CURRENT: u32 = 2;
/// Glyph id of the FORWARD card (on the next station).
pub const FORWARD: u32 = 12;
/// Glyph id of the REAR card (on the previous station).
pub const REAR: u32 = 26;

/// Smallest misfit ever refused, metres.
pub const MISFIT_FLOOR_M: f64 = 0.10;
/// Misfit refused as a share of the distance it is measured over.
pub const MISFIT_SHARE: f64 = 0.03;
/// Two shared points closer than this sideways give no turn (a pitch): the compass is used instead.
const MIN_SIDEWAYS_M: f64 = 0.3;
/// Placed stations of a chain must spread at least this far sideways to give its heading.
const MIN_ANCHOR_SPREAD_M: f64 = 0.5;

/// The misfit allowed over `dist` metres: max(10 cm, 3 % of it).
pub fn misfit_limit(dist: f64) -> f64 {
    MISFIT_FLOOR_M.max(MISFIT_SHARE * dist)
}

/// Where the second station lies from the first along the survey.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    /// The second is the next station (A3 → A4): seen on the first's FORWARD card.
    Forward,
    /// The second is the previous station (A4 → A3): seen on the first's REAR card.
    Rear,
}

/// A label split into its prefix and trailing number (`"A 12"` → `("a", 12)`), matched as
/// [`label_key`] does; `None` without a trailing number.
fn split_label(label: &str) -> Option<(String, u64)> {
    let k = label_key(label);
    let digits = k.len() - k.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    if digits == 0 {
        return None;
    }
    let (prefix, num) = k.split_at(k.len() - digits);
    Some((prefix.to_string(), num.parse().ok()?))
}

/// Neighbours by label: same prefix (ignoring case and spaces), numbers one apart. `A3`, `A4` is
/// [`Dir::Forward`]; `A4`, `A3` is [`Dir::Rear`]. Anything else (`A3`, `B1`; `A3`, `A5`; a label
/// with no number) is not guessed: `None`.
pub fn neighbours(a_label: &str, b_label: &str) -> Option<Dir> {
    let (pa, na) = split_label(a_label)?;
    let (pb, nb) = split_label(b_label)?;
    if pa != pb {
        return None;
    }
    if na.checked_add(1) == Some(nb) {
        Some(Dir::Forward)
    } else if nb.checked_add(1) == Some(na) {
        Some(Dir::Rear)
    } else {
        None
    }
}

/// Neighbours by survey leg (station ids, `from` → `to`): `a` → `b` is [`Dir::Forward`], `b` → `a`
/// is [`Dir::Rear`]; `None` when no leg joins them. Empty ids match nothing.
pub fn leg_neighbours(a_id: &str, b_id: &str, legs: &[(String, String)]) -> Option<Dir> {
    if a_id.is_empty() || b_id.is_empty() || a_id == b_id {
        return None;
    }
    legs.iter().find_map(|(f, t)| {
        if f == a_id && t == b_id {
            Some(Dir::Forward)
        } else if f == b_id && t == a_id {
            Some(Dir::Rear)
        } else {
            None
        }
    })
}

/// A turn about the vertical plus a shift, in survey axes: `x ↦ R(yaw)·x + shift`, `yaw`
/// counter-clockwise seen from above (east towards north), radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rigid {
    /// Counter-clockwise turn about z, radians.
    pub yaw: f64,
    /// Shift after turning, metres.
    pub shift: Vec3,
}

impl Rigid {
    /// No turn, no shift.
    pub const IDENTITY: Rigid = Rigid {
        yaw: 0.0,
        shift: Vec3::new(0.0, 0.0, 0.0),
    };

    /// `v` turned (no shift).
    pub fn turn(&self, v: Vec3) -> Vec3 {
        let (s, c) = self.yaw.sin_cos();
        Vec3::new(c * v.x - s * v.y, s * v.x + c * v.y, v.z)
    }

    /// `v` turned and shifted.
    pub fn apply(&self, v: Vec3) -> Vec3 {
        self.turn(v).plus(self.shift)
    }

    /// `self ∘ o`: first `o`, then `self`.
    pub fn then_after(&self, o: &Rigid) -> Rigid {
        Rigid {
            yaw: self.yaw + o.yaw,
            shift: self.apply(o.shift),
        }
    }

    /// The transform undoing this one.
    pub fn inverse(&self) -> Rigid {
        let back = Rigid {
            yaw: -self.yaw,
            shift: Vec3::new(0.0, 0.0, 0.0),
        };
        let s = back.turn(self.shift);
        Rigid {
            yaw: -self.yaw,
            shift: Vec3::new(-s.x, -s.y, -s.z),
        }
    }

    /// The yaw in degrees, `-180..180`.
    pub fn yaw_deg(&self) -> f64 {
        let d = self.yaw.to_degrees().rem_euclid(360.0);
        if d >= 180.0 { d - 360.0 } else { d }
    }

    /// The same placement as a [`Placement`] (survey = turn(capture_to_survey(p − start)) + origin),
    /// taken at `start` (any capture-frame point).
    pub fn placement(&self, start: [f64; 3]) -> Placement {
        Placement {
            turn_deg: (-self.yaw.to_degrees()).rem_euclid(360.0),
            origin: self.apply(capture_to_survey(start)),
            start,
        }
    }

    /// A [`Placement`] as a rigid transform of survey-axis capture points.
    pub fn of_placement(p: &Placement) -> Rigid {
        let r = Rigid {
            yaw: -p.turn_deg.to_radians(),
            shift: Vec3::new(0.0, 0.0, 0.0),
        };
        let s = r.turn(capture_to_survey(p.start));
        Rigid {
            yaw: r.yaw,
            shift: p.origin.minus(s),
        }
    }
}

/// Least-squares turn about z plus shift taking each `src` onto its `dst` (scale locked): the turn
/// and horizontal shift from the horizontal plane, the vertical shift as the mean height difference.
/// Returns the transform and the largest distance left between a moved `src` and its `dst`.
/// `None` for no pairs.
pub fn fit_turn_shift(pairs: &[(Vec3, Vec3)]) -> Option<(Rigid, f64)> {
    if pairs.is_empty() {
        return None;
    }
    let n = pairs.len() as f64;
    let mean = |f: &dyn Fn(&(Vec3, Vec3)) -> Vec3| {
        let s = pairs
            .iter()
            .map(f)
            .fold(Vec3::new(0.0, 0.0, 0.0), Vec3::plus);
        Vec3::new(s.x / n, s.y / n, s.z / n)
    };
    let cs = mean(&|p| p.0);
    let cd = mean(&|p| p.1);
    let (mut dot, mut cross) = (0.0, 0.0);
    for (s, d) in pairs {
        let (sx, sy, dx, dy) = (s.x - cs.x, s.y - cs.y, d.x - cd.x, d.y - cd.y);
        dot += dx * sx + dy * sy;
        cross += dy * sx - dx * sy;
    }
    let yaw = if dot == 0.0 && cross == 0.0 {
        0.0
    } else {
        cross.atan2(dot)
    };
    Some(fixed_turn(pairs, yaw))
}

/// The shift (centroids matched) and largest residual for a turn that is already known.
fn fixed_turn(pairs: &[(Vec3, Vec3)], yaw: f64) -> (Rigid, f64) {
    let n = pairs.len() as f64;
    let r = Rigid {
        yaw,
        shift: Vec3::new(0.0, 0.0, 0.0),
    };
    let mut sum = Vec3::new(0.0, 0.0, 0.0);
    for (s, d) in pairs {
        sum = sum.plus(d.minus(r.turn(*s)));
    }
    let t = Rigid {
        yaw,
        shift: Vec3::new(sum.x / n, sum.y / n, sum.z / n),
    };
    let worst = pairs
        .iter()
        .map(|(s, d)| t.apply(*s).minus(*d).norm())
        .fold(0.0, f64::max);
    (t, worst)
}

/// What one shot brings to a pair fit: its cards (capture frame) and whether its frame is the
/// compass's (north-aligned), which is what lets one shared point be enough.
#[derive(Clone, Copy, Debug)]
pub struct FitShot<'a> {
    /// The cards the shot saw, capture frame.
    pub cards: &'a [CardSeen],
    /// The shot's frame is north-aligned (`.gravityAndHeading`).
    pub compass: bool,
}

fn card(cards: &[CardSeen], id: u32) -> Option<Vec3> {
    cards
        .iter()
        .find(|c| c.id == id)
        .map(|c| capture_to_survey(c.point))
}

/// Two neighbouring shots joined through their cards.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PairFit {
    /// Takes T's points (survey axes of T's capture frame) into S's.
    pub t_to_s: Rigid,
    /// Largest distance left between a shared card pair after the fit, metres.
    pub residual_m: f64,
    /// The misfit allowed: max(10 cm, 3 % of the shared points' separation).
    pub limit_m: f64,
    /// Only one shared point (or two straight above each other): the turn is the compass's.
    pub one_point: bool,
    /// S's station to T's station, measured by the LiDAR, survey axes of S's frame (both cards
    /// averaged where both shots saw one); `None` when a station's card was not seen at all.
    pub leg: Option<Vec3>,
}

impl PairFit {
    /// The fit is within its limit.
    pub fn ok(&self) -> bool {
        self.residual_m <= self.limit_m
    }
}

fn mean_of(a: Option<Vec3>, b: Option<Vec3>) -> Option<Vec3> {
    match (a, b) {
        (Some(a), Some(b)) => Some(Vec3::new(
            (a.x + b.x) / 2.0,
            (a.y + b.y) / 2.0,
            (a.z + b.z) / 2.0,
        )),
        (a, b) => a.or(b),
    }
}

/// [`pair_fit`] without the refusal: the fit and its residual even when it is over its limit, so a
/// caller can say by how much. `None` when the cards cannot give a fit at all.
pub fn pair_fit_any(s: FitShot, t: FitShot, dir: Dir) -> Option<PairFit> {
    // S's card on S's station ↔ T's card on S's station; S's card on T's station ↔ T's CURRENT.
    let (t_on_s, s_on_t) = match dir {
        Dir::Forward => (REAR, FORWARD),
        Dir::Rear => (FORWARD, REAR),
    };
    let at_s = (card(s.cards, CURRENT), card(t.cards, t_on_s));
    let at_t = (card(s.cards, s_on_t), card(t.cards, CURRENT));
    let pairs: Vec<(Vec3, Vec3)> = [at_s, at_t]
        .into_iter()
        .filter_map(|(sp, tp)| Some((tp?, sp?)))
        .collect();
    let sideways = if pairs.len() == 2 {
        pairs[1].1.minus(pairs[0].1).horizontal()
    } else {
        0.0
    };
    let separation = if pairs.len() == 2 {
        pairs[1].1.minus(pairs[0].1).norm()
    } else {
        0.0
    };
    let (t_to_s, residual, one_point) = if pairs.len() == 2 && sideways >= MIN_SIDEWAYS_M {
        let (r, w) = fit_turn_shift(&pairs)?;
        (r, w, false)
    } else if !pairs.is_empty() && s.compass && t.compass {
        // both frames north-aligned: no turn between them, the point(s) give the shift
        let (r, w) = fixed_turn(&pairs, 0.0);
        (r, w, true)
    } else {
        return None;
    };
    let s_station = mean_of(at_s.0, at_s.1.map(|p| t_to_s.apply(p)));
    let t_station = mean_of(at_t.0, at_t.1.map(|p| t_to_s.apply(p)));
    Some(PairFit {
        t_to_s,
        residual_m: residual,
        limit_m: misfit_limit(separation),
        one_point,
        leg: s_station.zip(t_station).map(|(a, b)| b.minus(a)),
    })
}

/// Joins shot T to shot S, T being `dir` of S, through the cards both saw: S.CURRENT ↔ T.REAR and
/// S.FORWARD ↔ T.CURRENT (T ahead), or S.CURRENT ↔ T.FORWARD and S.REAR ↔ T.CURRENT (T behind).
/// Two shared points (at least 30 cm apart sideways) fix the turn and shift by least squares;
/// one shared point (or two straight above each other) is enough only when both shots are compass
/// shots (no turn between north-aligned frames), and is flagged `one_point`. `None` when the cards
/// give no fit or the fit misses by more than max(10 cm, 3 % of the separation).
pub fn pair_fit(s: FitShot, t: FitShot, dir: Dir) -> Option<PairFit> {
    pair_fit_any(s, t, dir).filter(PairFit::ok)
}

/// Where a shot's place came from, best first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Chained to a neighbouring shot, the chain placed through a station the cave has.
    Chain,
    /// Its own CURRENT card on its station, turned by the compass.
    CardCompass,
    /// The compass, the rig over its station (start camera, instrument height).
    Compass,
    /// An older heading-tap shot, placed by its tap.
    Tap,
    /// In a chain none of whose stations the cave has yet: placed relative to the chain's first
    /// shot, not in the cave.
    Floating,
    /// Waiting for its station, in no chain: not placed.
    Waiting,
    /// Its station is gone from the cave, or it could not be placed.
    Unplaced,
}

impl Source {
    /// The name written in the JSON.
    pub fn name(self) -> &'static str {
        match self {
            Source::Chain => "chain",
            Source::CardCompass => "card+compass",
            Source::Compass => "compass",
            Source::Tap => "tap",
            Source::Floating => "floating",
            Source::Waiting => "waiting",
            Source::Unplaced => "unplaced",
        }
    }
}

/// Something about a shot's chaining that a person should see.
#[derive(Clone, Debug, PartialEq)]
pub enum Flag {
    /// The LiDAR's station-to-station vector disagrees with the survey's: the pair is not used.
    LegMisfit {
        /// The other shot (index).
        other: usize,
        /// LiDAR station-to-station distance, metres.
        lidar_m: f64,
        /// Survey distance, metres.
        survey_m: f64,
        /// How far apart the two are (length and height), metres.
        off_m: f64,
        /// The limit, max(10 cm, 3 % of the survey distance).
        limit_m: f64,
    },
    /// The shared cards do not agree: the pair is not used.
    CardsMisfit {
        /// The other shot (index).
        other: usize,
        /// Largest card misfit, metres.
        residual_m: f64,
        /// The limit.
        limit_m: f64,
    },
    /// Joined to `other` through one shared point and the compass only.
    OnePoint {
        /// The other shot (index).
        other: usize,
    },
}

/// One shot's place after chaining.
#[derive(Clone, Debug, PartialEq)]
pub struct ShotPlace {
    /// Where it came from.
    pub source: Source,
    /// Its chain (index into [`Chains::chains`]) when chained to at least one neighbour.
    pub chain: Option<usize>,
    /// Survey-axis capture points (`capture_to_survey(p)`) to survey coordinates; for
    /// [`Source::Floating`], to the floating chain's own frame (north-aligned by the compass, the
    /// chain's first shot's station at the origin). `None` when not placed.
    pub to_survey: Option<Rigid>,
    /// The shot's station point in its capture frame: its CURRENT card when seen, else under the
    /// start camera by the instrument height.
    pub station_point: [f64; 3],
    /// The neighbouring shots it is joined to (indices).
    pub joined: Vec<usize>,
    /// Flags for a person to see.
    pub flags: Vec<Flag>,
}

impl ShotPlace {
    /// Joined to any neighbour through one shared point only.
    pub fn one_point(&self) -> bool {
        self.flags
            .iter()
            .any(|f| matches!(f, Flag::OnePoint { .. }))
    }
    /// A misfit was found with a neighbour.
    pub fn misfit(&self) -> bool {
        self.flags
            .iter()
            .any(|f| matches!(f, Flag::LegMisfit { .. } | Flag::CardsMisfit { .. }))
    }
}

/// Neighbouring shots joined into one rigid group.
#[derive(Clone, Debug, PartialEq)]
pub struct Chain {
    /// Its shots (indices), in input order; the first is the chain's first shot.
    pub shots: Vec<usize>,
    /// Placed in the cave through at least one of its stations.
    pub placed: bool,
}

/// Every shot's place, and the chains found.
#[derive(Clone, Debug, PartialEq)]
pub struct Chains {
    /// One per input shot, in input order.
    pub shots: Vec<ShotPlace>,
    /// Chains of two or more shots.
    pub chains: Vec<Chain>,
}

fn station_point(h: &ShotHeader) -> [f64; 3] {
    h.cards
        .iter()
        .find(|c| c.id == CURRENT)
        .map(|c| c.point)
        .unwrap_or([
            h.start_camera[0],
            h.start_camera[1] - h.instrument_height_m,
            h.start_camera[2],
        ])
}

struct Edge {
    s: usize,
    t: usize,
    fit: PairFit,
}

fn find(parent: &mut [usize], i: usize) -> usize {
    let mut r = i;
    while parent[r] != r {
        r = parent[r];
    }
    let mut j = i;
    while parent[j] != r {
        let n = parent[j];
        parent[j] = r;
        j = n;
    }
    r
}

/// Circular mean of angles (radians); `None` for none.
fn mean_angle(a: &[f64]) -> Option<f64> {
    if a.is_empty() {
        return None;
    }
    let (s, c) = a
        .iter()
        .fold((0.0, 0.0), |(s, c), x| (s + x.sin(), c + x.cos()));
    Some(s.atan2(c))
}

/// Works out every shot's place: neighbouring shots (by survey `legs` when both stations are in
/// `stations` and legs are given, else by label) are joined through their cards ([`pair_fit`]);
/// joined shots form chains. A chain with any station in `stations` (by id, or a waiting shot's
/// label) is placed as a whole: its heading from two or more of its placed stations when they
/// spread sideways, else from its compass shots (or a tap); its position from its placed
/// stations. A chain with none floats, held together around its first shot. Each chained shot's
/// station point is its CURRENT card when seen. Where both stations of a joined pair are placed,
/// the LiDAR's station-to-station vector is checked against the survey's; a misfit beyond
/// max(10 cm, 3 % of the leg) is flagged on both shots and the pair is not used.
/// Shots in no chain are placed alone: CURRENT card + compass, else compass, else tap.
pub fn chains(shots: &[ShotHeader], stations: &[StationRef], legs: &[(String, String)]) -> Chains {
    let n = shots.len();
    let found: Vec<Option<(usize, Option<usize>)>> =
        shots.iter().map(|h| station_for(h, stations)).collect();
    let label = |i: usize| -> &str {
        found[i].map_or(shots[i].station_label.as_str(), |(s, _)| stations[s].label)
    };
    let id = |i: usize| -> &str { found[i].map_or("", |(s, _)| stations[s].id) };
    let fit_shot = |i: usize| FitShot {
        cards: &shots[i].cards,
        compass: shots[i].compass.is_some(),
    };
    let mut flags: Vec<Vec<Flag>> = vec![Vec::new(); n];

    // 1. neighbouring pairs, fitted and checked
    let mut edges: Vec<Edge> = Vec::new();
    for s in 0..n {
        for t in s + 1..n {
            let dir = if !legs.is_empty() && !id(s).is_empty() && !id(t).is_empty() {
                leg_neighbours(id(s), id(t), legs)
            } else {
                neighbours(label(s), label(t))
            };
            let Some(dir) = dir else { continue };
            let Some(fit) = pair_fit_any(fit_shot(s), fit_shot(t), dir) else {
                continue;
            };
            if !fit.ok() {
                for (a, b) in [(s, t), (t, s)] {
                    flags[a].push(Flag::CardsMisfit {
                        other: b,
                        residual_m: fit.residual_m,
                        limit_m: fit.limit_m,
                    });
                }
                continue;
            }
            if let (Some((ss, _)), Some((ts, _)), Some(leg)) = (found[s], found[t], fit.leg) {
                let survey = stations[ts].pos.minus(stations[ss].pos);
                let dh = leg.horizontal() - survey.horizontal();
                let dv = leg.z - survey.z;
                let off = dh.hypot(dv);
                let limit = misfit_limit(survey.norm());
                if off > limit {
                    for (a, b) in [(s, t), (t, s)] {
                        flags[a].push(Flag::LegMisfit {
                            other: b,
                            lidar_m: leg.norm(),
                            survey_m: survey.norm(),
                            off_m: off,
                            limit_m: limit,
                        });
                    }
                    continue;
                }
            }
            if fit.one_point {
                flags[s].push(Flag::OnePoint { other: t });
                flags[t].push(Flag::OnePoint { other: s });
            }
            edges.push(Edge { s, t, fit });
        }
    }

    // 2. components: best-fitting pairs first, loops left out
    edges.sort_by(|a, b| a.fit.residual_m.total_cmp(&b.fit.residual_m));
    let mut parent: Vec<usize> = (0..n).collect();
    let mut tree: Vec<Vec<(usize, Rigid)>> = vec![Vec::new(); n];
    let mut joined: Vec<Vec<usize>> = vec![Vec::new(); n];
    for e in &edges {
        let (a, b) = (find(&mut parent, e.s), find(&mut parent, e.t));
        if a == b {
            continue;
        }
        parent[a.max(b)] = a.min(b);
        // tree[x] holds (y, x→y frame transform)
        tree[e.t].push((e.s, e.fit.t_to_s));
        tree[e.s].push((e.t, e.fit.t_to_s.inverse()));
        joined[e.s].push(e.t);
        joined[e.t].push(e.s);
    }

    let mut out: Vec<ShotPlace> = (0..n)
        .map(|i| ShotPlace {
            source: Source::Unplaced,
            chain: None,
            to_survey: None,
            station_point: station_point(&shots[i]),
            joined: {
                let mut j = joined[i].clone();
                j.sort_unstable();
                j
            },
            flags: std::mem::take(&mut flags[i]),
        })
        .collect();
    let mut chains_out: Vec<Chain> = Vec::new();
    let mut done = vec![false; n];

    // 3. each chain: members' frames into the first shot's, then into the survey
    for root in 0..n {
        if done[root] || tree[root].is_empty() {
            continue;
        }
        // to_root[m]: m's frame → root's frame
        let mut to_root: Vec<Option<Rigid>> = vec![None; n];
        to_root[root] = Some(Rigid::IDENTITY);
        let mut queue = vec![root];
        let mut members = Vec::new();
        while let Some(x) = queue.pop() {
            members.push(x);
            done[x] = true;
            let xr = to_root[x].expect("set before queued");
            for &(y, x_to_y) in &tree[x] {
                if to_root[y].is_none() {
                    // y → root = (x → root) ∘ (y → x)
                    to_root[y] = Some(xr.then_after(&x_to_y.inverse()));
                    queue.push(y);
                }
            }
        }
        members.sort_unstable();
        let r = |m: usize| to_root[m].expect("member");
        let anchors: Vec<(Vec3, Vec3)> = members
            .iter()
            .filter_map(|&m| {
                found[m].map(|(s, _)| {
                    (
                        r(m).apply(capture_to_survey(out[m].station_point)),
                        stations[s].pos,
                    )
                })
            })
            .collect();
        // root frame → north: compass members are north-aligned, so each says root's heading
        let compass_yaw = mean_angle(
            &members
                .iter()
                .filter(|&&m| shots[m].compass.is_some())
                .map(|&m| -r(m).yaw)
                .collect::<Vec<_>>(),
        );
        let tap_yaw = || {
            members.iter().find_map(|&m| {
                let (s, t) = found[m]?;
                let p = placement(&shots[m], stations[s].pos, t.map(|t| stations[t].pos))?;
                Some(Rigid::of_placement(&p).yaw - r(m).yaw)
            })
        };
        let spread = anchors
            .iter()
            .map(|a| a.1.minus(anchors[0].1).horizontal())
            .fold(0.0, f64::max);
        let k = chains_out.len();
        let root_to_survey = if anchors.len() >= 2 && spread >= MIN_ANCHOR_SPREAD_M {
            fit_turn_shift(&anchors).map(|(t, _)| t)
        } else if !anchors.is_empty() {
            compass_yaw
                .or_else(tap_yaw)
                .map(|yaw| fixed_turn(&anchors, yaw).0)
        } else {
            None
        };
        let placed = root_to_survey.is_some();
        let (frame, source) = match root_to_survey {
            Some(t) => (Some(t), Source::Chain),
            None if anchors.is_empty() => {
                // floating: north by the compass, the first shot's station at the origin
                let yaw = compass_yaw.unwrap_or(0.0);
                let turn = Rigid {
                    yaw,
                    shift: Vec3::new(0.0, 0.0, 0.0),
                };
                let at = turn.apply(capture_to_survey(out[root].station_point));
                (
                    Some(Rigid {
                        yaw,
                        shift: Vec3::new(-at.x, -at.y, -at.z),
                    }),
                    Source::Floating,
                )
            }
            None => (None, Source::Unplaced),
        };
        for &m in &members {
            out[m].chain = Some(k);
            out[m].source = source;
            out[m].to_survey = frame.map(|f| f.then_after(&r(m)));
        }
        chains_out.push(Chain {
            shots: members,
            placed,
        });
    }

    // 4. shots in no chain, alone
    for i in 0..n {
        if out[i].chain.is_some() {
            continue;
        }
        let h = &shots[i];
        let Some((s, t)) = found[i] else {
            out[i].source = if h.is_waiting() {
                Source::Waiting
            } else {
                Source::Unplaced
            };
            continue;
        };
        let pos = stations[s].pos;
        let has_current = h.cards.iter().any(|c| c.id == CURRENT);
        if h.compass.is_some() && has_current {
            let at = capture_to_survey(out[i].station_point);
            out[i].source = Source::CardCompass;
            out[i].to_survey = Some(Rigid {
                yaw: 0.0,
                shift: pos.minus(at),
            });
        } else if let Some(p) = placement(h, pos, t.map(|t| stations[t].pos)) {
            out[i].source = if h.compass.is_some() {
                Source::Compass
            } else {
                Source::Tap
            };
            out[i].to_survey = Some(Rigid::of_placement(&p));
        }
    }
    Chains {
        shots: out,
        chains: chains_out,
    }
}

/// For a viewer that places a shot the old way (its station and target, [`placement`]) and has its
/// points relative to `station`: the turn and shift that move those points to where `to_survey`
/// puts them, still relative to `station`: `wanted − station = R(yaw)·(old − station) + shift`.
/// `None` when the shot cannot be placed the old way with these stations.
pub fn display_fix(
    h: &ShotHeader,
    to_survey: &Rigid,
    station: Vec3,
    target: Option<Vec3>,
) -> Option<Rigid> {
    let old = Rigid::of_placement(&placement(h, station, target)?);
    // absolute fix F = to_survey ∘ old⁻¹; relative: R(yaw)·r + (F(station) − station)
    let f = to_survey.then_after(&old.inverse());
    Some(Rigid {
        yaw: f.yaw,
        shift: f.apply(station).minus(station),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f64, y: f64, z: f64) -> Vec3 {
        Vec3::new(x, y, z)
    }

    /// Survey-axis → capture: the inverse of `capture_to_survey`.
    fn to_capture(p: Vec3) -> [f64; 3] {
        [p.x, p.z, -p.y]
    }

    /// A shot whose frame is `frame` (its survey-axis capture points → true survey) seeing `cards`
    /// (id, true survey point).
    fn shot(
        label: &str,
        id: &str,
        frame: Rigid,
        cards: &[(u32, Vec3)],
        compass: bool,
    ) -> ShotHeader {
        let inv = frame.inverse();
        let cards = cards
            .iter()
            .map(|&(cid, p)| CardSeen {
                id: cid,
                word: String::new(),
                point: to_capture(inv.apply(p)),
                corners: vec![],
                sightings: 9,
                spread_m: 0.01,
            })
            .collect();
        let start = to_capture(inv.apply(v(0.0, 0.0, 0.0)));
        let mut h: ShotHeader = serde_json::from_str(
            r#"{"shot_id":"s","cave_id":"c","station_id":"","station_label":"","captured_at_ms":0,
                "device":"t","instrument_height_m":0.3,"start_camera":[0,0,0],"taps":[],
                "compass":{"true_north":true,"accuracy_deg":5.0},
                "tracking":"normal","arkit_version":"26.1","gravity":[0,-1,0]}"#,
        )
        .unwrap();
        h.station_id = id.into();
        h.station_label = label.into();
        h.start_camera = start;
        h.cards = cards;
        if !compass {
            h.compass = None;
        }
        h
    }

    fn rig(yaw_deg: f64, x: f64, y: f64, z: f64) -> Rigid {
        Rigid {
            yaw: yaw_deg.to_radians(),
            shift: v(x, y, z),
        }
    }

    const A3: Vec3 = Vec3::new(10.0, 20.0, 5.0);
    const A4: Vec3 = Vec3::new(14.0, 23.0, 4.5);
    const A5: Vec3 = Vec3::new(18.0, 22.0, 4.0);

    fn close(a: Vec3, b: Vec3, tol: f64) -> bool {
        a.minus(b).norm() < tol
    }

    #[test]
    fn labels_next_to_each_other() {
        assert_eq!(neighbours("A3", "A4"), Some(Dir::Forward));
        assert_eq!(neighbours("a 4", "A3"), Some(Dir::Rear));
        assert_eq!(neighbours("A3", "B4"), None);
        assert_eq!(neighbours("A3", "B1"), None);
        assert_eq!(neighbours("A3", "A5"), None);
        assert_eq!(neighbours("A3", "A3"), None);
        assert_eq!(neighbours("A", "A1"), None);
        assert_eq!(neighbours("9", "10"), Some(Dir::Forward));
    }

    #[test]
    fn legs_give_direction() {
        let legs = vec![("a3".to_string(), "b1".to_string())];
        assert_eq!(leg_neighbours("a3", "b1", &legs), Some(Dir::Forward));
        assert_eq!(leg_neighbours("b1", "a3", &legs), Some(Dir::Rear));
        assert_eq!(leg_neighbours("a3", "a4", &legs), None);
        assert_eq!(leg_neighbours("", "b1", &legs), None);
    }

    #[test]
    fn rigid_inverse_and_placement_round_trip() {
        let r = rig(37.0, 1.0, -2.0, 0.5);
        let p = v(3.0, 4.0, 5.0);
        assert!(close(r.inverse().apply(r.apply(p)), p, 1e-12));
        let pl = r.placement([0.2, 0.1, -0.4]);
        let q = [1.0, 2.0, 3.0];
        assert!(close(pl.apply(q), r.apply(capture_to_survey(q)), 1e-12));
        assert!(close(
            Rigid::of_placement(&pl).apply(capture_to_survey(q)),
            pl.apply(q),
            1e-12
        ));
    }

    #[test]
    fn two_shared_points_recover_turn_and_shift() {
        // S's frame is 4° off north; T's is turned 37° more and shifted
        let fs = rig(4.0, 10.2, 19.9, 5.3);
        let ft = rig(41.0, 13.8, 23.1, 4.8);
        let s = shot("A3", "", fs, &[(CURRENT, A3), (FORWARD, A4)], true);
        let t = shot("A4", "", ft, &[(CURRENT, A4), (REAR, A3)], true);
        let fit = pair_fit(
            FitShot {
                cards: &s.cards,
                compass: true,
            },
            FitShot {
                cards: &t.cards,
                compass: true,
            },
            Dir::Forward,
        )
        .unwrap();
        let want = fs.inverse().then_after(&ft);
        assert!(
            (fit.t_to_s.yaw_deg() - 37.0).abs() < 0.5,
            "{}",
            fit.t_to_s.yaw_deg()
        );
        assert!(close(fit.t_to_s.shift, want.shift, 0.01));
        assert!(!fit.one_point);
        assert!(fit.residual_m < 1e-9);
        // the LiDAR's A3 → A4, in S's frame, has the survey leg's length
        assert!((fit.leg.unwrap().norm() - A4.minus(A3).norm()).abs() < 1e-9);
        // the same pair the other way round
        let back = pair_fit(
            FitShot {
                cards: &t.cards,
                compass: true,
            },
            FitShot {
                cards: &s.cards,
                compass: true,
            },
            Dir::Rear,
        )
        .unwrap();
        assert!((back.t_to_s.yaw_deg() + 37.0).abs() < 0.5);
    }

    #[test]
    fn one_shared_point_uses_the_compass() {
        let fs = rig(0.0, 10.0, 20.0, 5.3);
        let ft = rig(0.0, 14.0, 23.0, 4.8);
        // T saw no REAR card: only A4 is shared
        let s = shot("A3", "", fs, &[(CURRENT, A3), (FORWARD, A4)], true);
        let t = shot("A4", "", ft, &[(CURRENT, A4)], true);
        let fs_ = FitShot {
            cards: &s.cards,
            compass: true,
        };
        let fit = pair_fit(
            fs_,
            FitShot {
                cards: &t.cards,
                compass: true,
            },
            Dir::Forward,
        )
        .unwrap();
        assert!(fit.one_point);
        assert!(fit.t_to_s.yaw.abs() < 1e-12);
        assert!(close(fit.t_to_s.shift, ft.shift.minus(fs.shift), 1e-9));
        // without a compass one point is not enough
        assert!(
            pair_fit(
                fs_,
                FitShot {
                    cards: &t.cards,
                    compass: false
                },
                Dir::Forward
            )
            .is_none()
        );
    }

    #[test]
    fn misfit_is_refused() {
        let fs = rig(0.0, 10.0, 20.0, 5.3);
        let ft = rig(20.0, 14.0, 23.0, 4.8);
        let s = shot("A3", "", fs, &[(CURRENT, A3), (FORWARD, A4)], true);
        // T's REAR card 40 cm off where S's CURRENT says A3 is
        let t = shot(
            "A4",
            "",
            ft,
            &[(CURRENT, A4), (REAR, A3.plus(v(0.0, 0.0, 0.4)))],
            true,
        );
        let a = FitShot {
            cards: &s.cards,
            compass: true,
        };
        let b = FitShot {
            cards: &t.cards,
            compass: true,
        };
        assert!(pair_fit(a, b, Dir::Forward).is_none());
        let any = pair_fit_any(a, b, Dir::Forward).unwrap();
        assert!(any.residual_m > 0.15 && any.limit_m >= 0.1);
        let c = chains(&[s, t], &[], &[]);
        assert!(c.chains.is_empty());
        assert!(c.shots[0].misfit() && c.shots[1].misfit());
    }

    fn refs<'a>(st: &'a [(&'a str, &'a str, Vec3)]) -> Vec<StationRef<'a>> {
        st.iter()
            .map(|&(id, label, pos)| StationRef { id, label, pos })
            .collect()
    }

    #[test]
    fn chain_places_through_its_stations_and_beats_the_compass() {
        // both compass frames 6° wrong; the two placed stations give the true heading
        let fs = rig(6.0, 10.1, 20.2, 5.3);
        let ft = rig(-6.0, 14.2, 22.9, 4.8);
        let s = shot("A3", "a3", fs, &[(CURRENT, A3), (FORWARD, A4)], true);
        let t = shot("A4", "a4", ft, &[(CURRENT, A4), (REAR, A3)], true);
        let st = [("a3", "A3", A3), ("a4", "A4", A4)];
        let c = chains(&[s.clone(), t.clone()], &refs(&st), &[]);
        assert_eq!(c.chains.len(), 1);
        assert!(c.chains[0].placed);
        for (i, (h, f)) in [(&s, fs), (&t, ft)].into_iter().enumerate() {
            let p = &c.shots[i];
            assert_eq!(p.source, Source::Chain);
            let m = p.to_survey.unwrap();
            assert!(
                (m.yaw_deg() - f.yaw_deg()).abs() < 0.5,
                "{} {}",
                m.yaw_deg(),
                f.yaw_deg()
            );
            for card in &h.cards {
                let truth = f.apply(capture_to_survey(card.point));
                assert!(close(m.apply(capture_to_survey(card.point)), truth, 0.01));
            }
        }
        assert_eq!(c.shots[0].joined, vec![1]);
    }

    #[test]
    fn three_shot_chain_with_one_station_placed_by_the_compass() {
        let fs = rig(0.0, 10.0, 20.0, 5.3);
        let ft = rig(0.0, 14.0, 23.0, 4.8);
        let fu = rig(0.0, 18.0, 22.0, 4.3);
        let s = shot("A3", "a3", fs, &[(CURRENT, A3), (FORWARD, A4)], true);
        let t = shot(
            "A4",
            "",
            ft,
            &[(CURRENT, A4), (REAR, A3), (FORWARD, A5)],
            true,
        );
        let u = shot("A5", "", fu, &[(CURRENT, A5), (REAR, A4)], true);
        let st = [("a3", "A3", A3)];
        let c = chains(&[u, s, t], &refs(&st), &[]);
        assert_eq!(c.chains.len(), 1);
        assert_eq!(c.chains[0].shots, vec![0, 1, 2]);
        let m = c.shots[0].to_survey.unwrap();
        // A5's CURRENT card lands on A5
        assert!(close(
            m.apply(capture_to_survey(c.shots[0].station_point)),
            A5,
            0.01
        ));
        assert!(c.shots.iter().all(|p| p.source == Source::Chain));
    }

    #[test]
    fn waiting_chain_floats_then_places() {
        let a7 = v(100.0, 50.0, -3.0);
        let a8 = v(103.0, 54.0, -3.5);
        let f7 = rig(0.0, 100.0, 50.0, -2.7);
        let f8 = rig(0.0, 103.0, 54.0, -3.2);
        let s = shot("A7", "", f7, &[(CURRENT, a7), (FORWARD, a8)], true);
        let t = shot("A8", "", f8, &[(CURRENT, a8), (REAR, a7)], true);
        let none = chains(&[s.clone(), t.clone()], &[], &[]);
        assert_eq!(none.chains.len(), 1);
        assert!(!none.chains[0].placed);
        let p7 = &none.shots[0];
        assert_eq!(p7.source, Source::Floating);
        // the first shot's station at the origin, the other where the chain says
        let m7 = p7.to_survey.unwrap();
        assert!(close(
            m7.apply(capture_to_survey(p7.station_point)),
            v(0.0, 0.0, 0.0),
            1e-9
        ));
        let p8 = &none.shots[1];
        let at8 = p8
            .to_survey
            .unwrap()
            .apply(capture_to_survey(p8.station_point));
        assert!(close(at8, a8.minus(a7), 0.01));
        // A7 arrives: the whole chain drops into place
        let st = [("x", "a 7", a7)];
        let placed = chains(&[s, t], &refs(&st), &[]);
        assert!(placed.chains[0].placed);
        let at8 = placed.shots[1]
            .to_survey
            .unwrap()
            .apply(capture_to_survey(placed.shots[1].station_point));
        assert!(close(at8, a8, 0.01));
        assert_eq!(placed.shots[1].source, Source::Chain);
    }

    #[test]
    fn labels_out_of_sequence_are_not_chained() {
        let b1 = v(14.0, 23.0, 4.5);
        let s = shot(
            "A3",
            "",
            rig(0.0, 10.0, 20.0, 5.3),
            &[(CURRENT, A3), (FORWARD, b1)],
            true,
        );
        let t = shot(
            "B1",
            "",
            rig(0.0, 14.0, 23.0, 4.8),
            &[(CURRENT, b1), (REAR, A3)],
            true,
        );
        let c = chains(&[s, t], &[], &[]);
        assert!(c.chains.is_empty());
        assert!(c.shots.iter().all(|p| p.source == Source::Waiting));
    }

    #[test]
    fn legs_override_labels() {
        let b1 = A4;
        let s = shot(
            "A3",
            "a3",
            rig(0.0, 10.0, 20.0, 5.3),
            &[(CURRENT, A3), (FORWARD, b1)],
            true,
        );
        let t = shot(
            "B1",
            "b1",
            rig(0.0, 14.0, 23.0, 4.8),
            &[(CURRENT, b1), (REAR, A3)],
            true,
        );
        let st = [("a3", "A3", A3), ("b1", "B1", b1)];
        let legs = vec![("a3".to_string(), "b1".to_string())];
        let c = chains(&[s.clone(), t.clone()], &refs(&st), &legs);
        assert_eq!(c.chains.len(), 1, "the leg joins A3 to B1");
        // and A3, A4 by label but no leg between them: not chained
        let st = [("a3", "A3", A3), ("a4", "A4", b1)];
        let mut t4 = t;
        t4.station_id = "a4".into();
        t4.station_label = "A4".into();
        let legs = vec![("a3".to_string(), "zz".to_string())];
        let c = chains(&[s, t4], &refs(&st), &legs);
        assert!(c.chains.is_empty());
    }

    #[test]
    fn leg_misfit_is_flagged_on_both_and_not_used() {
        let fs = rig(0.0, 10.0, 20.0, 5.3);
        let ft = rig(0.0, 14.0, 23.0, 4.8);
        let s = shot("A3", "a3", fs, &[(CURRENT, A3), (FORWARD, A4)], true);
        let t = shot("A4", "a4", ft, &[(CURRENT, A4), (REAR, A3)], true);
        // the survey puts A4 half a metre further on than the LiDAR measured
        let far = A4.plus(v(0.4, 0.3, 0.0));
        let st = [("a3", "A3", A3), ("a4", "A4", far)];
        let c = chains(&[s, t], &refs(&st), &[]);
        assert!(c.chains.is_empty());
        for p in &c.shots {
            assert!(p.misfit());
            assert_eq!(p.source, Source::CardCompass);
            let Flag::LegMisfit {
                off_m,
                lidar_m,
                survey_m,
                ..
            } = p.flags[0]
            else {
                panic!("{:?}", p.flags)
            };
            assert!((off_m - 0.5).abs() < 0.01 && (survey_m - lidar_m - 0.5).abs() < 0.01);
        }
    }

    #[test]
    fn lone_shot_uses_its_current_card() {
        let f = rig(0.0, 10.0, 20.0, 5.3);
        let s = shot(
            "A3",
            "a3",
            f,
            &[(CURRENT, A3.plus(v(0.0, 0.0, 0.05)))],
            true,
        );
        let st = [("a3", "A3", A3)];
        let c = chains(std::slice::from_ref(&s), &refs(&st), &[]);
        let p = &c.shots[0];
        assert_eq!(p.source, Source::CardCompass);
        assert!(close(
            p.to_survey
                .unwrap()
                .apply(capture_to_survey(p.station_point)),
            A3,
            1e-9
        ));
        // no card: plain compass
        let bare = shot("A3", "a3", f, &[], true);
        let c = chains(&[bare], &refs(&st), &[]);
        assert_eq!(c.shots[0].source, Source::Compass);
    }

    #[test]
    fn display_fix_moves_old_points_to_the_chain_place() {
        let fs = rig(6.0, 10.1, 20.2, 5.3);
        let ft = rig(-6.0, 14.2, 22.9, 4.8);
        let s = shot("A3", "a3", fs, &[(CURRENT, A3), (FORWARD, A4)], true);
        let t = shot("A4", "a4", ft, &[(CURRENT, A4), (REAR, A3)], true);
        let st = [("a3", "A3", A3), ("a4", "A4", A4)];
        let c = chains(&[s.clone(), t], &refs(&st), &[]);
        let m = c.shots[0].to_survey.unwrap();
        let fix = display_fix(&s, &m, A3, None).unwrap();
        let old = placement(&s, A3, None).unwrap();
        let p = [1.0, 0.5, -2.0];
        let rel = old.apply(p).minus(A3);
        let got = fix.apply(rel).plus(A3);
        assert!(close(got, m.apply(capture_to_survey(p)), 1e-9));
    }
}
