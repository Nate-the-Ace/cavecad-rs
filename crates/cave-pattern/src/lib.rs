//! Patterns people author: motifs (small drawings), linetypes (dashes, dots and motifs repeated along a line) and fills
//! (hatches, dots and tiled motifs repeated inside an outline).
//!
//! Everything here is source data plus pure layout. A definition says what repeats and how far apart, in millimetres on paper
//! (so a printed map looks alike at 1:500 and 1:2000); the drawing's scale turns that into metres at layout time. The same
//! definition and the same seed always lay out the same marks, on every device, so nothing derived is ever stored.
//!
//! This crate knows nothing about documents, files or screens: the document keeps each definition as JSON text and the viewer and
//! the DXF export ask here for the marks.

mod fill;
mod library;
mod linetype;
mod motif;

pub use fill::{FillDef, Layer, fill_marks, rotated};
pub use library::{Library, builtin_library};
pub use linetype::{
    Element, Layout, LineDef, Placement, lay_out, marks as line_marks, motifs_used,
};
pub use motif::Motif;

/// Why a definition was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatternError {
    /// The name is empty, too long, or has characters other than letters, digits, spaces, `-`, `_` and `.`.
    BadName,
    /// A drawing with nothing in it, or with a point that is not a finite number.
    BadDrawing,
    /// A length, size or angle that is not a finite number in the allowed range.
    BadNumber(&'static str),
    /// A linetype with no dash, dot or motif in it, or with nothing that moves along the line.
    NothingToDraw,
    /// Too many elements or layers.
    TooLong,
    /// A motif the definition names that is not in the library.
    MissingMotif(String),
    /// The JSON text is not a definition.
    BadJson,
}

impl std::fmt::Display for PatternError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PatternError::BadName => {
                write!(f, "A name needs 1 to 40 letters, digits, spaces, - _ or .")
            }
            PatternError::BadDrawing => write!(
                f,
                "The drawing is empty or has a point that is not a number."
            ),
            PatternError::BadNumber(what) => write!(f, "{what} is not a usable number."),
            PatternError::NothingToDraw => write!(
                f,
                "The pattern draws nothing (it needs a dash, dot or motif and some length)."
            ),
            PatternError::TooLong => write!(f, "The pattern has too many parts."),
            PatternError::MissingMotif(m) => write!(f, "The motif \"{m}\" does not exist."),
            PatternError::BadJson => write!(f, "That is not a pattern definition."),
        }
    }
}

impl std::error::Error for PatternError {}

/// Checks a name: 1 to 40 characters of letters, digits, spaces, `-`, `_` and `.`, not starting or ending with a space.
pub fn valid_name(name: &str) -> bool {
    let n = name.chars().count();
    (1..=40).contains(&n)
        && name == name.trim()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'))
}

/// The longest a dash, gap or spacing may be, in millimetres on paper.
pub const MAX_MM: f64 = 200.0;
/// The shortest a dash, gap or spacing may be, in millimetres on paper.
pub const MIN_MM: f64 = 0.05;
/// The most parts (elements or layers) one definition may have.
pub const MAX_PARTS: usize = 32;

/// A number that is finite and within `lo..=hi`.
pub(crate) fn in_range(v: f64, lo: f64, hi: f64, what: &'static str) -> Result<(), PatternError> {
    if v.is_finite() && v >= lo && v <= hi {
        Ok(())
    } else {
        Err(PatternError::BadNumber(what))
    }
}
