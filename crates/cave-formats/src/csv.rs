//! Plain CSV, for spreadsheet people (a port of the old app's CsCsv.js).
//!
//! Columns are mapped by the header row when there is one (the default is the legacy layout):
//! `from,to,distance,azimuth,inclination,left,right,up,down,backazimuth,backinclination,flags,notes`.
//! Everything after inclination is optional and notes is always last, so its commas survive. LRUD
//! cells speak the notebook shorthand ("5/10", "0", "P"). Flags: X exclude entirely, P don't plot, L
//! exclude from length, C don't adjust. What the columns cannot carry rides `#` lines: `# name:`,
//! `# date:`, `# team:`, `# declination:` (a record -- azimuths are stored TRUE), `# unit: ft|m`,
//! `# fix: <name> x y z`, `# startlrud: l,r,u,d`, `# startnote:`.

use crate::js::{num_str, parse_float, split_lines, split_ws, to_fixed, trim};
use crate::model::{
    Fixed, Lrud, PLUMB_DEG, Shot, Survey, lrud_entry_text, normalize_azimuth, parse_lrud_entry,
};

const DEFAULT_COLUMNS: [&str; 10] = [
    "from",
    "to",
    "distance",
    "azimuth",
    "inclination",
    "left",
    "right",
    "up",
    "down",
    "notes",
];

/// `#\s*([A-Za-z]+)\s*:\s*(.*)` on a line starting with '#': (key, value).
fn meta(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix('#')?;
    let rest = rest.trim_start_matches(crate::js::is_space);
    let n = rest.bytes().take_while(u8::is_ascii_alphabetic).count();
    if n == 0 {
        return None;
    }
    let key = &rest[..n];
    let after = rest[n..].trim_start_matches(crate::js::is_space);
    let value = after.strip_prefix(':')?;
    let value = value.trim_start_matches(crate::js::is_space);
    // `.*` stops at a line terminator, and the line has none
    Some((key.to_lowercase(), value.to_string()))
}

/// The separator a file uses: the one its first data line has most of (a European spreadsheet
/// writes `;`, some exports a tab), else the comma.
fn separator(content: &str) -> char {
    let first = split_lines(content)
        .into_iter()
        .map(|l| trim(&l).to_string())
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .unwrap_or_default();
    let count = |c: char| first.matches(c).count();
    if count(';') > count(',') && count(';') >= count('\t') {
        ';'
    } else if count('\t') > count(',') {
        '\t'
    } else {
        ','
    }
}

/// A line's cells. A cell that starts with a quote runs to its closing quote (`""` inside is one
/// quote), so a comma in a name or a comment does not split it; quotes elsewhere are ordinary.
fn cells_of(line: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = line.chars().peekable();
    loop {
        // blanks before an opening quote are not part of a quoted cell; before anything else
        // they are kept (a note written last keeps its spacing)
        let mut ahead = chars.clone();
        while ahead.peek().is_some_and(|c| *c == ' ') {
            ahead.next();
        }
        let mut cell = String::new();
        if ahead.peek() == Some(&'"') {
            chars = ahead;
            chars.next();
            while let Some(c) = chars.next() {
                if c == '"' {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        cell.push('"');
                    } else {
                        break;
                    }
                } else {
                    cell.push(c);
                }
            }
            // anything after the closing quote, up to the separator, belongs to the cell too
            while let Some(&c) = chars.peek() {
                if c == sep {
                    break;
                }
                cell.push(c);
                chars.next();
            }
        } else {
            while let Some(&c) = chars.peek() {
                if c == sep {
                    break;
                }
                cell.push(c);
                chars.next();
            }
        }
        out.push(cell);
        if chars.next().is_none() {
            break;
        }
    }
    out
}

