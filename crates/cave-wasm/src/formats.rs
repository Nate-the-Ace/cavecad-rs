//! Survey data files in the browser -- Compass, Walls, Survex, Therion, CSV: thin doors onto
//! `cave-formats`, which holds the readers, the writers and the document bridge and is tested natively.

use crate::{DOC, OpenResult, install};
use cave_doc::ActorId;
use cave_formats::{FORMATS, Format, Survey};
use serde::Serialize;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
struct FormatInfo {
    id: &'static str,
    label: &'static str,
    extensions: &'static [&'static str],
}

/// The survey file formats (JSON array of `{id, label, extensions}`), in the order to list them.
#[wasm_bindgen]
pub fn survey_formats() -> String {
    let list: Vec<FormatInfo> = FORMATS
        .iter()
        .map(|f| FormatInfo {
            id: f.id(),
            label: f.label(),
            extensions: f.extensions(),
        })
        .collect();
    serde_json::to_string(&list).unwrap_or_else(|_| "[]".into())
}

/// `a/./b/../c` as `a/c`.
fn tidy(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for p in path.split(['/', '\\']) {
        match p {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            p => parts.push(p),
        }
    }
    parts.join("/")
}

/// The last part of a path.
fn base(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// A file name without the copy number a browser adds to a second download of it (`cave[1].th2`,
/// `cave (2).th2` are `cave.th2`): a phone's export downloaded twice still answers to its own name.
fn without_copy_number(name: &str) -> String {
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s, format!(".{e}")),
        None => (name, String::new()),
    };
    let strip = |s: &str, open: char, close: char| -> Option<String> {
        let inner = s.strip_suffix(close)?;
        let (head, n) = inner.rsplit_once(open)?;
        (!n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
            .then(|| head.trim_end().to_string())
    };
    let stem = strip(stem, '[', ']')
        .or_else(|| strip(stem, '(', ')'))
        .unwrap_or_else(|| stem.to_string());
    format!("{stem}{ext}")
}

/// Finds a file among those picked: by path, then by path ignoring case, then by file name ignoring
/// case (a browser hands over names without folders, and projects made on Windows do not keep case),
/// then by file name without a browser's copy number. Among files of one name in different
/// folders, the one whose folders agree most with the path asked for wins (then the first by
/// path), so the answer never depends on hash order. Answers with the key it was picked under.
fn finder(files: &HashMap<String, String>) -> impl Fn(&str) -> Option<String> + '_ {
    let mut by_path: Vec<(String, &String)> = files.keys().map(|k| (tidy(k), k)).collect();
    by_path.sort_by(|a, b| a.0.cmp(&b.0));
    move |p: &str| {
        let p = tidy(p);
        let lower = p.to_lowercase();
        if let Some((_, k)) = by_path
            .iter()
            .find(|(k, _)| *k == p)
            .or_else(|| by_path.iter().find(|(k, _)| k.to_lowercase() == lower))
        {
            return Some(k.to_string());
        }
        let want: Vec<&str> = lower.rsplit('/').collect();
        let nearest = |same: &dyn Fn(&str) -> bool| {
            by_path
                .iter()
                .filter(|(k, _)| same(k))
                .max_by_key(|(k, _)| {
                    let k = k.to_lowercase();
                    let agree = k
                        .rsplit('/')
                        .zip(&want)
                        .take_while(|(a, b)| a == *b)
                        .count();
                    // more folders agreeing first; among equals the first path (max_by_key keeps the last)
                    (agree, std::cmp::Reverse(k))
                })
                .map(|(_, k)| k.to_string())
        };
        let name = base(&lower).to_string();
        nearest(&|k: &str| base(k).to_lowercase() == name).or_else(|| {
            let name = without_copy_number(&name);
            nearest(&|k: &str| without_copy_number(&base(k).to_lowercase()) == name)
        })
    }
}

/// [`finder`], answering with the file's text.
fn resolver(files: &HashMap<String, String>) -> impl Fn(&str) -> Option<String> + '_ {
    let find = finder(files);
    move |p: &str| find(p).map(|k| files[&k].clone())
}

/// Reads a survey file, or a project file (Compass .mak, Walls .wpj, Therion thconfig) and the
/// data files it names. `others` is a JSON object of the other files picked with it (name to text),
/// which a project's files and a Therion file's `input` lines are looked up in. Returns what it was
/// read as, and the survey.
pub(crate) fn read_survey(
    file_name: &str,
    text: &str,
    others: &str,
) -> Result<(String, Survey), String> {
    let files: HashMap<String, String> = serde_json::from_str(others).unwrap_or_default();
    let (label, mut survey) = cave_formats::read_any(file_name, text, &resolver(&files))?;
    if survey.cave_name.is_empty() && survey.name.is_empty() {
        // a file that names nothing is named after itself
        let stem = base(file_name);
        survey.cave_name = stem.rsplit_once('.').map_or(stem, |(s, _)| s).to_string();
    }
    let missing = survey
        .findings()
        .iter()
        .filter(|f| f.code == "therion-input-missing")
        .count();
    if survey.shots.is_empty() && missing > 0 {
        // the survey lives in files beside this one, which a browser does not hand over unasked
        return Err(format!(
            "{file_name} is a Therion project's top file: its survey is in {missing} other file{} it pulls in with \"input\", and they were not picked with it. Open the whole folder instead: Import, then Project folder…, and pick the folder {file_name} is in.",
            if missing == 1 { "" } else { "s" }
        ));
    }
    if survey.shots.is_empty() {
        let why = survey
            .findings()
            .first()
            .map(|f| format!(" {}", f.message))
            .unwrap_or_default();
        return Err(format!(
            "No survey shots were found in {file_name}, read as {label}.{why}"
        ));
    }
    Ok((label, survey))
}

