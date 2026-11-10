//! Clipping open polylines to a circle or a box: the pieces of a line that fall inside. Used by the
//! magnifier inset, which shows only the part of the map inside its circle (or box).

type P = [f64; 2];

/// The pieces of `line` inside the circle at `c` with radius `r` (a line that leaves and comes back
/// gives several pieces). Points exactly on the circle count as inside.
pub fn clip_to_circle(line: &[P], c: P, r: f64) -> Vec<Vec<P>> {
    clip(
        line,
        |p| (p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) <= r * r,
        |a, b| circle_hits(a, b, c, r),
    )
}

/// The pieces of `line` inside the box from `lo` to `hi` (corners, either order).
pub fn clip_to_box(line: &[P], lo: P, hi: P) -> Vec<Vec<P>> {
    let (x0, x1) = (lo[0].min(hi[0]), lo[0].max(hi[0]));
    let (y0, y1) = (lo[1].min(hi[1]), lo[1].max(hi[1]));
    clip(
        line,
        |p| p[0] >= x0 && p[0] <= x1 && p[1] >= y0 && p[1] <= y1,
        |a, b| box_hits(a, b, x0, y0, x1, y1),
    )
}

/// Walks the line segment by segment; `hits` gives the fractions (0..1) where a segment crosses the edge.
fn clip(line: &[P], inside: impl Fn(P) -> bool, hits: impl Fn(P, P) -> Vec<f64>) -> Vec<Vec<P>> {
    let mut out: Vec<Vec<P>> = Vec::new();
    let mut cur: Vec<P> = Vec::new();
    if line.len() == 1 && inside(line[0]) {
        return vec![line.to_vec()];
    }
    for w in line.windows(2) {
        let (a, b) = (w[0], w[1]);
        // the ends exactly as given (no rounding), anything between interpolated
        let at = |t: f64| {
            if t <= 0.0 {
                a
            } else if t >= 1.0 {
                b
            } else {
                [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
            }
        };
        // cut the segment at every crossing, then keep the parts whose middle is inside
        let mut ts = vec![0.0];
        let mut h = hits(a, b);
        h.retain(|t| *t > 0.0 && *t < 1.0);
        h.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
        ts.extend(h);
        ts.push(1.0);
        for k in 0..ts.len() - 1 {
            let (t0, t1) = (ts[k], ts[k + 1]);
            if t1 - t0 < 1e-12 {
                continue; // a touch, not a stretch of line
            }
            if inside(at((t0 + t1) / 2.0)) {
                let p0 = at(t0);
                if cur.last() != Some(&p0) {
                    if !cur.is_empty() {
                        out.push(std::mem::take(&mut cur));
                    }
                    cur.push(p0);
                }
                cur.push(at(t1));
            } else if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        }
    }
    if cur.len() >= 2 {
        out.push(cur);
    }
    out.retain(|piece| piece.len() >= 2);
    out
}

fn circle_hits(a: P, b: P, c: P, r: f64) -> Vec<f64> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let (fx, fy) = (a[0] - c[0], a[1] - c[1]);
    let qa = dx * dx + dy * dy;
    if qa == 0.0 {
        return Vec::new();
    }
    let qb = 2.0 * (fx * dx + fy * dy);
    let qc = fx * fx + fy * fy - r * r;
    let disc = qb * qb - 4.0 * qa * qc;
    if disc < 0.0 {
        return Vec::new();
    }
    let s = disc.sqrt();
    vec![(-qb - s) / (2.0 * qa), (-qb + s) / (2.0 * qa)]
}

fn box_hits(a: P, b: P, x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<f64> {
    let mut out = Vec::new();
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    for (d, s, edge, lo, hi, o) in [
        (dx, a[0], x0, y0, y1, 1),
        (dx, a[0], x1, y0, y1, 1),
        (dy, a[1], y0, x0, x1, 0),
        (dy, a[1], y1, x0, x1, 0),
    ] {
        if d == 0.0 {
            continue;
        }
        let t = (edge - s) / d;
        let other = if o == 1 { a[1] + dy * t } else { a[0] + dx * t };
        if other >= lo && other <= hi {
            out.push(t);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_through_a_circle_keeps_the_chord() {
        let out = clip_to_circle(&[[-5.0, 0.0], [5.0, 0.0]], [0.0, 0.0], 1.0);
        assert_eq!(out.len(), 1);
        assert!((out[0][0][0] + 1.0).abs() < 1e-9 && (out[0][1][0] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_line_that_leaves_and_returns_gives_two_pieces() {
        let line = [[-0.5, 0.0], [5.0, 0.0], [5.0, 0.2], [0.5, 0.2]];
        assert_eq!(clip_to_circle(&line, [0.0, 0.0], 1.0).len(), 2);
    }

    #[test]
    fn inside_stays_whole_and_outside_vanishes() {
        let line = [[0.0, 0.0], [0.2, 0.1], [0.3, -0.2]];
        assert_eq!(clip_to_circle(&line, [0.0, 0.0], 1.0), vec![line.to_vec()]);
        assert!(clip_to_circle(&[[3.0, 3.0], [4.0, 3.0]], [0.0, 0.0], 1.0).is_empty());
        // a tangent touch is not a piece
        assert!(clip_to_circle(&[[-2.0, 1.0], [2.0, 1.0]], [0.0, 0.0], 1.0).is_empty());
    }

    #[test]
    fn a_box_clips_on_every_side() {
        let out = clip_to_box(&[[-1.0, 0.5], [2.0, 0.5]], [0.0, 0.0], [1.0, 1.0]);
        assert_eq!(out, vec![vec![[0.0, 0.5], [1.0, 0.5]]]);
        let out = clip_to_box(&[[0.5, -1.0], [0.5, 2.0]], [1.0, 1.0], [0.0, 0.0]);
        assert_eq!(out, vec![vec![[0.5, 0.0], [0.5, 1.0]]]);
        assert!(clip_to_box(&[[2.0, 2.0], [3.0, 3.0]], [0.0, 0.0], [1.0, 1.0]).is_empty());
    }
}
