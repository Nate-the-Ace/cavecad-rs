//! The 3DEP elevation grid: a float32 GeoTIFF read into rows (row 0 at the top, north), metres
//! NAVD88. Ported from the old CsContour.parseFloatTiff: tiled (what the service sends, 128x128
//! tiles) or stripped, uncompressed only -- anything else is an error naming why, never a wrong grid.

use crate::request::Bounds;

/// An elevation grid, row-major, row 0 the north edge, values in metres.
#[derive(Clone, Debug, PartialEq)]
pub struct Grid {
    /// Columns.
    pub w: usize,
    /// Rows.
    pub h: usize,
    /// `w * h` values.
    pub values: Vec<f32>,
}

struct Reader<'a> {
    b: &'a [u8],
    le: bool,
}

impl Reader<'_> {
    fn u16(&self, o: usize) -> Result<u16, String> {
        let s = self.b.get(o..o + 2).ok_or("TIFF is cut short")?;
        Ok(if self.le {
            u16::from_le_bytes([s[0], s[1]])
        } else {
            u16::from_be_bytes([s[0], s[1]])
        })
    }
    fn u32(&self, o: usize) -> Result<u32, String> {
        let s = self.b.get(o..o + 4).ok_or("TIFF is cut short")?;
        let a = [s[0], s[1], s[2], s[3]];
        Ok(if self.le {
            u32::from_le_bytes(a)
        } else {
            u32::from_be_bytes(a)
        })
    }
    fn f32(&self, o: usize) -> Result<f32, String> {
        self.u32(o).map(f32::from_bits)
    }
}

struct Tag {
    ty: u16,
    count: u32,
    at: usize,
}

impl Grid {
    /// True for a hole: NaN, infinite, or far below any real ground (3DEP's no-data is a huge negative).
    pub fn no_data(v: f32) -> bool {
        !v.is_finite() || v < -12_000.0
    }

