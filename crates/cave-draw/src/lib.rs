//! Authoring operations: what a drawing tool does, as document actions.
//!
//! Every function returns `Action`s for `Document::edit`; nothing here writes to a document
//! directly. That is the rule that keeps undo, history and (later) co-editing working: a tool
//! never edits around the one edit path. Geometry captured with a pen keeps its pressure as
//! per-point widths, which change how the line is drawn and never where it is.

use cave_doc::{Action, Document, Field, Kind, Value};
use cave_geom::{Path, PathError, Point};
use cave_id::{Id, IdSource};

/// `Feature.type` of a stroke sketched on a profile band.
pub const PROFILE_SKETCH: &str = "profile-sketch";
/// The built-in layer sketches land on.
pub const SKETCH_LAYER_NAME: &str = "Profile sketch";

/// The one sketch layer. Its id is derived from its name, so every machine agrees on it.
pub fn sketch_layer_id() -> Id {
    Id::from_name("layer:profile-sketch")
}

/// Why a stroke could not be turned into actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawError {
    /// A stroke needs at least two distinct points.
    TooShort,
    /// A coordinate or pen weight was not a usable number.
    BadStroke,
}

impl From<PathError> for DrawError {
    fn from(e: PathError) -> Self {
        match e {
            PathError::TooShort => DrawError::TooShort,
            PathError::NotFinite | PathError::BadWidths => DrawError::BadStroke,
        }
    }
}

