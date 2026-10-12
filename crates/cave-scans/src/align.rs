//! Placing a scan under the map: a few spots on the scan matched to the stations they are, and the
//! fit that follows from them. Only the matches are stored (a scan's spots and the station each one
//! is), so a placed scan moves with the survey when it is adjusted or revised.

use crate::{ScanError, clean_path};
use cave_doc::{Action, Document, Field, Kind, Value};
use cave_id::Id;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

/// How a scan is fitted to its matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FitMode {
    /// Move, turn and resize only: the scan keeps its shape.
    Rigid,
    /// Also stretch and skew across and down to take out a flatbed scanner's distortion (three or
    /// more matches; with two it is the same as rigid).
    Warp,
}

/// One spot on a scan and the station it is. `px`, `py` are pixels from the scan's top left.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Anchor {
    /// Pixels from the left edge.
    pub px: f64,
    /// Pixels from the top edge.
    pub py: f64,
    /// The station this spot is.
    pub station: Id,
}

/// A scan's matches and how to fit them.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Alignment {
    /// Rigid or warped.
    pub mode: FitMode,
    /// The matches, in the order they were made.
    pub anchors: Vec<Anchor>,
}

impl Alignment {
    fn encode(&self) -> String {
        let mode = match self.mode {
            FitMode::Rigid => "rigid",
            FitMode::Warp => "warp",
        };
        let mut out = String::from(mode);
        for a in &self.anchors {
            out.push_str(&format!(";{},{},{}", a.px, a.py, a.station));
        }
        out
    }

    fn decode(text: &str) -> Option<Alignment> {
        let mut parts = text.split(';');
        let mode = match parts.next()? {
            "rigid" => FitMode::Rigid,
            "warp" => FitMode::Warp,
            _ => return None,
        };
        let anchors = parts
            .filter(|p| !p.is_empty())
            .map(|p| {
                let mut f = p.split(',');
                Some(Anchor {
                    px: f.next()?.parse().ok().filter(|v: &f64| v.is_finite())?,
                    py: f.next()?.parse().ok().filter(|v: &f64| v.is_finite())?,
                    station: f.next()?.parse().ok()?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Alignment { mode, anchors })
    }
}

/// The matches kept for a scan on a trip, or `None` when it has none.
pub fn alignment(doc: &Document, trip: Id, path: &str) -> Option<Alignment> {
    let e = doc.entity(trip).filter(|e| e.kind() == Kind::Trip)?;
    Alignment::decode(e.map(Field::TripScanAlign).get(&clean_path(path)?)?)
}

pub(crate) fn all(doc: &Document, trip: Id) -> BTreeMap<String, Alignment> {
    doc.entity(trip)
        .map(|e| {
            e.map(Field::TripScanAlign)
                .into_iter()
                .filter_map(|(k, v)| Alignment::decode(&v).map(|a| (k, a)))
                .collect()
        })
        .unwrap_or_default()
}

/// Writes (or, with `None`, clears) a scan's matches.
pub fn set_alignment(
    doc: &Document,
    trip: Id,
    path: &str,
    want: Option<Alignment>,
) -> Result<Vec<Action>, ScanError> {
    let path = clean_path(path).ok_or(ScanError::BadPath)?;
    let e = doc
        .entity(trip)
        .filter(|e| e.kind() == Kind::Trip)
        .ok_or(ScanError::NoSuchTrip)?;
    let mut map = e.map(Field::TripScanAlign);
    match want.filter(|a| !a.anchors.is_empty()) {
        Some(a) => {
            if a.anchors.iter().any(|x| {
                doc.entity(x.station)
                    .is_none_or(|e| e.kind() != Kind::Station)
            }) {
                return Err(ScanError::NoSuchStation);
            }
            map.insert(path, a.encode());
        }
        None => {
            map.remove(&path);
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

/// Adds a match (the spot `px`, `py` on the scan is `station`).
pub fn add_anchor(
    doc: &Document,
    trip: Id,
    path: &str,
    px: f64,
    py: f64,
    station: Id,
) -> Result<Vec<Action>, ScanError> {
    if !px.is_finite() || !py.is_finite() || px < 0.0 || py < 0.0 {
        return Err(ScanError::BadPath);
    }
    let mut a = alignment(doc, trip, path).unwrap_or(Alignment {
        mode: FitMode::Warp,
        anchors: Vec::new(),
    });
    a.anchors.push(Anchor { px, py, station });
    set_alignment(doc, trip, path, Some(a))
}

/// Takes away the match at `index`.
pub fn remove_anchor(
    doc: &Document,
    trip: Id,
    path: &str,
    index: usize,
) -> Result<Vec<Action>, ScanError> {
    let Some(mut a) = alignment(doc, trip, path) else {
        return Ok(Vec::new());
    };
    if index < a.anchors.len() {
        a.anchors.remove(index);
    }
    set_alignment(doc, trip, path, Some(a))
}

/// Chooses how the matches are fitted.
pub fn set_mode(
    doc: &Document,
    trip: Id,
    path: &str,
    mode: FitMode,
) -> Result<Vec<Action>, ScanError> {
    let Some(mut a) = alignment(doc, trip, path) else {
        return Ok(Vec::new());
    };
    a.mode = mode;
    set_alignment(doc, trip, path, Some(a))
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
        FitMode::Warp if pairs.len() >= 3 => warp().or_else(rigid)?,
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

/// A scan placed under the map.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Placed {
    /// The trip it is linked to.
    pub trip: Id,
    /// The scan.
    pub path: String,
    /// Pixel to map, see [`Fit`].
    pub matrix: [f64; 6],
    /// Matches used (those whose station is on the map).
    pub used: usize,
    /// Mean miss of the matches, metres.
    pub rms: f64,
    /// How it was fitted.
    pub mode: FitMode,
}

/// Every scan with enough matches, placed from where its stations are now (`positions`: east, north).
pub fn placements(doc: &Document, positions: &HashMap<Id, (f64, f64)>) -> Vec<Placed> {
    let mut out = Vec::new();
    for trip in doc.ids_of(Kind::Trip) {
        let linked = crate::scans_of(doc, trip);
        for (path, al) in all(doc, trip) {
            if !linked.contains(&path) {
                continue;
            }
            let pairs: Vec<_> = al
                .anchors
                .iter()
                .filter_map(|a| positions.get(&a.station).map(|p| ((a.px, a.py), *p)))
                .collect();
            if let Some(f) = fit(&pairs, al.mode) {
                out.push(Placed {
                    trip,
                    path,
                    matrix: f.matrix,
                    used: pairs.len(),
                    rms: f.rms,
                    mode: al.mode,
                });
            }
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
