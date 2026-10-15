//! What goes into a snap and what comes out.

use crate::camera::Camera;
use crate::mode::Mode;
use serde::{Deserialize, Serialize};

/// A drawing plane: an origin and two orthonormal axes. A thing's curve is described by `(u, v)` in this plane.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    /// Where `(0, 0)` is.
    pub origin: [f64; 3],
    /// Unit axis of the first coordinate.
    pub u: [f64; 3],
    /// Unit axis of the second coordinate (perpendicular to `u`).
    pub v: [f64; 3],
}

impl Frame {
    /// The horizontal plane at height `z` (the plan): `u` east, `v` north.
    pub fn plan(z: f64) -> Frame {
        Frame {
            origin: [0.0, 0.0, z],
            u: [1.0, 0.0, 0.0],
            v: [0.0, 1.0, 0.0],
        }
    }

    /// The world point at `(u, v)`.
    pub fn world(&self, p: [f64; 2]) -> [f64; 3] {
        [
            self.origin[0] + self.u[0] * p[0] + self.v[0] * p[1],
            self.origin[1] + self.u[1] * p[0] + self.v[1] * p[1],
            self.origin[2] + self.u[2] * p[0] + self.v[2] * p[1],
        ]
    }

    /// Where a world point falls in the plane (it is dropped onto the plane first).
    pub fn to_uv(&self, w: [f64; 3]) -> [f64; 2] {
        let d = [
            w[0] - self.origin[0],
            w[1] - self.origin[1],
            w[2] - self.origin[2],
        ];
        [dot(d, self.u), dot(d, self.v)]
    }

    /// The plane's unit normal.
    pub fn normal(&self) -> [f64; 3] {
        let n = cross(self.u, self.v);
        let l = dot(n, n).sqrt();
        if l < 1e-12 {
            [0.0, 0.0, 1.0]
        } else {
            [n[0] / l, n[1] / l, n[2] / l]
        }
    }

    /// A direction in the plane as a world vector.
    pub fn dir(&self, d: [f64; 2]) -> [f64; 3] {
        [
            self.u[0] * d[0] + self.v[0] * d[1],
            self.u[1] * d[0] + self.v[1] * d[1],
            self.u[2] * d[0] + self.v[2] * d[1],
        ]
    }
}

pub(crate) fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub(crate) fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// The shape of a thing, in its frame's `(u, v)` (a point is a world position).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Geometry {
    /// Straight and arc segments. `bulges[i]` is the bulge of the segment leaving vertex `i` (`tan(sweep / 4)`).
    Polyline {
        /// Corners.
        pts: Vec<[f64; 2]>,
        /// One bulge per corner (may be empty: all straight).
        #[serde(default)]
        bulges: Vec<f64>,
        /// The last corner joins the first.
        #[serde(default)]
        closed: bool,
    },
    /// A smooth curve through fit points (the drawn line).
    Spline {
        /// Fit points.
        pts: Vec<[f64; 2]>,
        /// The curve loops back.
        #[serde(default)]
        closed: bool,
    },
    /// A full circle.
    Circle {
        /// Centre.
        center: [f64; 2],
        /// Radius.
        radius: f64,
    },
    /// An arc of a circle.
    Arc {
        /// Centre.
        center: [f64; 2],
        /// Radius.
        radius: f64,
        /// Angle of the start, radians.
        start: f64,
        /// Signed sweep, radians (positive is counter-clockwise).
        sweep: f64,
    },
    /// A point.
    Point {
        /// Its world position.
        at: [f64; 3],
    },
}

/// What a thing is, which decides the point mode it offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThingKind {
    /// A drawn line.
    Line,
    /// A symbol (its first point is the insertion point).
    Symbol,
    /// A note or label.
    Text,
    /// A leader.
    Leader,
    /// A survey station.
    Station,
    /// A point object.
    Node,
}

/// One snappable thing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Thing {
    /// Its stable id, as text (never a list position).
    pub id: String,
    /// What it is.
    pub kind: ThingKind,
    /// The plane its curve is described in (ignored for a point).
    pub frame: Frame,
    /// Its shape.
    pub geom: Geometry,
    /// What the tooltip calls it (a station's name, a line's type).
    #[serde(default)]
    pub label: String,
}

/// Everything in view that can be snapped to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    /// The camera.
    pub camera: Camera,
    /// The things, hidden layers and the thing being edited already left out.
    pub things: Vec<Thing>,
    /// The drawing plane of the view (Top, Bottom, Profile, Section) or `None` from an oblique camera: angles and
    /// distances (tracking, polar, the distance field) are only meaningful in it.
    #[serde(default)]
    pub plane: Option<Frame>,
}

