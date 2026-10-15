//! The camera as the crate needs it: a 4x4 view-projection matrix and the size of the view in pixels.

use serde::{Deserialize, Serialize};

/// A camera. `view_proj` is the 4x4 matrix `projection * view`, column-major (three.js `Matrix4.elements`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Camera {
    /// Column-major `projection * view`.
    pub view_proj: [f64; 16],
    /// Width of the view in pixels.
    pub width: f64,
    /// Height of the view in pixels.
    pub height: f64,
}

type V3 = [f64; 3];

impl Camera {
    /// A camera looking straight down at the world's xy plane, `px_per_unit` pixels to a unit, with world
    /// `(cx, cy)` in the middle of a `width` by `height` view. For tests and simple hosts.
    pub fn top(cx: f64, cy: f64, px_per_unit: f64, width: f64, height: f64) -> Camera {
        let (sx, sy) = (2.0 * px_per_unit / width, 2.0 * px_per_unit / height);
        #[rustfmt::skip]
        let m = [
            sx, 0.0, 0.0, 0.0,
            0.0, sy, 0.0, 0.0,
            0.0, 0.0, -0.001, 0.0,
            -sx * cx, -sy * cy, 0.0, 1.0,
        ];
        Camera {
            view_proj: m,
            width,
            height,
        }
    }

    /// A perspective camera at `eye` looking at `target` (`up` is world up), vertical field of view `fov_deg`. For tests
    /// and simple hosts; a browser hands over its own camera's matrix instead.
    pub fn look_at(eye: V3, target: V3, up: V3, fov_deg: f64, width: f64, height: f64) -> Camera {
        let norm = |v: V3| {
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            [v[0] / l, v[1] / l, v[2] / l]
        };
        let sub = |a: V3, b: V3| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        let cross = |a: V3, b: V3| {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        let dot = |a: V3, b: V3| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let z = norm(sub(eye, target));
        let x = norm(cross(up, z));
        let y = cross(z, x);
        // view: rows x, y, z, with the eye moved to the origin.
        let v = [
            [x[0], x[1], x[2], -dot(x, eye)],
            [y[0], y[1], y[2], -dot(y, eye)],
            [z[0], z[1], z[2], -dot(z, eye)],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let (n, f) = (0.1, 10_000.0);
        let t = 1.0 / (fov_deg.to_radians() / 2.0).tan();
        let p = [
            [t / (width / height), 0.0, 0.0, 0.0],
            [0.0, t, 0.0, 0.0],
            [0.0, 0.0, (f + n) / (n - f), 2.0 * f * n / (n - f)],
            [0.0, 0.0, -1.0, 0.0],
        ];
        let mut m = [0.0; 16];
        for r in 0..4 {
            for c in 0..4 {
                m[c * 4 + r] = (0..4).map(|k| p[r][k] * v[k][c]).sum();
            }
        }
        Camera {
            view_proj: m,
            width,
            height,
        }
    }

    fn mul(&self, p: [f64; 4]) -> [f64; 4] {
        let m = &self.view_proj;
        let mut out = [0.0; 4];
        for (r, o) in out.iter_mut().enumerate() {
            *o = m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r] * p[3];
        }
        out
    }

    /// The pixel a world point lands on (y down), or `None` when it is behind the camera.
    pub fn project(&self, p: V3) -> Option<[f64; 2]> {
        let c = self.mul([p[0], p[1], p[2], 1.0]);
        if c[3] <= 1e-12 {
            return None;
        }
        Some([
            (c[0] / c[3] * 0.5 + 0.5) * self.width,
            (0.5 - c[1] / c[3] * 0.5) * self.height,
        ])
    }

    /// The inverse of `view_proj` (column-major), or `None` when it is singular.
    fn inverse(&self) -> Option<[f64; 16]> {
        let m = &self.view_proj;
        let mut a = [[0.0f64; 8]; 4];
        for r in 0..4 {
            for c in 0..4 {
                a[r][c] = m[c * 4 + r];
            }
            a[r][4 + r] = 1.0;
        }
        for col in 0..4 {
            let piv = (col..4).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
            if a[piv][col].abs() < 1e-14 {
                return None;
            }
            a.swap(col, piv);
            let d = a[col][col];
            for v in a[col].iter_mut() {
                *v /= d;
            }
            for r in 0..4 {
                if r != col {
                    let f = a[r][col];
                    let row = a[col];
                    for (k, x) in a[r].iter_mut().enumerate() {
                        *x -= f * row[k];
                    }
                }
            }
        }
        let mut out = [0.0; 16];
        for r in 0..4 {
            for c in 0..4 {
                out[c * 4 + r] = a[r][4 + c];
            }
        }
        Some(out)
    }

    /// The ray through a pixel: a point on it and its direction (unit).
    pub fn ray(&self, px: [f64; 2]) -> Option<(V3, V3)> {
        let inv = self.inverse()?;
        let ndc = [
            (px[0] / self.width) * 2.0 - 1.0,
            1.0 - (px[1] / self.height) * 2.0,
        ];
        let at = |z: f64| -> Option<V3> {
            let v = [ndc[0], ndc[1], z, 1.0];
            let mut o = [0.0; 4];
            for (r, x) in o.iter_mut().enumerate() {
                *x = inv[r] * v[0] + inv[4 + r] * v[1] + inv[8 + r] * v[2] + inv[12 + r] * v[3];
            }
            (o[3].abs() > 1e-14).then(|| [o[0] / o[3], o[1] / o[3], o[2] / o[3]])
        };
        let (a, b) = (at(-1.0)?, at(1.0)?);
        let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let n = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        (n > 1e-14).then(|| (a, [d[0] / n, d[1] / n, d[2] / n]))
    }

    /// Where the pixel's ray meets the plane through `origin` with unit `normal`, or `None` when it is parallel to it.
    pub fn on_plane(&self, px: [f64; 2], origin: V3, normal: V3) -> Option<V3> {
        let (o, d) = self.ray(px)?;
        let denom = d[0] * normal[0] + d[1] * normal[1] + d[2] * normal[2];
        if denom.abs() < 1e-9 {
            return None;
        }
        let t = ((origin[0] - o[0]) * normal[0]
            + (origin[1] - o[1]) * normal[1]
            + (origin[2] - o[2]) * normal[2])
            / denom;
        Some([o[0] + d[0] * t, o[1] + d[1] * t, o[2] + d[2] * t])
    }
}
