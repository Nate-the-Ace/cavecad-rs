//! Therion .th2: the DRAWING half of a Therion survey (a port of the old app's CsTherion2.js).
//!
//! A caver who sketched the whole cave on a phone (TopoDroid, PocketTopo exporting to Therion)
//! should not have to retrace it. This reads the sketch into a neutral model -- scraps of lines,
//! areas and points in the sketcher's own coordinates, with the station markers that tie them to
//! the cave -- and [`crate::sketch`] decides what each Therion type becomes and where it lands.
//!
//! Lines keep their Bézier curves (a segment is one point for a straight run, three for a curve).
//! Station markers (`point x y station -name 1`) are ties, not symbols; section references
//! (`point x y section -scrap xs1`) likewise. Refused, and said so: `input` of another file, a raster
//! pinned behind the sketch (`xth_me_image_insert`), and projected (not extended) elevations.

use crate::js::JsObj;
use crate::therion::{logical_lines, tokenize};
use serde::Serialize;

/// Something the person should be told about the sketch.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[allow(missing_docs)]
pub struct Finding {
    pub severity: String,
    pub code: String,
    pub message: String,
}

/// A scrap's declared scale: real units per sketch unit.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct Scale {
    pub units_per_drawing: f64,
    pub unit: String,
}

/// A station marker: the tie between the sketch and the cave.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[allow(missing_docs)]
pub struct StationTie {
    pub name: String,
    pub x: f64,
    pub y: f64,
}

/// One segment of a line: to a point, straight or by a cubic with two control points.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[allow(missing_docs)]
pub struct Seg {
    pub to: [f64; 2],
    pub c1: Option<[f64; 2]>,
    pub c2: Option<[f64; 2]>,
}

/// A drawn line.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[allow(missing_docs)]
pub struct Line {
    #[serde(rename = "type")]
    pub kind: String,
    pub subtype: Option<String>,
    pub closed: bool,
    pub reversed: bool,
    pub id: Option<String>,
    pub options: JsObj<String>,
    pub segs: Vec<Seg>,
}

/// An area: the lines (by id) that bound it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct Area {
    #[serde(rename = "type")]
    pub kind: String,
    pub line_ids: Vec<String>,
    pub options: JsObj<String>,
}

/// A point symbol, label or remark.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[allow(missing_docs)]
pub struct Point {
    #[serde(rename = "type")]
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub orientation: Option<f64>,
    pub scale: Option<String>,
    pub text: Option<String>,
    pub options: JsObj<String>,
}

/// A reference to a cross-section scrap.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[allow(missing_docs)]
pub struct SectionRef {
    pub x: f64,
    pub y: f64,
    pub scrap: Option<String>,
}

/// One scrap.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct Scrap {
    pub name: String,
    /// "plan", "extended", "elevation", "none" (a cross section), or `None` when unreadable.
    pub projection: Option<String>,
    pub projection_angle: Option<f64>,
    pub scale: Option<Scale>,
    pub options: JsObj<String>,
    pub stations: Vec<StationTie>,
    pub lines: Vec<Line>,
    pub areas: Vec<Area>,
    pub points: Vec<Point>,
    pub sections: Vec<SectionRef>,
}

/// A whole sketch file.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Sketch {
    /// The scraps, in file order.
    pub scraps: Vec<Scrap>,
    /// What could not be read.
    pub findings: Vec<Finding>,
}

impl Sketch {
    fn add(&mut self, severity: &str, code: &str, message: String) {
        if self
            .findings
            .iter()
            .any(|f| f.code == code && f.message == message)
        {
            return;
        }
        self.findings.push(Finding {
            severity: severity.into(),
            code: code.into(),
            message,
        });
    }
}

/// A number as this format writes them (strict: "12abc" is not 12).
fn number(t: &str) -> Option<f64> {
    let b = t.as_bytes();
    let mut i = usize::from(matches!(b.first(), Some(b'+' | b'-')));
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let ints = i - int_start;
    let mut fracs = 0;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let f0 = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        fracs = i - f0;
    }
    if ints == 0 && fracs == 0 {
        return None;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && matches!(b[j], b'+' | b'-') {
            j += 1;
        }
        let e0 = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j == e0 {
            return None;
        }
        i = j;
    }
    if i != b.len() {
        return None;
    }
    t.parse().ok()
}

