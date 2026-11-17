//! Talks to a DJI Osmo Mobile gimbal over Bluetooth from a computer.
//!
//! DJI publishes nothing for current Osmo Mobile models. The gimbal speaks DJI's "DUML" message format over a
//! vendor Bluetooth service (`fff0`): commands are written to `fff5` (without response), the gimbal talks back on
//! `fff4` (and maybe `fff5`). The message layout, the two checksums and the rotate commands come from community work
//! (alkersan/om-research on OM2-4, shaun-xenon/osmo-remote on OM7P, yigitkonur/lib-osmo-ble on Pocket 3); what the
//! Osmo Mobile 7 itself accepts is measured with the tools in `src/bin` and logged in FINDINGS.md.
//!
//! Two layers, so it plugs into anything: the wire format ([`Message`], [`Parser`], [`rotate_payload`]) has no
//! Bluetooth in it and works over any transport (CaveCAD's native bridge, Web Bluetooth via wasm, a test); [`Osmo`] is
//! a btleplug transport with `turn_at`, `go_to`, `turn_by`, `stop`, `set_mode`. On macOS, once the Mac has bonded with
//! the gimbal it stops advertising and btleplug cannot find it; `swift/osmo-probe.swift` shows the CoreBluetooth fix
//! (`retrieveConnectedPeripherals`), which a native bridge must copy. See FINDINGS.md.
//!
//! The library sends only gimbal commands (set 0x04). Nothing that pairs, changes Wi-Fi, updates firmware or
//! calibrates is built in; [`Osmo::send_raw`] exists for probing and logs what it sent.

use btleplug::api::{Central, CentralEvent, Characteristic, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Manager, Peripheral};
use futures::StreamExt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use uuid::Uuid;

pub const SERVICE: &str = "0000fff0-0000-1000-8000-00805f9b34fb";
pub const NOTIFY: &str = "0000fff4-0000-1000-8000-00805f9b34fb";
pub const WRITE: &str = "0000fff5-0000-1000-8000-00805f9b34fb";

/// Who's who inside a DUML message.
pub const APP: u8 = 0x02;
pub const GIMBAL: u8 = 0x04;

/// Gimbal command set and the commands in it this library knows.
pub mod cmd {
    pub const SET_GIMBAL: u8 = 0x04;
    /// joystick-style channels, centre 1024
    pub const PWM: u8 = 0x01;
    pub const GET_POSITION: u8 = 0x02;
    /// gimbal → app: its own angles (Pocket 3: ~20 Hz)
    pub const PUSH_POSITION: u8 = 0x05;
    /// rate (mode 0x80)
    pub const SPEED: u8 = 0x0C;
    /// absolute (mode 0x05) or relative (mode 0x04) angle over a time. The OM7 ignores it (no reply, no move, even
    /// with a correct checksum, bench 2026-11-17); 0x0A answers "ok" and does not move either.
    pub const ANGLE: u8 = 0x14;
    /// 0 lock, 1 follow, 2 FPV
    pub const SET_MODE: u8 = 0x4C;
}

/// The last byte-but-one of a rotate payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rotate {
    Relative = 0x04,
    Absolute = 0x05,
    Speed = 0x80,
}

// ---- checksums: reflected CRC-8 (poly 0x31, init 0x77) over the 3-byte header, reflected CRC-16 (poly 0x1021,
// init 0x3692) over the WHOLE message before it, header and header checksum included (DJI's standard; found in a
// capture of DJI Mimo, 2026-11-17). The community projects seeded it 0xDF0C over the body only, which is the same thing
// for a 21-byte message (header 55 15 04 a9) and wrong for every other length: every non-speed command we sent before
// that had a bad checksum and was silently dropped.

const fn crc8_table() -> [u8; 256] {
    let mut t = [0u8; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u8;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0x8C } else { c >> 1 };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}

const fn crc16_table() -> [u16; 256] {
    let mut t = [0u16; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u16;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0x8408 } else { c >> 1 };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}

static CRC8: [u8; 256] = crc8_table();
static CRC16: [u16; 256] = crc16_table();

pub fn crc8(data: &[u8]) -> u8 {
    data.iter().fold(0x77, |c, &b| CRC8[(b ^ c) as usize])
}

pub fn crc16(data: &[u8]) -> u16 {
    data.iter().fold(0x3692, |c, &b| (c >> 8) ^ CRC16[((b as u16 ^ c) & 0xFF) as usize])
}

/// One DUML message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub sender: u8,
    pub receiver: u8,
    pub seq: u16,
    /// 0x00 push, 0x40 request wanting an answer, 0x80 bit set on answers
    pub attributes: u8,
    pub set: u8,
    pub cmd: u8,
    pub payload: Vec<u8>,
}

impl Message {
    pub fn to_gimbal(seq: u16, set: u8, cmd: u8, payload: Vec<u8>) -> Message {
        Message { sender: APP, receiver: GIMBAL, seq, attributes: 0x00, set, cmd, payload }
    }

