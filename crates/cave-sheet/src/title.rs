//! The title block: its fields, how a value wraps into lines, what the notebook can fill in without
//! asking, and which fields keep following the notebook after a person has typed over them.
//! Ported from the old `CsSheet`, `CsSheetSetup` (title lines, name de-duplication, auto fill) and
//! `CsSheetLink`, and checked against them (`tests/title_golden.rs`).
//!
//! The location field is never filled in here. A cave's coordinates must never reach a printed sheet
//! without a person typing them.

use crate::{CHAR_WIDTH, TEXT, TITLE_INCHES};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One title block field.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Field {
    /// Stable id (the key a value is stored under).
    pub id: &'static str,
    /// What the field is called in the form.
    pub label: &'static str,
    /// Printed ahead of the value.
    pub prefix: &'static str,
    /// Printed even when empty (an empty line invites a person to type one).
    pub required: bool,
    /// Follows the notebook until a person types over it.
    pub linked: bool,
    /// Differs from sheet to sheet (the sheet's name).
    pub per_sheet: bool,
    /// What to put in it.
    pub hint: &'static str,
}

/// The fields, in the order they are printed.
pub const FIELDS: &[Field] = &[
    Field {
        id: "caveName",
        label: "Cave name",
        prefix: "",
        required: true,
        linked: true,
        per_sheet: false,
        hint: "Unabbreviated, most prominent text on the sheet.",
    },
    Field {
        id: "sheetNumber",
        label: "Sheet",
        prefix: "Sheet:  ",
        required: false,
        linked: false,
        per_sheet: true,
        hint: "The sheet's name; changing either it or the sheet's name changes both.",
    },
    Field {
        id: "location",
        label: "Geographic location",
        prefix: "Location:  ",
        required: true,
        linked: false,
        per_sheet: false,
        hint: "State and county at minimum. Coordinates must name their system, units and datum.",
    },
    Field {
        id: "surveyedBy",
        label: "Surveyed by",
        prefix: "Surveyed by:  ",
        required: true,
        linked: true,
        per_sheet: false,
        hint: "Who held the instruments.",
    },
    Field {
        id: "cartographyBy",
        label: "Cartography by",
        prefix: "Cartography by:  ",
        required: true,
        linked: false,
        per_sheet: false,
        hint: "Who drew the map, with the year.",
    },
    Field {
        id: "date",
        label: "Survey date(s)",
        prefix: "Date surveyed:  ",
        required: true,
        linked: true,
        per_sheet: false,
        hint: "The date range of the fieldwork, not the drafting.",
    },
    Field {
        id: "surveyMethod",
        label: "Survey method",
        prefix: "Survey method:  ",
        required: false,
        linked: false,
        per_sheet: false,
        hint: "Instruments used, or the survey grade.",
    },
    Field {
        id: "length",
        label: "Surveyed length",
        prefix: "Length:  ",
        required: false,
        linked: true,
        per_sheet: false,
        hint: "Total surveyed length.",
    },
    Field {
        id: "depth",
        label: "Vertical extent",
        prefix: "Depth:  ",
        required: false,
        linked: true,
        per_sheet: false,
        hint: "Total vertical range.",
    },
    Field {
        id: "personnel",
        label: "Personnel",
        prefix: "Personnel:  ",
        required: false,
        linked: false,
        per_sheet: false,
        hint: "Full crew list, if different from Surveyed by.",
    },
    Field {
        id: "surveyCode",
        label: "Survey grade",
        prefix: "Survey code:  ",
        required: false,
        linked: true,
        per_sheet: false,
        hint: "e.g. UISv2 5-c.",
    },
    Field {
        id: "copyright",
        label: "Copyright",
        prefix: "\u{a9}  ",
        required: false,
        linked: false,
        per_sheet: false,
        hint: "If present, must carry a year.",
    },
    Field {
        id: "legendNote",
        label: "Legend note",
        prefix: "",
        required: false,
        linked: false,
        per_sheet: false,
        hint: "Which symbol standard the map follows.",
    },
];

/// A field by its id.
pub fn field(id: &str) -> Option<&'static Field> {
    FIELDS.iter().find(|f| f.id == id)
}

