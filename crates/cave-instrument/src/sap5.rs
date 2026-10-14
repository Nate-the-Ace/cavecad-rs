//! The Shetland Attack Pony 5's Bluetooth LE format: the classic DistoX 8-byte data packet, notified over
//! the Nordic UART service. Pure functions and constants. Written from TopoDroid's SAP and DistoX handling
//! (GPL: the layout only, nothing copied) and the SAP5 firmware repo (github.com/furbrain/SAP5). Not yet
//! checked against a unit. (The SAP6 uses the CaveBLE service: [`crate::caveble`].)
//!
//! The Nordic UART characteristics are the other way round to the Cavway X1's: the SAP5 sends on
//! `6e400002` and is written to on `6e400003`.

/// Nordic UART service: the SAP5's only service (the Cavway X1 uses the same one).
pub const SERVICE: &str = "6e400001-b5a3-f393-e0a9-e50e24dcca9e";
/// SAP5 to app (notify).
pub const NOTIFY: &str = "6e400002-b5a3-f393-e0a9-e50e24dcca9e";
/// App to SAP5.
pub const WRITE: &str = "6e400003-b5a3-f393-e0a9-e50e24dcca9e";

/// Bytes in a DistoX data packet.
pub const PACKET_BYTES: usize = 8;

/// A data packet, decoded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DistoXPacket {
    /// The sequence bit (bit 7 of the first byte), which flips with each packet.
    pub seq: u8,
    /// Metres.
    pub distance: f64,
    /// Degrees, 0 to 360.
    pub azimuth: f64,
    /// Degrees, up positive.
    pub inclination: f64,
    /// Degrees.
    pub roll: f64,
}

/// Decodes one 8-byte DistoX data packet, or `None` for anything else (another length, or another packet
/// type). `fix_bit16` applies the SAP5 firmware's known fault: it sets the distance's bit 16 when a distance
/// passes 32.768 m, not 65.536 m, so a set bit with the next byte's top bit also set is cleared. On by
/// default, as in TopoDroid.
pub fn decode_packet(bytes: &[u8], fix_bit16: bool) -> Option<DistoXPacket> {
    if bytes.len() != PACKET_BYTES {
        return None;
    }
    let mut b = [0u8; PACKET_BYTES];
    b.copy_from_slice(bytes);
    if b[0] & 0x3f != 0x01 {
        return None;
    }
    if fix_bit16 && b[2] & 0x80 != 0 {
        b[0] &= 0xbf;
    }
    let mm = u32::from(b[0] & 0x40) * 1024 + ((u32::from(b[2]) << 8) | u32::from(b[1]));
    let az = (u32::from(b[4]) << 8) | u32::from(b[3]);
    let cl = i16::from_le_bytes([b[5], b[6]]);
    Some(DistoXPacket {
        seq: (b[0] >> 7) & 1,
        distance: f64::from(mm) / 1000.0,
        azimuth: f64::from(az) * 180.0 / 32768.0,
        inclination: f64::from(cl) * 90.0 / 16384.0,
        roll: f64::from(b[7]) * 180.0 / 128.0,
    })
}

/// What a SAP5 sends, for the tests and the simulator: a shot as an 8-byte packet.
pub fn encode_packet(distance: f64, azimuth: f64, inclination: f64, roll: f64, seq: u8) -> Vec<u8> {
    let mm = crate::js_round(distance * 1000.0) as i64;
    let az = (crate::js_round(crate::mod360(azimuth) * 32768.0 / 180.0) as i64) & 0xffff;
    let cl = (crate::js_round(inclination * 16384.0 / 90.0) as i64) & 0xffff;
    vec![
        0x01 | if mm >= 65536 { 0x40 } else { 0 } | if seq != 0 { 0x80 } else { 0 },
        (mm & 0xff) as u8,
        ((mm >> 8) & 0xff) as u8,
        (az & 0xff) as u8,
        (az >> 8) as u8,
        (cl & 0xff) as u8,
        (cl >> 8) as u8,
        ((crate::js_round(roll % 360.0 * 128.0 / 180.0) as i64) & 0xff) as u8,
    ]
}
