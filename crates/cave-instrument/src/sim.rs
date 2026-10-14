//! A pretend instrument, for any supported device, so the whole in-cave workflow can be tried and tested
//! with no hardware. It makes the shots of a survey (three close shots per leg, one per splay, a backsight
//! leg marked backsight), writes each one in the device's OWN wire format (the same encoders the tests use)
//! and hands it to the device's real link, so framing, acknowledgement, repeat handling and decoding run
//! exactly as for a real unit. Each device has quirks that can be switched on to see how the logic copes
//! before there is hardware to find out: a lost acknowledgement, a firmware fault, a missing indication, a
//! calibration shot in the middle of a survey. What each quirk is meant to show is in [`devices`].
//!
//! The survey it walks is made from a seed, never from a clock: the same seed is the same cave. Nothing here
//! is timed; the host fires [`Simulator::take_shot`] or [`Simulator::take_station`] when it likes.

use crate::bric::{self, ShotSpec as BricShot};
use crate::caveble::{self, Leg};
use crate::cavway::{self, PacketSpec};
use crate::link::{BricKind, BricLink, LegLink, LinkOut, Sap5Link, X1Link};
use crate::{Mark, Side, js_round, mod360, sap5};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};

/// The passage around a station, metres: what a surveyor would write as L R U D.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Walls {
    /// Left.
    pub left: f64,
    /// Right.
    pub right: f64,
    /// Up.
    pub up: f64,
    /// Down.
    pub down: f64,
}

impl Walls {
    fn of(&self, side: Side) -> f64 {
        match side {
            Side::Left => self.left,
            Side::Right => self.right,
            Side::Up => self.up,
            Side::Down => self.down,
        }
    }
}

/// What a surveyor does at one point of a traverse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StepKind {
    /// A foresight.
    Leg,
    /// A backsight.
    Back,
    /// A splay.
    Splay,
    /// A shot at a wall.
    Lrud,
}

/// One step of a traverse.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Step {
    /// What it is.
    pub kind: StepKind,
    /// On an lrud step: the wall it measures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<Side>,
    /// Metres.
    pub distance: f64,
    /// Degrees.
    pub azimuth: f64,
    /// Degrees.
    pub inclination: f64,
    /// On a leg: the passage at the station it arrives at (the splays that follow measure exactly this).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub walls: Option<Walls>,
    /// On the first leg only, when the survey starts with splays: the passage at the station it leaves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub behind: Option<Walls>,
}

/// A small, repeatable random stream (mulberry32), the same numbers as the web app's first simulator.
pub struct Rng(u32);

impl Rng {
    /// A stream from `seed`.
    pub fn new(seed: u32) -> Self {
        Rng(seed)
    }

    /// The next number in [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6d2b_79f5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }
}

fn round(v: f64, d: i32) -> f64 {
    let k = 10f64.powi(d);
    js_round(v * k) / k
}

/// What [`traverse`] makes a survey from.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraverseOpts {
    /// Seed: the same seed is the same cave.
    pub seed: u32,
    /// How many legs.
    pub legs: usize,
    /// A backsight after each foresight.
    #[serde(default)]
    pub backsights: bool,
    /// Readings round each new station (left, right, up, down).
    #[serde(default)]
    pub splays: usize,
    /// Start with the splays at the first station.
    #[serde(default)]
    pub start_splays: bool,
    /// Those readings are wall dimensions of the station (kind `lrud`, carrying the side), not splays.
    #[serde(default)]
    pub lrud: bool,
}

