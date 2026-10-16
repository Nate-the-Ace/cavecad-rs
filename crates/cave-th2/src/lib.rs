//! Therion sketch pages (`.th2`, which TopoDroid and PocketTopo also write) brought onto the map as
//! this app's own linework: walls, ledges, symbols, words and area fills that can be selected,
//! edited and restyled like anything drawn here, anchored to the survey so they follow it when it
//! is adjusted.
//!
//! Plan scraps are placed by their station markers (see [`place`]). Extended-elevation and
//! cross-section scraps are read and reported, not yet drawn.

pub mod place;
pub mod vocab;

use cave_doc::{Action, Document};
use cave_formats::th2::{Line, Scrap, Sketch};
use cave_id::IdSource;
use place::{Placement, Stations};
use std::collections::{BTreeMap, HashMap};
use vocab::Ink;

/// How far (metres) a flattened curve may stray from the drawn one: well under a pen line at 1:500.
pub const FLATNESS: f64 = 0.02;
/// A symbol's radius on paper, millimetres (a symbol dropped here is about this size).
pub const SYMBOL_MM: f64 = 3.0;
/// Words' height on paper, millimetres.
pub const TEXT_MM: f64 = 2.5;

/// What a sketch import did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    /// Plan scraps placed, by how.
    pub placed: BTreeMap<&'static str, usize>,
    /// Lines, symbols, words and fills made.
    pub lines: usize,
    #[allow(missing_docs)]
    pub symbols: usize,
    #[allow(missing_docs)]
    pub words: usize,
    #[allow(missing_docs)]
    pub fills: usize,
    /// Plan scraps not placed: name and why.
    pub not_placed: Vec<(String, String)>,
    /// Scraps of views not drawn yet, by projection.
    pub other_views: BTreeMap<String, usize>,
    /// Items left out on purpose, by reason.
    pub skipped: BTreeMap<&'static str, usize>,
    /// Types these tables do not know, by Therion name, and how many.
    pub unknown: BTreeMap<String, usize>,
    /// Tie names the survey has no station for.
    pub missing: Vec<String>,
    /// The worst disagreement, metres, between a scrap and the survey before bending: name and size.
    pub worst: Option<(String, f64)>,
}

