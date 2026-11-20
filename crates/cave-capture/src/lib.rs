//! LiDAR room capture: the shot file the Capture app writes, where a shot sits in the cave, and the
//! stations a shot can be placed over. Pure: bytes and documents in, data out (spec:
//! `docs/superpowers/specs/2026-11-17-lidar-room-capture-design.md`).

pub mod bundle;
pub mod frame;
pub mod stations;

pub use bundle::{BundleError, HeadingTap, Keyframe, Shot, ShotHeader, decode, encode};
// pub use frame::{Placement, bearing_deg, capture_to_survey, heading_from_tap};
// pub use stations::{StationSeed, document_from_room, stations};