/// The settings of snapping.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// The running modes.
    pub modes: Vec<Mode>,
    /// Half the width of the aperture box, in pixels.
    pub aperture: f64,
    /// Polar tracking increments, degrees.
    pub polar: Vec<f64>,
    /// Tracking guides off acquired and track points.
    pub tracking: bool,
    /// How far apart (world units) two things may be where they cross on screen for plain Intersection.
    pub intersection_tol: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            modes: Mode::defaults(),
            aperture: 10.0,
            polar: vec![15.0, 30.0, 45.0, 90.0],
            tracking: true,
            intersection_tol: 0.05,
        }
    }
}

/// Where a candidate came from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Source {
    /// The thing.
    pub id: String,
    /// Its segment (for an Endpoint, the vertex index).
    pub seg: usize,
    /// The parameter along that segment, 0 to 1.
    pub t: f64,
}

/// One place the cursor could snap to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    /// How it was found.
    pub mode: Mode,
    /// Where it is on the screen.
    pub screen: [f64; 2],
    /// The point to use.
    pub world: [f64; 3],
    /// The thing it is on, when it is on one.
    pub source: Option<Source>,
    /// The tooltip text: the mode, and the thing when it has a name.
    pub label: String,
    /// The distance used for ranking, pixels.
    pub dist: f64,
}

/// What a guide is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuideKind {
    /// The continuation of an acquired end.
    Extension,
    /// A horizontal or vertical track line.
    Track,
    /// A polar-angle track line.
    Polar,
    /// A line parallel to an acquired segment.
    Parallel,
}

/// A line to draw on the overlay, and to place a point along.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Guide {
    /// What it is.
    pub kind: GuideKind,
    /// A point on it (world).
    pub origin: [f64; 3],
    /// Its direction (world, unit) when it is straight; `None` for an arc.
    pub dir: Option<[f64; 3]>,
    /// The guide as pixels to draw (a polyline).
    pub px: Vec<[f64; 2]>,
    /// The tooltip text: `Extension: 3.20 < 45°`.
    pub label: String,
}

/// Something the cursor has been left over long enough to acquire.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Acquired {
    /// An end of a line, with the way the line goes on from it.
    Extension {
        /// The end (world).
        at: [f64; 3],
        /// The thing it is on.
        id: String,
        /// How the line continues.
        cont: Continuation,
    },
    /// A segment's direction, for Parallel.
    Parallel {
        /// Direction (world, unit).
        dir: [f64; 3],
        /// The thing it is on.
        id: String,
    },
    /// A point to track from.
    Track {
        /// The point (world).
        at: [f64; 3],
    },
}

/// How a line goes on past its end.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Continuation {
    /// Straight on.
    Line {
        /// Unit direction (world).
        dir: [f64; 3],
    },
    /// Round the same circle.
    Arc {
        /// The circle's plane.
        frame: Frame,
        /// Its centre in the plane.
        center: [f64; 2],
        /// Its radius.
        radius: f64,
        /// Angle at the end, radians.
        angle: f64,
        /// Continuing counter-clockwise.
        ccw: bool,
    },
}

/// The state of a pick in progress. Passed in and returned, never held.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Session {
    /// Things acquired by hovering.
    #[serde(default)]
    pub acquired: Vec<Acquired>,
    /// Temporary track points: tracking guides run through them until the pick ends.
    #[serde(default)]
    pub track_points: Vec<[f64; 3]>,
    /// The From base point for the next pick.
    #[serde(default)]
    pub from_base: Option<[f64; 3]>,
    /// The first of the two points of Mid Between 2 Points.
    #[serde(default)]
    pub m2p_first: Option<[f64; 3]>,
    /// The previous point of the pick in progress (for Perpendicular, Tangent and Parallel).
    #[serde(default)]
    pub last_point: Option<[f64; 3]>,
    /// The one-pick override: search only this mode for the next pick.
    #[serde(default, rename = "override")]
    pub override_mode: Option<Mode>,
}

/// What a snap found.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SnapResult {
    /// The best candidate.
    pub best: Option<Candidate>,
    /// The rest in Tab order (the best comes round again after them).
    pub others: Vec<Candidate>,
    /// Guides to draw.
    pub guides: Vec<Guide>,
    /// What hovering here could acquire.
    pub acquirable: Vec<Acquired>,
}
