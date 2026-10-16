//! Sheets as authored data: a sheet is a named paper holding viewframes (windows onto the map, each
//! with its own centre, scale and twist), the title block, a scale bar and a north arrow per viewframe,
//! and, for a big cave, the match lines to its neighbours. Replaces the one-sheet-from-browser-settings
//! model. Pure: a sheet definition and what the notebook knows go in, shapes and viewframe placements
//! come out, so screen, PNG and PDF all draw the same sheet.
//!
//! The ideas are the old app's viewport layouts (`CsLayoutGen`, `CsSheetTile`, `CsTitleBlock`), without
//! its file-per-sheet or its CAD plumbing: a viewframe is plain data, and the map is drawn through it at
//! plot time.

use crate::tile::{self, Layout};
use crate::title::{self, Values};
use crate::{
    bar_for, furniture, papers, piece_shapes, title_height, Furniture, North, Piece, Shape, Title,
    Wants, CHAR_WIDTH, FT_PER_M, MARGIN_INCHES, TEXT,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn yes() -> bool {
    true
}

/// A window onto the map: what part of the cave shows in a box on the paper, at what scale and turn.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Viewframe {
    /// Stable id.
    pub id: String,
    /// The box on the paper, `[x1, y1, x2, y2]` inches from the paper's lower left.
    pub rect: [f64; 4],
    /// The map point at the box's middle, in metres.
    pub center: [f64; 2],
    /// Feet of cave per inch of paper.
    pub feet_per_inch: f64,
    /// How far the map is turned on the paper, degrees counter-clockwise.
    #[serde(default)]
    pub twist: f64,
    /// A locked frame cannot be panned or rescaled by accident.
    #[serde(default)]
    pub locked: bool,
    /// Kinds of thing (`wall`, `leg`, `line`, `symbol`...) not shown through this frame.
    #[serde(default)]
    pub hidden: Vec<String>,
    /// A polygon on the paper (inches) the frame is trimmed to; empty for the whole box.
    #[serde(default)]
    pub clip: Vec<[f64; 2]>,
    /// A name shown on the frame (a detail's letter, "PROFILE").
    #[serde(default)]
    pub label: String,
}

/// A piece of furniture on a sheet.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PieceDef {
    /// `title`, `bar` or `north`.
    pub kind: String,
    /// The viewframe a bar or arrow belongs to (the first one when empty).
    #[serde(default)]
    pub frame: String,
    /// Lower left corner on the paper in inches, when a person has moved it; `None` for the packed default.
    #[serde(default)]
    pub at: Option<[f64; 2]>,
}

/// A neighbouring sheet across one edge.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MatchEdge {
    /// `N`, `E`, `S` or `W`.
    pub edge: String,
    /// The neighbour's sheet name.
    pub to: String,
}

/// Where a generated sheet came from, so it can be regenerated.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Auto {
    /// The set of sheets it was made with.
    pub group: String,
    /// Its place in the grid ("B2").
    pub tile: String,
    /// A person has changed it by hand: regenerating leaves it alone.
    #[serde(default)]
    pub edited: bool,
    /// How far this sheet's neighbours overlap it, inches.
    #[serde(default)]
    pub overlap_in: f64,
    /// The neighbours.
    #[serde(default)]
    pub matches: Vec<MatchEdge>,
}

/// One sheet.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SheetDef {
    /// Stable id.
    pub id: String,
    /// The name people see (a tab name, the title block's Sheet line).
    pub name: String,
    /// A paper's name from [`papers`].
    pub paper: String,
    /// The paper is turned (portrait).
    #[serde(default)]
    pub turned: bool,
    /// Its viewframes.
    #[serde(default)]
    pub frames: Vec<Viewframe>,
    /// Its furniture.
    #[serde(default)]
    pub pieces: Vec<PieceDef>,
    /// The title block's words (a linked field follows the notebook).
    #[serde(default)]
    pub values: Values,
    /// Keep a band under the frame clear for the furniture (a single sheet); otherwise the map runs to the
    /// margin and the furniture sits over it on a white backing.
    #[serde(default = "yes")]
    pub band: bool,
    /// Set on a sheet made by [`generate`].
    #[serde(default)]
    pub auto: Option<Auto>,
}

