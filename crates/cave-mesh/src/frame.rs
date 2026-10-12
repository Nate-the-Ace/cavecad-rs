//! A passage's local frame and the cross-section ring measured at a station.

use cave_survey::{Splay, StationLrud, Vec3};

/// Unit vectors for a passage heading along `forward`: `right` is horizontal wherever the passage
/// is not vertical, so a cross-section stands the way a caver would draw it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// Direction of travel.
    pub forward: Vec3,
    /// To the caver's right.
    pub right: Vec3,
    /// Up, perpendicular to both.
    pub up: Vec3,
}

pub(crate) fn dot(a: Vec3, b: Vec3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

pub(crate) fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

/// Unit vector, or `None` when there is no direction to normalise (never a zero vector, which
/// would carry on and turn into NaN three operations later).
pub(crate) fn normalize(a: Vec3) -> Option<Vec3> {
    let len = a.norm();
    (len.is_finite() && len >= 1e-12).then(|| Vec3::new(a.x / len, a.y / len, a.z / len))
}

/// The frame for a passage heading along `dir`. In a vertical shaft every horizontal direction is
/// equally perpendicular and noise would pick one, so the frame would spin from station to station;
/// north is used instead: arbitrary, but the same arbitrary every time.
pub fn frame_at(dir: Vec3) -> Option<Frame> {
    let forward = normalize(dir)?;
    let right = normalize(cross(forward, Vec3::new(0.0, 0.0, 1.0)))
        .or_else(|| normalize(cross(forward, Vec3::new(0.0, 1.0, 0.0))))?;
    let up = normalize(cross(right, forward))?;
    Some(Frame { forward, right, up })
}

/// One measured wall point of a cross-section.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RingPoint {
    /// Position in the world.
    pub p: Vec3,
    /// Angle around the passage axis (radians, `atan2(up, right)`).
    pub angle: f64,
}

/// The cross-section ring at a station: the LRUD ticks and every splay tip, projected into the
/// plane perpendicular to the passage and sorted by angle. A tick of 0 is a real measurement (the
/// wall is at the station). Unmeasured sides and splays along the axis add nothing.
pub fn ring_at(
    station: Vec3,
    dir: Vec3,
    lrud: Option<&StationLrud>,
    splays: &[&Splay],
) -> Vec<RingPoint> {
    let Some(frame) = frame_at(dir) else {
        return Vec::new();
    };
    let mut local: Vec<(f64, f64)> = Vec::new();
    let mut add = |u: f64, v: f64| {
        if u.is_finite() && v.is_finite() {
            local.push((u, v));
        }
    };
    if let Some(l) = lrud {
        for (d, us, vs) in [
            (l.right, 1.0, 0.0),
            (l.left, -1.0, 0.0),
            (l.up, 0.0, 1.0),
            (l.down, 0.0, -1.0),
        ] {
            if let Some(d) = d.filter(|d| d.is_finite()) {
                add(us * d, vs * d);
            }
        }
    }
    for s in splays {
        let (u, v) = (dot(s.offset, frame.right), dot(s.offset, frame.up));
        if u.abs() < 1e-9 && v.abs() < 1e-9 {
            continue;
        }
        add(u, v);
    }
    let mut ring: Vec<RingPoint> = local
        .into_iter()
        .map(|(u, v)| RingPoint {
            p: Vec3::new(
                station.x + u * frame.right.x + v * frame.up.x,
                station.y + u * frame.right.y + v * frame.up.y,
                station.z + u * frame.right.z + v * frame.up.z,
            ),
            angle: v.atan2(u),
        })
        .collect();
    // Stable, and -0.0 equals 0.0 (a 0 m floor tick has angle -0.0): ties keep measurement order.
    ring.sort_by(|a, b| {
        a.angle
            .partial_cmp(&b.angle)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    ring
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Vec3, x: f64, y: f64, z: f64) -> bool {
        (a.x - x).abs() < 1e-9 && (a.y - y).abs() < 1e-9 && (a.z - z).abs() < 1e-9
    }

    #[test]
    fn heading_north_has_east_on_the_right_and_z_up() {
        let f = frame_at(Vec3::new(0.0, 5.0, 0.0)).unwrap();
        assert!(
            near(f.right, 1.0, 0.0, 0.0)
                && near(f.up, 0.0, 0.0, 1.0)
                && near(f.forward, 0.0, 1.0, 0.0)
        );
    }

    #[test]
    fn a_vertical_shaft_has_the_same_frame_at_every_station() {
        let down = frame_at(Vec3::new(0.0, 0.0, -9.0)).unwrap();
        assert!(near(down.right, 1.0, 0.0, 0.0), "east, whatever noise says");
        assert!(near(down.up, 0.0, 1.0, 0.0) || near(down.up, 0.0, -1.0, 0.0));
        let noisy = frame_at(Vec3::new(1e-15, -1e-15, -2.0)).unwrap();
        assert!(
            near(noisy.right, 1.0, 0.0, 0.0) && near(noisy.up, 0.0, 1.0, 0.0),
            "noise does not spin the frame"
        );
        assert!(frame_at(Vec3::ZERO).is_none());
    }

    #[test]
    fn the_ring_has_one_point_per_measurement_sorted_by_angle() {
        let lrud = StationLrud {
            left: Some(2.0),
            right: Some(3.0),
            up: Some(1.5),
            down: Some(0.5),
            ..Default::default()
        };
        let ring = ring_at(
            Vec3::new(10.0, 10.0, 5.0),
            Vec3::new(0.0, 1.0, 0.0),
            Some(&lrud),
            &[],
        );
        assert_eq!(ring.len(), 4);
        assert!(ring.windows(2).all(|w| w[0].angle <= w[1].angle));
        let right = ring.iter().find(|r| r.angle.abs() < 1e-9).unwrap();
        assert!(near(right.p, 13.0, 10.0, 5.0));
        let up = ring
            .iter()
            .find(|r| (r.angle - std::f64::consts::FRAC_PI_2).abs() < 1e-9)
            .unwrap();
        assert!(near(up.p, 10.0, 10.0, 6.5));
    }

    #[test]
    fn nothing_measured_gives_no_ring_and_a_zero_tick_is_kept() {
        assert!(ring_at(Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0), None, &[]).is_empty());
        let lrud = StationLrud {
            left: Some(0.0),
            ..Default::default()
        };
        assert_eq!(
            ring_at(Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0), Some(&lrud), &[]).len(),
            1,
            "the wall is at the station"
        );
    }
}
