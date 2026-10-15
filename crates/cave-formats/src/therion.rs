//! Therion .th: reader and writer (a port of the old app's CsTherion.js).
//!
//! One reader is the door for three programs: TopoDroid writes .th natively and PocketTopo exports
//! it. Supported: survey/endsurvey nesting as name prefixes (joined with "."), centreline blocks,
//! date, team, units, declination (east-positive, added), `data normal` with its field order and LRUD
//! on the leg, `data dimensions`/`topofil` LRUD per station, plumbed legs, flags, fix, `#` comments,
//! backslash continuation, `input` (spliced in where it stands, when a file reader is given) and
//! `equate` (members of a class renamed to one name). The diving/cartesian/cylpolar/nosurvey styles
//! are reported, not read; scraps and maps are drawing data and skipped.
//!
//! STATION NAMES: nested surveys join with "." like Survex ("inner.1", not Therion's "1@inner").
//! When a file has exactly ONE top-level survey its name is NOT prefixed: it is the cave, the same
//! word on every station, and noise on the map.

use crate::js::{
    JsObj, is_finite_str, parse_float, split_lines, to_fixed, trim, trim_end, trim_start,
};
use crate::model::{
    Fixed, Lrud, Shot, Survey, Trip, applied_declination, grads_to_degrees, normalize_azimuth,
    unknown_elevation_note,
};
use crate::survex::{Units, field_alias, into_survey_unit, read_date};

const TEAM_ROLES: &[&str] = &[
    "tape",
    "length",
    "compass",
    "bearing",
    "clino",
    "gradient",
    "counter",
    "depth",
    "station",
    "position",
    "notes",
    "pictures",
    "pics",
    "insts",
    "instruments",
    "assistant",
    "altitude",
    "dimensions",
    "dog",
    "book",
    "explorer",
    "topofil",
];

fn clino_keyword(t: &str) -> Option<f64> {
    match t.to_lowercase().as_str() {
        "up" | "u" | "+v" | "plumb" => Some(90.0),
        "down" | "d" | "-v" => Some(-90.0),
        "level" | "h" | "horizontal" => Some(0.0),
        _ => None,
    }
}

fn is_anonymous(n: &str) -> bool {
    matches!(n, "-" | "." | ".." | "...")
}

/// Splits a line into tokens, keeping "quoted strings" (unwrapped) and [bracket groups] whole.
pub fn tokenize(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let mut k = 0;
    while k < chars.len() {
        let (i, ch) = chars[k];
        if crate::js::is_space(ch) {
            k += 1;
            continue;
        }
        if ch == '"' {
            match line[i + 1..].find('"') {
                None => {
                    tokens.push(line[i + 1..].to_string());
                    break;
                }
                Some(e) => {
                    let end = i + 1 + e;
                    tokens.push(line[i + 1..end].to_string());
                    k = chars.partition_point(|(p, _)| *p <= end);
                    continue;
                }
            }
        }
        if ch == '[' {
            match line[i + 1..].find(']') {
                None => {
                    tokens.push(line[i..].to_string());
                    break;
                }
                Some(e) => {
                    let end = i + 1 + e;
                    tokens.push(line[i..=end].to_string());
                    k = chars.partition_point(|(p, _)| *p <= end);
                    continue;
                }
            }
        }
        let start = i;
        while k < chars.len() && !crate::js::is_space(chars[k].1) {
            k += 1;
        }
        let end = chars.get(k).map_or(line.len(), |(p, _)| *p);
        tokens.push(line[start..end].to_string());
    }
    tokens
}

/// Strips a `#` comment outside quotes: (text, comment).
fn split_comment(line: &str) -> (String, String) {
    let mut in_quote = false;
    for (i, ch) in line.char_indices() {
        if ch == '"' {
            in_quote = !in_quote;
        } else if ch == '#' && !in_quote {
            return (line[..i].to_string(), trim(&line[i + 1..]).to_string());
        }
    }
    (line.to_string(), String::new())
}

struct Logical {
    text: String,
    comment: String,
}