/// What the notebook knows and the survey's compass, at render time.
#[derive(Clone, Debug, Default)]
pub struct Context {
    /// What the notebook can fill in ([`title::auto_fill`]).
    pub filled: BTreeMap<String, String>,
    /// Magnetic declination in degrees, east positive.
    pub declination: Option<f64>,
}

/// A viewframe placed on the paper, ready to draw the map through.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Placed {
    /// The frame's id.
    pub id: String,
    /// Its box in inches.
    pub rect: [f64; 4],
    /// Its trim polygon in inches (empty: the box).
    pub clip: Vec<[f64; 2]>,
    /// The map point at the box's middle, metres.
    pub center: [f64; 2],
    /// Inches of paper per metre of cave.
    pub k: f64,
    /// Turn in degrees counter-clockwise.
    pub twist: f64,
    /// Kinds not shown through the frame.
    pub hidden: Vec<String>,
    /// Its name.
    pub label: String,
}

impl Placed {
    /// Where a map point lands on the paper, inches: the frame's middle plus the point's offset from the
    /// frame's centre, turned and scaled.
    pub fn to_paper(&self, x: f64, y: f64) -> [f64; 2] {
        let (dx, dy) = ((x - self.center[0]) * self.k, (y - self.center[1]) * self.k);
        let t = self.twist.to_radians();
        let (c, s) = (t.cos(), t.sin());
        let mid = [(self.rect[0] + self.rect[2]) / 2.0, (self.rect[1] + self.rect[3]) / 2.0];
        [mid[0] + dx * c - dy * s, mid[1] + dx * s + dy * c]
    }
}

/// A sheet drawn.
#[derive(Clone, Debug, Serialize)]
pub struct Rendered {
    /// Paper width, inches.
    pub w: f64,
    /// Paper height, inches.
    pub h: f64,
    /// The viewframes.
    pub frames: Vec<Placed>,
    /// The furniture, border, match lines and labels.
    pub shapes: Vec<Shape>,
    /// Things a person should look at (a title block that does not fit, an unknown paper).
    pub warnings: Vec<String>,
}

/// A sheet's paper size in inches with the way up applied, or `None` for an unknown paper.
pub fn paper_size(def: &SheetDef) -> Option<(f64, f64)> {
    let p = papers().into_iter().find(|p| p.name == def.paper)?;
    Some(if def.turned { (p.h, p.w) } else { (p.w, p.h) })
}

/// The title block's words as printed: a person's typing wins, a linked field follows the notebook, and
/// the Sheet line is the sheet's name.
pub fn effective_values(def: &SheetDef, filled: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut v = title::plain(&def.values);
    let mut merged = def.values.clone();
    title::sync(&mut merged, filled);
    for f in title::FIELDS.iter().filter(|f| f.linked) {
        if let Some(m) = merged.get(f.id) {
            v.insert(f.id.to_string(), m.text.clone());
        }
    }
    if !def.name.is_empty() {
        v.insert("sheetNumber".to_string(), def.name.clone());
    }
    v
}

fn title_words(values: &BTreeMap<String, String>) -> (String, Vec<String>) {
    let lines = title::title_lines(values);
    let mut name = String::new();
    let mut rest = Vec::new();
    for (n, l) in lines.iter().enumerate() {
        if n == 0 {
            name = l.text.clone();
        } else {
            rest.push(l.text.clone());
        }
    }
    (name, rest)
}

/// Where each piece goes on this sheet, and how tall the footer band is (0 when `band` is off).
fn pack(def: &SheetDef, w: f64, th: f64) -> Furniture {
    let has = |k: &str| def.pieces.iter().any(|p| p.kind == k);
    furniture(
        w,
        Wants {
            title: has("title"),
            bar: has("bar"),
            north: has("north"),
        },
        th,
    )
}