/// The shots of a wandering passage, in the order a surveyor takes them: a foresight per leg, then (when
/// `backsights`) the backsight, then `splays` readings round the new station (left, right, up, down). With
/// `lrud` those readings are wall dimensions of the station, not splays. Readings are rounded the way an
/// instrument shows them, and a backsight reads the opposite way with a little noise, so a closing check on
/// it has something to do.
pub fn traverse(opts: &TraverseOpts) -> Vec<Step> {
    let mut rng = Rng::new(opts.seed);
    let mut out = Vec::new();
    let mut heading = rng.next_f64() * 360.0;
    // A passage that changes slowly: each station is a little wider or narrower, higher or lower than the last.
    let wander = |rng: &mut Rng, v: f64, lo: f64, hi: f64, step: f64| {
        round((v + (rng.next_f64() - 0.5) * step).clamp(lo, hi), 2)
    };
    let left = round(0.7 + rng.next_f64() * 1.6, 2);
    let right = round(0.7 + rng.next_f64() * 1.6, 2);
    let up = round(1.0 + rng.next_f64() * 2.5, 2);
    let down = round(0.3 + rng.next_f64() * 0.9, 2);
    let mut walls = Walls {
        left,
        right,
        up,
        down,
    };
    // A surveyor shoots the splays at the first station before the first leg: they measure the passage there.
    let start = walls;
    let aims = [
        (-90.0, 0.0, Side::Left),
        (90.0, 0.0, Side::Right),
        (0.0, 90.0, Side::Up),
        (0.0, -90.0, Side::Down),
    ];
    let kind = if opts.lrud {
        StepKind::Lrud
    } else {
        StepKind::Splay
    };
    let side_of = |side: Side| opts.lrud.then_some(side);
    if opts.start_splays && opts.splays > 0 {
        let first = mod360(heading + (rng.next_f64() - 0.5) * 70.0);
        for s in 0..opts.splays {
            let (turn, up, side) = aims[s % 4];
            out.push(Step {
                kind,
                side: side_of(side),
                distance: round((start.of(side) + (rng.next_f64() - 0.5) * 0.06).max(0.1), 2),
                azimuth: round(mod360(first + turn), 1),
                inclination: up,
                walls: None,
                behind: None,
            });
        }
    }
    for i in 0..opts.legs {
        walls = Walls {
            left: wander(&mut rng, walls.left, 0.4, 3.5, 1.2),
            right: wander(&mut rng, walls.right, 0.4, 3.5, 1.2),
            up: wander(&mut rng, walls.up, 0.6, 4.5, 1.5),
            down: wander(&mut rng, walls.down, 0.2, 1.6, 0.6),
        };
        heading = mod360(heading + (rng.next_f64() - 0.5) * 70.0);
        let distance = round(2.0 + rng.next_f64() * 7.0, 2);
        let inclination = round((rng.next_f64() - 0.5) * 24.0, 1);
        out.push(Step {
            kind: StepKind::Leg,
            side: None,
            distance,
            azimuth: round(heading, 1),
            inclination,
            walls: Some(walls),
            behind: (i == 0 && opts.start_splays).then_some(start),
        });
        if opts.backsights {
            out.push(Step {
                kind: StepKind::Back,
                side: None,
                distance: round(distance + (rng.next_f64() - 0.5) * 0.04, 2),
                azimuth: round(mod360(heading + 180.0 + (rng.next_f64() - 0.5) * 1.6), 1),
                inclination: round(-inclination + (rng.next_f64() - 0.5) * 1.2, 1),
                walls: None,
                behind: None,
            });
        }
        // Splays aim left, right, up and down and measure the passage, give or take a few centimetres.
        let splay_aims = [
            (-90.0, 0.0, walls.left, Side::Left),
            (90.0, 0.0, walls.right, Side::Right),
            (0.0, 90.0, walls.up, Side::Up),
            (0.0, -90.0, walls.down, Side::Down),
        ];
        for s in 0..opts.splays {
            let (turn, up, wall, side) = splay_aims[s % 4];
            out.push(Step {
                kind,
                side: side_of(side),
                distance: round((wall + (rng.next_f64() - 0.5) * 0.06).max(0.1), 2),
                azimuth: round(mod360(heading + turn), 1),
                inclination: up,
                walls: None,
                behind: None,
            });
        }
    }
    out
}

// ---- devices ----

/// The devices that can be simulated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceKind {
    /// Cavway X1.
    X1,
    /// SAP6.
    Sap6,
    /// SAP5.
    Sap5,
    /// BRIC5.
    Bric,
}

/// A way a device misbehaves, and what to watch for.
#[derive(Clone, Debug, Serialize)]
pub struct Quirk {
    /// Switch name.
    pub id: &'static str,
    /// For the picker.
    pub label: &'static str,
    /// What it should look like when the logic copes.
    pub expect: &'static str,
}

/// A simulated device and its quirks.
#[derive(Clone, Debug, Serialize)]
pub struct Device {
    /// Which device.
    pub kind: DeviceKind,
    /// For the picker.
    pub label: &'static str,
    /// Its quirks, the first being "none".
    pub quirks: Vec<Quirk>,
}

const fn q(id: &'static str, label: &'static str, expect: &'static str) -> Quirk {
    Quirk { id, label, expect }
}
const NONE: Quirk = q("none", "No quirks", "A clean survey.");