/// Joins backslash-continued lines, each logical line keeping its comments.
fn logical_lines(content: &str) -> Vec<Logical> {
    let mut out = Vec::new();
    let mut pending: Option<Logical> = None;
    for raw in split_lines(content) {
        let (text, comment) = split_comment(&raw);
        let mut text = trim_end(&text).to_string();
        let continues = text.ends_with('\\');
        if continues {
            text.pop();
        }
        match pending.as_mut() {
            None => pending = Some(Logical { text, comment }),
            Some(p) => {
                p.text += " ";
                p.text += trim_start(&text);
                if !comment.is_empty() {
                    p.comment = if p.comment.is_empty() {
                        comment
                    } else {
                        format!("{} {comment}", p.comment)
                    };
                }
            }
        }
        if !continues && let Some(p) = pending.take() {
            out.push(Logical {
                text: trim(&p.text).to_string(),
                comment: p.comment,
            });
        }
    }
    if let Some(p) = pending {
        out.push(Logical {
            text: trim(&p.text).to_string(),
            comment: p.comment,
        });
    }
    out
}

/// How many survey blocks the file opens at its top level.
fn top_level_surveys(lines: &[Logical]) -> usize {
    let (mut depth, mut count, mut skip) = (0usize, 0usize, 0usize);
    for l in lines {
        let tokens = tokenize(&l.text);
        let Some(first) = tokens.first() else {
            continue;
        };
        let cmd = first.to_lowercase();
        if skip > 0 {
            if cmd == "scrap" || cmd == "map" {
                skip += 1;
            } else if cmd == "endscrap" || cmd == "endmap" {
                skip -= 1;
            }
            continue;
        }
        if cmd == "scrap" || cmd == "map" {
            skip = 1;
        } else if cmd == "survey" {
            if depth == 0 {
                count += 1;
            }
            depth += 1;
        } else if cmd == "endsurvey" && depth > 0 {
            depth -= 1;
        }
    }
    count
}

fn dir_of(p: &str) -> String {
    let cut = p.rfind(['/', '\\']);
    match cut {
        None => ".".into(),
        Some(0) => "/".into(),
        Some(i) => p[..i].to_string(),
    }
}

fn is_absolute(p: &str) -> bool {
    let b = p.as_bytes();
    matches!(b.first(), Some(b'/' | b'\\'))
        || (b.len() >= 3
            && b[0].is_ascii_alphabetic()
            && b[1] == b':'
            && matches!(b[2], b'/' | b'\\'))
}

/// A finding from following `input` lines.
struct InputFinding {
    code: &'static str,
    text: String,
}

/// `^\s*input\s+(?:"([^"]+)"|(\S+))` (case-insensitive): the name.
fn input_name(line: &str) -> Option<String> {
    let t = trim_start(line);
    if !t.get(..5).is_some_and(|w| w.eq_ignore_ascii_case("input")) {
        return None;
    }
    let rest = &t[5..];
    let after = trim_start(rest);
    if after.len() == rest.len() || after.is_empty() {
        return None;
    }
    if let Some(q) = after.strip_prefix('"')
        && let Some(e) = q.find('"')
        && e > 0
    {
        return Some(q[..e].to_string());
    }
    let n: String = after
        .chars()
        .take_while(|c| !crate::js::is_space(*c))
        .collect();
    Some(n)
}

fn has_extension(name: &str) -> bool {
    match name.rfind('.') {
        Some(i) => {
            let ext = &name[i + 1..];
            !ext.is_empty() && ext.bytes().all(|b| b.is_ascii_alphanumeric())
        }
        None => false,
    }
}

