//! Walls .srv: reader and writer (a port of the old app's CsWalls.js).
//!
//! The common core. Supported: `#Units` (Meters/Feet, Order=, Decl=, LRUD=F/T/FB/TB, TypeAB=/TypeVB=
//! corrected or normal), `#Prefix` 1-3, `#Fix`, `#Date`, `#[ ... #]` exclusion blocks (shots kept,
//! excluded), inline `#S` with the P (don't plot) and L (length-exclude) letter conventions, inline LRUD
//! in `<...>` or `*...*`, "--" as not measured, station-only LRUD lines, FS/BS backsight pairs
//! ("100/282"), per-value unit suffixes (3m, 10f, 6i), quadrant bearings (N30E), `;` comments, and
//! splays ("-" as FROM or TO; FROM-side splays are reversed to hang from the named station).
//!
//! Walls distances default to METRES. The model keeps ONE unit: the one in force at the first
//! measured value wins and later `#Units` changes convert into it. Walls' LRUD default is the FROM
//! station; the writer declares LRUD=T so what it writes needs no shift.

use crate::js::{JsObj, parse_float, split_lines, split_ws, to_fixed, trim};
use crate::model::{
    Fixed, Lrud, Shot, Survey, convert, normalize_azimuth, parse_quadrant, unknown_elevation_note,
};

fn is_missing(t: &str) -> bool {
    t.len() >= 2 && t.bytes().all(|b| b == b'-')
}

fn all_dashes(t: &str) -> bool {
    !t.is_empty() && t.bytes().all(|b| b == b'-')
}

struct State {
    unit: &'static str,
    prefixes: [String; 3],
}

/// A reading in the survey's unit, honouring an m/f/i suffix.
fn value(survey: &mut Survey, st: &State, tok: &str) -> f64 {
    let mut t = tok;
    let mut v_unit = st.unit;
    let mut scale = 1.0;
    if let Some(last) = tok.chars().last() {
        let body = &tok[..tok.len() - last.len_utf8()];
        let numeric = !body.is_empty()
            && body
                .bytes()
                .all(|b| b.is_ascii_digit() || b == b'-' || b == b'.');
        if numeric && matches!(last, 'M' | 'm' | 'F' | 'f' | 'I' | 'i') {
            t = body;
            match last.to_ascii_lowercase() {
                'm' => v_unit = "m",
                'f' => v_unit = "ft",
                _ => {
                    v_unit = "ft";
                    scale = 1.0 / 12.0;
                }
            }
        }
    }
    let v = parse_float(t);
    if v.is_nan() {
        return f64::NAN;
    }
    if survey.distance_unit.is_none() {
        survey.distance_unit = Some(st.unit.into());
    }
    convert(v * scale, v_unit, survey.unit())
}

fn apply_prefix(st: &State, name: &str) -> String {
    if name.is_empty() || all_dashes(name) || name.contains(':') {
        return name.to_string();
    }
    let mut pfx = String::new();
    for p in st.prefixes.iter().rev() {
        if !p.is_empty() {
            pfx = if pfx.is_empty() {
                p.clone()
            } else {
                format!("{pfx}:{p}")
            };
        }
    }
    if pfx.is_empty() {
        name.to_string()
    } else {
        format!("{pfx}:{name}")
    }
}

/// "100/282": the foresight and the backsight; either may be empty or "--".
fn angle_pair(tok: &str) -> (Option<f64>, Option<f64>) {
    let parts: Vec<&str> = tok.split('/').collect();
    let one = |p: Option<&&str>| match p {
        None => None,
        Some(p) if p.is_empty() || is_missing(p) => None,
        Some(p) => parse_quadrant(p).or_else(|| Some(parse_float(p)).filter(|v| !v.is_nan())),
    };
    (
        one(parts.first()),
        if parts.len() > 1 {
            one(parts.get(1))
        } else {
            None
        },
    )
}

/// `#s(?:eg(?:ment)?)?\s+(\S+)\s*$` (case-insensitive): (where it starts, the word).
fn segment(line: &str) -> Option<(usize, String)> {
    let lower = line.to_ascii_lowercase();
    let t = crate::js::trim_end(line);
    let word_start = t
        .rfind(crate::js::is_space)
        .map(|i| i + t[i..].chars().next().map_or(1, char::len_utf8))?;
    let word = &t[word_start..];
    if word.is_empty() {
        return None;
    }
    let before = crate::js::trim_end(&t[..word_start]);
    if before.len() == word_start {
        return None; // no white space between the directive and the word
    }
    let lb = &lower[..before.len()];
    for d in ["#segment", "#seg", "#s"] {
        if lb.ends_with(d) {
            return Some((before.len() - d.len(), word.to_string()));
        }
    }
    None
}

