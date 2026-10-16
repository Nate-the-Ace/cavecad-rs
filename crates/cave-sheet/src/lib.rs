//! Sheets: a map on paper. The paper sizes, the plot scales, which scale fits a cave on a sheet, the
//! scale bar's divisions and where the title block, scale bar and north arrow go, ported from the old
//! `CsSheetSetup` and checked against its answers (`tests/golden.rs`). Like the old code it works in
//! feet of cave per inch of paper and inches on the sheet; callers convert at the edge. The sheet's
//! furniture comes out as plain shapes ([`furniture`]) so any output (screen, SVG, DXF, PDF) draws it.

use serde::Serialize;

pub mod sheetset;
pub mod tile;
pub mod title;

const M_PER_FT: f64 = 0.3048;
const MM: f64 = 1.0 / 25.4;
/// How much of the sheet is kept clear at every edge, in inches.
pub const MARGIN_INCHES: f64 = 0.5;
/// Width of the title block column, in inches.
pub const TITLE_INCHES: f64 = 6.0;
/// Printed text heights in inches: cave name, heading, body, small.
pub const TEXT: Text = Text {
    cave_name: 0.42,
    heading: 0.18,
    body: 0.14,
    small: 0.11,
};
/// Scale bar sizes in inches: its length, the depth of its blocks, how far its labels sit below.
pub const BAR: Bar = Bar {
    length: 3.0,
    height: 0.16,
    tick: 0.06,
};
/// How wide a character is, as a fraction of its height.
pub const CHAR_WIDTH: f64 = 0.74;
/// How far apart stacked lines sit, as a multiple of their height.
pub const LINE_SPACING: f64 = 1.9;

/// Printed text heights in inches.
pub struct Text {
    /// The cave's name.
    pub cave_name: f64,
    /// Headings.
    pub heading: f64,
    /// Body lines.
    pub body: f64,
    /// Small print.
    pub small: f64,
}

/// Scale bar sizes in inches.
pub struct Bar {
    /// How long the bar is drawn.
    pub length: f64,
    /// The depth of its blocks.
    pub height: f64,
    /// How far its labels hang below it.
    pub tick: f64,
}

/// A paper size, in inches (landscape: `w >= h`).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Paper {
    /// Its name, with the size.
    pub name: &'static str,
    /// Width in inches.
    pub w: f64,
    /// Height in inches.
    pub h: f64,
}

/// A plot scale.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ScaleRow {
    /// How it is written: `1" = 50 ft` or `1:500`.
    pub label: &'static str,
    /// Feet of cave per inch of paper.
    pub feet_per_inch: f64,
    /// True for a ratio scale (the bar counts metres).
    pub metric: bool,
}

const fn paper(name: &'static str, w: f64, h: f64) -> Paper {
    Paper { name, w, h }
}

/// The papers a map is plotted on, imperial first and ISO after.
pub fn papers() -> Vec<Paper> {
    vec![
        paper("ANSI A -- 11 x 8.5", 11.0, 8.5),
        paper("ANSI B -- 17 x 11", 17.0, 11.0),
        paper("ANSI C -- 22 x 17", 22.0, 17.0),
        paper("ARCH C -- 24 x 18", 24.0, 18.0),
        paper("ANSI D -- 34 x 22", 34.0, 22.0),
        paper("ARCH D -- 36 x 24", 36.0, 24.0),
        paper("ARCH E -- 48 x 36", 48.0, 36.0),
        paper("ISO A4 -- 297 x 210 mm", 297.0 * MM, 210.0 * MM),
        paper("ISO A3 -- 420 x 297 mm", 420.0 * MM, 297.0 * MM),
        paper("ISO A2 -- 594 x 420 mm", 594.0 * MM, 420.0 * MM),
        paper("ISO A1 -- 841 x 594 mm", 841.0 * MM, 594.0 * MM),
        paper("ISO A0 -- 1189 x 841 mm", 1189.0 * MM, 841.0 * MM),
    ]
}