/// Splices every `input` line's file into the text where it stands (Therion reads one stream).
/// A `.th2` input is the drawing half and is left out.
fn expand_inputs(
    content: &str,
    path: &str,
    read: &dyn Fn(&str) -> Option<String>,
) -> (String, Vec<InputFinding>) {
    fn walk(
        text: &str,
        here: &str,
        stack: &mut Vec<String>,
        read: &dyn Fn(&str) -> Option<String>,
        findings: &mut Vec<InputFinding>,
    ) -> String {
        let mut out = Vec::new();
        for line in split_lines(text) {
            let Some(name) = input_name(&line) else {
                out.push(line);
                continue;
            };
            if name.to_lowercase().ends_with(".th2") {
                out.push(format!(
                    "# input {name} (a sketch page: offered separately)"
                ));
                continue;
            }
            let base = if is_absolute(&name) {
                name.clone()
            } else {
                format!("{}/{name}", dir_of(here))
            };
            let candidates = if has_extension(&name) {
                vec![base.clone()]
            } else {
                vec![base.clone(), format!("{base}.th")]
            };
            let mut got = None;
            let mut got_path = String::new();
            for c in candidates {
                got_path = c.clone();
                got = read(&c);
                if got.is_some() {
                    break;
                }
            }
            let Some(got) = got else {
                findings.push(InputFinding {
                    code: "therion-input-missing",
                    text: format!("This file pulls in \"{name}\" with \"input\", and it could not be read -- every passage in it is missing from this import."),
                });
                out.push(format!("# input {name} (not found)"));
                continue;
            };
            if stack.contains(&got_path) || stack.len() >= 20 {
                findings.push(InputFinding {
                    code: "therion-input-loop",
                    text: format!("\"{name}\" is pulled in by itself, directly or through other files, so it was read once and not again."),
                });
                out.push(format!("# input {name} (already read)"));
                continue;
            }
            out.push(format!("# ---- input {name} ----"));
            stack.push(got_path.clone());
            out.push(walk(&got, &got_path, stack, read, findings));
            stack.pop();
            out.push(format!("# ---- end input {name} ----"));
        }
        out.join("\n")
    }
    let mut findings = Vec::new();
    let mut stack = vec![path.to_string()];
    let text = walk(content, path, &mut stack, read, &mut findings);
    (text, findings)
}

#[derive(Clone, Copy, Default)]
struct Flags {
    splay: bool,
    duplicate: bool,
    surface: bool,
    approximate: bool,
}

#[derive(Clone)]
struct Scope {
    units: Units,
    declination: f64,
    flags: Flags,
    style: &'static str,
    normal: Vec<String>,
    dims: Vec<String>,
}

/// Reads a Therion .th file on its own (an `input` line is reported, not followed).
pub fn parse(content: &str) -> Survey {
    parse_inner(content, Vec::new())
}

/// Reads a Therion .th file at `path`, following its `input` lines through `read` (which answers a
/// file's text, or `None`).
pub fn parse_with_inputs(
    content: &str,
    path: &str,
    read: &dyn Fn(&str) -> Option<String>,
) -> Survey {
    let (text, findings) = expand_inputs(content, path, read);
    parse_inner(&text, findings)
}