/// Reads a whole folder (path to text): through its project file when it has one (a Therion file
/// that pulls others in counts), otherwise every survey file in it, merged into one cave in name order.
pub(crate) fn read_folder(files_json: &str) -> Result<(String, Survey), String> {
    let files: HashMap<String, String> =
        serde_json::from_str(files_json).map_err(|e| e.to_string())?;
    let mut names: Vec<&String> = files.keys().collect();
    // the folder's own top file before any buried in sub-folders (a project keeps test and
    // experimental configs further down), then by name
    names.sort_by_key(|n| (tidy(n).matches('/').count(), n.to_string()));
    let others = files_json;
    if let Some(p) = names
        .iter()
        .find(|n| cave_formats::detect_project(n, &files[**n]).is_some())
    {
        return read_survey(p, &files[*p], others);
    }
    let input = |t: &str| {
        t.lines()
            .any(|l| l.trim_start().to_ascii_lowercase().starts_with("input "))
    };
    if let Some(p) = names
        .iter()
        .find(|n| n.to_lowercase().ends_with(".th") && input(&files[**n]))
    {
        return read_survey(p, &files[*p], others);
    }
    // a .txt in a folder is a survey only when its content says so (CSV claims the extension)
    let is_survey = |n: &str| {
        if n.to_lowercase().ends_with(".txt") {
            cave_formats::detect_by_content(&files[n]).is_some()
        } else {
            cave_formats::detect(n, &files[n]).is_some()
        }
    };
    let surveys: Vec<&String> = names.into_iter().filter(|n| is_survey(n)).collect();
    let Some(first) = surveys.first() else {
        return Err(
            "There is no survey data in this folder (Compass, Walls, Survex, Therion or CSV)."
                .into(),
        );
    };
    let (label, mut survey) = read_survey(first, &files[*first], "{}")?;
    for n in &surveys[1..] {
        match read_survey(n, &files[*n], "{}") {
            Ok((_, s)) => survey.absorb(s),
            Err(e) => survey.add_finding("warning", "folder-file", e),
        }
    }
    let label = if surveys.len() == 1 {
        label
    } else {
        format!("{} survey files", surveys.len())
    };
    Ok((label, survey))
}

/// Opens every survey file of a folder (JSON object, path to text) as one cave. Returns JSON
/// `{ok, message, warnings}`, as [`import_survey`].
#[wasm_bindgen]
pub fn import_folder(files: &str) -> String {
    // a folder holds pages its project leaves out on purpose: only those pulled in are drawn
    finish_import("the folder", read_folder(files), files, false)
}

/// Opens a survey data file as the cave (replacing the one open). Returns JSON
/// `{ok, message, warnings}`: what the reader could not use is in `warnings`.
#[wasm_bindgen]
pub fn import_survey(file_name: &str, text: &str, others: &str) -> String {
    finish_import(
        file_name,
        read_survey(file_name, text, others),
        others,
        true,
    )
}

/// A Therion sketch page (`.th2`), which TopoDroid and the like export apart from the survey.
fn is_sketch(name: &str) -> bool {
    name.to_lowercase().ends_with(".th2")
}

/// Draws the sketch pages a survey's project pulls in (`files`: JSON, path to text, where they are
/// looked up) onto `doc`'s map. With `loose`, every other sketch page among `files` is drawn too,
/// inside the survey its station names fit best: picked with the data on purpose, though the data
/// does not pull it in (TopoDroid leaves that line out, or writes it as a comment). Returns what
/// happened, as sentences.
fn draw_sketches(
    doc: &mut cave_doc::Document,
    survey: &Survey,
    files: &str,
    loose: bool,
) -> Vec<String> {
    let files: HashMap<String, String> = serde_json::from_str(files).unwrap_or_default();
    let find = finder(&files);
    let aliases: Vec<(String, String)> = survey
        .aliases
        .iter()
        .map(|(a, k)| (a.to_string(), k.clone()))
        .collect();
    let mut out = Vec::new();
    // pages to draw: (file key, name to report, survey scope); `None` scope is worked out
    let mut pages: Vec<(String, String, Option<String>)> = Vec::new();
    for page in &survey.sketches {
        match find(&page.path) {
            Some(k) if !pages.iter().any(|p| p.0 == k) => {
                pages.push((k, base(&page.path).to_string(), Some(page.survey.clone())))
            }
            Some(_) => {}
            None => out.push(format!(
                "The sketch page {} was not among the files picked, so its drawing is missing.",
                base(&page.path)
            )),
        }
    }
    if loose {
        let mut extra: Vec<&String> = files
            .keys()
            .filter(|k| is_sketch(k) && !pages.iter().any(|p| &p.0 == *k))
            .collect();
        extra.sort();
        for k in extra {
            pages.push((k.clone(), base(k).to_string(), None));
        }
    }
    let station_names: Vec<String> = cave_scene::build(doc, true)
        .stations
        .iter()
        .map(|s| s.label.clone())
        .collect();
    // a fresh cave, so a fixed sequence (as `to_document` gives its survey) cannot collide
    let mut ids = cave_id::SeqIds::new(0x5ce7);
    for (key, name, scope) in pages {
        let sketch = cave_formats::th2::parse(&files[&key]);
        out.extend(
            sketch
                .findings
                .iter()
                .map(|f| format!("{name}: {}", f.message)),
        );
        let scope = scope.unwrap_or_else(|| cave_th2::place::likely_scope(&sketch, &station_names));
        let (actions, report) = cave_th2::import(doc, &mut ids, &sketch, &scope, &aliases, true);
        if let Err(e) = doc.edit(actions) {
            out.push(format!("{name} could not be drawn ({e:?})."));
            continue;
        }
        tracing::info!(source = "import", "drew sketch page {name}");
        out.extend(report.messages(&name));
        if sketch
            .scraps
            .iter()
            .any(|s| matches!(s.projection.as_deref(), Some("extended" | "elevation")))
        {
            let (actions, report) = cave_th2::import_profile(doc, &mut ids, &sketch, &scope, true);
            if let Err(e) = doc.edit(actions) {
                out.push(format!("{name}'s profile could not be drawn ({e:?})."));
                continue;
            }
            out.extend(report.messages(&name));
        }
    }
    out
}

/// Makes what was read the open cave, and says how it went.
fn finish_import(
    file_name: &str,
    read: Result<(String, Survey), String>,
    files: &str,
    loose: bool,
) -> String {
    finish_import_with(file_name, read, |doc, survey| {
        draw_sketches(doc, survey, files, loose)
    })
}

