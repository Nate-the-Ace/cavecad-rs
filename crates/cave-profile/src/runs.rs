//! Grouping stations into runs, the walkable-leg graph, and the longest chain through a run.

use crate::names::{is_number_name, member_seq, seq_num_of, seq_order, split_name};
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

/// Groups stations into runs by name, as [`group_runs`], except that stations named with a number
/// only join up along `legs`: a leg between two consecutive numbers (`7`-`8`, either way round,
/// never a closure) puts both in one run, so `0, 1, 2, ... 61` is one run and a branch numbered
/// on from elsewhere (`9`-`300`, `300`, `301`, ...) is another, tying in where it leaves. A run
/// of numbers is keyed by its lowest-numbered station's name. A number station with no
/// consecutive neighbour is a run of its own, as before.
pub fn group_runs_linked(labels: &HashMap<Id, String>, order: &[Id], legs: &[LegInfo]) -> Runs {
    let number = |id: &Id| {
        labels
            .get(id)
            .filter(|l| is_number_name(l))
            .and_then(|l| l.parse::<u128>().ok())
    };
    // union-find over number stations joined by consecutive-number legs
    let mut parent: HashMap<Id, Id> = HashMap::new();
    fn root(parent: &mut HashMap<Id, Id>, x: Id) -> Id {
        let mut r = x;
        while let Some(&p) = parent.get(&r) {
            if p == r {
                break;
            }
            r = p;
        }
        // flatten the path walked
        let mut c = x;
        while let Some(&p) = parent.get(&c) {
            if p == r {
                break;
            }
            parent.insert(c, r);
            c = p;
        }
        r
    }
    for l in legs {
        if l.kind == LegKind::Closure || l.from == l.to {
            continue;
        }
        let (Some(a), Some(b)) = (number(&l.from), number(&l.to)) else {
            continue;
        };
        if a.abs_diff(b) != 1 {
            continue;
        }
        parent.entry(l.from).or_insert(l.from);
        parent.entry(l.to).or_insert(l.to);
        let (ra, rb) = (root(&mut parent, l.from), root(&mut parent, l.to));
        if ra != rb {
            parent.insert(ra, rb);
        }
    }
    // each joined-up group's key: its lowest number's name
    let mut key_of_root: HashMap<Id, (u128, String)> = HashMap::new();
    let members: Vec<Id> = parent.keys().copied().collect();
    for id in members {
        let r = root(&mut parent, id);
        let (n, name) = (number(&id).unwrap_or(u128::MAX), labels[&id].clone());
        let e = key_of_root.entry(r).or_insert((n, name.clone()));
        if n < e.0 {
            *e = (n, name);
        }
    }
    let key_of = |id: &Id, parent: &mut HashMap<Id, Id>| -> Option<String> {
        parent.contains_key(id).then(|| {
            let r = root(parent, *id);
            key_of_root[&r].1.clone()
        })
    };

    let mut out = Runs::default();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut members: Vec<Vec<(String, Id)>> = Vec::new();
    for &id in order {
        let Some(label) = labels.get(&id) else {
            out.ungrouped.push(id);
            continue;
        };
        let (key, seq) = match key_of(&id, &mut parent) {
            Some(k) => (k, label.clone()),
            None => match split_name(label) {
                Some(split) => (split.base, split.seq),
                None => {
                    out.ungrouped.push(id);
                    continue;
                }
            },
        };
        let i = *index.entry(key.clone()).or_insert_with(|| {
            out.runs.push(Run {
                key,
                stations: Vec::new(),
            });
            members.push(Vec::new());
            out.runs.len() - 1
        });
        members[i].push((seq, id));
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
        let seq = |s: &Id| labels.get(s).and_then(|l| member_seq(l));
        if let (Some(a), Some(b)) = (seq(&chain[0]), seq(&chain[chain.len() - 1]))
            && seq_order(&a, &b).is_gt()
        {
            chain.reverse();
        }
    }
    Chain { chain, omitted }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cave_id::{IdSource, SeqIds};

    /// Stations named `names`, and legs between them by name.
    fn cave(names: &[&str], legs: &[(&str, &str, LegKind)]) -> (Runs, HashMap<String, Id>) {
        let mut ids = SeqIds::new(7);
        let by: HashMap<String, Id> = names
            .iter()
            .map(|n| (n.to_string(), ids.next_id()))
            .collect();
        let labels: HashMap<Id, String> = by.iter().map(|(n, id)| (*id, n.clone())).collect();
        let order: Vec<Id> = names.iter().map(|n| by[*n]).collect();
        let legs: Vec<LegInfo> = legs
            .iter()
            .map(|(a, b, kind)| LegInfo {
                shot: ids.next_id(),
                from: by[*a],
                to: by[*b],
                kind: *kind,
                plan: 1.0,
            })
            .collect();
        (group_runs_linked(&labels, &order, &legs), by)
    }

    fn keys(r: &Runs) -> Vec<&str> {
        r.runs.iter().map(|r| r.key.as_str()).collect()
    }

    #[test]
    fn numbered_stations_on_one_line_are_one_run() {
        use LegKind::New;
        let (r, by) = cave(
            &["0", "1", "2", "3", "300", "301", "302"],
            &[
                ("0", "1", New),
                ("1", "2", New),
                ("2", "3", New),
                ("2", "300", New),
                ("300", "301", New),
                ("301", "302", New),
            ],
        );
        assert_eq!(keys(&r), ["0", "300"]);
        assert_eq!(r.runs[0].stations, ["0", "1", "2", "3"].map(|n| by[n]));
        assert_eq!(r.runs[1].stations, ["300", "301", "302"].map(|n| by[n]));
    }

    #[test]
    fn a_closure_between_consecutive_numbers_does_not_join_runs() {
        use LegKind::{Closure, New};
        let (r, _) = cave(
            &["1", "2", "5", "6"],
            &[
                ("1", "2", New),
                ("5", "6", New),
                ("2", "5", New),
                ("6", "5", Closure),
            ],
        );
        assert_eq!(keys(&r), ["1", "5"]);
    }

    #[test]
    fn a_survey_taken_backwards_still_runs_in_number_order() {
        use LegKind::New;
        let (r, by) = cave(&["12", "11", "10"], &[("12", "11", New), ("11", "10", New)]);
        assert_eq!(keys(&r), ["10"]);
        assert_eq!(r.runs[0].stations, ["10", "11", "12"].map(|n| by[n]));
    }

    #[test]
    fn lettered_names_are_grouped_as_before() {
        use LegKind::New;
        let (r, _) = cave(
            &["A1", "A2", "A2a1", "7"],
            &[("A1", "A2", New), ("A2", "A2a1", New), ("A2", "7", New)],
        );
        assert_eq!(keys(&r), ["A", "A2a", "7"]);
    }
}