/// Every plot scale offered, imperial first and metric after.
pub fn scales() -> Vec<ScaleRow> {
    let imperial = |label, f| ScaleRow {
        label,
        feet_per_inch: f,
        metric: false,
    };
    let metric = |label, n: f64| ScaleRow {
        label,
        feet_per_inch: n / 12.0,
        metric: true,
    };
    vec![
        imperial("1\" = 10 ft", 10.0),
        imperial("1\" = 20 ft", 20.0),
        imperial("1\" = 25 ft", 25.0),
        imperial("1\" = 30 ft", 30.0),
        imperial("1\" = 40 ft", 40.0),
        imperial("1\" = 50 ft", 50.0),
        imperial("1\" = 60 ft", 60.0),
        imperial("1\" = 80 ft", 80.0),
        imperial("1\" = 100 ft", 100.0),
        imperial("1\" = 150 ft", 150.0),
        imperial("1\" = 200 ft", 200.0),
        imperial("1\" = 300 ft", 300.0),
        imperial("1\" = 400 ft", 400.0),
        imperial("1\" = 500 ft", 500.0),
        metric("1:100", 100.0),
        metric("1:200", 200.0),
        metric("1:250", 250.0),
        metric("1:500", 500.0),
        metric("1:1000", 1000.0),
        metric("1:2000", 2000.0),
    ]
}

fn row_of(feet_per_inch: f64) -> Option<ScaleRow> {
    scales()
        .into_iter()
        .find(|r| (r.feet_per_inch - feet_per_inch).abs() < 1e-9)
}

/// What the bar is captioned with: `SCALE:  1" = 50 FT`, or `SCALE:  1:500`.
pub fn scale_text(feet_per_inch: f64) -> String {
    match row_of(feet_per_inch) {
        Some(r) => format!("SCALE:  {}", r.label.to_uppercase()),
        None => format!("SCALE:  1\" = {feet_per_inch} FT"),
    }
}

/// Which scale fits a cave on a sheet.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Fit {
    /// Feet of cave per inch of paper.
    pub scale: f64,
    /// True when the paper has to be turned (portrait).
    pub turned: bool,
    /// False when even the smallest scale does not fit.
    pub fits: bool,
}

/// The most detailed standard scale that fits a cave of `w` x `h` feet on `sheet` with a footer band
/// of `footer` inches; `force_turned` fixes the paper's way up (otherwise either is tried and a
/// turned answer says so).
pub fn fit(w: f64, h: f64, sheet: &Paper, footer: f64, force_turned: Option<bool>) -> Fit {
    let footer = if footer > 0.0 { footer } else { 0.0 };
    let usable_w = sheet.w - MARGIN_INCHES * 2.0;
    let usable_h = sheet.h - MARGIN_INCHES * 2.0 - footer;
    let usable_h_turned = sheet.w - MARGIN_INCHES * 2.0 - footer;
    let usable_w_turned = sheet.h - MARGIN_INCHES * 2.0;
    let (w, h) = (w.max(0.0001), h.max(0.0001));
    let landscape_ok = force_turned != Some(true);
    let portrait_ok = force_turned != Some(false);
    let all = scales();
    for s in all.iter().map(|r| r.feet_per_inch) {
        if landscape_ok && w / s <= usable_w && h / s <= usable_h {
            return Fit {
                scale: s,
                turned: false,
                fits: true,
            };
        }
        if portrait_ok && w / s <= usable_w_turned && h / s <= usable_h_turned {
            return Fit {
                scale: s,
                turned: true,
                fits: true,
            };
        }
    }
    Fit {
        scale: all.last().map_or(1.0, |r| r.feet_per_inch),
        turned: force_turned == Some(true),
        fits: false,
    }
}

