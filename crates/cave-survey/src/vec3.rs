//! A point or offset in 3D.

/// Three coordinates in metres: x east, y north, z up.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Vec3 {
    /// East.
    pub x: f64,
    /// North.
    pub y: f64,
    /// Up.
    pub z: f64,
}

impl Vec3 {
    /// The origin.
    pub const ZERO: Vec3 = Vec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    /// A vector from components.
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Vec3 { x, y, z }
    }

    /// Component-wise sum.
    pub fn plus(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }

    /// Component-wise difference.
    pub fn minus(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }

    /// Length in 3D.
    pub fn norm(self) -> f64 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    /// Length in plan (ignoring z).
    pub fn horizontal(self) -> f64 {
        self.x.hypot(self.y)
    }
}