/// The devices that can be simulated, and the quirks of each.
pub fn devices() -> Vec<Device> {
    vec![
        Device {
            kind: DeviceKind::X1,
            label: "Cavway X1",
            quirks: vec![
                NONE,
                q(
                    "flag-all",
                    "Leg flag on every shot",
                    "The same survey: a leg is reported at its first shot and refined by the next two.",
                ),
                q(
                    "flag-none",
                    "No leg flag",
                    "The same survey: three close shots still make a leg, by closeness alone.",
                ),
                q(
                    "short",
                    "33-byte packets",
                    "The same survey: the short packet form decodes.",
                ),
                q(
                    "lost-acks",
                    "Lost acks (packets repeat)",
                    "The same survey: every repeat is acknowledged but written once.",
                ),
                q(
                    "calibration",
                    "Calibration shots mixed in",
                    "The same survey: calibration packets are acknowledged and left out, and the log says so.",
                ),
                q(
                    "accuracy",
                    "Accuracy errors",
                    "The same survey, with some shots carrying an \"accuracy error\" warning.",
                ),
            ],
        },
        Device {
            kind: DeviceKind::Sap6,
            label: "SAP6",
            quirks: vec![
                NONE,
                q(
                    "lost-acks",
                    "Lost acks (frames repeat)",
                    "The same survey: each repeat is acknowledged again and written once.",
                ),
            ],
        },
        Device {
            kind: DeviceKind::Sap5,
            label: "SAP5",
            quirks: vec![
                NONE,
                q(
                    "fault32",
                    "Long shots (firmware bit-16 fault)",
                    "Shots past 32.768 m carry the SAP5 firmware fault; they must still read their true length, not 65 m too long.",
                ),
                q(
                    "repeats",
                    "Repeated packets",
                    "The same survey: a repeated packet is written once.",
                ),
            ],
        },
        Device {
            kind: DeviceKind::Bric,
            label: "BRIC5",
            quirks: vec![
                NONE,
                q(
                    "no-errors",
                    "No error indication",
                    "The same survey: each shot is written when the next begins, or after 400 ms.",
                ),
                q(
                    "missed",
                    "Missed shots (index jumps)",
                    "The same survey, and the log says shots were missed.",
                ),
                q(
                    "scan",
                    "Scan shots (splays)",
                    "The same survey: scan shots (type 1) are ordinary readings.",
                ),
                q(
                    "zero",
                    "Zero-length shots",
                    "The same survey: shots with no length are ignored, and the log says so.",
                ),
                q(
                    "errors",
                    "Instrument errors",
                    "The same survey, with some shots carrying \"the target moved\".",
                ),
                q(
                    "repeats",
                    "Repeated indications",
                    "The same survey: a repeated primary is written once.",
                ),
            ],
        },
    ]
}

/// The device of this kind.
pub fn device_of(kind: DeviceKind) -> Device {
    devices()
        .into_iter()
        .find(|d| d.kind == kind)
        .expect("every kind is listed")
}

/// `All`: every shot of a leg is flagged; `Third`: only the third is; `None`: the flag is never set (an X1 only).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FlagPolicy {
    /// Every shot of a leg.
    All,
    /// Only the third.
    Third,
    /// Never.
    None,
}

/// One single shot a surveyor produces.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShotSpec {
    /// Metres.
    pub distance: f64,
    /// Degrees.
    pub azimuth: f64,
    /// Degrees.
    pub inclination: f64,
    /// Flagged as part of a leg by the device (an X1 only; depends on the flag policy).
    pub leg: bool,
    /// Part of a leg whatever the flag says: for the quirks that need to know.
    pub is_leg: bool,
    /// The mark.
    pub mark: Mark,
    /// A shot that measures this wall of the station (the surveyor has the LRUD mode on): never a splay.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lrud: Option<Side>,
    /// The passage at the station a leg arrives at (carried on the first shot of a foresight leg).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub walls: Option<Walls>,
    /// On the first leg, when the survey starts with splays: the passage at the station it leaves.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behind: Option<Walls>,
}