/// A scale bar's divisions.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BarSteps {
    /// What each block's label counts up by, in the display unit.
    pub per_block: f64,
    /// How many blocks.
    pub blocks: u32,
    /// `M` or `FT`.
    pub unit: &'static str,
    /// One block in feet of cave.
    pub per_block_feet: f64,
    /// The whole bar in feet of cave.
    pub total_feet: f64,
}

/// The scale bar's divisions: a round number per block and enough blocks to fill the bar (a metric
/// scale counts metres).
pub fn bar_for(feet_per_inch: f64) -> BarSteps {
    let metric = row_of(feet_per_inch).is_some_and(|r| r.metric);
    let per_display = if metric { M_PER_FT } else { 1.0 };
    let wanted = BAR.length * feet_per_inch * per_display;
    let steps: &[f64] = if metric {
        &[
            1.0, 2.0, 5.0, 10.0, 20.0, 25.0, 50.0, 100.0, 200.0, 250.0, 500.0, 1000.0,
        ]
    } else {
        &[
            1.0, 2.0, 5.0, 10.0, 15.0, 20.0, 25.0, 50.0, 100.0, 200.0, 250.0, 500.0, 1000.0,
        ]
    };
    let mut best: Option<(f64, u32, f64)> = None;
    for &step in steps {
        let blocks = (wanted / step).floor();
        if blocks < 2.0 || blocks > 10.0 {
            continue;
        }
        let score = (blocks - 6.0).abs();
        if best.is_none_or(|b| score < b.2) {
            best = Some((step, blocks as u32, score));
        }
    }
    let (per_block, blocks) = best.map_or(((wanted / 4.0).round().max(1.0), 4), |b| (b.0, b.1));
    BarSteps {
        per_block,
        blocks,
        unit: if metric { "M" } else { "FT" },
        per_block_feet: per_block / per_display,
        total_feet: per_block * f64::from(blocks) / per_display,
    }
}

/// One piece of furniture's box, in inches from the margin box's lower left corner.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Piece {
    /// `title`, `bar` or `north`.
    pub kind: &'static str,
    /// Width.
    pub w: f64,
    /// Height.
    pub h: f64,
    /// Left edge.
    pub x: f64,
    /// Bottom edge.
    pub y: f64,
    /// Where the arrow's pin is inside its box (north only).
    pub pin: Option<[f64; 2]>,
}

/// Where the furniture goes.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Furniture {
    /// The height of the band under the map the furniture takes, margin included.
    pub footer: f64,
    /// Rows the pieces were packed into.
    pub rows: usize,
    /// The pieces placed.
    pub pieces: Vec<Piece>,
}

/// Which pieces to put on the sheet.
#[derive(Clone, Copy, Debug)]
pub struct Wants {
    /// The title block.
    pub title: bool,
    /// The scale bar.
    pub bar: bool,
    /// The north arrow.
    pub north: bool,
}

/// The north arrow's reach from its pin (true north only): left, right, down, up, in inches.
fn north_extent() -> (f64, f64, f64, f64) {
    let head_half: f64 = 0.18;
    let small = TEXT.small;
    let char_w = small * CHAR_WIDTH;
    let right = (head_half + 0.1).max(-0.9 + "TRUE NORTH".len() as f64 * char_w);
    (0.9, right, 0.2 + small / 2.0, 1.4 + 0.28 + TEXT.heading)
}

/// How far the scale bar's labels hang below its line (the bar is drawn this far above its box).
pub const BAR_LIFT: f64 = BAR.tick * 2.0 + TEXT.small / 2.0 + 0.02;

