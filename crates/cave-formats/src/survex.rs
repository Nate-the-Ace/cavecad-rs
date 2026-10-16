//! Survex .svx: reader and writer (a port of the old app's CsSurvex.js).
//!
//! The common core. Supported: `*begin`/`*end` name prefixes (joined with "."), `*data normal` with
//! its declared field order (including the length/bearing/gradient aliases and backcompass/backclino),
//! plumbed legs (compass "-", clino UP/DOWN/LEVEL...), `*data passage` (LRUD per station, matched to
//! shots by TO name; the first station's lands in `start_lrud`), `*units` length/compass/clino with an
//! optional factor, `*calibrate declination` (a ZERO ERROR: Survex subtracts it, so the model's
//! declination is its negation), `*declination <value>` (added), `*flags` splay/duplicate/surface
//! with "not", anonymous stations, `*fix`, `;` comments, `*team`, `*date`. Not supported: `*include`,
//! the diving/cartesian/nosurvey styles, other `*calibrate` corrections, `*declination auto`.
//! Survex's default length unit is metres.

use crate::js::{JsObj, is_finite_str, parse_float, split_lines, split_ws, to_fixed, trim};
use crate::model::{
    Fixed, Lrud, Shot, Survey, Trip, applied_declination, convert, grads_to_degrees,
    normalize_azimuth, unknown_elevation_note,
};
use std::collections::HashMap;

pub(crate) fn field_alias(w: &str) -> &str {
    match w {
        "length" => "tape",
        "bearing" => "compass",
        "gradient" => "clino",
        "backlength" => "backtape",
        "backbearing" => "backcompass",
        "backgradient" => "backclino",
        "ceiling" => "up",
        "floor" => "down",
        _ => w,
    }
}

const TEAM_ROLES: &[&str] = &[
    "tape",
    "length",
    "compass",
    "bearing",
    "clino",
    "gradient",
    "backtape",
    "backcompass",
    "backclino",
    "backbearing",
    "backgradient",
    "insts",
    "instruments",
    "counter",
    "depth",
    "station",
    "position",
    "notes",
    "pictures",
    "assistant",
    "altitude",
    "dimensions",
    "left",
    "right",
    "up",
    "down",
    "explorer",
];

fn clino_keyword(t: &str) -> Option<f64> {
    match t.to_lowercase().as_str() {
        "up" | "u" | "+v" => Some(90.0),
        "down" | "d" | "-v" => Some(-90.0),
        "level" | "h" => Some(0.0),
        _ => None,
    }
}

fn is_anonymous(n: &str) -> bool {
    matches!(n, "-" | "." | ".." | "...")
}

#[derive(Clone, Copy, PartialEq)]
enum Clino {
    Deg,
    Grad,
    Percent,
}

/// The running state the readers keep for units (Survex and Therion share it).
#[derive(Clone)]
pub(crate) struct Units {
    pub length_unit: &'static str,
    pub length_scale: f64,
    pub compass_grads: bool,
    clino: Clino,
}

impl Default for Units {
    fn default() -> Self {
        Units {
            length_unit: "m",
            length_scale: 1.0,
            compass_grads: false,
            clino: Clino::Deg,
        }
    }
}

impl Units {
    pub fn compass(&self, v: f64) -> f64 {
        if self.compass_grads {
            grads_to_degrees(v)
        } else {
            v
        }
    }

    pub fn clino(&self, v: f64) -> f64 {
        match self.clino {
            Clino::Grad => grads_to_degrees(v),
            Clino::Percent => (v / 100.0).atan() * 180.0 / std::f64::consts::PI,
            Clino::Deg => v,
        }
    }

    /// `*units <quantities> [factor] <unit>` / `units ...`; `feet_words` are the extra spellings
    /// of feet the format accepts.
    pub fn apply(&mut self, tokens: &[String], feet_words: &[&str]) {
        let unit_word = tokens[tokens.len() - 1].to_lowercase();
        let mut factor = 1.0;
        let mut qty_end = tokens.len() - 1;
        if qty_end > 1
            && !parse_float(&tokens[qty_end - 1]).is_nan()
            && is_finite_str(&tokens[qty_end - 1])
        {
            factor = parse_float(&tokens[qty_end - 1]);
            qty_end -= 1;
        }
        let qtys: Vec<String> = tokens[1..qty_end.max(1)]
            .iter()
            .map(|t| t.to_lowercase())
            .collect();
        let has = |q: &str| qtys.iter().any(|x| x == q);
        if has("length") || has("tape") {
            if unit_word.contains("feet") || feet_words.contains(&unit_word.as_str()) {
                self.length_unit = "ft";
                self.length_scale = factor;
            } else if unit_word.contains("yard") {
                self.length_unit = "m";
                self.length_scale = 0.9144 * factor;
            } else {
                self.length_unit = "m";
                self.length_scale = factor;
            }
        }
        let grads = unit_word.contains("grad") || unit_word.contains("mil");
        if has("compass") || has("bearing") {
            self.compass_grads = grads;
        }
        if has("clino") || has("gradient") {
            self.clino = if grads {
                Clino::Grad
            } else if unit_word.contains("percent") {
                Clino::Percent
            } else {
                Clino::Deg
            };
        }
    }
}