    pub fn encode(&self) -> Vec<u8> {
        let len = 13 + self.payload.len();
        let raw_len = (len as u16 & 0x3FF) | (1 << 10);
        let mut out = vec![0x55, raw_len as u8, (raw_len >> 8) as u8];
        out.push(crc8(&out));
        out.extend([self.sender, self.receiver, self.seq as u8, (self.seq >> 8) as u8, self.attributes, self.set, self.cmd]);
        out.extend(&self.payload);
        let c = crc16(&out);
        out.extend(c.to_le_bytes());
        out
    }

    /// One line: `04→02 #12 a=00 04/05 [..]`.
    pub fn summary(&self) -> String {
        let hex: Vec<String> = self.payload.iter().map(|b| format!("{b:02x}")).collect();
        format!(
            "{:02x}→{:02x} #{} a={:02x} {:02x}/{:02x} [{}]",
            self.sender,
            self.receiver,
            self.seq,
            self.attributes,
            self.set,
            self.cmd,
            hex.join(" ")
        )
    }
}

/// Splits a byte stream into DUML messages. Messages may span notifications; junk before a 0x55 is dropped.
#[derive(Default)]
pub struct Parser {
    buf: Vec<u8>,
    /// messages thrown away for a bad checksum (counted, so a wrong CRC guess shows)
    pub bad: usize,
}

impl Parser {
    pub fn feed(&mut self, data: &[u8]) -> Vec<Message> {
        self.buf.extend_from_slice(data);
        let mut out = Vec::new();
        loop {
            match self.buf.iter().position(|&b| b == 0x55) {
                None => {
                    self.buf.clear();
                    break;
                }
                Some(i) => {
                    self.buf.drain(..i);
                }
            }
            if self.buf.len() < 4 {
                break;
            }
            let len = (u16::from_le_bytes([self.buf[1], self.buf[2]]) & 0x3FF) as usize;
            if len < 13 || crc8(&self.buf[..3]) != self.buf[3] {
                self.buf.remove(0);
                self.bad += 1;
                continue;
            }
            if self.buf.len() < len {
                break;
            }
            let raw: Vec<u8> = self.buf.drain(..len).collect();
            if crc16(&raw[..len - 2]) != u16::from_le_bytes([raw[len - 2], raw[len - 1]]) {
                self.bad += 1;
                continue;
            }
            out.push(Message {
                sender: raw[4],
                receiver: raw[5],
                seq: u16::from_le_bytes([raw[6], raw[7]]),
                attributes: raw[8],
                set: raw[9],
                cmd: raw[10],
                payload: raw[11..len - 2].to_vec(),
            });
        }
        out
    }
}

/// Rotate payload, tenths of a degree (or tenths of a degree per second for [`Rotate::Speed`]). `time` is in tenths of
/// a second for angle moves; for speed it is how long the gimbal keeps turning without a fresh command.
pub fn rotate_payload(yaw: i16, pitch: i16, roll: i16, mode: Rotate, time: u8) -> Vec<u8> {
    let mut p = Vec::with_capacity(8);
    p.extend(yaw.to_le_bytes());
    p.extend(roll.to_le_bytes());
    p.extend(pitch.to_le_bytes());
    p.push(mode as u8);
    p.push(time);
    p
}

/// The gimbal's own angles, if a push-position payload reads like the Pocket 3's (tenths of a degree).
#[derive(Clone, Copy, Debug)]
pub struct Angles {
    pub pitch: f64,
    pub roll: f64,
    pub yaw: f64,
}

pub fn decode_angles(payload: &[u8]) -> Option<Angles> {
    if payload.len() < 6 {
        return None;
    }
    let v = |i: usize| i16::from_le_bytes([payload[i], payload[i + 1]]) as f64 / 10.0;
    Some(Angles { pitch: v(0), roll: v(2), yaw: v(4) })
}

/// The OM7's own report (04/05, once a second after the handshake is answered; bench test 2026-11-17): the clamp's
/// attitude as a unit quaternion (w, x, y, z; floats at bytes 24..40) in a north-east-down world whose north is
/// wherever the gimbal woke up, plus its joint angles in degrees (pan at 4, tilt at 20, roll at 22, int16 tenths).
/// With the clamp on its side the joints move together for one commanded axis, so aim by the quaternion and guard the
/// limits by the joints.
#[derive(Clone, Copy, Debug)]
pub struct Attitude {
    pub q: [f64; 4],
    pub pan: f64,
    pub tilt: f64,
    pub roll: f64,
}

impl Attitude {
    pub fn decode(payload: &[u8]) -> Option<Attitude> {
        if payload.len() < 40 {
            return None;
        }
        let v = |i: usize| i16::from_le_bytes([payload[i], payload[i + 1]]) as f64 / 10.0;
        let f = |i: usize| f32::from_le_bytes([payload[i], payload[i + 1], payload[i + 2], payload[i + 3]]) as f64;
        Some(Attitude { q: [f(24), f(28), f(32), f(36)], pan: v(4), tilt: v(20), roll: v(22) })
    }

    /// Body vector to world (north, east, down).
    pub fn rotate(&self, v: [f64; 3]) -> [f64; 3] {
        let [w, x, y, z] = self.q;
        let t = [2.0 * (y * v[2] - z * v[1]), 2.0 * (z * v[0] - x * v[2]), 2.0 * (x * v[1] - y * v[0])];
        [
            v[0] + w * t[0] + (y * t[2] - z * t[1]),
            v[1] + w * t[1] + (z * t[0] - x * t[2]),
            v[2] + w * t[2] + (x * t[1] - y * t[0]),
        ]
    }

