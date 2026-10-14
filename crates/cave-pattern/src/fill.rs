//! A fill: layers of hatch lines, dots and tiled motifs laid inside an outline.

use crate::{MAX_MM, MAX_PARTS, MIN_MM, Motif, PatternError, in_range, valid_name};
use cave_fill::{Marks, Rng, dots, hatch, inside};
use serde::{Deserialize, Serialize};

/// One layer of a fill. Layers draw on top of each other, so a cross-hatch is two hatch layers at different angles.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum Layer {
    /// Parallel lines.
    Hatch {
        /// Which way they run, in degrees counter-clockwise from east.
        angle_deg: f64,
        /// Distance between lines, in millimetres on paper.
        spacing_mm: f64,
    },
    /// A grid of dots.
    Dots {
        /// Distance between dots, in millimetres on paper.
        spacing_mm: f64,
        /// Width of a dot, in millimetres on paper.
        dot_mm: f64,
    },
    /// A motif repeated on a grid.
    Tile {
        /// Which motif (by name).
        motif: String,
        /// Its longest side, in millimetres on paper.
        size_mm: f64,
        /// Distance between columns, in millimetres on paper.
        across_mm: f64,
        /// Distance between rows, in millimetres on paper.
        down_mm: f64,
        /// Shift every other row by half a column (bricks, scales).
        #[serde(default)]
        stagger: bool,
        /// Turn each mark by this many degrees...
        #[serde(default)]
        turn_deg: f64,
        /// ...or at random.
        #[serde(default)]
        random_turn: bool,
        /// How much sizes vary, 0 (all alike) to 1 (down to half size).
        #[serde(default)]
        size_var: f64,
        /// How far marks wander from their grid spot, as a fraction of the spacing, 0 to 1.
        #[serde(default)]
        jitter: f64,
        /// The share of marks left out at random, 0 to 0.9.
        #[serde(default)]
        skip: f64,
    },
}

/// A named fill for areas.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FillDef {
    /// The name people pick it by.
    pub name: String,
    /// The layers, drawn in order.
    pub layers: Vec<Layer>,
    /// Tint the whole outline solid as well (water).
    #[serde(default)]
    pub tint: bool,
}

impl FillDef {
    /// Checks the definition is storable; `has_motif` says which motif names exist.
    pub fn validate(&self, has_motif: &dyn Fn(&str) -> bool) -> Result<(), PatternError> {
        if !valid_name(&self.name) {
            return Err(PatternError::BadName);
        }
        if self.layers.is_empty() && !self.tint {
            return Err(PatternError::NothingToDraw);
        }
        if self.layers.len() > MAX_PARTS {
            return Err(PatternError::TooLong);
        }
        for l in &self.layers {
            match l {
                Layer::Hatch {
                    angle_deg,
                    spacing_mm,
                } => {
                    in_range(*angle_deg, -360.0, 360.0, "A hatch angle")?;
                    in_range(*spacing_mm, 0.2, MAX_MM, "A hatch spacing")?;
                }
                Layer::Dots { spacing_mm, dot_mm } => {
                    in_range(*spacing_mm, 0.2, MAX_MM, "A dot spacing")?;
                    in_range(*dot_mm, MIN_MM, 20.0, "A dot width")?;
                }
                Layer::Tile {
                    motif,
                    size_mm,
                    across_mm,
                    down_mm,
                    turn_deg,
                    size_var,
                    jitter,
                    skip,
                    ..
                } => {
                    if !has_motif(motif) {
                        return Err(PatternError::MissingMotif(motif.clone()));
                    }
                    in_range(*size_mm, 0.2, 50.0, "A motif size")?;
                    in_range(*across_mm, 0.5, MAX_MM, "A column spacing")?;
                    in_range(*down_mm, 0.5, MAX_MM, "A row spacing")?;
                    in_range(*turn_deg, -360.0, 360.0, "A turn")?;
                    in_range(*size_var, 0.0, 1.0, "The size variation")?;
                    in_range(*jitter, 0.0, 1.0, "The wander")?;
                    in_range(*skip, 0.0, 0.9, "The share left out")?;
                }
            }
        }
        Ok(())
    }
}

