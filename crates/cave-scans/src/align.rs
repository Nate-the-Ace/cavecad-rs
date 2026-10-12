//! Placing a scan under the map: a few spots on the scan matched to the stations they are, and the
//! fit that follows from them. Only the matches are stored (a scan's spots and the station each one
//! is), so a placed scan moves with the survey when it is adjusted or revised.

use crate::{ScanError, key, key_checked};
use cave_doc::{Action, Document, Field, Kind, Value};
use cave_id::Id;
use serde::Serialize;
use std::collections::HashMap;

/// How a scan is fitted to its matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FitMode {
    /// Move, turn and resize only: the scan keeps its shape.
    Rigid,
    /// Also stretch and skew across and down to take out a flatbed scanner's distortion (three or
    /// more matches; with two it is the same as rigid).
    Warp,
    /// Rubber-sheet: the scan bends so that every match lands exactly on its station and the
    /// places between follow smoothly (three or more matches not in a line; otherwise it is the same
    /// as warp).
    Rubber,
}

/// What a spot on a scan is matched to.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Target {
    /// A station: the spot follows it when the survey changes.
    Station {
        /// The station.
        id: Id,
    },
    /// A fixed place in the view (metres in the view's own frame): for a drawing that shows
    /// something no station marks, such as a wall point on a cross section.
    Point {
        /// Across (profile: along the run; section: across the passage).
        x: f64,
        /// Up (elevation, or up the passage).
        y: f64,
    },
}

/// One spot on a scan and what it is. `px`, `py` are pixels from the scan's top left.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Anchor {
    /// Pixels from the left edge.
    pub px: f64,
    /// Pixels from the top edge.
    pub py: f64,
    /// What the spot is.
    pub target: Target,
}

/// A scan's matches in one view, and how to fit them.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Alignment {
    /// Rigid or warped.
    pub mode: FitMode,
    /// The matches, in the order they were made.
    pub anchors: Vec<Anchor>,
}

/// Where each station is in a view: east and north on the map, along and up on a profile band,
/// across and up on a cross section.
pub type Positions = HashMap<Id, (f64, f64)>;

impl Alignment {
    fn encode(&self) -> String {
        let mode = match self.mode {
            FitMode::Rigid => "rigid",
            FitMode::Warp => "warp",
            FitMode::Rubber => "rubber",
        };
        let mut out = String::from(mode);
        for a in &self.anchors {
            let target = match a.target {
                Target::Station { id } => id.to_string(),
                Target::Point { x, y } => format!("{x}:{y}"),
            };
            out.push_str(&format!(";{},{},{}", a.px, a.py, target));
        }
        out
    }

