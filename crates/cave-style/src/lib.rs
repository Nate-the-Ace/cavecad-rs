//! The type palette: one table that says how every kind of thing looks.
//!
//! There are no layers. Each thing a person sees has a *type* (`wall`, `ledge`, `pit`, `splay`,
//! a symbol's name, ...) and the type decides its colour, weight and linetype, here and nowhere
//! else. The viewer and the DXF export both read this table, so a change here changes both.
//! Showing and hiding is a view filter by type and trip and is not part of this crate.
//!
//! Colours are *inks*, not fixed numbers: the viewer maps an ink onto its light or dark theme, and
//! the DXF export maps it onto an AutoCAD colour index. DXF layers are an export artefact only,
//! named here from the type (and the trip, when a drawing is split by trip).

use serde::Serialize;

/// A colour role. The viewer picks the theme colour; the DXF export picks a colour index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Ink {
    /// Main drawing ink: black on light, white on dark.
    Text,
    /// Quiet ink for supporting lines.
    Muted,
    /// The highlight colour.
    Accent,
    /// Caution colour (ties between sections).
    Warn,
    /// Error colour (loop misclosures).
    Danger,
    /// Survey stations.
    Station,
    /// Geology (joints, faults, bedding): a magenta, as on the UIS list.
    Geology,
}

/// How a line is broken up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Linetype {
    /// Unbroken.
    Solid,
    /// Dashes.
    Dashed,
    /// Long dashes with narrow gaps (presumed dimensions of a space).
    LongDash,
    /// Short dashes with wide gaps (a passage running underneath another).
    Sparse,
    /// A dash and a dot, repeated (joints, bedding planes).
    DashDot,
}

/// Every line pattern, in the order the DXF line-type table lists them.
pub const LINETYPES: &[Linetype] = &[
    Linetype::Dashed,
    Linetype::LongDash,
    Linetype::Sparse,
    Linetype::DashDot,
];

impl Linetype {
    /// The name of the pattern in a DXF file's line-type table.
    pub fn dxf_name(self) -> &'static str {
        match self {
            Linetype::Solid => "Continuous",
            Linetype::Dashed => "DASHED",
            Linetype::LongDash => "LONGDASH",
            Linetype::Sparse => "SPARSE",
            Linetype::DashDot => "DASHDOT",
        }
    }

    /// The pattern in metres: a positive length is a dash, a negative one a gap, zero a dot.
    pub fn pattern(self) -> &'static [f64] {
        match self {
            Linetype::Solid => &[],
            Linetype::Dashed => &[0.5, -0.3],
            Linetype::LongDash => &[1.2, -0.2],
            Linetype::Sparse => &[0.3, -0.9],
            Linetype::DashDot => &[0.8, -0.25, 0.0, -0.25],
        }
    }
}

/// How one type looks.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Style {
    /// The type this style is for.
    pub key: &'static str,
    /// Name shown to people (filters, legends).
    pub label: &'static str,
    /// Colour role.
    pub ink: Ink,
    /// Line weight in screen pixels at normal size.
    pub weight: f64,
    /// Line pattern.
    pub linetype: Linetype,
    /// How opaque it is, 0 to 1.
    pub opacity: f64,
    /// Opacity when another passage lies over it (it is also drawn dashed then).
    pub under_opacity: f64,
    /// Name of the DXF layer the type exports to.
    pub layer: &'static str,
}

#[allow(clippy::too_many_arguments)]
const fn s(
    key: &'static str,
    label: &'static str,
    ink: Ink,
    weight: f64,
    linetype: Linetype,
    opacity: f64,
    under_opacity: f64,
    layer: &'static str,
) -> Style {
    Style {
        key,
        label,
        ink,
        weight,
        linetype,
        opacity,
        under_opacity,
        layer,
    }
}

use Ink::*;
use Linetype::*;

