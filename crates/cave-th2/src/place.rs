//! Landing a scrap on the survey.
//!
//! A scrap is drawn in the sketcher's own coordinates (a phone screen, a page), tied to the cave
//! only by the station markers dropped on it. How far those ties go:
//!
//! - three or more not in a line: an affine fit (stretch and skew taken out), then bent so every
//!   marker lands on its station and the ink between follows (a thin-plate spline). The survey is
//!   adjusted, so stations have moved relative to each other since the page was drawn, and a rigid
//!   fit would leave walls drifting off the stations they were drawn against.
//! - two: turned, moved and resized only (the shape is kept).
//! - one: moved only, at the scrap's own declared scale, north up the page.
//! - none: not placed. Putting it anywhere would be a guess that looks like a measurement.
//!
//! The residual reported is from before the bend: after it every marker is on its station by
//! construction, which says nothing.

use cave_formats::th2::{Scale, Scrap, Seg, StationTie};
use cave_scans::{FitMode, Tps, fit, rubber};
use std::collections::HashMap;

/// How a scrap was placed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Bent through three or more ties.
    Warped,
    /// Kept its shape, fitted to two ties (or more, all in a line).
    Similar,
    /// Moved onto its one tie at its declared scale.
    Moved,
}

/// A scrap's placement: sketch coordinates (y up) to the map's metres.
pub struct Placement {
    /// How.
    pub kind: Kind,
    /// `x = m0 sx + m1 sy + m2`, `y = m3 sx + m4 sy + m5`.
    m: [f64; 6],
    bend: Option<Tps>,
    /// Root mean square distance, metres, of the ties from their stations before any bend.
    pub residual: f64,
    /// Ties that found their station.
    pub used: usize,
    /// Tie names the survey has no station for.
    pub missing: Vec<String>,
    /// Therion's `-flip`: the scrap mirrored (x, y multipliers) before anything else.
    flip: [f64; 2],
}

/// A scrap's `-flip` (`horizontal` mirrors left-right, `vertical` top-bottom) as multipliers.
fn flip_of(scrap: &Scrap) -> [f64; 2] {
    match scrap
        .options
        .get("flip")
        .map(|f| f.to_ascii_lowercase())
        .as_deref()
    {
        Some("horizontal") => [-1.0, 1.0],
        Some("vertical") => [1.0, -1.0],
        _ => [1.0, 1.0],
    }
}

/// A scrap's station markers: those the reader pulled out, and `station:fixed` and the like (a
/// marker written with Therion's `type:subtype` shorthand, which the reader keeps as a point).
pub fn ties(scrap: &Scrap) -> Vec<StationTie> {
    let mut out = scrap.stations.clone();
    for p in &scrap.points {
        if crate::vocab::split(&p.kind, None).0 == "station"
            && let Some(name) = p.options.get("name").filter(|n| !n.is_empty())
        {
            out.push(StationTie {
                name: name.clone(),
                x: p.x,
                y: p.y,
            });
        }
    }
    out
}

/// Metres in one unit of a scrap's `-scale`.
fn metres_per(unit: &str) -> f64 {
    match unit.to_ascii_lowercase().as_str() {
        "ft" | "feet" | "foot" => 0.3048,
        "in" | "inch" | "inches" => 0.0254,
        "yd" | "yard" | "yards" => 0.9144,
        "cm" | "centimetre" | "centimeter" | "centimetres" | "centimeters" => 0.01,
        "mm" => 0.001,
        _ => 1.0,
    }
}

/// Map metres per sketch unit, by the scrap's declaration.
pub fn declared(scale: Option<&Scale>) -> Option<f64> {
    let s = scale?;
    let k = s.units_per_drawing * metres_per(&s.unit);
    (k.is_finite() && k > 0.0).then_some(k)
}

