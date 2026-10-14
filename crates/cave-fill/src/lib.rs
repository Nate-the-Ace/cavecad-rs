//! Area fills: a closed outline filled with a pattern (sand, clay, breakdown, water, flowstone, ...).
//! Only the outline is authored; the marks are derived from it every time, and a seed taken from the
//! stroke's id makes them the same on every device and every redraw. A port of the old app's `CsArea`
//! (Park-Miller random numbers, even-odd inside test, rejection sampling with a minimum spacing),
//! checked against its answers in `tests/golden.rs`.
//!
//! Lengths are metres. The old app drew in feet, so its element sizes and densities (elements per
//! 100 square feet) are kept as authored and converted by `FOOT`.

use cave_symbols::Prim;
use serde::Serialize;

/// Metres in one foot: the old app's drawing unit.
pub const FOOT: f64 = 0.3048;

/// Park-Miller minimal standard generator, as the old app used (`CsArea.rng`), so the same seed and
/// outline give the same marks. Every step is exact in a double.
pub struct Rng(u64);

impl Rng {
    /// A generator for `seed` (zero becomes one).
    pub fn new(seed: u64) -> Self {
        let s = seed % 2_147_483_647;
        Rng(if s == 0 { 1 } else { s })
    }

    /// The next number in `[0, 1]`.
    pub fn next_f64(&mut self) -> f64 {
        self.0 = (self.0 * 16807) % 2_147_483_647;
        (self.0 - 1) as f64 / 2_147_483_646.0
    }
}

/// A seed from a stroke's id text: the same id always gives the same seed.
pub fn seed_of(id: &str) -> u64 {
    // FNV-1a, folded into the generator's range
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in id.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    1 + h % 2_147_483_645
}

/// How an area is filled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pattern {
    /// Pictures scattered at random, never closer than a spacing.
    Scatter {
        /// Elements to draw (from `ELEMENTS`), picked at random per mark.
        elements: &'static [&'static str],
        /// Marks per 100 square feet.
        density: f64,
        /// Smallest and largest size, in feet of element radius.
        scale: (f64, f64),
        /// Turn each mark at random.
        rotate: bool,
        /// Minimum spacing as a fraction of the even spacing, and an absolute floor in feet.
        spacing: Option<(f64, f64)>,
    },
    /// Parallel lines at an angle, a spacing apart (feet).
    Hatch {
        /// Angle of the lines in radians.
        angle: f64,
        /// Distance between lines in feet.
        spacing: f64,
    },
    /// A regular grid of small dots (feet apart).
    Dots {
        /// Distance between dots in feet.
        spacing: f64,
    },
    /// A solid tint.
    Solid,
    /// Nothing but the outline.
    Outline,
}

/// One kind of fill.
#[derive(Clone, Copy, Debug)]
pub struct Fill {
    /// The type key a filled stroke carries (`fill-sand`).
    pub key: &'static str,
    /// The name people see.
    pub label: &'static str,
    /// What it means (shown as a tooltip).
    pub help: &'static str,
    /// How it is filled.
    pub pattern: Pattern,
}

const BLOCKS: &[&str] = &["sym-breakdown", "sym-breakdown-b", "sym-breakdown-c"];

