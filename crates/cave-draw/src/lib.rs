//! Authoring operations: what a drawing tool does, as document actions.
//!
//! Every function returns `Action`s for `Document::edit`; nothing here writes to a document
//! directly. That is the rule that keeps undo, history and (later) co-editing working: a tool
//! never edits around the one edit path. Strokes are anchored to stations, so they follow the
//! survey when it changes. Pen pressure is kept as per-point widths, which change how the line is
//! drawn and never where it is.
//!
//! Linework is one thing drawn in a view: its `FeatureType` says what it is (`wall`, `ledge`,
//! `pit`, ...) and `FeatureView` says where it was drawn, which decides what its points mean. The
//! person picks a drawing tool and then draws in whichever view they like.

use cave_doc::{Action, AnchorError, Document, Field, Frame, Kind, Value, anchor_stroke};
use cave_geom::{Path, Point};
use cave_id::{Id, IdSource};

/// `FeatureView` of a stroke drawn on a profile band.
pub const VIEW_PROFILE: &str = "profile";
/// `FeatureView` of a stroke drawn on a cross section.
pub const VIEW_SECTION: &str = "section";
/// `FeatureView` of a stroke drawn in plan on the map.
pub const VIEW_MAP: &str = "map";

/// `Feature.type` of the record of where a station's section sketches are placed in a view.
pub const SECTION_PLACEMENT: &str = "section-placement";

/// The views a section can be placed in by hand.
pub const PLACEMENT_VIEWS: [&str; 2] = ["map", "profile"];

/// Why a stroke could not be turned into actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawError {
    /// A stroke needs at least two distinct points.
    TooShort,
    /// A coordinate or pen weight was not a usable number, or the type was empty.
    BadStroke,
    /// There is no usable stretch of survey (or station) to anchor the stroke to.
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

/// What a stroke is (`wall`, `ledge`, ...): a short plain word, never empty.
fn valid_kind(kind: &str) -> bool {
    !kind.is_empty() && kind.len() <= 40 && kind.chars().all(|c| c.is_ascii_graphic())
}

/// A stroke's points and, when drawn with pressure, a pen weight for each.
type Stroke = (Vec<[f64; 2]>, Option<Vec<f64>>);

/// Drops repeated points (a stroke that never moved is not a line), with their pen weights.
fn distinct(points: &[[f64; 2]], widths: Option<Vec<f64>>) -> Result<Stroke, DrawError> {
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
    Ok((
        kept.iter().map(|&i| points[i]).collect(),
        widths.map(|w| kept.iter().map(|&i| w[i]).collect()),
    ))
}

/// Actions that create a feature of `kind` drawn in `view`, then set `fields`.
fn feature(
    ids: &mut impl IdSource,
    kind: &str,
    view: &str,
    fields: Vec<(Field, Value)>,
) -> (Id, Vec<Action>) {
    let id = ids.next_id();
    let mut actions = vec![
        Action::Create {
            id,
            kind: Kind::Feature,
        },
        Action::Set {
            id,
            field: Field::FeatureType,
            value: Value::Text(kind.into()),
        },
        Action::Set {
            id,
            field: Field::FeatureView,
            value: Value::Text(view.into()),
        },
    ];
    actions.extend(
        fields
            .into_iter()
            .map(|(field, value)| Action::Set { id, field, value }),
    );
    (id, actions)
}

/// Actions that add a stroke of `kind` (`wall`, `ledge`, ...) drawn on the profile band of `run`.
///
/// `points` are `(s, z)` in metres on the band (distance along the run, elevation); `frames` are
/// the band's consecutive station pairs with their band positions, and the stroke is anchored to
/// the nearest of them. `widths` is optional pen weight per point (1.0 is the normal weight).
/// Returns the new feature's id with the actions. Grouping and visibility come from type and
/// trip, not from layers.
pub fn profile_sketch(
    _doc: &Document,
    ids: &mut impl IdSource,
    kind: &str,
    run: &str,
    points: &[[f64; 2]],
    widths: Option<Vec<f64>>,
    frames: &[Frame],
) -> Result<(Id, Vec<Action>), DrawError> {
    if !valid_kind(kind) {
        return Err(DrawError::BadStroke);
    }
    let (pts, widths) = distinct(points, widths)?;
    let anchored = anchor_stroke(&pts, frames, widths)?;
    Ok(feature(
        ids,
        kind,
        VIEW_PROFILE,
        vec![
            (Field::FeatureRun, Value::Text(run.into())),
            (Field::FeatureAnchored, Value::Anchored(Some(anchored))),
        ],
    ))
}