/// A header cell as one of the column names this reads, and the length unit it names, if any
/// (`Distance (ft)`). `None` for a cell that is not a column this knows.
fn column_of(cell: &str) -> (Option<&'static str>, Option<&'static str>) {
    let lower = trim(cell).to_lowercase();
    let (name, unit) = match lower.find(['(', '[']) {
        Some(i) => {
            let u = lower[i + 1..]
                .trim_end_matches([')', ']'])
                .trim()
                .to_string();
            (trim(&lower[..i]).to_string(), Some(u))
        }
        None => (lower.clone(), None),
    };
    let unit = unit.and_then(|u| match u.as_str() {
        "ft" | "feet" | "foot" => Some("ft"),
        "m" | "metres" | "meters" | "metre" | "meter" => Some("m"),
        _ => None,
    });
    let words: String = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let col = match words.as_str() {
        "from" | "from station" | "station from" | "from st" | "fr" => "from",
        "to" | "to station" | "station to" | "to st" => "to",
        "distance" | "length" | "tape" | "dist" | "len" => "distance",
        "azimuth" | "bearing" | "compass" | "az" | "azm" | "heading" | "direction" => "azimuth",
        "inclination" | "clino" | "inc" | "incl" | "dip" | "gradient" | "slope"
        | "vertical angle" | "vertical" => "inclination",
        "left" | "l" => "left",
        "right" | "r" => "right",
        "up" | "u" | "ceiling" => "up",
        "down" | "d" | "floor" => "down",
        "backazimuth" | "back azimuth" | "back bearing" | "backbearing" | "back compass"
        | "backcompass" | "azimuth2" | "az2" => "backazimuth",
        "backinclination" | "back inclination" | "back clino" | "backclino" | "back gradient"
        | "inclination2" | "inc2" => "backinclination",
        "flags" | "flag" => "flags",
        "notes" | "note" | "comment" | "comments" | "remarks" | "remark" => "notes",
        _ => return (None, unit),
    };
    (Some(col), unit)
}