/// The box a frame fills by default on a sheet: inside the margin, above the furniture band when there is one.
pub fn default_rect(def: &SheetDef, values: &BTreeMap<String, String>) -> Option<[f64; 4]> {
    let (w, h) = paper_size(def)?;
    let (name, lines) = title_words(values);
    let th = title_height(&Title { name: &name, lines: &lines });
    let band = if def.band { pack(def, w, th).footer } else { 0.0 };
    let m = MARGIN_INCHES;
    Some([m, m + band, w - m, h - m])
}

/// Draws a sheet: where each viewframe sits and the shapes of everything that is not the map.
pub fn render(def: &SheetDef, ctx: &Context) -> Option<Rendered> {
    let (w, h) = paper_size(def)?;
    let m = MARGIN_INCHES;
    let values = effective_values(def, &ctx.filled);
    let (name, lines) = title_words(&values);
    let words = Title { name: &name, lines: &lines };
    let th = title_height(&words);
    let packed = pack(def, w, th);
    let mut warnings = Vec::new();
    let mut shapes = vec![Shape::Rect { a: [m, m], b: [w - m, h - m] }];
    let frames: Vec<Placed> = def
        .frames
        .iter()
        .map(|f| Placed {
            id: f.id.clone(),
            rect: f.rect,
            clip: f.clip.clone(),
            center: f.center,
            k: FT_PER_M / f.feet_per_inch,
            twist: f.twist,
            hidden: f.hidden.clone(),
            label: f.label.clone(),
        })
        .collect();
    for d in &def.pieces {
        let Some(mut piece) = packed.pieces.iter().find(|p| p.kind == d.kind.as_str()).cloned() else {
            continue;
        };
        if let Some(at) = d.at {
            piece.x = at[0] - m;
            piece.y = at[1] - m;
        }
        let frame = def
            .frames
            .iter()
            .find(|f| f.id == d.frame)
            .or_else(|| def.frames.first());
        let fpi = frame.map_or(50.0, |f| f.feet_per_inch);
        let north = North {
            twist: frame.map_or(0.0, |f| f.twist),
            declination: ctx.declination,
        };
        let (x, y) = (m + piece.x, m + piece.y);
        let pad = 0.05;
        shapes.push(Shape::Back {
            a: [x - pad, y - pad],
            b: [x + piece.w + pad, y + piece.h + pad],
        });
        piece_shapes(&mut shapes, &piece, [m, m], &words, fpi, &north);
        if x < m - 1e-9 || y < m - 1e-9 || x + piece.w > w - m + 1e-9 || y + piece.h > h - m + 1e-9 {
            warnings.push(format!("The {} runs past the margin.", d.kind));
        }
    }
    if let Some(a) = &def.auto {
        corner_and_matches(&mut shapes, def, a, w, h);
    }
    for f in &def.frames {
        if !f.label.is_empty() {
            shapes.push(Shape::Text {
                at: [f.rect[0] + 0.08, f.rect[3] - 0.08 - TEXT.heading],
                h: TEXT.heading,
                s: f.label.to_uppercase(),
            });
        }
    }
    Some(Rendered { w, h, frames, shapes, warnings })
}

fn text_len(s: &str, h: f64) -> f64 {
    s.chars().count() as f64 * h * CHAR_WIDTH
}