/// [`finish_import`], drawing whatever `draw` draws onto the new cave before it opens.
fn finish_import_with(
    file_name: &str,
    read: Result<(String, Survey), String>,
    draw: impl FnOnce(&mut cave_doc::Document, &Survey) -> Vec<String>,
) -> String {
    let result = match read {
        Err(message) => OpenResult {
            ok: false,
            message,
            warnings: Vec::new(),
        },
        Ok((label, survey)) => match cave_formats::to_document(&survey, ActorId(1)) {
            Err(e) => OpenResult {
                ok: false,
                message: format!("{file_name} could not be turned into a cave ({e})."),
                warnings: Vec::new(),
            },
            Ok(mut doc) => {
                let drawn = draw(&mut doc, &survey);
                let scene = cave_scene::build(&doc, true);
                let message = format!(
                    "{}: {} stations, {} legs, {} loop closures",
                    label,
                    scene.stations.len(),
                    scene.legs.len(),
                    scene.closures.len()
                );
                tracing::info!(source = "import", "imported {file_name}: {message}");
                install(doc);
                OpenResult {
                    ok: true,
                    message,
                    warnings: survey
                        .findings()
                        .iter()
                        .map(|f| f.message.clone())
                        .chain(drawn)
                        .collect(),
                }
            }
        },
    };
    if !result.ok {
        tracing::warn!(
            source = "import",
            "could not import {file_name}: {}",
            result.message
        );
    }
    serde_json::to_string(&result).unwrap_or_else(|_| "{}".into())
}

/// Opens a TopoDroid archive (the ZIP TopoDroid's survey Export makes first) as the cave: its
/// survey, and its plan sketches drawn on the map. `index` is JSON `[[name, start, length], ...]`
/// for the files of the zip, laid end to end in `data` (the browser unzips). Returns JSON
/// `{ok, message, warnings}`, as [`import_survey`].
#[wasm_bindgen]
pub fn import_topodroid(archive_name: &str, index: &str, data: &[u8]) -> String {
    let index: Vec<(String, usize, usize)> = serde_json::from_str(index).unwrap_or_default();
    let files: Vec<(String, Vec<u8>)> = index
        .into_iter()
        .filter_map(|(n, start, len)| Some((n, data.get(start..start.checked_add(len)?)?.to_vec())))
        .collect();
    let archive = cave_formats::topodroid::read_archive(&files);
    let (read, plots, shift) = match archive {
        Ok(a) => (
            Ok(("a TopoDroid survey".to_string(), a.survey)),
            a.plots,
            a.bearing_shift,
        ),
        Err(e) => (Err(e), Vec::new(), 0.0),
    };
    finish_import_with(archive_name, read, |doc, _| draw_plots(doc, &plots, shift))
}

/// A TopoDroid survey's sketches: plans on the map, profiles on the runs' profile bands, cross
/// sections on their stations' sections, each tied by the station names TopoDroid stores in it.
/// `shift` is what the archive turned every bearing by (see `Archive::bearing_shift`).
fn draw_plots(
    doc: &mut cave_doc::Document,
    plots: &[cave_formats::topodroid::Plot],
    shift: f64,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut ids = cave_id::SeqIds::new(0x5ce7);
    let mut sections = 0;
    let mut sections_missed: Vec<String> = Vec::new();
    for p in plots {
        let Some(sketch) = &p.sketch else { continue };
        let drawn = sketch
            .scraps
            .iter()
            .any(|s| !s.lines.is_empty() || !s.points.is_empty());
        if !drawn {
            continue;
        }
        out.extend(sketch.findings.iter().map(|f| f.message.clone()));
        let name = format!("sketch {}", p.name);
        let (actions, report) = match p.projection {
            "plan" => cave_th2::import(doc, &mut ids, sketch, "", &[], true),
            "extended" | "elevation" => cave_th2::import_profile(doc, &mut ids, sketch, "", true),
            _ => match section_of(doc, p, shift) {
                Ok((station, to_plane)) => {
                    sections += 1;
                    let trip = cave_scene::section_trip(doc, true, &station.to_string());
                    cave_th2::import_section(doc, &mut ids, sketch, station, trip, to_plane)
                }
                Err(why) => {
                    sections_missed.push(format!("{} ({why})", p.name));
                    continue;
                }
            },
        };
        if let Err(e) = doc.edit(actions) {
            out.push(format!("The {name} could not be drawn ({e:?})."));
            continue;
        }
        tracing::info!(source = "import", "drew TopoDroid {name}");
        if p.projection == "none" {
            // one line for all the sections, below; only what went wrong in one is told
            out.extend(
                report
                    .messages(&name)
                    .into_iter()
                    .filter(|m| !m.contains("placed on")),
            );
        } else {
            out.extend(report.messages(&name));
        }
    }
    if sections > 0 {
        out.push(
            format!(
                "{sections} cross-section sketch{} drawn on {} stations' cross sections.",
                if sections == 1 { " was" } else { "es were" },
                if sections == 1 { "its" } else { "their" },
            )
            .replace("its stations'", "its station's"),
        );
    }
    if !sections_missed.is_empty() {
        out.push(format!(
            "Cross-section sketches not drawn: {}.",
            sections_missed.join(", ")
        ));
    }
    out
}

/// Where a TopoDroid cross section goes: the station whose section it is drawn on, and how a
/// point of its page lands on that section's `[across, up]` metres.
///
/// TopoDroid draws a section on a plane facing its plot's bearing and tilt
/// (`DrawingWindow.makeSinglelegSectionReferences`): the page's right is the plane's horizontal
/// across, its up the plane's up. The page is pinned by a station it marks (both of a leg's when
/// it has them), so where on the leg TopoDroid cut it does not matter: each point becomes a
/// point in the cave, and that point is seen on the station's own section, whose plane follows
/// the passage here.
/// A station's section, and how a page point lands on it.
type SectionOf = (cave_id::Id, Box<dyn Fn([f64; 2]) -> [f64; 2]>);

