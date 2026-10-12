//! Which run each run hangs off, and the station it ties in at.
//!
//! Ported from the old `CsProfile.hierarchy` (its comments explain every choice). The graph decides,
//! not the name: a run's tie is the station on its PARENT's side of the surveyed leg that placed it.
//! Phase 1 uses only legs that placed a new station (the earlier-placed end is the parent) and
//! breaks any cycle among them; phase 2 lets a run still without a parent use closure and tie legs,
//! skipping candidates that already descend from it (an ordinary loop). Second ties, mismatch
//! reports and band order are not ported: nothing here needs them yet.

use crate::runs::{LegInfo, Runs};
use cave_id::Id;
use cave_survey::LegKind;
use std::collections::{HashMap, HashSet};

/// Parent run and tie station for every run, keyed by run key.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hierarchy {
    /// The run each run hangs off (`None` for a root).
    pub parents: HashMap<String, Option<String>>,
    /// The station on the parent's side of the tie (`None` for a root).
    pub ties: HashMap<String, Option<Id>>,
}

struct Contact {
    station: Id,
    other_run: String,
}

/// Works out every run's parent and tie. `legs` are the surveyed legs in resolution order (no
/// splays); `seq` is each station's resolution order.
pub fn hierarchy(runs: &Runs, legs: &[LegInfo], seq: &HashMap<Id, usize>) -> Hierarchy {
    let seq_of = |s: Id| seq.get(&s).copied().unwrap_or(usize::MAX);
    let run_of: HashMap<Id, &str> = runs
        .runs
        .iter()
        .flat_map(|r| r.stations.iter().map(move |s| (*s, r.key.as_str())))
        .collect();

    // Contacts of each run with another run, in leg order: rank 0 from legs that placed a
    // station (other end placed earlier), rank 1 from closures and ties.
    let mut rank0: HashMap<&str, Vec<Contact>> = HashMap::new();
    let mut rank1: HashMap<&str, Vec<Contact>> = HashMap::new();
    for run in &runs.runs {
        let key = run.key.as_str();
        let mine: HashSet<Id> = run.stations.iter().copied().collect();
        let (mut r0, mut r1) = (Vec::new(), Vec::new());
        for l in legs {
            let (m, o) = if mine.contains(&l.from) {
                (l.from, l.to)
            } else if mine.contains(&l.to) {
                (l.to, l.from)
            } else {
                continue;
            };
            let Some(&other_run) = run_of.get(&o) else {
                continue;
            };
            if other_run == key {
                continue;
            }
            let contact = Contact {
                station: o,
                other_run: other_run.to_string(),
            };
            if l.kind == LegKind::New {
                if seq_of(o) >= seq_of(m) {
                    continue;
                }
                r0.push(contact);
            } else {
                r1.push(contact);
            }
        }
        rank0.insert(key, r0);
        rank1.insert(key, r1);
    }

    // Phase 1.
    let mut parents: HashMap<String, Option<String>> = HashMap::new();
    let mut ties: HashMap<String, Option<Id>> = HashMap::new();
    for run in &runs.runs {
        match rank0[run.key.as_str()].first() {
            Some(c) => {
                parents.insert(run.key.clone(), Some(c.other_run.clone()));
                ties.insert(run.key.clone(), Some(c.station));
            }
            None => {
                parents.insert(run.key.clone(), None);
                ties.insert(run.key.clone(), None);
            }
        }
    }

    // A cycle among phase-1 parents is real; the run that began earliest becomes the root.
    let earliest: HashMap<&str, usize> = runs
        .runs
        .iter()
        .map(|r| {
            (
                r.key.as_str(),
                r.stations
                    .iter()
                    .map(|s| seq_of(*s))
                    .min()
                    .unwrap_or(usize::MAX),
            )
        })
        .collect();
    let mut settled: HashSet<String> = HashSet::new();
    for start in &runs.runs {
        if settled.contains(&start.key) {
            continue;
        }
        let mut path: Vec<String> = Vec::new();
        let mut at: HashMap<String, usize> = HashMap::new();
        let mut cur = start.key.clone();
        loop {
            if settled.contains(&cur) {
                break;
            }
            if let Some(&i) = at.get(&cur) {
                let members = &path[i..];
                let mut root = &members[0];
                for m in &members[1..] {
                    if earliest[m.as_str()] < earliest[root.as_str()] {
                        root = m;
                    }
                }
                parents.insert(root.clone(), None);
                ties.insert(root.clone(), None);
                break;
            }
            at.insert(cur.clone(), path.len());
            path.push(cur.clone());
            match parents.get(&cur).cloned().flatten() {
                Some(p) if runs.runs.iter().any(|r| r.key == p) => cur = p,
                _ => break,
            }
        }
        settled.extend(path);
    }

    // Phase 2, last run first: a run still parentless takes its first closure or tie contact that
    // does not descend from it.
    let descends = |parents: &HashMap<String, Option<String>>, ancestor: &str, candidate: &str| {
        let mut cur = Some(candidate.to_string());
        for _ in 0..=runs.runs.len() {
            let Some(c) = cur else { return false };
            if c == ancestor {
                return true;
            }
            cur = parents.get(&c).cloned().flatten();
        }
        false
    };
    for run in runs.runs.iter().rev() {
        if parents[&run.key].is_some() {
            continue;
        }
        let chosen = rank1[run.key.as_str()]
            .iter()
            .find(|c| !descends(&parents, &run.key, &c.other_run));
        if let Some(c) = chosen {
            parents.insert(run.key.clone(), Some(c.other_run.clone()));
            ties.insert(run.key.clone(), Some(c.station));
        }
    }
    Hierarchy { parents, ties }
}