    /// The joints (pan unwrapped onto the OM7's stops, tilt, roll), degrees.
    pub fn joints(&self) -> [f64; 3] {
        [om7_pan(self.pan), self.tilt, self.roll]
    }

    /// The rotation between two attitudes, degrees.
    pub fn angle_to(&self, other: &Attitude) -> f64 {
        let d: f64 = self.q.iter().zip(other.q.iter()).map(|(a, b)| a * b).sum();
        (2.0 * d.abs().min(1.0).acos()).to_degrees()
    }

    /// Euler angles (roll, pitch, yaw), degrees, aerospace order.
    pub fn euler(&self) -> (f64, f64, f64) {
        let [w, x, y, z] = self.q;
        let roll = (2.0 * (w * x + y * z)).atan2(1.0 - 2.0 * (x * x + y * y));
        let pitch = (2.0 * (w * y - z * x)).clamp(-1.0, 1.0).asin();
        let yaw = (2.0 * (w * z + x * y)).atan2(1.0 - 2.0 * (y * y + z * z));
        (roll.to_degrees(), pitch.to_degrees(), yaw.to_degrees())
    }

    /// The Euler angle that `axis`'s speed command turns (pan = yaw, tilt = pitch, roll = roll), degrees.
    pub fn along(&self, axis: Axis) -> f64 {
        let (r, p, y) = self.euler();
        match axis {
            Axis::Pan => y,
            Axis::Tilt => p,
            Axis::Roll => r,
        }
    }

    /// Where the clamp's long axis points (end +1 = body +Y, the top when it went portrait the usual way): azimuth
    /// clockwise from the gimbal's north and inclination up from level, degrees.
    pub fn clamp_axis(&self, end: f64) -> (f64, f64) {
        let [n, e, d] = self.rotate([0.0, end, 0.0]);
        (e.atan2(n).to_degrees(), (-d).clamp(-1.0, 1.0).asin().to_degrees())
    }
}

pub fn wrap180(a: f64) -> f64 {
    let x = a.rem_euclid(360.0);
    if x > 180.0 { x - 360.0 } else { x }
}

/// The OM7's pan stops, from its centre, with the clamp in portrait (range test 2026-11-17): +92.5° one way, −202° the
/// other. Its pan joint reports wrap at ±180, so a reading past the + stop is really the far side of the − stop.
pub const OM7_PAN_STOPS: (f64, f64) = (-202.0, 92.5);

/// Safe joint windows (degrees, as the gimbal reports its joints), measured with the clamp in portrait (range test
/// 2026-11-17) less a margin. Pan and tilt end at hard stops (a stall, harmless); the roll has NO stop: past its window
/// the clamp flips over violently (it swung 110–170° in under a second and nearly knocked the gimbal over), so the roll
/// window is a hard rule. Measured clean: pan −202…+86, tilt −10…+30, roll 63…219.
#[derive(Clone, Copy, Debug)]
pub struct JointLimits {
    pub pan: (f64, f64),
    pub tilt: (f64, f64),
    pub roll: (f64, f64),
}

pub const OM7_PORTRAIT: JointLimits = JointLimits { pan: (-195.0, 80.0), tilt: (-7.0, 27.0), roll: (75.0, 205.0) };

impl JointLimits {
    /// How far (degrees) each joint is inside its window; negative = outside.
    pub fn room(&self, j: [f64; 3]) -> [f64; 3] {
        let w = [self.pan, self.tilt, self.roll];
        let mut out = [0.0; 3];
        for k in 0..3 {
            out[k] = (j[k] - w[k].0).min(w[k].1 - j[k]);
        }
        out
    }
}

/// The pan joint unwrapped onto the OM7's stops.
pub fn om7_pan(joint: f64) -> f64 {
    if joint > OM7_PAN_STOPS.1 + 5.0 { joint - 360.0 } else { joint }
}

/// One gimbal axis, for commands that drive one at a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Pan,
    Tilt,
    Roll,
}

impl Axis {
    /// (yaw, pitch, roll) rates for turning this axis at `dps`.
    pub fn rates(self, dps: f64) -> (f64, f64, f64) {
        match self {
            Axis::Pan => (dps, 0.0, 0.0),
            Axis::Tilt => (0.0, dps, 0.0),
            Axis::Roll => (0.0, 0.0, dps),
        }
    }
}

/// Sweep plans for an X1 in the Osmo's clamp. No Bluetooth here: a plan is a list of relative turns, testable and
/// usable by any transport.
///
/// Measured on the OM7 (FINDINGS.md): only speed commands work and the gimbal reports no angles, so every move is a
/// rate held for a time (rates land within ~3 %); the X1's own compass and clino say where each shot really went.
/// Ranges from rest: pan ~287° between stops, tilt +34° / −24°, roll clean to −164° but unstable past about +65°
/// (it swings to its landscape position). So plans roll negative only.
pub mod sweep {
    /// Turn rate used for every move (°/s): slow enough to stop clean, measured accurate.
    pub const RATE_DPS: f64 = 30.0;
    /// Pause after a move before shooting (s).
    pub const SETTLE_S: f64 = 0.8;
    /// Furthest negative roll a plan uses (°); the stop is at about −164°.
    pub const ROLL_LIMIT: f64 = -150.0;

