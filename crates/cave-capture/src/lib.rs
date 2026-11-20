//! LiDAR room capture: the shot file the Capture app writes, where a shot sits in the cave, and the
//! stations a shot can be placed over. Pure: bytes and documents in, data out (spec:
//! `docs/superpowers/specs/2026-11-17-lidar-room-capture-design.md`).

pub mod bundle;
pub mod frame;
pub mod stations;

pub use bundle::{
    BundleError, CardSeen, Compass, HeaderInfo, HeadingTap, Keyframe, KeyframeRef, Shot,
    ShotHeader, check_header, decode, encode, encode_parts,
};
pub use frame::{
    PIVOT_HEIGHT_M, Placement, bearing_deg, capture_to_survey, from_compass, heading_from_tap,
};
pub use stations::{StationSeed, document_from_room, stations};
