//! Laying a cave out over several sheets, ported from the old `CsSheetTile`.
//!
//! One sheet is a viewframe the cartographer places over the map. Where the cave runs past its margin,
//! neighbouring sheets of the same size appear, matched along a MATCH LINE. Each sheet owns a *core*
//! (cores butt exactly, so every point of the cave belongs to one sheet) and prints a strip half the
//! overlap wide past every edge of its core, so a passage that crosses a match line reads on both.
//! Sheets are numbered by grid: rows lettered from the north, columns numbered from the west, and a
//! cell the cave does not touch is not built but keeps its place in the numbering.
//!
//! Units: model units (metres in the app). `scale` is model units per inch of paper; every box is in
//! model units. Callers convert at the edge.

use crate::MARGIN_INCHES;
use serde::{Deserialize, Serialize};

/// How far neighbouring tiles overlap, in inches of paper.
pub const OVERLAP_INCHES: f64 = 0.5;
/// The most sheets one cave is ever laid over. A scale far too large for the cave would otherwise ask
/// for tens of thousands of cells; past this the layout comes back `too_many`.
pub const MAX_SHEETS: usize = 150;

/// An axis-aligned box in model units.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BBox {
    /// West edge.
    pub min_x: f64,
    /// South edge.
    pub min_y: f64,
    /// East edge.
    pub max_x: f64,
    /// North edge.
    pub max_y: f64,
}

impl BBox {
    /// Width.
    pub fn w(&self) -> f64 {
        self.max_x - self.min_x
    }
    /// Height.
    pub fn h(&self) -> f64 {
        self.max_y - self.min_y
    }
    /// The middle.
    pub fn centre(&self) -> [f64; 2] {
        [
            (self.min_x + self.max_x) / 2.0,
            (self.min_y + self.max_y) / 2.0,
        ]
    }
}

/// Row 0 is "A"; row 26 is "AA".
pub fn row_letter(row: usize) -> String {
    let mut n = row as i64;
    let mut out = String::new();
    loop {
        out.insert(0, (b'A' + (n % 26) as u8) as char);
        n = n / 26 - 1;
        if n < 0 {
            break;
        }
    }
    out
}

/// "B2": row letter, then 1-based column.
pub fn id_of(row: usize, col: usize) -> String {
    format!("{}{}", row_letter(row), col + 1)
}

/// One axis of the grid: how many tiles, the size of a core, how far a tile prints past its core and
/// the whole distance covered.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Axis {
    /// Tiles along the axis.
    pub count: usize,
    /// One core's size.
    pub core_size: f64,
    /// How far a tile prints past each edge of its core (0 for a single tile).
    pub expand: f64,
    /// The distance the grid covers.
    pub span: f64,
}

/// One axis of the grid for a cave `cave_size` long, a tile map `map_size` long and an overlap.
pub fn axis(cave_size: f64, map_size: f64, overlap: f64) -> Axis {
    if cave_size <= map_size + 1e-9 {
        return Axis {
            count: 1,
            core_size: map_size,
            expand: 0.0,
            span: map_size,
        };
    }
    let core_size = map_size - overlap;
    let count = (((cave_size - overlap) / core_size - 1e-9).ceil() as usize).max(2);
    Axis {
        count,
        core_size,
        expand: overlap / 2.0,
        span: count as f64 * core_size + overlap,
    }
}

/// Do two boxes share any area or edge?
pub fn meet(a: &BBox, b: &BBox) -> bool {
    a.min_x <= b.max_x && a.max_x >= b.min_x && a.min_y <= b.max_y && a.max_y >= b.min_y
}

/// A box clipped to another, or `None` when they do not meet.
pub fn clip(a: &BBox, b: &BBox) -> Option<BBox> {
    let r = BBox {
        min_x: a.min_x.max(b.min_x),
        max_x: a.max_x.min(b.max_x),
        min_y: a.min_y.max(b.min_y),
        max_y: a.max_y.min(b.max_y),
    };
    (r.min_x <= r.max_x && r.min_y <= r.max_y).then_some(r)
}

/// Is box `part` wholly inside the UNION of the rectangles? Subtracts one rectangle at a time and
/// asks the same of what is left, so a piece that straddles two sheets' maps is recognised as printed.
pub fn covered(part: &BBox, rects: &[BBox]) -> bool {
    let near: Vec<BBox> = rects
        .iter()
        .filter(|r| clip(part, r).is_some())
        .copied()
        .collect();
    covered_from(part, &near, 0)
}

