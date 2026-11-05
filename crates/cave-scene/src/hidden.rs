//! Drawings that cannot be shown because the survey they hang on is gone: a map or profile stroke
//! whose stations are no longer placed (a shot deleted, so a station went with it), or a cross-section
//! sketch whose station is gone. They stay in the cave (re-entering the shot brings them back), but
//! nothing draws them, so the app says how many there are and offers to delete them.

use crate::solved::solve;
use cave_doc::{Document, Field, Kind, Value};
use cave_id::Id;

/// The drawings nothing can show now, because a station they hang on is gone.
pub fn hidden_drawings(doc: &Document, use_adjustment: bool) -> Vec<Id> {
    let s = solve(doc, use_adjustment);
    let placed = |st: Id| s.positions.contains_key(&st);
    doc.ids_of(Kind::Feature)
        .into_iter()
        .filter(|id| {
            let Some(e) = doc.entity(*id) else {
                return false;
            };
            let view = e.text(Field::FeatureView);
            if view == cave_draw::VIEW_SECTION {
                return e
                    .reference(Field::FeatureStation)
                    .is_some_and(|st| !placed(st));
            }
            if view != cave_draw::VIEW_MAP && view != cave_draw::VIEW_PROFILE {
                return false;
            }
            match e.get(Field::FeatureAnchored) {
                Value::Anchored(Some(path)) => {
                    path.frames.iter().any(|(a, b)| !placed(*a) || !placed(*b))
                }
                _ => false,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_frames;
    use cave_doc::ActorId;
    use cave_id::SeqIds;

    const CAVE: &str = r#"{"unit":"m","name":"p","shots":[
        {"from":"A1","to":"A2","distance":10,"azimuth":0,"inclination":0},
        {"from":"A2","to":"A3","distance":10,"azimuth":90,"inclination":0},
        {"from":"A2","to":"B1","distance":8,"azimuth":270,"inclination":0}]}"#;

    #[test]
    fn a_drawing_on_a_deleted_branch_is_hidden_and_comes_back_with_it() {
        let mut doc = cave_fixture::from_shot_list(CAVE, ActorId(1)).unwrap();
        let frames = map_frames(&doc, false);
        // a wall beside the side branch A2-B1, and one beside the main line A1-A2
        let (_, branch) = cave_draw::map_line(
            &doc,
            &mut SeqIds::new(1),
            "wall",
            &[[-2.0, 11.0], [-6.0, 11.0]],
            None,
            &frames,
        )
        .unwrap();
        doc.edit(branch).unwrap();
        let (_, main) = cave_draw::map_line(
            &doc,
            &mut SeqIds::new(100),
            "wall",
            &[[1.0, 2.0], [1.0, 8.0]],
            None,
            &frames,
        )
        .unwrap();
        doc.edit(main).unwrap();
        assert!(hidden_drawings(&doc, false).is_empty());

        let shots = doc.ids_of(Kind::Shot);
        let to_b1 = shots
            .into_iter()
            .find(|s| {
                let to = doc.entity(*s).and_then(|e| e.reference(Field::ShotTo));
                to.and_then(|t| doc.entity(t))
                    .is_some_and(|st| st.text(Field::StationLabel) == "B1")
            })
            .unwrap();
        let removed = cave_notebook::remove_shot(&doc, to_b1).unwrap();
        let edit = doc.edit(removed).unwrap();
        let hidden = hidden_drawings(&doc, false);
        assert_eq!(hidden.len(), 1, "only the wall on the deleted branch");
        assert_eq!(
            crate::map_lines_3d(&doc, false).len(),
            1,
            "and it is the one not drawn"
        );

        doc.edit(edit.inverse.clone()).unwrap();
        assert!(
            hidden_drawings(&doc, false).is_empty(),
            "the shot back, the wall shows again"
        );
    }
}