/// The palette. Order is the order filters list them in.
pub const PALETTE: &[Style] = &[
    s("wall", "Wall", Text, 2.4, Solid, 1.0, 0.4, "CAVE-WALLS"),
    s("ledge", "Ledge", Text, 1.6, Dashed, 1.0, 0.4, "CAVE-LEDGES"),
    s("pit", "Pit", Text, 2.0, Solid, 1.0, 0.4, "CAVE-PITS"),
    s(
        "fill-blocks",
        "Blocks",
        Muted,
        1.0,
        Solid,
        0.9,
        0.4,
        "CAVE-FILL-BLOCKS",
    ),
    s(
        "fill-debris",
        "Debris",
        Muted,
        1.0,
        Solid,
        0.9,
        0.4,
        "CAVE-FILL-DEBRIS",
    ),
    s(
        "fill-pebbles",
        "Pebbles",
        Muted,
        1.0,
        Solid,
        0.9,
        0.4,
        "CAVE-FILL-PEBBLES",
    ),
    s(
        "fill-sand",
        "Sand",
        Muted,
        1.0,
        Solid,
        0.9,
        0.4,
        "CAVE-FILL-SAND",
    ),
    s(
        "fill-clay",
        "Clay / silt",
        Muted,
        1.0,
        Solid,
        0.9,
        0.4,
        "CAVE-FILL-CLAY",
    ),
    s(
        "fill-bedrock",
        "Bedrock",
        Muted,
        1.0,
        Solid,
        0.9,
        0.4,
        "CAVE-FILL-BEDROCK",
    ),
    s(
        "fill-water",
        "Water / lake",
        Accent,
        1.0,
        Solid,
        0.9,
        0.4,
        "CAVE-FILL-WATER",
    ),
    s(
        "fill-sump",
        "Sump",
        Accent,
        1.0,
        Solid,
        0.9,
        0.4,
        "CAVE-FILL-SUMP",
    ),
    s(
        "fill-flowstone",
        "Flowstone",
        Muted,
        1.0,
        Solid,
        0.9,
        0.4,
        "CAVE-FILL-FLOWSTONE",
    ),
    s(
        "fill-moonmilk",
        "Moonmilk",
        Muted,
        1.0,
        Solid,
        0.9,
        0.4,
        "CAVE-FILL-MOONMILK",
    ),
    s(
        "fill-guano",
        "Guano",
        Muted,
        1.0,
        Solid,
        0.9,
        0.4,
        "CAVE-FILL-GUANO",
    ),
    s(
        "fill-ice",
        "Ice / snow",
        Accent,
        1.0,
        Solid,
        0.9,
        0.4,
        "CAVE-FILL-ICE",
    ),
    s(
        "fill-bones",
        "Bones",
        Muted,
        1.0,
        Solid,
        0.9,
        0.4,
        "CAVE-FILL-BONES",
    ),
    s(
        "underlying",
        "Underlying passage",
        Text,
        1.6,
        Sparse,
        1.0,
        0.4,
        "CAVE-UNDERLYING",
    ),
    s(
        "presumed",
        "Presumed dimensions",
        Text,
        1.6,
        LongDash,
        1.0,
        0.4,
        "CAVE-PRESUMED",
    ),
    s(
        "narrow",
        "Too narrow to pass",
        Text,
        1.6,
        Dashed,
        1.0,
        0.4,
        "CAVE-NARROW",
    ),
    s(
        "dripline",
        "Dripline",
        Text,
        1.4,
        Solid,
        1.0,
        0.4,
        "CAVE-DRIPLINE",
    ),
    s(
        "joint",
        "Joint / fracture",
        Geology,
        1.4,
        DashDot,
        1.0,
        0.4,
        "CAVE-JOINTS",
    ),
    s(
        "fault",
        "Fault",
        Geology,
        2.0,
        Solid,
        1.0,
        0.4,
        "CAVE-FAULTS",
    ),
    s(
        "bedding",
        "Bedding plane",
        Geology,
        1.4,
        Dashed,
        1.0,
        0.4,
        "CAVE-BEDDING",
    ),
    s(
        "contour",
        "Contour line",
        Muted,
        1.0,
        Solid,
        1.0,
        0.4,
        "CAVE-CONTOURS",
    ),
    s("text", "Text", Text, 1.4, Solid, 1.0, 0.4, "CAVE-TEXT"),
    s(
        "flowstone",
        "Flowstone",
        Muted,
        1.4,
        Solid,
        1.0,
        0.4,
        "CAVE-FLOWSTONE",
    ),
    s(
        "rimstone",
        "Rimstone",
        Muted,
        1.4,
        Solid,
        1.0,
        0.4,
        "CAVE-RIMSTONE",
    ),
    s("slope", "Slope", Text, 1.2, Solid, 1.0, 0.4, "CAVE-SLOPES"),
    s(
        "centerline",
        "Survey line",
        Text,
        2.2,
        Solid,
        1.0,
        0.4,
        "CAVE-CENTERLINE",
    ),
    s(
        "centerline-behind",
        "Survey line (behind walls)",
        Muted,
        1.4,
        Solid,
        1.0,
        0.4,
        "CAVE-CENTERLINE-BEHIND",
    ),
    s(
        "splay",
        "Splay",
        Muted,
        1.0,
        Solid,
        0.45,
        0.2,
        "CAVE-SPLAYS",
    ),
    s(
        "station",
        "Station",
        Station,
        6.0,
        Solid,
        1.0,
        1.0,
        "CAVE-STATIONS",
    ),
    s(
        "label",
        "Station label",
        Muted,
        1.0,
        Solid,
        1.0,
        1.0,
        "CAVE-LABELS",
    ),
    s(
        "closure",
        "Loop closure",
        Accent,
        2.0,
        Dashed,
        1.0,
        0.4,
        "CAVE-CLOSURES",
    ),
    s(
        "tie",
        "Tie between sections",
        Warn,
        2.0,
        Dashed,
        1.0,
        0.4,
        "CAVE-TIES",
    ),
    s(
        "closure-note",
        "Closure error",
        Danger,
        1.5,
        Solid,
        1.0,
        1.0,
        "CAVE-CLOSURE-NOTES",
    ),
    s(
        "profile",
        "Profile band",
        Text,
        1.4,
        Solid,
        1.0,
        1.0,
        "CAVE-PROFILE",
    ),
    s(
        "profile-wall",
        "Profile wall",
        Muted,
        1.4,
        Solid,
        1.0,
        1.0,
        "CAVE-PROFILE-WALLS",
    ),
    s(
        "sketch",
        "Sketch",
        Accent,
        2.0,
        Solid,
        1.0,
        1.0,
        "CAVE-SKETCHES",
    ),
];

