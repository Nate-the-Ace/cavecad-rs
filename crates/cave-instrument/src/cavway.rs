//! The Cavway X1's wire format: pure functions and constants, no Bluetooth, so every byte can be tested.
//! Written from the vendor's public "Cavway X1 Communication Protocol" (v1.0), the user manual (Rev. C) and a
//! reading of their Windows app's shot parser, as set out in docs/CAVWAY-X1.md; nothing here is copied from
//! the GPL code. Where two sources disagree the real instrument decides (the list is at the end of
//! docs/CAVWAY-X1.md).
//!
//! The X1 speaks the Nordic UART service: it pushes each stored shot on the notify characteristic, and wants
//! every one acknowledged (the shot's flag byte OR 0x55) before it stops offering it. Everything written to it
//! is framed: `data:`, a length byte, the payload, CR LF.

use crate::{Mark, js_round};

/// Nordic UART service: the X1's only service.
pub const SERVICE: &str = "6e400001-b5a3-f393-e0a9-e50e24dcca9e";
/// App to X1.
pub const WRITE: &str = "6e400002-b5a3-f393-e0a9-e50e24dcca9e";
/// X1 to app (notify).
pub const NOTIFY: &str = "6e400003-b5a3-f393-e0a9-e50e24dcca9e";

/// Fires the laser and stores a shot, as a press of DIST does.
pub const CMD_TRIGGER: u8 = 0x38;
/// Laser on.
pub const CMD_LASER_ON: u8 = 0x36;
/// Laser off.
pub const CMD_LASER_OFF: u8 = 0x37;

/// The most a single query returns.
pub const MAX_READ: usize = 124;
/// Bytes in a full shot packet.
pub const PACKET_BYTES: usize = 64;
/// The short form the BLE link may send (UNVERIFIED which: the PDF says 64, the TopoDroid driver takes 33).
pub const SHORT_PACKET_BYTES: usize = 33;

const HEADER: [u8; 5] = [0x64, 0x61, 0x74, 0x61, 0x3a]; // "data:"
const FOOTER: [u8; 2] = [0x0d, 0x0a];

/// Wraps a payload the way the X1 expects it: `data:`, length, payload, CR LF.
pub fn frame(payload: &[u8]) -> Vec<u8> {
    let mut out = HEADER.to_vec();
    out.push(payload.len() as u8);
    out.extend_from_slice(payload);
    out.extend_from_slice(&FOOTER);
    out
}

/// The frame for a single-byte command.
pub fn command(cmd: u8) -> Vec<u8> {
    frame(&[cmd])
}

/// The acknowledgement for a shot packet: its flag byte OR 0x55, framed.
pub fn ack_frame(flag: u8) -> Vec<u8> {
    frame(&[flag | 0x55])
}

/// A memory query: `3d`, the address (low, high), how many bytes (at most 124).
pub fn query_frame(address: u16, length: usize) -> Option<Vec<u8>> {
    (1..=MAX_READ).contains(&length).then(|| {
        frame(&[
            0x3d,
            (address & 0xff) as u8,
            (address >> 8) as u8,
            length as u8,
        ])
    })
}

/// What a memory query comes back as.
#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    /// `3d`, or `3e` for a write echo.
    pub command: u8,
    /// The address read.
    pub address: u16,
    /// The bytes.
    pub data: Vec<u8>,
}

/// Parses a reply to a memory query.
pub fn parse_reply(b: &[u8]) -> Option<Reply> {
    if b.len() < 4 || (b[0] != 0x3d && b[0] != 0x3e) {
        return None;
    }
    let length = usize::from(b[3]);
    if b.len() < 4 + length {
        return None;
    }
    Some(Reply {
        command: b[0],
        address: u16::from(b[1]) | (u16::from(b[2]) << 8),
        data: b[4..4 + length].to_vec(),
    })
}

const MARKS: [Mark; 4] = [Mark::Feature, Mark::Ridge, Mark::Backsight, Mark::Generic];

/// Which accuracy check complained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// Gravity magnitude.
    AbsG,
    /// Magnetic magnitude.
    AbsM,
    /// Dip.
    Dip,
    /// Angle.
    Angle,
}

impl ErrorKind {
    /// The name used in the log.
    pub fn name(self) -> &'static str {
        match self {
            ErrorKind::AbsG => "absG",
            ErrorKind::AbsM => "absM",
            ErrorKind::Dip => "dip",
            ErrorKind::Angle => "angle",
        }
    }
}

