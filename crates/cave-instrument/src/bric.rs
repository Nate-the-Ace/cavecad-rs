//! The BRIC4 and BRIC5 instruments' Bluetooth LE format: pure functions and constants. Written from BricQt
//! (Philip Schuchardt, MIT, github.com/vpicaver/BricQt), cross-checked against TopoDroid's driver (GPL: the
//! layout only, nothing copied); see docs/INSTRUMENTS.md. Not yet checked against a unit.
//!
//! A shot arrives as three indications, in this order: the PRIMARY (when, distance, azimuth, inclination),
//! the META (running index, dip, roll, temperature, samples averaged, scan or not), and the ERRORS (up to two
//! things the instrument did not like). Commands are plain ASCII words written to a control characteristic.
//! Every number is little-endian.

fn base(short: u16) -> String {
    format!("0000{short:04x}-0000-1000-8000-00805f9b34fb")
}

/// The measurement service.
pub fn measure_service() -> String {
    base(0x58d0)
}
/// Indicates: when, distance, azimuth, inclination (20 bytes).
pub fn primary() -> String {
    base(0x58d1)
}
/// Indicates: index, dip, roll, temperature, samples, type (19 bytes).
pub fn meta() -> String {
    base(0x58d2)
}
/// Indicates: two error entries (18 bytes).
pub fn errors() -> String {
    base(0x58d3)
}
/// Read and write: where a download should start from (20 bytes, a time).
pub fn last_time() -> String {
    base(0x58d4)
}
/// The control service.
pub fn control_service() -> String {
    base(0x58e0)
}
/// The control characteristic that takes ASCII commands.
pub fn control() -> String {
    base(0x58e1)
}

/// Fires the laser and takes a shot.
pub const CMD_SHOT: &str = "shot";
/// Bytes in the primary indication.
pub const PRIMARY_BYTES: usize = 20;
/// Bytes in the meta indication.
pub const META_BYTES: usize = 19;
/// Bytes in the errors indication.
pub const ERRORS_BYTES: usize = 18;

/// How long after a primary indication a shot is given up on waiting for its meta and error indications.
pub const SETTLE_MS: u32 = 400;

/// When the instrument took the shot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Time {
    /// Year.
    pub year: u16,
    /// Month.
    pub month: u8,
    /// Day.
    pub day: u8,
    /// Hour.
    pub hour: u8,
    /// Minute.
    pub minute: u8,
    /// Second.
    pub second: u8,
    /// Hundredths of a second.
    pub centisecond: u8,
}

/// The primary indication.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Primary {
    /// When.
    pub time: Time,
    /// Metres.
    pub distance: f64,
    /// Degrees.
    pub azimuth: f64,
    /// Degrees, up positive.
    pub inclination: f64,
}

/// The meta indication.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Meta {
    /// The shot's running number in the instrument's memory: a jump means shots were missed.
    pub index: u32,
    /// Magnetic dip.
    pub dip: f64,
    /// Roll.
    pub roll: f64,
    /// Temperature.
    pub temperature: f64,
    /// How many readings were averaged for it.
    pub samples: u16,
    /// 0 a regular shot, 1 a scan shot.
    pub kind: u8,
}

/// One error entry: a code and two numbers.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ErrorPair {
    /// What went wrong (see [`describe_errors`]); 0 is nothing.
    pub code: u8,
    /// First figure, the instrument's own units.
    pub a: f64,
    /// Second figure.
    pub b: f64,
}

fn f32_at(b: &[u8], at: usize) -> f64 {
    f64::from(f32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]))
}
fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

/// The primary indication, or `None` when it is not one (too short, or not numbers).
pub fn decode_primary(b: &[u8]) -> Option<Primary> {
    if b.len() < PRIMARY_BYTES {
        return None;
    }
    let (distance, azimuth, inclination) = (f32_at(b, 8), f32_at(b, 12), f32_at(b, 16));
    if ![distance, azimuth, inclination]
        .iter()
        .all(|v| v.is_finite())
    {
        return None;
    }
    Some(Primary {
        time: Time {
            year: u16_at(b, 0),
            month: b[2],
            day: b[3],
            hour: b[4],
            minute: b[5],
            second: b[6],
            centisecond: b[7],
        },
        distance,
        azimuth,
        inclination,
    })
}