/// Actions that add a stroke of `kind` drawn in plan on the map.
///
/// `points` are `(x, y)` in metres (east, north); `frames` are the survey's legs with their plan
/// positions, and the stroke is anchored to the nearest of them so it follows the survey when it
/// is adjusted. `widths` is optional pen weight per point.
pub fn map_line(
    _doc: &Document,
    ids: &mut impl IdSource,
    kind: &str,
    points: &[[f64; 2]],
    widths: Option<Vec<f64>>,
    frames: &[Frame],
) -> Result<(Id, Vec<Action>), DrawError> {
    if !valid_kind(kind) {
        return Err(DrawError::BadStroke);
    }
    let (pts, widths) = distinct(points, widths)?;
    let anchored = anchor_stroke(&pts, frames, widths)?;
    Ok(feature(
        ids,
        kind,
        VIEW_MAP,
        vec![(Field::FeatureAnchored, Value::Anchored(Some(anchored)))],
    ))
}

/// Actions that add a stroke of `kind` drawn on the cross section at `station`.
///
/// `points` are `[across, up]` in metres in the section's plane (right and up as the passage is
/// seen looking along it), measured from the station. The feature names its station; `trip` (the
/// trip that surveyed the station) lets the trip filter hide it with its passage. `widths` is
/// optional pen weight per point.
pub fn section_sketch(
    doc: &Document,
    ids: &mut impl IdSource,
    kind: &str,
    station: Id,
    points: &[[f64; 2]],
    widths: Option<Vec<f64>>,
    trip: Option<Id>,
) -> Result<(Id, Vec<Action>), DrawError> {
    if !valid_kind(kind) {
        return Err(DrawError::BadStroke);
    }
    if doc
        .entity(station)
        .is_none_or(|e| e.kind() != Kind::Station)
    {
        return Err(DrawError::NoStations);
    }
    let (pts, widths) = distinct(points, widths)?;
    let mut fields = vec![
        (Field::FeatureStation, Value::Ref(Some(station))),
        (
            Field::FeaturePath,
            Value::Path(Some(spline(&pts, widths, false)?)),
        ),
    ];
    if let Some(trip) = trip {
        fields.push((Field::FeatureTrip, Value::Ref(Some(trip))));
    }
    Ok(feature(ids, kind, VIEW_SECTION, fields))
}

/// A smooth curve through the fit points, with pen weights when there are some.
fn spline(pts: &[[f64; 2]], widths: Option<Vec<f64>>, closed: bool) -> Result<Path, DrawError> {
    if closed && pts.len() < 3 {
        return Err(DrawError::TooShort);
    }
    let path = Path::Spline {
        points: pts.iter().map(|p| Point::new(p[0], p[1])).collect(),
        closed,
        widths: None,
    };
    match widths {
        Some(w) => path.with_widths(w).map_err(|_| DrawError::BadStroke),
        None => Ok(path),
    }
}

/// What kind of drawing a feature is, if it is linework drawn in a view: `(view, kind)`.
fn drawn_in(doc: &Document, id: Id) -> Option<(String, String)> {
    let e = doc.entity(id).filter(|e| e.kind() == Kind::Feature)?;
    let view = e.text(Field::FeatureView);
    [VIEW_PROFILE, VIEW_SECTION, VIEW_MAP]
        .contains(&view.as_str())
        .then(|| (view, e.text(Field::FeatureType)))
}

