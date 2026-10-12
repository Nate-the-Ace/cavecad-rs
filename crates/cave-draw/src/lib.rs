//! Authoring operations: what a drawing tool does, as document actions.
//!
//! Every function returns `Action`s for `Document::edit`; nothing here writes to a document
//! directly. That is the rule that keeps undo, history and (later) co-editing working: a tool
//! never edits around the one edit path. Strokes are anchored to stations, so they follow the
//! survey when it changes. Pen pressure is kept as per-point widths, which change how the line is
//! drawn and never where it is.

use cave_doc::{Action, AnchorError, Document, Field, Frame, Kind, Value, anchor_stroke};
use cave_geom::{Path, Point};
use cave_id::{Id, IdSource};

/// `Feature.type` of a stroke sketched on a profile band.
pub const PROFILE_SKETCH: &str = "profile-sketch";

/// `Feature.type` of a stroke sketched on a station's cross section.
pub const SECTION_SKETCH: &str = "section-sketch";

/// `Feature.type` of the record of where a station's section sketches are placed in a view.
pub const SECTION_PLACEMENT: &str = "section-placement";

/// The views a section can be placed in by hand.
pub const PLACEMENT_VIEWS: [&str; 2] = ["map", "profile"];

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

/// Actions that add a stroke sketched on the cross section at `station`.
///
/// `points` are `[across, up]` in metres in the section's plane (right and up as the passage is
/// seen looking along it), measured from the station. A section sketch is a feature of type
/// `section-sketch` that names its station; `trip` (the trip that surveyed the station) lets the
/// trip filter hide it with its passage. `widths` is optional pen weight per point.
pub fn section_sketch(
    doc: &Document,
    ids: &mut impl IdSource,
    station: Id,
    points: &[[f64; 2]],
    widths: Option<Vec<f64>>,
    trip: Option<Id>,
) -> Result<(Id, Vec<Action>), DrawError> {
    if doc
        .entity(station)
        .is_none_or(|e| e.kind() != Kind::Station)
    {
        return Err(DrawError::NoStations);
    }
    if widths.as_ref().is_some_and(|w| w.len() != points.len()) {
        return Err(DrawError::BadStroke);
    }
    let mut kept: Vec<usize> = Vec::with_capacity(points.len());
    for (i, p) in points.iter().enumerate() {
        if !p[0].is_finite() || !p[1].is_finite() {
            return Err(DrawError::BadStroke);
        }
        if kept.last().is_none_or(|&k| points[k] != *p) {
            kept.push(i);
        }
    }
    if kept.len() < 2 {
        return Err(DrawError::TooShort);
    }
    let pts: Vec<Point> = kept
        .iter()
        .map(|&i| Point::new(points[i][0], points[i][1]))
        .collect();
    let mut path = Path::polyline(&pts, false);
    if let Some(w) = widths {
        path = path
            .with_widths(kept.iter().map(|&i| w[i]).collect())
            .map_err(|_| DrawError::BadStroke)?;
    }
    let id = ids.next_id();
    let mut actions = vec![
        Action::Create {
            id,
            kind: Kind::Feature,
        },
        Action::Set {
            id,
            field: Field::FeatureType,
            value: Value::Text(SECTION_SKETCH.into()),
        },
        Action::Set {
            id,
            field: Field::FeatureStation,
            value: Value::Ref(Some(station)),
        },
        Action::Set {
            id,
            field: Field::FeaturePath,
            value: Value::Path(Some(path)),
        },
    ];
    if let Some(trip) = trip {
        actions.push(Action::Set {
            id,
            field: Field::FeatureTrip,
            value: Value::Ref(Some(trip)),
        });
    }
    Ok((id, actions))
}

