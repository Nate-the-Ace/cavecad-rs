//! DXF export and import with a lossless round trip for the cave itself.
//!
//! A saved file is a normal R2000 ASCII DXF that other CAD programs can open and draw (stations,
//! legs, splays, walls, loop closures, profile sketches, labels, layers), and it carries the whole
//! cave document inside, checksummed, in a named XRECORD dictionary (`CAVECAD`). Reading the file
//! back restores exactly the document that was written; the drawing geometry is for other programs
//! and is never trusted over the embedded cave.
//!
//! Safety rules, each one backed by a test:
//! - No value line is ever longer than 240 bytes. An over-long line once desynchronised a DXF
//!   reader in the old app, which then silently dropped the rest of the file.
//! - Output is pure ASCII: non-ASCII text is escaped (`\U+XXXX` in drawing text, `\uXXXX` in the
//!   embedded JSON), so no code page can corrupt a name.
//! - The embedded data carries its length, chunk count and a checksum. A truncated or altered file
//!   is an error, never a half-loaded cave.
//! - A file from a newer version of the app is refused, not guessed at.
//! - The reader never panics, whatever it is given.
//! - A file saved by another program (geometry no longer matches what we wrote) still loads from
//!   the embedded cave, with a warning that the drawing geometry is not used.

mod export;
mod import;
mod tags;

pub use export::{ExportOptions, export};
pub use import::{ImportError, Imported, Warning, import};
pub use tags::{Tag, parse_tags};

/// Largest value line the writer produces, in bytes.
pub const MAX_VALUE_LINE: usize = 240;
/// Version of the embedded-cave record layout.
pub const FORMAT_VERSION: u32 = 1;
