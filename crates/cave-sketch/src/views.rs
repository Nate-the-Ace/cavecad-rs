use crate::Pt;
use crate::layout::Layout;
use crate::source::DimsByStation;
use serde::Serialize;

/// Heading (unit east, north) of the passage at chain position `i`: the bisector of the legs either side.
fn heading(l: &Layout, i: usize) -> Pt {
    let c = &l.chain;
    let a = l
        .station(&c[i.saturating_sub(1)])
        .expect("the chain is made of placed stations");
    let b = l
        .station(&c[(i + 1).min(c.len() - 1)])
        .expect("the chain is made of placed stations");
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let n = dx.hypot(dy);
    if n < 1e-9 {
        [0.0, 1.0]
    } else {
        [dx / n, dy / n]
    }
}

/// A station by label in a view's own metres.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Mark {
    /// Its label.
    pub label: String,
    /// Where it is.
    pub p: Pt,
}

/// Splits `0..n` into runs of consecutive entries where `f` gives a value.
fn runs<R>(n: usize, mut f: impl FnMut(usize) -> Option<R>) -> Vec<Vec<R>> {
    let mut out = Vec::new();
    let mut cur = Vec::new();
    for i in 0..n {
        match f(i) {
            None => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            Some(v) => cur.push(v),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// What the plan draws, in metres (y up).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlanView {
    /// Every placed station.
    pub stations: Vec<Mark>,
    /// The legs.
    pub legs: Vec<[Pt; 2]>,
    /// The splays.
    pub splays: Vec<[Pt; 2]>,
    /// Wall lines (left and right of travel), each a run of points.
    pub walls: Vec<Vec<Pt>>,
    /// The passage between the walls, as polygons.
    pub passage: Vec<Vec<Pt>>,
}

/// Flushes a run of paired edges into a polygon when it has more than one pair.
fn polygon(run: &mut Vec<(Pt, Pt)>, out: &mut Vec<Vec<Pt>>) {
    if run.len() > 1 {
        let mut poly: Vec<Pt> = run.iter().map(|r| r.0).collect();
        poly.extend(run.iter().rev().map(|r| r.1));
        out.push(poly);
    }
    run.clear();
}

/// The plan: legs, splays, and the walls and passage wherever the chain has left and right dimensions.
pub fn plan_view(l: &Layout, dims: &DimsByStation) -> PlanView {
    let p = |label: &str| -> Pt {
        let s = l.station(label).expect("a placed station");
        [s.x, s.y]
    };
    let side = |i: usize, left: bool| -> Option<Pt> {
        let d = dims
            .get(&l.chain[i])
            .and_then(|d| if left { d.left } else { d.right })?;
        let [hx, hy] = heading(l, i);
        let [nx, ny] = if left { [-hy, hx] } else { [hy, -hx] };
        let at = p(&l.chain[i]);
        Some([at[0] + nx * d, at[1] + ny * d])
    };
    let n = l.chain.len();
    let lefts = runs(n, |i| side(i, true));
    let rights = runs(n, |i| side(i, false));
    // A passage polygon where a left run and a right run cover the same stations.
    let mut passage = Vec::new();
    let mut run = Vec::new();
    for i in 0..n {
        match (side(i, true), side(i, false)) {
            (Some(a), Some(b)) => run.push((a, b)),
            _ => polygon(&mut run, &mut passage),
        }
    }
    polygon(&mut run, &mut passage);
    PlanView {
        stations: l
            .stations
            .iter()
            .map(|s| Mark {
                label: s.label.clone(),
                p: [s.x, s.y],
            })
            .collect(),
        legs: l.legs.iter().map(|g| [p(&g.from), p(&g.to)]).collect(),
        splays: l
            .splays
            .iter()
            .map(|s| [p(&s.from), [s.end[0], s.end[1]]])
            .collect(),
        walls: lefts
            .into_iter()
            .chain(rights)
            .filter(|r| r.len() > 1)
            .collect(),
        passage,
    }
}

/// What the profile draws: the main chain unrolled, distance along it against elevation.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProfileView {
    /// Stations on the main chain: x is distance along it, y elevation.
    pub stations: Vec<Mark>,
    /// The legs.
    pub legs: Vec<[Pt; 2]>,
    /// Floor and ceiling lines.
    pub walls: Vec<Vec<Pt>>,
    /// The passage between floor and ceiling.
    pub passage: Vec<Vec<Pt>>,
}

/// The profile.
pub fn profile_view(l: &Layout, dims: &DimsByStation) -> ProfileView {
    let mut s = 0.0;
    let mut along: Vec<Pt> = Vec::new();
    for (i, label) in l.chain.iter().enumerate() {
        let b = l.station(label).expect("a placed station");
        if i > 0 {
            let a = l.station(&l.chain[i - 1]).expect("a placed station");
            s += (b.x - a.x).hypot(b.y - a.y);
        }
        along.push([s, b.z]);
    }
    let at = |i: usize, up: bool| -> Option<Pt> {
        let d = dims
            .get(&l.chain[i])
            .and_then(|d| if up { d.up } else { d.down })?;
        Some([along[i][0], along[i][1] + if up { d } else { -d }])
    };
    let n = l.chain.len();
    let ceilings = runs(n, |i| at(i, true));
    let floors = runs(n, |i| at(i, false));
    let mut passage = Vec::new();
    let mut run = Vec::new();
    for i in 0..n {
        match (at(i, true), at(i, false)) {
            (Some(u), Some(d)) => run.push((u, d)),
            _ => polygon(&mut run, &mut passage),
        }
    }
    polygon(&mut run, &mut passage);
    ProfileView {
        stations: l
            .chain
            .iter()
            .zip(&along)
            .map(|(label, p)| Mark {
                label: label.clone(),
                p: *p,
            })
            .collect(),
        legs: along.windows(2).map(|w| [w[0], w[1]]).collect(),
        walls: ceilings
            .into_iter()
            .chain(floors)
            .filter(|r| r.len() > 1)
            .collect(),
        passage,
    }
}

/// What the cross section draws at one station.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SectionView {
    /// The station.
    pub station: String,
    /// The passage outline from the dimensions: left, up, right, down. `None` with no dimensions at all.
    pub outline: Option<Vec<Pt>>,
    /// The ends of the splays taken at this station, across and up.
    pub splays: Vec<Pt>,
}