fn covered_from(part: &BBox, rects: &[BBox], from: usize) -> bool {
    let e = 1e-7 * (1.0 + part.w().abs() + part.h().abs());
    let rects = &rects[from.min(rects.len())..];
    let near: Vec<BBox> = rects
        .iter()
        .filter(|r| clip(part, r).is_some())
        .copied()
        .collect();
    for (i, r) in near.iter().enumerate() {
        let Some(c) = clip(part, r) else { continue };
        let mut rest = Vec::new();
        if c.min_x - part.min_x > e {
            rest.push(BBox {
                min_x: part.min_x,
                max_x: c.min_x,
                min_y: part.min_y,
                max_y: part.max_y,
            });
        }
        if part.max_x - c.max_x > e {
            rest.push(BBox {
                min_x: c.max_x,
                max_x: part.max_x,
                min_y: part.min_y,
                max_y: part.max_y,
            });
        }
        if c.min_y - part.min_y > e {
            rest.push(BBox {
                min_x: c.min_x,
                max_x: c.max_x,
                min_y: part.min_y,
                max_y: c.min_y,
            });
        }
        if part.max_y - c.max_y > e {
            rest.push(BBox {
                min_x: c.min_x,
                max_x: c.max_x,
                min_y: c.max_y,
                max_y: part.max_y,
            });
        }
        return rest.iter().all(|q| covered_from(q, &near, i + 1));
    }
    false
}

/// What to lay out.
#[derive(Clone, Debug)]
pub struct TileInput {
    /// The cave's box.
    pub cave: BBox,
    /// The paper's width and height in inches (landscape, before turning).
    pub sheet: (f64, f64),
    /// Model units per inch of paper.
    pub scale: f64,
    /// The paper is turned (portrait).
    pub turned: bool,
    /// A band to keep clear under every sheet's map, in inches (0 for none).
    pub footer_inches: f64,
    /// Overlap in inches; `None` for [`OVERLAP_INCHES`].
    pub overlap_inches: Option<f64>,
    /// What the cave is made of (empty: the cave's own box stands in).
    pub occupied: Vec<BBox>,
    /// The viewframe slid by hand, in inches of paper.
    pub shift_inches: (f64, f64),
}

/// A neighbour across one edge of a tile.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Match {
    /// Which edge of this tile (`N`, `E`, `S`, `W`).
    pub edge: &'static str,
    /// The neighbour's sheet id.
    pub to: String,
    /// The line's ends.
    pub x1: f64,
    /// The line's ends.
    pub y1: f64,
    /// The line's ends.
    pub x2: f64,
    /// The line's ends.
    pub y2: f64,
}

/// The paper behind a tile's map, in model units.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct PaperBox {
    /// West.
    pub min_x: f64,
    /// East.
    pub max_x: f64,
    /// South.
    pub min_y: f64,
    /// North.
    pub max_y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
    /// The footer band.
    pub footer: f64,
    /// The margin.
    pub margin: f64,
}

/// One sheet of the grid.
#[derive(Clone, Debug, Serialize)]
pub struct Tile {
    /// "A1", "B2"...
    pub id: String,
    /// Row from the north, 0-based.
    pub row: usize,
    /// Column from the west, 0-based.
    pub col: usize,
    /// Cell column relative to the viewframe's own cell.
    pub i: i64,
    /// Cell row relative to the viewframe's own cell.
    pub j: i64,
    /// Carries the title block (the first sheet in reading order).
    pub title: bool,
    /// Is the viewframe's own sheet.
    pub primary: bool,
    /// What the sheet is responsible for.
    pub core: BBox,
    /// What the sheet prints (core plus half the overlap all round).
    pub map: BBox,
    /// The paper around the map.
    pub paper: PaperBox,
    /// Neighbours across its edges.
    pub matches: Vec<Match>,
    #[serde(skip)]
    pieces: Vec<BBox>,
}

/// A match line between two sheets.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MatchLine {
    /// One sheet.
    pub a: String,
    /// The other.
    pub b: String,
    /// The line's ends.
    pub x1: f64,
    /// The line's ends.
    pub y1: f64,
    /// The line's ends.
    pub x2: f64,
    /// The line's ends.
    pub y2: f64,
}