/// Actions that add a stroke sketched on the profile band of `run`. Points are `(s, z)` in metres
/// on the band (distance along the run, elevation). `widths` is optional pen weight per point
/// (1.0 is the normal line weight). Creates the sketch layer first if the cave has none yet.
/// Returns the new feature's id with the actions.
pub fn profile_sketch(
    doc: &Document,
    ids: &mut impl IdSource,
    run: &str,
    points: &[[f64; 2]],
    widths: Option<Vec<f64>>,
) -> Result<(Id, Vec<Action>), DrawError> {
    // Drop repeated points: a stroke that never moved is not a line.
    let mut pts: Vec<Point> = Vec::with_capacity(points.len());
    let mut kept: Vec<usize> = Vec::new();
    for (i, p) in points.iter().enumerate() {
        let pt = Point::new(p[0], p[1]);
        if pts.last() != Some(&pt) {
            pts.push(pt);
            kept.push(i);
        }
    }
    let mut path = Path::polyline(&pts, false);
    if let Some(w) = widths {
        if w.len() != points.len() {
            return Err(DrawError::BadStroke);
        }
        path = path.with_widths(kept.iter().map(|&i| w[i]).collect())?;
    }
    path.validate()?;

    let layer = sketch_layer_id();
    let mut actions = Vec::new();
    if doc.entity(layer).is_none() {
        actions.extend([
            Action::Create {
                id: layer,
                kind: Kind::Layer,
            },
            Action::Set {
                id: layer,
                field: Field::LayerName,
                value: Value::Text(SKETCH_LAYER_NAME.into()),
            },
        ]);
    }
    let id = ids.next_id();
    actions.extend([
        Action::Create {
            id,
            kind: Kind::Feature,
        },
        Action::Set {
            id,
            field: Field::FeatureLayer,
            value: Value::Ref(Some(layer)),
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
            field: Field::FeaturePath,
            value: Value::Path(Some(path)),
        },
    ]);
    Ok((id, actions))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cave_doc::{ActorId, History, validate};
    use cave_id::SeqIds;

    fn stroke() -> Vec<[f64; 2]> {
        vec![[0.0, 0.0], [1.0, 0.5], [2.0, 0.0]]
    }

    #[test]
    fn a_sketch_is_a_valid_feature_on_the_shared_layer() {
        let mut doc = Document::new(ActorId(1));
        let mut ids = SeqIds::new(1);
        let (id, actions) = profile_sketch(&doc, &mut ids, "A", &stroke(), None).unwrap();
        doc.edit(actions).unwrap();
        let e = doc.entity(id).unwrap();
        assert_eq!(e.text(Field::FeatureRun), "A");
        assert_eq!(e.text(Field::FeatureType), PROFILE_SKETCH);
        assert_eq!(e.path(Field::FeaturePath).unwrap().points().len(), 3);
        assert!(doc.entity(sketch_layer_id()).is_some());
        assert!(
            validate(&doc).is_empty(),
            "no dangling layer, nothing missing"
        );
    }

    #[test]
    fn the_layer_is_created_once_and_a_second_stroke_reuses_it() {
        let mut doc = Document::new(ActorId(1));
        let mut ids = SeqIds::new(1);
        let (_, a) = profile_sketch(&doc, &mut ids, "A", &stroke(), None).unwrap();
        doc.edit(a).unwrap();
        let (_, b) = profile_sketch(&doc, &mut ids, "A", &stroke(), None).unwrap();
        assert!(!b.iter().any(|x| matches!(
            x,
            Action::Create {
                kind: Kind::Layer,
                ..
            }
        )));
        doc.edit(b).unwrap();
        assert_eq!(doc.ids_of(Kind::Layer).len(), 1);
        assert_eq!(doc.ids_of(Kind::Feature).len(), 2);
    }

    #[test]
    fn two_people_creating_the_layer_independently_agree() {
        let (mut a, mut b) = (Document::new(ActorId(1)), Document::new(ActorId(2)));
        let (_, ea) = profile_sketch(&a, &mut SeqIds::new(1), "A", &stroke(), None).unwrap();
        let (_, eb) = profile_sketch(&b, &mut SeqIds::new(2), "A", &stroke(), None).unwrap();
        let (ops_a, ops_b) = (a.edit(ea).unwrap().ops, b.edit(eb).unwrap().ops);
        for op in &ops_b {
            a.apply(op).unwrap();
        }
        for op in &ops_a {
            b.apply(op).unwrap();
        }
        assert_eq!(a, b);
        assert_eq!(a.ids_of(Kind::Layer).len(), 1, "one shared layer, not two");
        assert_eq!(a.ids_of(Kind::Feature).len(), 2);
    }

    #[test]
    fn pen_pressure_is_kept_as_widths_and_never_moves_points() {
        let doc = Document::new(ActorId(1));
        let (_, actions) = profile_sketch(
            &doc,
            &mut SeqIds::new(1),
            "A",
            &stroke(),
            Some(vec![0.4, 1.6, 0.8]),
        )
        .unwrap();
        let path = actions.iter().find_map(|a| match a {
            Action::Set {
                value: Value::Path(Some(p)),
                ..
            } => Some(p.clone()),
            _ => None,
        });
        let path = path.unwrap();
        assert_eq!(path.widths(), Some(&[0.4, 1.6, 0.8][..]));
        assert_eq!(
            path.points(),
            stroke()
                .iter()
                .map(|p| Point::new(p[0], p[1]))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn repeated_points_are_dropped_with_their_widths() {
        let doc = Document::new(ActorId(1));
        let pts = vec![[0.0, 0.0], [0.0, 0.0], [1.0, 1.0]];
        let (_, actions) = profile_sketch(
            &doc,
            &mut SeqIds::new(1),
            "A",
            &pts,
            Some(vec![1.0, 1.1, 1.2]),
        )
        .unwrap();
        let p = actions.iter().find_map(|a| match a {
            Action::Set {
                value: Value::Path(Some(p)),
                ..
            } => Some(p.clone()),
            _ => None,
        });
        let p = p.unwrap();
        assert_eq!(p.points().len(), 2);
        assert_eq!(p.widths().unwrap().len(), 2);
    }

    #[test]
    fn bad_strokes_are_refused() {
        let doc = Document::new(ActorId(1));
        let mut ids = SeqIds::new(1);
        assert_eq!(
            profile_sketch(&doc, &mut ids, "A", &[[0.0, 0.0]], None).unwrap_err(),
            DrawError::TooShort
        );
        assert_eq!(
            profile_sketch(&doc, &mut ids, "A", &[[1.0, 1.0], [1.0, 1.0]], None).unwrap_err(),
            DrawError::TooShort
        );
        assert_eq!(
            profile_sketch(&doc, &mut ids, "A", &[[0.0, 0.0], [f64::NAN, 1.0]], None).unwrap_err(),
            DrawError::BadStroke
        );
        assert_eq!(
            profile_sketch(&doc, &mut ids, "A", &stroke(), Some(vec![1.0])).unwrap_err(),
            DrawError::BadStroke
        );
    }

    #[test]
    fn a_sketch_can_be_undone_and_redone() {
        let mut doc = Document::new(ActorId(1));
        let mut history = History::new();
        let (id, actions) =
            profile_sketch(&doc, &mut SeqIds::new(1), "A", &stroke(), None).unwrap();
        let edit = doc.edit(actions).unwrap();
        history.record(&edit);
        history.undo(&mut doc).unwrap();
        assert!(doc.entity(id).is_none());
        history.redo(&mut doc).unwrap();
        assert!(doc.entity(id).is_some());
    }
}
