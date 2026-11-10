//! Compass .dat: reader and writer (a port of the old app's CsCompass.js).
//!
//! The common everyday core of the format, not all of it. Supported: several surveys per file
//! (form-feed separated), DECLINATION (added to every azimuth, east positive), the fixed column order
//! FROM TO LENGTH BEARING INC LEFT UP DOWN RIGHT [AZM2 INC2] [flags] [comment] (the FORMAT header's
//! unit and order letters only document the notebook, but its B -- backsight columns -- and its last
//! letter -- F/T, which station the LRUD belongs to -- are real), negative LRUD as "not measured",
//! -999 as a missing backsight, the #|PXLCS# flags, and the trailing comment. Not supported: .MAK
//! projects and instrument corrections (read past; virtually always 0).
//!
//! LRUD: Compass' de-facto default is the FROM station, so a line's reading is attached to the shot
//! ARRIVING at its FROM station and the first station's lands in `start_lrud`. A zero-length shot to
//! a station nothing else uses is the documented way to carry a leaf station's LRUD; it is folded into
//! the station rather than becoming a shot. Compass stores decimal feet, always.

use crate::js::{JsObj, parse_float, split_lines, split_ws, to_fixed, trim};
use crate::model::{Lrud, Shot, Survey, Trip, applied_declination, convert, normalize_azimuth};

/// A reading Compass writes as "not taken": -999 (and 999, from Karst). Anything beyond 639 is
/// outside every real compass or clino reading even after a correction, which is Survex's test.
fn is_missing_reading(v: f64) -> bool {
    v.is_nan() || v.abs() > 999.0 - 360.0
}

/// `CORRECTIONS: compass clino tape` (or `CORRECTIONS2: backcompass backclino`): the numbers, which
/// are ADDED to every reading of that kind (instrument zero errors, already worked out).
fn corrections(block: &str, key: &str, count: usize) -> Option<Vec<f64>> {
    first_match(block, key, |rest| {
        let words: Vec<f64> = rest
            .split_whitespace()
            .take(count)
            .map(parse_float)
            .take_while(|v| !v.is_nan())
            .collect();
        (words.len() == count).then_some(words)
    })
}

/// The text after the first case-insensitive `key` in `block`, if it is there.
fn after_ci<'a>(block: &'a str, key: &str) -> Option<&'a str> {
    let lower = block.to_ascii_lowercase();
    lower
        .find(&key.to_ascii_lowercase())
        .map(|i| &block[i + key.len()..])
}

/// Like a case-insensitive regular expression `key<pattern>`: the first place `key` is followed by
/// something `read` accepts.
fn first_match<T>(block: &str, key: &str, read: impl Fn(&str) -> Option<T>) -> Option<T> {
    let lower = block.to_ascii_lowercase();
    let k = key.to_ascii_lowercase();
    let mut from = 0;
    while let Some(off) = lower[from..].find(&k) {
        let at = from + off;
        if let Some(v) = read(&block[at + k.len()..]) {
            return Some(v);
        }
        from = at + 1;
        while !lower.is_char_boundary(from) {
            from += 1;
        }
    }
    None
}

/// `-?[0-9.]+` at the front (after `\s*`).
fn decl_number(rest: &str) -> Option<f64> {
    let t = rest.trim_start_matches(crate::js::is_space);
    let b = t.as_bytes();
    let mut i = usize::from(b.first() == Some(&b'-'));
    let start = i;
    while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
        i += 1;
    }
    (i > start).then(|| parse_float(&t[..i]))
}

/// A line's FORMAT letters (`FORMAT:\s*([A-Za-z]+)`, case-sensitive like the old reader).
fn format_letters(block: &str) -> String {
    let Some(i) = block.find("FORMAT:") else {
        return String::new();
    };
    let t = block[i + 7..].trim_start_matches(crate::js::is_space);
    t.chars().take_while(|c| c.is_ascii_alphabetic()).collect()
}