/// Reads a CSV survey.
pub fn parse(content: &str) -> Survey {
    let mut survey = Survey::default();
    let mut columns: Vec<String> = DEFAULT_COLUMNS.iter().map(|s| s.to_string()).collect();
    let sep = separator(content);
    let mut header_unit: Option<&'static str> = None;
    let mut unit_said = false;
    let mut rows_seen = 0usize;

    for raw in split_lines(content) {
        let line = trim(&raw);
        if line.is_empty() {
            continue;
        }
        if line.starts_with('#') {
            if let Some((key, val)) = meta(line) {
                let val = trim(&val).to_string();
                match key.as_str() {
                    "name" => survey.name = val,
                    "date" => survey.date = val,
                    "team" => survey.team = val,
                    "declination" => {
                        let d = parse_float(&val);
                        if !d.is_nan() {
                            survey.declination = d;
                            survey.declination_source = "file".into();
                        }
                    }
                    "unit" => {
                        unit_said = true;
                        let m = val.to_lowercase().starts_with('m');
                        survey.distance_unit = Some(if m { "m" } else { "ft" }.into());
                    }
                    "fix" => {
                        let ft = split_ws(&val);
                        if ft.len() >= 3 {
                            let (fx, fy) = (parse_float(&ft[1]), parse_float(&ft[2]));
                            let fz = if ft.len() >= 4 {
                                parse_float(&ft[3])
                            } else {
                                0.0
                            };
                            if !fx.is_nan() && !fy.is_nan() {
                                let z = if fz.is_nan() { 0.0 } else { fz };
                                survey.fixed.set(&ft[0], Fixed::at(fx, fy, z));
                            }
                        }
                    }
                    "startlrud" => {
                        let sc: Vec<&str> = val.split(',').collect();
                        let se = |i: usize| parse_lrud_entry(sc.get(i).copied().unwrap_or(""));
                        let (l, r, u, d) = (se(0), se(1), se(2), se(3));
                        survey.start_lrud = Some(Lrud {
                            left: l.value,
                            right: r.value,
                            up: u.value,
                            down: d.value,
                            left_all: l.all,
                            right_all: r.all,
                            up_all: u.all,
                            down_all: d.all,
                            left_open: l.open,
                            right_open: r.open,
                            up_open: u.open,
                            down_open: d.open,
                        });
                    }
                    "startnote" => survey.start_note = val,
                    _ => {}
                }
            }
            continue;
        }
        let owned = cells_of(line, sep);
        let owned: Vec<String> = if sep == ';' {
            // a semicolon file is a decimal-comma one: 10,5 is ten and a half
            owned
                .into_iter()
                .map(|c| {
                    let t = trim(&c);
                    if !t.is_empty()
                        && t.chars()
                            .all(|ch| ch.is_ascii_digit() || matches!(ch, ',' | '-' | '+'))
                    {
                        t.replace(',', ".")
                    } else {
                        c
                    }
                })
                .collect()
        } else {
            owned
        };
        let cells: Vec<&str> = owned.iter().map(String::as_str).collect();
        if cells.len() < 4 {
            continue;
        }
        // a header row maps the columns for everything below it: one naming both stations, in
        // whatever order and spelling (an unknown column is kept under its own name and ignored)
        let named: Vec<(Option<&str>, Option<&str>)> = cells.iter().map(|c| column_of(c)).collect();
        if named.iter().any(|(c, _)| *c == Some("from"))
            && named.iter().any(|(c, _)| *c == Some("to"))
        {
            columns = cells
                .iter()
                .zip(&named)
                .map(|(c, (col, _))| col.map_or_else(|| trim(c).to_lowercase(), str::to_string))
                .collect();
            header_unit = named
                .iter()
                .zip(&columns)
                .find(|(_, c)| c.as_str() == "distance")
                .and_then(|((_, u), _)| *u);
            continue;
        }
        rows_seen += 1;
        let mut rec: Vec<(String, String)> = Vec::new();
        let notes_last = columns.last().is_some_and(|c| c == "notes");
        for (ci, col) in columns.iter().enumerate() {
            if ci >= cells.len() {
                break;
            }
            // notes written last and unquoted keep their commas (the app's own files do this)
            if col == "notes" && notes_last && sep == ',' {
                rec.push(("notes".into(), trim(&cells[ci..].join(",")).to_string()));
                break;
            }
            match rec.iter_mut().find(|(k, _)| k == col) {
                Some(slot) => slot.1 = trim(cells[ci]).to_string(),
                None => rec.push((col.clone(), trim(cells[ci]).to_string())),
            }
        }
        let get = |k: &str| {
            rec.iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.as_str())
        };

        let dist = get("distance").map_or(f64::NAN, parse_float);
        if dist.is_nan() {
            continue;
        }
        let opt = |v: Option<&str>| match v {
            None | Some("") => None,
            Some(t) => Some(parse_float(t)).filter(|n| !n.is_nan()),
        };
        let lrud = |k: &str| parse_lrud_entry(get(k).unwrap_or(""));

        let mut shot = Shot {
            from: get("from").unwrap_or("").to_string(),
            to: get("to").unwrap_or("").to_string(),
            ..Shot::default()
        };
        shot.splay = shot.to.is_empty() || shot.to == "-";
        if shot.splay {
            shot.to.clear();
        }
        shot.distance = dist;
        let back_inc = opt(get("backinclination"));
        // an empty foresight beside a backsight was not taken: the backsight carries it
        shot.inclination =
            opt(get("inclination")).unwrap_or(if back_inc.is_some() { f64::NAN } else { 0.0 });
        // an empty azimuth cell is not a bearing of zero
        let az_raw = trim(get("azimuth").unwrap_or(""));
        let az = parse_float(az_raw);
        if (az_raw.is_empty() || az.is_nan()) && opt(get("backazimuth")).is_some() {
            shot.azimuth = f64::NAN;
        } else if az_raw.is_empty() || az.is_nan() {
            if shot.inclination.abs() >= PLUMB_DEG {
                shot.azimuth = 0.0;
                shot.azimuth_omitted = true;
            } else {
                let or_q = |v: Option<&str>| match v {
                    Some(t) if !t.is_empty() => t.to_string(),
                    _ => "?".to_string(),
                };
                survey.add_finding(
                    "warning",
                    "bearing-omitted-not-plumb",
                    format!(
                        "Leg {} to {} has no azimuth and is not plumb (inclination {}), so the leg was SKIPPED -- anything beyond it is now unconnected. A bearing may only be left out on a plumbed shot.",
                        or_q(get("from")),
                        or_q(get("to")),
                        to_fixed(shot.inclination, 2)
                    ),
                );
                continue;
            }
        } else {
            shot.azimuth = normalize_azimuth(az);
        }
        let (l, r, u, d) = (lrud("left"), lrud("right"), lrud("up"), lrud("down"));
        (shot.left, shot.left_all, shot.left_open) = (l.value, l.all, l.open);
        (shot.right, shot.right_all, shot.right_open) = (r.value, r.all, r.open);
        (shot.up, shot.up_all, shot.up_open) = (u.value, u.all, u.open);
        (shot.down, shot.down_all, shot.down_open) = (d.value, d.all, d.open);
        shot.back_azimuth = opt(get("backazimuth"));
        shot.back_inclination = opt(get("backinclination"));
        if let Some(f) = get("flags") {
            let f = f.to_uppercase();
            shot.exclude_from_all = f.contains('X');
            shot.exclude_from_plot = f.contains('P');
            shot.exclude_from_length = f.contains('L');
            shot.no_adjust = f.contains('C');
        }
        shot.notes = get("notes").unwrap_or("").to_string();
        survey.shots.push(shot);
    }
    // a `# unit:` line says it outright; otherwise a unit in the distance column's header does
    if !unit_said && let Some(u) = header_unit {
        survey.distance_unit = Some(u.into());
    }
    if survey.shots.is_empty() && rows_seen > 0 && survey.findings().is_empty() {
        survey.add_finding(
            "warning",
            "csv-no-legs",
            format!("No legs could be read from {rows_seen} row{}: a header row naming at least From, To, Distance and Azimuth (in any order) tells this reader which column is which.", if rows_seen == 1 { "" } else { "s" }),
        );
    }
    survey.ensure_trips();
    survey
}

