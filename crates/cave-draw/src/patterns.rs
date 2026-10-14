//! The patterns a caver authors (motifs, linetypes and fills), kept in the document so they save, undo and merge with the cave.
//!
//! Each pattern is one entity whose id comes from its kind and name: two people saving the same pattern write to the same entity
//! and agree. The definition is JSON text (see `cave-pattern`); the viewer and the export ask `library` for the built-in patterns
//! with the cave's own laid over them.

use cave_doc::{Action, Document, Field, Kind, Value};
use cave_id::Id;
use cave_pattern::{FillDef, Library, LineDef, Motif, PatternError, builtin_library, motifs_used};

/// `PatternKind` of a small drawing that repeats.
pub const MOTIF: &str = "motif";
/// `PatternKind` of a pattern for lines.
pub const LINE: &str = "line";
/// `PatternKind` of a pattern for areas.
pub const FILL: &str = "fill";

/// Why a pattern was not saved or removed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatternStoreError {
    /// The definition is not usable.
    Invalid(PatternError),
    /// A motif cannot be removed while a linetype or fill of the cave uses it; the string names the first one.
    InUse(String),
    /// There is no such pattern of the cave's own (a built-in one cannot be removed).
    Unknown,
}

impl From<PatternError> for PatternStoreError {
    fn from(e: PatternError) -> Self {
        PatternStoreError::Invalid(e)
    }
}

impl std::fmt::Display for PatternStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PatternStoreError::Invalid(e) => write!(f, "{e}"),
            PatternStoreError::InUse(by) => write!(f, "It is used by \"{by}\"; change that first."),
            PatternStoreError::Unknown => write!(f, "There is no such pattern of your own."),
        }
    }
}

impl std::error::Error for PatternStoreError {}

/// The id of the pattern of this kind and name.
pub fn pattern_id(kind: &str, name: &str) -> Id {
    Id::from_name(&format!("pattern:{kind}:{name}"))
}

/// The pattern a line's style names (`pat:Name;scale;density`): its name and the style's own scale and density. `None` for a style that
/// names none of the caver's patterns.
pub fn pattern_style(style: &str) -> Option<(String, f64, f64)> {
    let mut parts = style.split(';');
    let name = parts.next()?.strip_prefix("pat:")?;
    if name.is_empty() {
        return None;
    }
    let num = |s: Option<&str>| {
        s.and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(1.0)
    };
    Some((name.to_string(), num(parts.next()), num(parts.next())))
}

/// The cave's own patterns, by name. A stored definition that no longer parses is skipped rather than failing the whole cave.
pub fn own_library(doc: &Document) -> Library {
    let mut lib = Library::default();
    for id in doc.ids_of(Kind::Pattern) {
        let Some(e) = doc.entity(id) else { continue };
        let def = e.text(Field::PatternDef);
        match e.text(Field::PatternKind).as_str() {
            MOTIF => lib.motifs.extend(serde_json::from_str::<Motif>(&def)),
            LINE => lib.lines.extend(serde_json::from_str::<LineDef>(&def)),
            FILL => lib.fills.extend(serde_json::from_str::<FillDef>(&def)),
            _ => {}
        }
    }
    let key = |n: &str| n.to_lowercase();
    lib.motifs.sort_by_key(|m| key(&m.name));
    lib.lines.sort_by_key(|l| key(&l.name));
    lib.fills.sort_by_key(|f| key(&f.name));
    lib
}

/// Everything the cave can pick from: the built-in patterns with the cave's own laid over them.
pub fn library(doc: &Document) -> Library {
    Library::over(builtin_library(), own_library(doc))
}

/// The actions that make the pattern `kind` `name` exist with this definition (nothing when it already is so).
fn write(doc: &Document, kind: &str, name: &str, def: String) -> Vec<Action> {
    let id = pattern_id(kind, name);
    let mut actions = Vec::new();
    let live = doc.entity(id);
    if live.is_none() {
        actions.push(Action::Create {
            id,
            kind: Kind::Pattern,
        });
    }
    for (field, text) in [
        (Field::PatternKind, kind.to_string()),
        (Field::PatternName, name.to_string()),
        (Field::PatternDef, def),
    ] {
        if live.is_none_or(|e| e.text(field) != text) {
            actions.push(Action::Set {
                id,
                field,
                value: Value::Text(text),
            });
        }
    }
    actions
}

