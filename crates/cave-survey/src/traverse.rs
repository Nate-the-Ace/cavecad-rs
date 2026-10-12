//! One shot to a coordinate offset.
//!
//! Tape lengths are slope distances (the tape follows the shot): plan = d*cos(inc),
//! rise = d*sin(inc). A held-level tape (`Horizontal`) reads plan directly and cannot measure a
//! plumb, so a near-vertical shot has no offset in that mode.

use crate::vec3::Vec3;

/// How the tape was held.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TapeMode {
    /// The tape follows the shot (what every file format means).
    #[default]
    Slope,
    /// The tape was held level; the length is already the plan distance.
    Horizontal,
}

/// Inclination magnitude (degrees) at and beyond which a shot is a pitch: its compass reading is
/// noise, not a passage bearing. Coordinates still come from the numbers as recorded.
pub const PLUMB_DEG: f64 = 85.0;

/// What was read on one shot. Missing readings are `None`.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Reading {
    /// Tape length in metres.
    pub distance: f64,
    /// Foresight bearing, degrees clockwise from north.
    pub azimuth: Option<f64>,
    /// Foresight inclination, degrees, positive up.
    pub inclination: Option<f64>,
    /// Backsight bearing (reads 180 degrees opposite).
    pub back_azimuth: Option<f64>,
    /// Backsight inclination (reads sign-flipped).
    pub back_inclination: Option<f64>,
    /// The notes record no bearing at all (a plumbed pitch); treated as bearing 0.
    pub azimuth_omitted: bool,
}

/// A shot's displacement.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Offset {
    /// East, north, up.
    pub v: Vec3,
    /// Horizontal distance.
    pub plan: f64,
}

fn finite(v: Option<f64>) -> Option<f64> {
    v.filter(|n| n.is_finite())
}

/// Bearing to use: the circular mean of foresight and (backsight - 180) when both exist, so
/// 359 and 001 average to 0, not 180.
pub fn effective_azimuth(r: &Reading) -> Option<f64> {
    match (finite(r.azimuth), finite(r.back_azimuth)) {
        (Some(f), Some(b)) => {
            let (fs, bs) = (f.to_radians(), (b + 180.0).to_radians());
            let mean = (fs.sin() + bs.sin())
                .atan2(fs.cos() + bs.cos())
                .to_degrees();
            Some(mean.rem_euclid(360.0))
        }
        (Some(f), None) => Some(f),
        (None, Some(b)) => Some((b + 180.0).rem_euclid(360.0)),
        (None, None) => None,
    }
}

/// Inclination to use: the mean of foresight and sign-flipped backsight when both exist.
pub fn effective_inclination(r: &Reading) -> Option<f64> {
    match (finite(r.inclination), finite(r.back_inclination)) {
        (Some(f), Some(b)) => Some((f - b) / 2.0),
        (Some(f), None) => Some(f),
        (None, Some(b)) => Some(-b),
        (None, None) => None,
    }
}

/// True when the shot is a pitch (steeper than [`PLUMB_DEG`]). Absence of a reading is not
/// evidence of vertical.
pub fn is_plumb(r: &Reading) -> bool {
    effective_inclination(r).is_some_and(|i| i.abs() >= PLUMB_DEG)
}

/// The shot's displacement, or `None` when a reading is missing or the geometry is impossible.
pub fn offset(r: &Reading, mode: TapeMode) -> Option<Offset> {
    if !r.distance.is_finite() || r.distance < 0.0 {
        return None;
    }
    let inc = effective_inclination(r)?;
    let az = match effective_azimuth(r) {
        Some(a) => a,
        None if r.azimuth_omitted => 0.0,
        None => return None,
    };
    let (az, inc_r) = (az.to_radians(), inc.to_radians());
    let (plan, dz) = match mode {
        TapeMode::Slope => (r.distance * inc_r.cos(), r.distance * inc_r.sin()),
        TapeMode::Horizontal => {
            if inc.abs() >= 90.0 - 1e-9 {
                return None;
            }
            (r.distance, r.distance * inc_r.tan())
        }
    };
    Some(Offset {
        v: Vec3::new(plan * az.sin(), plan * az.cos(), dz),
        plan,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shot(d: f64, az: f64, inc: f64) -> Reading {
        Reading {
            distance: d,
            azimuth: Some(az),
            inclination: Some(inc),
            ..Default::default()
        }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn north_east_and_up() {
        let n = offset(&shot(10.0, 0.0, 0.0), TapeMode::Slope).unwrap();
        assert!(close(n.v.y, 10.0) && close(n.v.x, 0.0) && close(n.v.z, 0.0));
        let e = offset(&shot(10.0, 90.0, 0.0), TapeMode::Slope).unwrap();
        assert!(close(e.v.x, 10.0) && close(e.v.y, 0.0));
        let up = offset(&shot(10.0, 0.0, 30.0), TapeMode::Slope).unwrap();
        assert!(close(up.v.z, 5.0) && close(up.plan, 10.0 * 30f64.to_radians().cos()));
    }

    #[test]
    fn slope_is_shorter_in_plan_than_horizontal() {
        let s = offset(&shot(10.0, 0.0, 30.0), TapeMode::Slope).unwrap();
        let h = offset(&shot(10.0, 0.0, 30.0), TapeMode::Horizontal).unwrap();
        assert!(s.plan < h.plan && close(h.plan, 10.0));
    }

    #[test]
    fn horizontal_tape_cannot_measure_a_plumb() {
        assert!(offset(&shot(10.0, 0.0, 90.0), TapeMode::Horizontal).is_none());
        assert!(offset(&shot(10.0, 0.0, 90.0), TapeMode::Slope).is_some());
    }

    #[test]
    fn backsights_average_with_wraparound() {
        let r = Reading {
            distance: 1.0,
            azimuth: Some(359.0),
            back_azimuth: Some(181.0),
            inclination: Some(10.0),
            back_inclination: Some(-12.0),
            azimuth_omitted: false,
        };
        assert!(close(
            effective_azimuth(&r)
                .unwrap()
                .rem_euclid(360.0)
                .min(360.0 - effective_azimuth(&r).unwrap()),
            0.0
        ));
        assert!(close(effective_inclination(&r).unwrap(), 11.0));
    }

    #[test]
    fn missing_or_bad_readings_give_none() {
        assert!(
            offset(
                &Reading {
                    distance: 5.0,
                    azimuth: Some(0.0),
                    ..Default::default()
                },
                TapeMode::Slope
            )
            .is_none()
        );
        assert!(offset(&shot(f64::NAN, 0.0, 0.0), TapeMode::Slope).is_none());
        assert!(offset(&shot(-1.0, 0.0, 0.0), TapeMode::Slope).is_none());
    }

    #[test]
    fn omitted_bearing_is_allowed_for_pitches_only_when_flagged() {
        let flagged = Reading {
            distance: 10.0,
            inclination: Some(-90.0),
            azimuth_omitted: true,
            ..Default::default()
        };
        let o = offset(&flagged, TapeMode::Slope).unwrap();
        assert!(close(o.v.z, -10.0) && o.plan.abs() < 1e-9);
        assert!(is_plumb(&flagged));
        let unflagged = Reading {
            azimuth_omitted: false,
            ..flagged
        };
        assert!(offset(&unflagged, TapeMode::Slope).is_none());
    }

    #[test]
    fn unknown_inclination_is_not_plumb() {
        assert!(!is_plumb(&Reading {
            distance: 1.0,
            ..Default::default()
        }));
    }
}
