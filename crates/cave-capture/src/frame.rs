//! Where a shot sits in the cave. ARKit's capture frame is metric, gravity-aligned, Y up, with an
//! arbitrary heading. The survey frame is x east, y north, z up. Gravity is shared, scale is never
//! changed, so placing a shot is one turn about the vertical plus a shift.

use cave_survey::Vec3;

/// A capture-frame point in survey axes (before any turn): `(x, -z, y)`.
pub fn capture_to_survey(p: [f64; 3]) -> Vec3 {
    Vec3 {
        x: p[0],
        y: -p[2],
        z: p[1],
    }
}

/// Compass bearing of a horizontal direction, degrees clockwise from north, in `0..360`.
pub fn bearing_deg(east: f64, north: f64) -> f64 {
    east.atan2(north).to_degrees().rem_euclid(360.0)
}

/// A shot's place in the survey: survey = turn(capture_to_survey(p − start)) + origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// Clockwise turn applied to bearings, degrees.
    pub turn_deg: f64,
    /// Survey position of the shot's start camera.
    pub origin: Vec3,
    /// Capture-frame start camera position, taken off before turning.
    pub start: [f64; 3],
}

impl Placement {
    /// A capture-frame point in survey coordinates.
    pub fn apply(&self, p: [f64; 3]) -> Vec3 {
        let v = capture_to_survey([
            p[0] - self.start[0],
            p[1] - self.start[1],
            p[2] - self.start[2],
        ]);
        let (s, c) = self.turn_deg.to_radians().sin_cos();
        Vec3 {
            x: v.x * c + v.y * s + self.origin.x,
            y: -v.x * s + v.y * c + self.origin.y,
            z: v.z + self.origin.z,
        }
    }
}

/// Places a shot from a heading tap: the tap ray (capture frame) pointed at `target`, the rig stood
/// over `station`, the phone `height_m` above the mark, the shot started with the camera at `start`.
/// `None` when the ray is within 2° of vertical, or the target is under 1 cm sideways of the
/// station (neither has a bearing).
pub fn heading_from_tap(
    station: Vec3,
    target: Vec3,
    dir: [f64; 3],
    start: [f64; 3],
    height_m: f64,
) -> Option<Placement> {
    let d = capture_to_survey(dir);
    let horiz = (d.x * d.x + d.y * d.y).sqrt();
    let len = (horiz * horiz + d.z * d.z).sqrt();
    if len == 0.0 || horiz / len < 2f64.to_radians().sin() {
        return None;
    }
    // A target straight above or below the station has no bearing; never invent one.
    if (target.x - station.x).hypot(target.y - station.y) < 0.01 {
        return None;
    }
    let want = bearing_deg(target.x - station.x, target.y - station.y);
    let have = bearing_deg(d.x, d.y);
    Some(Placement {
        turn_deg: (want - have).rem_euclid(360.0),
        origin: Vec3 {
            x: station.x,
            y: station.y,
            z: station.z + height_m,
        },
        start,
    })
}
