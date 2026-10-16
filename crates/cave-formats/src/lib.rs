//! Reading and writing the standard cave survey data formats: Compass (.dat), Walls (.srv), Survex
//! (.svx), Therion (.th) and plain CSV.
//!
//! A port of the old CaveCAD's format readers, which were checked against real files over years; the
//! golden tests here hold this port to what they did, call for call. Every reader turns text into one
//! [`Survey`] (the old app's survey shape) and every writer turns a [`Survey`] back into text. "Ask for
//! a file, not for a format": [`detect`] picks the format from the file's name and content.
//!
//! A format detail read wrong does not fail -- it draws a plausible but WRONG map -- so what each
//! reader does not understand it reports as a [`Finding`] rather than passing over in silence.
//!
//! Pure and wasm32-safe: text in, text out. A Therion file's `input` lines are followed through a
//! function the caller supplies ([`therion::parse_with_inputs`]).

pub mod compass;
pub mod csv;
mod document;
pub mod js;
mod model;
pub mod project;
pub mod survex;
pub mod therion;
pub mod walls;

pub use document::{from_document, to_document};
pub use model::{
    FEET_PER_METER, Finding, Fixed, GeoRef, Lrud, LrudEntry, PLUMB_DEG, Shot, Survey, Trip,
    applied_declination, convert, lrud_entry_text, normalize_azimuth, parse_lrud_entry,
};
pub use project::{ProjectFormat, ProjectRead, detect_project, read_project};

/// A survey data format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// Compass .dat
    Compass,
    /// Walls .srv
    Walls,
    /// Survex .svx
    Survex,
    /// Therion .th
    Therion,
    /// Plain CSV with CaveCAD's header comments
    Csv,
}

/// Every format, in the order a file dialog lists them.
pub const FORMATS: [Format; 5] = [
    Format::Compass,
    Format::Walls,
    Format::Survex,
    Format::Therion,
    Format::Csv,
];

impl Format {
    /// The short name ("compass", "walls", "survex", "therion", "csv").
    pub fn id(self) -> &'static str {
        match self {
            Format::Compass => "compass",
            Format::Walls => "walls",
            Format::Survex => "survex",
            Format::Therion => "therion",
            Format::Csv => "csv",
        }
    }

    /// The format with this short name.
    pub fn from_id(id: &str) -> Option<Format> {
        FORMATS.into_iter().find(|f| f.id() == id)
    }

    /// What people call it.
    pub fn label(self) -> &'static str {
        match self {
            Format::Compass => "Compass (.dat)",
            Format::Walls => "Walls (.srv)",
            Format::Survex => "Survex (.svx)",
            Format::Therion => "Therion (.th)",
            Format::Csv => "CSV (.csv)",
        }
    }

    /// The file extensions it is saved with, the first being the one written.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Format::Compass => &["dat"],
            Format::Walls => &["srv"],
            Format::Survex => &["svx"],
            Format::Therion => &["th"],
            Format::Csv => &["csv", "txt"],
        }
    }

    /// Reads a file of this format.
    pub fn parse(self, content: &str) -> Survey {
        match self {
            Format::Compass => compass::parse(content),
            Format::Walls => walls::parse(content),
            Format::Survex => survex::parse(content),
            Format::Therion => therion::parse(content),
            Format::Csv => csv::parse(content),
        }
    }

    /// Writes a survey in this format. The writers first make sure the survey has its trip table
    /// (which is why they take it mutably), exactly as the old ones did.
    pub fn write(self, survey: &mut Survey) -> String {
        match self {
            Format::Compass => compass::write(survey),
            Format::Walls => walls::write(survey),
            Format::Survex => survex::write(survey),
            Format::Therion => therion::write(survey),
            Format::Csv => csv::write(survey),
        }
    }
}

/// Picks a file's format from its name and content: an extension whose content does not say
/// otherwise wins; when they disagree the content wins; `None` when neither can tell.
pub fn detect(file_name: &str, content: &str) -> Option<Format> {
    let ext = extension(file_name);
    let by_content = detect_by_content(content);
    for f in FORMATS {
        if f.extensions().contains(&ext.as_str()) {
            return match by_content {
                None => Some(f),
                Some(c) if c == f => Some(f),
                Some(c) => Some(c),
            };
        }
    }
    by_content
}

/// `\.([A-Za-z0-9]+)$`, lower-cased.
fn extension(name: &str) -> String {
    match name.rfind('.') {
        Some(i)
            if i + 1 < name.len() && name[i + 1..].bytes().all(|b| b.is_ascii_alphanumeric()) =>
        {
            name[i + 1..].to_ascii_lowercase()
        }
        _ => String::new(),
    }
}