/// How many characters fit across the title block at a text height in inches.
pub fn chars_per_line(text_inches: f64) -> usize {
    if text_inches.is_nan() || text_inches <= 0.0 {
        return 40;
    }
    8.max((TITLE_INCHES / (text_inches * CHAR_WIDTH)).floor() as usize)
}

/// Greedy word wrap. A word longer than the budget keeps its own line rather than being cut in half.
pub fn wrap_text(text: &str, budget: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for w in text.split(' ').filter(|w| !w.is_empty()) {
        if line.is_empty() {
            line = w.to_string();
        } else if line.chars().count() + 1 + w.chars().count() <= budget {
            line.push(' ');
            line.push_str(w);
        } else {
            lines.push(std::mem::take(&mut line));
            line = w.to_string();
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// One printed line of the title block.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Line {
    /// The words.
    pub text: String,
    /// Text height in inches.
    pub inches: f64,
    /// The field this line begins (empty on a wrapped continuation).
    pub field_id: String,
}

/// The title block laid out as lines. A blank field prints anyway when required, and is left out when not.
pub fn title_lines(values: &BTreeMap<String, String>) -> Vec<Line> {
    let mut out = Vec::new();
    for f in FIELDS {
        let value = values.get(f.id).map(String::as_str).unwrap_or("");
        let blank = value.chars().all(char::is_whitespace);
        if blank && !f.required {
            continue;
        }
        let is_name = f.id == "caveName";
        let inches = if is_name { TEXT.cave_name } else { TEXT.body };
        let full = if is_name {
            if blank {
                "CAVE NAME".to_string()
            } else {
                value.to_string()
            }
        } else {
            format!("{}{}", f.prefix, value)
        };
        for (n, text) in wrap_text(&full, chars_per_line(inches))
            .into_iter()
            .enumerate()
        {
            out.push(Line {
                text,
                inches,
                field_id: if n == 0 {
                    f.id.to_string()
                } else {
                    String::new()
                },
            });
        }
    }
    out
}

/// How tall a stack of title lines is, in inches of paper.
pub fn lines_height(lines: &[Line]) -> f64 {
    lines.iter().map(|l| l.inches * crate::LINE_SPACING).sum()
}

fn name_key(n: &str) -> String {
    n.replace('.', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// A crew list with each person once. Two names are the same person when they match ignoring case,
/// spacing and full stops, or when the shorter is a first name (or "Nathan S.") that fits exactly ONE
/// longer name in the list. If two longer names fit (two Adams) the short one is left alone; guessing
/// which Adam is how a surveyor is credited with someone else's work. Different spellings are never
/// merged. The first spelling seen is kept, in the order names first appeared.
pub fn dedupe_names(names: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut unique: Vec<(String, Vec<String>)> = Vec::new();
    for n in names {
        let k = name_key(n);
        if k.is_empty() || !seen.insert(k.clone()) {
            continue;
        }
        unique.push((
            n.split_whitespace().collect::<Vec<_>>().join(" "),
            k.split(' ').map(str::to_string).collect(),
        ));
    }
    let fits = |s: &[String], l: &[String]| -> bool {
        if s.len() > l.len() || (s.len() == l.len() && s.join(" ") == l.join(" ")) {
            return false;
        }
        let mut at = 0;
        for st in s {
            let mut found = false;
            while at < l.len() {
                let t = &l[at];
                at += 1;
                if t == st || (st.chars().count() == 1 && t.starts_with(st.as_str())) {
                    found = true;
                    break;
                }
            }
            if !found {
                return false;
            }
        }
        l[0] == s[0] || (s[0].chars().count() == 1 && l[0].starts_with(s[0].as_str()))
    };
    let mut gone = vec![false; unique.len()];
    for a in 0..unique.len() {
        let candidates = (0..unique.len())
            .filter(|&b| a != b && !gone[b] && fits(&unique[a].1, &unique[b].1))
            .count();
        if candidates == 1 {
            gone[a] = true;
        }
    }
    unique
        .into_iter()
        .zip(gone)
        .filter(|(_, g)| !g)
        .map(|(u, _)| u.0)
        .collect()
}

/// "2024-04-06 to 2024-11-03", or the one date, or "".
pub fn dates_for(dates: &[String]) -> String {
    let mut seen: Vec<&str> = Vec::new();
    for d in dates.iter().map(|d| d.trim()).filter(|d| !d.is_empty()) {
        if !seen.contains(&d) {
            seen.push(d);
        }
    }
    seen.sort_unstable();
    match seen.len() {
        0 => String::new(),
        1 => seen[0].to_string(),
        n => format!("{} to {}", seen[0], seen[n - 1]),
    }
}

/// Everyone on every trip team, each once, comma separated. Every name, never a count and never
/// "and others": the people who carried the tape are the reason the map exists.
pub fn surveyed_by_for(teams: &[String]) -> String {
    let names: Vec<String> = teams
        .iter()
        .flat_map(|t| t.split(','))
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .collect();
    dedupe_names(&names).join(", ")
}

/// What the notebook knows, for filling the title block.
#[derive(Clone, Debug, Default)]
pub struct Notebook {
    /// The cave's name.
    pub name: String,
    /// Each trip's team (comma separated names).
    pub teams: Vec<String>,
    /// Each trip's date.
    pub dates: Vec<String>,
    /// Surveyed length, already worded with its unit ("1,234 ft").
    pub length: Option<String>,
    /// Vertical extent, worded with its unit.
    pub depth: Option<String>,
    /// The survey grade, if one is known.
    pub grade: Option<String>,
}

/// What the notebook can fill in, as `{field id: value}`. A field it has no answer for is absent, so a
/// value already typed is never replaced by nothing. The location is never here.
pub fn auto_fill(nb: &Notebook) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut put = |k: &str, v: String| {
        if !v.trim().is_empty() {
            out.insert(k.to_string(), v);
        }
    };
    put("caveName", nb.name.clone());
    put("surveyedBy", surveyed_by_for(&nb.teams));
    put("date", dates_for(&nb.dates));
    if let Some(l) = &nb.length {
        put("length", l.clone());
    }
    if let Some(d) = &nb.depth {
        put("depth", d.clone());
    }
    if let Some(g) = &nb.grade {
        put("surveyCode", g.clone());
    }
    out
}

/// Whether a field's value follows the notebook or was typed by a person.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Link {
    /// Follows the notebook.
    Auto,
    /// A person typed it; never overwritten again until they ask to relink.
    Manual,
}

/// A field's stored state in a title block.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Value {
    /// What is printed (without the prefix).
    pub text: String,
    /// Following the notebook or typed.
    pub link: Link,
}

/// A title block's values by field id.
pub type Values = BTreeMap<String, Value>;

/// Brings the linked fields up to date with the notebook and returns what changed. A manual field is
/// never touched; a field the notebook has no answer for is never blanked; a linked field always takes
/// the notebook's answer (a person's edit makes it manual at the moment they make it, see [`edit`]).
pub fn sync(values: &mut Values, filled: &BTreeMap<String, String>) -> Vec<String> {
    let mut changed = Vec::new();
    for f in FIELDS.iter().filter(|f| f.linked) {
        let Some(new) = filled.get(f.id) else {
            continue;
        };
        match values.get_mut(f.id) {
            Some(v) if v.link == Link::Manual => {}
            Some(v) => {
                if v.text != *new {
                    v.text = new.clone();
                    changed.push(f.id.to_string());
                }
            }
            None => {
                values.insert(
                    f.id.to_string(),
                    Value {
                        text: new.clone(),
                        link: Link::Auto,
                    },
                );
                changed.push(f.id.to_string());
            }
        }
    }
    changed
}

/// A person types into a field: it holds their words and (for a linked field) stops following the notebook.
pub fn edit(values: &mut Values, id: &str, text: &str) {
    values.insert(
        id.to_string(),
        Value {
            text: text.to_string(),
            link: Link::Manual,
        },
    );
}

/// A person asks a field to follow the notebook again (an empty value until the next [`sync`]).
pub fn relink(values: &mut Values, id: &str) {
    if field(id).is_some_and(|f| f.linked) {
        values.remove(id);
    }
}

/// The values as the plain `{field id: text}` the layout reads.
pub fn plain(values: &Values) -> BTreeMap<String, String> {
    values
        .iter()
        .map(|(k, v)| (k.clone(), v.text.clone()))
        .collect()
}