/// A distance read under `units`, in the survey's unit (which the first distance read decides).
pub(crate) fn into_survey_unit(survey: &mut Survey, units: &Units, v: f64) -> f64 {
    if survey.distance_unit.is_none() {
        survey.distance_unit = Some(units.length_unit.into());
    }
    convert(v * units.length_scale, units.length_unit, survey.unit())
}

/// `^(\d{4})[seps](\d{1,2})[seps](\d{1,2})` as YYYY-MM-DD.
pub(crate) fn read_date(t: &str, seps: &[u8]) -> Option<String> {
    let b = t.as_bytes();
    if b.len() < 8 || !b[..4].iter().all(u8::is_ascii_digit) || !seps.contains(&b[4]) {
        return None;
    }
    let mut i = 5;
    let m0 = i;
    while i < b.len() && i - m0 < 2 && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == m0 || i >= b.len() || !seps.contains(&b[i]) {
        return None;
    }
    let month = &t[m0..i];
    i += 1;
    let d0 = i;
    while i < b.len() && i - d0 < 2 && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == d0 {
        return None;
    }
    let day = &t[d0..i];
    let two = |s: &str| {
        if s.len() < 2 {
            format!("0{s}")
        } else {
            s.to_string()
        }
    };
    Some(format!("{}-{}-{}", &t[..4], two(month), two(day)))
}

/// A `*calibrate` correction: `(reading - zero) * scale`.
#[derive(Clone, Copy)]
struct Correction {
    zero: f64,
    scale: f64,
}

impl Default for Correction {
    fn default() -> Self {
        Correction {
            zero: 0.0,
            scale: 1.0,
        }
    }
}

impl Correction {
    fn apply(&self, v: f64) -> f64 {
        (v - self.zero) * self.scale
    }
}

/// The corrections in force, per quantity.
#[derive(Clone, Copy, Default)]
struct Calib {
    tape: Correction,
    compass: Correction,
    clino: Correction,
    backcompass: Correction,
    backclino: Correction,
}

/// Every setting Survex saves at `*begin` and restores at `*end`.
#[derive(Clone)]
struct Settings {
    units: Units,
    declination: f64,
    flags: Flags,
    style: &'static str,
    normal: Vec<String>,
    passage: Vec<String>,
    calib: Calib,
    /// `*case`: 'L' tolower (Survex's default: names that differ only in case are one), 'U'
    /// toupper, 'P' preserve.
    case: char,
    decimal_comma: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            units: Units::default(),
            declination: 0.0,
            flags: Flags::default(),
            style: "normal",
            normal: ["from", "to", "tape", "compass", "clino"]
                .map(String::from)
                .to_vec(),
            passage: ["station", "left", "right", "up", "down"]
                .map(String::from)
                .to_vec(),
            calib: Calib::default(),
            case: 'L',
            decimal_comma: false,
        }
    }
}