/// A token that starts an option (`-name`).
fn is_option(t: &str) -> bool {
    t.starts_with('-') && t.chars().nth(1).is_some_and(|c| c.is_ascii_alphabetic())
}

/// Positional words and `-name value` options (a value may be a bracket group or quoted string;
/// an option with nothing after it is recorded as "").
fn options(tokens: &[String]) -> (Vec<String>, JsObj<String>) {
    let mut words = Vec::new();
    let mut opts = JsObj::new();
    let mut i = 0;
    while i < tokens.len() {
        if is_option(&tokens[i]) {
            let value = tokens.get(i + 1).cloned().unwrap_or_default();
            opts.set(&tokens[i][1..].to_lowercase(), value);
            i += 2;
            continue;
        }
        words.push(tokens[i].clone());
        i += 1;
    }
    (words, opts)
}

/// The words inside a `[a b c]` bracket token; empty for anything not bracketed.
fn bracket(t: &str) -> Vec<String> {
    let t = crate::js::trim(t);
    if !t.starts_with('[') {
        return Vec::new();
    }
    let inner = match t.rfind(']') {
        Some(e) if e > 0 => &t[1..e],
        _ => &t[1..],
    };
    let inner = crate::js::trim(inner);
    if inner.is_empty() {
        Vec::new()
    } else {
        crate::js::split_ws(inner)
    }
}

/// `-scale`: a number (real units per sketch unit), or `[x1 y1 x2 y2 X1 Y1 X2 Y2 unit]`.
fn scale(t: Option<&String>) -> Option<Scale> {
    let t = t?;
    if let Some(d) = number(t) {
        return (d > 0.0).then(|| Scale {
            units_per_drawing: d,
            unit: "m".into(),
        });
    }
    let parts = bracket(t);
    if parts.len() < 8 {
        return None;
    }
    let n: Vec<f64> = parts[..8]
        .iter()
        .map(|p| number(p))
        .collect::<Option<_>>()?;
    let scrap = ((n[2] - n[0]).powi(2) + (n[3] - n[1]).powi(2)).sqrt();
    let real = ((n[6] - n[4]).powi(2) + (n[7] - n[5]).powi(2)).sqrt();
    if scrap <= 0.0 || real <= 0.0 {
        return None;
    }
    Some(Scale {
        units_per_drawing: real / scrap,
        unit: parts.get(8).map_or("m".into(), |u| u.to_lowercase()),
    })
}

/// `-projection plan` or `-projection [elevation 90]`: the name (when one this reads) and angle.
fn projection(t: Option<&String>) -> (Option<String>, Option<f64>) {
    let Some(t) = t.filter(|t| !t.is_empty()) else {
        return (None, None);
    };
    let mut parts = bracket(t);
    if parts.is_empty() {
        parts = vec![t.clone()];
    }
    let name = parts[0].to_lowercase();
    if !matches!(name.as_str(), "plan" | "extended" | "elevation" | "none") {
        return (None, None);
    }
    (Some(name), parts.get(1).and_then(|a| number(a)))
}

/// An option switch: "on", "", "1", "true", "yes" mean on.
fn is_on(v: &str) -> bool {
    matches!(v.to_lowercase().as_str(), "" | "on" | "1" | "true" | "yes")
}

/// A line segment: 2 numbers (to), or 6 (c1 c2 to; the first segment's controls mean nothing).
fn segment(tokens: &[String], first: bool) -> Option<Seg> {
    let n: Vec<f64> = tokens.iter().map(|t| number(t)).collect::<Option<_>>()?;
    match n.len() {
        2 => Some(Seg {
            to: [n[0], n[1]],
            c1: None,
            c2: None,
        }),
        6 if first => Some(Seg {
            to: [n[4], n[5]],
            c1: None,
            c2: None,
        }),
        6 => Some(Seg {
            to: [n[4], n[5]],
            c1: Some([n[0], n[1]]),
            c2: Some([n[2], n[3]]),
        }),
        _ => None,
    }
}

