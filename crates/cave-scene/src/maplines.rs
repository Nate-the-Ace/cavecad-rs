//! Linework drawn in plan on the map: anchored to the survey's legs so it follows them.
//!
//! A stroke drawn in the Top view is stored relative to the nearest legs (how far along, how far
//! off), like a profile stroke, so adjusting the survey moves it with the passage. Seen again it is
//! lifted to the elevation of the leg it hangs on, so it lies along the passage in 3D.

use crate::profile::Sketch3d;
use crate::solved::solve;
use cave_doc::{
    AnchoredPath, Document, Field, Frame, Kind, Value, frame_index, other_level, path_hops_level,
};
use cave_id::Id;
use cave_survey::LegKind;
use serde::Serialize;
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
                za: a.z,
                zb: b.z,
            })
        })
        .collect()
}

/// A map line's anchored path, if `line` is a stroke drawn in plan.
fn plan_path(doc: &Document, line: Id) -> Option<AnchoredPath> {
    let e = doc.entity(line)?;
    if e.text(Field::FeatureView) != cave_draw::VIEW_MAP {
        return None;
    }
    match e.get(Field::FeatureAnchored) {
        Value::Anchored(Some(a)) => Some(a),
        _ => None,
    }
}

/// A line's plan points (east, north) and its pen weights, if it has any.
pub type PlacedLine = (Vec<[f64; 2]>, Option<Vec<f64>>);

/// A map line's plan points and pen weights where the survey puts it now.
pub fn placed_line(doc: &Document, line: Id, use_adjustment: bool) -> Option<PlacedLine> {
    let path = plan_path(doc, line)?;
    let s = solve(doc, use_adjustment);
    let pts = path.place(&|id| s.positions.get(&id).map(|p| [p.x, p.y]))?;
    Some((pts, path.widths))
}

/// Which passage a map line hangs on, for the map to glow, and the other level it could be on.
#[derive(Debug, Serialize)]
pub struct LineLevel {
    /// Each leg the line hangs on: its two ends, `[east, north, up]` metres.
    pub legs: Vec<[[f64; 3]; 2]>,
    /// The other level's leg `[a, b]` (station ids) when the line starts where levels overlap.
    pub other: Option<[String; 2]>,
}

/// [`LineLevel`] for `line`, or `None` when it is not a placed map line.
pub fn line_level(doc: &Document, line: Id, use_adjustment: bool) -> Option<LineLevel> {
    let path = plan_path(doc, line)?;
    let s = solve(doc, use_adjustment);
    let at = |id: Id| s.positions.get(&id).map(|p| [p.x, p.y, p.z]);
    let legs = path
        .frames
        .iter()
        .map(|(a, b)| Some([at(*a)?, at(*b)?]))
        .collect::<Option<Vec<_>>>()?;
    let frames = map_frames(doc, use_adjustment);
    let first = path.frames[path.points[0].frame as usize];
    let start = path.place(&|id| s.positions.get(&id).map(|p| [p.x, p.y]))?[0];
    let other = frame_index(&frames, first)
        .and_then(|i| other_level(start, &frames, i))
        .map(|k| [frames[k].a.to_string(), frames[k].b.to_string()]);
    Some(LineLevel { legs, other })
}

/// Map lines that step between legs of passages on different levels: drawn before strokes
/// stayed on their level, and worth re-attaching.
pub fn hopped_lines(doc: &Document, use_adjustment: bool) -> Vec<Id> {
    let frames = map_frames(doc, use_adjustment);
    doc.ids_of(Kind::Feature)
        .into_iter()
        .filter(|id| plan_path(doc, *id).is_some_and(|p| path_hops_level(&p, &frames)))
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

    /// Upper passage U0-U4 east at z 0; lower L0-L4 north at z -8 crossing it under U2 (20, 0);
    /// joined far away through J, so it is one survey.
    fn two_levels() -> Document {
        let leg = |from: &str, to: &str, dx: f64, dy: f64, dz: f64| {
            let h = dx.hypot(dy);
            format!(
                r#"{{"from":"{from}","to":"{to}","distance":{},"azimuth":{},"inclination":{}}}"#,
                h.hypot(dz),
                dx.atan2(dy).to_degrees().rem_euclid(360.0),
                dz.atan2(h).to_degrees()
            )
        };
        let mut shots = Vec::new();
        for k in 0..4 {
            shots.push(leg(
                &format!("U{k}"),
                &format!("U{}", k + 1),
                10.0,
                0.0,
                0.0,
            ));
        }
        shots.push(leg("U4", "J", 10.0, -30.0, -4.0));
        shots.push(leg("J", "L0", -30.0, 10.0, -4.0));
        for k in 0..4 {
            shots.push(leg(
                &format!("L{k}"),
                &format!("L{}", k + 1),
                0.0,
                10.0,
                0.0,
            ));
        }
        let json = format!(r#"{{"unit":"m","name":"x","shots":[{}]}}"#, shots.join(","));
        cave_fixture::from_shot_list(&json, ActorId(1)).unwrap()
    }

    #[test]
    fn maplines_hopped_line_is_listed_and_a_path_line_is_not() {
        let mut doc = two_levels();
        let frames = map_frames(&doc, false);
        let wall: Vec<[f64; 2]> = (0..13).map(|k| [5.0 + 2.5 * k as f64, 2.0]).collect();
        let (old_id, mut old) =
            cave_draw::map_line(&doc, &mut SeqIds::new(800), "wall", &wall, None, &frames).unwrap();
        let hopped = cave_doc::anchor_stroke(&wall, &frames, None).unwrap();
        old.push(cave_doc::Action::Set {
            id: old_id,
            field: Field::FeatureAnchored,
            value: Value::Anchored(Some(hopped)),
        });
        doc.edit(old).unwrap();
        let (new_id, new) =
            cave_draw::map_line(&doc, &mut SeqIds::new(850), "wall", &wall, None, &frames).unwrap();
        doc.edit(new).unwrap();
        let list = hopped_lines(&doc, false);
        assert!(list.contains(&old_id), "{list:?}");
        assert!(!list.contains(&new_id));
        // the line that stayed up lies at the upper passage's elevation all the way across
        let line = map_lines_3d(&doc, false)
            .into_iter()
            .find(|l| l.id == new_id.to_string())
            .unwrap();
        assert!(
            line.points.iter().all(|p| p[2].abs() < 1e-6),
            "{:?}",
            line.points
        );
    }

    #[test]
    fn maplines_line_started_at_the_crossing_offers_the_other_level() {
        let mut doc = two_levels();
        let frames = map_frames(&doc, false);
        let stroke = [[20.5, 1.0], [15.0, 1.0], [8.0, 1.0]];
        let (id, a) =
            cave_draw::map_line(&doc, &mut SeqIds::new(800), "wall", &stroke, None, &frames)
                .unwrap();
        doc.edit(a).unwrap();
        let level = line_level(&doc, id, false).unwrap();
        assert!(!level.legs.is_empty());
        assert!(level.other.is_some());
        let (pts, _) = placed_line(&doc, id, false).unwrap();
        assert!((pts[0][0] - 20.5).abs() < 1e-6 && (pts[0][1] - 1.0).abs() < 1e-6);
    }

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