    fn decode(text: &str) -> Option<Alignment> {
        let mut parts = text.split(';');
        let mode = match parts.next()? {
            "rigid" => FitMode::Rigid,
            "warp" => FitMode::Warp,
            "rubber" => FitMode::Rubber,
            _ => return None,
        };
        let finite = |s: &str| s.parse::<f64>().ok().filter(|v| v.is_finite());
        let anchors = parts
            .filter(|p| !p.is_empty())
            .map(|p| {
                let mut f = p.split(',');
                let (px, py, t) = (finite(f.next()?)?, finite(f.next()?)?, f.next()?);
                let target = match t.split_once(':') {
                    Some((x, y)) => Target::Point {
                        x: finite(x)?,
                        y: finite(y)?,
                    },
                    None => Target::Station {
                        id: t.parse().ok()?,
                    },
                };
                Some(Anchor { px, py, target })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Alignment { mode, anchors })
    }
}

/// The matches kept for a scan on a trip in a view, or `None` when it has none.
pub fn alignment(doc: &Document, trip: Id, view: &str, path: &str) -> Option<Alignment> {
    let e = doc.entity(trip).filter(|e| e.kind() == Kind::Trip)?;
    Alignment::decode(e.map(Field::TripScanAlign).get(&key(view, path)?)?)
}

/// Writes (or, with `None`, clears) a scan's matches in a view.
pub fn set_alignment(
    doc: &Document,
    trip: Id,
    view: &str,
    path: &str,
    want: Option<Alignment>,
) -> Result<Vec<Action>, ScanError> {
    let k = key_checked(view, path)?;
    let e = doc
        .entity(trip)
        .filter(|e| e.kind() == Kind::Trip)
        .ok_or(ScanError::NoSuchTrip)?;
    let mut map = e.map(Field::TripScanAlign);
    match want.filter(|a| !a.anchors.is_empty()) {
        Some(a) => {
            let bad = |x: &Anchor| match x.target {
                Target::Station { id } => doc.entity(id).is_none_or(|e| e.kind() != Kind::Station),
                Target::Point { x, y } => !x.is_finite() || !y.is_finite(),
            };
            if a.anchors.iter().any(bad) {
                return Err(ScanError::NoSuchStation);
            }
            map.insert(k, a.encode());
        }
        None => {
            map.remove(&k);
        }
    }
    if map == e.map(Field::TripScanAlign) {
        return Ok(Vec::new());
    }
    Ok(vec![Action::Set {
        id: trip,
        field: Field::TripScanAlign,
        value: Value::Map(map),
    }])
}

/// Adds a match (the spot `px`, `py` on the scan is `target`).
pub fn add_anchor(
    doc: &Document,
    trip: Id,
    view: &str,
    path: &str,
    px: f64,
    py: f64,
    target: Target,
) -> Result<Vec<Action>, ScanError> {
    if !px.is_finite() || !py.is_finite() || px < 0.0 || py < 0.0 {
        return Err(ScanError::BadPath);
    }
    let mut a = alignment(doc, trip, view, path).unwrap_or(Alignment {
        mode: FitMode::Warp,
        anchors: Vec::new(),
    });
    a.anchors.push(Anchor { px, py, target });
    set_alignment(doc, trip, view, path, Some(a))
}

/// Takes away the match at `index`.
pub fn remove_anchor(
    doc: &Document,
    trip: Id,
    view: &str,
    path: &str,
    index: usize,
) -> Result<Vec<Action>, ScanError> {
    let Some(mut a) = alignment(doc, trip, view, path) else {
        return Ok(Vec::new());
    };
    if index < a.anchors.len() {
        a.anchors.remove(index);
    }
    set_alignment(doc, trip, view, path, Some(a))
}

/// Chooses how the matches are fitted.
pub fn set_mode(
    doc: &Document,
    trip: Id,
    view: &str,
    path: &str,
    mode: FitMode,
) -> Result<Vec<Action>, ScanError> {
    let Some(mut a) = alignment(doc, trip, view, path) else {
        return Ok(Vec::new());
    };
    a.mode = mode;
    set_alignment(doc, trip, view, path, Some(a))
}

/// A fit: `[a, b, c, d, e, f]` taking a pixel (from the top left, y down) to the map:
/// `x = a*px + b*py + c`, `y = d*px + e*py + f`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    /// The six numbers.
    pub matrix: [f64; 6],
    /// How far, on average (root mean square, metres), the matched spots land from their stations.
    pub rms: f64,
}

/// A spot on a scan `(px, py)` and the place on the map `(x, y)` it is.
pub type Pair = ((f64, f64), (f64, f64));