/// The viewframe's own cell, whether or not any cave is in it.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Origin {
    /// Its core.
    pub core: BBox,
    /// Its map.
    pub map: BBox,
    /// Its paper.
    pub paper: PaperBox,
}

/// The whole grid.
#[derive(Clone, Debug, Serialize)]
pub struct Layout {
    /// More than one sheet.
    pub tiled: bool,
    /// Would have taken more than [`MAX_SHEETS`].
    pub too_many: bool,
    /// How many it would have taken (when `too_many`).
    pub count: usize,
    /// Rows.
    pub rows: usize,
    /// Columns.
    pub cols: usize,
    /// The paper is turned.
    pub turned: bool,
    /// The sheets, in reading order.
    pub tiles: Vec<Tile>,
    /// Match lines.
    pub match_lines: Vec<MatchLine>,
    /// The span of all the paper.
    pub span: Option<BBox>,
    /// The overlap in model units.
    pub overlap: f64,
    /// The viewframe's centre.
    pub anchor: [f64; 2],
    /// The viewframe's own cell.
    pub origin: Option<Origin>,
}

fn too_many(count: usize, turned: bool) -> Layout {
    Layout {
        tiled: true,
        too_many: true,
        count,
        rows: 0,
        cols: 0,
        turned,
        tiles: Vec::new(),
        match_lines: Vec::new(),
        span: None,
        overlap: 0.0,
        anchor: [0.0, 0.0],
        origin: None,
    }
}

struct Cell {
    i: i64,
    j: i64,
    pieces: Vec<BBox>,
}

