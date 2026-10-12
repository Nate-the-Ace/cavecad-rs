//! Grouping stations into runs, the walkable-leg graph, and the longest chain through a run.

use crate::names::{seq_num_of, seq_order, split_name};
use cave_id::Id;
use cave_survey::LegKind;
use std::collections::HashMap;

/// A leg as the profile sees it: which stations it joins, its kind, and its measured plan length.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegInfo {
    /// The shot.
    pub shot: Id,
    /// Start station.
    pub from: Id,
    /// End station.
    pub to: Id,
    /// How resolution used the leg.
    pub kind: LegKind,
    /// The shot's own horizontal length in metres (not the placed, adjusted distance).
    pub plan: f64,
}

/// One survey run: its key and its stations in sequence order.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    /// The run key (the station name without its trailing group).
    pub key: String,
    /// Member stations, origin first, then by sequence.
    pub stations: Vec<Id>,
}

/// All runs of a survey.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Runs {
    /// Runs in first-appearance (resolution) order.
    pub runs: Vec<Run>,
    /// Stations whose names are not station names (empty, or splays).
    pub ungrouped: Vec<Id>,
}

/// Groups stations into runs by name. `order` is the stations in resolution order.
pub fn group_runs(labels: &HashMap<Id, String>, order: &[Id]) -> Runs {
    let mut out = Runs::default();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut members: Vec<Vec<(String, Id)>> = Vec::new();
    for &id in order {
        let Some(split) = labels.get(&id).and_then(|l| split_name(l)) else {
            out.ungrouped.push(id);
            continue;
        };
        let i = *index.entry(split.base.clone()).or_insert_with(|| {
            out.runs.push(Run {
                key: split.base.clone(),
                stations: Vec::new(),
            });
            members.push(Vec::new());
            out.runs.len() - 1
        });
        members[i].push((split.seq, id));
    }
    for (run, mut m) in out.runs.iter_mut().zip(members) {
        m.sort_by(|a, b| seq_order(&a.0, &b.0));
        run.stations = m.into_iter().map(|(_, id)| id).collect();
    }
    out
}

/// The legs between stations, indexed two ways: walkable neighbours (closures out) and by pair.
pub struct Graph {
    legs: Vec<LegInfo>,
    adjacency: HashMap<Id, Vec<Id>>,
    pairs: HashMap<(Id, Id), Vec<usize>>,
}

fn pair(a: Id, b: Id) -> (Id, Id) {
    if a <= b { (a, b) } else { (b, a) }
}

impl Graph {
    /// Builds the graph. The two lookups below differ ONLY in whether a closure is allowed: the
    /// chain search and `leg_between` must agree (no closures), while the single tie step per band
    /// uses `tie_leg_between`, which admits a closure on purpose.
    pub fn new(legs: &[LegInfo]) -> Self {
        let mut adjacency: HashMap<Id, Vec<Id>> = HashMap::new();
        let mut pairs: HashMap<(Id, Id), Vec<usize>> = HashMap::new();
        for (i, l) in legs.iter().enumerate() {
            pairs.entry(pair(l.from, l.to)).or_default().push(i);
            if l.kind != LegKind::Closure {
                adjacency.entry(l.from).or_default().push(l.to);
                adjacency.entry(l.to).or_default().push(l.from);
            }
        }
        Graph {
            legs: legs.to_vec(),
            adjacency,
            pairs,
        }
    }

    fn neighbours(&self, s: Id) -> &[Id] {
        self.adjacency.get(&s).map_or(&[], Vec::as_slice)
    }

    /// The walked leg joining two stations (never a closure).
    pub fn leg_between(&self, a: Id, b: Id) -> Option<&LegInfo> {
        self.pairs
            .get(&pair(a, b))?
            .iter()
            .map(|&i| &self.legs[i])
            .find(|l| l.kind != LegKind::Closure)
    }

    /// The leg joining a run to its parent: a closure is admitted here (and only here).
    pub fn tie_leg_between(&self, a: Id, b: Id) -> Option<&LegInfo> {
        self.pairs.get(&pair(a, b))?.first().map(|&i| &self.legs[i])
    }
}

