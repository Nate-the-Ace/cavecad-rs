//! Brush areas: a stroke dragged with a round brush becomes the outline(s) of the ground it swept,
//! stopped at the passage walls. The breakdown brush uses it: the outline it gives is stored as an
//! ordinary area fill (blocks or debris), so it selects, reshapes, undoes and exports like any fill.
//!
//! The swept ground is found on a grid: a cell is in when its middle is within the brush's reach of
//! the stroke AND the straight way from the nearest point of the stroke to it crosses no wall, so
//! the brush never spills through a wall into the next passage. The cells' edges are then traced
//! into closed outlines and simplified. Lengths are metres; no randomness, so the same stroke always
//! gives the same outline.

type Pt = [f64; 2];

/// The most grid cells one stroke may sweep: a very long, wide stroke gets a coarser grid rather than a slow one.
pub const MAX_CELLS: usize = 400_000;

/// Outlines of the ground a round brush of `radius` swept along `stroke`, never crossing a line in
/// `walls` (open polylines). Each outline is a closed polygon (first point not repeated), largest
/// first; pieces smaller than a tenth of the brush's own circle are dropped.
pub fn swept_outlines(stroke: &[Pt], radius: f64, walls: &[Vec<Pt>]) -> Vec<Vec<Pt>> {
    use std::collections::BTreeSet;
    if stroke.is_empty()
        || radius <= 0.0
        || !radius.is_finite()
        || stroke
            .iter()
            .any(|p| !p[0].is_finite() || !p[1].is_finite())
    {
        return Vec::new();
    }
    let stroke = resample(stroke, radius / 4.0);
    // a grid fixed to the ground (not to the stroke), so the same stroke always lands on the same cells
    let length: f64 = stroke
        .windows(2)
        .map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt())
        .sum();
    let mut cell = radius / 6.0;
    let estimate = |c: f64| ((length + 2.0 * radius) * 2.0 * radius) / (c * c);
    while estimate(cell) > MAX_CELLS as f64 {
        cell *= 1.5;
    }
    let all: Vec<(Pt, Pt)> = walls
        .iter()
        .flat_map(|w| w.windows(2).map(|s| (s[0], s[1])))
        .collect();
    let r2 = radius * radius;
    let reach = (radius / cell).ceil() as i64 + 1;
    let mut on: BTreeSet<(i64, i64)> = BTreeSet::new();
    for p in &stroke {
        // only the walls that could stand between this point and a cell it reaches
        let near: Vec<&(Pt, Pt)> = all
            .iter()
            .filter(|(a, b)| {
                a[0].max(b[0]) >= p[0] - radius
                    && a[0].min(b[0]) <= p[0] + radius
                    && a[1].max(b[1]) >= p[1] - radius
                    && a[1].min(b[1]) <= p[1] + radius
            })
            .collect();
        let (ci, cj) = ((p[0] / cell).floor() as i64, (p[1] / cell).floor() as i64);
        for j in cj - reach..=cj + reach {
            for i in ci - reach..=ci + reach {
                if on.contains(&(i, j)) {
                    continue;
                }
                let c = [(i as f64 + 0.5) * cell, (j as f64 + 0.5) * cell];
                if (c[0] - p[0]).powi(2) + (c[1] - p[1]).powi(2) > r2
                    || near.iter().any(|(a, b)| crosses(*p, c, *a, *b))
                {
                    continue;
                }
                on.insert((i, j));
            }
        }
    }
    // a cell a wall runs through is left out, so ground on the two sides of a wall never joins up
    on.retain(|&(i, j)| {
        let (x0, y0) = (i as f64 * cell, j as f64 * cell);
        !all.iter()
            .any(|(a, b)| seg_in_box(*a, *b, x0, y0, x0 + cell, y0 + cell))
    });
    let mut outlines: Vec<Vec<Pt>> = trace(&on)
        .into_iter()
        .map(|ring| {
            ring.into_iter()
                .map(|(i, j)| [i as f64 * cell, j as f64 * cell])
                .collect::<Vec<Pt>>()
        })
        .map(|ring| simplify_closed(&ring, cell * 0.6))
        .filter(|ring| ring.len() >= 3 && area(ring) > 0.0)
        .collect();
    let least = std::f64::consts::PI * radius * radius * 0.1;
    outlines.retain(|r| area(r) >= least);
    outlines.sort_by(|a, b| {
        area(b)
            .partial_cmp(&area(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    outlines
}

/// True when segment a-b touches the box.
fn seg_in_box(a: Pt, b: Pt, x0: f64, y0: f64, x1: f64, y1: f64) -> bool {
    let inb = |p: Pt| p[0] >= x0 && p[0] <= x1 && p[1] >= y0 && p[1] <= y1;
    if inb(a) || inb(b) {
        return true;
    }
    if a[0].max(b[0]) < x0 || a[0].min(b[0]) > x1 || a[1].max(b[1]) < y0 || a[1].min(b[1]) > y1 {
        return false;
    }
    let c = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
    (0..4).any(|k| crosses(a, b, c[k], c[(k + 1) % 4]))
}

/// Points along the stroke no farther than `step` apart (a click is a single point).
fn resample(stroke: &[Pt], step: f64) -> Vec<Pt> {
    let mut out = vec![stroke[0]];
    for w in stroke.windows(2) {
        let d = ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt();
        let n = (d / step).ceil().max(1.0) as usize;
        for k in 1..=n {
            let t = k as f64 / n as f64;
            out.push([
                w[0][0] + (w[1][0] - w[0][0]) * t,
                w[0][1] + (w[1][1] - w[0][1]) * t,
            ]);
        }
    }
    out
}

/// True when segments p-q and a-b properly cross (touching at an end counts as crossing a wall).
fn crosses(p: Pt, q: Pt, a: Pt, b: Pt) -> bool {
    let o = |u: Pt, v: Pt, w: Pt| (v[0] - u[0]) * (w[1] - u[1]) - (v[1] - u[1]) * (w[0] - u[0]);
    let d1 = o(a, b, p);
    let d2 = o(a, b, q);
    let d3 = o(p, q, a);
    let d4 = o(p, q, b);
    ((d1 > 0.0) != (d2 > 0.0) || d1 == 0.0 || d2 == 0.0)
        && ((d3 > 0.0) != (d4 > 0.0) || d3 == 0.0 || d4 == 0.0)
        && !(d1 == 0.0 && d2 == 0.0)
}

/// The boundaries of the filled cells as closed rings of grid corners, outer edges anticlockwise
/// (holes come out clockwise and are dropped by the caller's area test).
fn trace(cells: &std::collections::BTreeSet<(i64, i64)>) -> Vec<Vec<(i64, i64)>> {
    use std::collections::BTreeMap;
    let at = |i: i64, j: i64| cells.contains(&(i, j));
    // directed boundary edges, cell on the left: start corner -> end corners
    let mut next: BTreeMap<(i64, i64), Vec<(i64, i64)>> = BTreeMap::new();
    for &(i, j) in cells {
        if !at(i, j - 1) {
            next.entry((i, j)).or_default().push((i + 1, j));
        }
        if !at(i + 1, j) {
            next.entry((i + 1, j)).or_default().push((i + 1, j + 1));
        }
        if !at(i, j + 1) {
            next.entry((i + 1, j + 1)).or_default().push((i, j + 1));
        }
        if !at(i - 1, j) {
            next.entry((i, j + 1)).or_default().push((i, j));
        }
    }
    let mut rings = Vec::new();
    while let Some((&start, _)) = next.iter().find(|(_, v)| !v.is_empty()) {
        let mut ring = vec![start];
        let mut cur = start;
        let mut prev_dir: Option<(i64, i64)> = None;
        loop {
            let outs = next.get_mut(&cur).expect("edge");
            // at a corner shared by two diagonal cells, turn the same way every time (keeps rings simple)
            let pick = if outs.len() > 1 {
                let d = prev_dir.unwrap_or((1, 0));
                let left = (d.1, -d.0);
                outs.iter()
                    .position(|e| (e.0 - cur.0, e.1 - cur.1) == left)
                    .unwrap_or(0)
            } else {
                0
            };
            let to = outs.remove(pick);
            prev_dir = Some((to.0 - cur.0, to.1 - cur.1));
            cur = to;
            if cur == start {
                break;
            }
            ring.push(cur);
        }
        rings.push(ring);
    }
    rings
}

/// Signed area (anticlockwise positive).
fn area(r: &[Pt]) -> f64 {
    let n = r.len();
    let mut a = 0.0;
    for i in 0..n {
        let p = r[i];
        let q = r[(i + 1) % n];
        a += p[0] * q[1] - q[0] * p[1];
    }
    a / 2.0
}

/// Douglas-Peucker on a closed ring, split at its two farthest-apart points so the ring stays closed.
fn simplify_closed(r: &[Pt], tol: f64) -> Vec<Pt> {
    let n = r.len();
    if n < 4 {
        return r.to_vec();
    }
    let far = (1..n)
        .max_by(|&a, &b| {
            let da = (r[a][0] - r[0][0]).powi(2) + (r[a][1] - r[0][1]).powi(2);
            let db = (r[b][0] - r[0][0]).powi(2) + (r[b][1] - r[0][1]).powi(2);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(n / 2);
    let mut first: Vec<Pt> = r[..=far].to_vec();
    let mut second: Vec<Pt> = r[far..].to_vec();
    second.push(r[0]);
    first = dp(&first, tol);
    second = dp(&second, tol);
    first.pop();
    second.pop();
    first.extend(second);
    first
}

fn dp(pts: &[Pt], tol: f64) -> Vec<Pt> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let (a, b) = (pts[0], pts[pts.len() - 1]);
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = (dx * dx + dy * dy).sqrt();
    let mut best = (0.0, 0);
    for (i, p) in pts.iter().enumerate().take(pts.len() - 1).skip(1) {
        let d = if len == 0.0 {
            ((p[0] - a[0]).powi(2) + (p[1] - a[1]).powi(2)).sqrt()
        } else {
            ((p[0] - a[0]) * dy - (p[1] - a[1]) * dx).abs() / len
        };
        if d > best.0 {
            best = (d, i);
        }
    }
    if best.0 <= tol {
        return vec![a, b];
    }
    let mut left = dp(&pts[..=best.1], tol);
    let right = dp(&pts[best.1..], tol);
    left.pop();
    left.extend(right);
    left
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inside(r: &[Pt], p: Pt) -> bool {
        let mut c = false;
        let n = r.len();
        for i in 0..n {
            let (a, b) = (r[i], r[(i + 1) % n]);
            if (a[1] > p[1]) != (b[1] > p[1])
                && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
            {
                c = !c;
            }
        }
        c
    }

    #[test]
    fn a_stroke_in_the_open_sweeps_a_band() {
        let out = swept_outlines(&[[0.0, 0.0], [10.0, 0.0]], 1.0, &[]);
        assert_eq!(out.len(), 1);
        let a = area(&out[0]);
        // a 10 x 2 band with round ends: about 20 + pi
        assert!((a - (20.0 + std::f64::consts::PI)).abs() < 1.5, "area {a}");
        assert!(inside(&out[0], [5.0, 0.5]));
        assert!(!inside(&out[0], [5.0, 1.4]));
    }

    #[test]
    fn a_wall_stops_the_brush() {
        // a wall along y = 0.5 under a stroke along y = 0: nothing beyond the wall
        let wall = vec![vec![[-5.0, 0.5], [15.0, 0.5]]];
        let out = swept_outlines(&[[0.0, 0.0], [10.0, 0.0]], 1.0, &wall);
        assert_eq!(out.len(), 1);
        assert!(inside(&out[0], [5.0, -0.5]));
        assert!(!inside(&out[0], [5.0, 0.8]));
        for p in &out[0] {
            assert!(p[1] <= 0.5 + 1e-9, "point past the wall: {p:?}");
        }
    }

    #[test]
    fn a_wall_between_two_passages_splits_the_stroke() {
        // the stroke crosses a wall: the ground on each side is its own piece
        let wall = vec![vec![[5.0, -5.0], [5.0, 5.0]]];
        let out = swept_outlines(&[[0.0, 0.0], [10.0, 0.0]], 1.0, &wall);
        // the stroke's own points on each side see their own side, so there are two pieces
        assert_eq!(out.len(), 2);
        assert!(
            out.iter()
                .all(|r| r.iter().all(|p| p[0] <= 5.0 + 1e-9)
                    || r.iter().all(|p| p[0] >= 5.0 - 1e-9))
        );
    }

    #[test]
    fn the_same_stroke_gives_the_same_outline() {
        let s = [[0.0, 0.0], [3.0, 2.0], [6.0, -1.0]];
        assert_eq!(swept_outlines(&s, 0.8, &[]), swept_outlines(&s, 0.8, &[]));
    }

    #[test]
    fn a_click_is_a_round_patch_and_nonsense_is_nothing() {
        let out = swept_outlines(&[[1.0, 1.0]], 1.0, &[]);
        assert_eq!(out.len(), 1);
        assert!((area(&out[0]) - std::f64::consts::PI).abs() < 0.6);
        assert!(swept_outlines(&[], 1.0, &[]).is_empty());
        assert!(swept_outlines(&[[0.0, 0.0]], 0.0, &[]).is_empty());
        assert!(swept_outlines(&[[0.0, 0.0]], f64::NAN, &[]).is_empty());
    }

    #[test]
    fn a_very_long_stroke_stays_bounded() {
        let out = swept_outlines(&[[0.0, 0.0], [500.0, 0.0]], 0.5, &[]);
        assert_eq!(out.len(), 1);
    }
}