/// `<([^>]*)>|\*([^*]*)\*`: (start, end, inner) of the first.
fn lrud_block(line: &str) -> Option<(usize, usize, String)> {
    let lt = line.find('<').and_then(|s| {
        line[s + 1..]
            .find('>')
            .map(|e| (s, s + 1 + e + 1, line[s + 1..s + 1 + e].to_string()))
    });
    let star = line.find('*').and_then(|s| {
        line[s + 1..]
            .find('*')
            .map(|e| (s, s + 1 + e + 1, line[s + 1..s + 1 + e].to_string()))
    });
    match (lt, star) {
        (Some(a), Some(b)) => Some(if a.0 <= b.0 { a } else { b }),
        (a, b) => a.or(b),
    }
}

/// Reads a Walls .srv file.
pub fn parse(content: &str) -> Survey {
    let mut survey = Survey {
        distance_unit: None,
        ..Survey::default()
    };
    let mut order: Vec<char> = vec!['D', 'A', 'V'];
    let mut st = State {
        unit: "m",
        prefixes: Default::default(),
    };
    let mut declination = 0.0;
    let mut from_station = true;
    let mut typeab_corrected = false;
    let mut typevb_corrected = false;
    let mut exclude_depth = 0usize;
    // readings for stations: (lrud, came from a splay line)
    let mut station_lrud: JsObj<(Lrud, bool)> = JsObj::new();
    let record = |sl: &mut JsObj<(Lrud, bool)>, station: &str, l: &Lrud, from_splay: bool| {
        if let Some((_, prev_splay)) = sl.get(station)
            && from_splay
            && !prev_splay
        {
            return; // a real shot's reading beats a splay line's
        }
        sl.set(
            station,
            (Lrud::of(l.left, l.right, l.up, l.down), from_splay),
        );
    };

    for raw in split_lines(content) {
        let mut line = trim(&raw).to_string();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('#') {
            if line.starts_with("#[") {
                exclude_depth += 1;
                continue;
            }
            if line.starts_with("#]") {
                exclude_depth = exclude_depth.saturating_sub(1);
                continue;
            }
            let dt = split_ws(&line);
            let directive = dt[0].to_lowercase();
            let after_eq = |t: &str| t.split('=').nth(1).unwrap_or("").to_string();
            if directive == "#units" {
                for tok in &dt[1..] {
                    let low = tok.to_lowercase();
                    if low == "feet" || low == "f" {
                        st.unit = "ft";
                    } else if low == "meters" || low == "metres" || low == "m" {
                        st.unit = "m";
                    } else if low.starts_with("d=") || low.starts_with("s=") {
                        st.unit = if after_eq(&low).starts_with('f') {
                            "ft"
                        } else {
                            "m"
                        };
                    } else if low.starts_with("order=") || low.starts_with("o=") {
                        order = after_eq(tok).to_uppercase().chars().collect();
                    } else if low.starts_with("decl=") || low.starts_with("declination=") {
                        let d = parse_float(&after_eq(tok));
                        declination = if d.is_nan() || d == 0.0 { 0.0 } else { d };
                        if survey.declination_source.is_empty() {
                            survey.declination = declination;
                            survey.declination_source = "file".into();
                        }
                    } else if low.starts_with("lrud=") {
                        let v = after_eq(tok);
                        let assoc = v.split(':').next().unwrap_or("").to_uppercase();
                        from_station = !assoc.starts_with('T');
                    } else if low.starts_with("typeab=") {
                        typeab_corrected = after_eq(&low).starts_with('c');
                    } else if low.starts_with("typevb=") {
                        typevb_corrected = after_eq(&low).starts_with('c');
                    } else if let Some(rest) = low.strip_prefix("prefix") {
                        let digit = rest.chars().next().filter(char::is_ascii_digit);
                        let after_digit = if digit.is_some() { &rest[1..] } else { rest };
                        if after_digit.starts_with('=') {
                            let idx = digit.map_or(0, |d| d.to_digit(10).unwrap_or(1) as i64 - 1);
                            if (0..3).contains(&idx) {
                                st.prefixes[idx as usize] = after_eq(tok);
                            }
                        }
                    }
                }
            } else if let Some(level) = prefix_level(&directive) {
                st.prefixes[level] = dt.get(1).cloned().unwrap_or_default();
            } else if directive == "#fix" {
                if dt.len() >= 4 {
                    let fx = value(&mut survey, &st, &dt[2]);
                    let fy = value(&mut survey, &st, &dt[3]);
                    let fz = if dt.len() >= 5 {
                        value(&mut survey, &st, &dt[4])
                    } else {
                        0.0
                    };
                    if !fx.is_nan() && !fy.is_nan() {
                        let name = apply_prefix(&st, &dt[1]);
                        survey
                            .fixed
                            .set(&name, Fixed::at(fx, fy, if fz.is_nan() { 0.0 } else { fz }));
                    }
                }
            } else if directive == "#date" && dt.len() > 1 {
                survey.date = dt[1].clone();
            }
            continue;
        }

        let (mut ex_plot, mut ex_length, mut ex_all) = (false, false, false);
        if let Some((at, word)) = segment(&line) {
            if (1..=3).contains(&word.len())
                && word
                    .chars()
                    .all(|c| matches!(c, 'P' | 'L' | 'X' | 'p' | 'l' | 'x'))
            {
                let u = word.to_uppercase();
                ex_plot = u.contains('P');
                ex_length = u.contains('L');
                ex_all = u.contains('X');
            }
            line.truncate(at);
        }

        let mut lrud = None;
        let mut work = line.clone();
        if let Some((s, e, inner)) = lrud_block(&line) {
            let parts: Vec<&str> = split_set(&inner);
            let mut vals = [None; 4];
            for (p, slot) in parts.iter().take(4).zip(vals.iter_mut()) {
                let pv = trim(p);
                if !(pv.is_empty() || is_missing(pv)) {
                    let v = value(&mut survey, &st, pv);
                    *slot = if v.is_nan() { None } else { Some(v) };
                }
            }
            lrud = Some(Lrud::of(vals[0], vals[1], vals[2], vals[3]));
            work = format!("{} {}", &line[..s], &line[e..]);
        }
        let mut notes = String::new();
        if let Some(i) = work.find(';') {
            notes = trim(&work[i + 1..]).to_string();
            work.truncate(i);
        }
        let fields = split_ws(trim(&work));
        if fields.is_empty() || fields[0].is_empty() {
            continue;
        }
        if fields.len() == 1 {
            if let Some(l) = &lrud {
                record(&mut station_lrud, &apply_prefix(&st, &fields[0]), l, false);
            }
            continue;
        }

        let from_dash = all_dashes(&fields[0]);
        let to_dash = all_dashes(&fields[1]);
        let mut shot = Shot {
            splay: from_dash || to_dash,
            notes,
            ..Shot::default()
        };
        let mut az_pair = None;
        let mut inc_pair = None;
        for (field, m) in order.iter().zip(fields[2..].iter()) {
            match field {
                'D' => {
                    let d = if is_missing(m) {
                        f64::NAN
                    } else {
                        value(&mut survey, &st, m)
                    };
                    if !d.is_nan() {
                        shot.distance = d;
                    }
                }
                'A' => az_pair = Some(angle_pair(m)),
                'V' => inc_pair = Some(angle_pair(m)),
                _ => {}
            }
        }
        let mut az = 0.0;
        let mut back_az = None;
        if let Some((front, back)) = az_pair {
            back_az = back.map(|b| {
                if typeab_corrected {
                    normalize_azimuth(b + 180.0)
                } else {
                    b
                }
            });
            if let Some(f) = front {
                az = f;
            } else if let Some(b) = back_az {
                az = normalize_azimuth(b + 180.0);
            }
        }
        let mut inc = 0.0;
        let mut back_inc = None;
        if let Some((front, back)) = inc_pair {
            back_inc = back.map(|b| if typevb_corrected { -b } else { b });
            if let Some(f) = front {
                inc = f;
            } else if let Some(b) = back_inc {
                inc = -b;
            }
        }
        if from_dash {
            shot.from = apply_prefix(&st, &fields[1]);
            shot.to = String::new();
            az = normalize_azimuth(az + 180.0);
            inc = -inc;
            back_az = back_az.map(|b| normalize_azimuth(b + 180.0));
            back_inc = back_inc.map(|b| -b);
        } else {
            shot.from = apply_prefix(&st, &fields[0]);
            shot.to = if to_dash {
                String::new()
            } else {
                apply_prefix(&st, &fields[1])
            };
        }
        shot.azimuth = normalize_azimuth(az + declination);
        shot.back_azimuth = back_az.map(|b| normalize_azimuth(b + declination));
        shot.declination = Some(declination);
        shot.inclination = inc;
        shot.back_inclination = back_inc;
        shot.exclude_from_plot = ex_plot;
        shot.exclude_from_length = ex_length;
        shot.exclude_from_all = ex_all || exclude_depth > 0;
        if let Some(l) = &lrud {
            if !from_station && !shot.splay {
                (shot.left, shot.right, shot.up, shot.down) = (l.left, l.right, l.up, l.down);
            } else {
                record(&mut station_lrud, &shot.from, l, shot.splay);
            }
        }
        survey.shots.push(shot);
    }

    for s in &mut survey.shots {
        if !s.splay
            && !s.any_lrud()
            && let Some((l, _)) = station_lrud.get(&s.to)
        {
            (s.left, s.right, s.up, s.down) = (l.left, l.right, l.up, l.down);
        }
    }
    if let Some((l, _)) = survey.shots.first().and_then(|s| station_lrud.get(&s.from)) {
        survey.start_lrud = Some(Lrud::of(l.left, l.right, l.up, l.down));
    }
    if survey.distance_unit.is_none() {
        survey.distance_unit = Some(st.unit.into());
    }
    survey.ensure_trips();
    survey
}

