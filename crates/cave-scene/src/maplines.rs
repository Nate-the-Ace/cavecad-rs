//! Linework drawn in plan on the map: anchored to the survey's legs so it follows them.
//!
//! A stroke drawn in the Top view is stored relative to the nearest legs (how far along, how far
//! off), like a profile stroke, so adjusting the survey moves it with the passage. Seen again it is
//! lifted to the elevation of the leg it hangs on, so it lies along the passage in 3D.

use crate::profile::Sketch3d;
use crate::solved::solve;
use cave_doc::{Document, Field, Frame, Kind, Value};
use cave_survey::LegKind;
use std::collections::HashMap;

/// The survey's ordinary legs with their plan positions: what a new plan stroke anchors to.
pub fn map_frames(doc: &Document, use_adjustment: bool) -> Vec<Frame> {
    let s = solve(doc, use_adjustment);
    s.resolved
        .legs
        .iter()
        .filter(|l| l.kind == LegKind::New)
        .filter_map(|l| {
            let (a, b) = (s.positions.get(&l.from)?, s.positions.get(&l.to)?);
            Some(Frame {
                a: l.from,
                b: l.to,
                pa: [a.x, a.y],
                pb: [b.x, b.y],
            })
        })
        .collect()
}

/// Every stroke drawn in plan, in the map's metres (east, north, up). A stroke whose stations are
/// gone, or that hangs on a leg that has collapsed to a point, is left out.
pub fn map_lines_3d(doc: &Document, use_adjustment: bool) -> Vec<Sketch3d> {
    let s = solve(doc, use_adjustment);
    let plan: HashMap<_, _> = s
        .positions
        .iter()
        .map(|(id, p)| (*id, [p.x, p.y]))
        .collect();
    let mut out = Vec::new();
    for id in doc.ids_of(Kind::Feature) {
        let Some(e) = doc.entity(id) else { continue };
        if e.text(Field::FeatureView) != cave_draw::VIEW_MAP {
            continue;
        }
        let Value::Anchored(Some(anchored)) = e.get(Field::FeatureAnchored) else {
            continue;
        };
        let Some(placed) = anchored.place_with_frames(&|st| plan.get(&st).copied()) else {
            continue;
        };
        let points: Vec<[f64; 3]> = placed
            .iter()
            .zip(&anchored.points)
            .filter_map(|((frame, p), ap)| {
                let (a, b) = anchored.frames[*frame];
                let (za, zb) = (s.positions.get(&a)?.z, s.positions.get(&b)?.z);
                Some([p[0], p[1], za + ap.u.clamp(0.0, 1.0) * (zb - za)])
            })
            .collect();
        if points.len() == placed.len() {
            out.push(Sketch3d {
                id: id.to_string(),
                kind: e.text(Field::FeatureType),
                run: String::new(),
                points,
                widths: anchored.widths.clone(),
                closed: anchored.closed,
                text: e.text(Field::FeatureText),
                style: e.text(Field::FeatureStyle),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use cave_doc::ActorId;
    use cave_id::SeqIds;

    const CAVE: &str = r#"{"unit":"m","name":"p","shots":[
        {"from":"A1","to":"A2","distance":10,"azimuth":0,"inclination":0},
        {"from":"A2","to":"A3","distance":10,"azimuth":90,"inclination":-30}]}"#;

    #[test]
    fn a_plan_stroke_follows_the_survey_and_lies_at_the_leg_elevation() {
        let mut doc = cave_fixture::from_shot_list(CAVE, ActorId(1)).unwrap();
        let frames = map_frames(&doc, false);
        assert_eq!(frames.len(), 2);
        // A wall drawn 1 m east of A1-A2 (halfway) and then along the sloping A2-A3.
        let run = 10.0 * 30f64.to_radians().cos();
        let (_, actions) = cave_draw::map_line(
            &doc,
            &mut SeqIds::new(1),
            "wall",
            &[[1.0, 5.0], [run / 2.0, 9.0]],
            Some(vec![1.0, 1.0]),
            &frames,
        )
        .unwrap();
        doc.edit(actions).unwrap();
        let lines = map_lines_3d(&doc, false);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].kind, "wall");
        let near = |p: [f64; 3], x: f64, y: f64, z: f64| {
            (p[0] - x).abs() < 1e-6 && (p[1] - y).abs() < 1e-6 && (p[2] - z).abs() < 1e-6
        };
        assert!(
            near(lines[0].points[0], 1.0, 5.0, 0.0),
            "{:?}",
            lines[0].points
        );
        // Over the sloping leg the stroke rides its elevation (halfway down: -2.5 m).
        assert!(
            (lines[0].points[1][2] + 2.5).abs() < 0.8,
            "{:?}",
            lines[0].points
        );
    }

    #[test]
    fn adjusting_the_survey_moves_the_stroke_with_it() {
        let text = r#"{"unit":"m","name":"loop","shots":[
            {"from":"A1","to":"A2","distance":10,"azimuth":0,"inclination":0},
            {"from":"A2","to":"A3","distance":10,"azimuth":90,"inclination":0},
            {"from":"A3","to":"A4","distance":10,"azimuth":180,"inclination":0},
            {"from":"A4","to":"A1","distance":10.6,"azimuth":270,"inclination":0}]}"#;
        let mut doc = cave_fixture::from_shot_list(text, ActorId(1)).unwrap();
        let frames = map_frames(&doc, false);
        let (_, actions) = cave_draw::map_line(
            &doc,
            &mut SeqIds::new(2),
            "ledge",
            &[[9.0, 20.0], [10.5, 5.0], [10.5, 2.0]],
            None,
            &frames,
        )
        .unwrap();
        doc.edit(actions).unwrap();
        let raw = map_lines_3d(&doc, false);
        let adj = map_lines_3d(&doc, true);
        assert_eq!((raw.len(), adj.len()), (1, 1));
        assert_ne!(
            raw[0].points, adj[0].points,
            "the stroke went with the adjusted stations"
        );
    }

    #[test]
    fn profile_and_section_strokes_are_not_map_lines() {
        let doc = cave_fixture::from_shot_list(CAVE, ActorId(1)).unwrap();
        assert!(map_lines_3d(&doc, false).is_empty());
    }
}
