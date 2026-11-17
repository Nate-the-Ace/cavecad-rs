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
    /// absolute (mode 0x05) or relative (mode 0x04) angle over a time
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
// init 0xDF0C) over everything after the header checksum.

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
    data.iter().fold(0xDF0C, |c, &b| (c >> 8) ^ CRC16[((b as u16 ^ c) & 0xFF) as usize])
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
        let body_start = out.len();
        out.extend([self.sender, self.receiver, self.seq as u8, (self.seq >> 8) as u8, self.attributes, self.set, self.cmd]);
        out.extend(&self.payload);
        let c = crc16(&out[body_start..]);
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
            if crc16(&raw[4..len - 2]) != u16::from_le_bytes([raw[len - 2], raw[len - 1]]) {
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
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoBluetooth(e) => write!(f, "no Bluetooth: {e}"),
            Error::NotFound => write!(f, "no Osmo found (is it on, and not connected to a phone?)"),
            Error::Timeout => write!(f, "timed out"),
            Error::Link(e) => write!(f, "link: {e}"),
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
        tokio::spawn(async move {
            let mut parsers: std::collections::HashMap<Uuid, Parser> = Default::default();
            while let Some(n) = stream.next().await {
                let at = Instant::now();
                for message in parsers.entry(n.uuid).or_default().feed(&n.value) {
                    if tx.send(Heard { at, on: n.uuid, message }).is_err() {
                        return;
                    }
                }
            }
        });
        Ok(Osmo { peripheral, write, heard, name, characteristics, seq: 1 })
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

    pub async fn disconnect(self) {
        let _ = self.peripheral.disconnect().await;
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
            let (body, crc) = b.split_at(b.len() - 2);
            assert_eq!(crc16(body), u16::from_le_bytes([crc[0], crc[1]]), "{hex}");
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
