//! One station's wall points on one side: the LRUD tick and every splay that hit that side.

use crate::axes::relative_bearing;
use cave_survey::{Splay, StationLrud, Vec3, effective_azimuth};

/// Which wall.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// Left, facing the direction of travel (azimuth - 90).
    Left,
    /// Right (azimuth + 90).
    Right,
}

/// Was this side written "P" (open passage)?
pub fn side_open(lrud: Option<&StationLrud>, side: Side) -> bool {
    lrud.is_some_and(|l| match side {
        Side::Left => l.left_open,
        Side::Right => l.right_open,
    })
}

/// Did the party record anything about this station's walls in plan: a length either side, or a
/// "P"? "P" counts: a station read "P P" was looked at and found open, which is evidence.
pub fn has_plan_evidence(lrud: Option<&StationLrud>) -> bool {
    lrud.is_some_and(|l| l.left.is_some() || l.right.is_some() || l.left_open || l.right_open)
}

/// The bearing the L and R ticks are drawn along: the bearing the caver faced when pulling the
/// tapes, else the direction the passage runs (the foot of a pitch), else none. A measured length
/// aimed in a direction nobody knows is not half a wall point.
pub fn tick_azimuth(lrud: Option<&StationLrud>, passage: Option<f64>) -> Option<f64> {
    lrud.and_then(|l| l.azimuth.filter(|a| a.is_finite()))
        .or(passage.filter(|a| a.is_finite()))
}

/// The end of one LRUD tick, or `None` when not measured or open. A length of 0 is a real
/// measurement (the wall passes through the station) and needs no direction. With no bearing a
/// nonzero length gives no point: the one default that used to be picked, north, is a wall the
/// survey never saw.
pub fn tick_end(
    station: Vec3,
    azimuth: Option<f64>,
    side: Side,
    length: Option<f64>,
) -> Option<Vec3> {
    let length = length?;
    if length == 0.0 {
        return Some(station);
    }
    let az = azimuth.filter(|a| a.is_finite())?;
    let perp = match side {
        Side::Right => az + 90.0,
        Side::Left => az - 90.0,
    }
    .to_radians();
    Some(Vec3::new(
        station.x + length * perp.sin(),
        station.y + length * perp.cos(),
        station.z,
    ))
}

/// A station's wall points on one side, in along-passage order (ties keep input order, tick first).
///
/// The tick sits at the station's own elevation; a splay tip keeps its elevation. A splay exactly
/// along the passage axis is on the centreline and belongs to neither wall. No steepness filter: a
/// splay aimed at the ceiling contributes its plan projection like any other.
///
/// Divergence from the old code: with no passage direction at all (it would have coerced to
/// north) splays are left out rather than sided against an invented bearing.
pub fn station_wall_points(
    station: Vec3,
    passage_az: Option<f64>,
    lrud: Option<&StationLrud>,
    splays: &[&Splay],
    side: Side,
) -> Vec<Vec3> {
    let mut entries: Vec<(f64, i64, Vec3)> = Vec::new();
    if let Some(l) = lrud {
        let len = match side {
            Side::Left => l.left,
            Side::Right => l.right,
        };
        if let Some(p) = tick_end(station, tick_azimuth(lrud, passage_az), side, len) {
            entries.push((0.0, -1, p));
        }
    }
    if let Some(passage) = passage_az.filter(|a| a.is_finite()) {
        let (along_x, along_y) = (passage.to_radians().sin(), passage.to_radians().cos());
        for (i, sp) in splays.iter().enumerate() {
            let Some(az) = effective_azimuth(&sp.reading) else {
                continue;
            };
            let rel = relative_bearing(passage, az);
            if rel == 0.0 || rel == 180.0 || rel == -180.0 {
                continue;
            }
            if (side == Side::Right) != (rel > 0.0) {
                continue;
            }
            let o = sp.offset;
            entries.push((
                o.x * along_x + o.y * along_y,
                i as i64,
                Vec3::new(station.x + o.x, station.y + o.y, station.z + o.z),
            ));
        }
    }
    entries.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    entries.into_iter().map(|e| e.2).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn tick_ends_swing_off_the_bearing() {
        let o = Vec3::ZERO;
        let r = tick_end(o, Some(0.0), Side::Right, Some(2.0)).unwrap();
        assert!(near(r.x, 2.0) && near(r.y, 0.0), "right of north is east");
        let l = tick_end(o, Some(0.0), Side::Left, Some(2.0)).unwrap();
        assert!(near(l.x, -2.0));
    }

    #[test]
    fn zero_is_a_measurement_and_missing_is_not() {
        let at = Vec3::new(5.0, 7.0, 3.0);
        assert_eq!(tick_end(at, Some(0.0), Side::Left, Some(0.0)), Some(at));
        assert_eq!(
            tick_end(at, None, Side::Left, Some(0.0)),
            Some(at),
            "no bearing needed when the wall is at the station"
        );
        assert_eq!(tick_end(at, Some(0.0), Side::Left, None), None);
    }

    #[test]
    fn a_length_with_no_bearing_is_not_a_wall_point() {
        assert_eq!(tick_end(Vec3::ZERO, None, Side::Left, Some(5.0)), None);
        assert_eq!(
            tick_end(Vec3::ZERO, Some(f64::NAN), Side::Right, Some(5.0)),
            None
        );
    }

    #[test]
    fn tick_azimuth_prefers_what_the_caver_faced() {
        let faced = StationLrud {
            azimuth: Some(40.0),
            ..Default::default()
        };
        let pitch = StationLrud {
            azimuth: None,
            ..Default::default()
        };
        assert_eq!(tick_azimuth(Some(&faced), Some(90.0)), Some(40.0));
        assert_eq!(
            tick_azimuth(Some(&pitch), Some(90.0)),
            Some(90.0),
            "a pitch borrows the passage direction"
        );
        assert_eq!(tick_azimuth(Some(&pitch), None), None);
        assert_eq!(tick_azimuth(None, None), None);
    }

    #[test]
    fn p_counts_as_evidence() {
        assert!(!has_plan_evidence(None));
        assert!(!has_plan_evidence(Some(&StationLrud::default())));
        assert!(has_plan_evidence(Some(&StationLrud {
            left: Some(0.0),
            ..Default::default()
        })));
        assert!(has_plan_evidence(Some(&StationLrud {
            right_open: true,
            ..Default::default()
        })));
        assert!(
            !has_plan_evidence(Some(&StationLrud {
                up: Some(3.0),
                ..Default::default()
            })),
            "up and down are not plan evidence"
        );
    }
}