/// Lays the cave over sheets at one orientation.
pub fn layout_one(o: &TileInput) -> Layout {
    let scale = o.scale;
    let margin = MARGIN_INCHES;
    let turned = o.turned;
    let footer = if o.footer_inches > 0.0 {
        o.footer_inches
    } else {
        0.0
    };
    let (w, h) = if turned {
        (o.sheet.1, o.sheet.0)
    } else {
        (o.sheet.0, o.sheet.1)
    };
    let map_w = ((w - margin * 2.0) * scale).max(1e-6);
    let map_h = ((h - margin * 2.0 - footer) * scale).max(1e-6);
    let overlap = (o.overlap_inches.unwrap_or(OVERLAP_INCHES) * scale)
        .min(map_w.min(map_h) * 0.25)
        .max(0.0);
    let (core_w, core_h) = (map_w - overlap, map_h - overlap);
    let half = overlap / 2.0;
    let cave = o.cave;
    let (cave_w, cave_h) = (cave.w(), cave.h());

    // THE VIEWFRAME (cell 0,0): the cave centred in it along an axis it fits, its north-west corner on
    // the cave's along one it does not, which is the fewest sheets.
    let mut centre_x = if cave_w <= map_w + 1e-9 {
        (cave.min_x + cave.max_x) / 2.0
    } else {
        cave.min_x + core_w / 2.0 + half
    };
    let mut centre_y = if cave_h <= map_h + 1e-9 {
        (cave.min_y + cave.max_y) / 2.0
    } else {
        cave.max_y - core_h / 2.0 - half
    };
    centre_x += o.shift_inches.0 * scale;
    centre_y += o.shift_inches.1 * scale;

    let core_of = |i: i64, j: i64| BBox {
        min_x: centre_x - core_w / 2.0 + i as f64 * core_w,
        max_x: centre_x + core_w / 2.0 + i as f64 * core_w,
        min_y: centre_y - core_h / 2.0 + j as f64 * core_h,
        max_y: centre_y + core_h / 2.0 + j as f64 * core_h,
    };
    let map_of = |c: &BBox| BBox {
        min_x: c.min_x - half,
        max_x: c.max_x + half,
        min_y: c.min_y - half,
        max_y: c.max_y + half,
    };
    let occ: Vec<BBox> = if o.occupied.is_empty() {
        vec![cave]
    } else {
        o.occupied.clone()
    };

    // WHICH CELLS THE CAVE REACHES, and for each the pieces of it that reach there.
    let origin_core = core_of(0, 0);
    let mut cells: Vec<Cell> = Vec::new();
    let mut at: std::collections::HashMap<(i64, i64), usize> = std::collections::HashMap::new();
    for b in &occ {
        let i0 = ((b.min_x - origin_core.min_x) / core_w).floor() as i64;
        let i1 = ((b.max_x - origin_core.min_x) / core_w).floor() as i64;
        let j0 = ((b.min_y - origin_core.min_y) / core_h).floor() as i64;
        let j1 = ((b.max_y - origin_core.min_y) / core_h).floor() as i64;
        if (i1 - i0 + 1) * (j1 - j0 + 1) > 4000 {
            continue; // a piece larger than any sensible grid
        }
        for ci in i0..=i1 {
            for cj in j0..=j1 {
                let idx = *at.entry((ci, cj)).or_insert_with(|| {
                    cells.push(Cell {
                        i: ci,
                        j: cj,
                        pieces: Vec::new(),
                    });
                    cells.len() - 1
                });
                if let Some(part) = clip(b, &core_of(ci, cj)) {
                    cells[idx].pieces.push(part);
                }
            }
        }
    }
    if cells.len() > MAX_SHEETS * 6 {
        return too_many(cells.len(), turned);
    }
    cells.sort_by(|a, c| {
        let da = a.i.abs() + a.j.abs();
        let dc = c.i.abs() + c.j.abs();
        da.cmp(&dc)
            .then_with(|| c.j.cmp(&a.j))
            .then_with(|| a.i.cmp(&c.i))
    });

    // KEEP A CELL IF ITS CORE HOLDS CAVE THAT THE SHEETS ALREADY KEPT DO NOT PRINT between them.
    struct Kept {
        i: i64,
        j: i64,
        core: BBox,
        map: BBox,
        pieces: Vec<BBox>,
    }
    let mut kept: Vec<Kept> = Vec::new();
    for cell in &cells {
        if kept.len() > MAX_SHEETS {
            break;
        }
        let maps: Vec<BBox> = kept.iter().map(|k| k.map).collect();
        if cell.pieces.iter().any(|p| !covered(p, &maps)) {
            let core = core_of(cell.i, cell.j);
            kept.push(Kept {
                i: cell.i,
                j: cell.j,
                core,
                map: map_of(&core),
                pieces: cell.pieces.clone(),
            });
        }
    }
    let mut back = kept.len();
    while back > 0 && kept.len() > 1 {
        back -= 1;
        let rest: Vec<BBox> = kept
            .iter()
            .enumerate()
            .filter(|(q, _)| *q != back)
            .map(|(_, k)| k.map)
            .collect();
        if kept[back].pieces.iter().all(|p| covered(p, &rest)) {
            kept.remove(back);
        }
    }
    if kept.len() > MAX_SHEETS {
        return too_many(kept.len(), turned);
    }
    if kept.is_empty() {
        let c0 = core_of(0, 0);
        kept.push(Kept {
            i: 0,
            j: 0,
            core: c0,
            map: map_of(&c0),
            pieces: Vec::new(),
        });
    }

    // NUMBERED BY GRID: rows from the north, columns from the west.
    let min_i = kept.iter().map(|k| k.i).min().unwrap_or(0);
    let max_j = kept.iter().map(|k| k.j).max().unwrap_or(0);
    let m_px = margin * scale;
    let f_px = footer * scale;
    let paper_of = |map: &BBox| PaperBox {
        min_x: map.min_x - m_px,
        max_x: map.max_x + m_px,
        min_y: map.min_y - m_px - f_px,
        max_y: map.max_y + m_px,
        width: map.w() + m_px * 2.0,
        height: map.h() + m_px * 2.0 + f_px,
        footer: f_px,
        margin: m_px,
    };
    let mut tiles: Vec<Tile> = kept
        .into_iter()
        .map(|k| {
            let row = (max_j - k.j) as usize;
            let col = (k.i - min_i) as usize;
            Tile {
                id: id_of(row, col),
                row,
                col,
                i: k.i,
                j: k.j,
                title: false,
                primary: k.i == 0 && k.j == 0,
                core: k.core,
                map: k.map,
                paper: paper_of(&k.map),
                matches: Vec::new(),
                pieces: k.pieces,
            }
        })
        .collect();
    let rows = tiles.iter().map(|t| t.row + 1).max().unwrap_or(0);
    let cols = tiles.iter().map(|t| t.col + 1).max().unwrap_or(0);
    tiles.sort_by(|a, c| a.row.cmp(&c.row).then(a.col.cmp(&c.col)));
    for (k, t) in tiles.iter_mut().enumerate() {
        t.title = k == 0;
    }
    if !tiles.iter().any(|t| t.primary) {
        tiles[0].primary = true;
    }

    // match lines, by what actually faces what
    let by_cell: std::collections::HashMap<(i64, i64), usize> = tiles
        .iter()
        .enumerate()
        .map(|(n, t)| ((t.i, t.j), n))
        .collect();
    let mut match_lines = Vec::new();
    for k in 0..tiles.len() {
        let (ai, aj, aid, core) = (tiles[k].i, tiles[k].j, tiles[k].id.clone(), tiles[k].core);
        if let Some(&e) = by_cell.get(&(ai + 1, aj)) {
            let eid = tiles[e].id.clone();
            let (x1, y1, x2, y2) = (core.max_x, core.min_y, core.max_x, core.max_y);
            tiles[k].matches.push(Match {
                edge: "E",
                to: eid.clone(),
                x1,
                y1,
                x2,
                y2,
            });
            tiles[e].matches.push(Match {
                edge: "W",
                to: aid.clone(),
                x1,
                y1,
                x2,
                y2,
            });
            match_lines.push(MatchLine {
                a: aid.clone(),
                b: eid,
                x1,
                y1,
                x2,
                y2,
            });
        }
        if let Some(&s) = by_cell.get(&(ai, aj - 1)) {
            let sid = tiles[s].id.clone();
            let (x1, y1, x2, y2) = (core.min_x, core.min_y, core.max_x, core.min_y);
            tiles[k].matches.push(Match {
                edge: "S",
                to: sid.clone(),
                x1,
                y1,
                x2,
                y2,
            });
            tiles[s].matches.push(Match {
                edge: "N",
                to: aid.clone(),
                x1,
                y1,
                x2,
                y2,
            });
            match_lines.push(MatchLine {
                a: aid,
                b: sid,
                x1,
                y1,
                x2,
                y2,
            });
        }
    }
    let mut span = BBox {
        min_x: f64::INFINITY,
        min_y: f64::INFINITY,
        max_x: f64::NEG_INFINITY,
        max_y: f64::NEG_INFINITY,
    };
    for t in &tiles {
        span.min_x = span.min_x.min(t.paper.min_x);
        span.min_y = span.min_y.min(t.paper.min_y);
        span.max_x = span.max_x.max(t.paper.max_x);
        span.max_y = span.max_y.max(t.paper.max_y);
    }
    let origin_map = map_of(&origin_core);
    Layout {
        tiled: tiles.len() > 1,
        too_many: false,
        count: tiles.len(),
        rows,
        cols,
        turned,
        match_lines,
        span: Some(span),
        overlap,
        anchor: [centre_x, centre_y],
        origin: Some(Origin {
            core: origin_core,
            map: origin_map,
            paper: paper_of(&origin_map),
        }),
        tiles,
    }
}