fn apply_line_options(line: &mut Line, opts: &JsObj<String>) {
    for (k, v) in opts.iter() {
        line.options.set(k, v.clone());
        match k {
            "subtype" => line.subtype = Some(v.to_lowercase()),
            "close" => line.closed = is_on(v),
            "reverse" => line.reversed = is_on(v),
            "id" => line.id = Some(v.clone()),
            _ => {}
        }
    }
}

fn begin_scrap(sketch: &mut Sketch, tokens: &[String]) -> Scrap {
    let (words, opts) = options(&tokens[1..]);
    let name = words.first().cloned().unwrap_or_default();
    let (proj, angle) = projection(opts.get("projection"));
    if opts.has("projection") && proj.is_none() {
        sketch.add(
            "warning",
            "th2-projection",
            format!(
                "Scrap \"{name}\" is drawn in a projection this reader does not know ({}). It is not imported.",
                opts.get("projection").cloned().unwrap_or_default()
            ),
        );
    } else if proj.as_deref() == Some("elevation") {
        sketch.add(
            "warning",
            "th2-projected-elevation",
            format!("Scrap \"{name}\" is a projected elevation. This suite draws an EXTENDED elevation, which is a different drawing of the same cave, so the scrap is not imported."),
        );
    }
    Scrap {
        name,
        // no -projection at all is a plan: Therion's default, and most phone sketches
        projection: if opts.has("projection") {
            proj
        } else {
            Some("plan".into())
        },
        projection_angle: angle,
        scale: scale(opts.get("scale")),
        options: opts,
        stations: Vec::new(),
        lines: Vec::new(),
        areas: Vec::new(),
        points: Vec::new(),
        sections: Vec::new(),
    }
}

fn read_point(scrap: &mut Scrap, tokens: &[String]) {
    let (words, opts) = options(&tokens[1..]);
    if words.len() < 3 {
        return;
    }
    let (Some(x), Some(y)) = (number(&words[0]), number(&words[1])) else {
        return;
    };
    let kind = words[2].to_lowercase();
    if kind == "station" {
        if let Some(name) = opts.get("name").filter(|n| !n.is_empty()) {
            scrap.stations.push(StationTie {
                name: name.clone(),
                x,
                y,
            });
        }
        return;
    }
    if kind == "section" {
        scrap.sections.push(SectionRef {
            x,
            y,
            scrap: opts.get("scrap").cloned(),
        });
        return;
    }
    scrap.points.push(Point {
        kind,
        x,
        y,
        orientation: opts.get("orientation").and_then(|o| number(o)),
        scale: opts.get("scale").map(|s| s.to_lowercase()),
        // -text captions a label, remark or name; -value is what an altitude or height measures
        text: opts.get("text").or(opts.get("value")).cloned(),
        options: opts,
    });
}