/// Every fill, in palette order. Numbers are the old app's `CsArea.CATALOG`.
pub const FILLS: &[Fill] = &[
    Fill {
        key: "fill-blocks",
        label: "Blocks",
        help: "Boulders fallen from the ceiling: the kind you climb over or squeeze between. Use Debris for anything smaller.",
        pattern: Pattern::Scatter {
            elements: BLOCKS,
            density: 16.0,
            scale: (0.7, 1.5),
            rotate: true,
            spacing: Some((0.55, 0.6)),
        },
    },
    Fill {
        key: "fill-debris",
        label: "Debris",
        help: "Rubble smaller than your helmet: chips and cobbles rather than boulders.",
        pattern: Pattern::Scatter {
            elements: BLOCKS,
            density: 40.0,
            scale: (0.25, 0.5),
            rotate: true,
            spacing: Some((0.55, 0.0)),
        },
    },
    Fill {
        key: "fill-pebbles",
        label: "Pebbles",
        help: "Rounded stones carried and sorted by water. Gravel, not sand.",
        pattern: Pattern::Scatter {
            elements: &["pebble"],
            density: 55.0,
            scale: (0.6, 1.2),
            rotate: true,
            spacing: Some((0.55, 0.0)),
        },
    },
    Fill {
        key: "fill-sand",
        label: "Sand",
        help: "Loose grains dropped by flowing water. Firm underfoot.",
        pattern: Pattern::Scatter {
            elements: &["stipple"],
            density: 120.0,
            scale: (0.8, 1.2),
            rotate: false,
            spacing: None,
        },
    },
    Fill {
        key: "fill-clay",
        label: "Clay / silt",
        help: "Fine mud dropped by standing water. Sticky, and it holds a bootprint.",
        pattern: Pattern::Scatter {
            elements: &["stipple"],
            density: 45.0,
            scale: (0.7, 1.0),
            rotate: false,
            spacing: None,
        },
    },
    Fill {
        key: "fill-bedrock",
        label: "Bedrock",
        help: "Solid rock floor with nothing lying on it. Draws its outline and no fill.",
        pattern: Pattern::Outline,
    },
    Fill {
        key: "fill-water",
        label: "Water / lake",
        help: "Standing water: a pool or a lake. Its edge prints as the shoreline.",
        pattern: Pattern::Solid,
    },
    Fill {
        key: "fill-sump",
        label: "Sump",
        help: "Where the passage floods to the ceiling and the way on is underwater.",
        pattern: Pattern::Hatch {
            angle: std::f64::consts::FRAC_PI_4,
            spacing: 2.0,
        },
    },
    Fill {
        key: "fill-flowstone",
        label: "Flowstone",
        help: "Calcite sheeting over the floor or wall, laid down by a film of water.",
        pattern: Pattern::Hatch {
            angle: std::f64::consts::FRAC_PI_4,
            spacing: 1.0,
        },
    },
    Fill {
        key: "fill-moonmilk",
        label: "Moonmilk",
        help: "Soft white paste on rock. Popcorn and coralloid belong here too.",
        pattern: Pattern::Dots { spacing: 1.5 },
    },
    Fill {
        key: "fill-guano",
        label: "Guano",
        help: "Bat droppings. Worth marking: it says where the roost is, and it is a hazard.",
        pattern: Pattern::Scatter {
            elements: &["dash"],
            density: 60.0,
            scale: (0.8, 1.3),
            rotate: true,
            spacing: None,
        },
    },
    Fill {
        key: "fill-ice",
        label: "Ice / snow",
        help: "Ice or firn that lasts the year round.",
        pattern: Pattern::Scatter {
            elements: &["crystal"],
            density: 30.0,
            scale: (0.8, 1.4),
            rotate: true,
            spacing: None,
        },
    },
    Fill {
        key: "fill-bones",
        label: "Bones",
        help: "Bone deposits. Archaeological: record them and leave them alone.",
        pattern: Pattern::Scatter {
            elements: &["bone"],
            density: 6.0,
            scale: (0.9, 1.3),
            rotate: true,
            spacing: None,
        },
    },
];

/// The fill for a type key, if it is one.
pub fn fill(key: &str) -> Option<&'static Fill> {
    FILLS.iter().find(|f| f.key == key)
}

/// True for a type key that names a fill.
pub fn is_fill_key(key: &str) -> bool {
    key.starts_with("fill-")
}

/// A drawn element in unit size (a radius of one foot), y up.
fn element(name: &str) -> Vec<Prim> {
    let poly = |pts: &[[f64; 2]]| Prim::Poly {
        pts: pts.to_vec(),
        closed: false,
        fill: false,
    };
    match name {
        "stipple" => vec![Prim::Circle {
            c: [0.0, 0.0],
            r: 0.05,
            fill: true,
        }],
        "pebble" => vec![
            Prim::Circle {
                c: [0.0, 0.0],
                r: 0.25,
                fill: false,
            },
            Prim::Circle {
                c: [0.4, 0.2],
                r: 0.15,
                fill: false,
            },
        ],
        "dash" => vec![poly(&[[-0.3, 0.0], [0.3, 0.0]])],
        "crystal" => (0..3)
            .map(|i| {
                let a = std::f64::consts::PI * f64::from(i) / 3.0;
                poly(&[
                    [-0.25 * a.cos(), -0.25 * a.sin()],
                    [0.25 * a.cos(), 0.25 * a.sin()],
                ])
            })
            .collect(),
        "bone" => vec![
            poly(&[[-0.5, 0.0], [0.5, 0.0]]),
            Prim::Circle {
                c: [-0.5, 0.0],
                r: 0.12,
                fill: false,
            },
            Prim::Circle {
                c: [0.5, 0.0],
                r: 0.12,
                fill: false,
            },
        ],
        key => cave_symbols::symbol(key)
            .map(|s| s.prims.clone())
            .unwrap_or_default(),
    }
}

