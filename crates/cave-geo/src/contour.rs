//! Contour lines from an elevation grid: marching squares with linear interpolation, stitched into
//! runs. Points are in CELL coordinates (x = column, y = row from the top; cell centres at integers);
//! the caller maps them to the world. Ported from the old CsContour.levels/lines/stitch.

use crate::grid::Grid;
use std::collections::HashMap;

/// One contour line in cell coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    /// The points, in order.
    pub points: Vec<[f64; 2]>,
    /// True when the line closes on itself (the last point joins the first; not repeated).
    pub closed: bool,
}

/// Every multiple of `interval` strictly inside `(min, max)`, walked by index so float drift never
/// skips or repeats a level.
pub fn levels(min: f64, max: f64, interval: f64) -> Vec<f64> {
    let mut out = Vec::new();
    if interval.is_nan() || interval <= 0.0 || !min.is_finite() || !max.is_finite() {
        return out;
    }
    let span = (max - min) / interval;
    if !span.is_finite() || span > 10_000.0 {
        return out;
    }
    let first = (min / interval).ceil();
    if !first.is_finite() || first.abs() > 1e15 {
        return out;
    }
    let mut k = first as i64;
    while (k as f64) * interval < max {
        let v = k as f64 * interval;
        if v > min {
            out.push(v);
        }
        k += 1;
    }
    out
}

/// The contour runs at `level` (same unit as the grid). Cells touching a hole contribute nothing.
pub fn lines(g: &Grid, level: f64) -> Vec<Run> {
    let interp = |a: f64, b: f64| {
        let d = b - a;
        if d == 0.0 {
            0.5
        } else {
            ((level - a) / d).clamp(0.0, 1.0)
        }
    };
    let mut segs: Vec<[f64; 4]> = Vec::new();
    for r in 0..g.h.saturating_sub(1) {
        for c in 0..g.w.saturating_sub(1) {
            let (Some(tl), Some(tr), Some(br), Some(bl)) = (
                g.at(c, r),
                g.at(c + 1, r),
                g.at(c + 1, r + 1),
                g.at(c, r + 1),
            ) else {
                continue;
            };
            let idx = (u8::from(tl >= level) << 3)
                | (u8::from(tr >= level) << 2)
                | (u8::from(br >= level) << 1)
                | u8::from(bl >= level);
            if idx == 0 || idx == 15 {
                continue;
            }
            let (cf, rf) = (c as f64, r as f64);
            let top = [cf + interp(tl, tr), rf];
            let right = [cf + 1.0, rf + interp(tr, br)];
            let bottom = [cf + interp(bl, br), rf + 1.0];
            let left = [cf, rf + interp(tl, bl)];
            let mut add = |p: [f64; 2], q: [f64; 2]| segs.push([p[0], p[1], q[0], q[1]]);
            match idx {
                1 => add(left, bottom),
                2 => add(bottom, right),
                3 => add(left, right),
                4 => add(top, right),
                6 => add(top, bottom),
                7 => add(left, top),
                8 => add(top, left),
                9 => add(top, bottom),
                11 => add(top, right),
                12 => add(right, left),
                13 => add(right, bottom),
                14 => add(bottom, left),
                5 | 10 => {
                    let high = (tl + tr + br + bl) / 4.0 >= level;
                    match (idx, high) {
                        (5, true) => {
                            add(left, top);
                            add(bottom, right);
                        }
                        (5, false) => {
                            add(left, bottom);
                            add(top, right);
                        }
                        (_, true) => {
                            add(top, right);
                            add(bottom, left);
                        }
                        (_, false) => {
                            add(top, left);
                            add(bottom, right);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    stitch(&segs)
}

fn key(x: f64, y: f64) -> (i64, i64) {
    ((x * 4096.0).round() as i64, (y * 4096.0).round() as i64)
}

/// Chains loose segments into runs by shared endpoints; every segment is used once.
fn stitch(segs: &[[f64; 4]]) -> Vec<Run> {
    let mut ends: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, s) in segs.iter().enumerate() {
        ends.entry(key(s[0], s[1])).or_default().push(i);
        ends.entry(key(s[2], s[3])).or_default().push(i);
    }
    let mut used = vec![false; segs.len()];
    let mut out = Vec::new();
    for i in 0..segs.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        let mut pts =
            std::collections::VecDeque::from([[segs[i][0], segs[i][1]], [segs[i][2], segs[i][3]]]);
        for at_end in [true, false] {
            loop {
                let tip = if at_end {
                    *pts.back().unwrap()
                } else {
                    *pts.front().unwrap()
                };
                let k = key(tip[0], tip[1]);
                let Some(&next) = ends.get(&k).and_then(|l| l.iter().find(|&&j| !used[j])) else {
                    break;
                };
                used[next] = true;
                let s = segs[next];
                let p = if key(s[0], s[1]) == k {
                    [s[2], s[3]]
                } else {
                    [s[0], s[1]]
                };
                if at_end {
                    pts.push_back(p)
                } else {
                    pts.push_front(p)
                }
            }
        }
        let mut points: Vec<[f64; 2]> = pts.into_iter().collect();
        let closed = points.len() > 2
            && key(points[0][0], points[0][1])
                == key(points[points.len() - 1][0], points[points.len() - 1][1]);
        if closed {
            points.pop();
        }
        out.push(Run { points, closed });
    }
    out
}
