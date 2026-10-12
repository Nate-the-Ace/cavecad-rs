//! Keeps only the outside of a wall line.
//!
//! Where a wall line crosses itself (a splay tip or tick that lands past the next station's wall,
//! so the line doubles back), the part between the two visits is a loop of overlapping limits, not
//! passage boundary. The loop is cut out and the line is rejoined at the crossing point, so what is
//! left is the outer shell. Plan (x, y) decides the crossings; height is carried along the line.

use cave_survey::Vec3;

/// Intersection of segments `a-b` and `c-d` in plan, as the fraction along `a-b`, if they properly cross.
fn cross(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> Option<f64> {
    let (r, s) = ((b.x - a.x, b.y - a.y), (d.x - c.x, d.y - c.y));
    let den = r.0 * s.1 - r.1 * s.0;
    if den.abs() < 1e-12 {
        return None;
    }
    let (qp0, qp1) = (c.x - a.x, c.y - a.y);
    let t = (qp0 * s.1 - qp1 * s.0) / den;
    let u = (qp0 * r.1 - qp1 * r.0) / den;
    const E: f64 = 1e-9;
    (t > E && t < 1.0 - E && u > E && u < 1.0 - E).then_some(t)
}

/// Removes every self-crossing loop from `pts`, outermost first.
pub fn outer_shell(pts: &[Vec3]) -> Vec<Vec3> {
    let mut p = pts.to_vec();
    'again: loop {
        for i in 0..p.len().saturating_sub(3) {
            // The farthest segment this one crosses removes the biggest loop.
            for j in (i + 2..p.len() - 1).rev() {
                if let Some(t) = cross(p[i], p[i + 1], p[j], p[j + 1]) {
                    let (a, b) = (p[i], p[i + 1]);
                    let x = Vec3::new(
                        a.x + t * (b.x - a.x),
                        a.y + t * (b.y - a.y),
                        a.z + t * (b.z - a.z),
                    );
                    p.splice(i + 1..=j, [x]);
                    continue 'again;
                }
            }
        }
        return p;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f64, y: f64) -> Vec3 {
        Vec3::new(x, y, 0.0)
    }

    #[test]
    fn a_line_that_never_crosses_is_untouched() {
        let l = [v(0.0, 0.0), v(1.0, 1.0), v(2.0, 0.0), v(3.0, 1.0)];
        assert_eq!(outer_shell(&l), l);
    }

    #[test]
    fn a_doubled_back_loop_is_cut_out() {
        // Goes right, loops up and back across itself, then continues right.
        let l = [
            v(0.0, 0.0),
            v(4.0, 0.0),
            v(4.0, 2.0),
            v(2.0, 2.0),
            v(2.0, -1.0),
            v(6.0, -1.0),
        ];
        let o = outer_shell(&l);
        assert_eq!(
            o,
            vec![v(0.0, 0.0), v(2.0, 0.0), v(2.0, -1.0), v(6.0, -1.0)]
        );
    }

    #[test]
    fn nested_loops_all_go() {
        let l = [
            v(0.0, 0.0),
            v(10.0, 0.0),
            v(10.0, 5.0),
            v(5.0, 5.0),
            v(5.0, -5.0),
            v(3.0, -5.0),
            v(3.0, 2.0),
            v(8.0, 2.0),
            v(8.0, -2.0),
            v(12.0, -2.0),
        ];
        let o = outer_shell(&l);
        for w in 0..o.len().saturating_sub(3) {
            for k in w + 2..o.len() - 1 {
                assert!(cross(o[w], o[w + 1], o[k], o[k + 1]).is_none());
            }
        }
        assert_eq!(o.first(), l.first());
        assert_eq!(o.last(), l.last());
    }
}