/// Actions that move the fit points of an existing stroke (its curve is redrawn through them).
///
/// `points` are in the same space the stroke was drawn in (band metres for a profile stroke, plan
/// metres for a map stroke, section metres for a section stroke). For profile and map strokes
/// `frames` re-anchor it to the survey, as when it was drawn; a section stroke ignores them.
pub fn edit_stroke(
    doc: &Document,
    id: Id,
    points: &[[f64; 2]],
    widths: Option<Vec<f64>>,
    frames: &[Frame],
) -> Result<Vec<Action>, DrawError> {
    let (view, _) = drawn_in(doc, id).ok_or(DrawError::NoStations)?;
    let (pts, widths) = distinct(points, widths)?;
    let closed = is_closed(doc, id);
    let (field, value) = if view == VIEW_SECTION {
        (
            Field::FeaturePath,
            Value::Path(Some(spline(&pts, widths, closed)?)),
        )
    } else {
        let mut anchored = anchor_stroke(&pts, frames, widths)?;
        anchored.closed = closed && pts.len() >= 3;
        (Field::FeatureAnchored, Value::Anchored(Some(anchored)))
    };
    Ok(vec![Action::Set { id, field, value }])
}

/// True if the stroke's curve loops back to its first point.
fn is_closed(doc: &Document, id: Id) -> bool {
    let Some(e) = doc.entity(id) else {
        return false;
    };
    match (e.get(Field::FeaturePath), e.get(Field::FeatureAnchored)) {
        (Value::Path(Some(Path::Spline { closed, .. })), _) => closed,
        (_, Value::Anchored(Some(a))) => a.closed,
        _ => false,
    }
}

/// `actions` (from a `*_sketch` or `map_line` call) with the stroke made a closed loop when
/// `closed` is set. A loop needs at least three fit points.
pub fn with_closed(mut actions: Vec<Action>, closed: bool) -> Result<Vec<Action>, DrawError> {
    if !closed {
        return Ok(actions);
    }
    for a in &mut actions {
        if let Action::Set { value, .. } = a {
            match value {
                Value::Anchored(Some(an)) if an.points.len() >= 3 => an.closed = true,
                Value::Path(Some(Path::Spline { points, closed, .. })) if points.len() >= 3 => {
                    *closed = true;
                }
                Value::Anchored(Some(_)) | Value::Path(Some(_)) => return Err(DrawError::TooShort),
                _ => {}
            }
        }
    }
    Ok(actions)
}

/// Actions that close or open an existing stroke (a closed one loops back to its first point).
pub fn set_stroke_closed(doc: &Document, id: Id, closed: bool) -> Result<Vec<Action>, DrawError> {
    drawn_in(doc, id).ok_or(DrawError::NoStations)?;
    let e = doc.entity(id).ok_or(DrawError::NoStations)?;
    let value = match (e.get(Field::FeaturePath), e.get(Field::FeatureAnchored)) {
        (Value::Path(Some(Path::Spline { points, widths, .. })), _) => {
            if closed && points.len() < 3 {
                return Err(DrawError::TooShort);
            }
            (
                Field::FeaturePath,
                Value::Path(Some(Path::Spline {
                    points,
                    closed,
                    widths,
                })),
            )
        }
        (_, Value::Anchored(Some(mut a))) => {
            if closed && a.points.len() < 3 {
                return Err(DrawError::TooShort);
            }
            a.closed = closed;
            (Field::FeatureAnchored, Value::Anchored(Some(a)))
        }
        _ => return Err(DrawError::NoStations),
    };
    Ok(vec![Action::Set {
        id,
        field: value.0,
        value: value.1,
    }])
}

/// Adds a text to a new stroke's actions (the stroke's label, or the words of a text stroke), so the
/// stroke and its words are one edit. Empty text adds nothing.
pub fn with_text(mut actions: Vec<Action>, id: Id, text: &str) -> Vec<Action> {
    let text = text.trim();
    if !text.is_empty() {
        actions.push(Action::Set {
            id,
            field: Field::FeatureText,
            value: Value::Text(text.chars().take(MAX_TEXT).collect()),
        });
    }
    actions
}

/// The longest text a stroke carries.
pub const MAX_TEXT: usize = 200;

