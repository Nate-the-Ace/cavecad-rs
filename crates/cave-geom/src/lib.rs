//! 2D geometry for traced cave linework: points, paths with curves, boxes and moves.
//!
//! Pure math: no document, no drawing, no I/O. A [`Path`] is what a person traces. It is stored
//! whole and replaced whole when edited (never merged vertex by vertex).
//!
//! Conventions: x right, y up, angles in radians counter-clockwise, and a polyline
//! [`Vertex::bulge`] is `tan(sweep / 4)` of the arc from that vertex to the next, positive for
//! counter-clockwise, as in DXF.

mod arc;
mod path;
mod point;
mod transform;

pub use arc::Arc;
pub use path::{Path, PathError, Vertex};
pub use point::{Point, Rect};
pub use transform::Transform;

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn semicircle() -> Path {
        Path::Polyline {
            vertices: vec![
                Vertex {
                    p: Point::new(0.0, 0.0),
                    bulge: 1.0,
                },
                Vertex::at(2.0, 0.0),
            ],
            closed: false,
            widths: None,
        }
    }

    #[test]
    fn counter_clockwise_semicircle_bulges_below_a_rightward_chord() {
        let b = semicircle().bounds().unwrap();
        assert!(
            close(b.min.y, -1.0)
                && close(b.max.y, 0.0)
                && close(b.min.x, 0.0)
                && close(b.max.x, 2.0)
        );
        assert!(close(semicircle().length(), PI));
    }

    #[test]
    fn negative_bulge_goes_the_other_way() {
        let p = Path::Polyline {
            vertices: vec![
                Vertex {
                    p: Point::new(0.0, 0.0),
                    bulge: -1.0,
                },
                Vertex::at(2.0, 0.0),
            ],
            closed: false,
            widths: None,
        };
        let b = p.bounds().unwrap();
        assert!(close(b.max.y, 1.0) && close(b.min.y, 0.0));
    }

    #[test]
    fn mirroring_keeps_the_arc_on_the_same_side() {
        let m = semicircle().transformed(&Transform::scale(-1.0, 1.0));
        let b = m.bounds().unwrap();
        assert!(
            close(b.min.y, -1.0)
                && close(b.max.y, 0.0)
                && close(b.min.x, -2.0)
                && close(b.max.x, 0.0)
        );
    }

    #[test]
    fn rotation_moves_bounds() {
        let r = semicircle().transformed(&Transform::rotate(PI / 2.0));
        let b = r.bounds().unwrap();
        assert!(
            close(b.min.x, 0.0)
                && close(b.max.x, 1.0)
                && close(b.min.y, 0.0)
                && close(b.max.y, 2.0)
        );
    }

    #[test]
    fn closed_square_length_and_bounds() {
        let sq = Path::polyline(
            &[
                Point::new(0.0, 0.0),
                Point::new(3.0, 0.0),
                Point::new(3.0, 3.0),
                Point::new(0.0, 3.0),
            ],
            true,
        );
        assert!(close(sq.length(), 12.0));
        assert_eq!(sq.bounds().unwrap().size(), (3.0, 3.0));
    }

    #[test]
    fn validation_rejects_short_and_non_finite() {
        assert_eq!(
            Path::polyline(&[Point::new(0.0, 0.0)], false).validate(),
            Err(PathError::TooShort)
        );
        assert_eq!(
            Path::polyline(&[Point::new(0.0, 0.0), Point::new(f64::NAN, 1.0)], false).validate(),
            Err(PathError::NotFinite)
        );
        assert_eq!(semicircle().validate(), Ok(()));
    }

    #[test]
    fn degenerate_arcs_are_straight_not_nan() {
        let same = Path::Polyline {
            vertices: vec![
                Vertex {
                    p: Point::new(1.0, 1.0),
                    bulge: 1.0,
                },
                Vertex::at(1.0, 1.0),
            ],
            closed: false,
            widths: None,
        };
        assert!(close(same.length(), 0.0));
        assert!(same.bounds().unwrap().min.x.is_finite());
    }

    #[test]
    fn spline_bounds_and_length_use_fit_points() {
        let s = Path::Spline {
            points: vec![
                Point::new(0.0, 0.0),
                Point::new(1.0, 1.0),
                Point::new(2.0, 0.0),
            ],
            closed: false,
            widths: None,
        };
        assert_eq!(s.bounds().unwrap().size(), (2.0, 1.0));
        assert!(s.length() > 2.0);
        assert_eq!(s.validate(), Ok(()));
    }

    #[test]
    fn pen_weights_ride_along_without_changing_geometry() {
        let line = Path::polyline(
            &[
                Point::new(0.0, 0.0),
                Point::new(4.0, 0.0),
                Point::new(8.0, 0.0),
            ],
            false,
        );
        let inked = line.clone().with_widths(vec![0.4, 1.0, 1.6]).unwrap();
        assert_eq!(inked.widths(), Some(&[0.4, 1.0, 1.6][..]));
        assert_eq!(inked.bounds(), line.bounds());
        assert!(close(inked.length(), line.length()));
        let moved = inked.transformed(&Transform::translate(5.0, 5.0));
        assert_eq!(moved.widths(), inked.widths(), "widths survive a transform");
        assert_eq!(moved.validate(), Ok(()));
    }

    #[test]
    fn bad_pen_weights_are_refused() {
        let line = Path::polyline(&[Point::new(0.0, 0.0), Point::new(1.0, 0.0)], false);
        assert_eq!(
            line.clone().with_widths(vec![1.0]),
            Err(PathError::BadWidths),
            "one per point"
        );
        assert_eq!(
            line.clone().with_widths(vec![1.0, 0.0]),
            Err(PathError::BadWidths),
            "positive"
        );
        assert_eq!(
            line.with_widths(vec![1.0, f64::NAN]),
            Err(PathError::BadWidths)
        );
    }

    #[test]
    fn plain_paths_serialize_without_widths() {
        let line = Path::polyline(&[Point::new(0.0, 0.0), Point::new(1.0, 0.0)], false);
        assert!(!format!("{line:?}").is_empty());
        assert_eq!(line.widths(), None);
    }

    #[test]
    fn transform_composition_order() {
        let t = Transform::translate(1.0, 0.0).then(Transform::scale(2.0, 2.0));
        assert_eq!(t.apply(Point::new(0.0, 0.0)), Point::new(2.0, 0.0));
    }
}