/// SURVEY DATE: month day year (`\s*(\d+)\s+(\d+)\s+(\d+)`), as YYYY-MM-DD.
fn block_date(block: &str) -> String {
    let three = |rest: &str| {
        let mut nums = Vec::new();
        let mut t = rest;
        for k in 0..3 {
            let skipped = t.trim_start_matches(crate::js::is_space);
            if k > 0 && skipped.len() == t.len() {
                return None;
            }
            let n = skipped.bytes().take_while(u8::is_ascii_digit).count();
            if n == 0 {
                return None;
            }
            nums.push(crate::js::parse_float(&skipped[..n]) as i64);
            t = &skipped[n..];
        }
        Some((nums[0], nums[1], nums[2]))
    };
    let Some((mo, da, mut yr)) = first_match(block, "SURVEY DATE:", three) else {
        return String::new();
    };
    // A two-digit year is always 19XX (Larry Fish, Compass's author, via Survex), and 1 1 1901 is
    // what Compass writes when no date was given. (The old reader guessed 20XX for 00-50.)
    if yr < 100 {
        yr += 1900;
    }
    if (yr, mo, da) == (1901, 1, 1) {
        return String::new();
    }
    // a month or day that cannot be is no date at all (a declination worked out from it would be
    // for a day that never was)
    if !(1..=12).contains(&mo) || !(1..=31).contains(&da) {
        return String::new();
    }
    format!("{yr}-{mo:02}-{da:02}")
}

/// The file's surveys: each ends at a form feed, and a survey header that follows another survey's
/// shots with no form feed (a hand-joined file) starts a new one at its cave-name line.
fn blocks(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    for block in content.split('\u{0C}') {
        let lines = split_lines(block);
        let mut start = 0;
        let mut seen_shots = false;
        for (i, l) in lines.iter().enumerate() {
            let lower = l.to_ascii_lowercase();
            if lower.contains("survey name:") && seen_shots && i > start + 1 {
                out.push(lines[start..i - 1].join("\n"));
                start = i - 1;
                seen_shots = false;
            } else if lower.trim_start().starts_with("from") && lower.contains("to") {
                seen_shots = true;
            }
        }
        out.push(lines[start..].join("\n"));
    }
    out
}

/// The rest of the line after SURVEY NAME:, trimmed.
fn block_name(block: &str) -> String {
    match after_ci(block, "SURVEY NAME:") {
        Some(rest) => {
            let line = rest.split(['\n', '\r']).next().unwrap_or("");
            trim(line).to_string()
        }
        None => String::new(),
    }
}

struct Raw {
    from: String,
    to: String,
    distance: f64,
    azimuth: f64,
    inclination: f64,
    back_azimuth: Option<f64>,
    back_inclination: Option<f64>,
    lrud: Lrud,
    flags: String,
    notes: String,
    to_station: bool,
    trip: usize,
    declination: f64,
    azimuth_omitted: bool,
}

/// `#\|([A-Za-z]*)#\s*` in `s`: (start, end of match, letters).
fn find_flags(s: &str) -> Option<(usize, usize, String)> {
    let b = s.as_bytes();
    let mut from = 0;
    while let Some(off) = s[from..].find("#|") {
        let start = from + off;
        let mut i = start + 2;
        while i < b.len() && b[i].is_ascii_alphabetic() {
            i += 1;
        }
        if i < b.len() && b[i] == b'#' {
            let letters = s[start + 2..i].to_string();
            let mut end = i + 1;
            while end < s.len() {
                let c = s[end..].chars().next().unwrap_or(' ');
                if !crate::js::is_space(c) {
                    break;
                }
                end += c.len_utf8();
            }
            return Some((start, end, letters));
        }
        from = start + 1;
    }
    None
}

/// Reads a Compass .dat file.
pub fn parse(content: &str) -> Survey {
    parse_keeping(content, &[], None)
}

