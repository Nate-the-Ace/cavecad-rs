//! The set of motifs, linetypes and fills in play: the built-in ones plus the cave's own.

use crate::{Element, FillDef, Layer, LineDef, Motif, PatternError};
use cave_symbols::Prim;
use serde::{Deserialize, Serialize};

/// Everything a cave can pick from.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Library {
    /// Small drawings that repeat.
    pub motifs: Vec<Motif>,
    /// Patterns for lines.
    pub lines: Vec<LineDef>,
    /// Patterns for areas.
    pub fills: Vec<FillDef>,
}

impl Library {
    /// The motif called `name`, if there is one. A name is unique within the library: a later one replaces an earlier one.
    pub fn motif(&self, name: &str) -> Option<&Motif> {
        self.motifs.iter().rev().find(|m| m.name == name)
    }

    /// The linetype called `name`.
    pub fn line(&self, name: &str) -> Option<&LineDef> {
        self.lines.iter().rev().find(|l| l.name == name)
    }

    /// The fill called `name`.
    pub fn fill(&self, name: &str) -> Option<&FillDef> {
        self.fills.iter().rev().find(|f| f.name == name)
    }

    /// `base` with `own` laid over it: an own definition replaces a built-in one of the same name.
    pub fn over(base: Library, own: Library) -> Library {
        let mut out = base;
        out.motifs.extend(own.motifs);
        out.lines.extend(own.lines);
        out.fills.extend(own.fills);
        out
    }

    /// Checks a linetype against this library's motifs.
    pub fn check_line(&self, def: &LineDef) -> Result<(), PatternError> {
        def.validate(&|m| self.motif(m).is_some())
    }

    /// Checks a fill against this library's motifs.
    pub fn check_fill(&self, def: &FillDef) -> Result<(), PatternError> {
        def.validate(&|m| self.motif(m).is_some())
    }
}

/// A motif made of straight strokes through unit-square points.
fn strokes(name: &str, lines: &[&[[f64; 2]]]) -> Motif {
    let prims = lines
        .iter()
        .map(|pts| Prim::Poly {
            pts: pts.to_vec(),
            closed: false,
            fill: false,
        })
        .collect();
    Motif::from_drawing(name, prims).expect("a built-in motif is valid")
}

/// The patterns that ship with the app: a few motifs and a few linetypes and fills to start from or copy.
pub fn builtin_library() -> Library {
    let mut half_circle = Vec::new();
    for i in 0..=12 {
        let a = std::f64::consts::PI * i as f64 / 12.0;
        half_circle.push([a.cos() * 0.5, a.sin() * 0.5]);
    }
    let motifs = vec![
        strokes("Tick", &[&[[0.0, -0.5], [0.0, 0.5]]]),
        strokes("Tick up", &[&[[0.0, 0.0], [0.0, 1.0]]]),
        strokes("Chevron", &[&[[-0.35, 0.5], [0.0, 0.0], [-0.35, -0.5]]]),
        strokes(
            "Arrow",
            &[
                &[[-0.5, 0.0], [0.5, 0.0]],
                &[[0.2, 0.3], [0.5, 0.0], [0.2, -0.3]],
            ],
        ),
        strokes(
            "Cross",
            &[&[[-0.4, -0.4], [0.4, 0.4]], &[[-0.4, 0.4], [0.4, -0.4]]],
        ),
        strokes("Scallop", &[&half_circle]),
        Motif::from_drawing(
            "Ring",
            vec![Prim::Circle {
                c: [0.0, 0.0],
                r: 0.5,
                fill: false,
            }],
        )
        .expect("a built-in motif is valid"),
    ];
    let lines = vec![
        LineDef {
            name: "Ticked line".into(),
            elements: vec![
                Element::Dash { mm: 6.0 },
                Element::Motif {
                    motif: "Tick up".into(),
                    size_mm: 1.6,
                    turn_deg: 0.0,
                    along_mm: -3.0,
                    across_mm: 0.0,
                },
            ],
        },
        LineDef {
            name: "Chevron line".into(),
            elements: vec![
                Element::Dash { mm: 1.5 },
                Element::Motif {
                    motif: "Chevron".into(),
                    size_mm: 2.4,
                    turn_deg: 0.0,
                    along_mm: 0.0,
                    across_mm: 0.0,
                },
                Element::Gap { mm: 3.0 },
            ],
        },
        LineDef {
            name: "Scalloped line".into(),
            elements: vec![
                Element::Motif {
                    motif: "Scallop".into(),
                    size_mm: 3.0,
                    turn_deg: 0.0,
                    along_mm: 0.0,
                    across_mm: 0.0,
                },
                Element::Gap { mm: 3.0 },
            ],
        },
    ];
    let fills = vec![
        FillDef {
            name: "Cross-hatch".into(),
            layers: vec![
                Layer::Hatch {
                    angle_deg: 45.0,
                    spacing_mm: 3.0,
                },
                Layer::Hatch {
                    angle_deg: -45.0,
                    spacing_mm: 3.0,
                },
            ],
            tint: false,
        },
        FillDef {
            name: "Stippled".into(),
            layers: vec![Layer::Dots {
                spacing_mm: 2.5,
                dot_mm: 0.35,
            }],
            tint: false,
        },
        FillDef {
            name: "Scattered crosses".into(),
            layers: vec![Layer::Tile {
                motif: "Cross".into(),
                size_mm: 1.6,
                across_mm: 4.0,
                down_mm: 4.0,
                stagger: true,
                turn_deg: 0.0,
                random_turn: true,
                size_var: 0.4,
                jitter: 0.6,
                skip: 0.15,
            }],
            tint: false,
        },
    ];
    Library {
        motifs,
        lines,
        fills,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_that_ships_is_valid() {
        let lib = builtin_library();
        for l in &lib.lines {
            lib.check_line(l)
                .unwrap_or_else(|e| panic!("{}: {e}", l.name));
        }
        for f in &lib.fills {
            lib.check_fill(f)
                .unwrap_or_else(|e| panic!("{}: {e}", f.name));
        }
        for m in &lib.motifs {
            m.validate().unwrap_or_else(|e| panic!("{}: {e}", m.name));
        }
    }

    #[test]
    fn an_own_definition_replaces_a_built_in_one_of_the_same_name() {
        let own = Library {
            motifs: vec![strokes("Tick", &[&[[0.0, 0.0], [1.0, 0.0]]])],
            ..Default::default()
        };
        let lib = Library::over(builtin_library(), own.clone());
        assert_eq!(lib.motif("Tick"), own.motifs.first());
    }

    #[test]
    fn definitions_survive_json() {
        let lib = builtin_library();
        let text = serde_json::to_string(&lib).unwrap();
        assert_eq!(serde_json::from_str::<Library>(&text).unwrap(), lib);
    }
}