/// The single shots a surveyor produces for these steps: three close shots per leg, one per splay.
pub fn shots_for(steps: &[Step], policy: FlagPolicy, seed: u32) -> Vec<ShotSpec> {
    let mut rng = Rng::new(seed);
    let mut out = Vec::new();
    for s in steps {
        if matches!(s.kind, StepKind::Splay | StepKind::Lrud) {
            out.push(ShotSpec {
                distance: s.distance,
                azimuth: s.azimuth,
                inclination: s.inclination,
                leg: false,
                is_leg: false,
                mark: Mark::None,
                lrud: if s.kind == StepKind::Lrud {
                    s.side
                } else {
                    None
                },
                walls: None,
                behind: None,
            });
            continue;
        }
        for i in 0..3 {
            // Close but not identical: a few millimetres and a fraction of a degree, as three real shots are.
            let distance = round(s.distance + (rng.next_f64() - 0.5) * 0.006, 3);
            let azimuth = round(
                ((s.azimuth + (rng.next_f64() - 0.5) * 0.4) % 360.0 + 360.0) % 360.0,
                2,
            );
            let inclination = round(s.inclination + (rng.next_f64() - 0.5) * 0.4, 2);
            out.push(ShotSpec {
                distance,
                azimuth,
                inclination,
                leg: policy == FlagPolicy::All || (policy == FlagPolicy::Third && i == 2),
                is_leg: true,
                mark: if s.kind == StepKind::Back {
                    Mark::Backsight
                } else {
                    Mark::None
                },
                lrud: None,
                walls: if i == 0 && s.kind == StepKind::Leg {
                    s.walls
                } else {
                    None
                },
                behind: if i == 0 && s.kind == StepKind::Leg {
                    s.behind
                } else {
                    None
                },
            });
        }
    }
    out
}

/// What a [`Simulator`] is made from.
pub struct SimOptions {
    /// Which device it pretends to be.
    pub kind: DeviceKind,
    /// The survey it walks.
    pub steps: Vec<Step>,
    /// X1 only; a quirk of `flag-all` or `flag-none` overrides it.
    pub policy: FlagPolicy,
    /// The quirks switched on (any of the device's quirk ids).
    pub quirks: Vec<String>,
}

/// An accuracy block with an absG error and an angle error.
const ACCURACY_ERRORS: [u8; 9] = [0x6f, 0x04, 0x29, 0x1c, 0x25, 0x1c, 0x07, 0x00, 0x00];

/// A pretend instrument.
pub struct Simulator {
    kind: DeviceKind,
    quirks: HashSet<String>,
    shots: Vec<ShotSpec>,
    x1: X1Link,
    sap6: LegLink,
    sap5: Sap5Link,
    bric: BricLink,
    index: usize,
    /// Which wall each shot fired measures, in the order fired: put on its reading when it comes out of the
    /// decoder (a BRIC's comes late).
    walls: VecDeque<Option<Side>>,
    fired: u32,
    seq: u8,
    bric_index: u32,
    hint: Option<(Walls, Walls)>,
    behind: Option<Walls>,
    clock: u32,
    connected: bool,
    received: Vec<Vec<u8>>,
}

impl Simulator {
    /// A simulator that will walk `opts.steps` (default: see [`default_steps`]).
    pub fn new(opts: SimOptions) -> Self {
        let quirks: HashSet<String> = opts.quirks.into_iter().collect();
        let policy = if quirks.contains("flag-all") {
            FlagPolicy::All
        } else if quirks.contains("flag-none") {
            FlagPolicy::None
        } else {
            opts.policy
        };
        let mut shots = shots_for(&opts.steps, policy, 1);
        // The SAP5 fault shows on long shots: a survey of legs several times the length shows it.
        if quirks.contains("fault32") {
            for s in shots.iter_mut().filter(|s| s.is_leg) {
                s.distance = round(s.distance * 8.0, 3);
            }
        }
        Simulator {
            kind: opts.kind,
            quirks,
            shots,
            x1: X1Link::new("sim"),
            sap6: LegLink::new("sim"),
            sap5: Sap5Link::new("sim"),
            bric: BricLink::new("sim"),
            index: 0,
            walls: VecDeque::new(),
            fired: 0,
            seq: 0,
            bric_index: 0,
            hint: None,
            behind: None,
            clock: 1_700_000_000,
            connected: false,
            received: Vec::new(),
        }
    }

    /// What it is called in the Survey panel.
    pub fn name(&self) -> String {
        format!("Simulated {}", device_of(self.kind).label)
    }

    /// Connects (there is nothing to connect to).
    pub fn connect(&mut self) {
        self.connected = true;
    }

    /// Disconnects; a BRIC shot still waiting for its errors is written.
    pub fn disconnect(&mut self) -> Vec<LinkOut> {
        self.walls.clear();
        let out = self.bric.flush();
        self.connected = false;
        self.mark_walls(out)
    }