impl Settings {
    /// `*calibrate <quantities> <zero error> [<units>] [<scale>]`, or `*calibrate default`.
    fn calibrate(&mut self, args: &[String]) {
        if args
            .first()
            .is_some_and(|a| a.eq_ignore_ascii_case("default"))
        {
            self.calib = Calib::default();
            return;
        }
        let Some(at) = args
            .iter()
            .position(|a| !parse_float(a).is_nan() && is_finite_str(a))
        else {
            return;
        };
        let zero = parse_float(&args[at]);
        let rest = &args[at + 1..];
        // an optional units word, then an optional scale
        let (unit_word, scale_at) = match rest.first() {
            Some(w) if parse_float(w).is_nan() => (Some(w.to_lowercase()), 1),
            _ => (None, 0),
        };
        let scale = rest
            .get(scale_at)
            .map(|s| parse_float(s))
            .filter(|v| !v.is_nan())
            .unwrap_or(1.0);
        for q in &args[..at] {
            let q = q.to_lowercase();
            let angle = |z: f64| match unit_word.as_deref() {
                Some(u) if u.contains("grad") || u.contains("mil") => {
                    crate::model::grads_to_degrees(z)
                }
                _ => z,
            };
            let length = |z: f64| match unit_word.as_deref() {
                Some(u) if u.contains("feet") || u == "ft" => {
                    crate::model::convert(z, "ft", self.units.length_unit)
                }
                Some(u) if u.starts_with("met") || u == "m" => {
                    crate::model::convert(z, "m", self.units.length_unit) / self.units.length_scale
                }
                _ => z,
            };
            match q.as_str() {
                "tape" | "length" => {
                    self.calib.tape = Correction {
                        zero: length(zero),
                        scale,
                    }
                }
                "compass" | "bearing" => {
                    self.calib.compass = Correction {
                        zero: angle(zero),
                        scale,
                    }
                }
                "clino" | "gradient" => {
                    self.calib.clino = Correction {
                        zero: angle(zero),
                        scale,
                    }
                }
                "backcompass" | "backbearing" => {
                    self.calib.backcompass = Correction {
                        zero: angle(zero),
                        scale,
                    }
                }
                "backclino" | "backgradient" => {
                    self.calib.backclino = Correction {
                        zero: angle(zero),
                        scale,
                    }
                }
                _ => {}
            }
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Flags {
    splay: bool,
    duplicate: bool,
    surface: bool,
}

/// The name in a `*include` line (quoted or not), if the line is one.
fn include_name(line: &str) -> Option<String> {
    let code = line.split(';').next().unwrap_or("");
    let t = crate::js::trim_start(code);
    let rest = t.strip_prefix('*')?;
    let rest = crate::js::trim_start(rest);
    if !rest
        .get(..7)
        .is_some_and(|w| w.eq_ignore_ascii_case("include"))
    {
        return None;
    }
    let after = &rest[7..];
    if !after.starts_with(crate::js::is_space) {
        return None;
    }
    let arg = trim(after);
    match arg.strip_prefix('"') {
        Some(q) => Some(q.split('"').next().unwrap_or("").to_string()),
        None => arg.split_whitespace().next().map(str::to_string),
    }
}

/// The ways Survex tries a file name it was given: as written, with ".svx", with backslashes as
/// slashes, lower case, an upper-case first letter, all upper case.
fn include_candidates(dir: &str, name: &str) -> Vec<String> {
    let name = name.replace('\\', "/");
    let has_ext = name
        .rsplit('/')
        .next()
        .is_some_and(|leaf| leaf.contains('.'));
    let mut variants = vec![name.clone(), name.to_lowercase()];
    let leaf_at = name.rfind('/').map_or(0, |i| i + 1);
    let lower = name.to_lowercase();
    let mut cap = lower.clone();
    if let Some(c) = cap[leaf_at..].chars().next() {
        cap.replace_range(
            leaf_at..leaf_at + c.len_utf8(),
            &c.to_uppercase().to_string(),
        );
    }
    variants.push(cap);
    variants.push(name.to_uppercase());
    let mut out = Vec::new();
    for v in variants {
        let base = if v.starts_with('/') || dir.is_empty() {
            v.clone()
        } else {
            format!("{dir}/{v}")
        };
        if !has_ext {
            out.push(format!("{base}.svx"));
            out.push(format!("{base}.SVX"));
        }
        out.push(base);
    }
    out.dedup();
    out
}

/// Splices every `*include` into the text where it stands (Survex reads one stream: settings made
/// in an included file carry on after it). Returns the text and what could not be followed.
pub(crate) fn expand_includes(
    content: &str,
    path: &str,
    read: &dyn Fn(&str) -> Option<String>,
) -> (String, Vec<String>) {
    fn walk(
        text: &str,
        here: &str,
        stack: &mut Vec<String>,
        read: &dyn Fn(&str) -> Option<String>,
        problems: &mut Vec<String>,
    ) -> String {
        let dir = match here.replace('\\', "/").rfind('/') {
            Some(i) => here[..i].to_string(),
            None => String::new(),
        };
        let mut out = Vec::new();
        for line in split_lines(text) {
            let Some(name) = include_name(&line) else {
                out.push(line);
                continue;
            };
            let found = include_candidates(&dir, &name)
                .into_iter()
                .find_map(|c| read(&c).map(|t| (c, t)));
            match found {
                None => {
                    problems.push(format!("This file includes \"{name}\", and it could not be read -- every passage in it is missing from this import."));
                    out.push(format!("; *include {name} (not found)"));
                }
                Some((p, _)) if stack.contains(&p) || stack.len() > 50 => {
                    problems.push(format!("\"{name}\" includes itself, directly or through other files, so it was read once."));
                }
                Some((p, t)) => {
                    stack.push(p.clone());
                    out.push(walk(&t, &p, stack, read, problems));
                    stack.pop();
                }
            }
        }
        out.join("\n")
    }
    let mut problems = Vec::new();
    let mut stack = vec![path.to_string()];
    let text = walk(content, path, &mut stack, read, &mut problems);
    (text, problems)
}

/// Reads a Survex .svx file at `path`, following its `*include`s through `read`.
pub fn parse_with_includes(
    content: &str,
    path: &str,
    read: &dyn Fn(&str) -> Option<String>,
) -> Survey {
    let (text, problems) = expand_includes(content, path, read);
    let mut survey = parse(&text);
    for p in problems {
        survey.add_finding("warning", "survex-include", p);
    }
    survey
}

/// Reads a Survex .svx file.
pub fn parse(content: &str) -> Survey {
    let mut survey = Survey {
        distance_unit: None,
        ..Survey::default()
    };
    // *equate groups, by full station name
    let mut equates: Vec<Vec<String>> = Vec::new();
    let mut passage: JsObj<Lrud> = JsObj::new();
    let mut prefix: Vec<String> = Vec::new();
    // everything *begin saves and *end restores
    let mut st = Settings::default();
    let mut saved: Vec<Settings> = Vec::new();
    // the first spelling of each station name, by its case-folded form
    let mut spelled: HashMap<String, String> = HashMap::new();
    let mut team_dirty = false;
    let mut cur_date = String::new();
    let mut cur_team = String::new();
    // A station's full name, with Survex's *case applied: by default (tolower) names differ only
    // in case are one station, shown as first written; toupper shows them upper case; preserve
    // keeps them apart.
    let full =
        |prefix: &[String], name: &str, case: char, spelled: &mut HashMap<String, String>| {
            let n = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{}.{name}", prefix.join("."))
            };
            match case {
                'P' => n,
                'U' => n.to_uppercase(),
                _ => spelled.entry(n.to_lowercase()).or_insert(n).clone(),
            }
        };

    for (li, raw) in split_lines(content).iter().enumerate() {
        let (code, notes) = match raw.find(';') {
            Some(i) => (&raw[..i], trim(&raw[i + 1..]).to_string()),
            None => (raw.as_str(), String::new()),
        };
        let line = trim(code);
        if line.is_empty() {
            continue;
        }

        if line.starts_with('*') {
            let tokens = split_ws(line);
            let cmd = tokens[0].to_lowercase();
            match cmd.as_str() {
                "*begin" => {
                    prefix.push(
                        tokens
                            .get(1)
                            .cloned()
                            .unwrap_or_else(|| format!("anon{li}")),
                    );
                    saved.push(st.clone());
                    if survey.name.is_empty() && tokens.len() > 1 {
                        survey.name = tokens[1].clone();
                    }
                }
                "*end" => {
                    prefix.pop();
                    if let Some(f) = saved.pop() {
                        st = f;
                    }
                }
                "*date" if tokens.len() > 1 => {
                    let w = tokens[1].to_lowercase();
                    let dtok = if (w == "surveyed" || w == "explored") && tokens.len() > 2 {
                        &tokens[2]
                    } else {
                        &tokens[1]
                    };
                    let new_date = read_date(dtok, b".-");
                    if team_dirty
                        || new_date
                            .as_ref()
                            .is_some_and(|d| !cur_date.is_empty() && *d != cur_date)
                    {
                        cur_team.clear();
                        survey.team.clear();
                        team_dirty = false;
                    }
                    cur_date = new_date.unwrap_or_else(|| dtok.replace('.', "-"));
                    survey.date = cur_date.clone();
                }
                "*team" if tokens.len() > 1 => {
                    let member = match quoted(line) {
                        Some(q) => q,
                        None => tokens[1..]
                            .iter()
                            .take_while(|t| !TEAM_ROLES.contains(&t.to_lowercase().as_str()))
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(" "),
                    };
                    if !member.is_empty() {
                        cur_team = if cur_team.is_empty() {
                            member
                        } else {
                            format!("{cur_team}, {member}")
                        };
                        survey.team = cur_team.clone();
                    }
                }
                "*flags" => {
                    let mut negate = false;
                    for t in &tokens[1..] {
                        let f = t.to_lowercase();
                        if f == "not" {
                            negate = true;
                            continue;
                        }
                        match f.as_str() {
                            "splay" => st.flags.splay = !negate,
                            "duplicate" => st.flags.duplicate = !negate,
                            "surface" => st.flags.surface = !negate,
                            _ => {}
                        }
                        negate = false;
                    }
                }
                "*data" => {
                    if tokens.len() == 1 {
                        st.style = "normal";
                    } else {
                        let kind = tokens[1].to_lowercase();
                        let fields = || {
                            tokens[2..]
                                .iter()
                                .map(|t| field_alias(&t.to_lowercase()).to_string())
                                .collect()
                        };
                        if kind == "normal" {
                            st.style = "normal";
                            st.normal = fields();
                        } else if kind == "passage" {
                            st.style = "passage";
                            st.passage = fields();
                        } else {
                            st.style = "other";
                        }
                    }
                }
                "*units" => st.units.apply(&tokens, &["ft"]),
                "*calibrate" => {
                    if tokens.len() >= 3 && tokens[1].to_lowercase() == "declination" {
                        let d = parse_float(&tokens[2]);
                        if !d.is_nan() {
                            st.declination = -d;
                            survey.declination = -d;
                            survey.declination_source = "file".into();
                        }
                    } else {
                        st.calibrate(&tokens[1..]);
                    }
                }
                "*case" if tokens.len() > 1 => {
                    st.case = match tokens[1].to_lowercase().as_str() {
                        "preserve" => 'P',
                        "toupper" => 'U',
                        _ => 'L',
                    };
                }
                "*set" if tokens.len() > 2 && tokens[1].eq_ignore_ascii_case("decimal") => {
                    st.decimal_comma = tokens[2].contains(',');
                }
                "*declination" => {
                    if tokens.len() >= 2 && tokens[1].to_lowercase() != "auto" {
                        let d = parse_float(&tokens[1]);
                        if !d.is_nan() {
                            st.declination = d;
                            survey.declination = d;
                            survey.declination_source = "file".into();
                        }
                    }
                }
                "*fix" => {
                    let ft = if tokens.len() > 2 && tokens[2].to_lowercase() == "reference" {
                        3
                    } else {
                        2
                    };
                    if tokens.len() >= ft + 2 {
                        let fx = parse_float(&tokens[ft]);
                        let fy = parse_float(&tokens[ft + 1]);
                        let fz = if tokens.len() > ft + 2 {
                            parse_float(&tokens[ft + 2])
                        } else {
                            0.0
                        };
                        if !fx.is_nan() && !fy.is_nan() {
                            let x = into_survey_unit(&mut survey, &st.units, fx);
                            let y = into_survey_unit(&mut survey, &st.units, fy);
                            let z = if fz.is_nan() {
                                0.0
                            } else {
                                into_survey_unit(&mut survey, &st.units, fz)
                            };
                            survey.fixed.set(
                                &full(&prefix, &tokens[1], st.case, &mut spelled),
                                Fixed::at(x, y, z),
                            );
                        }
                    }
                }
                "*equate" if tokens.len() > 2 => {
                    equates.push(
                        tokens[1..]
                            .iter()
                            .map(|t| full(&prefix, t, st.case, &mut spelled))
                            .collect(),
                    );
                }
                _ => {}
            }
            continue;
        }

        let mut fields = split_ws(line);
        if st.decimal_comma {
            // "*set decimal ,": a comma is the decimal point
            for f in &mut fields {
                *f = f.replace(',', ".");
            }
        }
        if st.style == "normal" {
            let mut rec: JsObj<String> = JsObj::new();
            for (k, v) in data_fields(line, &st.normal, st.decimal_comma) {
                rec.set(&k, v);
            }
            let (Some(from), Some(to), Some(tape)) =
                (rec.get("from"), rec.get("to"), rec.get("tape"))
            else {
                continue;
            };
            let tape = parse_float(tape);
            if tape.is_nan() {
                continue;
            }
            let tape = st.calib.tape.apply(tape);
            let (from, to) = (from.clone(), to.clone());
            let mut clino = 0.0;
            let mut plumbed = false;
            if let Some(c) = rec.get("clino") {
                match clino_keyword(c) {
                    Some(k) => {
                        clino = k;
                        plumbed = k != 0.0;
                    }
                    None => {
                        let v = parse_float(c);
                        clino = if !v.is_nan() {
                            st.calib.clino.apply(st.units.clino(v))
                        } else if has_reading(rec.get("backclino")) {
                            f64::NAN // the backsight carries it
                        } else {
                            0.0
                        };
                        plumbed = clino.abs() >= 89.999;
                    }
                }
            }
            let mut compass = rec.get("compass").map_or(f64::NAN, |c| parse_float(c));
            let mut omitted = false;
            if compass.is_nan() && has_reading(rec.get("backcompass")) {
                // only the backsight was read: it carries the bearing
            } else if compass.is_nan() {
                if !plumbed {
                    survey.add_finding(
                        "warning",
                        "bearing-omitted-not-plumb",
                        format!(
                            "Line {}: leg {from} to {to} has no compass reading and is not plumb (clino {}), so the leg was SKIPPED -- anything beyond it is now unconnected. A bearing may only be omitted on a plumbed shot; record the clino as 90 (or UP/DOWN) if it really was one.",
                            li + 1,
                            to_fixed(clino, 2)
                        ),
                    );
                    continue;
                }
                compass = 0.0;
                omitted = true;
            } else {
                compass = st.calib.compass.apply(st.units.compass(compass));
            }

            let anon = is_anonymous(&to);
            let distance = into_survey_unit(&mut survey, &st.units, tape);
            let mut shot = Shot {
                splay: anon || st.flags.splay,
                from: full(&prefix, &from, st.case, &mut spelled),
                to: if anon {
                    String::new()
                } else {
                    full(&prefix, &to, st.case, &mut spelled)
                },
                distance,
                azimuth_omitted: omitted,
                azimuth: normalize_azimuth(compass + st.declination),
                declination: Some(st.declination),
                inclination: clino,
                exclude_from_length: st.flags.duplicate || st.flags.surface,
                exclude_from_plot: st.flags.surface,
                ..Shot::default()
            };
            if let Some(bc) = rec.get("backcompass") {
                let v = parse_float(bc);
                if !v.is_nan() {
                    shot.back_azimuth = Some(normalize_azimuth(
                        st.calib.backcompass.apply(st.units.compass(v)) + st.declination,
                    ));
                }
            }
            if let Some(bc) = rec.get("backclino") {
                match clino_keyword(bc) {
                    Some(k) => shot.back_inclination = Some(k),
                    None => {
                        let v = parse_float(bc);
                        if !v.is_nan() {
                            shot.back_inclination =
                                Some(st.calib.backclino.apply(st.units.clino(v)));
                        }
                    }
                }
            }
            shot.notes = notes;
            shot.trip = survey.trip_id_for(Trip {
                date: cur_date.clone(),
                team: cur_team.clone(),
                declination: st.declination,
                ..Trip::default()
            });
            team_dirty = true;
            survey.shots.push(shot);
        } else if st.style == "passage" {
            let mut rec: JsObj<String> = JsObj::new();
            for (k, v) in data_fields(line, &st.passage, st.decimal_comma) {
                rec.set(&k, v);
            }
            let Some(station) = rec.get("station").cloned() else {
                continue;
            };
            let mut num = |k: &str| match rec.get(k) {
                None => None,
                Some(v) if v == "-" => None,
                Some(v) => {
                    let n = parse_float(v);
                    if n.is_nan() {
                        None
                    } else {
                        Some(into_survey_unit(&mut survey, &st.units, n))
                    }
                }
            };
            let l = Lrud::of(num("left"), num("right"), num("up"), num("down"));
            passage.set(&full(&prefix, &station, st.case, &mut spelled), l);
        }
    }

    if !equates.is_empty() {
        join_equates(&mut survey, &mut passage, &equates);
    }
    for s in &mut survey.shots {
        if !s.splay
            && let Some(p) = passage.get(&s.to)
        {
            (s.left, s.right, s.up, s.down) = (p.left, p.right, p.up, p.down);
        }
    }
    if let Some(p) = survey.shots.first().and_then(|s| passage.get(&s.from)) {
        let start = Lrud::of(p.left, p.right, p.up, p.down);
        survey.start_lrud = Some(start.clone());
        survey.trips[0].start_lrud = Some(start);
    }
    if survey.distance_unit.is_none() {
        survey.distance_unit = Some(st.units.length_unit.into());
    }
    survey.ensure_trips();
    survey
}

/// The fields of a data line, read as Survex reads them: a station name runs to the next blank,
/// but a reading ends where its number does, so `8.2-down` is tape 8.2, compass omitted, clino
/// down. `ignoreall` takes the rest of the line.
fn data_fields(line: &str, order: &[String], decimal_comma: bool) -> Vec<(String, String)> {
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    let blank = |c: char| crate::js::is_space(c);
    for name in order {
        while i < chars.len() && blank(chars[i]) {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        let start = i;
        let word = |i: &mut usize| {
            while *i < chars.len() && !blank(chars[*i]) {
                *i += 1;
            }
        };
        match name.as_str() {
            "from" | "to" | "station" | "ignore" | "newline" => word(&mut i),
            "ignoreall" => i = chars.len(),
            _ => {
                // a reading: a number, else "-" (omitted, any run of them), else a word
                let mut j = i;
                if j < chars.len() && matches!(chars[j], '+' | '-') {
                    j += 1;
                }
                let digits_start = j;
                let point = if decimal_comma { ',' } else { '.' };
                while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == point) {
                    j += 1;
                }
                if j > digits_start && chars[digits_start..j].iter().any(char::is_ascii_digit) {
                    i = j;
                } else if chars[i] == '-'
                    && (i + 1 >= chars.len() || !chars[i + 1].is_ascii_alphabetic())
                {
                    while i < chars.len() && chars[i] == '-' {
                        i += 1;
                    }
                } else if chars[i] == '-'
                    && !(name.ends_with("clino") && {
                        let mut k = i;
                        word(&mut k);
                        clino_keyword(&chars[i..k].iter().collect::<String>()).is_some()
                    })
                {
                    i += 1; // "-down": compass omitted, then the clino ("-V" is a clino of its own)
                } else {
                    word(&mut i);
                }
            }
        }
        let text: String = chars[start..i].iter().collect();
        let text = if decimal_comma {
            text.replace(',', ".")
        } else {
            text
        };
        out.push((name.clone(), text));
    }
    out
}

/// `*equate`: every member of a group is one station. Each is renamed to the group's first member
/// that a shot uses (else its first), in the shots, the per-station LRUD and the fixed points.
fn join_equates(survey: &mut Survey, passage: &mut JsObj<Lrud>, groups: &[Vec<String>]) {
    let mut parent: HashMap<String, String> = HashMap::new();
    fn find(parent: &mut HashMap<String, String>, n: &str) -> String {
        let mut n = n.to_string();
        while let Some(p) = parent.get(&n).cloned() {
            if p == n {
                break;
            }
            let gp = parent.get(&p).cloned().unwrap_or_else(|| p.clone());
            parent.insert(n.clone(), gp.clone());
            n = gp;
        }
        n
    }
    let mut order: Vec<String> = Vec::new();
    for g in groups {
        for n in g {
            if !parent.contains_key(n) {
                parent.insert(n.clone(), n.clone());
                order.push(n.clone());
            }
        }
        for n in &g[1..] {
            let (a, b) = (find(&mut parent, &g[0]), find(&mut parent, n));
            if a != b {
                parent.insert(b, a);
            }
        }
    }
    let used: std::collections::HashSet<&str> = survey
        .shots
        .iter()
        .flat_map(|s| [s.from.as_str(), s.to.as_str()])
        .collect();
    let mut canon: HashMap<String, String> = HashMap::new();
    for n in &order {
        let root = find(&mut parent, n);
        let replace = match canon.get(&root) {
            None => true,
            Some(c) => !used.contains(c.as_str()) && used.contains(n.as_str()),
        };
        if replace {
            canon.insert(root, n.clone());
        }
    }
    let mut rename: HashMap<String, String> = HashMap::new();
    for n in &order {
        let root = find(&mut parent, n);
        if let Some(c) = canon.get(&root)
            && c != n
        {
            rename.insert(n.clone(), c.clone());
        }
    }
    for (alias, kept) in &rename {
        survey.aliases.set(alias, kept.clone());
    }
    let re = |n: &str| rename.get(n).cloned().unwrap_or_else(|| n.to_string());
    for s in &mut survey.shots {
        s.from = re(&s.from);
        if !s.to.is_empty() {
            s.to = re(&s.to);
        }
    }
    let mut joined: JsObj<Lrud> = JsObj::new();
    for (n, l) in passage.iter() {
        let t = re(n);
        if !joined.has(&t) || t == n {
            joined.set(&t, l.clone());
        }
    }
    *passage = joined;
    let mut fixed: JsObj<Fixed> = JsObj::new();
    for (n, f) in survey.fixed.iter() {
        let t = re(n);
        if !fixed.has(&t) || t == n {
            fixed.set(&t, f.clone());
        }
    }
    survey.fixed = fixed;
}

/// Whether a field holds a number.
pub(crate) fn has_reading(v: Option<&String>) -> bool {
    v.is_some_and(|v| !parse_float(v).is_nan())
}

/// The first `"..."` in a line.
pub(crate) fn quoted(line: &str) -> Option<String> {
    let start = line.find('"')? + 1;
    let len = line[start..].find('"')?;
    Some(line[start..start + len].to_string())
}

/// Writes a survey as a Survex .svx file. `*date`/`*team` go before each trip's legs;
/// `*declination` follows the leg (it is running state in Survex); bearings go back out magnetic;
/// splays become ".." stations; excluded shots become comments; LRUD is written per station.
pub fn write(survey: &mut Survey) -> String {
    survey.ensure_trips();
    let mut out: Vec<String> = Vec::new();
    let title = if !survey.cave_name.is_empty() {
        survey.cave_name.as_str()
    } else if !survey.name.is_empty() {
        survey.name.as_str()
    } else {
        "Cave survey"
    };
    out.push(format!("; {title}"));
    if survey.distance_unit.as_deref() == Some("ft") {
        out.push("*units length feet".into());
    }
    for (name, f) in survey.fixed.iter() {
        let fz = f.z();
        if fz.is_none() {
            out.push(format!("; {}", unknown_elevation_note(name)));
        }
        out.push(format!(
            "*fix {name} {} {} {}",
            to_fixed(f.x.unwrap_or(f64::NAN), 2),
            to_fixed(f.y.unwrap_or(f64::NAN), 2),
            to_fixed(fz.unwrap_or(0.0), 2)
        ));
    }
    let any_back = survey.shots.iter().any(Shot::has_backsight);
    out.push(format!(
        "*data normal from to tape compass clino{}",
        if any_back {
            " backcompass backclino"
        } else {
            ""
        }
    ));

    let mut trip_order: Vec<usize> = Vec::new();
    for s in &survey.shots {
        if !trip_order.contains(&s.trip) {
            trip_order.push(s.trip);
        }
    }
    trip_order.sort_unstable();

    let mut cur = Flags::default();
    let dash = |v: Option<f64>| v.map_or_else(|| "-".to_string(), |x| to_fixed(x, 2));
    let mut last_decl = 0.0;
    for &t in &trip_order {
        let trip = survey.trips.get(t);
        if let Some(trip) = trip {
            if !trip.date.is_empty() {
                out.push(format!("*date {}", trip.date));
            }
            if !trip.team.is_empty() {
                for m in trip.team.split(',') {
                    let m = trim(m);
                    if !m.is_empty() {
                        out.push(format!("*team \"{m}\""));
                    }
                }
            }
        }
        for s in survey.shots.iter().filter(|s| s.trip == t) {
            if s.exclude_from_all {
                out.push(format!(
                    "; excluded shot: {} {} {}{}",
                    s.from,
                    if s.to.is_empty() { "-" } else { &s.to },
                    to_fixed(s.distance, 2),
                    if s.notes.is_empty() {
                        String::new()
                    } else {
                        format!(" ; {}", s.notes)
                    }
                ));
                continue;
            }
            let decl = applied_declination(s, trip);
            if decl != last_decl {
                out.push(format!("*declination {} degrees", to_fixed(decl, 2)));
                last_decl = decl;
            }
            let want = Flags {
                splay: false,
                duplicate: s.exclude_from_length && !s.exclude_from_plot,
                surface: s.exclude_from_plot,
            };
            if want.duplicate != cur.duplicate || want.surface != cur.surface {
                let mut parts = Vec::new();
                if want.duplicate != cur.duplicate {
                    parts.push(format!(
                        "{}duplicate",
                        if want.duplicate { "" } else { "not " }
                    ));
                }
                if want.surface != cur.surface {
                    parts.push(format!("{}surface", if want.surface { "" } else { "not " }));
                }
                out.push(format!("*flags {}", parts.join(" ")));
                cur = want;
            }
            let az = normalize_azimuth(s.azimuth - decl);
            let to_name = if s.splay && s.to.is_empty() {
                ".."
            } else {
                s.to.as_str()
            };
            // no bearing sighted, or only a backsight taken: Survex's own "-"
            let az_out = if s.azimuth_omitted || !s.azimuth.is_finite() {
                "-".to_string()
            } else {
                to_fixed(az, 2)
            };
            let mut line = format!(
                "{}\t{to_name}\t{}\t{az_out}\t{}",
                s.from,
                to_fixed(s.distance, 2),
                if s.inclination.is_finite() {
                    to_fixed(s.inclination, 2)
                } else {
                    "-".to_string()
                }
            );
            if any_back {
                let b = s.back_azimuth.map(|b| normalize_azimuth(b - decl));
                line += &format!("\t{}\t{}", dash(b), dash(s.back_inclination));
            }
            let with_note = if s.notes.is_empty() {
                line
            } else {
                format!("{line}\t; {}", s.notes)
            };
            if s.splay && !s.to.is_empty() {
                out.push("*flags splay".into());
                out.push(with_note);
                out.push("*flags not splay".into());
                continue;
            }
            out.push(with_note);
        }
    }
    if cur.duplicate || cur.surface {
        let mut offs = Vec::new();
        if cur.duplicate {
            offs.push("not duplicate");
        }
        if cur.surface {
            offs.push("not surface");
        }
        out.push(format!("*flags {}", offs.join(" ")));
    }

    let mut by_station: JsObj<Lrud> = JsObj::new();
    let mut order: Vec<String> = Vec::new();
    let mut conflicts: Vec<String> = Vec::new();
    if let (Some(sl), Some(first)) = (&survey.start_lrud, survey.shots.first())
        && sl.any()
    {
        by_station.set(&first.from, Lrud::of(sl.left, sl.right, sl.up, sl.down));
        order.push(first.from.clone());
    }
    for s in &survey.shots {
        if s.splay || s.exclude_from_all || !s.any_lrud() {
            continue;
        }
        match by_station.get(&s.to) {
            Some(prev) => {
                if prev.left != s.left
                    || prev.right != s.right
                    || prev.up != s.up
                    || prev.down != s.down
                {
                    conflicts.push(s.to.clone());
                }
            }
            None => order.push(s.to.clone()),
        }
        by_station.set(&s.to, Lrud::of(s.left, s.right, s.up, s.down));
    }
    if !order.is_empty() {
        out.push("*data passage station left right up down".into());
        for name in &order {
            let Some(l) = by_station.get(name) else {
                continue;
            };
            let mut line = format!(
                "{name}\t{}\t{}\t{}\t{}",
                dash(l.left),
                dash(l.right),
                dash(l.up),
                dash(l.down)
            );
            if conflicts.contains(name) {
                line += &format!(
                    "\t; two shots into {name} disagreed; kept the later reading (Survex stores LRUD per station)"
                );
            }
            out.push(line);
        }
    }
    out.join("\n") + "\n"
}
