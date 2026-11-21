//! LiDAR room capture: the shot file the Capture app writes, where a shot sits in the cave, and the
//! stations a shot can be placed over. Pure: bytes and documents in, data out (spec:
//! `docs/superpowers/specs/2026-11-17-lidar-room-capture-design.md`).

pub mod bundle;
pub mod chain;
pub mod frame;
pub mod passes;
pub mod place;
pub mod shared;
pub mod stations;
pub mod thin;

pub use bundle::{
    BundleError, CardSeen, Compass, HeaderInfo, HeadingTap, Keyframe, KeyframeRef, Shot,
    ShotHeader, check_header, decode, encode, encode_parts, mesh_positions,
};
pub use chain::{
    Chain, Chains, Dir, FitShot, Flag, PairFit, Rigid, ShotPlace, Source, chains, display_fix,
    leg_neighbours, neighbours, pair_fit,
};
pub use frame::{
    PIVOT_HEIGHT_M, Placement, bearing_deg, capture_to_survey, from_compass, heading_from_tap,
};
pub use passes::{
    CardFit, Ghost, IDENTITY, NewPass, Pass, PassMesh, add_pass, apply_dir, apply_point,
    card_offsets, cards_fit, ghost, mul,
};
pub use place::{StationRef, placement, relative_points, station_for};
pub use shared::{SharedStation, SharedStations, shared_stations};
pub use stations::{StationSeed, document_from_room, label_key, labels_match, stations};
pub use thin::thin_points;