/// "SHEET A1" in the corner and a dashed match line with its caption along every edge that has a neighbour.
fn corner_and_matches(out: &mut Vec<Shape>, def: &SheetDef, a: &Auto, w: f64, h: f64) {
    let m = MARGIN_INCHES;
    if def.frames.is_empty() {
        return;
    }
    let corner = format!("SHEET {}", def.name.to_uppercase());
    let back = text_len(&corner, TEXT.heading) + 0.2;
    out.push(Shape::Back {
        a: [w - m - back - 0.05, h - m - TEXT.heading - 0.2],
        b: [w - m - 0.05, h - m - 0.05],
    });
    out.push(Shape::Text {
        at: [w - m - back + 0.05, h - m - 0.15 - TEXT.heading * 0.7],
        h: TEXT.heading,
        s: corner,
    });
    let r = def.frames[0].rect;
    let half = a.overlap_in / 2.0;
    let t = TEXT.small;
    for e in &a.matches {
        let text = tile::match_text(&e.to);
        let len = text_len(&text, t);
        match e.edge.as_str() {
            "E" => {
                let x = r[2] - half;
                out.push(Shape::Dash { a: [x, r[1]], b: [x, r[3]] });
                out.push(Shape::Turned { at: [x - 0.06, (r[1] + r[3]) / 2.0 - len / 2.0], h: t, s: text, deg: 90.0 });
            }
            "W" => {
                let x = r[0] + half;
                out.push(Shape::Dash { a: [x, r[1]], b: [x, r[3]] });
                out.push(Shape::Turned { at: [x + 0.06 + t, (r[1] + r[3]) / 2.0 - len / 2.0], h: t, s: text, deg: 90.0 });
            }
            "N" => {
                let y = r[3] - half;
                out.push(Shape::Dash { a: [r[0], y], b: [r[2], y] });
                out.push(Shape::Text { at: [(r[0] + r[2]) / 2.0 - len / 2.0, y - 0.06 - t], h: t, s: text });
            }
            _ => {
                let y = r[1] + half;
                out.push(Shape::Dash { a: [r[0], y], b: [r[2], y] });
                out.push(Shape::Text { at: [(r[0] + r[2]) / 2.0 - len / 2.0, y + 0.06], h: t, s: text });
            }
        }
    }
}

/// What to generate.
#[derive(Clone, Debug)]
pub struct Gen<'a> {
    /// The grid ([`tile::layout`], in metres, with the scale in metres per inch).
    pub layout: &'a Layout,
    /// A paper's name from [`papers`].
    pub paper: &'a str,
    /// Feet of cave per inch of paper.
    pub feet_per_inch: f64,
    /// The set these sheets belong to.
    pub group: &'a str,
    /// The words of the title block (kept by every sheet; only the first prints it).
    pub values: &'a Values,
    /// A scale bar on every sheet.
    pub bar: bool,
    /// A north arrow on every sheet.
    pub north: bool,
    /// A title block on the first sheet.
    pub title: bool,
    /// Footer band kept clear under each frame, inches (0: furniture over the map).
    pub footer_inches: f64,
}

/// The id of a generated sheet.
pub fn generated_id(group: &str, tile: &str) -> String {
    format!("{group}-{}", tile.to_lowercase())
}

/// One sheet per tile of the grid, each with a viewframe over its map, a scale bar and a north arrow, the
/// first with the title block, and match lines to the neighbours.
pub fn generate(g: &Gen) -> Vec<SheetDef> {
    let m = MARGIN_INCHES;
    let tiled = g.layout.tiled;
    let overlap_in = if g.layout.overlap > 0.0 && g.layout.tiles.len() > 1 {
        // the layout's overlap is in model units; the paper is at a known scale
        g.layout.overlap / (g.feet_per_inch * crate::M_PER_FT)
    } else {
        0.0
    };
    let Some(paper) = papers().into_iter().find(|p| p.name == g.paper) else {
        return Vec::new();
    };
    let (pw, ph) = if g.layout.turned { (paper.h, paper.w) } else { (paper.w, paper.h) };
    g.layout
        .tiles
        .iter()
        .map(|t| {
            let mut pieces = Vec::new();
            if g.title && t.title {
                pieces.push(PieceDef { kind: "title".into(), frame: String::new(), at: None });
            }
            for (on, k) in [(g.bar, "bar"), (g.north, "north")] {
                if on {
                    pieces.push(PieceDef { kind: k.into(), frame: String::new(), at: None });
                }
            }
            let name = if tiled { t.id.clone() } else { "Plan".to_string() };
            let c = t.map.centre();
            SheetDef {
                id: generated_id(g.group, &t.id),
                name,
                paper: g.paper.to_string(),
                turned: g.layout.turned,
                frames: vec![Viewframe {
                    id: "main".into(),
                    rect: [m, m + g.footer_inches, pw - m, ph - m],
                    center: c,
                    feet_per_inch: g.feet_per_inch,
                    twist: 0.0,
                    locked: false,
                    hidden: Vec::new(),
                    clip: Vec::new(),
                    label: String::new(),
                }],
                pieces,
                values: g.values.clone(),
                band: g.footer_inches > 0.0,
                auto: Some(Auto {
                    group: g.group.to_string(),
                    tile: t.id.clone(),
                    edited: false,
                    overlap_in,
                    matches: t
                        .matches
                        .iter()
                        .map(|e| MatchEdge { edge: e.edge.to_string(), to: e.to.clone() })
                        .collect(),
                }),
            }
        })
        .collect()
}

