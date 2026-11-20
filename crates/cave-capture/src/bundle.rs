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

/// How the phone's compass stood when a shot started. A compass shot's capture frame is ARKit's
/// `.gravityAndHeading` (x east, y up, z south), so it needs no heading tap.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Compass {
    /// The heading was true north (location allowed); false means magnetic north.
    pub true_north: bool,
    /// iOS's heading accuracy, degrees; negative when iOS said the heading was invalid.
    pub accuracy_deg: f64,
}

/// A station card (CURRENT, FORWARD or REAR) the phone saw while scanning, summarised over all its sightings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CardSeen {
    /// The card's glyph id (2 CURRENT, 12 FORWARD, 26 REAR).
    pub id: u32,
    /// The card's printed word, for people reading the file.
    pub word: String,
    /// Running median of the card centre's 3D position in the capture frame, metres.
    pub point: [f64; 3],
    /// The card's four corners in the capture frame (median per corner); empty when depth never covered them.
    #[serde(default)]
    pub corners: Vec<[f64; 3]>,
    /// How many detections went into the median.
    pub sightings: u32,
    /// How far the centre wandered between sightings (largest distance from the median), metres.
    pub spread_m: f64,
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
    /// Heading taps, first one wins. Compass shots write none; absent in a file means none.
    #[serde(default)]
    pub taps: Vec<HeadingTap>,
    /// How tracking went: `normal`, `limited` or `lost`.
    pub tracking: String,
    /// The iOS version string the app reported (ARKit behaviour changes with it).
    pub arkit_version: String,
    /// Gravity direction in the capture frame (ARKit's gravity alignment makes it `(0,-1,0)`).
    pub gravity: [f64; 3],
    /// The compass at the start of a compass shot; `None` for a tap shot (and for shots written
    /// before compass shots existed).
    #[serde(default)]
    pub compass: Option<Compass>,
    /// Station cards seen during the scan; absent in files written before cards were detected.
    #[serde(default)]
    pub cards: Vec<CardSeen>,
    /// The scanning passes this shot was built from, first one first; absent (empty) in a shot of
    /// one pass written before passes existed. See [`crate::passes`].
    #[serde(default)]
    pub passes: Vec<crate::passes::Pass>,
    /// Flags of the shot's first pass (`gimbal`, `tracking lost`, `gimbal lost`, `low battery`);
    /// absent (empty) in older files.
    #[serde(default)]
    pub flags: Vec<String>,
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
    normal_count: u32,
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

/// A keyframe whose JPEG is borrowed, so a caller holding the bytes elsewhere (the phone's joined
/// buffer) can write a bundle without copying every photo first.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyframeRef<'a> {
    /// Camera-to-capture transform, column-major 4×4.
    pub pose: [f32; 16],
    /// Camera intrinsics, column-major 3×3.
    pub intrinsics: [f32; 9],
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// The JPEG bytes.
    pub jpeg: &'a [u8],
}

/// Writes a shot as bundle bytes. `BadMesh` for anything [`decode`] would refuse: ragged arrays
/// (positions or indices not a multiple of three, normals not one per vertex), a number that is
/// not finite anywhere in the file (header, poses, cards, passes), or an index past the vertices.
pub fn encode(shot: &Shot) -> Result<Vec<u8>, BundleError> {
    let refs: Vec<KeyframeRef> = shot
        .keyframes
        .iter()
        .map(|k| KeyframeRef {
            pose: k.pose,
            intrinsics: k.intrinsics,
            width: k.width,
            height: k.height,
            jpeg: &k.jpeg,
        })
        .collect();
    encode_parts(
        &shot.header,
        &shot.positions,
        &shot.normals,
        &shot.indices,
        &refs,
    )
}

fn card_is_finite(c: &CardSeen) -> bool {
    c.point.iter().all(|v| v.is_finite())
        && c.corners.iter().flatten().all(|v| v.is_finite())
        && c.spread_m.is_finite()
}

/// True when every number in the header is finite (JSON writes NaN as null, which no reader accepts).
fn header_is_finite(h: &ShotHeader) -> bool {
    h.instrument_height_m.is_finite()
        && h.start_camera
            .iter()
            .chain(&h.gravity)
            .all(|v| v.is_finite())
        && h.compass.is_none_or(|c| c.accuracy_deg.is_finite())
        && h.taps
            .iter()
            .all(|t| t.origin.iter().chain(&t.dir).all(|v| v.is_finite()))
        && h.cards.iter().all(card_is_finite)
        && h.passes.iter().all(|p| {
            p.transform.iter().all(|v| v.is_finite()) && p.cards.iter().all(card_is_finite)
        })
}