/// Actions that save a motif.
pub fn save_motif(doc: &Document, motif: &Motif) -> Result<Vec<Action>, PatternStoreError> {
    motif.validate()?;
    let def = serde_json::to_string(motif).map_err(|_| PatternError::BadJson)?;
    Ok(write(doc, MOTIF, &motif.name, def))
}

/// Actions that save a linetype (its motifs must exist).
pub fn save_line(doc: &Document, line: &LineDef) -> Result<Vec<Action>, PatternStoreError> {
    library(doc).check_line(line)?;
    let def = serde_json::to_string(line).map_err(|_| PatternError::BadJson)?;
    Ok(write(doc, LINE, &line.name, def))
}

/// Actions that save a fill (its motifs must exist).
pub fn save_fill(doc: &Document, fill: &FillDef) -> Result<Vec<Action>, PatternStoreError> {
    library(doc).check_fill(fill)?;
    let def = serde_json::to_string(fill).map_err(|_| PatternError::BadJson)?;
    Ok(write(doc, FILL, &fill.name, def))
}

/// The name of the first cave linetype or fill that uses the motif, if any.
pub fn motif_used_by(doc: &Document, motif: &str) -> Option<String> {
    let lib = own_library(doc);
    if let Some(l) = lib.lines.iter().find(|l| motifs_used(l).contains(&motif)) {
        return Some(l.name.clone());
    }
    lib.fills
        .iter()
        .find(|f| {
            f.layers
                .iter()
                .any(|l| matches!(l, cave_pattern::Layer::Tile { motif: m, .. } if m == motif))
        })
        .map(|f| f.name.clone())
}

/// Actions that remove one of the cave's own patterns. A motif in use cannot go.
pub fn remove_pattern(
    doc: &Document,
    kind: &str,
    name: &str,
) -> Result<Vec<Action>, PatternStoreError> {
    let id = pattern_id(kind, name);
    if doc.entity(id).is_none() {
        return Err(PatternStoreError::Unknown);
    }
    if kind == MOTIF
        && let Some(by) = motif_used_by(doc, name)
    {
        return Err(PatternStoreError::InUse(by));
    }
    Ok(vec![Action::Remove {
        id,
        kind: Kind::Pattern,
    }])
}

#[cfg(test)]
mod tests {
    use super::*;
    use cave_doc::ActorId;
    use cave_pattern::{Element, Layer};
    use cave_symbols::Prim;

    fn doc() -> Document {
        Document::new(ActorId(1))
    }
    fn motif(name: &str) -> Motif {
        Motif::from_drawing(
            name,
            vec![Prim::Poly {
                pts: vec![[0.0, 0.0], [1.0, 2.0]],
                closed: false,
                fill: false,
            }],
        )
        .unwrap()
    }
    fn line(name: &str, m: &str) -> LineDef {
        LineDef {
            name: name.into(),
            elements: vec![
                Element::Dash { mm: 3.0 },
                Element::Motif {
                    motif: m.into(),
                    size_mm: 2.0,
                    turn_deg: 0.0,
                    along_mm: 0.0,
                    across_mm: 0.0,
                },
                Element::Gap { mm: 2.0 },
            ],
        }
    }
    fn save(d: &mut Document, actions: Vec<Action>) {
        d.edit(actions).unwrap();
    }

    #[test]
    fn a_line_style_names_its_pattern_or_none() {
        assert_eq!(
            pattern_style("pat:Cave ticks;1.50;2.00"),
            Some(("Cave ticks".into(), 1.5, 2.0))
        );
        assert_eq!(pattern_style("pat:Hooks"), Some(("Hooks".into(), 1.0, 1.0)));
        assert_eq!(pattern_style("dashed;1.00;1.00"), None);
        assert_eq!(pattern_style(""), None);
        assert_eq!(pattern_style("pat:;1;1"), None);
    }