/// A change to the stored sheets.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    /// Write this sheet.
    Put(Box<SheetDef>),
    /// Delete the sheet with this id.
    Remove(String),
}

/// Brings the stored sheets of one group up to date with a fresh generation: a sheet a person has edited
/// is left alone (and reported), the rest are rewritten, and generated sheets the grid no longer has go.
pub fn merge(existing: &[SheetDef], generated: Vec<SheetDef>, group: &str) -> (Vec<Change>, Vec<String>) {
    let mut changes = Vec::new();
    let mut kept = Vec::new();
    let ids: Vec<&str> = generated.iter().map(|s| s.id.as_str()).collect();
    for e in existing {
        let Some(a) = &e.auto else { continue };
        if a.group == group && !ids.contains(&e.id.as_str()) {
            if a.edited {
                kept.push(e.id.clone());
            } else {
                changes.push(Change::Remove(e.id.clone()));
            }
        }
    }
    for s in generated {
        match existing.iter().find(|e| e.id == s.id) {
            Some(e) if e.auto.as_ref().is_some_and(|a| a.edited) => kept.push(e.id.clone()),
            Some(e) if *e == s => {}
            _ => changes.push(Change::Put(Box::new(s))),
        }
    }
    (changes, kept)
}

/// A new sheet with one viewframe over the whole box, the way a person adds one by hand.
pub fn blank(id: &str, name: &str, paper: &str, turned: bool, center: [f64; 2], feet_per_inch: f64) -> Option<SheetDef> {
    let mut def = SheetDef {
        id: id.to_string(),
        name: name.to_string(),
        paper: paper.to_string(),
        turned,
        frames: Vec::new(),
        pieces: ["title", "bar", "north"]
            .iter()
            .map(|k| PieceDef { kind: (*k).into(), frame: String::new(), at: None })
            .collect(),
        values: Values::new(),
        band: true,
        auto: None,
    };
    let rect = default_rect(&def, &effective_values(&def, &BTreeMap::new()))?;
    def.frames.push(Viewframe {
        id: "main".into(),
        rect,
        center,
        feet_per_inch,
        twist: 0.0,
        locked: false,
        hidden: Vec::new(),
        clip: Vec::new(),
        label: String::new(),
    });
    Some(def)
}

/// The scale bar's printed length in inches for a viewframe (what its drawn blocks add up to).
pub fn bar_inches(feet_per_inch: f64) -> f64 {
    let b = bar_for(feet_per_inch);
    b.per_block_feet / feet_per_inch * f64::from(b.blocks)
}

/// Is a piece's box (as packed or moved) within the margin? Used by tests and the editor.
pub fn piece_fits(p: &Piece, paper_w: f64, paper_h: f64) -> bool {
    let m = MARGIN_INCHES;
    m + p.x >= m - 1e-9 && m + p.y >= m - 1e-9 && m + p.x + p.w <= paper_w - m + 1e-9 && m + p.y + p.h <= paper_h - m + 1e-9
}
