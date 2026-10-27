//! Where a cave is on the Earth: one world frame (WGS84 latitude/longitude, NAVD88 heights) for
//! every cave, the surface downloaded over it (USGS NAIP photo, USGS 3DEP elevation, both requested
//! in the entrance's UTM zone), contours from that elevation, and the entrance pick. Pure: no I/O.

pub mod contour;
pub mod datum;
pub mod frame;
pub mod grid;
pub mod request;