    /// Every message the surveyor wrote to the device (the acknowledgements), in order.
    pub fn received(&self) -> &[Vec<u8>] {
        &self.received
    }

    /// The passage at the station of the leg being shot, and at the one behind it: what a surveyor would
    /// measure there.
    pub fn passage(&self) -> Option<(Walls, Walls)> {
        self.hint
    }

    /// The shot the next press will give, or `None` when the script has run out.
    pub fn upcoming(&self) -> Option<&ShotSpec> {
        self.shots.get(self.index)
    }

    /// True while a BRIC shot waits for the rest of its indications (the host flushes after
    /// [`bric::SETTLE_MS`]).
    pub fn has_pending(&self) -> bool {
        self.bric.has_pending()
    }

    /// Writes a BRIC shot that never got its error indication.
    pub fn flush_pending(&mut self) -> Vec<LinkOut> {
        let out = self.bric.flush();
        self.mark_walls(out)
    }

    /// An instrument cannot say that a shot is a wall dimension: the surveyor does, so the reading is marked
    /// after it is decoded. Acknowledgements are kept in [`Simulator::received`].
    fn mark_walls(&mut self, outs: Vec<LinkOut>) -> Vec<LinkOut> {
        let mut kept = Vec::new();
        for out in outs {
            match out {
                LinkOut::Send { bytes } => self.received.push(bytes),
                LinkOut::Reading { mut reading } => {
                    if let Some(Some(side)) = self.walls.pop_front() {
                        reading.lrud = Some(side);
                    }
                    kept.push(LinkOut::Reading { reading });
                }
                other => kept.push(other),
            }
        }
        kept
    }

    /// Fires one shot.
    pub fn take_shot(&mut self) -> Vec<LinkOut> {
        let Some(s) = self.shots.get(self.index).cloned() else {
            return Vec::new();
        };
        if !self.connected {
            return Vec::new();
        }
        self.index += 1;
        self.fired += 1;
        self.clock += 1;
        self.walls.push_back(s.lrud);
        if let Some(w) = s.walls {
            // A new leg: the passage at the station it reaches, and (for the first leg) at the one it leaves.
            self.hint = Some((w, s.behind.or(self.behind).unwrap_or(w)));
            self.behind = Some(w);
        }
        let outs = match self.kind {
            DeviceKind::X1 => self.send_x1(&s),
            DeviceKind::Sap6 => self.send_sap6(&s),
            DeviceKind::Sap5 => self.send_sap5(&s),
            DeviceKind::Bric => self.send_bric(&s),
        };
        self.mark_walls(outs)
    }

    fn send_x1(&mut self, s: &ShotSpec) -> Vec<LinkOut> {
        let mut outs = Vec::new();
        if self.quirks.contains("calibration") && self.fired.is_multiple_of(6) {
            let cal = cavway::encode_packet(&PacketSpec {
                kind: 2,
                azimuth: 295.0,
                inclination: 8.0,
                time: self.clock,
                ..PacketSpec::default()
            });
            outs.extend(self.x1.on_frame(&cal, "sim"));
        }
        let errors = (self.quirks.contains("accuracy") && self.fired.is_multiple_of(5))
            .then_some(ACCURACY_ERRORS);
        let packet = cavway::encode_packet(&PacketSpec {
            leg: s.leg,
            mark: s.mark,
            distance: s.distance,
            azimuth: s.azimuth,
            inclination: s.inclination,
            time: self.clock,
            short: self.quirks.contains("short"),
            errors,
            ..PacketSpec::default()
        });
        outs.extend(self.x1.on_frame(&packet, "sim"));
        if self.quirks.contains("lost-acks") {
            outs.extend(self.x1.on_frame(&packet, "sim")); // no ack heard: the X1 sends it again
        }
        outs
    }

    fn send_sap6(&mut self, s: &ShotSpec) -> Vec<LinkOut> {
        let frame = caveble::encode_leg(&Leg {
            seq: self.seq,
            azimuth: s.azimuth,
            inclination: s.inclination,
            roll: 0.0,
            distance: s.distance,
        });
        self.seq ^= 1;
        let mut outs = self.sap6.on_frame(&frame, "sim");
        if self.quirks.contains("lost-acks") {
            outs.extend(self.sap6.on_frame(&frame, "sim"));
        }
        outs
    }

