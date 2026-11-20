//! Thinning a scan's points for overviews and thumbnails: one point per occupied grid cell.
//! Pure: numbers in, numbers out.

use std::collections::HashMap;

/// Thins `points` (x y z per point, back to back) on a cubic grid of `cell_m` metres: one point per
/// occupied cell, at the mean of the points that fell in it. Points with a non-finite coordinate
/// are skipped (and a trailing partial triple is ignored). The output order is the order in which
/// each cell was first entered, so the same input always gives the same output. A `cell_m` that is
/// not a positive finite number thins nothing: the finite points come back as they were.
pub fn thin_points(points: &[f32], cell_m: f32) -> Vec<f32> {
    let finite = points
        .as_chunks::<3>()
        .0
        .iter()
        .filter(|p| p.iter().all(|x| x.is_finite()));
    if !(cell_m.is_finite() && cell_m > 0.0) {
        return finite.flatten().copied().collect();
    }
    // Lookup only; cells are numbered as first entered so the map's order never reaches the output.
    let mut cell_of: HashMap<[i64; 3], usize> = HashMap::new();
    let mut sums: Vec<([f64; 3], u32)> = Vec::new();
    for p in finite {
        let key = p.map(|x| (f64::from(x) / f64::from(cell_m)).floor() as i64);
        let id = *cell_of.entry(key).or_insert_with(|| {
            sums.push(([0.0; 3], 0));
            sums.len() - 1
        });
        let s = &mut sums[id];
        for (acc, x) in s.0.iter_mut().zip(p) {
            *acc += f64::from(*x);
        }
        s.1 += 1;
    }
    let mut out = Vec::with_capacity(sums.len() * 3);
    for (s, n) in &sums {
        out.extend(s.iter().map(|x| (*x / f64::from(*n)) as f32));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_point_per_cell_at_the_mean() {
        // two points share cell (0,0,0); one sits alone in cell (2,0,0) on a 5 cm grid
        let pts = [0.01, 0.01, 0.01, 0.03, 0.02, 0.04, 0.12, 0.0, 0.0];
        let out = thin_points(&pts, 0.05);
        assert_eq!(out.len(), 6, "{out:?}");
        assert!(
            (out[0] - 0.02).abs() < 1e-6 && (out[1] - 0.015).abs() < 1e-6,
            "{out:?}"
        );
        assert!((out[2] - 0.025).abs() < 1e-6, "{out:?}");
        assert_eq!(&out[3..], &[0.12, 0.0, 0.0]);
    }

    #[test]
    fn negative_coordinates_floor_into_their_own_cell() {
        // -0.01 and 0.01 straddle zero: two cells, not one
        let out = thin_points(&[-0.01, 0.0, 0.0, 0.01, 0.0, 0.0], 0.05);
        assert_eq!(out.len(), 6);
    }

    #[test]
    fn order_is_first_entered_and_repeatable() {
        let mut pts = Vec::new();
        for i in 0..500 {
            let t = i as f32 * 0.003;
            pts.extend_from_slice(&[t.sin() * 3.0 + 0.01, t.cos() * 2.0 + 0.01, 0.01]);
        }
        let a = thin_points(&pts, 0.05);
        let b = thin_points(&pts, 0.05);
        assert_eq!(a, b);
        // the first output point is in the cell of the first input point
        let cell = |p: &[f32]| {
            p.iter()
                .map(|x| (f64::from(*x) / f64::from(0.05f32)).floor() as i64)
                .collect::<Vec<_>>()
        };
        assert_eq!(cell(&a[..3]), cell(&pts[..3]));
        assert!(a.len() < pts.len());
    }

    #[test]
    fn non_finite_points_are_skipped() {
        let pts = [
            f32::NAN,
            0.0,
            0.0,
            1.01,
            1.01,
            1.01,
            0.0,
            f32::INFINITY,
            0.0,
            1.02,
            1.02,
            1.02,
        ];
        let out = thin_points(&pts, 0.05);
        assert_eq!(out.len(), 3, "{out:?}");
        assert!(out.iter().all(|x| x.is_finite()));
        assert!((out[0] - 1.015).abs() < 1e-6, "{out:?}");
    }

    #[test]
    fn empty_partial_and_bad_cell() {
        assert!(thin_points(&[], 0.05).is_empty());
        assert!(thin_points(&[1.0, 2.0], 0.05).is_empty());
        let pts = [0.0, 0.0, 0.0, 0.001, 0.0, 0.0, f32::NAN, 0.0, 0.0];
        assert_eq!(thin_points(&pts, 0.0), &pts[..6]);
        assert_eq!(thin_points(&pts, f32::NAN), &pts[..6]);
        assert_eq!(thin_points(&pts, -1.0), &pts[..6]);
    }

    #[test]
    fn many_points_in_one_cell_become_one() {
        let pts: Vec<f32> = (0..1000)
            .flat_map(|i| [0.001 * (i % 40) as f32, 0.0, 0.0])
            .collect();
        let out = thin_points(&pts, 0.05);
        assert_eq!(out.len(), 3);
        assert!((out[0] - 0.0195).abs() < 1e-5, "{out:?}");
    }
}