/// One accuracy complaint from the X1 (its two sensor pairs disagree).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AccuracyError {
    /// Which check.
    pub kind: ErrorKind,
    /// The first sensor's figure (angle has just this).
    pub a: f64,
    /// The second sensor's figure.
    pub b: Option<f64>,
}

/// One sensor's raw figures.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sensor {
    /// Gravity magnitude, raw.
    pub abs_g: u16,
    /// Magnetic magnitude, raw.
    pub abs_m: u16,
    /// Dip, degrees.
    pub dip: f64,
}

/// A shot or calibration packet, decoded.
#[derive(Clone, Debug, PartialEq)]
pub struct Packet {
    /// 1 shot, 2 calibration.
    pub kind: u8,
    /// The flag byte as sent: needed for the acknowledgement.
    pub flag: u8,
    /// The device marked it as part of a leg (bit 7 of the flag, clear = leg).
    pub leg: bool,
    /// A calibration shot.
    pub calibration: bool,
    /// The mark set with the M button.
    pub mark: Mark,
    /// Metres.
    pub distance: f64,
    /// Degrees, 0 to 360, as measured (magnetic).
    pub azimuth: f64,
    /// Degrees, up positive.
    pub inclination: f64,
    /// Roll, degrees.
    pub roll: f64,
    /// Seconds from the device's own clock (local wall-clock, not UTC).
    pub time: Option<u32>,
    /// Sensor 1 and 2 raw figures.
    pub sensors: Vec<Sensor>,
    /// Accuracy complaints.
    pub errors: Vec<AccuracyError>,
    /// Laser signal quality, raw.
    pub quality: Option<u32>,
}

const ANGLE: f64 = 360.0 / 65535.0;

/// A shot straight up or down is 0x4000 (or 0xC000) in the packet, which is 90.0014 degrees on this scale.
/// Read as it stands it would be "past vertical" and every pit and dome shot would be refused, so an
/// inclination a hair past 90, within the packet's own resolution, is vertical.
const VERTICAL_SLACK_DEG: f64 = 0.05;

fn snap_vertical(d: f64) -> f64 {
    if d.abs() > 90.0 && d.abs() <= 90.0 + VERTICAL_SLACK_DEG {
        90.0 * d.signum()
    } else {
        d
    }
}
fn u16_at(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}
fn i16_at(b: &[u8], i: usize) -> i16 {
    i16::from_le_bytes([b[i], b[i + 1]])
}