/// The types the Draw tab offers as drawing tools, in the order it lists them. Picking one and then
/// drawing in any view makes linework of that type in that view.
pub const DRAW_TOOLS: &[&str] = &[
    "wall",
    "ledge",
    "pit",
    "flowstone",
    "rimstone",
    "slope",
    "sketch",
    "text",
    "underlying",
    "presumed",
    "narrow",
    "dripline",
    "joint",
    "fault",
    "bedding",
    "contour",
];

/// How the Draw tab groups the drawing tools: a group name and the tools in it, in order.
pub const TOOL_GROUPS: &[(&str, &[&str])] = &[
    (
        "Linework",
        &[
            "wall",
            "ledge",
            "pit",
            "flowstone",
            "rimstone",
            "slope",
            "sketch",
            "text",
        ],
    ),
    (
        "Passage lines",
        &["underlying", "presumed", "narrow", "dripline"],
    ),
    (
        "Geology and relief",
        &["joint", "fault", "bedding", "contour"],
    ),
];

/// The styles of the drawing tools.
pub fn draw_tools() -> Vec<&'static Style> {
    DRAW_TOOLS.iter().map(|k| style(k)).collect()
}

/// What an unknown type (a symbol name nobody styled yet) looks like.
pub const DEFAULT: Style = s("default", "Other", Text, 1.6, Solid, 1.0, 0.4, "CAVE-OTHER");

