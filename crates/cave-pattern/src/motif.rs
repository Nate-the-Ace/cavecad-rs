//! A motif: a small drawing that repeats (a tick, a chevron, a scallop, a little rock).

use crate::{PatternError, valid_name};
use cave_symbols::Prim;
use serde::{Deserialize, Serialize};

/// A named small drawing, centred on the origin with its longest side 1, so a size in millimetres means what it says.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Motif {
    /// The name linetypes and fills refer to.
    pub name: String,
    /// The drawing, in unit coordinates (x along the line, y across it, y up).
    pub prims: Vec<Prim>,
}

fn finite(p: [f64; 2]) -> bool {
    p[0].is_finite() && p[1].is_finite()
}

impl Motif {
    /// Makes a motif from a drawing at any size and place: it is moved so its middle is the origin and scaled so its longest side
    /// is 1. Refused when empty, when any number is not finite, or when it has no size at all.
    pub fn from_drawing(name: &str, prims: Vec<Prim>) -> Result<Motif, PatternError> {
        if !valid_name(name) {
            return Err(PatternError::BadName);
        }
        let (lo, hi) = bounds(&prims).ok_or(PatternError::BadDrawing)?;
        let (w, h) = (hi[0] - lo[0], hi[1] - lo[1]);
        let side = w.max(h);
        if !(side.is_finite() && side > 1e-12) {
            return Err(PatternError::BadDrawing);
        }
        let mid = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
        let map = |p: [f64; 2]| [(p[0] - mid[0]) / side, (p[1] - mid[1]) / side];
        let prims = prims
            .into_iter()
            .map(|p| match p {
                Prim::Circle { c, r, fill } => Prim::Circle {
                    c: map(c),
                    r: r / side,
                    fill,
                },
                Prim::Poly { pts, closed, fill } => Prim::Poly {
                    pts: pts.into_iter().map(map).collect(),
                    closed,
                    fill,
                },
            })
            .collect();
        Ok(Motif {
            name: name.to_string(),
            prims,
        })
    }

    /// Checks the motif is storable: a good name and a drawing of finite numbers with some size.
    pub fn validate(&self) -> Result<(), PatternError> {
        if !valid_name(&self.name) {
            return Err(PatternError::BadName);
        }
        let (lo, hi) = bounds(&self.prims).ok_or(PatternError::BadDrawing)?;
        if (hi[0] - lo[0]).max(hi[1] - lo[1]) < 1e-9 {
            return Err(PatternError::BadDrawing);
        }
        Ok(())
    }

    /// The motif's strokes and circles placed at `at`, turned by `angle` (radians, counter-clockwise) and `size` across (metres).
    /// Lines are polylines; circles are `[x, y, r, filled]`. A closed polygon repeats its first point.
    pub fn place(
        &self,
        at: [f64; 2],
        angle: f64,
        size: f64,
    ) -> (Vec<Vec<[f64; 2]>>, Vec<[f64; 4]>) {
        let (s, c) = angle.sin_cos();
        let map = |p: [f64; 2]| {
            [
                at[0] + (p[0] * c - p[1] * s) * size,
                at[1] + (p[0] * s + p[1] * c) * size,
            ]
        };
        let mut lines = Vec::new();
        let mut circles = Vec::new();
        for prim in &self.prims {
            match prim {
                Prim::Circle { c: ctr, r, fill } => {
                    let q = map(*ctr);
                    circles.push([q[0], q[1], r * size, if *fill { 1.0 } else { 0.0 }]);
                }
                Prim::Poly { pts, closed, .. } => {
                    let mut line: Vec<[f64; 2]> = pts.iter().map(|p| map(*p)).collect();
                    if *closed && let Some(first) = line.first().copied() {
                        line.push(first);
                    }
                    if line.len() >= 2 {
                        lines.push(line);
                    }
                }
            }
        }
        (lines, circles)
    }
}

/// The box around a drawing, or `None` when it is empty or any number is not finite.
fn bounds(prims: &[Prim]) -> Option<([f64; 2], [f64; 2])> {
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    let mut any = false;
    let mut add = |p: [f64; 2], pad: f64| {
        lo = [lo[0].min(p[0] - pad), lo[1].min(p[1] - pad)];
        hi = [hi[0].max(p[0] + pad), hi[1].max(p[1] + pad)];
        any = true;
    };
    for prim in prims {
        match prim {
            Prim::Circle { c, r, .. } => {
                if !finite(*c) || !r.is_finite() || *r < 0.0 {
                    return None;
                }
                add(*c, *r);
            }
            Prim::Poly { pts, .. } => {
                if pts.is_empty() || pts.iter().any(|p| !finite(*p)) {
                    return None;
                }
                for p in pts {
                    add(*p, 0.0);
                }
            }
        }
    }
    any.then_some((lo, hi))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(a: [f64; 2], b: [f64; 2]) -> Prim {
        Prim::Poly {
            pts: vec![a, b],
            closed: false,
            fill: false,
        }
    }

    #[test]
    fn a_drawing_is_centred_and_scaled_to_one() {
        let m = Motif::from_drawing(
            "hook",
            vec![
                line([10.0, 20.0], [14.0, 20.0]),
                line([14.0, 20.0], [14.0, 22.0]),
            ],
        )
        .unwrap();
        let (lo, hi) = bounds(&m.prims).unwrap();
        assert!(((hi[0] - lo[0]).max(hi[1] - lo[1]) - 1.0).abs() < 1e-12);
        assert!(((lo[0] + hi[0]) / 2.0).abs() < 1e-12 && ((lo[1] + hi[1]) / 2.0).abs() < 1e-12);
    }

    #[test]
    fn bad_drawings_and_names_are_refused() {
        assert_eq!(
            Motif::from_drawing("x", vec![]),
            Err(PatternError::BadDrawing)
        );
        assert_eq!(
            Motif::from_drawing("x", vec![line([0.0, 0.0], [f64::NAN, 1.0])]),
            Err(PatternError::BadDrawing)
        );
        assert_eq!(
            Motif::from_drawing("x", vec![line([1.0, 1.0], [1.0, 1.0])]),
            Err(PatternError::BadDrawing)
        );
        assert_eq!(
            Motif::from_drawing("", vec![line([0.0, 0.0], [1.0, 0.0])]),
            Err(PatternError::BadName)
        );
        assert_eq!(
            Motif::from_drawing(" pad", vec![line([0.0, 0.0], [1.0, 0.0])]),
            Err(PatternError::BadName)
        );
    }

    #[test]
    fn placing_turns_and_sizes_the_drawing() {
        let m = Motif::from_drawing("tick", vec![line([0.0, -1.0], [0.0, 1.0])]).unwrap();
        let (lines, _) = m.place([10.0, 0.0], std::f64::consts::FRAC_PI_2, 4.0);
        // the tick stood across a line running along x; turned a quarter it lies along x, 4 long, centred at 10
        let l = &lines[0];
        assert!((l[0][0] - 12.0).abs() < 1e-9 || (l[0][0] - 8.0).abs() < 1e-9);
        assert!((l[0][1]).abs() < 1e-9 && (l[1][1]).abs() < 1e-9);
        assert!(((l[0][0] - l[1][0]).abs() - 4.0).abs() < 1e-9);
    }
}
