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
fn resolver(files: &HashMap<String, String>) -> impl Fn(&str) -> Option<String> + '_ {
    let by_path: Vec<(String, &String)> = files.iter().map(|(k, v)| (tidy(k), v)).collect();
    move |p: &str| {
        let p = tidy(p);
        let lower = p.to_lowercase();
        by_path
            .iter()
            .find(|(k, _)| *k == p)
            .or_else(|| by_path.iter().find(|(k, _)| k.to_lowercase() == lower))
            .or_else(|| {
                by_path
                    .iter()
                    .find(|(k, _)| base(k).to_lowercase() == base(&lower))
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
    names.sort();
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
    finish_import("the folder", read_folder(files))
}

/// Opens a survey data file as the cave (replacing the one open). Returns JSON
/// `{ok, message, warnings}`: what the reader could not use is in `warnings`.
#[wasm_bindgen]
pub fn import_survey(file_name: &str, text: &str, others: &str) -> String {
    finish_import(file_name, read_survey(file_name, text, others))
}

/// Makes what was read the open cave, and says how it went.
fn finish_import(file_name: &str, read: Result<(String, Survey), String>) -> String {
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
            Ok(doc) => {
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
}