/// Packs the title block, scale bar and north arrow into the band under the map: one row when they
/// fit, more when they do not; the arrow goes to the right margin and a bar between is centred.
pub fn furniture(width_inches: f64, wants: Wants, title_height: f64) -> Furniture {
    let (gap, vgap, pad) = (0.3, 0.2, 0.4);
    let usable = width_inches - MARGIN_INCHES * 2.0;
    let mut list: Vec<Piece> = Vec::new();
    let piece = |kind, w, h, pin| Piece {
        kind,
        w,
        h,
        x: 0.0,
        y: 0.0,
        pin,
    };
    if wants.title {
        list.push(piece("title", TITLE_INCHES, title_height.max(0.5), None));
    }
    if wants.bar {
        list.push(piece(
            "bar",
            BAR.length + 0.65,
            BAR_LIFT + BAR.height + TEXT.body * 1.6,
            None,
        ));
    }
    if wants.north {
        let (left, right, down, up) = north_extent();
        list.push(piece("north", left + right, up + down, Some([left, down])));
    }
    let mut rows: Vec<(Vec<Piece>, f64, f64)> = Vec::new();
    for p in list {
        let fits = rows.last().is_some_and(|(items, w, _)| {
            let need = if items.is_empty() { 0.0 } else { gap } + p.w;
            w + need <= usable + 1e-9
        });
        if fits {
            let (items, w, h) = rows.last_mut().expect("a row");
            *w += gap + p.w;
            *h = h.max(p.h);
            items.push(p);
        } else {
            let (w, h) = (p.w, p.h);
            rows.push((vec![p], w, h));
        }
    }
    let mut pieces = Vec::new();
    let mut y = 0.0;
    let n_rows = rows.len();
    let mut total = 0.0;
    for (mut items, _, h) in rows {
        let mut x = 0.0;
        for it in &mut items {
            it.x = x;
            x += it.w + gap;
        }
        let n = items.len();
        if n > 1 && items[n - 1].kind == "north" {
            let right = (usable - items[n - 1].w).max(items[n - 1].x);
            items[n - 1].x = right;
            if n == 3 {
                let free = right - (items[0].x + items[0].w);
                items[1].x = items[0].x + items[0].w + gap.max((free - items[1].w) / 2.0);
            }
        }
        for mut it in items {
            it.y = y;
            pieces.push(it);
        }
        total = y + h;
        y += h + vgap;
    }
    Furniture {
        footer: if n_rows > 0 { total + pad } else { 0.0 },
        rows: n_rows,
        pieces,
    }
}

/// A drawn shape on the sheet, in inches from the paper's lower left corner.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum Shape {
    /// A straight line.
    Line {
        /// Start.
        a: [f64; 2],
        /// End.
        b: [f64; 2],
    },
    /// An unfilled box.
    Rect {
        /// Lower left.
        a: [f64; 2],
        /// Upper right.
        b: [f64; 2],
    },
    /// A filled polygon.
    Fill {
        /// Corners.
        pts: Vec<[f64; 2]>,
    },
    /// A white box behind a piece of furniture laid over the map, so the map does not run through it.
    Back {
        /// Lower left.
        a: [f64; 2],
        /// Upper right.
        b: [f64; 2],
    },
    /// A dashed line (a match line).
    Dash {
        /// Start.
        a: [f64; 2],
        /// End.
        b: [f64; 2],
    },
    /// Text turned `deg` degrees counter-clockwise about its start.
    Turned {
        /// Where it starts.
        at: [f64; 2],
        /// Its height.
        h: f64,
        /// The words.
        s: String,
        /// The turn.
        deg: f64,
    },
    /// Text starting at a baseline point.
    Text {
        /// Where it starts.
        at: [f64; 2],
        /// Its height.
        h: f64,
        /// The words.
        s: String,
    },
}

/// The words of the title block, one per line (an empty one is left out); the first is the cave's name.
pub struct Title<'a> {
    /// The cave's name.
    pub name: &'a str,
    /// Further lines, such as location, who surveyed and when.
    pub lines: &'a [String],
}

/// How tall the title block has to be for these words.
pub fn title_height(title: &Title) -> f64 {
    let n = title.lines.iter().filter(|l| !l.trim().is_empty()).count();
    0.25 + TEXT.cave_name + n as f64 * TEXT.body * LINE_SPACING
}

