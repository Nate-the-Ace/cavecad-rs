//! The open "CaveBLE" protocol that paperless cave-surveying instruments (the SAP6) speak to TopoDroid and
//! SexyTopo (Phil Underwood, MIT; reference client `CaveBLE.kt` in github.com/furbrain/CircuitPython_CaveBLE).
//! Only the wire format: pure functions and constants, no Bluetooth. Verified against the reference client's
//! source, not against a unit.
//!
//! One service holds a name (read), a command (write, no response) and a leg (notify) characteristic. A leg
//! frame is 17 bytes, little-endian: a sequence byte (0 or 1), then four f32: azimuth, inclination, roll
//! (degrees) and distance (metres). The surveyor acknowledges every leg by writing `0x55 + sequence bit` to the
//! command characteristic; an instrument that hears no ack resends the same frame, so a repeat of the last
//! frame is a duplicate, not a new shot.

/// The CaveBLE service.
pub const SERVICE: &str = "137c4435-8a64-4bcb-93f1-3792c6bdc965";
/// Name characteristic (read).
pub const NAME_CHARACTERISTIC: &str = "137c4435-8a64-4bcb-93f1-3792c6bdc966";
/// Command characteristic (write, no response).
pub const COMMAND_CHARACTERISTIC: &str = "137c4435-8a64-4bcb-93f1-3792c6bdc967";
/// Leg characteristic (notify).
pub const LEG_CHARACTERISTIC: &str = "137c4435-8a64-4bcb-93f1-3792c6bdc968";

/// Stop calibration.
pub const CMD_STOP_CAL: u8 = 0x30;
/// Start calibration.
pub const CMD_START_CAL: u8 = 0x31;
/// Switch the device off.
pub const CMD_DEVICE_OFF: u8 = 0x34;
/// Laser on.
pub const CMD_LASER_ON: u8 = 0x36;
/// Laser off.
pub const CMD_LASER_OFF: u8 = 0x37;
/// Fire a shot.
pub const CMD_TAKE_SHOT: u8 = 0x38;
/// Acknowledge a leg (add the sequence bit).
pub const CMD_ACK: u8 = 0x55;

/// Bytes in one leg frame.
pub const LEG_BYTES: usize = 17;

/// One shot as the instrument sent it: raw, nothing converted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leg {
    /// The sequence bit (0 or 1) the ack must echo.
    pub seq: u8,
    /// Compass, degrees.
    pub azimuth: f64,
    /// Clino, degrees, up positive.
    pub inclination: f64,
    /// Roll of the instrument, degrees.
    pub roll: f64,
    /// Tape, metres.
    pub distance: f64,
}

fn f32_at(b: &[u8], at: usize) -> f64 {
    f64::from(f32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]))
}

/// Reads one leg frame, or `None` when it is not one (wrong length, unknown sequence byte, not a number).
pub fn decode_leg(bytes: &[u8]) -> Option<Leg> {
    if bytes.len() != LEG_BYTES || bytes[0] > 1 {
        return None;
    }
    let leg = Leg {
        seq: bytes[0],
        azimuth: f32_at(bytes, 1),
        inclination: f32_at(bytes, 5),
        roll: f32_at(bytes, 9),
        distance: f32_at(bytes, 13),
    };
    [leg.azimuth, leg.inclination, leg.roll, leg.distance]
        .iter()
        .all(|v| v.is_finite())
        .then_some(leg)
}

/// The frame an instrument would send for a leg (for the simulator and for tests).
pub fn encode_leg(leg: &Leg) -> Vec<u8> {
    let mut out = vec![leg.seq];
    for v in [leg.azimuth, leg.inclination, leg.roll, leg.distance] {
        out.extend_from_slice(&(v as f32).to_le_bytes());
    }
    out
}

/// The command byte that acknowledges a leg with this sequence bit (`0x55` or `0x56`).
pub const fn ack_for(seq: u8) -> u8 {
    CMD_ACK + seq
}

/// Tells a resend from a new shot. An instrument repeats a frame until it is acknowledged, so the same
/// frame as the last one is the same shot arriving twice. (Two real shots never agree to the last bit of
/// four floats.) The repeat must still be acknowledged.
#[derive(Clone, Debug, Default)]
pub struct LegFilter {
    last: Option<Vec<u8>>,
}

impl LegFilter {
    /// True for a new shot, false for a repeat of the previous frame.
    pub fn is_new(&mut self, frame: &[u8]) -> bool {
        let same = self.last.as_deref() == Some(frame);
        self.last = Some(frame.to_vec());
        !same
    }

    /// Forgets the last frame.
    pub fn reset(&mut self) {
        self.last = None;
    }
}
