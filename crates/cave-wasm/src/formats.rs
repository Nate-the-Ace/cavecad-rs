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

/// Finds a file among those picked: by path, then by path ignoring case, then by file name ignoring
/// case (a browser hands over names without folders, and projects made on Windows do not keep case).
/// Among files of one name in different folders, the one whose folders agree most with the path
/// asked for wins (then the first by path), so the answer never depends on hash order.
fn resolver(files: &HashMap<String, String>) -> impl Fn(&str) -> Option<String> + '_ {
    let mut by_path: Vec<(String, &String)> = files.iter().map(|(k, v)| (tidy(k), v)).collect();
    by_path.sort_by(|a, b| a.0.cmp(&b.0));
    move |p: &str| {
        let p = tidy(p);
        let lower = p.to_lowercase();
        if let Some((_, v)) = by_path
            .iter()
            .find(|(k, _)| *k == p)
            .or_else(|| by_path.iter().find(|(k, _)| k.to_lowercase() == lower))
        {
            return Some(v.to_string());
        }
        let want: Vec<&str> = lower.rsplit('/').collect();
        by_path
            .iter()
            .filter(|(k, _)| base(k).to_lowercase() == base(&lower))
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
            .map(|(_, v)| v.to_string())
    }
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
    finish_import("the folder", read_folder(files), files)
}

/// Opens a survey data file as the cave (replacing the one open). Returns JSON
/// `{ok, message, warnings}`: what the reader could not use is in `warnings`.
#[wasm_bindgen]
pub fn import_survey(file_name: &str, text: &str, others: &str) -> String {
    finish_import(file_name, read_survey(file_name, text, others), others)
}

/// Draws the sketch pages a survey's project pulls in (`files`: JSON, path to text, where they are
/// looked up) onto `doc`'s map. Returns what happened, as sentences.
fn draw_sketches(doc: &mut cave_doc::Document, survey: &Survey, files: &str) -> Vec<String> {
    let files: HashMap<String, String> = serde_json::from_str(files).unwrap_or_default();
    let read = resolver(&files);
    let aliases: Vec<(String, String)> = survey
        .aliases
        .iter()
        .map(|(a, k)| (a.to_string(), k.clone()))
        .collect();
    let mut out = Vec::new();
    // a fresh cave, so a fixed sequence (as `to_document` gives its survey) cannot collide
    let mut ids = cave_id::SeqIds::new(0x5ce7);
    for page in &survey.sketches {
        let name = base(&page.path);
        let Some(text) = read(&page.path) else {
            out.push(format!(
                "The sketch page {name} was not among the files picked, so its drawing is missing."
            ));
            continue;
        };
        let sketch = cave_formats::th2::parse(&text);
        out.extend(
            sketch
                .findings
                .iter()
                .map(|f| format!("{name}: {}", f.message)),
        );
        let (actions, report) =
            cave_th2::import(doc, &mut ids, &sketch, &page.survey, &aliases, true);
        if let Err(e) = doc.edit(actions) {
            out.push(format!("{name} could not be drawn ({e:?})."));
            continue;
        }
        tracing::info!(source = "import", "drew sketch page {name}");
        out.extend(report.messages(name));
    }
    out
}

/// Makes what was read the open cave, and says how it went.
fn finish_import(file_name: &str, read: Result<(String, Survey), String>, files: &str) -> String {
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
                let drawn = draw_sketches(&mut doc, &survey, files);
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
                    message: String::new(),
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
