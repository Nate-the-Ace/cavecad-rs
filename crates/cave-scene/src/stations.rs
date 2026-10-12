//! The station table: one row per station and what kind of place it is. A port of the old
//! `CsStationTable` (and the parts of `CsFrontier` it uses), checked against it on the test caves.
//!
//! A lead is not a record: surveyors already write "LEAD ..." in a note, so the table reads the notes,
//! and every other kind (open end, junction, control, loop, noted, flagged) is a derived fact computed
//! fresh each time and never stored. What is stored is the team's own mark on a row (status, team note,
//! who has it), as fields of the station, so it is shared with everyone editing the cave.
//!
//! No coordinates, ever: a row carries elevation and nothing of the plan position, because rows feed a
//! printable trip packet and a cave's location does not leave the cave.

use crate::solved::solve;
use cave_doc::{Document, Field, Kind};
use cave_id::Id;
use cave_survey::SkipReason;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// The kinds, in the order badges are listed.
pub const KINDS: [&str; 7] = [
    "lead", "openEnd", "junction", "control", "loop", "noted", "flagged",
];

/// A note and the trip it was written on.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Note {
    /// The words.
    pub text: String,
    /// The trip's position in survey order (0 first).
    pub trip: usize,
}

/// The loop a station lies on.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LoopMark {
    /// Misclosure as a percent of the loop's length.
    pub percent: Option<f64>,
    /// The 3D misclosure in metres.
    pub error: f64,
}

