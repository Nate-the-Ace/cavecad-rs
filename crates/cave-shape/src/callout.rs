//! Callouts: a note with a leader arrow to each thing it points at. Only the tips and the note's
//! position are authored; the leader (tip, elbow, landing on the note's edge) is derived every time.
//! A port of the pure `CsCallout.reflow` and `CsCallout.sideFor` of the old app, checked against its
//! answers in `tests/callout_golden.rs`.

use serde::Serialize;

/// Segments a curved leader is traced in.
pub const CURVE_SEGMENTS: usize = 8;
/// How far a curved leader bows off its chord, as a fraction of the chord.
pub const CURVE_OFFSET: f64 = 0.18;

/// Which edge of the note the leader leaves from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    /// The note's left edge.
    Left,
    /// The note's right edge.
    Right,
}

/// A note's box, `x1 < x2` and `y1 < y2` (either way up).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoteBox {
    /// Left edge.
    pub x1: f64,
    /// One horizontal edge.
    pub y1: f64,
    /// Right edge.
    pub x2: f64,
    /// The other horizontal edge.
    pub y2: f64,
}

/// The solved leader geometry of one callout.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Leaders {
    /// The side the leader leaves from.
    pub side: Side,
    /// Where it meets the note: the middle of the chosen edge.
    pub landing: [f64; 2],
    /// One polyline per tip: the tip first, then the elbow, then the landing.
    pub branches: Vec<Vec<[f64; 2]>>,
}

/// Which side the leader leaves by: toward the mean tip, left when the mean is at or left of
/// `reference_x` (a tie goes left).
pub fn side_for(tips: &[[f64; 2]], reference_x: f64) -> Side {
    if tips.is_empty() {
        return Side::Left;
    }
    let mean = tips.iter().map(|t| t[0]).sum::<f64>() / tips.len() as f64;
    if mean <= reference_x {
        Side::Left
    } else {
        Side::Right
    }
}

/// Solves the leaders for a note: the landing is the vertical middle of the chosen edge (so a note
/// that gains a line does not swing its arrows), the elbow sits half a text height out, and a curved
/// leader bows its tip-to-elbow stretch away from the note.
pub fn reflow(b: NoteBox, tips: &[[f64; 2]], side: Option<Side>, curved: bool) -> Leaders {
    let height = (b.y2 - b.y1).abs();
    let centre_x = (b.x1 + b.x2) / 2.0;
    let mid_y = (b.y1 + b.y2) / 2.0;
    let landing_len = height * 0.5;
    let side = side.unwrap_or_else(|| side_for(tips, centre_x));
    let landing = [if side == Side::Left { b.x1 } else { b.x2 }, mid_y];
    let elbow = [
        if side == Side::Left {
            landing[0] - landing_len
        } else {
            landing[0] + landing_len
        },
        landing[1],
    ];
    let sign = if side == Side::Left { 1.0 } else { -1.0 };
    let branches = tips
        .iter()
        .map(|&tip| {
            let mut pts = Vec::new();
            let (dx, dy) = (elbow[0] - tip[0], elbow[1] - tip[1]);
            if !curved || dx.hypot(dy) < 1e-9 {
                pts.push(tip);
            } else {
                let ctrl = [
                    (tip[0] + elbow[0]) / 2.0 + -dy * CURVE_OFFSET * sign,
                    (tip[1] + elbow[1]) / 2.0 + dx * CURVE_OFFSET * sign,
                ];
                for i in 0..CURVE_SEGMENTS {
                    let t = i as f64 / CURVE_SEGMENTS as f64;
                    let m = 1.0 - t;
                    pts.push([
                        m * m * tip[0] + 2.0 * m * t * ctrl[0] + t * t * elbow[0],
                        m * m * tip[1] + 2.0 * m * t * ctrl[1] + t * t * elbow[1],
                    ]);
                }
            }
            pts.push(elbow);
            pts.push(landing);
            pts
        })
        .collect();
    Leaders {
        side,
        landing,
        branches,
    }
}

/// The box of a note of `chars` characters and text height `h` whose near edge is at `at` (the note
/// grows away from the arrow: to the right of `at` for a left-hand leader, to its left otherwise).
/// Width is an estimate (0.6 of the height per character), the same one the web app draws with.
pub fn note_box(at: [f64; 2], chars: usize, h: f64, side: Side) -> NoteBox {
    let w = chars.max(1) as f64 * h * 0.6;
    let (x1, x2) = if side == Side::Left {
        (at[0], at[0] + w)
    } else {
        (at[0] - w, at[0])
    };
    NoteBox {
        x1,
        y1: at[1] - h / 2.0,
        x2,
        y2: at[1] + h / 2.0,
    }
}

/// Everything drawn for a single-arrow callout placed by dragging from the thing pointed at (`tip`)
/// to where the note goes (`at`): the note's height is a third of the drag, the leader leaves the
/// note on the side facing the tip, and the arrow head is a V of two short lines at the tip.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Layout {
    /// Text height.
    pub height: f64,
    /// Which edge the leader leaves from.
    pub side: Side,
    /// The note's box as `[x1, y1, x2, y2]`.
    pub note: [f64; 4],
    /// The leader: tip, elbow, landing (more points when curved).
    pub leader: Vec<[f64; 2]>,
    /// The arrow head as one polyline: one barb, the tip, the other barb.
    pub head: Vec<[f64; 2]>,
}