fn parse_inner(content: &str, input_findings: Vec<InputFinding>) -> Survey {
    let mut survey = Survey {
        distance_unit: None,
        ..Survey::default()
    };
    for f in input_findings {
        survey.add_finding("warning", f.code, f.text);
    }
    let mut station_lrud: JsObj<Lrud> = JsObj::new();
    let mut equates: Vec<(Vec<String>, Vec<String>)> = Vec::new();
    let lines = logical_lines(content);
    let mut prefix: Vec<String> = Vec::new();
    let mut sc = Scope {
        units: Units::default(),
        declination: 0.0,
        flags: Flags::default(),
        style: "normal",
        normal: ["from", "to", "tape", "compass", "clino"]
            .map(String::from)
            .to_vec(),
        dims: ["station", "left", "right", "up", "down"]
            .map(String::from)
            .to_vec(),
    };
    let mut scopes: Vec<Scope> = Vec::new();
    let mut in_centreline = false;
    let mut skip_depth = 0usize;
    let mut team_dirty = false;
    let mut cur_date = String::new();
    let mut cur_team = String::new();
    let mut reported_input = false;
    let mut reported_style: Vec<String> = Vec::new();

    let drop_outermost = top_level_surveys(&lines) == 1;
    let name_in = |scope: &[String], name: &str| {
        let parts = if drop_outermost && !scope.is_empty() {
            &scope[1..]
        } else {
            scope
        };
        if parts.is_empty() {
            name.to_string()
        } else {
            format!("{}.{name}", parts.join("."))
        }
    };

    for (li, l) in lines.iter().enumerate() {
        let line = &l.text;
        if line.is_empty() {
            continue;
        }
        let tokens = tokenize(line);
        if tokens.is_empty() {
            continue;
        }
        let cmd = tokens[0].to_lowercase();

        if skip_depth > 0 {
            if cmd == "scrap" || cmd == "map" {
                skip_depth += 1;
            } else if cmd == "endscrap" || cmd == "endmap" {
                skip_depth -= 1;
            }
            continue;
        }
        match cmd.as_str() {
            "scrap" | "map" => {
                skip_depth = 1;
                continue;
            }
            "survey" => {
                prefix.push(
                    tokens
                        .get(1)
                        .cloned()
                        .unwrap_or_else(|| format!("anon{li}")),
                );
                scopes.push(sc.clone());
                if survey.name.is_empty() && tokens.len() > 1 {
                    survey.name = tokens[1].clone();
                }
                if survey.cave_name.is_empty() {
                    let mut title_at = None;
                    for (ti, t) in tokens.iter().enumerate().skip(1) {
                        if t.to_lowercase() == "-title" {
                            title_at = Some(ti + 1);
                        }
                    }
                    if let Some(t) = title_at.and_then(|i| tokens.get(i)) {
                        survey.cave_name = t.clone();
                    }
                }
                continue;
            }
            "endsurvey" => {
                prefix.pop();
                if let Some(s) = scopes.pop() {
                    sc = s;
                }
                continue;
            }
            "centreline" | "centerline" => {
                in_centreline = true;
                scopes.push(sc.clone());
                continue;
            }
            "endcentreline" | "endcenterline" => {
                in_centreline = false;
                if let Some(s) = scopes.pop() {
                    sc = s;
                }
                continue;
            }
            "equate" => {
                if tokens.len() > 2 {
                    equates.push((tokens[1..].to_vec(), prefix.clone()));
                }
                continue;
            }
            _ => {}
        }
        if !in_centreline {
            if cmd == "input" && !reported_input {
                reported_input = true;
                survey.add_finding(
                    "warning",
                    "therion-input",
                    "This file pulls in other files with \"input\", which this reader does not follow -- only the passages written in this file were imported.".into(),
                );
            }
            continue;
        }

        match cmd.as_str() {
            "date" if tokens.len() > 1 => {
                if team_dirty {
                    cur_team.clear();
                    survey.team.clear();
                    team_dirty = false;
                }
                let dtok = &tokens[1];
                cur_date = read_date(dtok, b".-/").unwrap_or_else(|| dtok.replace('.', "-"));
                survey.date = cur_date.clone();
                continue;
            }
            "team" | "explo-team" if tokens.len() > 1 => {
                if cmd == "explo-team" {
                    continue;
                }
                let member = if tokens[1].chars().any(crate::js::is_space) {
                    tokens[1].clone()
                } else {
                    tokens[1..]
                        .iter()
                        .take_while(|t| !TEAM_ROLES.contains(&t.to_lowercase().as_str()))
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                if !member.is_empty() {
                    cur_team = if cur_team.is_empty() {
                        member
                    } else {
                        format!("{cur_team}, {member}")
                    };
                    survey.team = cur_team.clone();
                }
                continue;
            }
            "units" if tokens.len() > 2 => {
                sc.units.apply(&tokens, &["ft", "foot"]);
                continue;
            }
            "declination" if tokens.len() > 1 => {
                let mut d = parse_float(tokens[1].strip_prefix('[').unwrap_or(&tokens[1]));
                if !d.is_nan() {
                    let unit = if tokens.len() > 2 {
                        let u = tokens[2].to_lowercase();
                        u.strip_suffix(']').map(str::to_string).unwrap_or(u)
                    } else {
                        "degrees".into()
                    };
                    if unit.contains("grad") || unit.contains("mil") {
                        d = grads_to_degrees(d);
                    }
                    sc.declination = d;
                    survey.declination = d;
                    survey.declination_source = "file".into();
                }
                continue;
            }
            "flags" => {
                let mut negate = false;
                for t in &tokens[1..] {
                    let f = t.to_lowercase();
                    if f == "not" {
                        negate = true;
                        continue;
                    }
                    match f.as_str() {
                        "splay" => sc.flags.splay = !negate,
                        "duplicate" => sc.flags.duplicate = !negate,
                        "surface" => sc.flags.surface = !negate,
                        "approximate" => sc.flags.approximate = !negate,
                        _ => {}
                    }
                    negate = false;
                }
                continue;
            }
            "data" if tokens.len() > 1 => {
                let style = tokens[1].to_lowercase();
                let fields = || {
                    tokens[2..]
                        .iter()
                        .map(|t| field_alias(&t.to_lowercase()).to_string())
                        .collect()
                };
                if style == "normal" {
                    sc.style = "normal";
                    sc.normal = fields();
                } else if style == "dimensions" || style == "topofil" {
                    sc.style = "dimensions";
                    sc.dims = fields();
                } else {
                    sc.style = "other";
                    if !reported_style.contains(&style) {
                        survey.add_finding(
                            "warning",
                            "therion-data-style",
                            format!("This file has a \"{style}\" data section, which this reader does not understand -- those shots were skipped."),
                        );
                        reported_style.push(style);
                    }
                }
                continue;
            }
            "fix" if tokens.len() >= 4 => {
                let fx = parse_float(&tokens[2]);
                let fy = parse_float(&tokens[3]);
                let fz = if tokens.len() >= 5 {
                    parse_float(&tokens[4])
                } else {
                    0.0
                };
                if !fx.is_nan() && !fy.is_nan() {
                    let x = into_survey_unit(&mut survey, &sc.units, fx);
                    let y = into_survey_unit(&mut survey, &sc.units, fy);
                    let z = if fz.is_nan() {
                        0.0
                    } else {
                        into_survey_unit(&mut survey, &sc.units, fz)
                    };
                    survey
                        .fixed
                        .set(&name_in(&prefix, &tokens[1]), Fixed::at(x, y, z));
                }
                continue;
            }
            "extend" | "station" | "mark" | "break" | "group" | "endgroup" | "cs" | "sd"
            | "grade" | "calibrate" | "infer" | "copyright" | "instrument" | "explo-date"
            | "declination-auto" => continue,
            _ => {}
        }

        if sc.style == "normal" {
            let mut rec: JsObj<String> = JsObj::new();
            for (k, v) in sc.normal.iter().zip(tokens.iter()) {
                rec.set(k, v.clone());
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
                            sc.units.clino(v)
                        } else if crate::survex::has_reading(rec.get("backclino")) {
                            f64::NAN // the backsight carries it
                        } else {
                            0.0
                        };
                        plumbed = clino.abs() >= 89.999;
                    }
                }
            }
            let mut compass = rec.get("compass").map_or(f64::NAN, |c| parse_float(c));
            if compass.is_nan() && crate::survex::has_reading(rec.get("backcompass")) {
                // only the backsight was read: it carries the bearing
            } else if compass.is_nan() {
                if !plumbed {
                    continue;
                }
                compass = 0.0;
            } else {
                compass = sc.units.compass(compass);
            }
            let anon = is_anonymous(&to);
            let distance = into_survey_unit(&mut survey, &sc.units, tape);
            let mut shot = Shot {
                splay: anon || sc.flags.splay,
                from: name_in(&prefix, &from),
                to: if anon {
                    String::new()
                } else {
                    name_in(&prefix, &to)
                },
                distance,
                azimuth: normalize_azimuth(compass + sc.declination),
                declination: Some(sc.declination),
                inclination: clino,
                exclude_from_length: sc.flags.duplicate || sc.flags.surface,
                exclude_from_plot: sc.flags.surface,
                ..Shot::default()
            };
            if let Some(bc) = rec.get("backcompass") {
                let v = parse_float(bc);
                if !v.is_nan() {
                    shot.back_azimuth =
                        Some(normalize_azimuth(sc.units.compass(v) + sc.declination));
                }
            }
            if let Some(bc) = rec.get("backclino") {
                match clino_keyword(bc) {
                    Some(k) => shot.back_inclination = Some(k),
                    None => {
                        let v = parse_float(bc);
                        if !v.is_nan() {
                            shot.back_inclination = Some(sc.units.clino(v));
                        }
                    }
                }
            }
            let mut leg = |k: &str| match rec.get(k) {
                None => None,
                Some(v) if v == "-" || v == "." => None,
                Some(v) => {
                    let n = parse_float(v);
                    if n.is_nan() {
                        None
                    } else {
                        Some(into_survey_unit(&mut survey, &sc.units, n))
                    }
                }
            };
            shot.left = leg("left");
            shot.right = leg("right");
            shot.up = leg("up");
            shot.down = leg("down");
            shot.notes = l.comment.clone();
            shot.trip = survey.trip_id_for(Trip {
                date: cur_date.clone(),
                team: cur_team.clone(),
                declination: sc.declination,
                ..Trip::default()
            });
            team_dirty = true;
            survey.shots.push(shot);
        } else if sc.style == "dimensions" {
            let mut rec: JsObj<String> = JsObj::new();
            for (k, v) in sc.dims.iter().zip(tokens.iter()) {
                rec.set(k, v.clone());
            }
            let Some(station) = rec.get("station").cloned() else {
                continue;
            };
            if !parse_float(&station).is_nan() && is_finite_str(&station) {
                continue;
            }
            let mut num = |k: &str| match rec.get(k) {
                None => None,
                Some(v) if v == "-" || v == "." => None,
                Some(v) => {
                    let n = parse_float(v);
                    if n.is_nan() {
                        None
                    } else {
                        Some(into_survey_unit(&mut survey, &sc.units, n))
                    }
                }
            };
            let lrud = Lrud::of(num("left"), num("right"), num("up"), num("down"));
            station_lrud.set(&name_in(&prefix, &station), lrud);
        }
    }

    if !equates.is_empty() {
        apply_equates(&mut survey, &mut station_lrud, &equates, &name_in);
    }

    for s in &mut survey.shots {
        if s.splay {
            continue;
        }
        if let Some(p) = station_lrud.get(&s.to) {
            s.left = s.left.or(p.left);
            s.right = s.right.or(p.right);
            s.up = s.up.or(p.up);
            s.down = s.down.or(p.down);
        }
    }
    if let Some(p) = survey.shots.first().and_then(|s| station_lrud.get(&s.from)) {
        let start = Lrud::of(p.left, p.right, p.up, p.down);
        survey.start_lrud = Some(start.clone());
        survey.trips[0].start_lrud = Some(start);
    }
    if survey.distance_unit.is_none() {
        survey.distance_unit = Some(sc.units.length_unit.into());
    }
    survey.ensure_trips();
    survey
}