/// One station's row.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StationRow {
    /// The station's id as text.
    pub id: String,
    /// Its label.
    pub station: String,
    /// What kind of place it is (see `KINDS`).
    pub kinds: Vec<&'static str>,
    /// Trips (by position, 0 first) that surveyed to or from it.
    pub trips: Vec<usize>,
    /// How many legs touch it.
    pub degree: usize,
    /// The notes written at it.
    pub notes: Vec<Note>,
    /// All of them joined with " | ".
    pub note_text: String,
    /// The notes that read as leads.
    pub lead_notes: Vec<Note>,
    /// Elevation in metres (`None` when never placed). Never a plan position.
    pub z: Option<f64>,
    /// The loops it lies on.
    pub loops: Vec<LoopMark>,
    /// Why it is flagged: shots at it that could not be used, in words.
    pub flags: Vec<String>,
    /// The team's mark.
    pub status: String,
    /// The team's note.
    pub team: String,
    /// Who has it.
    pub who: String,
    /// What the table would suggest (`pushed` when a later trip surveyed on from a lead); never applied.
    pub suggest: &'static str,
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Does the text contain `word` as a whole word, ignoring case? ("lead" matches "LEAD W" and "a lead?",
/// not "leader" or "misled".)
fn has_word(text: &str, word: &str) -> bool {
    let (t, w) = (text.to_lowercase(), word.to_lowercase());
    if w.is_empty() {
        return false;
    }
    let mut from = 0;
    while let Some(i) = t[from..].find(&w) {
        let start = from + i;
        let end = start + w.len();
        let before_ok = t[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !is_word_char(c));
        let after_ok = t[end..].chars().next().is_none_or(|c| !is_word_char(c));
        if before_ok && after_ok {
            return true;
        }
        from = start + t[start..].chars().next().map_or(1, char::len_utf8);
    }
    false
}

/// A note that says the survey deliberately ended here: EOS, E.O.S. or END OF SURVEY as a word.
pub fn says_closed(note: &str) -> bool {
    let t = note.to_lowercase();
    ["eos", "e.o.s.", "e.o.s", "end of survey"]
        .iter()
        .any(|token| {
            let mut from = 0;
            while let Some(i) = t[from..].find(token) {
                let start = from + i;
                let end = start + token.len();
                let before_ok = t[..start]
                    .chars()
                    .next_back()
                    .is_none_or(|c| !c.is_ascii_alphanumeric());
                let after_ok = t[end..]
                    .chars()
                    .next()
                    .is_none_or(|c| !c.is_ascii_alphanumeric());
                if before_ok && after_ok {
                    return true;
                }
                from = start + 1;
            }
            false
        })
}

/// Station names in the order a caver reads them: A2 before A10.
pub fn compare_natural(a: &str, b: &str) -> std::cmp::Ordering {
    fn parts(s: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for c in s.chars() {
            let digit = c.is_ascii_digit();
            match out.last_mut() {
                Some(last) if last.chars().next().is_some_and(|f| f.is_ascii_digit()) == digit => {
                    last.push(c)
                }
                _ => out.push(c.to_string()),
            }
        }
        out
    }
    let (pa, pb) = (parts(a), parts(b));
    for (x, y) in pa.iter().zip(&pb) {
        if x == y {
            continue;
        }
        let (dx, dy) = (
            x.chars().all(|c| c.is_ascii_digit()),
            y.chars().all(|c| c.is_ascii_digit()),
        );
        if dx && dy {
            let (nx, ny) = (
                x.parse::<u128>().unwrap_or(0),
                y.parse::<u128>().unwrap_or(0),
            );
            if nx != ny {
                return nx.cmp(&ny);
            }
            continue;
        }
        let (lx, ly) = (x.to_lowercase(), y.to_lowercase());
        if lx != ly {
            return lx.cmp(&ly);
        }
        return x.cmp(y);
    }
    pa.len().cmp(&pb.len())
}

/// One row per station, unsorted by anything but natural name order. `keyword` is the word a note must
/// contain to be a lead (empty: "lead").
pub fn station_rows(doc: &Document, use_adjustment: bool, keyword: &str) -> Vec<StationRow> {
    let solved = solve(doc, use_adjustment);
    let keyword = if keyword.trim().is_empty() {
        "lead"
    } else {
        keyword.trim()
    };

    // trips by position
    let mut trips: Vec<(f64, Id)> = doc
        .ids_of(Kind::Trip)
        .into_iter()
        .map(|t| {
            (
                doc.entity(t)
                    .and_then(|e| e.number(Field::TripOrder))
                    .unwrap_or(0.0),
                t,
            )
        })
        .collect();
    trips.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let trip_index: HashMap<Id, usize> = trips
        .iter()
        .enumerate()
        .map(|(i, (_, t))| (*t, i))
        .collect();

    let label = |id: Id| {
        doc.entity(id)
            .map(|e| e.text(Field::StationLabel).trim().to_string())
            .unwrap_or_default()
    };
    let mut shots: Vec<(f64, Id)> = doc
        .ids_of(Kind::Shot)
        .into_iter()
        .map(|s| {
            (
                doc.entity(s)
                    .and_then(|e| e.number(Field::ShotOrder))
                    .unwrap_or(f64::MAX),
                s,
            )
        })
        .collect();
    shots.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));

    let mut degree: BTreeMap<String, usize> = BTreeMap::new();
    let mut notes: BTreeMap<String, Vec<Note>> = BTreeMap::new();
    let mut seen_trips: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    let mut closed: BTreeSet<String> = BTreeSet::new();
    let mut ids: BTreeMap<String, Id> = BTreeMap::new();
    let mut flags: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut first_leg: Option<(String, usize, Option<Id>)> = None;
    let skipped: HashMap<Id, SkipReason> = solved
        .ex
        .skipped
        .iter()
        .map(|s| (s.shot, s.reason))
        .collect();
    for (_, id) in &shots {
        let Some(e) = doc.entity(*id) else { continue };
        if e.flag(Field::ShotSplay) || e.flag(Field::ShotExcludeFromAll) {
            continue;
        }
        let (Some(f), Some(t)) = (e.reference(Field::ShotFrom), e.reference(Field::ShotTo)) else {
            continue;
        };
        let (from, to) = (label(f), label(t));
        if from.is_empty() || to.is_empty() || from == to {
            continue;
        }
        let trip_ref = e.reference(Field::ShotTrip);
        let trip = trip_ref
            .and_then(|r| trip_index.get(&r).copied())
            .unwrap_or(0);
        if first_leg.is_none() {
            first_leg = Some((from.clone(), trip, trip_ref));
        }
        for name in [&from, &to] {
            *degree.entry(name.clone()).or_default() += 1;
            seen_trips.entry(name.clone()).or_default().insert(trip);
        }
        ids.entry(from.clone()).or_insert(f);
        ids.entry(to.clone()).or_insert(t);
        let note = e.text(Field::ShotNote);
        let note = note.trim();
        if !note.is_empty() {
            notes.entry(to.clone()).or_default().push(Note {
                text: note.to_string(),
                trip,
            });
            if says_closed(note) {
                closed.insert(to.clone());
            }
        }
        // a leg whose readings could not be used flags the station it arrives at
        if let Some(reason) = skipped.get(id)
            && matches!(reason, SkipReason::Unusable | SkipReason::MissingStation)
        {
            flags
                .entry(to.clone())
                .or_default()
                .push("a shot here has missing or impossible readings".to_string());
        }
    }
    // the first station's own note rides on its trip's opening row
    if let Some((from, trip, trip_ref)) = &first_leg
        && let Some(t) = trip_ref.and_then(|r| doc.entity(r))
    {
        let start = t.text(Field::TripStartNote);
        let start = start.trim();
        if !start.is_empty() {
            notes.entry(from.clone()).or_default().insert(
                0,
                Note {
                    text: start.to_string(),
                    trip: *trip,
                },
            );
        }
    }
    // control: every fixed station, and the station the survey starts from
    let mut anchors: BTreeSet<String> = solved
        .ex
        .fixed
        .iter()
        .map(|f| label(f.station))
        .filter(|l| !l.is_empty())
        .collect();
    if let Some((from, _, _)) = &first_leg {
        anchors.insert(from.clone());
    }
    // loops
    let mut loops: BTreeMap<String, Vec<LoopMark>> = BTreeMap::new();
    for c in solved.resolved.loops() {
        for s in &c.path {
            let name = label(*s);
            if !name.is_empty() {
                loops.entry(name).or_default().push(LoopMark {
                    percent: c.percent,
                    error: c.distance,
                });
            }
        }
    }
    let mut names: BTreeSet<String> = BTreeSet::new();
    names.extend(degree.keys().cloned());
    names.extend(notes.keys().cloned());
    names.extend(anchors.iter().cloned());

    let mut out = Vec::new();
    for name in names {
        let ns = notes.get(&name).cloned().unwrap_or_default();
        let lead_notes: Vec<Note> = ns
            .iter()
            .filter(|n| has_word(&n.text, keyword))
            .cloned()
            .collect();
        let d = degree.get(&name).copied().unwrap_or(0);
        let mut kinds: Vec<&'static str> = Vec::new();
        if !lead_notes.is_empty() {
            kinds.push("lead");
        }
        if d == 1 && !anchors.contains(&name) && !closed.contains(&name) {
            kinds.push("openEnd");
        }
        if d >= 3 {
            kinds.push("junction");
        }
        if anchors.contains(&name) {
            kinds.push("control");
        }
        if loops.contains_key(&name) {
            kinds.push("loop");
        }
        if !ns.is_empty() {
            kinds.push("noted");
        }
        if flags.contains_key(&name) {
            kinds.push("flagged");
        }
        let id = ids.get(&name).copied();
        let trips_here: Vec<usize> = seen_trips
            .get(&name)
            .map(|s| s.iter().copied().collect())
            .unwrap_or_default();
        let z = id
            .and_then(|i| solved.positions.get(&i))
            .map(|p| p.z)
            .filter(|z| z.is_finite());
        let (status, team, who) = id
            .and_then(|i| doc.entity(i))
            .map(|e| {
                (
                    e.text(Field::StationStatus),
                    e.text(Field::StationTeamNote),
                    e.text(Field::StationAssigned),
                )
            })
            .unwrap_or_default();
        // a lead somebody surveyed on from after it was written is probably pushed
        let suggest = match lead_notes.iter().map(|n| n.trip).min() {
            Some(first) if trips_here.iter().any(|t| *t > first) => "pushed",
            _ => "",
        };
        out.push(StationRow {
            id: id.map(|i| i.to_string()).unwrap_or_default(),
            station: name.clone(),
            kinds,
            trips: trips_here,
            degree: d,
            note_text: ns
                .iter()
                .map(|n| n.text.as_str())
                .collect::<Vec<_>>()
                .join(" | "),
            notes: ns,
            lead_notes,
            z,
            loops: loops.get(&name).cloned().unwrap_or_default(),
            flags: flags.get(&name).cloned().unwrap_or_default(),
            status,
            team,
            who,
            suggest,
        });
    }
    out.sort_by(|a, b| compare_natural(&a.station, &b.station));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lead_is_a_whole_word_in_any_case() {
        for yes in ["LEAD W", "Lead?", "a lead", "lead", "go left (lead)"] {
            assert!(has_word(yes, "lead"), "{yes}");
        }
        for no in ["leader", "leads", "misled", "", "pleading"] {
            assert!(!has_word(no, "lead"), "{no}");
        }
    }

    #[test]
    fn eos_closes_a_station_only_as_a_word() {
        assert!(
            says_closed("EOS, too tight to follow")
                && says_closed("end of survey")
                && says_closed("E.O.S.")
                && says_closed("x eos")
        );
        assert!(!says_closed("EOSTER") && !says_closed("neos") && !says_closed(""));
    }

    #[test]
    fn stations_sort_the_way_a_caver_reads_them() {
        let mut v = vec!["A10", "A2", "B1", "A1", "A100", "A20"];
        v.sort_by(|a, b| compare_natural(a, b));
        assert_eq!(v, ["A1", "A2", "A10", "A20", "A100", "B1"]);
    }
}