impl Placement {
    /// Works out where a scrap goes. `find` gives a tie's station position on the map.
    pub fn solve(
        scrap: &Scrap,
        find: &dyn Fn(&str) -> Option<[f64; 2]>,
    ) -> Result<Placement, Vec<String>> {
        let mut pairs = Vec::new();
        let mut missing = Vec::new();
        let flip = flip_of(scrap);
        for t in &ties(scrap) {
            match find(&t.name) {
                // the fit works in scan pixels (y down): hand it the sketch with y turned over
                Some(p) => pairs.push(((flip[0] * t.x, -flip[1] * t.y), (p[0], p[1]))),
                None => missing.push(t.name.clone()),
            }
        }
        if pairs.is_empty() {
            return Err(missing);
        }
        if pairs.len() == 1 {
            let k = declared(scrap.scale.as_ref()).unwrap_or(1.0);
            let ((sx, sy), (x, y)) = pairs[0];
            return Ok(Placement {
                kind: Kind::Moved,
                m: [k, 0.0, x - k * sx, 0.0, k, y + k * sy],
                bend: None,
                residual: 0.0,
                used: 1,
                missing,
                flip,
            });
        }
        let warp = fit(&pairs, FitMode::Warp);
        let similar = fit(&pairs, FitMode::Rigid);
        let Some(f) = warp.or(similar) else {
            // every tie in the same place
            return Err(missing);
        };
        let bend = if pairs.len() >= 3 {
            rubber(&pairs)
        } else {
            None
        };
        let kind = if bend.is_some() {
            Kind::Warped
        } else {
            Kind::Similar
        };
        // back from pixels: py = -sy
        let m = f.matrix;
        Ok(Placement {
            kind,
            m: [m[0], -m[1], m[2], m[3], -m[4], m[5]],
            bend,
            residual: f.rms,
            used: pairs.len(),
            missing,
            flip,
        })
    }

    /// One sketch point on the map.
    pub fn at(&self, p: [f64; 2]) -> [f64; 2] {
        let p = [p[0] * self.flip[0], p[1] * self.flip[1]];
        if let Some(t) = &self.bend {
            let (x, y) = t.at(p[0], -p[1]);
            return [x, y];
        }
        let m = &self.m;
        [
            m[0] * p[0] + m[1] * p[1] + m[2],
            m[3] * p[0] + m[4] * p[1] + m[5],
        ]
    }

    /// Map metres per sketch unit (the geometric mean of the fit's two axes).
    pub fn scale(&self) -> f64 {
        let m = &self.m;
        let k = (m[0].hypot(m[3]) * m[1].hypot(m[4])).sqrt();
        if k.is_finite() && k > 0.0 { k } else { 1.0 }
    }