/// "equate 5@b 12@a": those are one place. Every member of a class is renamed to one name -- the
/// first member any shot uses, else the first listed -- in the shots, the LRUD and the fixed points.
fn apply_equates(
    survey: &mut Survey,
    station_lrud: &mut JsObj<Lrud>,
    equates: &[(Vec<String>, Vec<String>)],
    name_in: &dyn Fn(&[String], &str) -> String,
) {
    let mut known: JsObj<()> = JsObj::new();
    for s in &survey.shots {
        known.set(&s.from, ());
        if !s.to.is_empty() {
            known.set(&s.to, ());
        }
    }
    let mut parent: JsObj<String> = JsObj::new();
    let mut order: Vec<String> = Vec::new();
    fn find(parent: &mut JsObj<String>, n: &str) -> String {
        let mut n = n.to_string();
        loop {
            let p = parent.get(&n).cloned().unwrap_or_else(|| n.clone());
            if p == n {
                return n;
            }
            let gp = parent.get(&p).cloned().unwrap_or_else(|| p.clone());
            parent.set(&n, gp.clone());
            n = gp;
        }
    }
    let resolve = |token: &str, scope: &[String]| -> String {
        let Some(at) = token.find('@') else {
            return name_in(scope, token);
        };
        let st = &token[..at];
        let path: Vec<String> = token[at + 1..].split('.').rev().map(String::from).collect();
        let mut full_scope = scope.to_vec();
        full_scope.extend(path.iter().cloned());
        let rel = name_in(&full_scope, st);
        if known.has(&rel) {
            return rel;
        }
        let abs = name_in(&path, st);
        if known.has(&abs) { abs } else { rel }
    };
    let mut unknown: Vec<String> = Vec::new();
    for (refs, scope) in equates {
        let mut first: Option<String> = None;
        for r in refs {
            if is_anonymous(r) {
                continue;
            }
            let nm = resolve(r, scope);
            if !known.has(&nm) && !survey.fixed.has(&nm) && !unknown.contains(&nm) {
                unknown.push(nm.clone());
            }
            if !parent.has(&nm) {
                parent.set(&nm, nm.clone());
                order.push(nm.clone());
            }
            match &first {
                None => first = Some(nm),
                Some(f) => {
                    let ra = find(&mut parent, f);
                    let rb = find(&mut parent, &nm);
                    if ra != rb {
                        parent.set(&rb, ra);
                    }
                }
            }
        }
    }
    let mut canon: JsObj<String> = JsObj::new();
    for o in &order {
        let root = find(&mut parent, o);
        let replace = match canon.get(&root) {
            None => true,
            Some(c) => !known.has(c) && known.has(o),
        };
        if replace {
            canon.set(&root, o.clone());
        }
    }
    let mut rename: JsObj<String> = JsObj::new();
    for o in &order {
        let root = find(&mut parent, o);
        if let Some(target) = canon.get(&root)
            && target != o
        {
            rename.set(o, target.clone());
        }
    }
    let re = |n: &str| rename.get(n).cloned().unwrap_or_else(|| n.to_string());
    for s in &mut survey.shots {
        s.from = re(&s.from);
        if !s.to.is_empty() {
            s.to = re(&s.to);
        }
    }
    let mut lrud_joined: JsObj<Lrud> = JsObj::new();
    for (n, l) in station_lrud.iter() {
        let t = re(n);
        if !lrud_joined.has(&t) || t == n {
            lrud_joined.set(&t, l.clone());
        }
    }
    *station_lrud = lrud_joined;
    let mut fixed_joined: JsObj<Fixed> = JsObj::new();
    for (n, f) in survey.fixed.iter() {
        let t = re(n);
        if !fixed_joined.has(&t) || t == n {
            fixed_joined.set(&t, f.clone());
        }
    }
    survey.fixed = fixed_joined;
    if !unknown.is_empty() {
        let n = unknown.len();
        let shown = unknown
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        survey.add_finding(
            "warning",
            "therion-equate-unknown",
            format!(
                "\"equate\" names {n} station{} no shot uses ({shown}{}) -- a name misspelt, or a survey this import did not include.",
                if n == 1 { "" } else { "s" },
                if n > 5 { ", ..." } else { "" }
            ),
        );
    }
}