/// Fits spots `(px, py)` to the places `(x, y)` they are. Needs two or more spots that are not all
/// the same point. Warp needs three that are not in a line; without them it falls back to rigid.
pub fn fit(pairs: &[Pair], mode: FitMode) -> Option<Fit> {
    if pairs.len() < 2 {
        return None;
    }
    // Work with y up on the scan, as the map is, so a rotation is a rotation and never a reflection.
    let src: Vec<(f64, f64)> = pairs.iter().map(|p| (p.0.0, -p.0.1)).collect();
    let dst: Vec<(f64, f64)> = pairs.iter().map(|p| p.1).collect();
    let n = pairs.len() as f64;
    let mean = |v: &[(f64, f64)]| {
        (
            v.iter().map(|p| p.0).sum::<f64>() / n,
            v.iter().map(|p| p.1).sum::<f64>() / n,
        )
    };
    let (ps, qs) = (mean(&src), mean(&dst));
    let rigid = || -> Option<[f64; 6]> {
        let (mut a, mut b, mut s2) = (0.0, 0.0, 0.0);
        for (p, q) in src.iter().zip(&dst) {
            let (px, py, qx, qy) = (p.0 - ps.0, p.1 - ps.1, q.0 - qs.0, q.1 - qs.1);
            a += px * qx + py * qy;
            b += px * qy - py * qx;
            s2 += px * px + py * py;
        }
        if s2 < 1e-12 || a.hypot(b) < 1e-12 {
            return None;
        }
        let (c, s) = (a / s2, b / s2); // scale * (cos, sin)
        Some([
            c,
            -s,
            qs.0 - (c * ps.0 - s * ps.1),
            s,
            c,
            qs.1 - (s * ps.0 + c * ps.1),
        ])
    };
    let warp = || -> Option<[f64; 6]> {
        // Least squares for x = a px + b py + c and y = d px + e py + f over the centred spots.
        let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
        let (mut bx, mut by, mut cx, mut cy) = (0.0, 0.0, 0.0, 0.0);
        for (p, q) in src.iter().zip(&dst) {
            let (px, py, qx, qy) = (p.0 - ps.0, p.1 - ps.1, q.0 - qs.0, q.1 - qs.1);
            sxx += px * px;
            sxy += px * py;
            syy += py * py;
            bx += px * qx;
            by += py * qx;
            cx += px * qy;
            cy += py * qy;
        }
        let det = sxx * syy - sxy * sxy;
        if det.abs() < 1e-9 * (sxx * syy).max(1e-12) {
            return None; // all in a line
        }
        let solve = |u: f64, v: f64| ((u * syy - v * sxy) / det, (v * sxx - u * sxy) / det);
        let (a, b) = solve(bx, by);
        let (d, e) = solve(cx, cy);
        Some([
            a,
            b,
            qs.0 - (a * ps.0 + b * ps.1),
            d,
            e,
            qs.1 - (d * ps.0 + e * ps.1),
        ])
    };
    let m = match mode {
        FitMode::Warp | FitMode::Rubber if pairs.len() >= 3 => warp().or_else(rigid)?,
        _ => rigid()?,
    };
    let at = |p: &(f64, f64)| {
        (
            m[0] * p.0 + m[1] * p.1 + m[2],
            m[3] * p.0 + m[4] * p.1 + m[5],
        )
    };
    let rms = (src
        .iter()
        .zip(&dst)
        .map(|(p, q)| {
            let r = at(p);
            (r.0 - q.0).powi(2) + (r.1 - q.1).powi(2)
        })
        .sum::<f64>()
        / n)
        .sqrt();
    // Back to pixels with y down: py_up = -py_down.
    Some(Fit {
        matrix: [m[0], -m[1], m[2], m[3], -m[4], m[5]],
        rms,
    })
}

/// A rubber-sheet warp: a thin-plate spline taking a scan pixel `(px, py)` (y down) to the view's
/// metres, exactly through every match. With `u = (px - origin.0) / scale` and `v = (py - origin.1) / scale`:
/// `x = ax[0] + ax[1] u + ax[2] v + sum(wx[i] * U(|(u, v) - centers[i]|))`, `U(r) = r^2 ln r`, and
/// the same for `y` with `ay` and `wy`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Tps {
    /// The scan pixel taken as the centre of the working frame.
    pub origin: (f64, f64),
    /// Pixels per working unit.
    pub scale: f64,
    /// The matched spots in the working frame.
    pub centers: Vec<(f64, f64)>,
    /// The spline's weights for x and y.
    pub wx: Vec<f64>,
    /// See `wx`.
    pub wy: Vec<f64>,
    /// The affine part for x: constant, along u, along v.
    pub ax: [f64; 3],
    /// See `ax`.
    pub ay: [f64; 3],
}

fn kernel(r2: f64) -> f64 {
    if r2 < 1e-18 { 0.0 } else { 0.5 * r2 * r2.ln() } // r^2 ln r with r^2 given
}

