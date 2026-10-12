//! Linework is drawn as a smooth curve through a few fit points: a centripetal Catmull-Rom spline,
//! which never overshoots or loops on uneven spacing. This is the same curve the web app draws
//! (`apps/web/src/lib/spline.ts`), so exported linework and ornament follow the screen exactly;
//! `tests/spline_golden.rs` checks it against that code.

/// Samples of the curve through `pts`, `per` samples per span, starting at the first point. A
/// closed curve also runs from the last point back to the first, smoothly. Fewer than three points
/// are returned as they are.
pub fn smooth(pts: &[[f64; 2]], closed: bool, per: usize) -> Vec<[f64; 2]> {
    let n = pts.len();
    if n < 3 || per == 0 {
        return pts.to_vec();
    }
    let at = |i: isize| pts[i.rem_euclid(n as isize) as usize];
    let ext: Vec<[f64; 2]> = if closed {
        let mut v = vec![at(-1)];
        v.extend_from_slice(pts);
        v.push(at(n as isize));
        v.push(at(n as isize + 1));
        v
    } else {
        let mut v = vec![[2.0 * pts[0][0] - pts[1][0], 2.0 * pts[0][1] - pts[1][1]]];
        v.extend_from_slice(pts);
        v.push([
            2.0 * pts[n - 1][0] - pts[n - 2][0],
            2.0 * pts[n - 1][1] - pts[n - 2][1],
        ]);
        v
    };
    let spans = if closed { n } else { n - 1 };
    let knot = |a: [f64; 2], b: [f64; 2]| ((b[0] - a[0]).hypot(b[1] - a[1])).sqrt().max(1e-6);
    let mut out = vec![pts[0]];
    for i in 0..spans {
        let (p0, p1, p2, p3) = (ext[i], ext[i + 1], ext[i + 2], ext[i + 3]);
        let t0 = 0.0;
        let t1 = t0 + knot(p0, p1);
        let t2 = t1 + knot(p1, p2);
        let t3 = t2 + knot(p2, p3);
        for s in 1..=per {
            let t = t1 + (t2 - t1) * s as f64 / per as f64;
            let mix = |a: [f64; 2], b: [f64; 2], ta: f64, tb: f64| {
                [
                    a[0] * ((tb - t) / (tb - ta)) + b[0] * ((t - ta) / (tb - ta)),
                    a[1] * ((tb - t) / (tb - ta)) + b[1] * ((t - ta) / (tb - ta)),
                ]
            };
            let a1 = mix(p0, p1, t0, t1);
            let a2 = mix(p1, p2, t1, t2);
            let a3 = mix(p2, p3, t2, t3);
            let b1 = mix(a1, a2, t0, t2);
            let b2 = mix(a2, a3, t1, t3);
            out.push(mix(b1, b2, t1, t2));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_curve_passes_through_every_fit_point() {
        let pts = [[0.0, 0.0], [3.0, 2.0], [5.0, -1.0], [9.0, 4.0]];
        let c = smooth(&pts, false, 10);
        assert_eq!(c.len(), 1 + 3 * 10);
        for (k, p) in pts.iter().enumerate() {
            let q = c[k * 10];
            assert!((q[0] - p[0]).hypot(q[1] - p[1]) < 1e-9, "point {k}");
        }
    }

    #[test]
    fn a_closed_curve_returns_to_its_start_without_a_corner() {
        let pts = [[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]];
        let c = smooth(&pts, true, 8);
        assert_eq!(c.len(), 1 + 4 * 8);
        let (a, b) = (c[0], c[c.len() - 1]);
        assert!((a[0] - b[0]).hypot(a[1] - b[1]) < 1e-9);
    }

    #[test]
    fn two_points_are_a_straight_line_and_nothing_extra() {
        let pts = [[0.0, 0.0], [1.0, 1.0]];
        assert_eq!(smooth(&pts, false, 10), pts.to_vec());
        assert!(smooth(&[], false, 10).is_empty());
    }
}