/// Lays out a callout of `chars` characters (see [`Layout`]).
pub fn layout(tip: [f64; 2], at: [f64; 2], chars: usize, curved: bool) -> Layout {
    layout_with(tip, at, chars, curved, None, false)
}

/// A leader with any number of arrows, laid out.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MultiLayout {
    /// Text height.
    pub height: f64,
    /// Which edge of the note the leaders leave from.
    pub side: Side,
    /// The note's box as `[x1, y1, x2, y2]`.
    pub note: [f64; 4],
    /// One polyline per arrow: tip, elbow, landing (more points when curved; just tip and landing when straight).
    pub branches: Vec<Vec<[f64; 2]>>,
    /// One arrow head per arrow: a barb, the tip, the other barb.
    pub heads: Vec<Vec<[f64; 2]>>,
}

/// Lays out a note at `at` with an arrow to each of `tips`: the leaders all leave by the edge facing the middle of the tips and
/// meet the note at the middle of that edge. `height` fixes the text height (otherwise a third of the way to the first tip),
/// `straight` runs each leader straight to the note's edge instead of by an elbow, and `curved` bows them.
pub fn layout_many(
    tips: &[[f64; 2]],
    at: [f64; 2],
    chars: usize,
    curved: bool,
    height: Option<f64>,
    straight: bool,
) -> MultiLayout {
    let first = tips.first().copied().unwrap_or(at);
    let height = height
        .filter(|h| h.is_finite() && *h > 0.0)
        .unwrap_or_else(|| (at[0] - first[0]).hypot(at[1] - first[1]) / 3.0);
    let side = side_for(tips, at[0]);
    let b = note_box(at, chars, height, side);
    let r = reflow(b, tips, Some(side), curved && !straight);
    let mut branches = r.branches;
    if straight {
        // no elbow: from each tip directly to where the leader meets the note
        branches = tips.iter().map(|&t| vec![t, r.landing]).collect();
    }
    // the barbs lie back along each leader's first stretch
    let heads = tips
        .iter()
        .zip(&branches)
        .map(|(&tip, leader)| {
            let next = leader.get(1).copied().unwrap_or(at);
            let (dx, dy) = (next[0] - tip[0], next[1] - tip[1]);
            let len = dx.hypot(dy);
            if len < 1e-12 {
                return vec![tip];
            }
            let (ux, uy) = (dx / len, dy / len);
            let barb = height * 0.7;
            let (s, c) = (0.35_f64.sin(), 0.35_f64.cos());
            let rot = |sx: f64| {
                [
                    tip[0] + barb * (ux * c - sx * s * uy),
                    tip[1] + barb * (uy * c + sx * s * ux),
                ]
            };
            vec![rot(1.0), tip, rot(-1.0)]
        })
        .collect();
    MultiLayout {
        height,
        side,
        note: [b.x1, b.y1, b.x2, b.y2],
        branches,
        heads,
    }
}

/// Lays out a leader with its own settings: `height` fixes the text height (otherwise a third of the drag, as `layout` does),
/// `straight` runs the leader from the tip straight to the note's edge instead of by an elbow, and `curved` bows it.
pub fn layout_with(
    tip: [f64; 2],
    at: [f64; 2],
    chars: usize,
    curved: bool,
    height: Option<f64>,
    straight: bool,
) -> Layout {
    let height = height
        .filter(|h| h.is_finite() && *h > 0.0)
        .unwrap_or_else(|| (at[0] - tip[0]).hypot(at[1] - tip[1]) / 3.0);
    let side = side_for(&[tip], at[0]);
    let b = note_box(at, chars, height, side);
    let r = reflow(b, &[tip], Some(side), curved && !straight);
    let mut leader = r.branches.into_iter().next().unwrap_or_default();
    if straight {
        // no elbow: from the tip directly to where the leader meets the note
        leader = vec![tip, r.landing];
    }
    // the barbs lie back along the leader's first stretch
    let next = leader.get(1).copied().unwrap_or(at);
    let (dx, dy) = (next[0] - tip[0], next[1] - tip[1]);
    let len = dx.hypot(dy);
    let head = if len < 1e-12 {
        vec![tip]
    } else {
        let (ux, uy) = (dx / len, dy / len);
        let barb = height * 0.7;
        let (s, c) = (0.35_f64.sin(), 0.35_f64.cos());
        let rot = |sx: f64| {
            [
                tip[0] + barb * (ux * c - sx * s * uy),
                tip[1] + barb * (uy * c + sx * s * ux),
            ]
        };
        vec![rot(1.0), tip, rot(-1.0)]
    };
    Layout {
        height,
        side,
        note: [b.x1, b.y1, b.x2, b.y2],
        leader,
        head,
    }
}