impl Tps {
    /// Where a scan pixel lands in the view (metres).
    pub fn at(&self, px: f64, py: f64) -> (f64, f64) {
        let (u, v) = ((px - self.origin.0) / self.scale, (py - self.origin.1) / self.scale);
        let (mut x, mut y) = (self.ax[0] + self.ax[1] * u + self.ax[2] * v, self.ay[0] + self.ay[1] * u + self.ay[2] * v);
        for (i, c) in self.centers.iter().enumerate() {
            let k = kernel((u - c.0).powi(2) + (v - c.1).powi(2));
            x += self.wx[i] * k;
            y += self.wy[i] * k;
        }
        (x, y)
    }
}

/// Fits a rubber-sheet through the matches. Needs three or more spots that are not in a line and
/// are all different; otherwise `None`.
pub fn rubber(pairs: &[Pair]) -> Option<Tps> {
    let n = pairs.len();
    if n < 3 {
        return None;
    }
    let nf = n as f64;
    let origin = (
        pairs.iter().map(|p| p.0.0).sum::<f64>() / nf,
        pairs.iter().map(|p| p.0.1).sum::<f64>() / nf,
    );
    let scale = pairs
        .iter()
        .map(|p| (p.0.0 - origin.0).hypot(p.0.1 - origin.1))
        .fold(0.0, f64::max);
    if scale < 1e-9 {
        return None;
    }
    let centers: Vec<(f64, f64)> = pairs
        .iter()
        .map(|p| ((p.0.0 - origin.0) / scale, (p.0.1 - origin.1) / scale))
        .collect();
    let m = n + 3;
    // [K P; P^T 0] [w; a] = [target; 0], solved for x and y together by elimination.
    let mut a = vec![vec![0.0; m + 2]; m];
    for i in 0..n {
        for j in 0..n {
            a[i][j] = kernel((centers[i].0 - centers[j].0).powi(2) + (centers[i].1 - centers[j].1).powi(2));
        }
        a[i][i] += 1e-9;
        a[i][n] = 1.0;
        a[i][n + 1] = centers[i].0;
        a[i][n + 2] = centers[i].1;
        a[n][i] = 1.0;
        a[n + 1][i] = centers[i].0;
        a[n + 2][i] = centers[i].1;
        a[i][m] = pairs[i].1.0;
        a[i][m + 1] = pairs[i].1.1;
    }
    for col in 0..m {
        let pivot = (col..m).max_by(|&r, &s| a[r][col].abs().total_cmp(&a[s][col].abs()))?;
        if a[pivot][col].abs() < 1e-12 {
            return None; // in a line, or two spots the same
        }
        a.swap(col, pivot);
        for r in 0..m {
            if r != col {
                let f = a[r][col] / a[col][col];
                if f != 0.0 {
                    for c in col..m + 2 {
                        a[r][c] -= f * a[col][c];
                    }
                }
            }
        }
    }
    let sol = |c: usize| -> Vec<f64> { (0..m).map(|r| a[r][c] / a[r][r]).collect() };
    let (sx, sy) = (sol(m), sol(m + 1));
    if sx.iter().chain(&sy).any(|v| !v.is_finite()) {
        return None;
    }
    Some(Tps {
        origin,
        scale,
        centers,
        wx: sx[..n].to_vec(),
        wy: sy[..n].to_vec(),
        ax: [sx[n], sx[n + 1], sx[n + 2]],
        ay: [sy[n], sy[n + 1], sy[n + 2]],
    })
}

/// A scan placed in a view.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Placed {
    /// The trip it is linked to.
    pub trip: Id,
    /// The scan.
    pub path: String,
    /// Pixel to the view's metres, see [`Fit`].
    pub matrix: [f64; 6],
    /// Matches used (those whose station is in the view).
    pub used: usize,
    /// Mean miss of the matches, metres.
    pub rms: f64,
    /// How it was fitted.
    pub mode: FitMode,
    /// The outline it is trimmed to (scan pixels), or `None` for the whole scan.
    pub trim: Option<crate::Outline>,
    /// The rubber-sheet warp, when the scan is fitted that way: it replaces `matrix` for drawing.
    pub warp: Option<Tps>,
}

