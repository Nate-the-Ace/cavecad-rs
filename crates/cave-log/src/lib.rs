//! Backend feedback for the read-only console: a bounded in-memory log every tool reports to.
//!
//! Code logs through the ordinary `tracing` macros with a `source` field
//! (`tracing::info!(source = "adjust", "loop 3 of 12, closure 0.4%")`). A [`BufferLayer`] copies
//! each event into a [`LogBuffer`] the console panel reads. Nothing here touches files or the
//! clock, so it works unchanged in the browser; saving the per-cave `<cave>.log` is the caller's
//! job (see [`format_line`]). Messages pass through [`redact`] so coordinates never reach the log.

mod buffer;
mod layer;
mod redact;

pub use buffer::{Level, LogBuffer, LogEvent, format_line};
pub use layer::BufferLayer;
pub use redact::redact;