/// Writes a survey as a Therion .th file: one survey block around one centreline, date and team
/// before each run of a trip's legs, declination where it changes, LRUD on the leg, splays as "-".
pub fn write(survey: &mut Survey) -> String {
    survey.ensure_trips();
    let mut out: Vec<String> = Vec::new();
    let base = if !survey.name.is_empty() {
        survey.name.clone()
    } else if !survey.cave_name.is_empty() {
        survey.cave_name.clone()
    } else {
        "cave".into()
    };
    let mut block = String::new();
    let mut in_bad = false;
    for c in base.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            block.push(c);
            in_bad = false;
        } else if !in_bad {
            block.push('-');
            in_bad = true;
        }
    }
    let block = block.trim_matches('-');
    let block = if block.is_empty() { "cave" } else { block };
    let title = if !survey.cave_name.is_empty() {
        survey.cave_name.as_str()
    } else if !survey.name.is_empty() {
        survey.name.as_str()
    } else {
        "Cave survey"
    };
    out.push("encoding utf-8".into());
    out.push(format!("survey {block} -title \"{title}\""));
    out.push(String::new());
    out.push("  centreline".into());
    if !survey.date.is_empty() {
        out.push(format!("    date {}", survey.date.replace('-', ".")));
    }
    out.push(format!(
        "    units length {}",
        if survey.distance_unit.as_deref() == Some("ft") {
            "feet"
        } else {
            "metres"
        }
    ));
    for (name, f) in survey.fixed.iter() {
        let fz = f.z();
        if fz.is_none() {
            out.push(format!("    # {}", unknown_elevation_note(name)));
        }
        out.push(format!(
            "    fix {name} {} {} {}",
            to_fixed(f.x.unwrap_or(f64::NAN), 2),
            to_fixed(f.y.unwrap_or(f64::NAN), 2),
            to_fixed(fz.unwrap_or(0.0), 2)
        ));
    }
    let any_back = survey.shots.iter().any(Shot::has_backsight);
    let any_lrud = survey.shots.iter().any(Shot::any_lrud);
    let mut fields = vec!["from", "to", "length", "compass", "clino"];
    if any_back {
        fields.extend(["backcompass", "backclino"]);
    }
    if any_lrud {
        fields.extend(["left", "right", "up", "down"]);
    }
    out.push(format!("    data normal {}", fields.join(" ")));
    let dash = |v: Option<f64>| v.map_or_else(|| "-".to_string(), |x| to_fixed(x, 2));

    let mut last_trip: Option<usize> = None;
    let mut emitted_decl: Option<f64> = None;
    let mut state = Flags::default();
    for s in &survey.shots {
        if s.exclude_from_all {
            out.push(format!(
                "    # excluded shot (no Therion equivalent): {} {} {} {} {}{}",
                s.from,
                if s.to.is_empty() { "-" } else { &s.to },
                to_fixed(s.distance, 2),
                to_fixed(s.azimuth, 2),
                to_fixed(s.inclination, 2),
                if s.notes.is_empty() {
                    String::new()
                } else {
                    format!("  {}", s.notes)
                }
            ));
            continue;
        }
        let trip = survey.trip_of(s);
        if last_trip != Some(s.trip) {
            last_trip = Some(s.trip);
            if let Some(t) = trip {
                if !t.date.is_empty() {
                    out.push(format!("    date {}", t.date.replace('-', ".")));
                }
                if !t.team.is_empty() {
                    for m in split_commas(&t.team) {
                        if !m.is_empty() {
                            out.push(format!("    team \"{m}\""));
                        }
                    }
                }
            }
        }
        let decl = applied_declination(s, trip);
        if emitted_decl.is_none_or(|e| (decl - e).abs() > 1e-9) {
            out.push(format!("    declination {} degrees", to_fixed(decl, 2)));
            emitted_decl = Some(decl);
        }
        let mag = normalize_azimuth(s.azimuth - decl);
        let want = Flags {
            splay: s.splay,
            surface: s.exclude_from_plot,
            duplicate: !s.exclude_from_plot && s.exclude_from_length,
            approximate: false,
        };
        for (name, w, cur) in [
            ("splay", want.splay, &mut state.splay),
            ("duplicate", want.duplicate, &mut state.duplicate),
            ("surface", want.surface, &mut state.surface),
        ] {
            if w != *cur {
                out.push(format!("    flags {}{name}", if w { "" } else { "not " }));
                *cur = w;
            }
        }
        let to_name = if s.splay || s.to.is_empty() {
            "-"
        } else {
            s.to.as_str()
        };
        let mut row = vec![
            format!("    {}", s.from),
            to_name.to_string(),
            to_fixed(s.distance, 2),
            if mag.is_finite() {
                to_fixed(mag, 2)
            } else {
                "-".into()
            },
            if s.inclination.is_finite() {
                to_fixed(s.inclination, 2)
            } else {
                "-".into()
            },
        ];
        if any_back {
            row.push(dash(s.back_azimuth.map(|b| normalize_azimuth(b - decl))));
            row.push(dash(s.back_inclination));
        }
        if any_lrud {
            row.extend([dash(s.left), dash(s.right), dash(s.up), dash(s.down)]);
        }
        out.push(format!(
            "{}{}",
            row.join(" "),
            if s.notes.is_empty() {
                String::new()
            } else {
                format!(" # {}", s.notes)
            }
        ));
    }
    out.push("  endcentreline".into());
    out.push(String::new());
    out.push("endsurvey".into());
    out.push(String::new());
    out.join("\n")
}

/// `s.split(/\s*,\s*/)`.
fn split_commas(s: &str) -> Vec<String> {
    let parts: Vec<&str> = s.split(',').collect();
    let last = parts.len() - 1;
    parts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let p = if i > 0 { trim_start(p) } else { p };
            let p = if i < last { trim_end(p) } else { p };
            p.to_string()
        })
        .collect()
}
