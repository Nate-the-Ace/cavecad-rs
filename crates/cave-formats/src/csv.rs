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

/// Reads a CSV survey.
pub fn parse(content: &str) -> Survey {
    let mut survey = Survey::default();
    let mut columns: Vec<String> = DEFAULT_COLUMNS.iter().map(|s| s.to_string()).collect();

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
        let cells: Vec<&str> = line.split(',').collect();
        if cells.len() < 4 {
            continue;
        }
        // a header row maps the columns for everything below it
        if parse_float(cells[2]).is_nan() && trim(cells[0]).eq_ignore_ascii_case("from") {
            columns = cells.iter().map(|c| trim(c).to_lowercase()).collect();
            continue;
        }
        let mut rec: Vec<(String, String)> = Vec::new();
        for (ci, col) in columns.iter().enumerate() {
            if ci >= cells.len() {
                break;
            }
            if col == "notes" {
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
            s.notes.clone(),
        ];
        out.push(row.join(","));
    }
    out.join("\n") + "\n"
}

/// `String(v)` of a number that may be missing (JavaScript writes "null").
fn opt_str(v: Option<f64>) -> String {
    v.map_or_else(|| "null".into(), num_str)
}