fn section_of(
    doc: &cave_doc::Document,
    p: &cave_formats::topodroid::Plot,
    shift: f64,
) -> Result<SectionOf, String> {
    let scene = cave_scene::build(doc, true);
    let find = |label: &str| {
        scene
            .stations
            .iter()
            .find(|s| !label.is_empty() && s.label.eq_ignore_ascii_case(label))
    };
    let leg_section = matches!(p.kind, 3 | 5);
    // the station it is drawn on: a station's own, or the leg's end nearer the cut
    let near = if leg_section && !p.view.is_empty() && p.intercept.is_none_or(|t| t >= 0.5) {
        &p.view
    } else {
        &p.start
    };
    let home = find(near)
        .or_else(|| find(&p.start))
        .ok_or_else(|| format!("station {near} is not in the survey"))?;
    let station: cave_id::Id = home.id.parse().map_err(|_| "no station id".to_string())?;
    let (right, up) = cave_scene::section_frame(doc, true, station)
        .ok_or_else(|| format!("station {} is on no leg", home.label))?;

    // the page's plane: across (its right) and up, east-north-up
    let (a, c) = ((p.azimuth + shift).to_radians(), p.clino.to_radians());
    let across = [a.cos(), -a.sin(), 0.0];
    let page_up = [-c.sin() * a.sin(), -c.sin() * a.cos(), c.cos()];
    // a horizontal section of a steep leg is turned so north is up the page (TopoDroid's
    // mHThreshold, 70 degrees); undone here, in the page's own units
    // (in TopoDroid's own frame, x north: xn = V1.x, yn = -V2.x)
    let turn = (p.kind == 3 && p.clino.abs() > 70.0).then(|| {
        let a0 = p.azimuth.to_radians();
        (-a0.sin(), c.sin() * a0.cos())
    });

    // the page point a station is drawn at (TopoDroid's 20 units to the metre, y up)
    let sketch = p.sketch.as_ref().ok_or("no drawing")?;
    let ties: Vec<(String, [f64; 2])> = sketch
        .scraps
        .iter()
        .flat_map(cave_th2::place::ties)
        .map(|t| (t.name, [t.x, t.y]))
        .collect();
    let pin = [near, &p.view, &p.start]
        .iter()
        .find_map(|n| {
            let (_, at) = ties.iter().find(|(t, _)| t.eq_ignore_ascii_case(n))?;
            Some((find(n)?.at, *at))
        })
        // no station marked: the page's centre is where TopoDroid puts the station it is about
        .unwrap_or((home.at, [100.0, -120.0]));
    let (pos, page) = pin;
    // the pinned station, seen from the section's station within the page's plane: what lies
    // off the plane (the leg running through it) is not on the page
    let normal = [a.sin() * c.cos(), a.cos() * c.cos(), c.sin()];
    let d = [
        pos[0] - home.at[0],
        pos[1] - home.at[1],
        pos[2] - home.at[2],
    ];
    let off = d[0] * normal[0] + d[1] * normal[1] + d[2] * normal[2];
    let base = [
        d[0] - off * normal[0],
        d[1] - off * normal[1],
        d[2] - off * normal[2],
    ];
    let to_plane = move |q: [f64; 2]| {
        // page units from the pinned station: x right, y up (the sketch's y is already up)
        let (mut x, mut y) = ((q[0] - page[0]) / 20.0, (q[1] - page[1]) / 20.0);
        if let Some((xn, yn)) = turn {
            // TopoDroid drew (x', y'down) = (-yn x + xn y, -xn x - yn y) with y down: undo it
            let (sx, sy) = (x, -y);
            let det = xn * xn + yn * yn;
            let (ox, oy) = ((-yn * sx - xn * sy) / det, (xn * sx - yn * sy) / det);
            x = ox;
            y = -oy;
        }
        let w = [
            base[0] + x * across[0] + y * page_up[0],
            base[1] + x * across[1] + y * page_up[1],
            base[2] + x * across[2] + y * page_up[2],
        ];
        let dot = |v: [f64; 3]| w[0] * v[0] + w[1] * v[1] + w[2] * v[2];
        [dot(right), dot(up)]
    };
    Ok((station, Box::new(to_plane)))
}

#[derive(Serialize)]
struct Exported {
    ok: bool,
    /// The file's text, when `ok`.
    text: String,
    /// The extension to save it with.
    extension: &'static str,
    /// Why nothing was written, when not `ok`.
    message: String,
}