    /// Reads the service's float32 TIFF.
    pub fn parse_tiff(b: &[u8]) -> Result<Grid, String> {
        if b.len() < 8 {
            return Err(format!("not a TIFF: only {} bytes", b.len()));
        }
        let le = match &b[0..2] {
            b"II" => true,
            b"MM" => false,
            _ => return Err("not a TIFF: bad byte-order mark".into()),
        };
        let r = Reader { b, le };
        if r.u16(2)? != 42 {
            return Err("not a classic TIFF".into());
        }
        let ifd = r.u32(4)? as usize;
        let count = r.u16(ifd)? as usize;
        let mut tags = std::collections::HashMap::new();
        for i in 0..count {
            let e = ifd + 2 + i * 12;
            tags.insert(
                r.u16(e)?,
                Tag {
                    ty: r.u16(e + 2)?,
                    count: r.u32(e + 4)?,
                    at: e + 8,
                },
            );
        }
        let scalar = |t: u16| -> Result<Option<u32>, String> {
            match tags.get(&t) {
                None => Ok(None),
                Some(x) if x.ty == 3 => Ok(Some(r.u16(x.at)? as u32)),
                Some(x) => Ok(Some(r.u32(x.at)?)),
            }
        };
        let list = |t: u16| -> Result<Option<Vec<u32>>, String> {
            let Some(x) = tags.get(&t) else {
                return Ok(None);
            };
            let size = if x.ty == 3 { 2 } else { 4 };
            let base = if x.count as usize * size <= 4 {
                x.at
            } else {
                r.u32(x.at)? as usize
            };
            (0..x.count as usize)
                .map(|k| {
                    if x.ty == 3 {
                        r.u16(base + k * 2).map(u32::from)
                    } else {
                        r.u32(base + k * 4)
                    }
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Some)
        };
        let (w, h) = match (scalar(256)?, scalar(257)?) {
            (Some(w), Some(h)) if w > 0 && h > 0 => (w as usize, h as usize),
            _ => return Err("TIFF has no usable dimensions".into()),
        };
        if let Some(c) = scalar(259)?
            && c != 1
        {
            return Err(format!(
                "TIFF is compressed (compression {c}); only the service's uncompressed form is read"
            ));
        }
        if let Some(s) = scalar(277)?
            && s != 1
        {
            return Err(format!(
                "TIFF has {s} samples per pixel; expected one elevation band"
            ));
        }
        if scalar(258)? != Some(32) {
            return Err("TIFF sample is not 32-bit".into());
        }
        if scalar(339)? != Some(3) {
            return Err("TIFF sample is not IEEE float".into());
        }
        let mut values = Vec::with_capacity(w * h);
        if let (Some(tw), Some(tl), Some(offs)) = (scalar(322)?, scalar(323)?, list(324)?) {
            let (tw, tl) = (tw as usize, tl as usize);
            let across = w.div_ceil(tw);
            for row in 0..h {
                for col in 0..w {
                    let tile = (row / tl) * across + col / tw;
                    let base = *offs.get(tile).ok_or("TIFF tile missing")? as usize;
                    values.push(r.f32(base + 4 * ((row % tl) * tw + col % tw))?);
                }
            }
        } else if let Some(offs) = list(273)? {
            let rps = scalar(278)?.map_or(h, |v| v as usize);
            for row in 0..h {
                let base = *offs.get(row / rps).ok_or("TIFF strip missing")? as usize
                    + 4 * ((row % rps) * w);
                for col in 0..w {
                    values.push(r.f32(base + 4 * col)?);
                }
            }
        } else {
            return Err("TIFF has neither tiles nor strips".into());
        }
        Ok(Grid { w, h, values })
    }

    /// Lowest and highest real value, or `None` when the grid is all holes.
    pub fn range(&self) -> Option<(f64, f64)> {
        let mut out: Option<(f64, f64)> = None;
        for &v in &self.values {
            if Self::no_data(v) {
                continue;
            }
            let v = f64::from(v);
            out = Some(out.map_or((v, v), |(lo, hi)| (lo.min(v), hi.max(v))));
        }
        out
    }

    /// The value at cell `(c, r)`, or `None` for a hole or outside.
    pub fn at(&self, c: usize, r: usize) -> Option<f64> {
        if c >= self.w || r >= self.h {
            return None;
        }
        let v = self.values[r * self.w + c];
        (!Self::no_data(v)).then_some(f64::from(v))
    }

    /// Bilinear value at fractional cell coordinates (cell centres at integers), or `None` next to a hole.
    pub fn sample_at(&self, x: f64, y: f64) -> Option<f64> {
        if !(x >= 0.0 && y >= 0.0) || x > (self.w - 1) as f64 || y > (self.h - 1) as f64 {
            return None;
        }
        if self.w < 2 || self.h < 2 {
            return self.at(0, 0);
        }
        let c0 = (x.floor() as usize).min(self.w - 2);
        let r0 = (y.floor() as usize).min(self.h - 2);
        let (fx, fy) = (x - c0 as f64, y - r0 as f64);
        let (tl, tr) = (self.at(c0, r0)?, self.at(c0 + 1, r0)?);
        let (bl, br) = (self.at(c0, r0 + 1)?, self.at(c0 + 1, r0 + 1)?);
        Some(
            tl * (1.0 - fx) * (1.0 - fy)
                + tr * fx * (1.0 - fy)
                + bl * (1.0 - fx) * fy
                + br * fx * fy,
        )
    }

    /// The UTM point of fractional cell `(c, r)` (cell centres), for a grid cut to `b`.
    pub fn cell_utm(&self, b: &Bounds, c: f64, r: f64) -> (f64, f64) {
        let (sx, sy) = (b.width() / self.w as f64, b.height() / self.h as f64);
        (b.xmin + (c + 0.5) * sx, b.ymax - (r + 0.5) * sy)
    }

    /// The fractional cell of a UTM point (inverse of [`Grid::cell_utm`]).
    pub fn utm_cell(&self, b: &Bounds, e: f64, n: f64) -> (f64, f64) {
        let (sx, sy) = (b.width() / self.w as f64, b.height() / self.h as f64);
        ((e - b.xmin) / sx - 0.5, (b.ymax - n) / sy - 0.5)
    }
}
