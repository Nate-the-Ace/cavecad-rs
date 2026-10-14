//! Survey instruments: the bytes a distance-meter sends, turned into survey shots.
//!
//! Pure and clock-free (wasm32-safe): the Bluetooth transport lives in the browser and hands every
//! frame to a link here; the link answers with what to write back to the instrument, what to log and
//! what readings came out. No timer is kept: a link that wants one says so ([`link::BricLink::has_pending`])
//! and the host calls `flush` when it fires.
//!
//! - [`caveble`], [`cavway`], [`bric`], [`sap5`]: the wire formats (SAP6, Cavway X1, BRIC4/5, SAP5).
//! - [`link`]: the path from bytes to a [`Reading`], one per protocol.
//! - [`legs`]: grouping single shots into legs and splays.
//! - [`identify`]: which instrument is this, from its advertised name.
//! - [`sim`]: a pretend instrument, for every device and its quirks, driven by a seed.

pub mod bric;
pub mod caveble;
pub mod cavway;
pub mod identify;
pub mod legs;
pub mod link;
pub mod sap5;
pub mod sim;
mod types;

pub use types::{Mark, Reading, Side};

/// JavaScript's `Math.round`: halves go up. (Rust's `round` sends negative halves away from zero.)
pub fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// Rounds away the noise of a 32-bit float (8.84 arrives as 8.840000152...) to `digits` places.
pub fn tidy(v: f64, digits: i32) -> f64 {
    let k = 10f64.powi(digits);
    js_round(v * k) / k
}

/// `a` wrapped into 0 to 360 degrees.
pub fn mod360(a: f64) -> f64 {
    ((a % 360.0) + 360.0) % 360.0
}

/// Smallest angle between two bearings, degrees (0 to 180).
pub fn angle_gap(a: f64, b: f64) -> f64 {
    ((((a - b) % 360.0) + 540.0) % 360.0 - 180.0).abs()
}

/// Bytes as lowercase hex pairs separated by spaces, for the capture log.
pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&format!("{b:02x}"));
    }
    out
}