/// The most marks one fill makes: a very large outline gets sparser rather than slower.
pub const MAX_MARKS: usize = 6000;

fn bounds(poly: &[[f64; 2]]) -> ([f64; 2], [f64; 2]) {
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for p in poly {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    }
    (lo, hi)
}

/// The marks of `def` inside `outline` (a polygon, metres), seeded by `seed` (the stroke's own, so a stroke always fills the same
/// way). `paper_m` is metres per millimetre on paper; `amount` makes it that many times as dense (1 as authored). Empty for an
/// outline with fewer than three points.
pub fn fill_marks(
    def: &FillDef,
    motifs: &dyn Fn(&str) -> Option<Motif>,
    outline: &[[f64; 2]],
    seed: u64,
    paper_m: f64,
    amount: f64,
) -> Marks {
    let mut out = Marks {
        tint: def.tint,
        ..Default::default()
    };
    if outline.len() < 3
        || !(paper_m.is_finite() && paper_m > 0.0)
        || outline
            .iter()
            .any(|p| !(p[0].is_finite() && p[1].is_finite()))
    {
        return out;
    }
    let amount = if amount.is_finite() {
        amount.clamp(0.1, 6.0)
    } else {
        1.0
    };
    let (lo, hi) = bounds(outline);
    let span = (hi[0] - lo[0]).hypot(hi[1] - lo[1]);
    let area = (hi[0] - lo[0]) * (hi[1] - lo[1]);
    for (index, layer) in def.layers.iter().enumerate() {
        match layer {
            Layer::Hatch {
                angle_deg,
                spacing_mm,
            } => {
                let mut sp = spacing_mm * paper_m / amount;
                while span / sp > 600.0 {
                    sp *= 2.0;
                }
                out.lines.extend(hatch(outline, angle_deg.to_radians(), sp));
            }
            Layer::Dots { spacing_mm, dot_mm } => {
                let mut sp = spacing_mm * paper_m / amount.sqrt();
                while area / (sp * sp) > MAX_MARKS as f64 {
                    sp *= 1.5;
                }
                let r = dot_mm * paper_m / 2.0;
                out.circles
                    .extend(dots(outline, sp).into_iter().map(|p| [p[0], p[1], r, 1.0]));
            }
            Layer::Tile {
                motif,
                size_mm,
                across_mm,
                down_mm,
                stagger,
                turn_deg,
                random_turn,
                size_var,
                jitter,
                skip,
            } => {
                let Some(m) = motifs(motif) else { continue };
                let k = amount.sqrt();
                let (mut dx, mut dy) = (across_mm * paper_m / k, down_mm * paper_m / k);
                while area / (dx * dy) > MAX_MARKS as f64 {
                    dx *= 1.5;
                    dy *= 1.5;
                }
                // each layer has its own stream, so adding a layer never reshuffles the ones before it
                let mut rng =
                    Rng::new(seed.wrapping_add((index as u64 + 1).wrapping_mul(0x9E37_79B9)));
                let size = size_mm * paper_m;
                let (x0, y0) = (
                    (lo[0] / dx).floor() as i64 - 1,
                    (lo[1] / dy).floor() as i64 - 1,
                );
                let (x1, y1) = (
                    (hi[0] / dx).ceil() as i64 + 1,
                    (hi[1] / dy).ceil() as i64 + 1,
                );
                for iy in y0..=y1 {
                    for ix in x0..=x1 {
                        // the same draws in every cell, so leaving one out never shifts the ones after it
                        let (gone, jx, jy, sz, turn) = (
                            rng.next_f64(),
                            rng.next_f64(),
                            rng.next_f64(),
                            rng.next_f64(),
                            rng.next_f64(),
                        );
                        if gone < *skip {
                            continue;
                        }
                        let shift = if *stagger && iy.rem_euclid(2) == 1 {
                            dx / 2.0
                        } else {
                            0.0
                        };
                        let at = [
                            ix as f64 * dx + shift + (jx - 0.5) * jitter * dx,
                            iy as f64 * dy + (jy - 0.5) * jitter * dy,
                        ];
                        if !inside(at, outline) {
                            continue;
                        }
                        let s = size * (1.0 - size_var * 0.5 * sz);
                        let a = if *random_turn {
                            turn * std::f64::consts::TAU
                        } else {
                            turn_deg.to_radians()
                        };
                        let (lines, circles) = m.place(at, a, s);
                        out.lines.extend(lines);
                        out.circles.extend(circles);
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use cave_symbols::Prim;

    fn square(side: f64) -> Vec<[f64; 2]> {
        vec![[0.0, 0.0], [side, 0.0], [side, side], [0.0, side]]
    }
    fn tick() -> Motif {
        Motif::from_drawing(
            "tick",
            vec![Prim::Poly {
                pts: vec![[0.0, -1.0], [0.0, 1.0]],
                closed: false,
                fill: false,
            }],
        )
        .unwrap()
    }
    fn find(name: &str) -> Option<Motif> {
        (name == "tick").then(tick)
    }
    fn tiles() -> FillDef {
        FillDef {
            name: "ticks".into(),
            layers: vec![Layer::Tile {
                motif: "tick".into(),
                size_mm: 2.0,
                across_mm: 5.0,
                down_mm: 5.0,
                stagger: true,
                turn_deg: 0.0,
                random_turn: true,
                size_var: 0.5,
                jitter: 0.4,
                skip: 0.2,
            }],
            tint: false,
        }
    }

    #[test]
    fn the_same_seed_fills_the_same_and_another_seed_differs() {
        let a = fill_marks(&tiles(), &find, &square(0.1), 7, 0.001, 1.0);
        let b = fill_marks(&tiles(), &find, &square(0.1), 7, 0.001, 1.0);
        let c = fill_marks(&tiles(), &find, &square(0.1), 8, 0.001, 1.0);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(
            a.lines.len() > 100,
            "a 100 mm square at 5 mm spacing holds hundreds of marks, got {}",
            a.lines.len()
        );
    }

    #[test]
    fn marks_stay_in_the_outline_and_amount_makes_it_denser() {
        let one = fill_marks(&tiles(), &find, &square(0.1), 3, 0.001, 1.0);
        let two = fill_marks(&tiles(), &find, &square(0.1), 3, 0.001, 4.0);
        assert!(two.lines.len() as f64 > one.lines.len() as f64 * 2.5);
        for l in &one.lines {
            // each tick's middle is inside; its ends reach at most half its size past
            let mid = [(l[0][0] + l[1][0]) / 2.0, (l[0][1] + l[1][1]) / 2.0];
            assert!(mid[0] > -0.001 && mid[0] < 0.101 && mid[1] > -0.001 && mid[1] < 0.101);
        }
    }

    #[test]
    fn turning_a_fill_turns_the_marks_and_not_the_boundary() {
        let d = FillDef {
            name: "h".into(),
            layers: vec![Layer::Hatch {
                angle_deg: 0.0,
                spacing_mm: 4.0,
            }],
            tint: false,
        };
        let sq = square(0.1);
        let flat = fill_marks(&d, &find, &sq, 1, 0.001, 1.0);
        assert!(
            flat.lines.iter().all(|l| (l[0][1] - l[1][1]).abs() < 1e-9),
            "0 degrees: horizontal"
        );
        let up = rotated(&sq, 90.0, |o| fill_marks(&d, &find, o, 1, 0.001, 1.0));
        assert!(
            up.lines.iter().all(|l| (l[0][0] - l[1][0]).abs() < 1e-9),
            "90 degrees: vertical"
        );
        for l in &up.lines {
            for p in l {
                assert!(
                    p[0] > -1e-6 && p[0] < 0.1 + 1e-6 && p[1] > -1e-6 && p[1] < 0.1 + 1e-6,
                    "still inside the same square"
                );
            }
        }
        assert_eq!(
            rotated(&sq, 0.0, |o| fill_marks(&d, &find, o, 1, 0.001, 1.0)),
            flat
        );
    }

    #[test]
    fn a_cross_hatch_is_two_layers() {
        let d = FillDef {
            name: "cross".into(),
            layers: vec![
                Layer::Hatch {
                    angle_deg: 45.0,
                    spacing_mm: 4.0,
                },
                Layer::Hatch {
                    angle_deg: -45.0,
                    spacing_mm: 4.0,
                },
            ],
            tint: false,
        };
        d.validate(&|_| true).unwrap();
        let both = fill_marks(&d, &find, &square(0.1), 1, 0.001, 1.0)
            .lines
            .len();
        let one = fill_marks(
            &FillDef {
                layers: d.layers[..1].to_vec(),
                ..d.clone()
            },
            &find,
            &square(0.1),
            1,
            0.001,
            1.0,
        )
        .lines
        .len();
        assert!(both > one && both < one * 3);
    }

    #[test]
    fn bad_definitions_and_outlines_are_refused_not_run() {
        let mut d = tiles();
        d.layers[0] = Layer::Hatch {
            angle_deg: 0.0,
            spacing_mm: 0.0,
        };
        assert!(d.validate(&|_| true).is_err());
        assert!(
            FillDef {
                name: "empty".into(),
                layers: vec![],
                tint: false
            }
            .validate(&|_| true)
            .is_err()
        );
        assert_eq!(
            tiles().validate(&|_| false),
            Err(PatternError::MissingMotif("tick".into()))
        );
        assert_eq!(
            fill_marks(&tiles(), &find, &[[0.0, 0.0], [1.0, 1.0]], 1, 0.001, 1.0)
                .lines
                .len(),
            0
        );
        assert_eq!(
            fill_marks(
                &tiles(),
                &find,
                &[[0.0, 0.0], [f64::NAN, 1.0], [1.0, 0.0]],
                1,
                0.001,
                1.0
            )
            .lines
            .len(),
            0
        );
    }

    #[test]
    fn a_huge_outline_gets_sparser_not_slower() {
        let m = fill_marks(&tiles(), &find, &square(1000.0), 1, 0.001, 1.0);
        assert!(m.lines.len() <= MAX_MARKS * 2);
    }
}

/// The marks `make` lays inside `outline`, turned by `angle_deg` about the middle of the outline's box: the outline itself does not
/// move. The pattern is laid on the outline turned the other way, then every mark is turned back, so a hatch at 0 degrees ends up
/// at `angle_deg` inside the same boundary. Zero (or an unusable angle) is `make` unchanged.
pub fn rotated(
    outline: &[[f64; 2]],
    angle_deg: f64,
    make: impl FnOnce(&[[f64; 2]]) -> Marks,
) -> Marks {
    if !angle_deg.is_finite() || angle_deg.abs() < 1e-9 || outline.is_empty() {
        return make(outline);
    }
    let (lo, hi) = bounds(outline);
    let mid = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
    let turn = |p: [f64; 2], a: f64| {
        let (s, c) = a.sin_cos();
        let (x, y) = (p[0] - mid[0], p[1] - mid[1]);
        [mid[0] + x * c - y * s, mid[1] + x * s + y * c]
    };
    let a = angle_deg.to_radians();
    let back: Vec<[f64; 2]> = outline.iter().map(|p| turn(*p, -a)).collect();
    let mut m = make(&back);
    for line in &mut m.lines {
        for p in line.iter_mut() {
            *p = turn(*p, a);
        }
    }
    for c in &mut m.circles {
        let q = turn([c[0], c[1]], a);
        *c = [q[0], q[1], c[2], c[3]];
    }
    m
}