/// The marks of a fill, in metres.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Marks {
    /// Polylines to stroke.
    pub lines: Vec<Vec<[f64; 2]>>,
    /// Circles `[x, y, r, filled]` (filled is 1 or 0).
    pub circles: Vec<[f64; 4]>,
    /// True when the outline is also to be tinted solid (water).
    pub tint: bool,
}

/// Even-odd point in polygon (a figure of eight fills both lobes).
pub fn inside(p: [f64; 2], poly: &[[f64; 2]]) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut a = poly[0];
    for i in 1..=n {
        let b = poly[i % n];
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0];
            if p[0] < x {
                inside = !inside;
            }
        }
        a = b;
    }
    inside
}

/// Unsigned polygon area (shoelace).
pub fn area(poly: &[[f64; 2]]) -> f64 {
    let n = poly.len();
    let mut a = 0.0;
    for i in 0..n {
        let (p, q) = (poly[i], poly[(i + 1) % n]);
        a += p[0] * q[1] - q[0] * p[1];
    }
    a.abs() / 2.0
}

fn bounds(poly: &[[f64; 2]]) -> ([f64; 2], [f64; 2]) {
    let mut lo = poly[0];
    let mut hi = poly[0];
    for p in poly {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    }
    (lo, hi)
}

/// One scattered element: where, which, how big, how turned.
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    /// Position.
    pub at: [f64; 2],
    /// Which element.
    pub element: &'static str,
    /// Size multiplier.
    pub scale: f64,
    /// Turn in radians.
    pub angle: f64,
}

/// The old `CsArea.scatterPlacements`, in the polygon's own units. The random draws of every try
/// happen before either rejection test, so the sequence depends only on the seed and the outline.
#[allow(clippy::too_many_arguments)]
pub fn scatter(
    poly: &[[f64; 2]],
    elements: &'static [&'static str],
    density: f64,
    scale: (f64, f64),
    rotate: bool,
    spacing: Option<(f64, f64)>,
    rng: &mut Rng,
    mul: f64,
    density_mul: f64,
) -> Vec<Placement> {
    let mut out: Vec<Placement> = Vec::new();
    let a = area(poly);
    let want = ((a / 100.0) * density * density_mul).round();
    if want <= 0.0 {
        return out;
    }
    let want = want as usize;
    let (lo, hi) = bounds(poly);
    let (w, h) = (hi[0] - lo[0], hi[1] - lo[1]);
    let mut min_spacing = 0.0;
    if let Some((factor, floor)) = spacing
        && factor > 0.0
    {
        min_spacing = (a / want as f64).sqrt() * factor;
        if floor > min_spacing {
            min_spacing = floor;
        }
    }
    let min_sq = min_spacing * min_spacing;
    let cap = want * 60 + 500;
    let mut tries = 0;
    while out.len() < want && tries < cap {
        tries += 1;
        let x = lo[0] + rng.next_f64() * w;
        let y = lo[1] + rng.next_f64() * h;
        let pick_element = rng.next_f64();
        let pick_scale = rng.next_f64();
        let pick_angle = rng.next_f64();
        if !inside([x, y], poly) {
            continue;
        }
        if min_spacing > 0.0
            && out
                .iter()
                .any(|o| (x - o.at[0]).powi(2) + (y - o.at[1]).powi(2) < min_sq)
        {
            continue;
        }
        let index =
            ((pick_element * elements.len() as f64).floor() as usize).min(elements.len() - 1);
        out.push(Placement {
            at: [x, y],
            element: elements[index],
            scale: mul * (scale.0 + pick_scale * (scale.1 - scale.0)),
            angle: if rotate {
                pick_angle * 2.0 * std::f64::consts::PI
            } else {
                0.0
            },
        });
    }
    out
}