/// Actions that set where `station`'s section sketches sit in `view` (`map` or `profile`), as an
/// offset from the station in the view's own metres (`map`: east, north, up; `profile`: along the
/// band, elevation, unused). One record per station and view, so two people placing the same
/// station in different views never clash, and placing again edits the same record.
pub fn place_section(
    doc: &Document,
    ids: &mut impl IdSource,
    station: Id,
    view: &str,
    offset: [f64; 3],
) -> Result<(Id, Vec<Action>), DrawError> {
    if !PLACEMENT_VIEWS.contains(&view) || offset.iter().any(|v| !v.is_finite()) {
        return Err(DrawError::BadStroke);
    }
    if doc
        .entity(station)
        .is_none_or(|e| e.kind() != Kind::Station)
    {
        return Err(DrawError::NoStations);
    }
    let existing = doc.ids_of(Kind::Feature).into_iter().find(|id| {
        doc.entity(*id).is_some_and(|e| {
            e.text(Field::FeatureType) == SECTION_PLACEMENT
                && e.text(Field::FeatureRun) == view
                && e.reference(Field::FeatureStation) == Some(station)
        })
    });
    let tags = std::collections::BTreeMap::from([
        ("x".to_string(), offset[0].to_string()),
        ("y".to_string(), offset[1].to_string()),
        ("z".to_string(), offset[2].to_string()),
    ]);
    let mut actions = Vec::new();
    let id = match existing {
        Some(id) => id,
        None => {
            let id = ids.next_id();
            actions.extend([
                Action::Create {
                    id,
                    kind: Kind::Feature,
                },
                Action::Set {
                    id,
                    field: Field::FeatureType,
                    value: Value::Text(SECTION_PLACEMENT.into()),
                },
                Action::Set {
                    id,
                    field: Field::FeatureRun,
                    value: Value::Text(view.into()),
                },
                Action::Set {
                    id,
                    field: Field::FeatureStation,
                    value: Value::Ref(Some(station)),
                },
            ]);
            id
        }
    };
    actions.push(Action::Set {
        id,
        field: Field::FeatureTags,
        value: Value::Map(tags),
    });
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

    #[test]
    fn a_section_sketch_names_its_station_keeps_pressure_and_undoes() {
        let (mut doc, _, [a, _]) = cave();
        let pts = vec![[0.0, 0.0], [0.0, 0.0], [1.0, 0.5], [2.0, 0.0]];
        let (id, actions) = section_sketch(
            &doc,
            &mut SeqIds::new(5),
            a,
            &pts,
            Some(vec![0.5, 0.6, 1.0, 1.5]),
            None,
        )
        .unwrap();
        let mut history = History::new();
        let edit = doc.edit(actions).unwrap();
        history.record(&edit);
        let e = doc.entity(id).unwrap();
        assert_eq!(e.text(Field::FeatureType), SECTION_SKETCH);
        assert_eq!(e.reference(Field::FeatureStation), Some(a));
        let Value::Path(Some(path)) = e.get(Field::FeaturePath) else {
            panic!("no path")
        };
        assert_eq!(path.points().len(), 3, "the repeated point is dropped");
        assert_eq!(path.widths(), Some(&[0.5, 1.0, 1.5][..]), "with its weight");
        assert!(validate(&doc).is_empty());
        history.undo(&mut doc).unwrap();
        assert!(doc.ids_of(Kind::Feature).is_empty(), "undo removes it");
    }

    #[test]
    fn a_section_sketch_needs_a_station_and_a_real_line() {
        let (doc, _, [a, _]) = cave();
        let ids = &mut SeqIds::new(1);
        let line = [[0.0, 0.0], [1.0, 1.0]];
        let nowhere = Id::from_name("nothing");
        assert_eq!(
            section_sketch(&doc, ids, nowhere, &line, None, None).unwrap_err(),
            DrawError::NoStations
        );
        assert_eq!(
            section_sketch(&doc, ids, a, &[[1.0, 1.0], [1.0, 1.0]], None, None).unwrap_err(),
            DrawError::TooShort
        );
        assert_eq!(
            section_sketch(&doc, ids, a, &[[0.0, 0.0], [f64::NAN, 1.0]], None, None).unwrap_err(),
            DrawError::BadStroke
        );
        assert_eq!(
            section_sketch(&doc, ids, a, &line, Some(vec![1.0]), None).unwrap_err(),
            DrawError::BadStroke
        );
    }

    #[test]
    fn deleting_the_station_a_section_sketch_hangs_on_is_flagged() {
        let (mut doc, _, [a, _]) = cave();
        let (id, actions) = section_sketch(
            &doc,
            &mut SeqIds::new(1),
            a,
            &[[0.0, 0.0], [1.0, 1.0]],
            None,
            None,
        )
        .unwrap();
        doc.edit(actions).unwrap();
        doc.edit(vec![Action::Remove {
            id: a,
            kind: Kind::Station,
        }])
        .unwrap();
        assert!(validate(&doc).contains(&Issue::DanglingReference {
            entity: id,
            field: Field::FeatureStation,
            target: a
        }));
    }

    #[test]
    fn placing_a_section_twice_edits_one_record_per_station_and_view() {
        let (mut doc, _, [a, b]) = cave();
        let mut ids = SeqIds::new(1);
        let mut history = History::new();
        let mut place = |doc: &mut Document, st: Id, view: &str, off: [f64; 3]| {
            let (id, actions) = place_section(doc, &mut ids, st, view, off).unwrap();
            let edit = doc.edit(actions).unwrap();
            history.record(&edit);
            id
        };
        let first = place(&mut doc, a, "map", [1.0, 2.0, 0.0]);
        let again = place(&mut doc, a, "map", [3.0, 4.0, 0.5]);
        assert_eq!(first, again, "the same record is edited");
        place(&mut doc, a, "profile", [5.0, 6.0, 0.0]);
        place(&mut doc, b, "map", [7.0, 8.0, 0.0]);
        assert_eq!(doc.ids_of(Kind::Feature).len(), 3);
        let Value::Map(tags) = doc.entity(first).unwrap().get(Field::FeatureTags) else {
            panic!("no tags")
        };
        assert_eq!(tags["x"], "3");
        assert_eq!(tags["z"], "0.5");
        assert!(validate(&doc).is_empty());
        history.undo(&mut doc).unwrap();
        assert_eq!(
            doc.ids_of(Kind::Feature).len(),
            2,
            "undo removes the last record"
        );
    }

    #[test]
    fn a_placement_needs_a_station_a_known_view_and_real_numbers() {
        let (doc, _, [a, _]) = cave();
        let ids = &mut SeqIds::new(1);
        assert_eq!(
            place_section(&doc, ids, Id::from_name("x"), "map", [0.0; 3]).unwrap_err(),
            DrawError::NoStations
        );
        assert!(place_section(&doc, ids, a, "sheet", [0.0; 3]).is_err());
        assert!(place_section(&doc, ids, a, "map", [f64::NAN, 0.0, 0.0]).is_err());
    }
}