/// The meta indication, or `None`.
pub fn decode_meta(b: &[u8]) -> Option<Meta> {
    if b.len() < META_BYTES {
        return None;
    }
    let (dip, roll, temperature) = (f32_at(b, 4), f32_at(b, 8), f32_at(b, 12));
    if ![dip, roll, temperature].iter().all(|v| v.is_finite()) {
        return None;
    }
    Some(Meta {
        index: u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        dip,
        roll,
        temperature,
        samples: u16_at(b, 16),
        kind: b[18],
    })
}

/// The error indication: two entries. A short one (a single entry) gives just the first.
pub fn decode_errors(b: &[u8]) -> Option<[ErrorPair; 2]> {
    if b.len() < 9 {
        return None;
    }
    let pair = |at: usize| {
        if b.len() >= at + 9 {
            ErrorPair {
                code: b[at],
                a: f32_at(b, at + 1),
                b: f32_at(b, at + 5),
            }
        } else {
            ErrorPair::default()
        }
    };
    Some([pair(0), pair(9)])
}

fn meaning(code: u8) -> Option<&'static str> {
    Some(match code {
        1 => "accelerometer 1 reading too high",
        2 => "accelerometer 2 reading too high",
        3 => "magnetometer 1 reading too high",
        4 => "magnetometer 2 reading too high",
        5 => "the two accelerometers disagree",
        6 => "the two magnetometers disagree",
        7 => "the target moved",
        8 => "laser signal too weak",
        9 => "laser signal too strong",
        10 => "laser pattern error",
        11 => "laser timed out",
        12 => "laser error",
        13 => "laser answered the wrong message",
        14 => "clino error",
        15 => "azimuth error",
        _ => return None,
    })
}

/// One line for the log: "the target moved; laser signal too weak", or `None` when there is nothing to report.
pub fn describe_errors(errors: Option<&[ErrorPair; 2]>) -> Option<String> {
    let said: Vec<String> = errors?
        .iter()
        .filter(|e| e.code != 0)
        .map(|e| {
            meaning(e.code)
                .map(String::from)
                .unwrap_or_else(|| format!("error {}", e.code))
        })
        .collect();
    (!said.is_empty()).then(|| said.join("; "))
}

/// The 20 bytes written to the LAST_TIME characteristic to say where a download should resume from.
pub fn last_time_bytes(t: &Time) -> Vec<u8> {
    let mut out = vec![0u8; 20];
    out[..2].copy_from_slice(&t.year.to_le_bytes());
    out[2..8].copy_from_slice(&[t.month, t.day, t.hour, t.minute, t.second, t.centisecond]);
    out
}

/// What goes into [`encode_shot`].
#[derive(Clone, Debug, Default)]
pub struct ShotSpec {
    /// Metres.
    pub distance: f64,
    /// Degrees.
    pub azimuth: f64,
    /// Degrees.
    pub inclination: f64,
    /// Running index.
    pub index: u32,
    /// Roll.
    pub roll: f64,
    /// Dip (60 if the caller has no opinion).
    pub dip: f64,
    /// 0 regular, 1 scan.
    pub kind: u8,
    /// When.
    pub time: Time,
    /// Up to two error entries `(code, a, b)`.
    pub errors: Vec<(u8, f64, f64)>,
}

/// The three indications of one shot, as a BRIC sends them (for the tests and the simulator).
pub fn encode_shot(s: &ShotSpec) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut primary = vec![0u8; PRIMARY_BYTES];
    primary[..2].copy_from_slice(&s.time.year.to_le_bytes());
    primary[2..8].copy_from_slice(&[
        s.time.month,
        s.time.day,
        s.time.hour,
        s.time.minute,
        s.time.second,
        s.time.centisecond,
    ]);
    for (at, v) in [(8, s.distance), (12, s.azimuth), (16, s.inclination)] {
        primary[at..at + 4].copy_from_slice(&(v as f32).to_le_bytes());
    }
    let mut meta = vec![0u8; META_BYTES];
    meta[..4].copy_from_slice(&s.index.to_le_bytes());
    for (at, v) in [(4, s.dip), (8, s.roll), (12, 20.0)] {
        meta[at..at + 4].copy_from_slice(&(v as f32).to_le_bytes());
    }
    meta[16..18].copy_from_slice(&8u16.to_le_bytes());
    meta[18] = s.kind;
    let mut errors = vec![0u8; ERRORS_BYTES];
    for (i, (code, a, b)) in s.errors.iter().take(2).enumerate() {
        errors[i * 9] = *code;
        errors[i * 9 + 1..i * 9 + 5].copy_from_slice(&(*a as f32).to_le_bytes());
        errors[i * 9 + 5..i * 9 + 9].copy_from_slice(&(*b as f32).to_le_bytes());
    }
    (primary, meta, errors)
}
