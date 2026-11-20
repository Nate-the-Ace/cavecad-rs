//! The capture bundle (`.cvcap`): one shot as one file. Layout, little-endian:
//! `MAGIC`, `u16` format, `u32` header length, header JSON, then positions (`f32` × 3n), normals
//! (`f32` × 3n), indices (`u32` × 3m), then each keyframe's JPEG bytes in order. Counts and JPEG
//! lengths live in the header, so a reader never guesses.

use serde::{Deserialize, Serialize};

/// First bytes of every capture bundle.
pub const MAGIC: &[u8] = b"CVCAP\0";
/// The format this code writes. A reader refuses anything newer.
pub const FORMAT: u16 = 1;

/// A heading tap: the crosshair ray when the person tapped a station they could see.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HeadingTap {
    /// The station tapped (a `cave_id::Id` as text).
    pub target_station_id: String,
    /// Its label at the time, for people reading the file.
    pub target_label: String,
    /// Ray origin in the capture frame (ARKit world, metres, Y up).
    pub origin: [f64; 3],
    /// Ray direction in the capture frame (unit length).
    pub dir: [f64; 3],
}

/// Everything about a shot except its bulk arrays.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShotHeader {
    /// This shot's id (random, made by the app).
    pub shot_id: String,
    /// The server cave it belongs to.
    pub cave_id: String,
    /// The station the rig stood over (a `cave_id::Id` as text).
    pub station_id: String,
    /// That station's label at the time.
    pub station_label: String,
    /// When it was captured (milliseconds since 1970, from the phone).
    pub captured_at_ms: i64,
    /// Phone model identifier.
    pub device: String,
    /// Height of the phone above the station mark, metres.
    pub instrument_height_m: f64,
    /// Camera position in the capture frame when the shot started.
    pub start_camera: [f64; 3],
    /// Heading taps, first one wins.
    pub taps: Vec<HeadingTap>,
    /// How tracking went: `normal`, `limited` or `lost`.
    pub tracking: String,
}

/// One kept camera frame, for colouring the mesh later.
#[derive(Clone, Debug, PartialEq)]
pub struct Keyframe {
    /// Camera-to-capture transform, column-major 4×4.
    pub pose: [f32; 16],
    /// Camera intrinsics, column-major 3×3.
    pub intrinsics: [f32; 9],
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// The JPEG bytes.
    pub jpeg: Vec<u8>,
}

/// A whole shot: header, mesh in the capture frame, keyframes.
#[derive(Clone, Debug, PartialEq)]
pub struct Shot {
    /// The header.
    pub header: ShotHeader,
    /// Vertex positions, xyz per vertex.
    pub positions: Vec<f32>,
    /// Vertex normals, xyz per vertex.
    pub normals: Vec<f32>,
    /// Triangle vertex indices, three per triangle.
    pub indices: Vec<u32>,
    /// Kept camera frames.
    pub keyframes: Vec<Keyframe>,
}

/// Why bytes are not a usable shot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BundleError {
    /// The file does not start with [`MAGIC`].
    NotACapture,
    /// Written by a newer app (the format number).
    NewerFormat(u16),
    /// The file ends early.
    Truncated,
    /// The header is not readable.
    BadHeader,
    /// Mesh arrays disagree, an index points past the vertices, or a number is not finite.
    BadMesh,
}

#[derive(Serialize, Deserialize)]
struct Wire {
    header: ShotHeader,
    vertex_count: u32,
    triangle_count: u32,
    keyframes: Vec<WireKeyframe>,
}

#[derive(Serialize, Deserialize)]
struct WireKeyframe {
    pose: Vec<f32>,
    intrinsics: Vec<f32>,
    width: u32,
    height: u32,
    jpeg_len: u32,
}

/// Writes a shot as bundle bytes.
pub fn encode(shot: &Shot) -> Vec<u8> {
    let wire = Wire {
        header: shot.header.clone(),
        vertex_count: (shot.positions.len() / 3) as u32,
        triangle_count: (shot.indices.len() / 3) as u32,
        keyframes: shot
            .keyframes
            .iter()
            .map(|k| WireKeyframe {
                pose: k.pose.to_vec(),
                intrinsics: k.intrinsics.to_vec(),
                width: k.width,
                height: k.height,
                jpeg_len: k.jpeg.len() as u32,
            })
            .collect(),
    };
    let json = serde_json::to_vec(&wire).expect("header always serializes");
    let mut out =
        Vec::with_capacity(json.len() + shot.positions.len() * 8 + shot.indices.len() * 4);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT.to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(&json);
    for v in shot.positions.iter().chain(&shot.normals) {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for i in &shot.indices {
        out.extend_from_slice(&i.to_le_bytes());
    }
    for k in &shot.keyframes {
        out.extend_from_slice(&k.jpeg);
    }
    out
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], BundleError> {
        let end = self.at.checked_add(n).ok_or(BundleError::Truncated)?;
        let s = self.b.get(self.at..end).ok_or(BundleError::Truncated)?;
        self.at = end;
        Ok(s)
    }
    fn u16(&mut self) -> Result<u16, BundleError> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("2 bytes"),
        ))
    }
    fn u32(&mut self) -> Result<u32, BundleError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("4 bytes"),
        ))
    }
    fn f32s(&mut self, n: usize) -> Result<Vec<f32>, BundleError> {
        let raw = self.take(n.checked_mul(4).ok_or(BundleError::Truncated)?)?;
        Ok(raw
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes(*c))
            .collect())
    }
    fn u32s(&mut self, n: usize) -> Result<Vec<u32>, BundleError> {
        let raw = self.take(n.checked_mul(4).ok_or(BundleError::Truncated)?)?;
        Ok(raw
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes(*c))
            .collect())
    }
}

/// Reads bundle bytes back into a shot, checking everything a viewer or the matcher relies on.
pub fn decode(bytes: &[u8]) -> Result<Shot, BundleError> {
    if !bytes.starts_with(MAGIC) {
        return Err(BundleError::NotACapture);
    }
    let mut r = Reader {
        b: bytes,
        at: MAGIC.len(),
    };
    let format = r.u16()?;
    if format > FORMAT {
        return Err(BundleError::NewerFormat(format));
    }
    let len = r.u32()? as usize;
    let wire: Wire = serde_json::from_slice(r.take(len)?).map_err(|_| BundleError::BadHeader)?;
    let n = wire.vertex_count as usize;
    let m = wire.triangle_count as usize;
    let positions = r.f32s(n * 3)?;
    let normals = r.f32s(n * 3)?;
    let indices = r.u32s(m * 3)?;
    let mut keyframes = Vec::with_capacity(wire.keyframes.len());
    for k in wire.keyframes {
        let pose: [f32; 16] = k.pose.try_into().map_err(|_| BundleError::BadHeader)?;
        let intrinsics: [f32; 9] = k
            .intrinsics
            .try_into()
            .map_err(|_| BundleError::BadHeader)?;
        let jpeg = r.take(k.jpeg_len as usize)?.to_vec();
        keyframes.push(Keyframe {
            pose,
            intrinsics,
            width: k.width,
            height: k.height,
            jpeg,
        });
    }
    if r.at != bytes.len() {
        return Err(BundleError::Truncated);
    }
    let finite = positions.iter().chain(&normals).all(|v| v.is_finite());
    let in_range = indices.iter().all(|&i| (i as usize) < n);
    if !finite || !in_range {
        return Err(BundleError::BadMesh);
    }
    Ok(Shot {
        header: wire.header,
        positions,
        normals,
        indices,
        keyframes,
    })
}