    /// How the X1 sits in the clamp.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Mount {
        /// Laser out of the clamp's top: at rest it points straight up. Roll swings it across the passage (a ring),
        /// pan turns which wall that ring reaches.
        LaserUp,
    }

    /// One relative turn (degrees), and whether to shoot once it has settled.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Move {
        pub yaw: f64,
        pub pitch: f64,
        pub roll: f64,
        pub shoot: bool,
    }

    impl Move {
        /// Time to make the move at [`RATE_DPS`] on the axis that moves furthest.
        pub fn seconds(&self) -> f64 {
            self.yaw.abs().max(self.pitch.abs()).max(self.roll.abs()) / RATE_DPS
        }
    }

    fn turn(yaw: f64, roll: f64, shoot: bool) -> Move {
        Move { yaw, pitch: 0.0, roll, shoot }
    }

    /// A cross-section ring (LRUD sweep). Pan only reaches about ±143° from the gimbal's centre, so the gimbal is set
    /// down with its centre facing a WALL (across the passage): the ring pans −90° to look along the passage, shoots
    /// straight up, rolls down one wall in `step`° steps to [`ROLL_LIMIT`], rolls back, pans 180° to look the other way
    /// along the passage (same cross-section plane, other wall on the roll side), repeats, and pans back. Ends where it
    /// began. Leaves a wedge of about 30° straight down unreached (the floor: tape or a hand shot).
    pub fn ring(mount: Mount, step: f64) -> Vec<Move> {
        let Mount::LaserUp = mount;
        let n = (ROLL_LIMIT.abs() / step).floor() as usize;
        let mut plan = vec![turn(-90.0, 0.0, true)];
        for side in 0..2 {
            for _ in 0..n {
                plan.push(turn(0.0, -step, true));
            }
            plan.push(turn(0.0, step * n as f64, false));
            plan.push(turn(if side == 0 { 180.0 } else { -90.0 }, 0.0, false));
        }
        plan
    }

    /// Total time a plan takes, without the shots themselves.
    pub fn duration(plan: &[Move]) -> f64 {
        plan.iter().map(|m| m.seconds() + SETTLE_S).sum()
    }
}

#[derive(Debug)]
pub enum Error {
    NoBluetooth(String),
    NotFound,
    Timeout,
    Link(String),
    /// The gimbal swung far past what was asked (a roll past its short side flips the clamp): where it ended up.
    Snapped(Attitude),
    /// A guarded move stopped at the edge of the safe joint window (or against a hard stop): where it is.
    AtLimit(Attitude),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoBluetooth(e) => write!(f, "no Bluetooth: {e}"),
            Error::NotFound => write!(f, "no Osmo found (is it on, and not connected to a phone?)"),
            Error::Timeout => write!(f, "timed out"),
            Error::Link(e) => write!(f, "link: {e}"),
            Error::Snapped(_) => write!(f, "the gimbal snapped over"),
            Error::AtLimit(_) => write!(f, "at the edge of the gimbal's safe range"),
        }
    }
}

/// A message as it arrived, with when and on which characteristic.
pub struct Heard {
    pub at: Instant,
    pub on: Uuid,
    pub message: Message,
}

pub struct Osmo {
    peripheral: Peripheral,
    write: Characteristic,
    pub heard: mpsc::UnboundedReceiver<Heard>,
    pub name: String,
    pub characteristics: Vec<String>,
    seq: u16,
    latest: Arc<Mutex<Option<(Instant, Attitude)>>>,
}

fn tenths(deg: f64) -> i16 {
    (deg * 10.0).round().clamp(-1800.0, 1800.0) as i16
}

fn uuid(s: &str) -> Uuid {
    Uuid::parse_str(s).expect("a valid UUID constant")
}

fn looks_like_osmo(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    n.starts_with("OM") || n.contains("OSMO") || n.starts_with("DJI")
}

/// Everything heard while scanning: (name, rssi, advertises fff0).
pub async fn scan(wait: Duration) -> Result<Vec<(String, Option<i16>, bool)>, Error> {
    let adapter = adapter().await?;
    adapter.start_scan(ScanFilter::default()).await.map_err(|e| Error::NoBluetooth(e.to_string()))?;
    tokio::time::sleep(wait).await;
    let _ = adapter.stop_scan().await;
    let mut out = Vec::new();
    for p in adapter.peripherals().await.map_err(|e| Error::NoBluetooth(e.to_string()))? {
        if let Ok(Some(props)) = p.properties().await {
            let fff0 = props.services.contains(&uuid(SERVICE));
            out.push((props.local_name.unwrap_or_default(), props.rssi, fff0));
        }
    }
    out.sort_by(|a, b| b.2.cmp(&a.2).then(b.1.cmp(&a.1)));
    Ok(out)
}