/// `/^\s*<marker>(word)<then>/im`: whether, at the start of some line and after white space (which
/// may run over line ends, as `\s` does), `marker` and one of `words` (case-insensitive) appear and
/// `then` accepts everything after the word.
fn line_starts(head: &str, marker: &str, words: &[&str], then: impl Fn(&str) -> bool) -> bool {
    let mut starts = vec![0];
    for (i, c) in head.char_indices() {
        if matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}') {
            starts.push(i + c.len_utf8());
        }
    }
    for at in starts {
        let t = js::trim_start(&head[at..]);
        let Some(rest) = t.strip_prefix(marker) else {
            continue;
        };
        for w in words {
            if rest
                .get(..w.len())
                .is_some_and(|x| x.eq_ignore_ascii_case(w))
                && then(&rest[w.len()..])
            {
                return true;
            }
        }
    }
    false
}

/// Detection by content alone; `None` if nothing is unmistakable.
pub fn detect_by_content(content: &str) -> Option<Format> {
    // the first 4000 characters (UTF-16 units in the old code; characters here)
    let end = content
        .char_indices()
        .nth(4000)
        .map_or(content.len(), |(i, _)| i);
    let head = &content[..end];
    let any = |_: &str| true;
    if line_starts(
        head,
        "*",
        &[
            "begin",
            "data",
            "units",
            "fix",
            "calibrate",
            "declination",
            "flags",
            "alias",
            "date",
            "team",
        ],
        any,
    ) {
        return Some(Format::Survex);
    }
    if line_starts(
        head,
        "#",
        &["units", "prefix", "fix", "date", "segment"],
        any,
    ) {
        return Some(Format::Walls);
    }
    let upper = head.to_ascii_uppercase();
    if upper.contains("SURVEY NAME:") && upper.contains("DECLINATION:") {
        return Some(Format::Compass);
    }
    // a keyword, then white space, then something
    let word_then_more = |rest: &str| {
        let t = js::trim_start(rest);
        t.len() < rest.len() && !t.is_empty()
    };
    if line_starts(
        head,
        "",
        &["survey", "centreline", "centerline", "encoding"],
        word_then_more,
    ) {
        return Some(Format::Therion);
    }
    let csv_header = |rest: &str| {
        let t = js::trim_start(rest);
        let Some(t) = t.strip_prefix(',') else {
            return false;
        };
        let t = js::trim_start(t);
        if !t.get(..2).is_some_and(|w| w.eq_ignore_ascii_case("to")) {
            return false;
        }
        let t = js::trim_start(&t[2..]);
        let Some(t) = t.strip_prefix(',') else {
            return false;
        };
        let t = js::trim_start(t);
        t.get(..8)
            .is_some_and(|w| w.eq_ignore_ascii_case("distance"))
    };
    if line_starts(head, "", &["from"], csv_header) {
        return Some(Format::Csv);
    }
    None
}

/// A survey read from a file, with the format it was read as.
pub struct Read {
    /// The format.
    pub format: Format,
    /// What was in it.
    pub survey: Survey,
}

/// Detects a file's format and reads it; `None` when the format cannot be told.
pub fn read(file_name: &str, content: &str) -> Option<Read> {
    let format = detect(file_name, content)?;
    Some(Read {
        format,
        survey: format.parse(content),
    })
}

/// Reads a survey file, or a project file and the data files it names, following every reference
/// (`*include`, Therion `input`, a project's file list) through `read`, which answers a path's
/// text or `None`. Returns what it was read as, and the survey; an error when the file is not
/// survey data CaveCAD can read.
pub fn read_any(
    file_name: &str,
    text: &str,
    read: &dyn Fn(&str) -> Option<String>,
) -> Result<(String, Survey), String> {
    if let Some(project) = detect_project(file_name, text) {
        let r = read_project(project, file_name, text, read);
        let n = r.files.len();
        return Ok((
            format!(
                "{} ({n} file{})",
                project.label(),
                if n == 1 { "" } else { "s" }
            ),
            r.survey,
        ));
    }
    let Some(format) = detect(file_name, text) else {
        return Err(format!(
            "{file_name} is not a survey file CaveCAD can read. It reads Compass (.dat, .mak), Walls (.srv, .wpj), Survex (.svx), Therion (.th, thconfig) and CSV."
        ));
    };
    let survey = match format {
        Format::Therion => therion::parse_with_inputs(text, file_name, read),
        Format::Survex => survex::parse_with_includes(text, file_name, read),
        f => f.parse(text),
    };
    Ok((format.label().to_string(), survey))
}
