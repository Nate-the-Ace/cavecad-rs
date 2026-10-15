//! Survey data files in the browser -- Compass, Walls, Survex, Therion, CSV: thin doors onto
//! `cave-formats`, which holds the readers, the writers and the document bridge and is tested natively.

use crate::{DOC, OpenResult, install};
use cave_doc::ActorId;
use cave_formats::{FORMATS, Format, Survey, therion};
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

/// Reads a survey file. `others` is a JSON object of the other files picked with it (name to text),
/// which a Therion file's `input` lines are looked up in -- by path, and failing that by file name,
/// since a browser hands over names without folders.
pub(crate) fn read_survey(
    file_name: &str,
    text: &str,
    others: &str,
) -> Result<(Format, Survey), String> {
    let Some(format) = cave_formats::detect(file_name, text) else {
        return Err(format!(
            "{file_name} is not a survey file CaveCAD can read. It reads Compass (.dat), Walls (.srv), Survex (.svx), Therion (.th) and CSV."
        ));
    };
    let files: HashMap<String, String> = serde_json::from_str(others).unwrap_or_default();
    let mut survey = if format == Format::Therion && !files.is_empty() {
        let by_path: HashMap<String, &String> = files.iter().map(|(k, v)| (tidy(k), v)).collect();
        let read = |p: &str| {
            let p = tidy(p);
            by_path
                .get(&p)
                .or_else(|| {
                    by_path
                        .iter()
                        .find(|(k, _)| base(k) == base(&p))
                        .map(|(_, v)| v)
                })
                .map(|v| v.to_string())
        };
        therion::parse_with_inputs(text, file_name, &read)
    } else {
        format.parse(text)
    };
    if survey.cave_name.is_empty() && survey.name.is_empty() {
        // a file that names nothing is named after itself
        let stem = base(file_name);
        survey.cave_name = stem.rsplit_once('.').map_or(stem, |(s, _)| s).to_string();
    }
    if survey.shots.is_empty() {
        return Err(format!(
            "No survey shots were found in {file_name}, read as {}.",
            format.label()
        ));
    }
    Ok((format, survey))
}

/// Opens a survey data file as the cave (replacing the one open). Returns JSON
/// `{ok, message, warnings}`: what the reader could not use is in `warnings`.
#[wasm_bindgen]
pub fn import_survey(file_name: &str, text: &str, others: &str) -> String {
    let result = match read_survey(file_name, text, others) {
        Err(message) => OpenResult {
            ok: false,
            message,
            warnings: Vec::new(),
        },
        Ok((format, survey)) => match cave_formats::to_document(&survey, ActorId(1)) {
            Err(e) => OpenResult {
                ok: false,
                message: format!("{file_name} could not be turned into a cave ({e})."),
                warnings: Vec::new(),
            },
            Ok(doc) => {
                let scene = cave_scene::build(&doc, true);
                let message = format!(
                    "{}: {} stations, {} legs, {} loop closures",
                    format.label(),
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
    fn a_therion_input_is_found_among_the_other_files_picked() {
        let main = "survey cave\n  input passages/a.th\nendsurvey\n";
        let a = "survey a\n  centreline\n    data normal from to length compass clino\n    1 2 5 0 0\n  endcentreline\nendsurvey\n";
        let others = serde_json::json!({ "a.th": a }).to_string();
        let (_, survey) = read_survey("main.th", main, &others).unwrap();
        assert_eq!(survey.shots.len(), 1);
        assert!(survey.findings().is_empty(), "{:?}", survey.findings());
    }
}
