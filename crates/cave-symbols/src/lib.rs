//! The symbol catalogue: every point symbol a cartographer can place, with its names and its
//! drawing. A placed symbol is just a stroke of type `sym-<name>` (see `cave-draw`): its first
//! point is where it sits and its second point says which way it points and how big it is.
//!
//! Drawings are vectors in a unit square around the symbol's origin (the farthest drawn point is
//! at distance 1, y up), made only of polylines and circles, so any renderer can draw them. The
//! data is generated: `tools/extract_symbols.mjs` pulls the original app's symbols out of its
//! template, and `tools/import_symbol_set.mjs` adds symbols from other sets (UIS, USGS, ...).

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// One drawn piece of a symbol.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum Prim {
    /// A circle.
    Circle {
        /// Centre.
        c: [f64; 2],
        /// Radius.
        r: f64,
        /// True for a solid disc rather than a ring.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        fill: bool,
    },
    /// A polyline, optionally closed.
    Poly {
        /// The points, in order.
        pts: Vec<[f64; 2]>,
        /// True when the last point joins back to the first.
        #[serde(default)]
        closed: bool,
        /// True for a solid shape (only meaningful when closed).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        fill: bool,
    },
}

/// A point symbol.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Symbol {
    /// The type key a placed symbol carries (`sym-stalactite`).
    pub key: String,
    /// The old app's block name (`SYM_STALACTITE`), kept so its drawings can be matched up.
    pub block: String,
    /// Which set it comes from: `nss` (the original app's), `uis`, `usgs`, ...
    pub set: String,
    /// The name people see.
    pub name: String,
    /// The International Union of Speleology's name for it.
    pub uis: String,
    /// Where the palette files it (`Formations`, `Water`, ...).
    pub category: String,
    /// The DXF layer it exports to.
    pub layer: String,
    /// The drawing.
    pub prims: Vec<Prim>,
}

static DATA: &str = include_str!("../data/symbols.json");

/// Every symbol, in palette order (by category, then name).
pub fn catalogue() -> &'static [Symbol] {
    static CELL: OnceLock<Vec<Symbol>> = OnceLock::new();
    CELL.get_or_init(|| serde_json::from_str(DATA).expect("symbols.json is valid"))
}

/// The symbol for a type key, if there is one.
pub fn symbol(key: &str) -> Option<&'static Symbol> {
    catalogue().iter().find(|s| s.key == key)
}

/// True for a type key that names a symbol rather than a line.
pub fn is_symbol_key(key: &str) -> bool {
    key.starts_with("sym-")
}

/// The palette's categories, in order of first appearance.
pub fn categories() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for s in catalogue() {
        if !out.contains(&s.category.as_str()) {
            out.push(&s.category);
        }
    }
    out
}

impl Symbol {
    /// The drawing as polylines in a drawing's own units: the symbol sits at `at`, its up is the
    /// direction toward `aim` and its radius is the distance to it. Circles become closed
    /// 32-sided polygons (a DXF reader needs nothing but polylines for this).
    pub fn placed(&self, at: [f64; 2], aim: [f64; 2]) -> Vec<(Vec<[f64; 2]>, bool)> {
        let (dx, dy) = (aim[0] - at[0], aim[1] - at[1]);
        let r = dx.hypot(dy);
        if r < 1e-12 {
            return Vec::new();
        }
        let (ux, uy) = (dx / r, dy / r);
        // y is up here, so the symbol's right is (uy, -ux)
        let map = |p: [f64; 2]| {
            [
                at[0] + r * (p[0] * uy + p[1] * ux),
                at[1] + r * (-p[0] * ux + p[1] * uy),
            ]
        };
        let mut out = Vec::new();
        for p in &self.prims {
            match p {
                Prim::Circle { c, r: cr, .. } => {
                    let ring: Vec<[f64; 2]> = (0..32)
                        .map(|i| {
                            let a = std::f64::consts::TAU * f64::from(i) / 32.0;
                            map([c[0] + cr * a.cos(), c[1] + cr * a.sin()])
                        })
                        .collect();
                    out.push((ring, true));
                }
                Prim::Poly { pts, closed, .. } => {
                    out.push((pts.iter().map(|q| map(*q)).collect(), *closed))
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_original_apps_symbols_are_all_here() {
        assert_eq!(catalogue().len(), 80);
        assert!(symbol("sym-stalactite").is_some() && symbol("sym-bolt").is_some());
    }

    #[test]
    fn keys_are_unique_and_never_collide_with_line_types() {
        let mut keys: Vec<&str> = catalogue().iter().map(|s| s.key.as_str()).collect();
        keys.sort_unstable();
        let n = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), n);
        assert!(catalogue().iter().all(|s| is_symbol_key(&s.key)));
        for line in ["wall", "ledge", "pit", "slope", "sketch"] {
            assert!(symbol(line).is_none());
        }
    }

    #[test]
    fn every_drawing_is_finite_and_fits_the_unit_circle() {
        for s in catalogue() {
            assert!(!s.prims.is_empty(), "{} has no drawing", s.key);
            let mut far = 0.0_f64;
            for p in &s.prims {
                match p {
                    Prim::Circle { c, r, .. } => {
                        assert!(c.iter().all(|x| x.is_finite()) && *r > 0.0, "{}", s.key);
                        far = far.max(c[0].hypot(c[1]) + r);
                    }
                    Prim::Poly { pts, .. } => {
                        assert!(pts.len() >= 2, "{}", s.key);
                        for q in pts {
                            assert!(q.iter().all(|x| x.is_finite()), "{}", s.key);
                            far = far.max(q[0].hypot(q[1]));
                        }
                    }
                }
            }
            assert!((0.99..=1.01).contains(&far), "{} extent {far}", s.key);
        }
    }

    #[test]
    fn a_placed_symbol_points_where_it_is_aimed_and_is_as_big_as_the_aim_is_far() {
        let arrow = symbol("sym-north-arrow").unwrap();
        // aimed straight up (y up): the symbol is drawn as it is authored, scaled by 2
        let up = arrow.placed([10.0, 5.0], [10.0, 7.0]);
        let top = up
            .iter()
            .flat_map(|(p, _)| p)
            .map(|p| p[1])
            .fold(f64::MIN, f64::max);
        assert!(
            (top - 7.0).abs() < 1e-9,
            "the tip of the arrow is at the aim: {top}"
        );
        // aimed east: the tip swings to the east, still 2 away
        let east = arrow.placed([10.0, 5.0], [12.0, 5.0]);
        let tip = east
            .iter()
            .flat_map(|(p, _)| p)
            .map(|p| p[0])
            .fold(f64::MIN, f64::max);
        assert!((tip - 12.0).abs() < 1e-9, "{tip}");
        assert!(
            arrow.placed([1.0, 1.0], [1.0, 1.0]).is_empty(),
            "no aim, no symbol"
        );
    }

    #[test]
    fn every_symbol_is_named_and_filed() {
        for s in catalogue() {
            assert!(
                !s.name.is_empty()
                    && !s.uis.is_empty()
                    && !s.layer.is_empty()
                    && !s.category.is_empty()
            );
        }
        assert!(categories().contains(&"Water") && categories().contains(&"Formations"));
    }
}