    /// The map direction (unit vector) of `deg` degrees clockwise from the top of the sketch, at `p`.
    pub fn direction(&self, p: [f64; 2], deg: f64) -> [f64; 2] {
        let r = deg.to_radians();
        let step = 1.0 / self.scale().max(1e-9) * 0.1; // a tenth of a metre on the map
        let a = self.at(p);
        let b = self.at([p[0] + step * r.sin(), p[1] + step * r.cos()]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let n = dx.hypot(dy);
        if n > 1e-12 {
            [dx / n, dy / n]
        } else {
            [0.0, 1.0]
        }
    }

    /// A line's segments on the map as points lying on the curve, `tol` metres from it at most.
    /// Bezier control points go through the placement too, so a bent curve keeps to its ends.
    pub fn flatten(&self, segs: &[Seg], tol: f64) -> Vec<[f64; 2]> {
        let mut out: Vec<[f64; 2]> = Vec::new();
        for (i, s) in segs.iter().enumerate() {
            let to = self.at(s.to);
            match (i, s.c1, s.c2, out.last().copied()) {
                (0, ..) | (_, None, _, _) | (_, _, None, _) | (_, _, _, None) => {}
                (_, Some(c1), Some(c2), Some(from)) => {
                    subdivide(from, self.at(c1), self.at(c2), to, tol, 0, &mut out);
                }
            }
            out.push(to);
        }
        out
    }
}

/// How deep a cubic is split: a cusp (controls on an anchor) is never flat.
const MAX_DEPTH: u32 = 12;

/// One cubic split until flat, pushing every point but its end.
fn subdivide(
    p0: [f64; 2],
    p1: [f64; 2],
    p2: [f64; 2],
    p3: [f64; 2],
    tol: f64,
    depth: u32,
    out: &mut Vec<[f64; 2]>,
) {
    if depth >= MAX_DEPTH || flat(p0, p1, p2, p3, tol) {
        return;
    }
    let mid = |a: [f64; 2], b: [f64; 2]| [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
    let (p01, p12, p23) = (mid(p0, p1), mid(p1, p2), mid(p2, p3));
    let (p012, p123) = (mid(p01, p12), mid(p12, p23));
    let c = mid(p012, p123);
    subdivide(p0, p01, p012, c, tol, depth + 1, out);
    out.push(c);
    subdivide(c, p123, p23, p3, tol, depth + 1, out);
}

/// Both controls within `tol` of the chord (a curve lies inside its hull). Ends in the same place
/// are a loop, flat only if the controls are there too.
fn flat(p0: [f64; 2], p1: [f64; 2], p2: [f64; 2], p3: [f64; 2], tol: f64) -> bool {
    let (dx, dy) = (p3[0] - p0[0], p3[1] - p0[1]);
    let span = dx.hypot(dy);
    let off = |p: [f64; 2]| {
        if span < 1e-12 {
            (p[0] - p0[0]).hypot(p[1] - p0[1])
        } else {
            ((p[0] - p0[0]) * dy - (p[1] - p0[1]) * dx).abs() / span
        }
    };
    off(p1) <= tol && off(p2) <= tol
}

/// Finds a sketch's station names among the survey's, read inside `scope` (the survey the page was
/// pulled in inside). Therion writes `1@inner.outer` (innermost survey first); this app's names
/// are `outer.inner.1`. Case is ignored, as notebooks rarely agree with the data on it; failing an
/// exact name, a name that is the only one ending in the tie's own path is taken.
pub struct Stations<'a> {
    by_name: HashMap<String, [f64; 2]>,
    all: Vec<(String, [f64; 2])>,
    scope: &'a str,
}

impl<'a> Stations<'a> {
    /// `stations`: every station's name and map position.
    pub fn new(stations: impl IntoIterator<Item = (String, [f64; 2])>, scope: &'a str) -> Self {
        let all: Vec<(String, [f64; 2])> = stations
            .into_iter()
            .map(|(n, p)| (n.to_lowercase(), p))
            .collect();
        Stations {
            by_name: all.iter().cloned().collect(),
            all,
            scope,
        }
    }