/// The layout at whichever paper orientation needs fewer sheets (not turned on a tie), unless the
/// orientation is forced with `force`.
pub fn layout(o: &TileInput, force: Option<bool>) -> Layout {
    if let Some(t) = force {
        return layout_one(&TileInput {
            turned: t,
            ..o.clone()
        });
    }
    let plain = layout_one(&TileInput {
        turned: false,
        ..o.clone()
    });
    let turned = layout_one(&TileInput {
        turned: true,
        ..o.clone()
    });
    let size = |l: &Layout| {
        if l.too_many {
            usize::MAX
        } else {
            l.tiles.len()
        }
    };
    if size(&turned) < size(&plain) {
        turned
    } else {
        plain
    }
}

/// The words printed on a sheet along the edge it shares.
pub fn match_text(to_id: &str) -> String {
    format!("MATCH LINE - SEE SHEET {to_id}")
}

/// A one-line description for the panel: "3 x 2 grid, 5 sheets (A1 to B3)".
pub fn describe(l: &Layout) -> String {
    if !l.tiled {
        return String::new();
    }
    if l.too_many {
        return format!("more than {MAX_SHEETS} sheets");
    }
    let (first, last) = (&l.tiles[0].id, &l.tiles[l.tiles.len() - 1].id);
    format!(
        "{} x {} grid, {} sheet{} ({} to {})",
        l.cols,
        l.rows,
        l.tiles.len(),
        if l.tiles.len() == 1 { "" } else { "s" },
        first,
        last
    )
}