/// The cross section at `station`, looking along the passage; `None` when the station is not placed.
pub fn section_view(l: &Layout, dims: &DimsByStation, station: &str) -> Option<SectionView> {
    let st = l.station(station)?;
    // The passage direction here: along the chain, or the leg that arrives, or north.
    let mut h: Pt = [0.0, 1.0];
    if let Some(i) = l.chain.iter().position(|c| c == station) {
        h = heading(l, i);
    } else if let Some(arr) = l
        .legs
        .iter()
        .find(|g| g.to == station)
        .or_else(|| l.legs.iter().find(|g| g.from == station))
    {
        let (a, b) = (l.station(&arr.from)?, l.station(&arr.to)?);
        let n = (b.x - a.x).hypot(b.y - a.y);
        if n > 1e-9 {
            h = [(b.x - a.x) / n, (b.y - a.y) / n];
        }
    }
    let right = [h[1], -h[0]];
    let v = |x: Option<f64>| x.unwrap_or(0.0);
    let outline = dims
        .get(station)
        .filter(|d| [d.left, d.right, d.up, d.down].iter().any(|x| x.is_some()))
        .map(|d| {
            vec![
                [-v(d.left), 0.0],
                [0.0, v(d.up)],
                [v(d.right), 0.0],
                [0.0, -v(d.down)],
            ]
        });
    Some(SectionView {
        station: station.to_string(),
        outline,
        splays: l
            .splays
            .iter()
            .filter(|s| s.from == station)
            .map(|s| {
                [
                    (s.end[0] - st.x) * right[0] + (s.end[1] - st.y) * right[1],
                    s.end[2] - st.z,
                ]
            })
            .collect(),
    })
}