async fn adapter() -> Result<btleplug::platform::Adapter, Error> {
    let manager = Manager::new().await.map_err(|e| Error::NoBluetooth(e.to_string()))?;
    manager
        .adapters()
        .await
        .map_err(|e| Error::NoBluetooth(e.to_string()))?
        .into_iter()
        .next()
        .ok_or_else(|| Error::NoBluetooth("no adapter".into()))
}

impl Osmo {
    /// Finds the first gimbal (name like `OM…`/`Osmo…`/`DJI…`, or advertising `fff0`) and connects.
    pub async fn find(wait: Duration) -> Result<Osmo, Error> {
        let adapter = adapter().await?;
        let mut events = adapter.events().await.map_err(|e| Error::NoBluetooth(e.to_string()))?;
        adapter.start_scan(ScanFilter::default()).await.map_err(|e| Error::NoBluetooth(e.to_string()))?;
        let deadline = Instant::now() + wait;
        let found = loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break None;
            }
            let Ok(Some(ev)) = tokio::time::timeout(left, events.next()).await else { break None };
            let id = match ev {
                CentralEvent::DeviceDiscovered(id) | CentralEvent::DeviceUpdated(id) => id,
                _ => continue,
            };
            let Ok(p) = adapter.peripheral(&id).await else { continue };
            let Ok(Some(props)) = p.properties().await else { continue };
            let name = props.local_name.clone().unwrap_or_default();
            if looks_like_osmo(&name) || props.services.contains(&uuid(SERVICE)) {
                break Some((p, name));
            }
        };
        let _ = adapter.stop_scan().await;
        let (peripheral, name) = found.ok_or(Error::NotFound)?;
        Osmo::open(peripheral, name).await
    }

    async fn open(peripheral: Peripheral, name: String) -> Result<Osmo, Error> {
        let link = |e: btleplug::Error| Error::Link(e.to_string());
        tokio::time::timeout(Duration::from_secs(15), peripheral.connect())
            .await
            .map_err(|_| Error::Timeout)?
            .map_err(link)?;
        peripheral.discover_services().await.map_err(link)?;
        let chars = peripheral.characteristics();
        let characteristics = chars.iter().map(|c| format!("{} {} {:?}", c.service_uuid, c.uuid, c.properties)).collect();
        let find = |u: &str| chars.iter().find(|c| c.uuid == uuid(u)).cloned();
        let write = find(WRITE).ok_or_else(|| Error::Link("no fff5".into()))?;
        // listen everywhere the gimbal might talk: fff4, and fff5 when it notifies
        for c in chars.iter().filter(|c| c.uuid == uuid(NOTIFY) || c.uuid == uuid(WRITE)) {
            if c.properties.intersects(btleplug::api::CharPropFlags::NOTIFY | btleplug::api::CharPropFlags::INDICATE) {
                peripheral.subscribe(c).await.map_err(link)?;
            }
        }
        let mut stream = peripheral.notifications().await.map_err(link)?;
        let (tx, heard) = mpsc::unbounded_channel();
        let latest: Arc<Mutex<Option<(Instant, Attitude)>>> = Arc::new(Mutex::new(None));
        let (p2, w2, l2) = (peripheral.clone(), write.clone(), latest.clone());
        tokio::spawn(async move {
            let mut parsers: std::collections::HashMap<Uuid, Parser> = Default::default();
            while let Some(n) = stream.next().await {
                let at = Instant::now();
                for message in parsers.entry(n.uuid).or_default().feed(&n.value) {
                    // the handshake (as DJI Mimo answers it): without this the gimbal never reports its attitude
                    if message.set == 0x00 && message.cmd == 0x00 && message.attributes & 0x20 != 0 {
                        let ack = Message {
                            sender: APP,
                            receiver: message.sender,
                            seq: message.seq,
                            attributes: 0xC0,
                            set: 0x00,
                            cmd: 0x00,
                            payload: vec![0x00, message.payload.first().copied().unwrap_or(0)],
                        };
                        for chunk in ack.encode().chunks(20) {
                            let _ = p2.write(&w2, chunk, WriteType::WithoutResponse).await;
                        }
                    }
                    if message.set == cmd::SET_GIMBAL && message.cmd == cmd::PUSH_POSITION {
                        if let Some(a) = Attitude::decode(&message.payload) {
                            *l2.lock().unwrap() = Some((at, a));
                        }
                    }
                    let _ = tx.send(Heard { at, on: n.uuid, message });
                }
            }
        });
        Ok(Osmo { peripheral, write, heard, name, characteristics, seq: 1, latest })
    }

    fn next_seq(&mut self) -> u16 {
        let s = self.seq;
        self.seq = self.seq.wrapping_add(1).max(1);
        s
    }

    /// Sends one message to the gimbal; returns what went on the wire.
    pub async fn send(&mut self, set: u8, cmd: u8, payload: Vec<u8>, attributes: u8) -> Result<Vec<u8>, Error> {
        let mut m = Message::to_gimbal(self.next_seq(), set, cmd, payload);
        m.attributes = attributes;
        let bytes = m.encode();
        self.send_raw(&bytes).await?;
        Ok(bytes)
    }

    pub async fn send_raw(&mut self, bytes: &[u8]) -> Result<(), Error> {
        // 20-byte chunks fit any link; a rotate is 21 bytes, so it goes in two
        for chunk in bytes.chunks(20) {
            self.peripheral
                .write(&self.write, chunk, WriteType::WithoutResponse)
                .await
                .map_err(|e| Error::Link(e.to_string()))?;
        }
        Ok(())
    }

    pub async fn rotate(&mut self, yaw: i16, pitch: i16, mode: Rotate, time: u8) -> Result<Vec<u8>, Error> {
        let set = cmd::SET_GIMBAL;
        let c = if mode == Rotate::Speed { cmd::SPEED } else { cmd::ANGLE };
        self.send(set, c, rotate_payload(yaw, pitch, 0, mode, time), 0x00).await
    }

    pub async fn stop(&mut self) -> Result<Vec<u8>, Error> {
        self.rotate(0, 0, Rotate::Speed, 1).await
    }

    /// Turns at a rate (degrees per second) for `for_how_long`, refreshing every 100 ms, then stops.
    pub async fn turn_at(&mut self, yaw_dps: f64, pitch_dps: f64, for_how_long: Duration) -> Result<(), Error> {
        self.turn_rates(yaw_dps, pitch_dps, 0.0, for_how_long).await
    }

    /// Like [`Osmo::turn_at`] with roll too (roll spins the clamp round the camera axis).
    pub async fn turn_rates(&mut self, yaw_dps: f64, pitch_dps: f64, roll_dps: f64, for_how_long: Duration) -> Result<(), Error> {
        let end = Instant::now() + for_how_long;
        while Instant::now() < end {
            let p = rotate_payload(tenths(yaw_dps), tenths(pitch_dps), tenths(roll_dps), Rotate::Speed, 2);
            let seq = self.next_seq();
            self.send_raw(&Message::to_gimbal(seq, cmd::SET_GIMBAL, cmd::SPEED, p).encode()).await?;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        self.stop().await.map(|_| ())
    }

    /// Carries out one [`sweep::Move`] (the turn only; shooting is the caller's, after `settle`).
    pub async fn perform(&mut self, m: &sweep::Move) -> Result<(), Error> {
        let secs = m.seconds();
        if secs > 0.0 {
            let (y, p, r) = (m.yaw / secs, m.pitch / secs, m.roll / secs);
            self.turn_rates(y, p, r, Duration::from_secs_f64(secs)).await?;
        }
        tokio::time::sleep(Duration::from_secs_f64(sweep::SETTLE_S)).await;
        Ok(())
    }

    /// Goes to an absolute angle (degrees, relative to the gimbal's own centre) over `seconds`.
    pub async fn go_to(&mut self, yaw: f64, pitch: f64, seconds: f64) -> Result<(), Error> {
        self.rotate(tenths(yaw), tenths(pitch), Rotate::Absolute, (seconds * 10.0).clamp(1.0, 255.0) as u8).await.map(|_| ())
    }

    /// Turns by an angle (degrees) over `seconds`.
    pub async fn turn_by(&mut self, yaw: f64, pitch: f64, seconds: f64) -> Result<(), Error> {
        self.rotate(tenths(yaw), tenths(pitch), Rotate::Relative, (seconds * 10.0).clamp(1.0, 255.0) as u8).await.map(|_| ())
    }

    /// 0 lock, 1 follow, 2 FPV.
    pub async fn set_mode(&mut self, mode: u8) -> Result<(), Error> {
        self.send(cmd::SET_GIMBAL, cmd::SET_MODE, vec![mode, 0], 0x00).await.map(|_| ())
    }

    /// Everything heard within `quiet` of the last message (or `max` total).
    pub async fn drain(&mut self, quiet: Duration, max: Duration) -> Vec<Heard> {
        let end = Instant::now() + max;
        let mut out = Vec::new();
        loop {
            let left = end.saturating_duration_since(Instant::now()).min(quiet);
            if left.is_zero() {
                break;
            }
            match tokio::time::timeout(left, self.heard.recv()).await {
                Ok(Some(h)) => out.push(h),
                _ => break,
            }
        }
        out
    }

    /// The gimbal's latest attitude report and when it came (1 a second once the handshake is answered).
    pub fn attitude(&self) -> Option<(Instant, Attitude)> {
        *self.latest.lock().unwrap()
    }

    /// Waits for an attitude report that arrived after `after` (a report is sent once a second).
    pub async fn fresh_attitude(&mut self, after: Instant, wait: Duration) -> Option<Attitude> {
        let end = Instant::now() + wait;
        while Instant::now() < end {
            if let Some((at, a)) = self.attitude() {
                if at > after {
                    return Some(a);
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        None
    }

    /// Turns `axis` by `deg` (its [`Attitude::along`] angle) without letting any joint leave `limits`: it moves in
    /// steps of at most `dps` for one second, reads the next report, and before each step predicts every joint from
    /// how much the joints moved per degree on the step before (one axis moves several joints when the clamp is on its
    /// side). A step that would leave the window is shortened; if there is no room at all it stops with
    /// [`Error::AtLimit`]. Returns the attitude where it ended.
    pub async fn guarded_turn(&mut self, axis: Axis, deg: f64, dps: f64, limits: &JointLimits) -> Result<Attitude, Error> {
        let mut now = self.fresh_attitude(Instant::now(), Duration::from_secs(3)).await.ok_or(Error::Timeout)?;
        let start = now.along(axis);
        let dir = deg.signum();
        // joint change per degree of the axis; before any step, assume its own joint at 1.3 : 1 (tilt runs ~1.25, and
        // backwards: pitching up lowers the tilt joint)
        let mut per = [0.0f64; 3];
        match axis {
            Axis::Pan => per[0] = 1.3,
            Axis::Tilt => per[1] = -1.3,
            Axis::Roll => per[2] = 1.3,
        }
        let mut per_known = false;
        for _ in 0..64 {
            let done = wrap180(now.along(axis) - start) * dir;
            let left = deg.abs() - done;
            if left < 1.0 {
                return Ok(now);
            }
            let j = now.joints();
            // the furthest this step may go before some joint leaves its window
            let mut allowed = left.min(dps.abs());
            for k in 0..3 {
                let rate = per[k].abs() * if per_known { 1.2 } else { 1.0 };
                if rate > 1e-3 {
                    // only joints heading toward their nearer edge limit the step
                    let w = [limits.pan, limits.tilt, limits.roll][k];
                    let heading = per[k] * dir;
                    let edge_room = if heading > 0.0 { w.1 - j[k] } else { j[k] - w.0 };
                    allowed = allowed.min(edge_room.max(0.0) / rate);
                }
            }
            if allowed < 1.0 {
                return Err(Error::AtLimit(now));
            }
            let (y, p, r) = axis.rates(dir * dps.abs());
            self.turn_rates(y, p, r, Duration::from_secs_f64(allowed / dps.abs())).await?;
            tokio::time::sleep(Duration::from_millis(300)).await;
            let next = self.fresh_attitude(Instant::now(), Duration::from_secs(3)).await.ok_or(Error::Timeout)?;
            let turned = wrap180(next.along(axis) - now.along(axis)) * dir;
            if turned > 2.0 {
                let (a, b) = (now.joints(), next.joints());
                for k in 0..3 {
                    per[k] = (b[k] - a[k]) / turned * dir;
                }
                per_known = true;
            } else if turned.abs() <= 2.0 && allowed > 5.0 {
                // asked to move and it did not: a hard stop
                return Err(Error::AtLimit(next));
            }
            now = next;
        }
        Ok(now)
    }

    /// Turns one axis at `dps` until the gimbal stops following (a motor limit) or `max_deg` of travel; every second
    /// it checks the attitude. Returns the attitude at the stop and the travel the gimbal reported.
    /// Pan and tilt only: they end at hard stops. The roll has no stop, it flips over (see [`JointLimits`]); use
    /// [`Osmo::guarded_turn`] for it.
    pub async fn run_to_limit(&mut self, axis: Axis, dps: f64, max_deg: f64) -> Result<(Attitude, f64), Error> {
        if axis == Axis::Roll {
            return Err(Error::Link("run_to_limit refuses the roll: it flips over instead of stopping".into()));
        }
        let start = Instant::now();
        let mut last = self.fresh_attitude(start, Duration::from_secs(3)).await.ok_or(Error::Timeout)?;
        let mut travel = 0.0;
        loop {
            let (y, p, r) = axis.rates(dps);
            self.turn_rates(y, p, r, Duration::from_millis(1000)).await?;
            let now = self.fresh_attitude(Instant::now(), Duration::from_secs(3)).await.ok_or(Error::Timeout)?;
            let moved = last.angle_to(&now);
            travel += moved;
            if std::env::var_os("OSMO_VERBOSE").is_some() {
                println!("  step: moved {moved:.1}°, joints {:.1} {:.1} {:.1}", now.pan, now.tilt, now.roll);
            }
            last = now;
            // far more than asked: it snapped over (the roll's short side flips to landscape); stop and say so
            if moved > dps.abs() * 1.8 {
                self.stop().await?;
                return Err(Error::Snapped(now));
            }
            // under a fifth of what was asked: something stopped it
            if moved < dps.abs() * 0.2 || travel >= max_deg {
                return Ok((now, travel));
            }
        }
    }

    /// Turns `axis` until its angle ([`Attitude::along`]) is `target` within `tol` degrees: a timed turn, then the next
    /// report, then a correction (up to `tries`). `via` +1 / −1 makes the first turn go that way round (the pan has
    /// stops: the short way can run into one); 0 takes the short way. Returns the final attitude.
    pub async fn bring_to(&mut self, axis: Axis, target: f64, dps: f64, tol: f64, tries: usize, via: f64) -> Result<Attitude, Error> {
        let mut now = self.fresh_attitude(Instant::now(), Duration::from_secs(3)).await.ok_or(Error::Timeout)?;
        for k in 0..tries {
            let d = target - now.along(axis);
            let err = if k == 0 && via > 0.0 {
                d.rem_euclid(360.0)
            } else if k == 0 && via < 0.0 {
                -(-d).rem_euclid(360.0)
            } else {
                wrap180(d)
            };
            if err.abs() < tol {
                break;
            }
            let (y, p, r) = axis.rates(err.signum() * dps.abs());
            // timed turns land ~8 % short; aim a little long, the next pass trims
            self.turn_rates(y, p, r, Duration::from_secs_f64(err.abs() / dps.abs() * 1.04)).await?;
            tokio::time::sleep(Duration::from_millis(400)).await;
            now = self.fresh_attitude(Instant::now(), Duration::from_secs(3)).await.ok_or(Error::Timeout)?;
        }
        Ok(now)
    }

    pub async fn disconnect(self) {
        let _ = self.peripheral.disconnect().await;
    }
}

#[cfg(test)]
mod attitude_tests {
    use super::*;
    fn report(q: [f32; 4], pan: i16, tilt: i16, roll: i16) -> Vec<u8> {
        let mut p = vec![0u8; 49];
        p[4..6].copy_from_slice(&pan.to_le_bytes());
        p[20..22].copy_from_slice(&tilt.to_le_bytes());
        p[22..24].copy_from_slice(&roll.to_le_bytes());
        for (k, c) in q.iter().enumerate() {
            p[24 + 4 * k..28 + 4 * k].copy_from_slice(&c.to_le_bytes());
        }
        p
    }
    #[test]
    fn bench_rest_and_roll() {
        // bench 2026-11-17: at rest the top points up; after roll +30 (joint 80.6 -> 108.3) it leans ~27
        let rest = Attitude::decode(&report([0.7067, -0.7064, 0.0354, -0.016], -1, 219, 941)).unwrap();
        assert!((rest.clamp_axis(1.0).1 - 88.5).abs() < 1.0);
        assert!((rest.roll - 94.1).abs() < 1e-9 && (rest.tilt - 21.9).abs() < 1e-9);
        let rolled = Attitude::decode(&report([0.8044, -0.5005, -0.1525, 0.2815], 421, 163, 1083)).unwrap();
        assert!((rolled.clamp_axis(1.0).1 - 63.0).abs() < 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // reveng samples from om-research (OM2, Mobile SDK traffic): body bytes then CRC-16 LE
    #[test]
    fn crc16_matches_captured_om2_messages() {
        for hex in [
            "02031c004003f8ef979092ef3963f55eaa",
            "02044700400414c20100002000050feb63",
            "02045e004004143efe00002000050fa2e7",
            "02047300400414c20100002000050f2f9a",
        ] {
            let b: Vec<u8> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect();
            // these are 21-byte rotates: the header in front is always 55 15 04 a9
            let mut whole = vec![0x55, 0x15, 0x04, 0xa9];
            whole.extend(&b);
            let (body, crc) = whole.split_at(whole.len() - 2);
            assert_eq!(crc16(body), u16::from_le_bytes([crc[0], crc[1]]), "{hex}");
        }
    }

    // whole messages of other lengths, sent by DJI Mimo to an Osmo Mobile 7 (PacketLogger capture 2026-11-17)
    #[test]
    fn crc16_matches_mimo_messages_of_every_length() {
        for hex in ["550f04a202f050bd40002b0400e5d5", "550d0433021d51bd40ee06ca00", "55110492021c52bd40ee2f0000085a95d5"] {
            let b: Vec<u8> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect();
            let mut p = Parser::default();
            assert_eq!(p.feed(&b).len(), 1, "{hex}");
            assert_eq!(p.bad, 0, "{hex}");
        }
    }

    #[test]
    fn rotate_header_matches_om_research() {
        // om-research hard-codes this header for a 21-byte rotate
        let m = Message::to_gimbal(1, cmd::SET_GIMBAL, cmd::SPEED, rotate_payload(0, 0, 0, Rotate::Speed, 10));
        assert_eq!(&m.encode()[..4], &[0x55, 0x15, 0x04, 0xa9]);
    }

    #[test]
    fn ring_returns_home_and_stays_inside_the_roll_range() {
        let plan = sweep::ring(sweep::Mount::LaserUp, 15.0);
        let (mut yaw, mut roll, mut lowest) = (0.0, 0.0, 0.0f64);
        for m in &plan {
            yaw += m.yaw;
            roll += m.roll;
            lowest = lowest.min(roll);
            assert!(roll <= 0.0, "a plan never rolls positive");
        }
        assert_eq!((yaw, roll), (0.0, 0.0));
        assert!(lowest >= sweep::ROLL_LIMIT);
        // straight up once, then 10 shots down each wall
        assert_eq!(plan.iter().filter(|m| m.shoot).count(), 21);
        let mut pan = 0.0f64;
        for m in &plan {
            pan += m.yaw;
            assert!(pan.abs() <= 90.0, "pan stays inside the gimbal's reach");
        }
    }

    #[test]
    fn parser_round_trips_and_spans_chunks() {
        let m = Message::to_gimbal(7, cmd::SET_GIMBAL, cmd::ANGLE, rotate_payload(450, -100, 0, Rotate::Absolute, 15));
        let bytes = m.encode();
        let mut p = Parser::default();
        assert!(p.feed(&bytes[..9]).is_empty());
        let got = p.feed(&bytes[9..]);
        assert_eq!(got, vec![m]);
        assert_eq!(p.bad, 0);
    }
}