/// Actions that change the words of a stroke (empty clears them).
pub fn set_stroke_text(doc: &Document, id: Id, text: &str) -> Result<Vec<Action>, DrawError> {
    drawn_in(doc, id).ok_or(DrawError::NoStations)?;
    Ok(vec![Action::Set {
        id,
        field: Field::FeatureText,
        value: Value::Text(text.trim().chars().take(MAX_TEXT).collect()),
    }])
}

/// Actions that put a shaped line's ornament on the other side.
pub fn set_stroke_flip(doc: &Document, id: Id, flip: bool) -> Result<Vec<Action>, DrawError> {
    drawn_in(doc, id).ok_or(DrawError::NoStations)?;
    Ok(vec![Action::Set {
        id,
        field: Field::FeatureFlip,
        value: Value::Flag(flip),
    }])
}

/// Actions that change what a stroke is (`wall`, `ledge`, ...): its look follows its type.
pub fn set_stroke_kind(doc: &Document, id: Id, kind: &str) -> Result<Vec<Action>, DrawError> {
    drawn_in(doc, id).ok_or(DrawError::NoStations)?;
    if !valid_kind(kind) {
        return Err(DrawError::BadStroke);
    }
    Ok(vec![Action::Set {
        id,
        field: Field::FeatureType,
        value: Value::Text(kind.into()),
    }])
}

