use crate::source::SurveyShot;
use serde::Serialize;
use std::collections::HashMap;

/// A station placed in metres: east, north, up from the first station.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Station {
    /// Its label.
    pub label: String,
    /// East.
    pub x: f64,
    /// North.
    pub y: f64,
    /// Up.
    pub z: f64,
}

/// A splay: where its far end lands.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Splay {
    /// The shot.
    pub id: String,
    /// The station it starts at.
    pub from: String,
    /// East, north, up of its far end.
    pub end: [f64; 3],
}

/// A leg between two placed stations.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Leg {
    /// The shot.
    pub id: String,
    /// Where it starts.
    pub from: String,
    /// Where it ends.
    pub to: String,
}

/// The survey placed: stations by leg, loops drawn as they close (no adjustment here).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Layout {
    /// Every station reached from the first, in the order they were placed.
    pub stations: Vec<Station>,
    /// The legs between placed stations.
    pub legs: Vec<Leg>,
    /// The splays with a placed start.
    pub splays: Vec<Splay>,
    /// The longest run of stations from the first, the spine of the profile.
    pub chain: Vec<String>,
    #[serde(skip)]
    index: HashMap<String, usize>,
}

impl Layout {
    /// The station with this label.
    pub fn station(&self, label: &str) -> Option<&Station> {
        self.index.get(label).map(|i| &self.stations[*i])
    }
}

const PLUMB_DEG: f64 = 85.0;

fn mod360(a: f64) -> f64 {
    ((a % 360.0) + 360.0) % 360.0
}

/// A shot as a vector (east, north, up), combining foresight and backsight when both are there. `None` when it
/// cannot be placed (no tape, no clino, or no compass on a shot that is not a pitch).
pub fn shot_vector(s: &SurveyShot) -> Option<[f64; 3]> {
    let distance = s.distance.filter(|d| *d > 0.0)?;
    let inc0 = s.inc?;
    let inc = s.inc_back.map_or(inc0, |b| (inc0 - b) / 2.0);
    let mut az = s.az;
    if let (Some(f), Some(b)) = (az, s.az_back) {
        // Circular mean of the foresight and the backsight turned round.
        let (a, b) = (f.to_radians(), (b + 180.0).to_radians());
        az = Some(mod360(
            (a.sin() + b.sin()).atan2(a.cos() + b.cos()).to_degrees(),
        ));
    }
    let az = match az {
        Some(a) => a,
        None if inc.abs() < PLUMB_DEG => return None, // only a pitch has no bearing
        None => 0.0,
    };
    let h = inc.to_radians().cos() * distance;
    Some([
        h * az.to_radians().sin(),
        h * az.to_radians().cos(),
        inc.to_radians().sin() * distance,
    ])
}

/// Places every station reachable from the first, leg by leg. Loops are drawn as they close (no adjustment here).
pub fn layout(shots: &[SurveyShot]) -> Layout {
    let mut out = Layout::default();
    let legs: Vec<(&SurveyShot, Option<[f64; 3]>)> = shots
        .iter()
        .filter(|s| s.to.is_some())
        .map(|s| (s, shot_vector(s)))
        .collect();
    let Some(first) = legs
        .first()
        .map(|l| l.0.from.clone())
        .or_else(|| shots.first().map(|s| s.from.clone()))
    else {
        return out;
    };
    let put = |out: &mut Layout, label: &str, x: f64, y: f64, z: f64| {
        out.index.insert(label.to_string(), out.stations.len());
        out.stations.push(Station {
            label: label.to_string(),
            x,
            y,
            z,
        });
    };
    put(&mut out, &first, 0.0, 0.0, 0.0);
    let mut pending: Vec<(&SurveyShot, [f64; 3])> = legs
        .iter()
        .filter_map(|(s, v)| v.map(|v| (*s, v)))
        .collect();
    let mut moved = true;
    while moved && !pending.is_empty() {
        moved = false;
        for i in (0..pending.len()).rev() {
            let (s, v) = pending[i];
            let to = s.to.as_deref().unwrap_or_default();
            let (a, b) = (out.station(&s.from).cloned(), out.station(to).cloned());
            match (a, b) {
                (Some(a), None) => put(&mut out, to, a.x + v[0], a.y + v[1], a.z + v[2]),
                (None, Some(b)) => put(&mut out, &s.from, b.x - v[0], b.y - v[1], b.z - v[2]),
                (None, None) => continue,
                (Some(_), Some(_)) => {}
            }
            pending.remove(i);
            moved = true;
        }
    }
    out.legs = legs
        .iter()
        .filter(|(s, _)| {
            out.station(&s.from).is_some()
                && out.station(s.to.as_deref().unwrap_or_default()).is_some()
        })
        .map(|(s, _)| Leg {
            id: s.id.clone(),
            from: s.from.clone(),
            to: s.to.clone().unwrap_or_default(),
        })
        .collect();
    for s in shots.iter().filter(|s| s.to.is_none()) {
        if let (Some(v), Some(a)) = (shot_vector(s), out.station(&s.from)) {
            out.splays.push(Splay {
                id: s.id.clone(),
                from: s.from.clone(),
                end: [a.x + v[0], a.y + v[1], a.z + v[2]],
            });
        }
    }
    out.chain = main_chain(&first, &out);
    out
}

/// The longest run of legs from `start` (by length), following the survey's branches.
fn main_chain(start: &str, l: &Layout) -> Vec<String> {
    let mut next: Vec<Vec<usize>> = vec![Vec::new(); l.stations.len()];
    for g in &l.legs {
        let (a, b) = (l.index[&g.from], l.index[&g.to]);
        next[a].push(b);
        next[b].push(a);
    }
    let len = |a: usize, b: usize| {
        let (p, q) = (&l.stations[a], &l.stations[b]);
        ((p.x - q.x).powi(2) + (p.y - q.y).powi(2) + (p.z - q.z).powi(2)).sqrt()
    };
    struct Walk<'a> {
        next: &'a [Vec<usize>],
        len: &'a dyn Fn(usize, usize) -> f64,
        best: (Vec<usize>, f64),
    }
    fn walk(w: &mut Walk, at: usize, path: &mut Vec<usize>, length: f64, seen: &mut Vec<bool>) {
        if length > w.best.1 {
            w.best = (path.clone(), length);
        }
        for &n in &w.next[at] {
            if seen[n] {
                continue;
            }
            seen[n] = true;
            path.push(n);
            let step = (w.len)(at, n);
            walk(w, n, path, length + step, seen);
            path.pop();
            seen[n] = false;
        }
    }
    let s = l.index[start];
    let mut w = Walk {
        next: &next,
        len: &len,
        best: (vec![s], 0.0),
    };
    let mut seen = vec![false; l.stations.len()];
    seen[s] = true;
    walk(&mut w, s, &mut vec![s], 0.0, &mut seen);
    w.best
        .0
        .iter()
        .map(|i| l.stations[*i].label.clone())
        .collect()
}