/// Reads a Compass .dat file in a project: the stations in `keep` (those the project fixes or links)
/// are real stations, so a zero-length shot to one joins it rather than only carrying LRUD.
/// `lrud_to` overrides every block's FORMAT on which station the LRUD belong to (a project's `!O`).
pub fn parse_keeping(content: &str, keep: &[String], lrud_to: Option<bool>) -> Survey {
    let mut survey = Survey {
        distance_unit: Some("ft".into()),
        ..Survey::default()
    };
    let mut first_block = true;
    // shared across every block: a station can be shared at a block boundary
    let mut raws: Vec<Raw> = Vec::new();
    let mut station_refs: JsObj<usize> = JsObj::new();
    let mut station_lrud: JsObj<Lrud> = JsObj::new();

    for block in &blocks(content) {
        let block = block.as_str();
        // DOS editors end a file with ^Z, which is not a survey
        if block
            .chars()
            .all(|c| crate::js::is_space(c) || c == '\u{1A}')
        {
            continue;
        }
        let decl_found = first_match(block, "DECLINATION:", decl_number);
        let declination = decl_found.unwrap_or(0.0);
        let letters = format_letters(block);
        let last = letters.chars().last().map(|c| c.to_ascii_uppercase());
        let to_station = lrud_to.unwrap_or(last == Some('T'));
        // The backsight letter is the 12th of a 12- or 13-letter FORMAT and the 14th of a 15-letter
        // one; B means backsight columns, and so does C (the same, shown reversed in Compass).
        let chars: Vec<char> = letters.chars().collect();
        let back_at = if chars.len() >= 15 { 13 } else { 11 };
        let has_backsights = letters.chars().skip(8).any(|c| c == 'B')
            || (chars.len() >= 12 && matches!(chars[back_at], 'B' | 'C'));
        let fore = corrections(block, "CORRECTIONS:", 3).unwrap_or(vec![0.0; 3]);
        let back = corrections(block, "CORRECTIONS2:", 2).unwrap_or(vec![fore[0], fore[1]]);
        let lines = split_lines(block);

        let name = block_name(block);
        let date = block_date(block);
        let team = lines
            .iter()
            .position(|l| l.to_ascii_lowercase().contains("survey team:"))
            .and_then(|i| lines.get(i + 1))
            .map(|l| trim(l).to_string())
            .unwrap_or_default();

        if first_block {
            first_block = false;
            if let Some(l0) = lines.first() {
                survey.cave_name = trim(l0).to_string();
            }
            survey.name = name.clone();
            survey.date = date.clone();
            survey.team = team.clone();
            survey.declination = declination;
            survey.declination_source = if decl_found.is_some() {
                "file".into()
            } else {
                String::new()
            };
        }
        let trip = survey.trip_id_for(Trip {
            name,
            date,
            team,
            declination,
            declination_source: if decl_found.is_some() {
                "file".into()
            } else {
                String::new()
            },
            ..Trip::default()
        });

        // header lines can look numeric, so only lines after DECLINATION are shots
        let scan_start = lines
            .iter()
            .position(|l| l.to_ascii_lowercase().contains("declination:"))
            .map_or(0, |i| i + 1);
        for raw_line in &lines[scan_start..] {
            let line = trim(raw_line);
            if line.is_empty() {
                continue;
            }
            let tokens = split_ws(line);
            if tokens.len() < 9 {
                continue;
            }
            let length = parse_float(&tokens[2]);
            let bearing = parse_float(&tokens[3]);
            let inc = parse_float(&tokens[4]);
            if length.is_nan() || bearing.is_nan() || inc.is_nan() {
                continue;
            }
            let lrud_num = |t: &str| {
                let v = parse_float(t);
                if v.is_nan() || v < 0.0 { None } else { Some(v) }
            };
            let mut next = 9;
            let (mut back_az, mut back_inc) = (None, None);
            let t9 = if tokens.len() > 10 {
                parse_float(&tokens[9])
            } else {
                f64::NAN
            };
            let t10 = if tokens.len() > 10 {
                parse_float(&tokens[10])
            } else {
                f64::NAN
            };
            let have_back = has_backsights || (t9.is_finite() && t10.is_finite());
            if have_back && tokens.len() > 10 && !t9.is_nan() && !t10.is_nan() {
                back_az = (!is_missing_reading(t9)).then_some(t9 + back[0]);
                back_inc = (!is_missing_reading(t10)).then_some(t10 + back[1]);
                next = 11;
            }
            // A foresight of -999 was not taken: the backsight carries the shot. With neither,
            // a bearing is missing (the leg cannot be placed) and an inclination is level.
            let bearing = if is_missing_reading(bearing) {
                f64::NAN
            } else {
                bearing + fore[0]
            };
            let inc = if !is_missing_reading(inc) {
                inc + fore[1]
            } else if back_inc.is_some() {
                f64::NAN
            } else {
                0.0
            };
            let length = length + fore[2];
            // no bearing either way: fine on a vertical or zero-length shot (Compass writes -999
            // there), else the shot cannot be placed
            let vertical = inc.abs() >= 89.999 || back_inc.is_some_and(|b| b.abs() >= 89.999);
            let omitted = bearing.is_nan() && back_az.is_none() && (vertical || length == 0.0);
            let bearing = if omitted { 0.0 } else { bearing };
            if bearing.is_nan() && back_az.is_none() {
                survey.add_finding(
                    "warning",
                    "compass-no-bearing",
                    format!(
                        "Shot {} to {} has no compass reading either way (-999 and no backsight), so it cannot be placed -- anything beyond it is unconnected.",
                        tokens[0], tokens[1]
                    ),
                );
            }
            let remainder = tokens[next.min(tokens.len())..].join(" ");
            let (flags, comment) = match find_flags(&remainder) {
                Some((_, end, letters)) => (letters.to_uppercase(), remainder[end..].to_string()),
                None => (String::new(), remainder.clone()),
            };
            let raw = Raw {
                from: tokens[0].clone(),
                to: tokens[1].clone(),
                distance: length,
                azimuth: normalize_azimuth(bearing + declination),
                inclination: inc,
                back_azimuth: back_az.map(|b| normalize_azimuth(b + declination)),
                back_inclination: back_inc,
                lrud: Lrud {
                    left: lrud_num(&tokens[5]),
                    up: lrud_num(&tokens[6]),
                    down: lrud_num(&tokens[7]),
                    right: lrud_num(&tokens[8]),
                    ..Lrud::default()
                },
                flags,
                notes: trim(&comment).to_string(),
                to_station,
                trip,
                declination,
                azimuth_omitted: omitted,
            };
            for st in [&raw.from, &raw.to] {
                let n = station_refs.get(st).copied().unwrap_or(0);
                station_refs.set(st, n + 1);
            }
            raws.push(raw);
        }
    }

    for r in &raws {
        let has_lrud = r.lrud.any();
        let is_carrier = r.distance == 0.0
            && has_lrud
            && station_refs.get(&r.to) == Some(&1)
            && !r.flags.contains('S')
            && !keep.contains(&r.to);
        if !r.to_station && has_lrud {
            station_lrud.set(&r.from, r.lrud.clone()); // the last reading wins
        }
        if is_carrier {
            continue;
        }
        let splay = r.flags.contains('S');
        let mut shot = Shot {
            from: r.from.clone(),
            splay,
            to: if splay { String::new() } else { r.to.clone() },
            distance: r.distance,
            azimuth: r.azimuth,
            azimuth_omitted: r.azimuth_omitted,
            inclination: r.inclination,
            back_azimuth: r.back_azimuth,
            back_inclination: r.back_inclination,
            exclude_from_all: r.flags.contains('X'),
            exclude_from_plot: r.flags.contains('P'),
            exclude_from_length: r.flags.contains('L'),
            no_adjust: r.flags.contains('C'),
            notes: r.notes.clone(),
            trip: r.trip,
            declination: Some(r.declination),
            ..Shot::default()
        };
        if r.to_station && !splay {
            shot.left = r.lrud.left;
            shot.up = r.lrud.up;
            shot.down = r.lrud.down;
            shot.right = r.lrud.right;
        }
        survey.shots.push(shot);
    }

    for s in &mut survey.shots {
        if !s.splay
            && !s.any_lrud()
            && let Some(l) = station_lrud.get(&s.to)
        {
            s.left = l.left;
            s.up = l.up;
            s.down = l.down;
            s.right = l.right;
        }
    }
    if let Some(l) = survey.shots.first().and_then(|s| station_lrud.get(&s.from)) {
        let start = Lrud::of(l.left, l.right, l.up, l.down);
        survey.start_lrud = Some(start.clone());
        survey.trips[0].start_lrud = Some(start);
    }
    survey.ensure_trips();
    survey
}

