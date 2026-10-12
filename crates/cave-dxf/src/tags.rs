//! DXF is a stream of (group code, value) pairs, two lines each. This reads and writes that stream.

/// One group code and its value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    /// The group code (0 starts an object, 5 is a handle, 1 a string, ...).
    pub code: i32,
    /// The value, trimmed of surrounding whitespace.
    pub value: String,
}

/// Why a stream could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TagError {
    /// A group code line was not a number.
    BadCode(usize),
    /// The file looks like binary DXF.
    Binary,
}

/// Reads a text DXF into tags. Tolerant of CRLF, stray spaces around codes and a missing final
/// newline; values are kept exactly as written (a stored string may legitimately end in a space); a
/// dangling code with no value (a truncated file) ends the stream rather than failing.
pub fn parse_tags(text: &str) -> Result<Vec<Tag>, TagError> {
    if text.starts_with("AutoCAD Binary DXF") {
        return Err(TagError::Binary);
    }
    let mut lines = text.lines().enumerate();
    let mut out = Vec::new();
    while let Some((n, code_line)) = lines.next() {
        let trimmed = code_line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let code: i32 = trimmed.parse().map_err(|_| TagError::BadCode(n + 1))?;
        let Some((_, value)) = lines.next() else {
            break;
        };
        out.push(Tag {
            code,
            value: value.trim().to_string(),
        });
    }
    Ok(out)
}

/// Builds DXF text and hands out unique handles.
pub(crate) struct Writer {
    tags: Vec<Tag>,
    next_handle: u64,
}

impl Writer {
    pub fn new() -> Self {
        Writer {
            tags: Vec::new(),
            next_handle: 0x20,
        }
    }

    pub fn handle(&mut self) -> String {
        let h = format!("{:X}", self.next_handle);
        self.next_handle += 1;
        h
    }

    pub fn tag(&mut self, code: i32, value: impl Into<String>) {
        self.tags.push(Tag {
            code,
            value: value.into(),
        });
    }

    pub fn num(&mut self, code: i32, v: f64) {
        self.tag(code, fmt_num(v));
    }

    /// Takes the tags written so far (the handle counter keeps going).
    pub fn drain(&mut self) -> Vec<Tag> {
        std::mem::take(&mut self.tags)
    }

    /// The next handle that will be handed out.
    pub fn peek_handle(&self) -> u64 {
        self.next_handle
    }
}

/// A number as DXF text: shortest form that reads back exactly, never `NaN` or `inf`.
pub(crate) fn fmt_num(v: f64) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    let s = format!("{v}");
    // DXF readers want a plain decimal; Rust prints `1e21` style only for huge values.
    if s.contains('e') || s.contains('E') {
        format!("{v:.6}")
    } else {
        s
    }
}

/// Text as it may appear in a DXF value: ASCII only, `\U+XXXX` for anything else, no line breaks.
pub(crate) fn ascii_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\r' | '\n' | '\t' => out.push(' '),
            c if c.is_ascii() && !c.is_ascii_control() => out.push(c),
            c if c.is_ascii() => out.push(' '),
            c => out.push_str(&format!("\\U+{:04X}", c as u32)),
        }
    }
    out
}

/// A layer name DXF accepts: no `<>/\":;?*|=` or backquote, never empty, at most 255 characters.
pub(crate) fn layer_name(s: &str) -> String {
    let cleaned: String = ascii_text(s)
        .chars()
        .map(|c| {
            if "<>/\\\":;?*|=`,".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().to_string();
    if cleaned.is_empty() {
        "_".into()
    } else {
        cleaned.chars().take(255).collect()
    }
}

pub(crate) fn render(tags: &[Tag]) -> String {
    let mut s = String::new();
    for t in tags {
        s.push_str(&format!("{:>3}\n{}\n", t.code, t.value));
    }
    s
}

/// 128-bit FNV-1a over text: detects accidental damage (not tampering by someone determined).
pub(crate) fn checksum(text: &str) -> String {
    cave_id::Id::from_name(text).to_string()
}

/// The hash of drawing geometry: over canonical `code\nvalue\n` text, so it survives any change
/// in spacing or line endings but not a change in what is drawn.
pub(crate) fn geometry_hash(tags: &[Tag]) -> String {
    let mut s = String::new();
    for t in tags {
        s.push_str(&format!("{}\n{}\n", t.code, t.value));
    }
    checksum(&s)
}

/// Splits the stream into named sections (their contents, without the SECTION/name/ENDSEC tags).
pub(crate) fn sections(tags: &[Tag]) -> std::collections::HashMap<String, Vec<Tag>> {
    let mut out = std::collections::HashMap::new();
    let mut i = 0;
    while i < tags.len() {
        if tags[i].code == 0
            && tags[i].value.trim() == "SECTION"
            && i + 1 < tags.len()
            && tags[i + 1].code == 2
        {
            let name = tags[i + 1].value.trim().to_string();
            let mut j = i + 2;
            let mut body = Vec::new();
            while j < tags.len() && !(tags[j].code == 0 && tags[j].value.trim() == "ENDSEC") {
                body.push(tags[j].clone());
                j += 1;
            }
            out.insert(name, body);
            i = j + 1;
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pairs_with_crlf_and_spaces() {
        let t = parse_tags("  0\r\nSECTION\r\n  2\r\nENTITIES  \r\n  0\r\nEOF\r\n").unwrap();
        assert_eq!(t.len(), 3);
        assert_eq!(
            t[1],
            Tag {
                code: 2,
                value: "ENTITIES".into()
            }
        );
    }

    #[test]
    fn a_dangling_code_is_a_truncation_not_an_error() {
        assert_eq!(parse_tags("  0\nSECTION\n  2\n").unwrap().len(), 1);
        assert!(parse_tags("").unwrap().is_empty());
    }

    #[test]
    fn a_non_numeric_code_and_binary_are_refused() {
        assert_eq!(parse_tags("abc\nx\n"), Err(TagError::BadCode(1)));
        assert_eq!(
            parse_tags("AutoCAD Binary DXF\r\n\x1a\0"),
            Err(TagError::Binary)
        );
    }

    #[test]
    fn text_is_ascii_and_single_line() {
        assert_eq!(
            ascii_text("Cueva \u{d1}and\u{fa}"),
            "Cueva \\U+00D1and\\U+00FA"
        );
        assert_eq!(ascii_text("a\nb\tc"), "a b c");
        assert!(ascii_text("\u{2603}").is_ascii());
    }

    #[test]
    fn layer_names_are_made_safe() {
        assert_eq!(layer_name("Profile: sketch/1"), "Profile_ sketch_1");
        assert_eq!(layer_name("   "), "_");
        assert_eq!(
            layer_name("Pi\u{f1}a"),
            "Pi_U+00F1a",
            "the escape's backslash is not allowed in a layer name"
        );
        assert_eq!(layer_name(&"x".repeat(400)).len(), 255);
    }

    #[test]
    fn numbers_never_print_nan_or_exponents() {
        assert_eq!(fmt_num(f64::NAN), "0");
        assert_eq!(fmt_num(f64::INFINITY), "0");
        assert!(!fmt_num(1e30).contains('e'));
        assert_eq!(fmt_num(1.5).parse::<f64>().unwrap(), 1.5);
    }
}