/// Parallel lines at `angle` (radians), `spacing` apart, clipped to the polygon (even-odd).
pub fn hatch(poly: &[[f64; 2]], angle: f64, spacing: f64) -> Vec<Vec<[f64; 2]>> {
    let mut out = Vec::new();
    if poly.len() < 3 || spacing <= 0.0 {
        return out;
    }
    // work in a frame where the lines are horizontal: u along the line, v across
    let (c, s) = (angle.cos(), angle.sin());
    let to = |p: [f64; 2]| [p[0] * c + p[1] * s, -p[0] * s + p[1] * c];
    let back = |q: [f64; 2]| [q[0] * c - q[1] * s, q[0] * s + q[1] * c];
    let q: Vec<[f64; 2]> = poly.iter().map(|p| to(*p)).collect();
    let (lo, hi) = bounds(&q);
    let mut v = (lo[1] / spacing).floor() * spacing + spacing; // strictly inside: not along the bottom edge
    while v < hi[1] {
        let mut xs: Vec<f64> = Vec::new();
        for i in 0..q.len() {
            let (a, b) = (q[i], q[(i + 1) % q.len()]);
            if (a[1] > v) != (b[1] > v) {
                xs.push(a[0] + (v - a[1]) / (b[1] - a[1]) * (b[0] - a[0]));
            }
        }
        xs.sort_by(f64::total_cmp);
        for pair in xs.as_chunks::<2>().0 {
            out.push(vec![back([pair[0], v]), back([pair[1], v])]);
        }
        v += spacing;
    }
    out
}

/// A grid of points `spacing` apart that fall inside the polygon.
pub fn dots(poly: &[[f64; 2]], spacing: f64) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    if poly.len() < 3 || spacing <= 0.0 {
        return out;
    }
    let (lo, hi) = bounds(poly);
    let mut y = (lo[1] / spacing).ceil() * spacing;
    let mut row = 0;
    while y < hi[1] {
        let mut x =
            (lo[0] / spacing).ceil() * spacing + if row % 2 == 1 { spacing / 2.0 } else { 0.0 };
        while x < hi[0] {
            if inside([x, y], poly) {
                out.push([x, y]);
            }
            x += spacing;
        }
        y += spacing;
        row += 1;
    }
    out
}

/// The most marks drawn for one fill: a very large outline gets sparser rather than slower.
pub const MAX_MARKS: usize = 6000;

/// The marks of the fill `key` inside `outline` (metres, a polygon: the stroke's curve), seeded by
/// the stroke's id so the same stroke always fills the same way. Empty for an unknown type or a
/// degenerate outline.
pub fn marks(key: &str, outline: &[[f64; 2]], seed: u64) -> Marks {
    marks_with(key, outline, seed, 1.0)
}

/// The marks of a fill sized on paper at a drawing scale: `length` is the drawing's scale divided by the scale the patterns were
/// authored for (so 1 at 1:500, 4 at 1:2000). A fill is laid out as it was authored on an outline `length` times smaller and
/// then everything is made `length` times bigger, so on paper the marks keep their size and spacing at any scale. `amount` is as
/// for `marks_with`.
pub fn marks_at(key: &str, outline: &[[f64; 2]], seed: u64, amount: f64, length: f64) -> Marks {
    let k = if length.is_finite() {
        length.clamp(0.05, 200.0)
    } else {
        1.0
    };
    if (k - 1.0).abs() < 1e-9 {
        return marks_with(key, outline, seed, amount);
    }
    let small: Vec<[f64; 2]> = outline.iter().map(|p| [p[0] / k, p[1] / k]).collect();
    let mut m = marks_with(key, &small, seed, amount);
    for line in &mut m.lines {
        for p in line.iter_mut() {
            *p = [p[0] * k, p[1] * k];
        }
    }
    for c in &mut m.circles {
        *c = [c[0] * k, c[1] * k, c[2] * k, c[3]];
    }
    m
}