fn pad_left(s: String, width: usize) -> String {
    let n = s.chars().count();
    if n >= width {
        s
    } else {
        format!("{}{s}", " ".repeat(width - n))
    }
}

fn pad_right(s: &str, width: usize) -> String {
    let n = s.chars().count();
    if n >= width {
        s.to_string()
    } else {
        format!("{s}{}", " ".repeat(width - n))
    }
}

fn num(v: f64, width: usize) -> String {
    pad_left(to_fixed(v, 2), width)
}

/// Writes a survey as a Compass .dat file: one block per trip (and per declination within a trip,
/// since a block declares one), distances in feet, azimuths back to MAGNETIC, LRUD in Compass' own
/// FROM-station association, a leaf station's reading on a zero-length carrier shot.
pub fn write(survey: &mut Survey) -> String {
    survey.ensure_trips();
    // a .dat has nowhere to put a coordinate: Compass keeps fixed stations in the project (.mak)
    let fixed: Vec<String> = survey.fixed.iter().map(|(n, _)| n.to_string()).collect();
    if !fixed.is_empty() {
        let shown: Vec<&str> = fixed.iter().take(4).map(String::as_str).collect();
        survey.add_finding(
            "warning",
            "export-fixes-dropped",
            format!(
                "A Compass .dat file cannot hold fixed coordinates, so {} fixed station{} ({}{}) {} not in it: the survey will sit at Compass's own origin{} until {} set in a Compass project (.mak).",
                fixed.len(),
                if fixed.len() == 1 { "" } else { "s" },
                shown.join(", "),
                if fixed.len() > 4 { ", ..." } else { "" },
                if fixed.len() == 1 { "is" } else { "are" },
                if fixed.len() > 1 { ", and separately fixed parts will not keep their places" } else { "" },
                if fixed.len() == 1 { "it is" } else { "they are" },
            ),
        );
    }
    let unit = survey.unit().to_string();
    let to_ft = |v: f64| convert(v, &unit, "ft");
    let any_back = survey.shots.iter().any(Shot::has_backsight);

    let mut station_lrud: JsObj<Lrud> = JsObj::new();
    if let (Some(first), Some(start)) = (survey.shots.first(), &survey.start_lrud) {
        station_lrud.set(&first.from, start.clone());
    }
    for s in &survey.shots {
        if !s.splay && !s.to.is_empty() && s.any_lrud() {
            station_lrud.set(&s.to, Lrud::of(s.left, s.right, s.up, s.down));
        }
    }
    let mut emitted: JsObj<()> = JsObj::new();
    let lr = |v: Option<f64>| v.map_or(-9.90, to_ft);
    let line = |from: &str,
                to: &str,
                dist: f64,
                az: f64,
                inc: f64,
                l: &Lrud,
                back_az: Option<f64>,
                back_inc: Option<f64>,
                flags: &str,
                notes: &str| {
        let mut out = format!(
            "{} {}{} {} {} {} {} {} {}",
            pad_right(from, 12),
            pad_right(to, 12),
            num(dist, 7),
            num(az, 8),
            num(inc, 8),
            num(lr(l.left), 8),
            num(lr(l.up), 8),
            num(lr(l.down), 8),
            num(lr(l.right), 8)
        );
        if any_back {
            out += &format!(
                " {} {}",
                num(back_az.unwrap_or(-999.0), 8),
                num(back_inc.unwrap_or(-999.0), 8)
            );
        }
        if !flags.is_empty() {
            out += &format!(" #|{flags}#");
        }
        if !notes.is_empty() {
            out += &format!(" {notes}");
        }
        out
    };

    // one block per (trip, applied declination), in trip order
    struct Block {
        trip: usize,
        decl: f64,
        shots: Vec<usize>,
    }
    let mut blocks: Vec<Block> = Vec::new();
    for t in 0..survey.trips.len() {
        let mut groups: Vec<Block> = Vec::new();
        for (i, s) in survey.shots.iter().enumerate() {
            if s.trip != t {
                continue;
            }
            let d = applied_declination(s, survey.trips.get(t));
            match groups.iter_mut().find(|g| (g.decl - d).abs() < 5e-5) {
                Some(g) => g.shots.push(i),
                None => groups.push(Block {
                    trip: t,
                    decl: d,
                    shots: vec![i],
                }),
            }
        }
        blocks.extend(groups);
    }
    if blocks.is_empty() {
        blocks.push(Block {
            trip: 0,
            decl: Some(survey.trips[0].declination)
                .filter(|d| *d != 0.0 && !d.is_nan())
                .unwrap_or(0.0),
            shots: Vec::new(),
        });
    }

    let mut out: Vec<String> = Vec::new();
    let mut splay_seq = 0;
    let cave = if !survey.cave_name.is_empty() {
        survey.cave_name.clone()
    } else if !survey.name.is_empty() {
        survey.name.clone()
    } else {
        "CAVE".into()
    };
    let n_blocks = blocks.len();
    for (bi, block) in blocks.iter().enumerate() {
        let trip = &survey.trips[block.trip];
        let decl = block.decl;
        let date_line = iso_date(&trip.date).map_or_else(
            || "1 1 1900".to_string(),
            |(y, m, d)| format!("{m} {d} {y}"),
        );
        out.push(cave.clone());
        out.push(format!(
            "SURVEY NAME: {}",
            if trip.name.is_empty() {
                "1"
            } else {
                &trip.name
            }
        ));
        out.push(format!("SURVEY DATE: {date_line}  COMMENT:"));
        out.push("SURVEY TEAM:".into());
        out.push(trip.team.clone());
        out.push(format!(
            "DECLINATION: {}  FORMAT: DDDDLRUD{}F  CORRECTIONS: 0.00 0.00 0.00",
            to_fixed(decl, 2),
            if any_back { "LADadB" } else { "LADN" }
        ));
        out.push(String::new());
        out.push(format!(
            "FROM         TO           LENGTH  BEARING      INC     LEFT       UP     DOWN    RIGHT{}",
            if any_back { "     AZM2     INC2" } else { "" }
        ));
        out.push(String::new());

        for &i in &block.shots {
            let s = &survey.shots[i];
            // a reading not taken goes out as Compass writes it: -999
            let magnetic = if s.azimuth.is_finite() {
                normalize_azimuth(s.azimuth - decl)
            } else {
                -999.0
            };
            let inclination = if s.inclination.is_finite() {
                s.inclination
            } else {
                -999.0
            };
            let back_az = s.back_azimuth.map(|b| normalize_azimuth(b - decl));
            let mut flags = String::new();
            for (on, c) in [
                (s.exclude_from_all, 'X'),
                (s.splay, 'S'),
                (s.exclude_from_plot, 'P'),
                (s.exclude_from_length, 'L'),
                (s.no_adjust, 'C'),
            ] {
                if on {
                    flags.push(c);
                }
            }
            flags.truncate(3);
            let none = Lrud::default();
            let from_lrud = match station_lrud.get(&s.from) {
                Some(l) => {
                    emitted.set(&s.from, ());
                    l
                }
                None => &none,
            };
            let to_name = if s.splay && s.to.is_empty() {
                splay_seq += 1;
                format!("{}.s{splay_seq}", s.from)
            } else {
                s.to.clone()
            };
            out.push(line(
                &s.from,
                &to_name,
                to_ft(s.distance),
                magnetic,
                inclination,
                from_lrud,
                back_az,
                s.back_inclination,
                &flags,
                &s.notes,
            ));
        }

        if bi == n_blocks - 1 {
            for (st, l) in station_lrud.iter() {
                if !emitted.has(st) {
                    out.push(line(
                        st,
                        &format!("{st}_L"),
                        0.0,
                        0.0,
                        0.0,
                        l,
                        None,
                        None,
                        "",
                        "LRUD carrier",
                    ));
                }
            }
        }
        out.push("\u{0C}".into());
    }
    out.join("\r\n")
}

/// "YYYY-MM-DD" as (year text, month, day).
fn iso_date(d: &str) -> Option<(String, u32, u32)> {
    let b = d.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let digits = |r: std::ops::Range<usize>| d[r].bytes().all(|c| c.is_ascii_digit());
    if !digits(0..4) || !digits(5..7) || !digits(8..10) {
        return None;
    }
    Some((
        d[0..4].to_string(),
        d[5..7].parse().ok()?,
        d[8..10].parse().ok()?,
    ))
}