/// True if `path` is a better chain than `best`: longer wins; then lower first and last sequence
/// numbers; then plain text order, so the answer is the same in every engine.
pub fn better_chain(path: &[Id], best: &[Id], labels: &HashMap<Id, String>) -> bool {
    if path.len() != best.len() {
        return path.len() > best.len();
    }
    if path.is_empty() {
        return false;
    }
    let label = |s: &Id| labels.get(s).cloned().unwrap_or_default();
    let stat = |list: &[Id]| {
        list.iter()
            .map(|s| seq_num_of(&label(s)))
            .fold((f64::MAX, -1.0_f64), |(lo, hi), v| (lo.min(v), hi.max(v)))
    };
    let (a, b) = (stat(path), stat(best));
    if a.0 != b.0 {
        return a.0 < b.0;
    }
    if a.1 != b.1 {
        return a.1 < b.1;
    }
    let join = |l: &[Id]| l.iter().map(label).collect::<Vec<_>>().join(" ");
    join(path) < join(best)
}

/// The longest chain of a run and the stations left off it.
#[derive(Clone, Debug, PartialEq)]
pub struct Chain {
    /// Stations in walking order, oriented so the lower sequence comes first.
    pub chain: Vec<Id>,
    /// Run stations the chain does not pass through (side branches).
    pub omitted: Vec<Id>,
}

/// Most steps the search will take before keeping the best chain found so far. The search is
/// exhaustive (longest simple path), so a tangled run needs a stop.
const MAX_STEPS: u64 = 5_000_000;

/// The longest walkable chain through a run's stations, port of `CsProfile.longestChain`.
pub fn longest_chain(run: &Run, graph: &Graph, labels: &HashMap<Id, String>) -> Chain {
    let in_run: std::collections::HashSet<Id> = run.stations.iter().copied().collect();
    let degree = |s: Id| {
        graph
            .neighbours(s)
            .iter()
            .filter(|o| in_run.contains(o))
            .count()
    };
    let mut starts: Vec<Id> = run
        .stations
        .iter()
        .copied()
        .filter(|s| degree(*s) <= 1)
        .collect();
    if starts.is_empty() {
        starts = run.stations.clone();
    }

    struct Search<'a> {
        graph: &'a Graph,
        labels: &'a HashMap<Id, String>,
        in_run: &'a std::collections::HashSet<Id>,
        best: Vec<Id>,
        steps: u64,
    }
    impl Search<'_> {
        fn walk(
            &mut self,
            at: Id,
            visited: &mut std::collections::HashSet<Id>,
            path: &mut Vec<Id>,
        ) {
            self.steps += 1;
            if self.steps > MAX_STEPS {
                return;
            }
            path.push(at);
            if better_chain(path, &self.best, self.labels) {
                self.best = path.clone();
            }
            for &next in self.graph.neighbours(at) {
                if !self.in_run.contains(&next) || visited.contains(&next) {
                    continue;
                }
                visited.insert(next);
                self.walk(next, visited, path);
                visited.remove(&next);
            }
            path.pop();
        }
    }
    let mut search = Search {
        graph,
        labels,
        in_run: &in_run,
        best: Vec::new(),
        steps: 0,
    };
    for s in starts {
        let mut visited = std::collections::HashSet::from([s]);
        search.walk(s, &mut visited, &mut Vec::new());
    }
    let mut chain = search.best;
    let on: std::collections::HashSet<Id> = chain.iter().copied().collect();
    let omitted = run
        .stations
        .iter()
        .copied()
        .filter(|s| !on.contains(s))
        .collect();
    if chain.len() >= 2 {
        let name = |s: &Id| labels.get(s).and_then(|l| split_name(l));
        if let (Some(a), Some(b)) = (name(&chain[0]), name(&chain[chain.len() - 1]))
            && seq_order(&a.seq, &b.seq).is_gt()
        {
            chain.reverse();
        }
    }
    Chain { chain, omitted }
}