/// [`encode`] from borrowed parts. The output is sized once (photos included) and never grows.
pub fn encode_parts(
    header: &ShotHeader,
    positions: &[f32],
    normals: &[f32],
    indices: &[u32],
    keyframes: &[KeyframeRef],
) -> Result<Vec<u8>, BundleError> {
    let n = positions.len() / 3;
    if !positions.len().is_multiple_of(3)
        || normals.len() != positions.len()
        || !indices.len().is_multiple_of(3)
        || !positions.iter().chain(normals).all(|v| v.is_finite())
        || !indices.iter().all(|&i| (i as usize) < n)
        || u32::try_from(n).is_err()
        || u32::try_from(indices.len() / 3).is_err()
        || !header_is_finite(header)
        || !keyframes
            .iter()
            .all(|k| k.pose.iter().chain(&k.intrinsics).all(|v| v.is_finite()))
    {
        return Err(BundleError::BadMesh);
    }
    let mut wire_keyframes = Vec::with_capacity(keyframes.len());
    let mut photo_bytes = 0usize;
    for k in keyframes {
        let jpeg_len = u32::try_from(k.jpeg.len()).map_err(|_| BundleError::BadHeader)?;
        photo_bytes = photo_bytes
            .checked_add(k.jpeg.len())
            .ok_or(BundleError::BadHeader)?;
        wire_keyframes.push(WireKeyframe {
            pose: k.pose.to_vec(),
            intrinsics: k.intrinsics.to_vec(),
            width: k.width,
            height: k.height,
            jpeg_len,
        });
    }
    let wire = Wire {
        header: header.clone(),
        vertex_count: n as u32,
        normal_count: n as u32,
        triangle_count: (indices.len() / 3) as u32,
        keyframes: wire_keyframes,
    };
    let json = serde_json::to_vec(&wire).map_err(|_| BundleError::BadHeader)?;
    let json_len = u32::try_from(json.len()).map_err(|_| BundleError::BadHeader)?;
    let size = MAGIC.len()
        + 2
        + 4
        + json.len()
        + (positions.len() + normals.len() + indices.len()) * 4
        + photo_bytes;
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT.to_le_bytes());
    out.extend_from_slice(&json_len.to_le_bytes());
    out.extend_from_slice(&json);
    for v in positions.iter().chain(normals) {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for i in indices {
        out.extend_from_slice(&i.to_le_bytes());
    }
    for k in keyframes {
        out.extend_from_slice(k.jpeg);
    }
    debug_assert_eq!(out.len(), size);
    Ok(out)
}

/// What [`check_header`] finds: the header and the counts, with no array read.
#[derive(Clone, Debug, PartialEq)]
pub struct HeaderInfo {
    /// The shot's header.
    pub header: ShotHeader,
    /// Mesh vertices.
    pub vertices: usize,
    /// Mesh triangles.
    pub triangles: usize,
    /// Kept photos.
    pub keyframes: usize,
}

/// Reads only a bundle's header and checks the file is exactly as long as the header says. Cheap
/// (no mesh or photo is copied); it does not check the mesh numbers, which [`decode`] does.
pub fn check_header(bytes: &[u8]) -> Result<HeaderInfo, BundleError> {
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
    if wire.normal_count != wire.vertex_count {
        return Err(BundleError::BadMesh);
    }
    if wire
        .keyframes
        .iter()
        .any(|k| k.pose.len() != 16 || k.intrinsics.len() != 9)
    {
        return Err(BundleError::BadHeader);
    }
    let photos: u64 = wire.keyframes.iter().map(|k| u64::from(k.jpeg_len)).sum();
    let need =
        u64::from(wire.vertex_count) * 3 * 4 * 2 + u64::from(wire.triangle_count) * 3 * 4 + photos;
    if need != (bytes.len() - r.at) as u64 {
        return Err(BundleError::Truncated);
    }
    Ok(HeaderInfo {
        vertices: wire.vertex_count as usize,
        triangles: wire.triangle_count as usize,
        keyframes: wire.keyframes.len(),
        header: wire.header,
    })
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
    if wire.normal_count != wire.vertex_count {
        return Err(BundleError::BadMesh);
    }
    // Counts come from the file: size them in u64 and check against what is left before reading,
    // so a lying header cannot overflow a 32-bit usize or ask for more than the file holds.
    let need = (u64::from(wire.vertex_count) * 3 * 4) * 2 + u64::from(wire.triangle_count) * 3 * 4;
    if need > (bytes.len() - r.at) as u64 {
        return Err(BundleError::Truncated);
    }
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