/// Which way the north arrow points and what it says about the compass.
#[derive(Clone, Copy, Debug, Default)]
pub struct North {
    /// How far the map is turned on the paper, degrees counter-clockwise (the arrow turns with it).
    pub twist: f64,
    /// Magnetic declination in degrees, east positive; adds a magnetic arm when known.
    pub declination: Option<f64>,
}

/// One piece of furniture drawn as shapes. `origin` is the lower left of the box the piece's own
/// position is measured from (the margin box on a sheet).
pub fn piece_shapes(
    out: &mut Vec<Shape>,
    p: &Piece,
    origin: [f64; 2],
    title: &Title,
    feet_per_inch: f64,
    north: &North,
) {
    let (x, y) = (origin[0] + p.x, origin[1] + p.y);
    match p.kind {
        "title" => {
            out.push(Shape::Rect {
                a: [x, y],
                b: [x + p.w, y + p.h],
            });
            let mut ty = y + p.h - 0.15 - TEXT.cave_name;
            out.push(Shape::Text {
                at: [x + 0.15, ty],
                h: TEXT.cave_name,
                s: title.name.to_uppercase(),
            });
            for l in title.lines.iter().filter(|l| !l.trim().is_empty()) {
                ty -= TEXT.body * LINE_SPACING;
                out.push(Shape::Text {
                    at: [x + 0.15, ty],
                    h: TEXT.body,
                    s: l.to_uppercase(),
                });
            }
        }
        "bar" => {
            let bar = bar_for(feet_per_inch);
            let block_w = bar.per_block_feet / feet_per_inch;
            let (bx, by) = (x + 0.1, y + BAR_LIFT);
            for k in 0..=bar.blocks {
                let tx = bx + block_w * f64::from(k);
                out.push(Shape::Line {
                    a: [tx, by],
                    b: [tx, by + BAR.height],
                });
                out.push(Shape::Text {
                    at: [tx, by - BAR.tick * 2.0],
                    h: TEXT.small,
                    s: format!("{}", bar.per_block * f64::from(k)),
                });
            }
            for k in (0..bar.blocks).step_by(2) {
                let x0 = bx + block_w * f64::from(k);
                out.push(Shape::Fill {
                    pts: vec![
                        [x0, by],
                        [x0 + block_w, by],
                        [x0 + block_w, by + BAR.height],
                        [x0, by + BAR.height],
                    ],
                });
            }
            let end = bx + block_w * f64::from(bar.blocks);
            out.push(Shape::Line {
                a: [bx, by],
                b: [end, by],
            });
            out.push(Shape::Line {
                a: [bx, by + BAR.height],
                b: [end, by + BAR.height],
            });
            out.push(Shape::Text {
                at: [bx, by + BAR.height + TEXT.body],
                h: TEXT.body,
                s: scale_text(feet_per_inch),
            });
            out.push(Shape::Text {
                at: [
                    end + 0.1 + TEXT.small * CHAR_WIDTH * 4.0,
                    by - BAR.tick * 2.0,
                ],
                h: TEXT.small,
                s: bar.unit.to_string(),
            });
        }
        "north" => {
            let pin = p.pin.unwrap_or([0.0, 0.0]);
            let (px, py) = (x + pin[0], y + pin[1] + 0.2);
            let (height, head_len, head_half) = (1.4, 0.4, 0.18);
            // the arrow points where north is on the paper: straight up, turned with the map
            let t = north.twist.to_radians();
            let up = [-t.sin(), t.cos()];
            let right = [t.cos(), t.sin()];
            let at = |along: f64, across: f64| {
                [
                    px + up[0] * along + right[0] * across,
                    py + up[1] * along + right[1] * across,
                ]
            };
            out.push(Shape::Line {
                a: [px, py],
                b: at(height, 0.0),
            });
            out.push(Shape::Fill {
                pts: vec![
                    at(height, 0.0),
                    at(height - head_len, -head_half),
                    at(height - head_len, head_half),
                ],
            });
            let tip = at(height + 0.1, 0.0);
            out.push(Shape::Text {
                at: [tip[0] - TEXT.heading * 0.3, tip[1]],
                h: TEXT.heading,
                s: "N".to_string(),
            });
            out.push(Shape::Text {
                at: [px - 0.9, py - 0.2],
                h: TEXT.small,
                s: "TRUE NORTH".to_string(),
            });
            if let Some(d) = north.declination {
                // magnetic north is `d` degrees east (clockwise) of true: a shorter half-arrow
                let m = (north.twist - d).to_radians();
                let mu = [-m.sin(), m.cos()];
                let mr = [m.cos(), m.sin()];
                let mat = |along: f64, across: f64| {
                    [
                        px + mu[0] * along + mr[0] * across,
                        py + mu[1] * along + mr[1] * across,
                    ]
                };
                let mh = height * 0.8;
                out.push(Shape::Line {
                    a: [px, py],
                    b: mat(mh, 0.0),
                });
                out.push(Shape::Fill {
                    pts: vec![mat(mh, 0.0), mat(mh - head_len * 0.8, 0.0), mat(mh - head_len * 0.8, head_half)],
                });
                let lab = mat(mh + 0.08, 0.04);
                out.push(Shape::Text {
                    at: lab,
                    h: TEXT.small,
                    s: format!("MN {:.1}\u{b0}{}", d.abs(), if d >= 0.0 { "E" } else { "W" }),
                });
            }
        }
        _ => {}
    }
}