/// Writes a survey as CSV, with its header comments.
pub fn write(survey: &Survey) -> String {
    let mut out: Vec<String> = Vec::new();
    let name = if !survey.cave_name.is_empty() {
        &survey.cave_name
    } else {
        &survey.name
    };
    if !name.is_empty() {
        out.push(format!("# name: {name}"));
    }
    if !survey.date.is_empty() {
        out.push(format!("# date: {}", survey.date));
    }
    if !survey.team.is_empty() {
        out.push(format!("# team: {}", survey.team));
    }
    if survey.declination != 0.0 && !survey.declination.is_nan() {
        out.push(format!("# declination: {}", num_str(survey.declination)));
    }
    out.push(format!(
        "# unit: {}",
        if survey.distance_unit.as_deref() == Some("m") {
            "m"
        } else {
            "ft"
        }
    ));
    for (fname, f) in survey.fixed.iter() {
        let fz = f.z();
        if fz.is_none() {
            out.push(format!("# {}", crate::model::unknown_elevation_note(fname)));
        }
        out.push(format!(
            "# fix: {fname} {} {} {}",
            opt_str(f.x),
            opt_str(f.y),
            num_str(fz.unwrap_or(0.0))
        ));
    }
    if let Some(sl) = &survey.start_lrud {
        out.push(format!(
            "# startlrud: {},{},{},{}",
            lrud_entry_text(sl.left, sl.left_all.as_deref(), sl.left_open),
            lrud_entry_text(sl.right, sl.right_all.as_deref(), sl.right_open),
            lrud_entry_text(sl.up, sl.up_all.as_deref(), sl.up_open),
            lrud_entry_text(sl.down, sl.down_all.as_deref(), sl.down_open)
        ));
    }
    if !survey.start_note.is_empty() {
        out.push(format!("# startnote: {}", survey.start_note));
    }
    out.push("from,to,distance,azimuth,inclination,left,right,up,down,backazimuth,backinclination,flags,notes".into());
    for s in &survey.shots {
        let mut flags = String::new();
        for (on, c) in [
            (s.exclude_from_all, 'X'),
            (s.exclude_from_plot, 'P'),
            (s.exclude_from_length, 'L'),
            (s.no_adjust, 'C'),
        ] {
            if on {
                flags.push(c);
            }
        }
        let row = [
            s.from.clone(),
            if s.splay { String::new() } else { s.to.clone() },
            num_str(s.distance),
            if s.azimuth_omitted || !s.azimuth.is_finite() {
                String::new()
            } else {
                num_str(s.azimuth)
            },
            if s.inclination.is_finite() {
                num_str(s.inclination)
            } else {
                String::new()
            },
            lrud_entry_text(s.left, s.left_all.as_deref(), s.left_open),
            lrud_entry_text(s.right, s.right_all.as_deref(), s.right_open),
            lrud_entry_text(s.up, s.up_all.as_deref(), s.up_open),
            lrud_entry_text(s.down, s.down_all.as_deref(), s.down_open),
            s.back_azimuth.map(num_str).unwrap_or_default(),
            s.back_inclination.map(num_str).unwrap_or_default(),
            flags,
            // one row per leg: a line break in a note would start a new one
            s.notes.replace(['\r', '\n'], " "),
        ];
        out.push(row.join(","));
    }
    out.join("\n") + "\n"
}

/// `String(v)` of a number that may be missing (JavaScript writes "null").
fn opt_str(v: Option<f64>) -> String {
    v.map_or_else(|| "null".into(), num_str)
}