/// The style for `key`; the default look for a type the palette does not know.
pub fn style(key: &str) -> &'static Style {
    PALETTE
        .iter()
        .find(|p| p.key == key)
        .unwrap_or(&DEFAULT_STYLE)
}

static DEFAULT_STYLE: Style = DEFAULT;

/// The AutoCAD colour index for an ink.
pub fn dxf_color(ink: Ink) -> i32 {
    match ink {
        Text => 7,
        Muted => 8,
        Accent => 5,
        Warn => 30,
        Danger => 1,
        Station => 3,
        Geology => 6,
    }
}

/// The DXF layer a type exports to, split by trip (`-<trip>`) when one is given.
pub fn dxf_layer(key: &str, trip: Option<&str>) -> String {
    match trip.filter(|t| !t.trim().is_empty()) {
        Some(t) => format!("{}-{}", style(key).layer, t.trim()),
        None => style(key).layer.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique_and_known_types_resolve() {
        for (i, a) in PALETTE.iter().enumerate() {
            assert!(PALETTE[i + 1..].iter().all(|b| b.key != a.key), "{}", a.key);
            assert_eq!(style(a.key).key, a.key);
        }
    }

    #[test]
    fn a_dxf_layer_never_mixes_looks() {
        for a in PALETTE {
            for b in PALETTE.iter().filter(|b| b.layer == a.layer) {
                assert_eq!((a.ink, a.linetype), (b.ink, b.linetype), "{}", a.layer);
            }
        }
    }

    #[test]
    fn every_drawing_tool_is_in_the_palette() {
        for k in DRAW_TOOLS {
            assert_eq!(style(k).key, *k, "{k} has its own style");
        }
        assert_eq!(draw_tools().len(), DRAW_TOOLS.len());
    }

    #[test]
    fn the_tool_groups_hold_every_drawing_tool_once() {
        let mut grouped: Vec<&str> = TOOL_GROUPS
            .iter()
            .flat_map(|(_, t)| t.iter().copied())
            .collect();
        grouped.sort_unstable();
        let mut all: Vec<&str> = DRAW_TOOLS.to_vec();
        all.sort_unstable();
        assert_eq!(grouped, all);
    }

    #[test]
    fn every_line_pattern_a_type_uses_can_be_written_to_a_dxf() {
        for p in PALETTE.iter().filter(|p| p.linetype != Linetype::Solid) {
            assert!(
                LINETYPES.contains(&p.linetype),
                "{} uses a pattern the DXF table lacks",
                p.key
            );
        }
        for l in LINETYPES {
            // a dash, a gap, optionally dots: lengths alternate in sign, and a pattern is never empty
            let pat = l.pattern();
            assert!(
                pat.len() >= 2 && pat.iter().any(|x| *x > 0.0) && pat.iter().any(|x| *x < 0.0),
                "{l:?}"
            );
        }
        let names: Vec<&str> = LINETYPES.iter().map(|l| l.dxf_name()).collect();
        let mut unique = names.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), names.len());
    }

    #[test]
    fn unknown_types_get_the_default_look() {
        assert_eq!(style("no-such-symbol").key, "default");
    }

    #[test]
    fn opacities_and_weights_are_sane() {
        for p in PALETTE {
            assert!((0.0..=1.0).contains(&p.opacity) && (0.0..=1.0).contains(&p.under_opacity));
            assert!(p.weight > 0.0 && p.under_opacity <= p.opacity, "{}", p.key);
        }
    }

    #[test]
    fn layers_come_from_type_and_optionally_trip() {
        assert_eq!(dxf_layer("wall", None), "CAVE-WALLS");
        assert_eq!(dxf_layer("wall", Some(" T1 ")), "CAVE-WALLS-T1");
        assert_eq!(dxf_layer("wall", Some("  ")), "CAVE-WALLS");
    }
}