/// Reads a .th2 file. Always a model: a file with nothing readable comes back with no scraps and a
/// finding saying so.
pub fn parse(content: &str) -> Sketch {
    let mut sketch = Sketch::default();
    if content.is_empty() {
        sketch.add("error", "th2-empty", "The sketch file is empty.".into());
        return sketch;
    }
    // xtherion's metadata lines are comments, so the raw text is searched for this one
    if content.contains("xth_me_image_insert") {
        sketch.add(
            "warning",
            "th2-image",
            "The sketch pins a scanned image behind it. That image is not imported: place it with Sketch Scans, which can fit it on the stations.".into(),
        );
    }
    let mut scrap: Option<Scrap> = None;
    let mut line: Option<Line> = None;
    let mut area: Option<Area> = None;
    let mut first_seg = true;

    for l in logical_lines(content) {
        if l.text.is_empty() {
            continue;
        }
        let tokens = tokenize(&l.text);
        let Some(first) = tokens.first() else {
            continue;
        };
        let cmd = first.to_lowercase();

        if let Some(ln) = line.as_mut() {
            if cmd == "endline" {
                let done = line.take().unwrap_or_else(|| unreachable!());
                if let Some(s) = scrap.as_mut()
                    && !done.segs.is_empty()
                {
                    s.lines.push(done);
                }
                continue;
            }
            if is_option(&tokens[0]) {
                let (_, opts) = options(&tokens);
                apply_line_options(ln, &opts);
                continue;
            }
            if let Some(seg) = segment(&tokens, first_seg) {
                ln.segs.push(seg);
                first_seg = false;
            }
            // smooth, mark, orientation and the rest refine a vertex we already have
            continue;
        }

        if let Some(ar) = area.as_mut() {
            if cmd == "endarea" {
                let done = area.take().unwrap_or_else(|| unreachable!());
                if let Some(s) = scrap.as_mut() {
                    s.areas.push(done);
                }
                continue;
            }
            if cmd == "line" && tokens.len() > 1 {
                ar.line_ids.push(tokens[1].clone());
                continue;
            }
            // Therion's own form: the boundary lines' ids, bare, one or more to a line. The old app
            // read only `line <id>`, so every real area came through with no boundary.
            if is_option(&tokens[0]) {
                let (_, opts) = options(&tokens);
                for (k, v) in opts.iter() {
                    ar.options.set(k, v.clone());
                }
            } else {
                ar.line_ids.extend(tokens.iter().cloned());
            }
            continue;
        }

        match cmd.as_str() {
            "input" => {
                sketch.add(
                    "error",
                    "th2-input",
                    format!(
                        "This sketch pulls in another file ({}). Only what is in this file is imported -- open the other file separately.",
                        tokens.get(1).map_or("?", String::as_str)
                    ),
                );
            }
            "scrap" => {
                if let Some(s) = scrap.take() {
                    sketch.add(
                        "warning",
                        "th2-unclosed",
                        format!("Scrap \"{}\" was never closed with endscrap. Everything read before the next scrap is kept.", s.name),
                    );
                    sketch.scraps.push(s);
                }
                scrap = Some(begin_scrap(&mut sketch, &tokens));
            }
            "endscrap" => {
                if let Some(s) = scrap.take() {
                    sketch.scraps.push(s);
                }
            }
            _ => {
                let Some(s) = scrap.as_mut() else { continue };
                match cmd.as_str() {
                    "point" => read_point(s, &tokens),
                    "line" => {
                        let (words, opts) = options(&tokens[1..]);
                        let mut ln = Line {
                            kind: words
                                .first()
                                .map_or("u:unknown".into(), |w| w.to_lowercase()),
                            subtype: None,
                            closed: false,
                            reversed: false,
                            id: None,
                            options: JsObj::new(),
                            segs: Vec::new(),
                        };
                        apply_line_options(&mut ln, &opts);
                        line = Some(ln);
                        first_seg = true;
                    }
                    "area" => {
                        let (words, opts) = options(&tokens[1..]);
                        area = Some(Area {
                            kind: words
                                .first()
                                .map_or("u:unknown".into(), |w| w.to_lowercase()),
                            line_ids: Vec::new(),
                            options: opts,
                        });
                    }
                    _ => {}
                }
            }
        }
    }
    if let (Some(ln), Some(s)) = (line.take(), scrap.as_mut())
        && !ln.segs.is_empty()
    {
        s.lines.push(ln);
    }
    if let (Some(ar), Some(s)) = (area.take(), scrap.as_mut()) {
        s.areas.push(ar);
    }
    if let Some(s) = scrap.take() {
        sketch.add(
            "warning",
            "th2-unclosed",
            format!("Scrap \"{}\" was never closed with endscrap. Everything read before the end of the file is kept.", s.name),
        );
        sketch.scraps.push(s);
    }
    if sketch.scraps.is_empty() {
        sketch.add(
            "error",
            "th2-no-scraps",
            "No scrap was found in this sketch file.".into(),
        );
    }
    sketch
}
