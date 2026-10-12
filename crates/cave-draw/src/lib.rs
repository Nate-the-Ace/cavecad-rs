//! Authoring operations: what a drawing tool does, as document actions.
//!
//! Every function returns `Action`s for `Document::edit`; nothing here writes to a document
//! directly. That is the rule that keeps undo, history and (later) co-editing working: a tool
//! never edits around the one edit path. Strokes are anchored to stations, so they follow the
//! survey when it changes. Pen pressure is kept as per-point widths, which change how the line is
//! drawn and never where it is.

use cave_doc::{Action, AnchorError, Document, Field, Frame, Kind, Value, anchor_stroke};
use cave_id::{Id, IdSource};

/// `Feature.type` of a stroke sketched on a profile band.
pub const PROFILE_SKETCH: &str = "profile-sketch";

/// Why a stroke could not be turned into actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawError {
    /// A stroke needs at least two distinct points.
    TooShort,
    /// A coordinate or pen weight was not a usable number.
    BadStroke,
    /// The run has no usable stretch of survey to anchor the stroke to.
    NoStations,
}

impl From<AnchorError> for DrawError {
    fn from(e: AnchorError) -> Self {
        match e {
            AnchorError::TooShort => DrawError::TooShort,
            AnchorError::BadStroke => DrawError::BadStroke,
            AnchorError::NoFrames => DrawError::NoStations,
        }
    }
}

