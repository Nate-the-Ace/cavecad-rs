//! What a Therion sketch's words mean here: the one place Therion's type names stop.
//!
//! The reader (`cave_formats::th2`) keeps Therion's own names (`wall`, `floor-step`, `stalactite`,
//! `u:bolt-hanger`). Each is turned here into something this app draws: a line of one of its own
//! types, a catalogue symbol, words, an area fill. Ported from the old app's CsSketch tables, onto
//! this app's smaller set of line types.
//!
//! `Skip` and `Unknown` draw the same thing (nothing, or a plain sketch line) and mean opposite
//! things. A skip is a decision: the centreline and splays are already in the survey data, an
//! invisible wall was asked not to be seen. An unknown is a gap in these tables, and is reported
//! by name so a caver can tell us about it.

/// What one sketch item becomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    /// A line of this type (`wall`, `ledge`, `sketch`, ...).
    Line(&'static str),
    /// A symbol: the old app's block name, looked up in the symbol catalogue.
    Symbol(&'static str),
    /// Words at the point.
    Text,
    /// An area filled with this fill (`fill-water`, ...).
    Fill(&'static str),
    /// Read and deliberately not drawn, with the reason.
    Skip(&'static str),
    /// Not in these tables: lines and area outlines are kept as plain sketch lines, points are
    /// left out; every one is reported.
    Unknown,
}

/// Therion's `type:subtype` shorthand, split (a caver's own `u:name` stays whole).
pub fn split(kind: &str, subtype: Option<&str>) -> (String, Option<String>) {
    match kind.split_once(':') {
        Some((k, sub)) if k != "u" => (k.to_string(), Some(sub.to_string())),
        _ => (kind.to_string(), subtype.map(str::to_string)),
    }
}

/// What a line becomes. A wall's subtype says how well it is known, so it is read first.
pub fn line(kind: &str, subtype: Option<&str>) -> Ink {
    let (kind, subtype) = split(kind, subtype);
    if kind == "wall" {
        return match subtype.as_deref() {
            Some("presumed" | "unsurveyed") => Ink::Line("presumed"),
            Some("underlying") => Ink::Line("underlying"),
            // there to close an area, not to be seen: Therion draws nothing for it either
            Some("invisible") => Ink::Skip("invisible"),
            Some("pit") => Ink::Line("pit"),
            Some("flowstone") => Ink::Line("flowstone"),
            // the rest say what the wall is made of (blocks, clay, ice, ...), which is drawn the same
            _ => Ink::Line("wall"),
        };
    }
    if kind == "border" && subtype.as_deref() == Some("invisible") {
        return Ink::Skip("invisible");
    }
    match kind.as_str() {
        "pit" => Ink::Line("pit"),
        "slope" => Ink::Line("slope"),
        "floor-step" | "ceiling-step" | "overhang" => Ink::Line("ledge"),
        "flowstone" => Ink::Line("flowstone"),
        "dripline" => Ink::Line("dripline"),
        "contour" => Ink::Line("contour"),
        "joint" => Ink::Line("joint"),
        "fault" => Ink::Line("fault"),
        "label" => Ink::Text,
        // drawn, with no type of their own here: plain sketch lines
        "rock-border" | "rock-edge" | "chimney" | "pit-chimney" | "floor-meander"
        | "ceiling-meander" | "gradient" | "water-flow" | "arrow" | "border" | "map-connection"
        | "low-ceiling" | "moonmilk" | "handrail" | "rope" | "fixed-ladder" | "rope-ladder"
        | "walkway" | "via-ferrata" | "steps" | "abyss-entrance" => Ink::Line("sketch"),
        // the centreline and splays came in as survey data, with their stations and readings
        "survey" | "u:splay" => Ink::Skip("centreline"),
        // where a cross section is cut: sections are placed on their own here
        "section" => Ink::Skip("section"),
        _ => Ink::Unknown,
    }
}

/// What a point becomes. Station markers and section references never get here: the reader pulls
/// them out, since they tie the sketch to the cave rather than draw on it.
pub fn point(kind: &str) -> Ink {
    let (kind, _) = split(kind, None);
    let block = match kind.as_str() {
        "stalactite" | "stalactites" | "disc-stalactite" | "disc-stalactites"
        | "ice-stalactite" => "SYM_STALACTITE",
        "soda-straw" => "SYM_SODA_STRAW",
        "stalagmite" | "stalagmites" | "disc-stalagmite" | "disc-stalagmites"
        | "ice-stalagmite" => "SYM_STALAGMITE",
        "stalactite-stalagmite"
        | "stalactites-stalagmites"
        | "pillar"
        | "disc-pillar"
        | "disc-pillars"
        | "ice-pillar"
        | "pillar-with-curtains"
        | "pillars-with-curtains" => "SYM_COLUMN",
        "curtain" | "curtains" => "SYM_DRAPERY",
        "flowstone" | "wall-calcite" => "SYM_FLOWSTONE",
        "flowstone-choke" => "SYM_FLOWSTONE_CHOKE",
        "gours" | "rimstone-dam" | "rimstone-pool" => "SYM_RIMSTONE_DAM",
        "moonmilk" | "popcorn" => "SYM_MOONMILK_POPCORN",
        "helictite" | "helictites" => "SYM_HELICTITE",
        "crystal" | "aragonite-crystal" => "SYM_CRYSTAL",
        "aragonite" => "SYM_ARAGONITE",
        "gypsum" => "SYM_GYPSUM",
        "gypsum-flower" => "SYM_GYPSUM_FLOWER",
        "cave-pearl" => "SYM_CAVE_PEARL",
        "pendant" => "SYM_PENDANT",
        "volcano" | "raft-cone" => "SYM_VOLCANO",
        "clay-tree" => "SYM_CLAY_TREE",
        "disk" => return Ink::Symbol("sym-uis-disk"),
        "blocks" | "debris" => "SYM_BREAKDOWN",
        "breakdown-choke" => "SYM_BREAKDOWN_CHOKE",
        "clay" | "mudcrack" | "mud" => "SYM_CLAY_MUD_TICK",
        "clay-choke" => "SYM_CLAY_CHOKE",
        "sand" | "pebbles" => "SYM_SAND_GRAVEL_DOT",
        "guano" => "SYM_GUANO",
        "ice" | "snow" => return Ink::Symbol("sym-uis-ice"),
        "entrance" => "SYM_ENTRANCE",
        "water-flow" => "SYM_FLOW_ARROW",
        "spring" => "SYM_SPRING",
        "sump" => "SYM_SUMP",
        "water-drip" | "water" => "SYM_DRIP_SEEP",
        "gradient" => "SYM_SLOPE_TICK",
        "steps" => "SYM_STEPS",
        "passage-height" => "SYM_CEILING_HEIGHT",
        "dig" => "SYM_DIG",
        "danger" => "SYM_DANGER",
        "narrow-end" => "SYM_NARROW_END",
        "low-end" => "SYM_LOW_END",
        "continuation" => "SYM_CONTINUATION",
        "anchor" => "SYM_BOLT",
        "rope" => "SYM_ROPE_DROP",
        "rope-ladder" => "SYM_CABLE_LADDER",
        "fixed-ladder" => "SYM_FIXED_LADDER",
        "traverse" => "SYM_TRAVERSE_LINE",
        "masonry" => "SYM_MASONRY",
        "archeo-material" => "SYM_ARCHEO_MATERIAL",
        "archeo-excavation" => "SYM_ARCHEO_EXCAVATION",
        "paleo-material" => "SYM_PALEO_MATERIAL",
        "bones" => "SYM_BONES",
        "human-bones" => "SYM_HUMAN_BONES",
        "root" => "SYM_ROOT",
        "vegetable-debris" => "SYM_VEGETABLE_DEBRIS",
        "seed-germination" | "seed-germ" => "SYM_SEED_GERMINATION",
        "tree-trunk" => "SYM_TREE_TRUNK",
        "bat" => "SYM_BAT",
        "altar" => "SYM_ALTAR",
        "ex-voto" => "SYM_EX_VOTO",
        "air-draught" => "SYM_AIR_DRAUGHT",
        "scallop" => "SYM_SCALLOP",
        "flute" => "SYM_FLUTE",
        "karren" => "SYM_KARREN",
        "anastomosis" => "SYM_ANASTOMOSIS",
        "bridge" => "SYM_BRIDGE",
        "walkway" => "SYM_WALKWAY",
        "handrail" => "SYM_HANDRAIL",
        "gate" => "SYM_GATE",
        "camp" => "SYM_CAMP",
        "nameplate" => "SYM_NAMEPLATE",
        // a measured height or elevation: its value, as words
        "label" | "remark" | "name" | "date" | "altitude" | "height" => return Ink::Text,
        // the drawing letters its own stations
        "station-name" | "station" => return Ink::Skip("station"),
        // xtherion's own scaffolding, meaning nothing outside the editor
        "dimensions" | "extra" | "no-equipment" | "map-connection" => return Ink::Skip("editor"),
        _ => return Ink::Unknown,
    };
    Ink::Symbol(block)
}

/// What an area becomes. Area Fill's patterns were drawn from the same UIS vocabulary, so this is
/// mostly a spelling change.
pub fn area(kind: &str) -> Ink {
    let (kind, _) = split(kind, None);
    match kind.as_str() {
        "water" => Ink::Fill("fill-water"),
        "sump" => Ink::Fill("fill-sump"),
        "sand" => Ink::Fill("fill-sand"),
        "clay" | "mudcrack" => Ink::Fill("fill-clay"),
        "pebbles" => Ink::Fill("fill-pebbles"),
        "debris" => Ink::Fill("fill-debris"),
        "blocks" => Ink::Fill("fill-blocks"),
        "bedrock" => Ink::Fill("fill-bedrock"),
        "flowstone" => Ink::Fill("fill-flowstone"),
        "moonmilk" => Ink::Fill("fill-moonmilk"),
        "ice" | "snow" => Ink::Fill("fill-ice"),
        "guano" => Ink::Fill("fill-guano"),
        "bones" => Ink::Fill("fill-bones"),
        _ => Ink::Unknown,
    }
}

/// The symbol catalogue's key for an `Ink::Symbol` (a block name or already a key).
pub fn symbol_key(block: &str) -> Option<&'static str> {
    if let Some(s) = cave_symbols::symbol(block) {
        return Some(&s.key);
    }
    cave_symbols::catalogue()
        .iter()
        .find(|s| s.block == block)
        .map(|s| s.key.as_str())
}

/// A point's `-scale` (`xs` .. `xl`, or a number) as a size multiplier.
pub fn point_scale(word: Option<&str>) -> f64 {
    match word.map(str::trim) {
        Some("xs") => 0.4,
        Some("s") => 0.7,
        Some("l") => 1.5,
        Some("xl") => 2.2,
        // a number is a multiplier too, held to what still reads as a symbol
        Some(w) => w
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite() && *v > 0.0)
            .map_or(1.0, |v| v.clamp(0.2, 5.0)),
        None => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_symbol_named_is_in_the_catalogue() {
        let names = [
            "stalactite",
            "soda-straw",
            "stalagmite",
            "pillar",
            "curtain",
            "flowstone",
            "flowstone-choke",
            "gours",
            "moonmilk",
            "helictite",
            "crystal",
            "aragonite",
            "gypsum",
            "gypsum-flower",
            "cave-pearl",
            "pendant",
            "volcano",
            "clay-tree",
            "disk",
            "blocks",
            "breakdown-choke",
            "clay",
            "clay-choke",
            "sand",
            "guano",
            "ice",
            "entrance",
            "water-flow",
            "spring",
            "sump",
            "water-drip",
            "gradient",
            "steps",
            "passage-height",
            "dig",
            "danger",
            "narrow-end",
            "low-end",
            "continuation",
            "anchor",
            "rope",
            "rope-ladder",
            "fixed-ladder",
            "traverse",
            "masonry",
            "archeo-material",
            "archeo-excavation",
            "paleo-material",
            "bones",
            "human-bones",
            "root",
            "vegetable-debris",
            "seed-germination",
            "tree-trunk",
            "bat",
            "altar",
            "ex-voto",
            "air-draught",
            "scallop",
            "flute",
            "karren",
            "anastomosis",
            "bridge",
            "walkway",
            "handrail",
            "gate",
            "camp",
            "nameplate",
        ];
        for n in names {
            let Ink::Symbol(b) = point(n) else {
                panic!("{n} is not a symbol")
            };
            assert!(symbol_key(b).is_some(), "{n}: {b} is not in the catalogue");
        }
    }

    #[test]
    fn every_line_type_and_fill_named_exists() {
        for (k, s) in [
            ("wall", None),
            ("wall", Some("presumed")),
            ("wall", Some("underlying")),
            ("pit", None),
            ("slope", None),
            ("floor-step", None),
            ("flowstone", None),
            ("dripline", None),
            ("contour", None),
            ("joint", None),
            ("fault", None),
            ("rock-border", None),
        ] {
            let Ink::Line(t) = line(k, s) else {
                panic!("{k} is not a line")
            };
            assert_eq!(cave_style::style(t).key, t, "{k}: {t} is not a line type");
        }
        for a in [
            "water",
            "sump",
            "sand",
            "clay",
            "pebbles",
            "debris",
            "blocks",
            "bedrock",
            "flowstone",
            "moonmilk",
            "ice",
            "guano",
            "bones",
        ] {
            assert!(
                matches!(area(a), Ink::Fill(f) if f.starts_with("fill-")),
                "{a}"
            );
        }
    }

    #[test]
    fn the_subtype_shorthand_is_read() {
        assert_eq!(line("wall:presumed", None), Ink::Line("presumed"));
        assert_eq!(line("wall:invisible", None), Ink::Skip("invisible"));
        assert_eq!(line("wall:blocks", None), Ink::Line("wall"));
        assert_eq!(line("border:invisible", None), Ink::Skip("invisible"));
        assert_eq!(line("u:splay", None), Ink::Skip("centreline"));
        assert_eq!(line("u:texture", None), Ink::Unknown);
        assert_eq!(point("air-draught:summer"), Ink::Symbol("SYM_AIR_DRAUGHT"));
        assert_eq!(point("station:fixed"), Ink::Skip("station"));
    }
}