/// The accuracy block (bytes 45 on). A first byte of 0xFF means none. Bits 7, 6, 5, 4 set to 0 mean absG, absM,
/// dip, angle error, in that order; each takes four bytes after the first, and two at most fit.
pub fn parse_errors(b: &[u8]) -> Vec<AccuracyError> {
    if b.is_empty() || b[0] == 0xff {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut at = 1;
    let kinds = [
        (7, ErrorKind::AbsG),
        (6, ErrorKind::AbsM),
        (5, ErrorKind::Dip),
        (4, ErrorKind::Angle),
    ];
    for (bit, kind) in kinds {
        if (b[0] >> bit) & 1 != 0 {
            continue;
        }
        if at + if kind == ErrorKind::Angle { 1 } else { 3 } >= b.len() {
            break;
        }
        let first = u16_at(b, at);
        let second = (kind != ErrorKind::Angle).then(|| u16_at(b, at + 2));
        // absG and absM are in ten-thousandths; a dip error is a signed angle, an angle error an unsigned one.
        let scaled = |v: u16| match kind {
            ErrorKind::AbsG | ErrorKind::AbsM => f64::from(v) / 10000.0,
            ErrorKind::Dip => f64::from(v as i16) * ANGLE,
            ErrorKind::Angle => f64::from(v) * ANGLE,
        };
        out.push(AccuracyError {
            kind,
            a: scaled(first),
            b: second.map(scaled),
        });
        at += 4;
        if out.len() == 2 {
            break;
        }
    }
    out
}

/// One line for the log: "accuracy error: absG, dip", or `None` for none.
pub fn describe_errors(errors: &[AccuracyError]) -> Option<String> {
    (!errors.is_empty()).then(|| {
        let names: Vec<&str> = errors.iter().map(|e| e.kind.name()).collect();
        format!("accuracy error: {}", names.join(", "))
    })
}

/// Decodes a shot or calibration packet. Accepts the short form (33 bytes or more) as well as the full 64,
/// and returns `None` for anything else: too short, or an unknown type (an empty slot is 0xFF).
pub fn decode_packet(b: &[u8]) -> Option<Packet> {
    if b.len() < SHORT_PACKET_BYTES {
        return None;
    }
    let kind = b[0];
    if kind != 0x01 && kind != 0x02 {
        return None;
    }
    let flag = b[1];
    let mark_bits = usize::from((flag >> 1) & 0x07);
    // The distance bytes are in an unusual order: bits 16 to 23, then 0 to 7, then 8 to 15.
    let distance =
        f64::from((u32::from(b[2]) << 16) | (u32::from(b[4]) << 8) | u32::from(b[3])) / 1000.0;
    let sensor = |at: usize| Sensor {
        abs_g: u16_at(b, at),
        abs_m: u16_at(b, at + 2),
        dip: f64::from(i16_at(b, at + 4)) * ANGLE,
    };
    let mut sensors = vec![sensor(11)];
    if b.len() >= 64 {
        sensors.push(sensor(58));
    }
    Some(Packet {
        kind,
        flag,
        leg: flag & 0x80 == 0,
        calibration: kind == 0x02,
        mark: MARKS.get(mark_bits).copied().unwrap_or(Mark::None),
        distance,
        azimuth: f64::from(u16_at(b, 5)) * ANGLE,
        inclination: snap_vertical(f64::from(i16_at(b, 7)) * ANGLE),
        roll: f64::from(u16_at(b, 9)) * ANGLE,
        time: (b.len() >= 21).then(|| u32::from_le_bytes([b[17], b[18], b[19], b[20]])),
        sensors,
        errors: if b.len() >= 54 {
            parse_errors(&b[45..54])
        } else {
            Vec::new()
        },
        quality: (b.len() >= 58).then(|| u32::from(u16_at(b, 56))),
    })
}

/// What goes into [`encode_packet`].
#[derive(Clone, Debug)]
pub struct PacketSpec {
    /// 1 shot (default), 2 calibration.
    pub kind: u8,
    /// Flagged as part of a leg.
    pub leg: bool,
    /// The mark.
    pub mark: Mark,
    /// Metres.
    pub distance: f64,
    /// Degrees.
    pub azimuth: f64,
    /// Degrees.
    pub inclination: f64,
    /// Roll.
    pub roll: f64,
    /// Device clock, seconds.
    pub time: u32,
    /// Send the 33-byte short form.
    pub short: bool,
    /// Accuracy error bytes (9); default none.
    pub errors: Option<[u8; 9]>,
}

impl Default for PacketSpec {
    fn default() -> Self {
        PacketSpec {
            kind: 1,
            leg: false,
            mark: Mark::None,
            distance: 0.0,
            azimuth: 0.0,
            inclination: 0.0,
            roll: 0.0,
            time: 0,
            short: false,
            errors: None,
        }
    }
}

/// What a packet looks like on the wire, for the simulator and for tests (a full 64 bytes, or the 33-byte short form).
pub fn encode_packet(p: &PacketSpec) -> Vec<u8> {
    let mut out = vec![0u8; PACKET_BYTES];
    let mm = js_round(p.distance * 1000.0) as i64;
    let angle = |deg: f64| (js_round(crate::mod360(deg) / ANGLE) as i64) & 0xffff;
    let mark_bits = MARKS.iter().position(|m| *m == p.mark).unwrap_or(0) as u8;
    out[0] = p.kind;
    // Flag: bit 7 clear = leg; bit 0 set = not a calibration shot; bits 1 to 3 the mark (none is written as
    // 0x07: all clear is feature). The unused bits 4 to 6 are written as 1, like erased memory, so an ordinary
    // shot is 0xff.
    out[1] = (u8::from(!p.leg) << 7)
        | 0x70
        | ((if p.mark != Mark::None {
            mark_bits
        } else {
            0x07
        }) << 1)
        | u8::from(p.kind != 2);
    out[2] = ((mm >> 16) & 0xff) as u8;
    out[3] = (mm & 0xff) as u8;
    out[4] = ((mm >> 8) & 0xff) as u8;
    let mut put16 = |at: usize, v: i64| {
        out[at] = (v & 0xff) as u8;
        out[at + 1] = ((v >> 8) & 0xff) as u8;
    };
    put16(5, angle(p.azimuth));
    put16(7, (js_round(p.inclination / ANGLE) as i64) & 0xffff);
    put16(9, angle(p.roll));
    out[17..21].copy_from_slice(&p.time.to_le_bytes());
    let errs = p.errors.unwrap_or([0xff; 9]);
    out[45..54].copy_from_slice(&errs);
    if p.short {
        out.truncate(SHORT_PACKET_BYTES);
    }
    out
}