impl Report {
    /// The report as sentences for a person.
    pub fn messages(&self, file: &str) -> Vec<String> {
        let mut out = Vec::new();
        let placed: usize = self.placed.values().sum();
        out.push(format!(
            "{file}: {placed} sketch scrap{} placed on the map ({} lines, {} symbols, {} words, {} fills).",
            plural(placed),
            self.lines,
            self.symbols,
            self.words,
            self.fills
        ));
        if let Some(n) = self.placed.get("one-station") {
            out.push(format!(
                "{n} scrap{} marked only one station, so {} placed at the scale it declares, north up the page, and not bent onto the survey. Check {} size and direction.",
                plural(*n),
                if *n == 1 { "it is" } else { "they are" },
                if *n == 1 { "its" } else { "their" }
            ));
        }
        for (name, why) in &self.not_placed {
            out.push(format!("Scrap \"{name}\" was not placed: {why}"));
        }
        for (view, n) in &self.other_views {
            let what = match view.as_str() {
                "extended" | "elevation" => "profile",
                "none" => "cross-section",
                _ => "other",
            };
            out.push(format!(
                "{n} {what} scrap{} ({view}) not drawn: only plan sketches are brought in so far.",
                plural(*n)
            ));
        }
        if !self.missing.is_empty() {
            let shown: Vec<&str> = self.missing.iter().take(6).map(String::as_str).collect();
            out.push(format!(
                "The sketch names {} station{} the survey does not have ({}{}); those markers were ignored.",
                self.missing.len(),
                plural(self.missing.len()),
                shown.join(", "),
                if self.missing.len() > 6 { ", ..." } else { "" }
            ));
        }
        if !self.unknown.is_empty() {
            let list: Vec<String> = self
                .unknown
                .iter()
                .map(|(k, n)| format!("{k} ({n})"))
                .collect();
            out.push(format!(
                "Types this app does not know yet, lines kept as plain sketch lines and points left out: {}.",
                list.join(", ")
            ));
        }
        if let Some((name, r)) = &self.worst
            && *r > 0.5
        {
            out.push(format!(
                "Scrap \"{name}\" disagreed with the survey by {r:.1} m before it was bent onto its stations: check it."
            ));
        }
        out
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// Actions that draw a sketch's plan scraps onto the map of `doc`.
///
/// `scope` is the survey the page was pulled in inside (`cave_formats::SketchInput::survey`);
/// `aliases` are other names of joined stations, alias to the name kept (`Survey::aliases`);
/// `adjusted` is whether the map shows the adjusted survey (strokes anchor to what is shown).
pub fn import(
    doc: &Document,
    ids: &mut impl IdSource,
    sketch: &Sketch,
    scope: &str,
    aliases: &[(String, String)],
    adjusted: bool,
) -> (Vec<Action>, Report) {
    let scene = cave_scene::build(doc, adjusted);
    let at: HashMap<String, [f64; 2]> = scene
        .stations
        .iter()
        .map(|s| (s.label.to_lowercase(), [s.at[0], s.at[1]]))
        .collect();
    // a joined station answers to each of its names
    let also = aliases
        .iter()
        .filter_map(|(alias, kept)| at.get(&kept.to_lowercase()).map(|p| (alias.clone(), *p)));
    let stations = Stations::new(at.clone().into_iter().chain(also), scope);
    let frames = cave_scene::map_frames(doc, adjusted);
    let paper = cave_draw::drawing_scale(doc) / 1000.0; // map metres per paper millimetre
    let mut report = Report::default();
    let mut actions = Vec::new();
    for scrap in &sketch.scraps {
        match scrap.projection.as_deref() {
            Some("plan") => {}
            other => {
                *report
                    .other_views
                    .entry(other.unwrap_or("unreadable").to_string())
                    .or_default() += 1;
                continue;
            }
        }
        let placement = match Placement::solve(scrap, &|n| stations.find(n)) {
            Ok(p) => p,
            Err(missing) => {
                let why = if scrap.stations.is_empty() {
                    "it marks no stations, so nothing ties it to the cave.".to_string()
                } else if missing.len() == scrap.stations.len() {
                    "none of the stations it marks are in the survey.".to_string()
                } else {
                    "its station markers all sit in one place.".to_string()
                };
                report.missing.extend(missing);
                report.not_placed.push((scrap.name.clone(), why));
                continue;
            }
        };
        report.missing.extend(placement.missing.iter().cloned());
        let how = match placement.kind {
            place::Kind::Warped => "bent",
            place::Kind::Similar => "two-station",
            place::Kind::Moved => "one-station",
        };
        *report.placed.entry(how).or_default() += 1;
        if report
            .worst
            .as_ref()
            .is_none_or(|w| placement.residual > w.1)
        {
            report.worst = Some((scrap.name.clone(), placement.residual));
        }
        draw_scrap(
            doc,
            scrap,
            &placement,
            &frames,
            paper,
            ids,
            &mut actions,
            &mut report,
        );
    }
    report.missing.sort();
    report.missing.dedup();
    (actions, report)
}

/// A line's points on the map, in the direction it was drawn (`-reverse` turned round).
fn points_of(line: &Line, p: &Placement) -> Vec<[f64; 2]> {
    let mut pts = p.flatten(&line.segs, FLATNESS);
    if line.reversed {
        pts.reverse();
    }
    pts
}

#[allow(clippy::too_many_arguments)]
fn draw_scrap(
    doc: &Document,
    scrap: &Scrap,
    p: &Placement,
    frames: &[cave_doc::Frame],
    paper: f64,
    ids: &mut impl IdSource,
    actions: &mut Vec<Action>,
    report: &mut Report,
) {
    let mut add = |kind: &str,
                   pts: &[[f64; 2]],
                   closed: bool,
                   text: &str,
                   actions: &mut Vec<Action>|
     -> bool {
        let Ok((id, a)) = cave_draw::map_line(doc, ids, kind, pts, None, frames) else {
            return false;
        };
        let Ok(a) = cave_draw::with_closed(a, closed) else {
            return false;
        };
        actions.extend(cave_draw::with_text(a, id, text));
        true
    };
    // a border an area claims is drawn as that area's outline, not again on its own
    let claimed: Vec<&str> = scrap
        .areas
        .iter()
        .flat_map(|a| a.line_ids.iter().map(String::as_str))
        .collect();
    let by_id: HashMap<&str, &Line> = scrap
        .lines
        .iter()
        .filter_map(|l| l.id.as_deref().map(|id| (id, l)))
        .collect();
    for line in &scrap.lines {
        let ink = vocab::line(&line.kind, line.subtype.as_deref());
        let (base, _) = vocab::split(&line.kind, None);
        if base == "border" && line.id.as_deref().is_some_and(|id| claimed.contains(&id)) {
            continue;
        }
        let kind = match ink {
            Ink::Line(k) => k,
            Ink::Skip(why) => {
                *report.skipped.entry(why).or_default() += 1;
                continue;
            }
            Ink::Text => {
                let words = line
                    .options
                    .get("text")
                    .map(|t| clean_text(t))
                    .unwrap_or_default();
                let Some(first) = line.segs.first() else {
                    continue;
                };
                if !words.is_empty()
                    && stamp_words(first.to, &words, None, None, p, paper, &mut add, actions)
                {
                    report.words += 1;
                }
                continue;
            }
            _ => {
                *report.unknown.entry(line.kind.clone()).or_default() += 1;
                "sketch"
            }
        };
        if add(kind, &points_of(line, p), line.closed, "", actions) {
            report.lines += 1;
        }
    }
    for area in &scrap.areas {
        let kind = match vocab::area(&area.kind) {
            Ink::Fill(f) => f,
            _ => {
                *report.unknown.entry(area.kind.clone()).or_default() += 1;
                "sketch"
            }
        };
        let parts: Vec<Vec<[f64; 2]>> = area
            .line_ids
            .iter()
            .filter_map(|id| by_id.get(id.as_str()))
            .map(|l| points_of(l, p))
            .filter(|pts| pts.len() >= 2)
            .collect();
        let outline = chain(parts);
        if outline.len() >= 3 && add(kind, &outline, true, "", actions) {
            if kind == "sketch" {
                report.lines += 1;
            } else {
                report.fills += 1;
            }
        }
    }
    for point in &scrap.points {
        match vocab::point(&point.kind) {
            Ink::Symbol(block) => {
                let Some(key) = vocab::symbol_key(block) else {
                    continue;
                };
                let at = p.at([point.x, point.y]);
                let up = match point.orientation {
                    Some(deg) => p.direction([point.x, point.y], deg),
                    None => [0.0, 1.0],
                };
                let r = SYMBOL_MM * paper * vocab::point_scale(point.scale.as_deref());
                if add(
                    key,
                    &[at, [at[0] + r * up[0], at[1] + r * up[1]]],
                    false,
                    "",
                    actions,
                ) {
                    report.symbols += 1;
                }
            }
            Ink::Text => {
                let words = point
                    .text
                    .clone()
                    .or_else(|| point.options.get("value").cloned())
                    .map(|t| clean_text(&t))
                    .unwrap_or_default();
                if !words.is_empty()
                    && stamp_words(
                        [point.x, point.y],
                        &words,
                        point.orientation,
                        point.scale.as_deref(),
                        p,
                        paper,
                        &mut add,
                        actions,
                    )
                {
                    report.words += 1;
                }
            }
            Ink::Skip(why) => *report.skipped.entry(why).or_default() += 1,
            _ => *report.unknown.entry(point.kind.clone()).or_default() += 1,
        }
    }
}

/// Places words at a sketch point: upright on the map unless the sketch turned them.
#[allow(clippy::too_many_arguments)]
fn stamp_words(
    at: [f64; 2],
    words: &str,
    orientation: Option<f64>,
    scale: Option<&str>,
    p: &Placement,
    paper: f64,
    add: &mut impl FnMut(&str, &[[f64; 2]], bool, &str, &mut Vec<Action>) -> bool,
    actions: &mut Vec<Action>,
) -> bool {
    let up = match orientation {
        Some(deg) => p.direction(at, deg),
        None => [0.0, 1.0],
    };
    let h = TEXT_MM * paper * vocab::point_scale(scale);
    let a = p.at(at);
    add(
        "text",
        &[a, [a[0] + h * up[0], a[1] + h * up[1]]],
        false,
        words,
        actions,
    )
}

/// Therion's text markup made plain: `<br>` breaks become spaces, other `<...>` tags go, and
/// `[value units]` brackets lose their brackets.
fn clean_text(t: &str) -> String {
    let mut out = String::new();
    let mut tag = false;
    let mut name = String::new();
    for c in t.chars() {
        match (tag, c) {
            (false, '<') => {
                tag = true;
                name.clear();
            }
            (true, '>') => {
                tag = false;
                if name.trim().eq_ignore_ascii_case("br") {
                    out.push(' ');
                }
            }
            (true, c) => name.push(c),
            (false, '[' | ']') => {}
            (false, c) => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Pieces of an outline joined end to end, each turned round when that makes it meet the last.
fn chain(mut parts: Vec<Vec<[f64; 2]>>) -> Vec<[f64; 2]> {
    let d = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]);
    let mut out: Vec<[f64; 2]> = Vec::new();
    while !parts.is_empty() {
        let next = match out.last() {
            None => 0,
            Some(&end) => {
                // the piece with an end nearest to where the outline has got to
                let mut best = (0, f64::INFINITY);
                for (i, q) in parts.iter().enumerate() {
                    let near = d(q[0], end).min(d(*q.last().unwrap(), end));
                    if near < best.1 {
                        best = (i, near);
                    }
                }
                best.0
            }
        };
        let mut q = parts.remove(next);
        if let Some(&end) = out.last() {
            if d(*q.last().unwrap(), end) < d(q[0], end) {
                q.reverse();
            }
            if d(q[0], end) < 1e-9 {
                q.remove(0);
            }
        }
        out.extend(q);
    }
    if out.len() > 1 && d(out[0], *out.last().unwrap()) < 1e-9 {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_is_made_plain() {
        assert_eq!(clean_text("Big<br>Room"), "Big Room");
        assert_eq!(clean_text("<lang:en>Sump"), "Sump");
        assert_eq!(clean_text("[12.5 m]"), "12.5 m");
    }

    #[test]
    fn an_outline_is_chained_end_to_end() {
        let out = chain(vec![
            vec![[0.0, 0.0], [1.0, 0.0]],
            vec![[1.0, 1.0], [0.0, 1.0], [0.0, 0.0]],
            vec![[1.0, 1.0], [1.0, 0.0]],
        ]);
        assert_eq!(out, vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
    }
}