/// Actions that add a stroke sketched on the profile band of `run`.
///
/// `points` are `(s, z)` in metres on the band (distance along the run, elevation); `frames` are
/// the band's consecutive station pairs with their band positions, and the stroke is anchored to
/// the nearest of them. `widths` is optional pen weight per point (1.0 is the normal weight).
/// Returns the new feature's id with the actions. A sketch is a feature of type `profile-sketch`;
/// grouping and visibility come from type and trip, not from layers.
pub fn profile_sketch(
    _doc: &Document,
    ids: &mut impl IdSource,
    run: &str,
    points: &[[f64; 2]],
    widths: Option<Vec<f64>>,
    frames: &[Frame],
) -> Result<(Id, Vec<Action>), DrawError> {
    // Drop repeated points: a stroke that never moved is not a line.
    let mut kept: Vec<usize> = Vec::with_capacity(points.len());
    for (i, p) in points.iter().enumerate() {
        if kept.last().is_none_or(|&k| points[k] != *p) {
            kept.push(i);
        }
    }
    if widths.as_ref().is_some_and(|w| w.len() != points.len()) {
        return Err(DrawError::BadStroke);
    }
    let pts: Vec<[f64; 2]> = kept.iter().map(|&i| points[i]).collect();
    let widths = widths.map(|w| kept.iter().map(|&i| w[i]).collect());
    let anchored = anchor_stroke(&pts, frames, widths)?;

    let mut actions = Vec::new();
    let id = ids.next_id();
    actions.extend([
        Action::Create {
            id,
            kind: Kind::Feature,
        },
        Action::Set {
            id,
            field: Field::FeatureType,
            value: Value::Text(PROFILE_SKETCH.into()),
        },
        Action::Set {
            id,
            field: Field::FeatureRun,
            value: Value::Text(run.into()),
        },
        Action::Set {
            id,
            field: Field::FeatureAnchored,
            value: Value::Anchored(Some(anchored)),
        },
    ]);
    Ok((id, actions))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cave_doc::{ActorId, History, Issue, validate};
    use cave_id::SeqIds;

    /// A document with two stations and the frame between them.
    fn cave() -> (Document, Vec<Frame>, [Id; 2]) {
        let mut doc = Document::new(ActorId(1));
        let (a, b) = (Id::from_name("st:a"), Id::from_name("st:b"));
        let mut actions = Vec::new();
        for (id, label) in [(a, "A1"), (b, "A2")] {
            actions.push(Action::Create {
                id,
                kind: Kind::Station,
            });
            actions.push(Action::Set {
                id,
                field: Field::StationLabel,
                value: Value::Text(label.into()),
            });
        }
        doc.edit(actions).unwrap();
        let frames = vec![Frame {
            a,
            b,
            pa: [0.0, 0.0],
            pb: [10.0, 0.0],
        }];
        (doc, frames, [a, b])
    }

    fn stroke() -> Vec<[f64; 2]> {
        vec![[0.0, 0.0], [1.0, 0.5], [2.0, 0.0]]
    }

    #[test]
    fn a_sketch_is_a_valid_anchored_feature_() {
        let (mut doc, frames, [a, b]) = cave();
        let (id, actions) =
            profile_sketch(&doc, &mut SeqIds::new(1), "A", &stroke(), None, &frames).unwrap();
        doc.edit(actions).unwrap();
        let e = doc.entity(id).unwrap();
        assert_eq!(
            (e.text(Field::FeatureRun), e.text(Field::FeatureType)),
            ("A".into(), PROFILE_SKETCH.to_string())
        );
        let Value::Anchored(Some(anchored)) = e.get(Field::FeatureAnchored) else {
            panic!("not anchored")
        };
        assert_eq!(anchored.frames, vec![(a, b)]);
        assert!(
            validate(&doc).is_empty(),
            "nothing dangling, nothing missing"
        );
    }

    #[test]
    fn deleting_a_station_the_sketch_hangs_on_is_flagged() {
        let (mut doc, frames, [a, _]) = cave();
        let (id, actions) =
            profile_sketch(&doc, &mut SeqIds::new(1), "A", &stroke(), None, &frames).unwrap();
        doc.edit(actions).unwrap();
        doc.edit(vec![Action::Remove {
            id: a,
            kind: Kind::Station,
        }])
        .unwrap();
        assert!(validate(&doc).contains(&Issue::DanglingReference {
            entity: id,
            field: Field::FeatureAnchored,
            target: a
        }));
    }

    #[test]
    fn two_people_sketching_independently_merge_cleanly() {
        let (base, frames, _) = cave();
        let mut a = base.clone();
        let mut b = Document::from_json(&base.to_json(), ActorId(2)).unwrap();
        let (_, ea) =
            profile_sketch(&a, &mut SeqIds::new(1), "A", &stroke(), None, &frames).unwrap();
        let (_, eb) =
            profile_sketch(&b, &mut SeqIds::new(2), "A", &stroke(), None, &frames).unwrap();
        let (ops_a, ops_b) = (a.edit(ea).unwrap().ops, b.edit(eb).unwrap().ops);
        for op in &ops_b {
            a.apply(op).unwrap();
        }
        for op in &ops_a {
            b.apply(op).unwrap();
        }
        assert_eq!(a, b);
        assert_eq!(a.ids_of(Kind::Feature).len(), 2);
    }

    #[test]
    fn pen_pressure_is_kept_as_widths_and_repeated_points_are_dropped_with_theirs() {
        let (doc, frames, _) = cave();
        let pts = vec![[0.0, 0.0], [0.0, 0.0], [1.0, 1.0]];
        let (_, actions) = profile_sketch(
            &doc,
            &mut SeqIds::new(1),
            "A",
            &pts,
            Some(vec![1.0, 1.1, 1.2]),
            &frames,
        )
        .unwrap();
        let anchored = actions.iter().find_map(|a| match a {
            Action::Set {
                value: Value::Anchored(Some(p)),
                ..
            } => Some(p.clone()),
            _ => None,
        });
        let anchored = anchored.unwrap();
        assert_eq!(anchored.points.len(), 2);
        assert_eq!(anchored.widths, Some(vec![1.0, 1.2]));
    }

    #[test]
    fn bad_strokes_are_refused() {
        let (doc, frames, _) = cave();
        let mut ids = SeqIds::new(1);
        let run = |pts: &[[f64; 2]], w: Option<Vec<f64>>, f: &[Frame], ids: &mut SeqIds| {
            profile_sketch(&doc, ids, "A", pts, w, f).unwrap_err()
        };
        assert_eq!(
            run(&[[0.0, 0.0]], None, &frames, &mut ids),
            DrawError::TooShort
        );
        assert_eq!(
            run(&[[1.0, 1.0], [1.0, 1.0]], None, &frames, &mut ids),
            DrawError::TooShort
        );
        assert_eq!(
            run(&[[0.0, 0.0], [f64::NAN, 1.0]], None, &frames, &mut ids),
            DrawError::BadStroke
        );
        assert_eq!(
            run(&stroke(), Some(vec![1.0]), &frames, &mut ids),
            DrawError::BadStroke
        );
        assert_eq!(run(&stroke(), None, &[], &mut ids), DrawError::NoStations);
    }

    #[test]
    fn a_sketch_can_be_undone_and_redone() {
        let (mut doc, frames, _) = cave();
        let mut history = History::new();
        let (id, actions) =
            profile_sketch(&doc, &mut SeqIds::new(1), "A", &stroke(), None, &frames).unwrap();
        let edit = doc.edit(actions).unwrap();
        history.record(&edit);
        history.undo(&mut doc).unwrap();
        assert!(doc.entity(id).is_none());
        history.redo(&mut doc).unwrap();
        assert!(doc.entity(id).is_some());
    }
}