/// `#prefix`, `#prefix1`..`#prefix3` (lower-cased): the level, 0-based.
fn prefix_level(d: &str) -> Option<usize> {
    match d {
        "#prefix" | "#prefix1" => Some(0),
        "#prefix2" => Some(1),
        "#prefix3" => Some(2),
        _ => None,
    }
}

/// `s.split(/[\s,]+/)`.
fn split_set(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut in_sep = false;
    for (i, c) in s.char_indices() {
        let sep = c == ',' || crate::js::is_space(c);
        if sep && !in_sep {
            out.push(&s[start..i]);
            in_sep = true;
        }
        if !sep && in_sep {
            start = i;
            in_sep = false;
        }
    }
    out.push(if in_sep { "" } else { &s[start..] });
    out
}

/// Writes a survey as a Walls .srv file (LRUD=T, one file-wide declination: trip 0's).
pub fn write(survey: &mut Survey) -> String {
    survey.ensure_trips();
    let mut out: Vec<String> = Vec::new();
    let decl = if survey.declination.is_nan() {
        0.0
    } else {
        survey.declination
    };
    let unit_word = if survey.distance_unit.as_deref() == Some("ft") {
        "Feet"
    } else {
        "Meters"
    };
    let title = if !survey.cave_name.is_empty() {
        survey.cave_name.as_str()
    } else if !survey.name.is_empty() {
        survey.name.as_str()
    } else {
        "Cave survey"
    };
    out.push(format!("; {title}"));
    if !survey.date.is_empty() {
        out.push(format!("#Date {}", survey.date));
    }
    out.push(format!(
        "#Units {unit_word} Order=DAV LRUD=T{}",
        if decl != 0.0 {
            format!(" Decl={}", to_fixed(decl, 2))
        } else {
            String::new()
        }
    ));
    for (name, f) in survey.fixed.iter() {
        let fz = f.z();
        if fz.is_none() {
            out.push(format!("; {}", unknown_elevation_note(name)));
        }
        out.push(format!(
            "#Fix {name} {} {} {}",
            to_fixed(f.x.unwrap_or(f64::NAN), 2),
            to_fixed(f.y.unwrap_or(f64::NAN), 2),
            to_fixed(fz.unwrap_or(0.0), 2)
        ));
    }
    let fmt = |v: Option<f64>| v.map_or_else(|| "--".to_string(), |x| to_fixed(x, 2));
    let block = |l: Option<f64>, r: Option<f64>, u: Option<f64>, d: Option<f64>| {
        format!("<{},{},{},{}>", fmt(l), fmt(r), fmt(u), fmt(d))
    };
    if let (Some(sl), Some(first)) = (&survey.start_lrud, survey.shots.first())
        && sl.any()
    {
        out.push(format!(
            "{}\t{}",
            first.from,
            block(sl.left, sl.right, sl.up, sl.down)
        ));
    }
    let mut in_exclude = false;
    for s in &survey.shots {
        if s.exclude_from_all && !in_exclude {
            out.push("#[ excluded shots".into());
            in_exclude = true;
        } else if !s.exclude_from_all && in_exclude {
            out.push("#]".into());
            in_exclude = false;
        }
        let mut az = to_fixed(normalize_azimuth(s.azimuth - decl), 2);
        if let Some(b) = s.back_azimuth {
            az += &format!("/{}", to_fixed(normalize_azimuth(b - decl), 2));
        }
        let mut inc = to_fixed(s.inclination, 2);
        if let Some(b) = s.back_inclination {
            inc += &format!("/{}", to_fixed(b, 2));
        }
        let mut line = format!(
            "{}\t{}\t{}\t{az}\t{inc}",
            s.from,
            if s.splay { "-" } else { &s.to },
            to_fixed(s.distance, 2)
        );
        if s.any_lrud() {
            line += &format!("\t{}", block(s.left, s.right, s.up, s.down));
        }
        let seg = format!(
            "{}{}",
            if s.exclude_from_plot { "P" } else { "" },
            if s.exclude_from_length { "L" } else { "" }
        );
        if !seg.is_empty() {
            line += &format!("\t#S {seg}");
        }
        if !s.notes.is_empty() {
            line += &format!("\t; {}", s.notes);
        }
        out.push(line);
    }
    if in_exclude {
        out.push("#]".into());
    }
    out.join("\n") + "\n"
}