/// The sheet's furniture drawn as shapes: the border at the margin, the title block, the scale bar
/// (blocks alternate filled and open) and the north arrow. `origin` is the margin box's lower left
/// corner on the paper (the margin).
pub fn shapes(
    paper_w: f64,
    paper_h: f64,
    f: &Furniture,
    title: &Title,
    feet_per_inch: f64,
) -> Vec<Shape> {
    let m = MARGIN_INCHES;
    let mut out = vec![Shape::Rect {
        a: [m, m],
        b: [paper_w - m, paper_h - m],
    }];
    for p in &f.pieces {
        piece_shapes(
            &mut out,
            p,
            [m, m],
            title,
            feet_per_inch,
            &North::default(),
        );
    }
    out
}

/// A whole sheet laid out: the paper, the box the map may use, and the furniture as shapes.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Sheet {
    /// Paper width in inches, the way up chosen.
    pub w: f64,
    /// Paper height in inches.
    pub h: f64,
    /// The map's box `[x1, y1, x2, y2]` in inches from the paper's lower left (inside the margin, above the footer).
    pub map: [f64; 4],
    /// Feet of cave per inch of paper.
    pub feet_per_inch: f64,
    /// The border, title block, scale bar and north arrow.
    pub shapes: Vec<Shape>,
}

/// Lays out a sheet: paper (turned or not), plot scale, and the words of the title block.
pub fn layout(
    paper: &Paper,
    turned: bool,
    feet_per_inch: f64,
    wants: Wants,
    title: &Title,
) -> Sheet {
    let (w, h) = if turned {
        (paper.h, paper.w)
    } else {
        (paper.w, paper.h)
    };
    let th = title_height(title);
    let f = furniture(w, wants, th);
    let m = MARGIN_INCHES;
    Sheet {
        w,
        h,
        map: [m, m + f.footer, w - m, h - m],
        feet_per_inch,
        shapes: shapes(w, h, &f, title, feet_per_inch),
    }
}

/// The footer band for one orientation of one paper (what [`fit`] has to reserve).
pub fn footer_for(paper: &Paper, turned: bool, wants: Wants, title: &Title) -> f64 {
    let w = if turned { paper.h } else { paper.w };
    furniture(w, wants, title_height(title)).footer
}

/// Feet in a metre, for callers that hold metres.
pub const FT_PER_M: f64 = 1.0 / M_PER_FT;
