//! The 3D passage surface for the canvas.

use crate::solved::solve;
use cave_doc::Document;
pub use cave_mesh::{ColorMode, Mesh};
use cave_mesh::{MeshInput, build};

/// Builds the passage mesh from the open cave (adjusted positions when asked and available).
pub fn passage_mesh(doc: &Document, use_adjustment: bool, color: ColorMode) -> Mesh {
    let s = solve(doc, use_adjustment);
    build(&MeshInput {
        stations: &s.positions,
        legs: &s.resolved.legs,
        splays: &s.ex.splays,
        first_arrival_lruds: &s.ex.first_arrival_lruds,
        start_lruds: &s.ex.start_lruds,
        station_trip: &s.ex.station_trip,
        color,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cave_doc::ActorId;

    #[test]
    fn a_measured_passage_becomes_triangles() {
        let text = r#"{"unit":"m","name":"m","trips":[{"name":"m","startLrud":{"left":1,"right":1,"up":1,"down":1}}],"shots":[
            {"from":"A1","to":"A2","distance":10,"azimuth":0,"inclination":0,"left":1,"right":1,"up":1,"down":1}]}"#;
        let doc = cave_fixture::from_shot_list(text, ActorId(1)).unwrap();
        let m = passage_mesh(&doc, true, ColorMode::Trip);
        assert_eq!(m.triangles(), 8);
        assert!(passage_mesh(&Document::new(ActorId(1)), true, ColorMode::Depth).triangles() == 0);
    }
}