    #[test]
    fn saved_patterns_are_in_the_library_beside_the_built_in_ones() {
        let mut d = doc();
        let m = motif("Hook");
        let a = save_motif(&d, &m).unwrap();
        save(&mut d, a);
        let a = save_line(&d, &line("Hooked", "Hook")).unwrap();
        save(&mut d, a);
        let lib = library(&d);
        assert!(lib.motif("Hook").is_some() && lib.motif("Tick").is_some());
        assert_eq!(lib.line("Hooked").unwrap().elements.len(), 3);
        assert_eq!(own_library(&d).lines.len(), 1);
    }

    #[test]
    fn saving_the_same_thing_twice_writes_nothing_and_a_change_writes_it() {
        let mut d = doc();
        let a = save_motif(&d, &motif("Hook")).unwrap();
        save(&mut d, a);
        assert!(save_motif(&d, &motif("Hook")).unwrap().is_empty());
        let mut changed = motif("Hook");
        changed.prims.push(Prim::Circle {
            c: [0.0, 0.0],
            r: 0.1,
            fill: false,
        });
        assert_eq!(
            save_motif(&d, &changed).unwrap().len(),
            1,
            "only the definition"
        );
    }

    #[test]
    fn a_line_cannot_name_a_motif_that_does_not_exist() {
        let d = doc();
        assert_eq!(
            save_line(&d, &line("Bad", "Nope")),
            Err(PatternStoreError::Invalid(PatternError::MissingMotif(
                "Nope".into()
            )))
        );
    }

    #[test]
    fn a_motif_in_use_cannot_be_removed_until_nothing_uses_it() {
        let mut d = doc();
        let a = save_motif(&d, &motif("Hook")).unwrap();
        save(&mut d, a);
        let a = save_line(&d, &line("Hooked", "Hook")).unwrap();
        save(&mut d, a);
        assert_eq!(
            remove_pattern(&d, MOTIF, "Hook"),
            Err(PatternStoreError::InUse("Hooked".into()))
        );
        let a = remove_pattern(&d, LINE, "Hooked").unwrap();
        save(&mut d, a);
        let a = remove_pattern(&d, MOTIF, "Hook").unwrap();
        save(&mut d, a);
        assert!(own_library(&d).motifs.is_empty());
        assert_eq!(
            remove_pattern(&d, MOTIF, "Tick"),
            Err(PatternStoreError::Unknown),
            "built-ins are not the cave's to remove"
        );
    }

    #[test]
    fn a_fill_that_tiles_a_motif_also_holds_it_in_use() {
        let mut d = doc();
        let a = save_motif(&d, &motif("Hook")).unwrap();
        save(&mut d, a);
        let fill = FillDef {
            name: "Hooks".into(),
            layers: vec![Layer::Tile {
                motif: "Hook".into(),
                size_mm: 2.0,
                across_mm: 5.0,
                down_mm: 5.0,
                stagger: false,
                turn_deg: 0.0,
                random_turn: false,
                size_var: 0.0,
                jitter: 0.0,
                skip: 0.0,
            }],
            tint: false,
        };
        let a = save_fill(&d, &fill).unwrap();
        save(&mut d, a);
        assert_eq!(motif_used_by(&d, "Hook"), Some("Hooks".into()));
    }

    #[test]
    fn two_people_saving_the_same_name_converge_and_survive_json() {
        let mut a = doc();
        let mut b = Document::new(ActorId(2));
        let ea = a.edit(save_motif(&a, &motif("Hook")).unwrap()).unwrap();
        let eb = b.edit(save_motif(&b, &motif("Hook")).unwrap()).unwrap();
        for op in &ea.ops {
            b.apply(op).unwrap();
        }
        for op in &eb.ops {
            a.apply(op).unwrap();
        }
        assert_eq!(a.ids_of(Kind::Pattern), b.ids_of(Kind::Pattern));
        assert_eq!(own_library(&a), own_library(&b));
        let back = Document::from_json(&a.to_json(), ActorId(3)).unwrap();
        assert_eq!(own_library(&back), own_library(&a));
    }
}