/// The open cave as a survey data file in `format` (an id from [`survey_formats`]). Returns JSON
/// `{ok, text, extension, message}`.
#[wasm_bindgen]
pub fn export_survey(format: &str) -> String {
    let out = match Format::from_id(format) {
        None => Exported {
            ok: false,
            text: String::new(),
            extension: "",
            message: format!("There is no survey format called {format}."),
        },
        Some(f) => DOC.with(|d| match d.borrow().as_ref() {
            None => Exported {
                ok: false,
                text: String::new(),
                extension: f.extensions()[0],
                message: "Nothing to export: no cave is open.".into(),
            },
            Some(doc) => {
                let mut survey = cave_formats::from_document(doc);
                let text = f.write(&mut survey);
                tracing::info!(
                    source = "file",
                    "exported {} shots as {}",
                    survey.shots.len(),
                    f.label()
                );
                Exported {
                    ok: true,
                    text,
                    extension: f.extensions()[0],
                    // what the format could not hold: names (and what they became), fixes
                    message: survey
                        .findings()
                        .iter()
                        .filter(|x| x.code.starts_with("export-"))
                        .map(|x| x.message.clone())
                        .collect::<Vec<_>>()
                        .join(" "),
                }
            }
        }),
    };
    serde_json::to_string(&out).unwrap_or_else(|_| "{}".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SVX: &str = "*begin main\n*data normal from to tape compass clino\n1 2 10.0 0 0\n2 3 10.0 90 0\n*end main\n";

    #[test]
    fn a_survex_file_imports_and_exports_as_compass() {
        let r: serde_json::Value =
            serde_json::from_str(&import_survey("cave.svx", SVX, "{}")).unwrap();
        assert_eq!(r["ok"], true, "{r}");
        assert!(r["message"].as_str().unwrap().starts_with("Survex"));
        let e: serde_json::Value = serde_json::from_str(&export_survey("compass")).unwrap();
        assert_eq!(e["ok"], true);
        assert_eq!(e["extension"], "dat");
        let text = e["text"].as_str().unwrap();
        assert!(text.contains("DECLINATION:"));
        assert_eq!(cave_formats::compass::parse(text).shots.len(), 2);
    }

    #[test]
    fn a_file_that_names_no_cave_is_named_after_itself() {
        let svx = "*data normal from to tape compass clino\n1 2 10.0 0 0\n";
        let (_, survey) = read_survey("folder/Blue Spring.svx", svx, "{}").unwrap();
        assert_eq!(survey.cave_name, "Blue Spring");
        let (_, named) = read_survey("x.svx", SVX, "{}").unwrap();
        assert_eq!(
            named.cave_name, "",
            "a *begin name is the cave's own: {named:?}"
        );
    }

    #[test]
    fn a_file_that_is_not_a_survey_says_so() {
        let r: serde_json::Value =
            serde_json::from_str(&import_survey("notes.txt", "hello there", "{}")).unwrap();
        assert_eq!(r["ok"], false);
    }

    #[test]
    fn a_compass_project_reads_the_files_picked_with_it() {
        let dat = "CAVE\r\nSURVEY NAME: A\r\nSURVEY DATE: 7 10 2024  COMMENT:\r\nSURVEY TEAM:\r\nAnn\r\nDECLINATION: 0.00  FORMAT: DDDDLRUDLADNF\r\n\r\nFROM TO LENGTH BEARING INC LEFT UP DOWN RIGHT\r\n\r\nA1 A2 10.00 90.00 0.00 1 1 1 1\r\n\u{0C}";
        // the project names it in upper case with a folder; the browser gave a bare lower-case name
        let others = serde_json::json!({ "a.dat": dat }).to_string();
        let (label, survey) =
            read_survey("cave.mak", "#DATA\\A.DAT,A1[M,1,2,3];", &others).unwrap();
        assert!(label.starts_with("Compass project"), "{label}");
        assert_eq!(survey.shots.len(), 1);
        assert!(survey.fixed.has("A1"));
    }

    #[test]
    fn a_folder_without_a_project_file_is_every_survey_in_it() {
        let files = serde_json::json!({
            "cave/a.svx": "*data normal from to tape compass clino\n1 2 10 0 0\n",
            "cave/b.svx": "*data normal from to tape compass clino\n2 3 10 90 0\n",
            "cave/notes.txt": "nothing to see",
        })
        .to_string();
        let (label, survey) = read_folder(&files).unwrap();
        assert_eq!(label, "2 survey files");
        assert_eq!(survey.shots.len(), 2);
    }

    #[test]
    fn a_therion_input_is_found_among_the_other_files_picked() {
        let main = "survey cave\n  input passages/a.th\nendsurvey\n";
        let a = "survey a\n  centreline\n    data normal from to length compass clino\n    1 2 5 0 0\n  endcentreline\nendsurvey\n";
        let others = serde_json::json!({ "a.th": a }).to_string();
        let (_, survey) = read_survey("main.th", main, &others).unwrap();
        assert_eq!(survey.shots.len(), 1);
        assert!(survey.findings().is_empty(), "{:?}", survey.findings());
    }

    #[test]
    fn a_therion_sketch_page_is_drawn_onto_the_map() {
        let th = "survey cave\n  input pages/plan.th2\n  centreline\n    data normal from to length compass clino\n    1 2 10 90 0\n    2 3 10 0 0\n  endcentreline\nendsurvey\n";
        let th2 = "encoding utf-8\nscrap s1 -projection plan -scale [0 0 100 0 0 0 10 0 m]\n  point 0 0 station -name 1\n  point 100 0 station -name 2\n  point 100 100 station -name 3\n  line wall\n    -10 10\n    90 10\n    90 110\n  endline\n  line border -id pool -close on\n    20 20\n    40 20\n    30 40\n  endline\n  area water\n    pool\n  endarea\n  point 50 0 stalagmite\n  point 50 50 label -text \"Big Room\"\nendscrap\n";
        let others = serde_json::json!({ "plan.th2": th2 }).to_string();
        let r: serde_json::Value =
            serde_json::from_str(&import_survey("cave.th", th, &others)).unwrap();
        assert_eq!(r["ok"], true, "{r}");
        let said = r["warnings"].to_string();
        assert!(
            said.contains("1 sketch scrap placed on the map (1 lines, 1 symbols, 1 words, 1 fills"),
            "{said}"
        );
        let lines: serde_json::Value = serde_json::from_str(&crate::map_lines(true)).unwrap();
        let wall = lines
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["kind"] == "wall")
            .unwrap();
        // the sketch's (90, 110) is 1 m past station 3 at (10, 10) east and north
        let last = &wall["points"][2];
        assert!(
            (last[0].as_f64().unwrap() - 9.0).abs() < 1e-6
                && (last[1].as_f64().unwrap() - 11.0).abs() < 1e-6,
            "{wall}"
        );
    }

    /// A TopoDroid page: plan scrap, 1 m = 39.3701 units, stations E1..E3 on a 10 m east, 10 m north run.
    const TOPODROID_TH2: &str = "encoding utf-8\n# created by TopoDroid v. 6.3.20\nscrap Cave-1p -projection plan -scale [0 0 39.3701 0 0 0 1 0 m]\nline wall\n  0 39.3701\n  393.701 39.3701\n  393.701 433.0711\nendline\nline u:user\n  10 10\n  20 20\nendline\npoint 0 0 station -name \"E1\"\npoint 393.701 0 station -name \"E2\"\npoint 393.701 393.701 station -name \"E3\"\nendscrap\n";

    fn wall_end() -> (f64, f64) {
        let lines: serde_json::Value = serde_json::from_str(&crate::map_lines(true)).unwrap();
        let wall = lines
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["kind"] == "wall")
            .expect("a wall was drawn")
            .clone();
        let last = &wall["points"][2];
        (last[0].as_f64().unwrap(), last[1].as_f64().unwrap())
    }

    #[test]
    fn a_topodroid_page_picked_with_its_data_is_drawn_though_the_data_only_mentions_it() {
        // TopoDroid writes the input line as a comment, and the browser renamed the second download
        let th = "encoding utf-8\nsurvey Cave -title \"Cave\"\n  centreline\n    data normal from to length compass clino\n    E1 E2 10 90 0\n    E2 E3 10 0 0\n  endcentreline\n  #  input \"Cave-1p.th2\"\nendsurvey\n";
        let others = serde_json::json!({ "Cave-1p[1].th2": TOPODROID_TH2 }).to_string();
        let r: serde_json::Value =
            serde_json::from_str(&import_survey("Cave.th", th, &others)).unwrap();
        assert_eq!(r["ok"], true, "{r}");
        let said = r["warnings"].to_string();
        assert!(said.contains("1 sketch scrap placed on the map"), "{said}");
        assert!(said.contains("u:user (1)"), "{said}");
        // the page's (393.7, 433.1) is 1 m north of E3 at (10, 10)
        let (x, y) = wall_end();
        assert!(
            (x - 10.0).abs() < 1e-3 && (y - 11.0).abs() < 1e-3,
            "{x} {y}"
        );
    }

    /// `TOPODROID_DIR=<unzipped archive> cargo test -p cave-wasm real_topodroid -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_topodroid_archive_opens_with_its_sketches() {
        let Ok(dir) = std::env::var("TOPODROID_DIR") else {
            return;
        };
        let mut data = Vec::new();
        let mut index = Vec::new();
        for e in std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok()) {
            let b = std::fs::read(e.path()).unwrap();
            index.push((
                e.file_name().to_string_lossy().to_string(),
                data.len(),
                b.len(),
            ));
            data.extend(b);
        }
        let r = import_topodroid("cave.zip", &serde_json::to_string(&index).unwrap(), &data);
        println!("{r}");
        let r: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert_eq!(r["ok"], true);
        if let Ok(out) = std::env::var("DUMP_DIR") {
            std::fs::write(format!("{out}/lines.json"), crate::map_lines(true)).unwrap();
            let legs: Vec<[f64; 4]> = crate::DOC.with(|d| {
                let d = d.borrow();
                let scene = cave_scene::build(d.as_ref().unwrap(), true);
                scene
                    .legs
                    .iter()
                    .map(|l| [l.from[0], l.from[1], l.to[0], l.to[1]])
                    .collect()
            });
            std::fs::write(
                format!("{out}/legs.json"),
                serde_json::to_string(&legs).unwrap(),
            )
            .unwrap();
            let runs: Vec<serde_json::Value> =
                serde_json::from_str(&crate::profile_runs(true)).unwrap();
            let bands: Vec<serde_json::Value> = runs
                .iter()
                .map(|r| {
                    serde_json::from_str(&crate::profile_band(r["key"].as_str().unwrap(), true))
                        .unwrap()
                })
                .collect();
            std::fs::write(
                format!("{out}/sections.json"),
                crate::map_sections(true, ""),
            )
            .unwrap();
            std::fs::write(
                format!("{out}/bands.json"),
                serde_json::to_string(&bands).unwrap(),
            )
            .unwrap();
        }
    }

    /// Every two-station TopoDroid section: its second station lands where the survey has it.
    /// `TOPODROID_DIR=<unzipped archive> cargo test -p cave-wasm real_topodroid_sections -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_topodroid_sections_agree_with_the_survey() {
        let Ok(dir) = std::env::var("TOPODROID_DIR") else {
            return;
        };
        let files: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| {
                (
                    e.file_name().to_string_lossy().to_string(),
                    std::fs::read(e.path()).unwrap(),
                )
            })
            .collect();
        let a = cave_formats::topodroid::read_archive(&files).unwrap();
        let doc = cave_formats::to_document(&a.survey, cave_doc::ActorId(1)).unwrap();
        let scene = cave_scene::build(&doc, true);
        let (mut good, mut bad) = (0, 0);
        for p in a.plots.iter().filter(|p| p.projection == "none") {
            let Some(sk) = &p.sketch else { continue };
            let ties = cave_th2::place::ties(&sk.scraps[0]);
            if ties.len() < 2 {
                continue;
            }
            // the page's plane, as section_of builds it
            let (az, c) = (
                (p.azimuth + a.bearing_shift).to_radians(),
                p.clino.to_radians(),
            );
            let across = [az.cos(), -az.sin(), 0.0];
            let page_up = [-c.sin() * az.sin(), -c.sin() * az.cos(), c.cos()];
            let pos = |n: &str| scene.stations.iter().find(|s| s.label == n).unwrap().at;
            let (t0, t1) = (&ties[0], &ties[1]);
            let (p0, p1) = (pos(&t0.name), pos(&t1.name));
            let w = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let dot = |v: [f64; 3]| w[0] * v[0] + w[1] * v[1] + w[2] * v[2];
            let want = [dot(across), dot(page_up)];
            let got = [(t1.x - t0.x) / 20.0, (t1.y - t0.y) / 20.0];
            let off = (got[0] - want[0]).hypot(got[1] - want[1]);
            if off < 0.1 {
                good += 1
            } else {
                bad += 1
            }
            println!(
                "{} kind {} az {:.0} clino {:.0}: page {:.2},{:.2} survey {:.2},{:.2}",
                p.name, p.kind, p.azimuth, p.clino, got[0], got[1], want[0], want[1]
            );
        }
        println!("ties placed right {good}, wrong {bad}");
    }

    /// How far a TopoDroid profile's strokes lie from the legs they are tied to.
    #[test]
    #[ignore]
    fn real_topodroid_profile_reach() {
        let Ok(dir) = std::env::var("TOPODROID_DIR") else {
            return;
        };
        let files: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| {
                (
                    e.file_name().to_string_lossy().to_string(),
                    std::fs::read(e.path()).unwrap(),
                )
            })
            .collect();
        let a = cave_formats::topodroid::read_archive(&files).unwrap();
        let doc = cave_formats::to_document(&a.survey, cave_doc::ActorId(1)).unwrap();
        let label = |id: cave_id::Id| {
            doc.entity(id)
                .map(|e| e.text(cave_doc::Field::StationLabel))
                .unwrap_or_default()
        };
        let bands: Vec<cave_th2::views::Band> = cave_scene::runs(&doc, true)
            .into_iter()
            .filter_map(|r| {
                let f = cave_scene::profile_frames(&doc, &r.key, true)?;
                Some(cave_th2::views::Band {
                    key: r.key,
                    legs: f
                        .iter()
                        .map(|f| (label(f.a), label(f.b), f.pa, f.pb))
                        .collect(),
                })
            })
            .collect();
        for p in a
            .plots
            .iter()
            .filter(|p| p.projection != "plan" && p.projection != "none")
        {
            let Some(sk) = &p.sketch else { continue };
            let sc = &sk.scraps[0];
            let Some(place) = cave_th2::views::ProfilePlace::new(sc, &bands, "") else {
                continue;
            };
            let mut far: Vec<(f64, usize)> = sc
                .lines
                .iter()
                .map(|l| {
                    let d = l
                        .segs
                        .iter()
                        .map(|s| place.reach(s.to))
                        .fold(f64::INFINITY, f64::min);
                    (d, l.segs.len())
                })
                .collect();
            far.sort_by(|x, y| y.0.total_cmp(&x.0));
            let ties = cave_th2::place::ties(sc).len();
            println!(
                "{}: {} ties, {} legs tied, strokes {}; nearest-leg distance of farthest strokes: {:?}",
                p.name,
                ties,
                place.legs(),
                far.len(),
                &far[..far.len().min(8)]
            );
        }
    }

    /// A `.tdr` as TopoDroid 6.4 writes it: one line, and station names.
    fn tdr(line: &[[f32; 2]], stations: &[(&str, [f32; 2])]) -> Vec<u8> {
        let mut b: Vec<u8> = Vec::new();
        let int = |b: &mut Vec<u8>, v: i32| b.extend(v.to_be_bytes());
        let flt = |b: &mut Vec<u8>, v: f32| b.extend(v.to_be_bytes());
        let utf = |b: &mut Vec<u8>, s: &str| {
            b.extend((s.len() as u16).to_be_bytes());
            b.extend(s.as_bytes());
        };
        b.push(b'V');
        int(&mut b, 604100);
        b.push(b'L');
        utf(&mut b, "wall");
        utf(&mut b, "");
        b.extend([0, 0]);
        for v in [0, -1, 1, 0, 0] {
            int(&mut b, v); // outline, lside, level, scrap, scale
        }
        utf(&mut b, "");
        int(&mut b, line.len() as i32);
        for p in line {
            flt(&mut b, p[0]);
            flt(&mut b, p[1]);
            b.push(0);
        }
        b.push(b'F');
        for (n, p) in stations {
            b.push(b'X');
            flt(&mut b, p[0]);
            flt(&mut b, p[1]);
            utf(&mut b, n);
            int(&mut b, 1);
            int(&mut b, 0);
            int(&mut b, -1);
        }
        b.push(b'E');
        b
    }

    #[test]
    fn a_topodroid_profile_and_section_land_on_their_views() {
        let sql = "INSERT into surveys values( 1, \"c\", \"2024.06.01\", \"\", 0.0, \"\", \"0\", 0, 0, 0 );\n\
INSERT into shots values( 1, 1, \"0\", \"1\", 10.00, 90.00, 0.00, 0, 0, 0, 0, 1, 0, 0, 0, \"\", 0, 0, 0, 0.00, \"\" );\n\
INSERT into shots values( 1, 2, \"1\", \"2\", 10.00, 90.00, 0.00, 0, 0, 0, 0, 1, 0, 0, 0, \"\", 0, 0, 0, 0.00, \"\" );\n\
INSERT into plots values( 1, 1, \"1s\", 2, 0, \"0\", \"\", 0, 0, 1, 0, 0, \"\", \"\", 0, 0, -1.00 );\n\
INSERT into plots values( 1, 2, \"xx0\", 5, 0, \"0\", \"1\", 0, 0, 18, 90.0, 0, \"\", \"\", 0, 0, -1.00 );\n";
        // the profile: 0, 1, 2 along 20 m at 20 units a metre (TopoDroid's page y runs down), and
        // a ceiling 1 m above them
        let profile = tdr(
            &[[100.0, 100.0], [500.0, 100.0]],
            &[
                ("0", [100.0, 120.0]),
                ("1", [300.0, 120.0]),
                ("2", [500.0, 120.0]),
            ],
        );
        // the section of leg 0-1, looking east, centred on 1: a floor 1 m either side, 1 m down
        let section = tdr(&[[80.0, 140.0], [120.0, 140.0]], &[("1", [100.0, 120.0])]);
        let files: Vec<(&str, Vec<u8>)> = vec![
            ("manifest", b"6.4\n61\n".to_vec()),
            ("survey.sql", sql.as_bytes().to_vec()),
            ("c-1s.tdr", profile),
            ("c-xx0.tdr", section),
        ];
        let mut data = Vec::new();
        let mut index = Vec::new();
        for (n, b) in &files {
            index.push((n.to_string(), data.len(), b.len()));
            data.extend(b);
        }
        let r: serde_json::Value = serde_json::from_str(&import_topodroid(
            "c.zip",
            &serde_json::to_string(&index).unwrap(),
            &data,
        ))
        .unwrap();
        assert_eq!(r["ok"], true, "{r}");
        let said = r["warnings"].to_string();
        assert!(said.contains("placed on the profile"), "{said}");
        assert!(said.contains("1 cross-section sketch was drawn"), "{said}");

        // on the profile: every point 1 m above the leg it is beside
        let runs: Vec<serde_json::Value> =
            serde_json::from_str(&crate::profile_runs(true)).unwrap();
        let mut ceiling = Vec::new();
        for run in &runs {
            let band: serde_json::Value =
                serde_json::from_str(&crate::profile_band(run["key"].as_str().unwrap(), true))
                    .unwrap();
            for sk in band["sketches"].as_array().unwrap() {
                ceiling.extend(
                    sk["points"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|p| p[1].as_f64().unwrap()),
                );
            }
        }
        assert!(!ceiling.is_empty());
        assert!(
            ceiling.iter().all(|z| (z - 1.0).abs() < 1e-3),
            "{ceiling:?}"
        );

        // on station 1's section: 1 m down, from 1 m one side to 1 m the other
        let secs: Vec<serde_json::Value> =
            serde_json::from_str(&crate::map_sections(true, "")).unwrap();
        let one = secs
            .iter()
            .find(|s| s["label"] == "1")
            .expect("station 1's section");
        let pts = one["sketches"][0]["points"].as_array().unwrap();
        let xs: Vec<f64> = pts.iter().map(|p| p[0].as_f64().unwrap()).collect();
        assert!(
            pts.iter()
                .all(|p| (p[1].as_f64().unwrap() + 1.0).abs() < 1e-3),
            "{pts:?}"
        );
        assert!(
            (xs[0] + 1.0).abs() < 1e-3 && (xs[xs.len() - 1] - 1.0).abs() < 1e-3,
            "{xs:?}"
        );
    }

    /// A real project folder, as Import > Project folder hands it over.
    /// `FOLDER=<dir> cargo test -p cave-wasm real_project_folder -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_project_folder_opens() {
        let Ok(dir) = std::env::var("FOLDER") else {
            return;
        };
        let root = std::path::PathBuf::from(&dir);
        let mut files = serde_json::Map::new();
        let mut stack = vec![root.clone()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().filter_map(|e| e.ok()) {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                let name = p.file_name().unwrap().to_string_lossy().to_lowercase();
                let wanted = [
                    ".th", ".th2", ".svx", ".dat", ".mak", ".srv", ".wpj", ".csv",
                ]
                .iter()
                .any(|x| name.ends_with(x))
                    || name == "thconfig";
                if !wanted {
                    continue;
                }
                let rel = p.strip_prefix(&root).unwrap().to_string_lossy().to_string();
                let text = String::from_utf8_lossy(&std::fs::read(&p).unwrap()).to_string();
                files.insert(rel, serde_json::Value::String(text));
            }
        }
        let r: serde_json::Value = serde_json::from_str(&import_folder(
            &serde_json::Value::Object(files).to_string(),
        ))
        .unwrap();
        println!("{}", r["message"]);
        for w in r["warnings"].as_array().unwrap() {
            println!("- {w}");
        }
        assert_eq!(r["ok"], true);
    }

    #[test]
    fn a_projects_top_file_alone_says_to_open_its_folder() {
        let main = "survey cave\n  input surveys/a.th\n  input surveys/b.th\nendsurvey\n";
        let err = read_survey("main.th", main, "{}").unwrap_err();
        assert!(
            err.contains("2 other files") && err.contains("Project folder"),
            "{err}"
        );
    }

    #[test]
    fn a_namespace_off_survey_keeps_its_stations_names() {
        // as Therion's -namespace off: the stations are 0, 1, 2, and A79@inner reaches A79
        let main = "survey top -namespace off\n  survey inner -namespace off\n    centreline\n      data normal from to length compass clino\n      0 1 10 0 0\n      A78 A79 5 90 0\n    endcentreline\n  endsurvey\n  centreline\n    data normal from to length compass clino\n    1 2 10 0 0\n  endcentreline\n  equate 2 A79@inner\nendsurvey\n";
        let (_, s) = read_survey("main.th", main, "{}").unwrap();
        let names: Vec<&str> = s
            .shots
            .iter()
            .flat_map(|x| [x.from.as_str(), x.to.as_str()])
            .collect();
        assert!(names.contains(&"0") && names.contains(&"1"), "{names:?}");
        assert!(!names.iter().any(|n| n.contains('.')), "{names:?}");
        // the equate joined 2 and A79: one of the names stands for both
        assert!(
            !(names.contains(&"2") && names.contains(&"A79")),
            "{names:?}"
        );
    }

    #[test]
    fn a_browsers_copy_number_is_not_part_of_the_name() {
        assert_eq!(without_copy_number("Cave-1p[1].th2"), "Cave-1p.th2");
        assert_eq!(without_copy_number("Cave-1p (12).th2"), "Cave-1p.th2");
        assert_eq!(without_copy_number("Cave-1p.th2"), "Cave-1p.th2");
        assert_eq!(without_copy_number("Cave[a].th2"), "Cave[a].th2");
    }

    #[test]
    fn of_two_files_named_alike_the_nearer_folder_wins() {
        let files: HashMap<String, String> = [
            ("a/scraps/p.th2".to_string(), "A".to_string()),
            ("b/scraps/p.th2".to_string(), "B".to_string()),
            ("c/p.th2".to_string(), "C".to_string()),
        ]
        .into();
        let r = resolver(&files);
        assert_eq!(r("x/b/scraps/p.th2").as_deref(), Some("B"));
        assert_eq!(r("zz/scraps/P.TH2").as_deref(), Some("A"));
        assert_eq!(r("c/p.th2").as_deref(), Some("C"));
    }

    #[test]
    fn a_folder_opens_through_its_top_project_file() {
        let th = |n: &str, len: u32| {
            format!(
                "survey {n}\ncentreline\ndata normal from to length compass clino\n1 2 {len} 0 0\nendcentreline\nendsurvey\n"
            )
        };
        let files = serde_json::json!({
            "cave/configs/a/old.thconfig": "source ../../x.th\n",
            "cave/thconfig": "source main.th\n",
            "cave/main.th": th("main", 10),
            "cave/x.th": th("x", 20),
        })
        .to_string();
        let (_, survey) = read_folder(&files).unwrap();
        assert_eq!(
            survey.shots[0].distance, 10.0,
            "read the buried config instead"
        );
    }
}