    fn send_sap5(&mut self, s: &ShotSpec) -> Vec<LinkOut> {
        let mut packet = sap5::encode_packet(s.distance, s.azimuth, s.inclination, 0.0, self.seq);
        self.seq ^= 1;
        // The firmware fault: bit 16 of the distance is set once a distance passes 32.768 m (not 65.536 m).
        if self.quirks.contains("fault32") && s.distance >= 32.768 && s.distance < 65.536 {
            packet[0] |= 0x40;
        }
        let mut outs = self.sap5.on_frame(&packet, "sim");
        if self.quirks.contains("repeats") {
            outs.extend(self.sap5.on_frame(&packet, "sim"));
        }
        outs
    }

    fn send_bric(&mut self, s: &ShotSpec) -> Vec<LinkOut> {
        let mut outs = Vec::new();
        let errors = (self.quirks.contains("errors") && self.fired.is_multiple_of(5))
            .then(|| vec![(7u8, 0.0, 0.0)]);
        let kind = u8::from(self.quirks.contains("scan") && !s.is_leg);
        self.bric_shot(&mut outs, s, s.distance, kind, errors.unwrap_or_default());
        if self.quirks.contains("zero") && self.fired.is_multiple_of(9) {
            self.clock += 1;
            self.bric_shot(&mut outs, s, 0.0, 0, Vec::new()); // a mis-fire: a shot with no length
        }
        outs
    }

    fn bric_shot(
        &mut self,
        outs: &mut Vec<LinkOut>,
        s: &ShotSpec,
        distance: f64,
        kind: u8,
        errors: Vec<(u8, f64, f64)>,
    ) {
        self.bric_index += if self.quirks.contains("missed") && self.fired.is_multiple_of(7) {
            2
        } else {
            1
        };
        let sec = (self.clock % 60) as u8;
        let min = ((self.clock / 60) % 60) as u8;
        let (primary, meta, errs) = bric::encode_shot(&BricShot {
            distance,
            azimuth: s.azimuth,
            inclination: s.inclination,
            index: self.bric_index,
            roll: 0.0,
            dip: 60.0,
            kind,
            time: bric::Time {
                year: 2026,
                month: 10,
                day: 14,
                hour: 12,
                minute: min,
                second: sec,
                centisecond: 0,
            },
            errors,
        });
        outs.extend(self.bric.on_frame(BricKind::Primary, &primary));
        if self.quirks.contains("repeats") {
            outs.extend(self.bric.on_frame(BricKind::Primary, &primary)); // the same indication again
        }
        outs.extend(self.bric.on_frame(BricKind::Meta, &meta));
        if !self.quirks.contains("no-errors") {
            outs.extend(self.bric.on_frame(BricKind::Errors, &errs));
        }
    }

    /// Shoots the next whole station in one go, as a surveyor would: any splays still to be shot where they
    /// stand (the first station's), the leg (three close shots), its backsight when the script has one and
    /// `want_backsights` is on, and the splays that measure the passage at the new station (`want_lrud`).
    /// Stops before the next leg. Returns how many shots it fired and what came out; the host settles what is
    /// waiting afterwards (the surveyor has stopped).
    pub fn take_station(
        &mut self,
        want_backsights: bool,
        want_lrud: bool,
    ) -> (usize, Vec<LinkOut>) {
        if !self.connected || self.upcoming().is_none() {
            return (0, Vec::new());
        }
        let mut fired = 0;
        let mut saw_leg = false;
        let mut outs = Vec::new();
        loop {
            let next = self.upcoming().cloned();
            match next {
                Some(n) if n.mark == Mark::Backsight && !want_backsights => self.index += 1, // no backsights today
                Some(n) if n.lrud.is_some() && !want_lrud => self.index += 1, // no LRUD today
                Some(n) => {
                    if n.walls.is_some() {
                        saw_leg = true;
                    }
                    outs.extend(self.take_shot());
                    fired += 1;
                }
                None => break,
            }
            // A foresight leg's first shot carries the passage at its far end: the one after the leg this
            // call shot starts the next station.
            match self.upcoming() {
                Some(u) if !(saw_leg && u.walls.is_some()) => {}
                _ => break,
            }
        }
        outs.extend(self.flush_pending()); // the surveyor stops: a BRIC shot still waiting for its errors is written
        (fired, outs)
    }
}

/// The survey a simulator walks when it is not given one.
pub fn default_steps() -> Vec<Step> {
    traverse(&TraverseOpts {
        seed: 7,
        legs: 20,
        backsights: true,
        splays: 2,
        start_splays: false,
        lrud: false,
    })
}