/// Like `marks`, with the fill made `amount` times as dense (1 is the pattern as authored; 2 puts twice as many marks, or hatch
/// lines, in the same area; 0.5 half as many). Clamped to a sensible range.
pub fn marks_with(key: &str, outline: &[[f64; 2]], seed: u64, amount: f64) -> Marks {
    let amount = if amount.is_finite() {
        amount.clamp(0.1, 6.0)
    } else {
        1.0
    };
    let Some(f) = fill(key) else {
        return Marks::default();
    };
    if outline.len() < 3 {
        return Marks::default();
    }
    // the old patterns are laid out in feet
    let poly: Vec<[f64; 2]> = outline.iter().map(|p| [p[0] / FOOT, p[1] / FOOT]).collect();
    let m = |p: [f64; 2]| [p[0] * FOOT, p[1] * FOOT];
    let mut out = Marks::default();
    match f.pattern {
        Pattern::Outline => {}
        Pattern::Solid => out.tint = true,
        Pattern::Hatch { angle, spacing } => {
            let mut sp = spacing / amount;
            // keep the line count reasonable on a huge outline
            let (lo, hi) = bounds(&poly);
            let span = (hi[0] - lo[0]).hypot(hi[1] - lo[1]);
            while span / sp > 600.0 {
                sp *= 2.0;
            }
            out.lines = hatch(&poly, angle, sp)
                .into_iter()
                .map(|l| l.into_iter().map(m).collect())
                .collect();
        }
        Pattern::Dots { spacing } => {
            let mut sp = spacing / amount.sqrt();
            let a = area(&poly);
            while a / (sp * sp) > MAX_MARKS as f64 {
                sp *= 1.5;
            }
            out.circles = dots(&poly, sp)
                .into_iter()
                .map(|p| {
                    let q = m(p);
                    [q[0], q[1], 0.06 * FOOT, 1.0]
                })
                .collect();
        }
        Pattern::Scatter {
            elements,
            density,
            scale,
            rotate,
            spacing,
        } => {
            let a = area(&poly);
            // thin out a huge area so the marks stay under the cap
            let density = density * amount;
            let wanted = (a / 100.0) * density;
            let density_mul = if wanted > MAX_MARKS as f64 {
                MAX_MARKS as f64 / wanted
            } else {
                1.0
            };
            let mut rng = Rng::new(seed);
            for pl in scatter(
                &poly,
                elements,
                density,
                scale,
                rotate,
                spacing,
                &mut rng,
                1.0,
                density_mul,
            ) {
                let (c, s) = (pl.angle.cos(), pl.angle.sin());
                let put = |q: [f64; 2]| {
                    m([
                        pl.at[0] + pl.scale * (q[0] * c - q[1] * s),
                        pl.at[1] + pl.scale * (q[0] * s + q[1] * c),
                    ])
                };
                for prim in element(pl.element) {
                    match prim {
                        Prim::Circle { c: cc, r, fill } => {
                            let q = put(cc);
                            out.circles.push([
                                q[0],
                                q[1],
                                r * pl.scale * FOOT,
                                if fill { 1.0 } else { 0.0 },
                            ]);
                        }
                        Prim::Poly { pts, closed, .. } => {
                            let mut line: Vec<[f64; 2]> = pts.iter().map(|q| put(*q)).collect();
                            if closed {
                                line.push(line[0]);
                            }
                            out.lines.push(line);
                        }
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

    fn square(side: f64) -> Vec<[f64; 2]> {
        vec![[0.0, 0.0], [side, 0.0], [side, side], [0.0, side]]
    }

    #[test]
    fn the_generator_is_the_old_one() {
        // Park-Miller with seed 1: 16807 / 2147483647 steps, scaled to [0, 1]
        let mut r = Rng::new(1);
        assert!((r.next_f64() - 16806.0 / 2_147_483_646.0).abs() < 1e-15);
        assert_eq!(Rng::new(0).next_f64(), Rng::new(1).next_f64());
    }

    #[test]
    fn inside_and_area() {
        let sq = square(10.0);
        assert!(inside([5.0, 5.0], &sq) && !inside([11.0, 5.0], &sq));
        assert!((area(&sq) - 100.0).abs() < 1e-12);
    }

    #[test]
    fn the_same_stroke_always_fills_the_same_way() {
        let outline: Vec<[f64; 2]> = square(30.0);
        let a = marks("fill-pebbles", &outline, seed_of("stroke-1"));
        assert_eq!(a, marks("fill-pebbles", &outline, seed_of("stroke-1")));
        assert_ne!(a, marks("fill-pebbles", &outline, seed_of("stroke-2")));
        assert!(!a.circles.is_empty());
    }

    #[test]
    fn marks_stay_inside_the_outline() {
        let outline = square(20.0);
        let m = marks("fill-sand", &outline, 7);
        assert!(!m.circles.is_empty());
        assert!(m.circles.iter().all(|c| inside([c[0], c[1]], &outline)));
    }

    #[test]
    fn a_hatch_is_clipped_to_the_outline_and_spaced() {
        let lines = hatch(&square(10.0), 0.0, 2.0);
        assert_eq!(lines.len(), 4, "lines at 2, 4, 6, 8");
        assert!(
            lines
                .iter()
                .all(|l| (l[0][0] - 0.0).abs() < 1e-9 && (l[1][0] - 10.0).abs() < 1e-9)
        );
        // a concave outline is cut into separate pieces
        let u = vec![
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 10.0],
            [7.0, 10.0],
            [7.0, 3.0],
            [3.0, 3.0],
            [3.0, 10.0],
            [0.0, 10.0],
        ];
        let cut = hatch(&u, 0.0, 2.0);
        assert!(cut.len() > 5, "the notch splits the upper lines in two");
    }

    #[test]
    fn solid_and_outline_fills_have_no_marks_and_unknown_makes_nothing() {
        let o = square(10.0);
        assert!(marks("fill-water", &o, 1).tint && marks("fill-water", &o, 1).lines.is_empty());
        assert_eq!(marks("fill-bedrock", &o, 1), Marks::default());
        assert_eq!(marks("fill-nothing", &o, 1), Marks::default());
        assert_eq!(marks("fill-sand", &o[..2], 1), Marks::default());
    }

    #[test]
    fn amount_makes_a_fill_denser_or_sparser() {
        let o = square(6.0); // small enough that the cap on marks is not what is counted
        let count = |k: &str, a: f64| {
            let m = marks_with(k, &o, 5, a);
            m.lines.len() + m.circles.len()
        };
        for k in ["fill-sand", "fill-sump", "fill-flowstone"] {
            let (half, one, two) = (count(k, 0.5), count(k, 1.0), count(k, 2.0));
            assert!(half < one && one < two, "{k}: {half} < {one} < {two}");
        }
        assert_eq!(
            marks_with("fill-sand", &o, 5, 1.0),
            marks("fill-sand", &o, 5),
            "1 is the pattern as authored"
        );
        assert_eq!(
            count("fill-sand", f64::NAN),
            count("fill-sand", 1.0),
            "nonsense means as authored"
        );
    }

    #[test]
    fn marks_keep_their_size_on_paper_at_any_drawing_scale() {
        let small = square(8.0);
        let big: Vec<[f64; 2]> = small.iter().map(|p| [p[0] * 4.0, p[1] * 4.0]).collect();
        for key in ["fill-sand", "fill-sump", "fill-blocks"] {
            let at500 = marks_at(key, &small, 3, 1.0, 1.0);
            let at2000 = marks_at(key, &big, 3, 1.0, 4.0);
            assert_eq!(
                at500.lines.len(),
                at2000.lines.len(),
                "{key}: the same marks on paper"
            );
            assert_eq!(at500.circles.len(), at2000.circles.len(), "{key}");
            for (a, b) in at500.circles.iter().zip(&at2000.circles) {
                assert!(
                    (a[2] * 4.0 - b[2]).abs() < 1e-9,
                    "{key}: a mark is four times bigger on the ground"
                );
            }
        }
        assert_eq!(
            marks_at("fill-sand", &small, 3, 1.0, 1.0),
            marks("fill-sand", &small, 3),
            "1:500 is as authored"
        );
        assert_eq!(
            marks_at("fill-sand", &small, 3, 1.0, f64::NAN),
            marks("fill-sand", &small, 3),
            "nonsense means as authored"
        );
    }

    #[test]
    fn a_huge_outline_is_capped() {
        let m = marks("fill-sand", &square(3000.0), 3);
        assert!(m.circles.len() <= MAX_MARKS + 50);
    }

    #[test]
    fn every_fill_is_in_the_palette_with_its_own_layer() {
        for f in FILLS {
            let st = cave_style::style(f.key);
            assert_eq!(st.key, f.key, "{} has no palette entry", f.key);
            assert!(st.layer.starts_with("CAVE-FILL-"));
        }
    }

    #[test]
    fn every_fill_has_what_it_needs() {
        for f in FILLS {
            assert!(is_fill_key(f.key) && !f.label.is_empty() && !f.help.is_empty());
            if let Pattern::Scatter { elements, .. } = f.pattern {
                assert!(elements.iter().all(|e| !element(e).is_empty()), "{}", f.key);
            }
        }
    }
}
