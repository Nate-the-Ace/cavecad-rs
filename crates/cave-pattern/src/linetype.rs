//! A linetype: dashes, gaps, dots and motifs repeated along a line.

use crate::{MAX_MM, MAX_PARTS, MIN_MM, Motif, PatternError, in_range, valid_name};
use serde::{Deserialize, Serialize};

/// One part of a linetype's repeating pattern.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum Element {
    /// A drawn stretch, `mm` long on paper.
    Dash {
        /// Length in millimetres on paper.
        mm: f64,
    },
    /// An empty stretch, `mm` long on paper.
    Gap {
        /// Length in millimetres on paper.
        mm: f64,
    },
    /// A dot, as long as the drawing's dot length; it takes no room of its own.
    Dot,
    /// A motif stood on the line where the pattern has got to. It takes no room of its own: a gap before and after sets the spacing.
    Motif {
        /// Which motif (by name).
        motif: String,
        /// How big its longest side is, in millimetres on paper.
        size_mm: f64,
        /// How far it is turned from the way it was drawn (x along the line, y across it), in degrees.
        #[serde(default)]
        turn_deg: f64,
        /// How far it is moved along the line, in millimetres.
        #[serde(default)]
        along_mm: f64,
        /// How far it is moved to the left of the direction of travel, in millimetres.
        #[serde(default)]
        across_mm: f64,
    },
}

/// A named repeating pattern for a line.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LineDef {
    /// The name people pick it by.
    pub name: String,
    /// The parts, repeated from the start of the line to its end.
    pub elements: Vec<Element>,
}

impl LineDef {
    /// How long one repeat is, in millimetres on paper.
    pub fn period_mm(&self) -> f64 {
        self.elements
            .iter()
            .map(|e| match e {
                Element::Dash { mm } | Element::Gap { mm } => mm.abs(),
                _ => 0.0,
            })
            .sum()
    }

    /// Checks the definition is storable and will draw something; `has_motif` says which motif names exist.
    pub fn validate(&self, has_motif: &dyn Fn(&str) -> bool) -> Result<(), PatternError> {
        if !valid_name(&self.name) {
            return Err(PatternError::BadName);
        }
        if self.elements.is_empty() || self.elements.len() > MAX_PARTS {
            return Err(if self.elements.is_empty() {
                PatternError::NothingToDraw
            } else {
                PatternError::TooLong
            });
        }
        let mut draws = false;
        for e in &self.elements {
            match e {
                Element::Dash { mm } => {
                    in_range(*mm, MIN_MM, MAX_MM, "A dash length")?;
                    draws = true;
                }
                Element::Gap { mm } => in_range(*mm, MIN_MM, MAX_MM, "A gap length")?,
                Element::Dot => draws = true,
                Element::Motif {
                    motif,
                    size_mm,
                    turn_deg,
                    along_mm,
                    across_mm,
                } => {
                    if !has_motif(motif) {
                        return Err(PatternError::MissingMotif(motif.clone()));
                    }
                    in_range(*size_mm, 0.2, 50.0, "A motif size")?;
                    in_range(*turn_deg, -360.0, 360.0, "A turn")?;
                    in_range(*along_mm, -MAX_MM, MAX_MM, "A shift along the line")?;
                    in_range(*across_mm, -MAX_MM, MAX_MM, "A shift across the line")?;
                    draws = true;
                }
            }
        }
        // something has to move along the line, or the layout would never leave the start
        if !draws || self.period_mm() < MIN_MM {
            return Err(PatternError::NothingToDraw);
        }
        Ok(())
    }
}

/// A motif stood on a line.
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    /// Where its middle is, in metres.
    pub at: [f64; 2],
    /// Which way it points, in radians: the line's direction plus its own turn. A motif is drawn with x along the line and y across it.
    pub angle: f64,
    /// Its longest side, in metres.
    pub size: f64,
    /// Which motif.
    pub motif: String,
}

/// A line drawn with a linetype.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    /// The drawn stretches (dashes and dots), each a polyline in metres.
    pub dashes: Vec<Vec<[f64; 2]>>,
    /// The motifs, in order along the line.
    pub placements: Vec<Placement>,
}

/// The most dashes and motifs one line gets: a very long line gets nothing more rather than freezing the page.
pub const MAX_MARKS: usize = 20_000;

