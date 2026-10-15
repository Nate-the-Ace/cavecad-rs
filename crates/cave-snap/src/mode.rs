//! The snap modes.

use serde::{Deserialize, Serialize};

/// A way of snapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// An end of a segment (every fit point of a drawn line).
    Endpoint,
    /// The middle of a segment (by arc length), or of a whole spline.
    Midpoint,
    /// The centre of a circle or arc; fires on the outline too.
    Center,
    /// The centroid of a closed path; fires on the outline too.
    GeometricCenter,
    /// A quarter point (0, 90, 180, 270 degrees) of a circle or arc.
    Quadrant,
    /// The insertion point of a symbol or text.
    Insertion,
    /// A point object.
    Node,
    /// A survey station.
    Station,
    /// Where two things really cross.
    Intersection,
    /// Where two things appear to cross from here.
    ApparentIntersection,
    /// Where the extended paths of two things cross (override only).
    ExtendedIntersection,
    /// Along the continuation of an acquired end.
    Extension,
    /// The foot of a perpendicular from the last point.
    Perpendicular,
    /// The point where a line from the last point touches a curve.
    Tangent,
    /// On a line through the last point parallel to an acquired segment.
    Parallel,
    /// The closest point on a thing.
    Nearest,
    /// Along a track line (horizontal, vertical or a polar angle) through an acquired or track point.
    Tracking,
    /// A base point to track from (override).
    From,
    /// The midpoint of two picked points (override).
    MidBetween,
    /// A temporary track point (override).
    TrackPoint,
}

/// How a mode ranks: point modes beat relational modes, and Nearest is last.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    /// A definite point of the geometry.
    Point,
    /// A point that follows from geometry and the last point.
    Relational,
    /// Anywhere on a thing.
    Nearest,
}

impl Mode {
    /// How this mode ranks.
    pub fn tier(self) -> Tier {
        match self {
            Mode::Endpoint
            | Mode::Midpoint
            | Mode::Center
            | Mode::GeometricCenter
            | Mode::Quadrant
            | Mode::Insertion
            | Mode::Node
            | Mode::Station => Tier::Point,
            Mode::Nearest => Tier::Nearest,
            _ => Tier::Relational,
        }
    }

    /// What the tooltip calls it.
    pub fn name(self) -> &'static str {
        match self {
            Mode::Endpoint => "Endpoint",
            Mode::Midpoint => "Midpoint",
            Mode::Center => "Center",
            Mode::GeometricCenter => "Geometric Center",
            Mode::Quadrant => "Quadrant",
            Mode::Insertion => "Insertion",
            Mode::Node => "Node",
            Mode::Station => "Station",
            Mode::Intersection => "Intersection",
            Mode::ApparentIntersection => "Apparent Intersection",
            Mode::ExtendedIntersection => "Extended Intersection",
            Mode::Extension => "Extension",
            Mode::Perpendicular => "Perpendicular",
            Mode::Tangent => "Tangent",
            Mode::Parallel => "Parallel",
            Mode::Nearest => "Nearest",
            Mode::Tracking => "Tracking",
            Mode::From => "From",
            Mode::MidBetween => "Mid Between 2 Points",
            Mode::TrackPoint => "Temporary Track Point",
        }
    }

    /// The modes that are switched on by default.
    pub fn defaults() -> Vec<Mode> {
        vec![
            Mode::Endpoint,
            Mode::Midpoint,
            Mode::Center,
            Mode::Quadrant,
            Mode::Intersection,
            Mode::Station,
            Mode::Insertion,
        ]
    }
}