    /// The tie's station position.
    pub fn find(&self, tie: &str) -> Option<[f64; 2]> {
        let tie = tie.trim().to_lowercase();
        let rel = match tie.split_once('@') {
            Some((st, path)) => {
                let mut parts: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
                parts.reverse();
                parts.push(st);
                parts.join(".")
            }
            None => tie.clone(),
        };
        let scope = self.scope.to_lowercase();
        let full = if scope.is_empty() {
            rel.clone()
        } else {
            format!("{scope}.{rel}")
        };
        for name in [&full, &rel, &tie] {
            if let Some(p) = self.by_name.get(name) {
                return Some(*p);
            }
        }
        // a bare name must match exactly: "1" ending some other survey's name is a guess. A
        // qualified one ("1@nave") is specific enough to find as the one name ending in it.
        if !tie.contains('@') {
            return None;
        }
        let tail = format!(".{rel}");
        let mut hits = self.all.iter().filter(|(n, _)| n.ends_with(&tail));
        match (hits.next(), hits.next()) {
            (Some((_, p)), None) => Some(*p),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scrap(ties: &[(&str, f64, f64)]) -> Scrap {
        Scrap {
            name: "s".into(),
            projection: Some("plan".into()),
            projection_angle: None,
            scale: Some(Scale {
                units_per_drawing: 0.5,
                unit: "ft".into(),
            }),
            options: Default::default(),
            stations: ties
                .iter()
                .map(|(n, x, y)| StationTie {
                    name: n.to_string(),
                    x: *x,
                    y: *y,
                })
                .collect(),
            lines: vec![],
            areas: vec![],
            points: vec![],
            sections: vec![],
        }
    }

    #[test]
    fn three_ties_land_exactly_and_the_shape_follows() {
        let s = scrap(&[("a", 0.0, 0.0), ("b", 100.0, 0.0), ("c", 0.0, 100.0)]);
        let map: HashMap<&str, [f64; 2]> =
            [("a", [10.0, 20.0]), ("b", [10.0, 30.0]), ("c", [0.0, 20.0])].into();
        let p = Placement::solve(&s, &|n| map.get(n).copied()).unwrap();
        assert_eq!(p.kind, Kind::Warped);
        for (n, x, y) in [("a", 0.0, 0.0), ("b", 100.0, 0.0), ("c", 0.0, 100.0)] {
            let q = p.at([x, y]);
            assert!(
                (q[0] - map[n][0]).abs() < 1e-6 && (q[1] - map[n][1]).abs() < 1e-6,
                "{n} {q:?}"
            );
        }
        // turned a quarter left: the sketch's up is the map's west
        let d = p.direction([0.0, 0.0], 0.0);
        assert!((d[0] + 1.0).abs() < 1e-6 && d[1].abs() < 1e-6, "{d:?}");
        assert!((p.scale() - 0.1).abs() < 1e-9);
    }

    #[test]
    fn one_tie_moves_at_the_declared_scale_north_up() {
        let s = scrap(&[("a", 10.0, 10.0)]);
        let p = Placement::solve(&s, &|_| Some([100.0, 200.0])).unwrap();
        assert_eq!(p.kind, Kind::Moved);
        let q = p.at([20.0, 10.0]);
        assert!((q[0] - (100.0 + 10.0 * 0.5 * 0.3048)).abs() < 1e-9 && (q[1] - 200.0).abs() < 1e-9);
    }

    #[test]
    fn no_ties_is_not_placed() {
        let s = scrap(&[("a", 0.0, 0.0)]);
        assert_eq!(
            Placement::solve(&s, &|_| None).err(),
            Some(vec!["a".to_string()])
        );
    }

    #[test]
    fn a_curve_is_flattened_onto_itself() {
        let s = scrap(&[("a", 0.0, 0.0), ("b", 1.0, 0.0)]);
        let map: HashMap<&str, [f64; 2]> = [("a", [0.0, 0.0]), ("b", [1.0, 0.0])].into();
        let p = Placement::solve(&s, &|n| map.get(n).copied()).unwrap();
        let segs = [
            Seg {
                to: [0.0, 0.0],
                c1: None,
                c2: None,
            },
            Seg {
                to: [1.0, 0.0],
                c1: Some([0.0, 0.55]),
                c2: Some([1.0, 0.55]),
            },
        ];
        let pts = p.flatten(&segs, 0.001);
        assert!(pts.len() > 10);
        // the curve's middle is at three quarters of the control height
        let top = pts.iter().map(|q| q[1]).fold(0.0, f64::max);
        assert!((top - 0.4125).abs() < 0.002, "{top}");
    }

    #[test]
    fn therion_names_find_this_apps_stations() {
        let st = Stations::new(
            [
                ("main.1".to_string(), [1.0, 0.0]),
                ("main.side.4".to_string(), [4.0, 0.0]),
                ("other.side.4".to_string(), [9.0, 0.0]),
            ],
            "main",
        );
        assert_eq!(st.find("1"), Some([1.0, 0.0]));
        assert_eq!(st.find("4@side"), Some([4.0, 0.0]));
        assert_eq!(st.find("4@SIDE.other"), Some([9.0, 0.0]));
        assert_eq!(st.find("9"), None);
    }
}