/// The size of one pixel in metres in a fit (the square root of its area scale).
fn pixel_size(m: &[f64; 6]) -> f64 {
    (m[0] * m[4] - m[1] * m[3]).abs().sqrt()
}

/// Every scan with enough matches in `view`, placed from where its stations are now in that view.
/// A scan with a single match has no scale of its own, so it is set down north-up at the average
/// scale of the other scans placed in the same view (and is not placed if there are none).
pub fn placements(doc: &Document, view: &str, positions: &Positions) -> Vec<Placed> {
    let mut out = Vec::new();
    // scans with one match, waiting for an average scale: (trip, path, mode, the match)
    let mut singles: Vec<(Id, String, FitMode, Pair)> = Vec::new();
    for trip in doc.ids_of(Kind::Trip) {
        let Some(e) = doc.entity(trip) else { continue };
        let linked = crate::scans_of(doc, trip);
        for (k, text) in e.map(Field::TripScanAlign) {
            let Some((v, path)) = crate::split_key(&k) else {
                continue;
            };
            if v != view || !linked.iter().any(|p| p == path) {
                continue;
            }
            let Some(al) = Alignment::decode(&text) else {
                continue;
            };
            let pairs: Vec<_> = al
                .anchors
                .iter()
                .filter_map(|a| {
                    let at = match a.target {
                        Target::Station { id } => *positions.get(&id)?,
                        Target::Point { x, y } => (x, y),
                    };
                    Some(((a.px, a.py), at))
                })
                .collect();
            if pairs.len() == 1 {
                singles.push((trip, path.to_string(), al.mode, pairs[0]));
            } else if let Some(f) = fit(&pairs, al.mode) {
                out.push(Placed {
                    trip,
                    trim: crate::trim(doc, trip, view, path),
                    path: path.to_string(),
                    matrix: f.matrix,
                    used: pairs.len(),
                    rms: f.rms,
                    mode: al.mode,
                    warp: if al.mode == FitMode::Rubber { rubber(&pairs) } else { None },
                });
            }
        }
    }
    let sizes: Vec<f64> = out.iter().map(|p| pixel_size(&p.matrix)).filter(|s| s.is_finite() && *s > 0.0).collect();
    if !sizes.is_empty() {
        let size = sizes.iter().sum::<f64>() / sizes.len() as f64;
        for (trip, path, mode, ((px, py), (x, y))) in singles {
            out.push(Placed {
                trim: crate::trim(doc, trip, view, &path),
                trip,
                path,
                // x = size*px + c, y = -size*py + f (pixels go down, the view goes up), through the one match
                matrix: [size, 0.0, x - size * px, 0.0, -size, y + size * py],
                used: 1,
                rms: 0.0,
                mode,
                warp: None,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(m: [f64; 6], pts: &[(f64, f64)]) -> Vec<((f64, f64), (f64, f64))> {
        pts.iter()
            .map(|p| {
                (
                    *p,
                    (
                        m[0] * p.0 + m[1] * p.1 + m[2],
                        m[3] * p.0 + m[4] * p.1 + m[5],
                    ),
                )
            })
            .collect()
    }

    fn close(a: [f64; 6], b: [f64; 6]) {
        for i in 0..6 {
            assert!((a[i] - b[i]).abs() < 1e-6, "{a:?} vs {b:?}");
        }
    }

    #[test]
    fn two_spots_give_the_turn_scale_and_move() {
        // 90 degrees anticlockwise on the map from a scan with y down, 0.01 m per pixel, then moved.
        let want = [0.0, 0.01, 5.0, 0.01, 0.0, 7.0];
        let f = fit(&pairs(want, &[(0.0, 0.0), (100.0, 40.0)]), FitMode::Rigid).unwrap();
        close(f.matrix, want);
        assert!(f.rms < 1e-9);
    }

    #[test]
    fn a_rigid_fit_never_reflects() {
        let f = fit(
            &pairs(
                [0.02, 0.0, 1.0, 0.0, -0.02, 3.0],
                &[(0.0, 0.0), (50.0, 0.0), (0.0, 50.0)],
            ),
            FitMode::Rigid,
        )
        .unwrap();
        let det = f.matrix[0] * f.matrix[4] - f.matrix[1] * f.matrix[3];
        assert!(
            det < 0.0,
            "pixels go down and the map goes up, so the map's handedness flips once: {det}"
        );
        // A scan drawn correctly (north up the page) fits with its up as the map's north.
        let p = fit(
            &[((0.0, 100.0), (0.0, 0.0)), ((0.0, 0.0), (0.0, 1.0))],
            FitMode::Rigid,
        )
        .unwrap();
        assert!(p.matrix[4] < 0.0 && p.matrix[0] > 0.0, "{:?}", p.matrix);
    }

    #[test]
    fn warp_recovers_a_stretch_and_skew_exactly() {
        let want = [0.011, 0.002, -3.0, -0.001, -0.013, 8.0];
        let pts = [(0.0, 0.0), (900.0, 30.0), (40.0, 700.0), (800.0, 650.0)];
        let f = fit(&pairs(want, &pts), FitMode::Warp).unwrap();
        close(f.matrix, want);
        assert!(f.rms < 1e-9);
        let r = fit(&pairs(want, &pts), FitMode::Rigid).unwrap();
        assert!(r.rms > 0.01, "a rigid fit cannot take out a skew");
    }

    #[test]
    fn a_rubber_sheet_passes_exactly_through_every_match_and_is_affine_when_the_data_is() {
        // A bent set of matches: every one must land exactly.
        let bent = [
            ((0.0, 0.0), (0.0, 0.0)),
            ((100.0, 0.0), (10.0, 0.5)),
            ((100.0, 100.0), (10.2, -9.0)),
            ((0.0, 100.0), (-0.5, -10.0)),
            ((50.0, 50.0), (5.0, -5.5)),
        ];
        let t = rubber(&bent).unwrap();
        for ((px, py), (x, y)) in bent {
            let (rx, ry) = t.at(px, py);
            assert!((rx - x).abs() < 1e-6 && (ry - y).abs() < 1e-6, "({px},{py}) -> ({rx},{ry}) not ({x},{y})");
        }
        // Matches that are exactly affine make a spline that is the same affine map everywhere.
        let m = [0.01, 0.002, 3.0, -0.001, -0.012, 8.0];
        let p = pairs(m, &[(0.0, 0.0), (900.0, 30.0), (40.0, 700.0), (800.0, 650.0)]);
        let t = rubber(&p).unwrap();
        for (px, py) in [(10.0, 10.0), (450.0, 300.0), (1200.0, -200.0)] {
            let (x, y) = t.at(px, py);
            assert!((x - (m[0] * px + m[1] * py + m[2])).abs() < 1e-6 && (y - (m[3] * px + m[4] * py + m[5])).abs() < 1e-6);
        }
        // In a line, or fewer than three: nothing to bend.
        assert!(rubber(&pairs(m, &[(0.0, 0.0), (10.0, 10.0), (20.0, 20.0)])).is_none());
        assert!(rubber(&p[..2]).is_none());
    }

    #[test]
    fn the_size_of_a_pixel_is_the_square_root_of_the_area_scale() {
        assert!((pixel_size(&[0.0, 0.01, 5.0, 0.01, 0.0, 7.0]) - 0.01).abs() < 1e-12);
        assert!((pixel_size(&[0.02, 0.0, 1.0, 0.0, -0.02, 3.0]) - 0.02).abs() < 1e-12);
    }

    #[test]
    fn spots_in_a_line_fall_back_to_rigid_and_one_spot_is_not_enough() {
        let line = [(0.0, 0.0), (10.0, 10.0), (20.0, 20.0)];
        let p = pairs([0.1, 0.0, 0.0, 0.0, 0.1, 0.0], &line);
        assert!(fit(&p, FitMode::Warp).is_some());
        assert!(fit(&p[..1], FitMode::Rigid).is_none());
        assert!(
            fit(
                &[((1.0, 1.0), (0.0, 0.0)), ((1.0, 1.0), (5.0, 5.0))],
                FitMode::Rigid
            )
            .is_none()
        );
    }
}