/// Lays `def` along `path` (metres). `paper_m` is metres per millimetre on paper (the drawing's scale over 1000) and `dot_mm` how
/// long a dot is. Pure: the same inputs always give the same layout. A definition that does not draw anything (see `validate`)
/// gives an empty layout, so the page never hangs on a bad one.
pub fn lay_out(def: &LineDef, path: &[[f64; 2]], paper_m: f64, dot_mm: f64) -> Layout {
    let mut out = Layout::default();
    if path.len() < 2
        || !(paper_m.is_finite() && paper_m > 0.0)
        || def.period_mm() < MIN_MM
        || def.elements.is_empty()
    {
        return out;
    }
    let mut cum = Vec::with_capacity(path.len());
    let mut total = 0.0;
    cum.push(0.0);
    for w in path.windows(2) {
        total += (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
        cum.push(total);
    }
    if !(total.is_finite() && total > 0.0) {
        return out;
    }
    let dot = dot_mm.max(0.0) * paper_m;
    let mut s = 0.0;
    'repeat: loop {
        for e in &def.elements {
            if s >= total || out.dashes.len() + out.placements.len() >= MAX_MARKS {
                break 'repeat;
            }
            match e {
                Element::Dash { mm } => {
                    let len = mm.abs() * paper_m;
                    out.dashes.push(slice(path, &cum, s, (s + len).min(total)));
                    s += len;
                }
                Element::Gap { mm } => s += mm.abs() * paper_m,
                Element::Dot => {
                    out.dashes.push(slice(path, &cum, s, (s + dot).min(total)));
                }
                Element::Motif {
                    motif,
                    size_mm,
                    turn_deg,
                    along_mm,
                    across_mm,
                } => {
                    let at_s = s + along_mm * paper_m;
                    if (0.0..=total).contains(&at_s) {
                        let (p, tangent) = point_at(path, &cum, at_s);
                        let (sin, cos) = tangent.sin_cos();
                        let left = across_mm * paper_m;
                        out.placements.push(Placement {
                            at: [p[0] - sin * left, p[1] + cos * left],
                            angle: tangent + turn_deg.to_radians(),
                            size: size_mm * paper_m,
                            motif: motif.clone(),
                        });
                    }
                }
            }
        }
    }
    out
}

/// The point at distance `s` along the path and the direction of the segment it is on.
fn point_at(path: &[[f64; 2]], cum: &[f64], s: f64) -> ([f64; 2], f64) {
    let i = seg_index(cum, s);
    let (a, b) = (path[i], path[i + 1]);
    let len = (cum[i + 1] - cum[i]).max(1e-12);
    let t = ((s - cum[i]) / len).clamp(0.0, 1.0);
    (
        [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t],
        (b[1] - a[1]).atan2(b[0] - a[0]),
    )
}

/// The index of the segment holding distance `s` (the last one when `s` is at or past the end).
fn seg_index(cum: &[f64], s: f64) -> usize {
    let last = cum.len() - 2;
    match cum.binary_search_by(|c| c.partial_cmp(&s).unwrap_or(std::cmp::Ordering::Less)) {
        Ok(i) => i.min(last),
        Err(i) => i.saturating_sub(1).min(last),
    }
}

/// The part of the path between distances `s0` and `s1`, as a polyline.
fn slice(path: &[[f64; 2]], cum: &[f64], s0: f64, s1: f64) -> Vec<[f64; 2]> {
    let (p0, _) = point_at(path, cum, s0);
    let (p1, _) = point_at(path, cum, s1);
    let (i0, i1) = (seg_index(cum, s0), seg_index(cum, s1));
    let mut out = vec![p0];
    for p in path.iter().take(i1 + 1).skip(i0 + 1) {
        out.push(*p);
    }
    out.push(p1);
    out
}