/// Actions that delete a stroke drawn in a view.
pub fn remove_stroke(doc: &Document, id: Id) -> Result<Vec<Action>, DrawError> {
    drawn_in(doc, id).ok_or(DrawError::NoStations)?;
    Ok(vec![Action::Remove {
        id,
        kind: Kind::Feature,
    }])
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
    fn a_stroke_carries_words_in_the_same_edit_and_they_can_be_changed() {
        let (mut doc, frames, _) = cave();
        let (id, actions) = profile_sketch(
            &doc,
            &mut SeqIds::new(2),
            "sym-pit",
            "A",
            &stroke()[..2],
            None,
            &frames,
        )
        .unwrap();
        let n = actions.len();
        let with = with_text(actions.clone(), id, "  P 19  ");
        assert_eq!(with.len(), n + 1, "one extra action, in the same edit");
        assert_eq!(
            with_text(actions, id, "   ").len(),
            n,
            "empty words add nothing"
        );
        let mut history = History::new();
        let e = doc.edit(with).unwrap();
        history.record(&e);
        assert_eq!(doc.entity(id).unwrap().text(Field::FeatureText), "P 19");
        let e = doc
            .edit(set_stroke_text(&doc, id, "P 20").unwrap())
            .unwrap();
        history.record(&e);
        assert_eq!(doc.entity(id).unwrap().text(Field::FeatureText), "P 20");
        history.undo(&mut doc).unwrap();
        assert_eq!(doc.entity(id).unwrap().text(Field::FeatureText), "P 19");
        let long = "x".repeat(MAX_TEXT + 50);
        doc.edit(set_stroke_text(&doc, id, &long).unwrap()).unwrap();
        assert_eq!(
            doc.entity(id)
                .unwrap()
                .text(Field::FeatureText)
                .chars()
                .count(),
            MAX_TEXT
        );
    }

    #[test]
    fn a_sketch_is_a_valid_anchored_feature_() {
        let (mut doc, frames, [a, b]) = cave();
        let (id, actions) = profile_sketch(
            &doc,
            &mut SeqIds::new(1),
            "wall",
            "A",
            &stroke(),
            None,
            &frames,
        )
        .unwrap();
        doc.edit(actions).unwrap();
        let e = doc.entity(id).unwrap();
        assert_eq!(
            (
                e.text(Field::FeatureRun),
                e.text(Field::FeatureType),
                e.text(Field::FeatureView)
            ),
            ("A".into(), "wall".to_string(), VIEW_PROFILE.to_string())
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
        let (id, actions) = profile_sketch(
            &doc,
            &mut SeqIds::new(1),
            "wall",
            "A",
            &stroke(),
            None,
            &frames,
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
            field: Field::FeatureAnchored,
            target: a
        }));
    }

    #[test]
    fn two_people_sketching_independently_merge_cleanly() {
        let (base, frames, _) = cave();
        let mut a = base.clone();
        let mut b = Document::from_json(&base.to_json(), ActorId(2)).unwrap();
        let (_, ea) = profile_sketch(
            &a,
            &mut SeqIds::new(1),
            "wall",
            "A",
            &stroke(),
            None,
            &frames,
        )
        .unwrap();
        let (_, eb) = profile_sketch(
            &b,
            &mut SeqIds::new(2),
            "wall",
            "A",
            &stroke(),
            None,
            &frames,
        )
        .unwrap();
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
            "wall",
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
            profile_sketch(&doc, ids, "wall", "A", pts, w, f).unwrap_err()
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
        let (id, actions) = profile_sketch(
            &doc,
            &mut SeqIds::new(1),
            "wall",
            "A",
            &stroke(),
            None,
            &frames,
        )
        .unwrap();
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
            "wall",
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
        assert_eq!(e.text(Field::FeatureType), "wall");
        assert_eq!(e.text(Field::FeatureView), VIEW_SECTION);
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
            section_sketch(&doc, ids, "wall", nowhere, &line, None, None).unwrap_err(),
            DrawError::NoStations
        );
        assert_eq!(
            section_sketch(&doc, ids, "wall", a, &[[1.0, 1.0], [1.0, 1.0]], None, None)
                .unwrap_err(),
            DrawError::TooShort
        );
        assert_eq!(
            section_sketch(
                &doc,
                ids,
                "wall",
                a,
                &[[0.0, 0.0], [f64::NAN, 1.0]],
                None,
                None
            )
            .unwrap_err(),
            DrawError::BadStroke
        );
        assert_eq!(
            section_sketch(&doc, ids, "wall", a, &line, Some(vec![1.0]), None).unwrap_err(),
            DrawError::BadStroke
        );
    }

    #[test]
    fn deleting_the_station_a_section_sketch_hangs_on_is_flagged() {
        let (mut doc, _, [a, _]) = cave();
        let (id, actions) = section_sketch(
            &doc,
            &mut SeqIds::new(1),
            "wall",
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

    #[test]
    fn strokes_are_splines_and_their_fit_points_can_be_moved_deleted_and_undone() {
        let (mut doc, frames, [a, _]) = cave();
        let mut history = History::new();
        let ids = &mut SeqIds::new(1);
        // A section stroke: a spline whose points can be moved.
        let (sec, actions) = section_sketch(
            &doc,
            ids,
            "wall",
            a,
            &[[0.0, 0.0], [1.0, 1.0], [2.0, 0.0]],
            Some(vec![1.0, 1.5, 1.0]),
            None,
        )
        .unwrap();
        history.record(&doc.edit(actions).unwrap());
        let Value::Path(Some(Path::Spline { points, .. })) =
            doc.entity(sec).unwrap().get(Field::FeaturePath)
        else {
            panic!("a section stroke is a spline")
        };
        assert_eq!(points.len(), 3);
        let moved = [[0.0, 0.0], [1.0, 2.5], [2.0, 0.0]];
        history.record(
            &doc.edit(edit_stroke(&doc, sec, &moved, Some(vec![1.0, 1.5, 1.0]), &[]).unwrap())
                .unwrap(),
        );
        let Value::Path(Some(p)) = doc.entity(sec).unwrap().get(Field::FeaturePath) else {
            panic!()
        };
        assert_eq!(p.points()[1], Point::new(1.0, 2.5), "the vertex moved");
        // A profile stroke is re-anchored to the survey when its points move.
        let (prof, actions) = profile_sketch(
            &doc,
            ids,
            "ledge",
            "A",
            &[[1.0, 1.0], [5.0, 1.0], [9.0, 2.0]],
            None,
            &frames,
        )
        .unwrap();
        history.record(&doc.edit(actions).unwrap());
        history.record(
            &doc.edit(
                edit_stroke(
                    &doc,
                    prof,
                    &[[1.0, 1.0], [5.0, 3.0], [9.0, 2.0]],
                    None,
                    &frames,
                )
                .unwrap(),
            )
            .unwrap(),
        );
        let Value::Anchored(Some(an)) = doc.entity(prof).unwrap().get(Field::FeatureAnchored)
        else {
            panic!()
        };
        let placed = an
            .place(&|st| {
                frames
                    .iter()
                    .find_map(|f| (f.a == st).then_some(f.pa).or((f.b == st).then_some(f.pb)))
            })
            .unwrap();
        assert!((placed[1][1] - 3.0).abs() < 1e-9, "{placed:?}");
        // Delete, then undo brings it back.
        history.record(&doc.edit(remove_stroke(&doc, prof).unwrap()).unwrap());
        assert!(doc.entity(prof).is_none());
        history.undo(&mut doc).unwrap();
        assert!(
            doc.entity(prof).is_some(),
            "undo restores the deleted stroke"
        );
        assert!(validate(&doc).is_empty());
        // Only drawn linework can be edited or deleted.
        assert!(edit_stroke(&doc, a, &moved, None, &frames).is_err());
        assert!(remove_stroke(&doc, a).is_err());
    }

    #[test]
    fn strokes_can_be_closed_reopened_retyped_and_keep_their_loop_when_edited() {
        let (mut doc, frames, [a, _]) = cave();
        let mut history = History::new();
        let ids = &mut SeqIds::new(1);
        let tri = [[0.0, 0.0], [4.0, 0.0], [2.0, 3.0]];
        // drawn closed from the start
        let (sec, actions) = section_sketch(&doc, ids, "pit", a, &tri, None, None).unwrap();
        history.record(&doc.edit(with_closed(actions, true).unwrap()).unwrap());
        let closed_of = |doc: &Document, id: Id| is_closed(doc, id);
        assert!(closed_of(&doc, sec), "drawn as a loop");
        // moving a fit point keeps the loop
        history.record(
            &doc.edit(
                edit_stroke(&doc, sec, &[[0.0, 0.0], [4.0, 0.0], [2.0, 5.0]], None, &[]).unwrap(),
            )
            .unwrap(),
        );
        assert!(closed_of(&doc, sec));
        // open it again, then close it again
        history.record(
            &doc.edit(set_stroke_closed(&doc, sec, false).unwrap())
                .unwrap(),
        );
        assert!(!closed_of(&doc, sec));
        history.record(
            &doc.edit(set_stroke_closed(&doc, sec, true).unwrap())
                .unwrap(),
        );
        assert!(closed_of(&doc, sec));
        // change what it is
        history.record(
            &doc.edit(set_stroke_kind(&doc, sec, "ledge").unwrap())
                .unwrap(),
        );
        assert_eq!(doc.entity(sec).unwrap().text(Field::FeatureType), "ledge");
        history.undo(&mut doc).unwrap();
        assert_eq!(
            doc.entity(sec).unwrap().text(Field::FeatureType),
            "pit",
            "undo restores the type"
        );
        // an anchored (profile) stroke closes the same way and survives an edit
        let (prof, actions) = profile_sketch(
            &doc,
            ids,
            "wall",
            "A",
            &[[1.0, 1.0], [5.0, 4.0], [9.0, 1.0]],
            None,
            &frames,
        )
        .unwrap();
        history.record(&doc.edit(with_closed(actions, true).unwrap()).unwrap());
        assert!(closed_of(&doc, prof));
        history.record(
            &doc.edit(
                edit_stroke(
                    &doc,
                    prof,
                    &[[1.0, 1.0], [5.0, 5.0], [9.0, 1.0]],
                    None,
                    &frames,
                )
                .unwrap(),
            )
            .unwrap(),
        );
        assert!(closed_of(&doc, prof), "editing keeps the loop");
        // a loop needs three fit points; bad types are refused
        let (_, actions) =
            section_sketch(&doc, ids, "wall", a, &[[0.0, 0.0], [1.0, 1.0]], None, None).unwrap();
        assert!(with_closed(actions, true).is_err());
        assert!(set_stroke_kind(&doc, sec, "").is_err());
        assert!(validate(&doc).is_empty());
    }
}