/// Every motif a linetype names.
pub fn motifs_used(def: &LineDef) -> Vec<&str> {
    let mut v: Vec<&str> = def
        .elements
        .iter()
        .filter_map(|e| match e {
            Element::Motif { motif, .. } => Some(motif.as_str()),
            _ => None,
        })
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// The marks of a linetype laid on a path, as plain polylines, circles and filled shapes: dashes, then every motif placed. For the
/// viewer and the DXF export, which only need drawing.
pub fn marks(
    def: &LineDef,
    motifs: &dyn Fn(&str) -> Option<Motif>,
    path: &[[f64; 2]],
    paper_m: f64,
    dot_mm: f64,
) -> cave_fill::Marks {
    let lay = lay_out(def, path, paper_m, dot_mm);
    let mut m = cave_fill::Marks {
        lines: lay.dashes,
        ..Default::default()
    };
    for p in &lay.placements {
        if let Some(motif) = motifs(&p.motif) {
            let (lines, circles) = motif.place(p.at, p.angle, p.size);
            m.lines.extend(lines);
            m.circles.extend(circles);
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(elements: Vec<Element>) -> LineDef {
        LineDef {
            name: "test".into(),
            elements,
        }
    }
    fn all(_: &str) -> bool {
        true
    }
    const MM: f64 = 0.001; // metres per paper millimetre at 1:1

    #[test]
    fn dashes_and_gaps_repeat_to_the_end_of_a_straight_line() {
        let d = def(vec![Element::Dash { mm: 2.0 }, Element::Gap { mm: 1.0 }]);
        d.validate(&all).unwrap();
        let l = lay_out(&d, &[[0.0, 0.0], [0.010, 0.0]], MM, 0.4);
        // 2 on, 1 off: dashes at 0-2, 3-5, 6-8, 9-10 (the last one cut by the end)
        assert_eq!(l.dashes.len(), 4);
        assert!((l.dashes[1][0][0] - 0.003).abs() < 1e-12);
        assert!((l.dashes[3].last().unwrap()[0] - 0.010).abs() < 1e-12);
    }

    #[test]
    fn a_dash_follows_a_corner() {
        let d = def(vec![Element::Dash { mm: 6.0 }, Element::Gap { mm: 100.0 }]);
        let l = lay_out(&d, &[[0.0, 0.0], [0.004, 0.0], [0.004, 0.010]], MM, 0.4);
        assert_eq!(l.dashes[0].len(), 3, "start, the corner, the end");
        assert_eq!(l.dashes[0][1], [0.004, 0.0]);
        assert!((l.dashes[0][2][1] - 0.002).abs() < 1e-12);
    }

    #[test]
    fn a_motif_keeps_its_drawn_direction_and_can_be_moved_off_the_line() {
        let d = def(vec![
            Element::Motif {
                motif: "tick".into(),
                size_mm: 2.0,
                turn_deg: 0.0,
                along_mm: 0.0,
                across_mm: 1.0,
            },
            Element::Gap { mm: 5.0 },
        ]);
        let l = lay_out(&d, &[[0.0, 0.0], [0.012, 0.0]], MM, 0.4);
        assert_eq!(l.placements.len(), 3);
        let p = &l.placements[1];
        assert!(
            (p.at[0] - 0.005).abs() < 1e-12 && (p.at[1] - 0.001).abs() < 1e-12,
            "left of travel is +y"
        );
        assert!(
            p.angle.abs() < 1e-12,
            "a motif keeps the way it was drawn: its y stands across the line"
        );
        assert!((p.size - 0.002).abs() < 1e-12);
    }

    #[test]
    fn nothing_that_cannot_finish_is_accepted_or_laid_out() {
        assert!(def(vec![]).validate(&all).is_err());
        assert!(def(vec![Element::Gap { mm: 3.0 }]).validate(&all).is_err());
        assert!(
            def(vec![Element::Dash { mm: f64::NAN }])
                .validate(&all)
                .is_err()
        );
        assert!(def(vec![Element::Dash { mm: 0.0 }]).validate(&all).is_err());
        assert_eq!(
            def(vec![Element::Motif {
                motif: "gone".into(),
                size_mm: 2.0,
                turn_deg: 0.0,
                along_mm: 0.0,
                across_mm: 0.0
            }])
            .validate(&|_| false),
            Err(PatternError::MissingMotif("gone".into()))
        );
        // an unvalidated zero-length pattern must not hang the layout
        let bad = def(vec![Element::Dot]);
        assert_eq!(
            lay_out(&bad, &[[0.0, 0.0], [1.0, 0.0]], MM, 0.4),
            Layout::default()
        );
    }

    #[test]
    fn a_very_long_line_stops_at_the_cap() {
        let d = def(vec![Element::Dash { mm: 0.05 }, Element::Gap { mm: 0.05 }]);
        let l = lay_out(&d, &[[0.0, 0.0], [1000.0, 0.0]], MM, 0.4);
        assert_eq!(l.dashes.len(), MAX_MARKS);
    }

    #[test]
    fn the_same_inputs_lay_out_the_same() {
        let d = def(vec![
            Element::Dash { mm: 2.0 },
            Element::Dot,
            Element::Gap { mm: 1.5 },
        ]);
        let path = [[0.0, 0.0], [0.5, 0.2], [0.9, -0.1]];
        assert_eq!(lay_out(&d, &path, 0.5, 0.4), lay_out(&d, &path, 0.5, 0.4));
    }
}
