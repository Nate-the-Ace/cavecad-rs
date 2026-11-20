//! The stations a shot can stand over: a cave rebuilt from its server texts, placed by the survey.

use cave_doc::{ActorId, Document, Field, Op};
use cave_id::Id;
use cave_survey::{Params, Vec3, adjust, extract, resolve};
use serde::Deserialize;

/// A placed station.
#[derive(Clone, Debug, PartialEq)]
pub struct StationSeed {
    /// Its id.
    pub id: Id,
    /// Its label.
    pub label: String,
    /// Survey position, metres (x east, y north, z up), as the app shows it: loop-closure adjusted,
    /// or raw where the adjustment did not converge.
    pub pos: Vec3,
}

#[derive(Deserialize)]
struct Ops {
    ops: Vec<Op>,
}

/// Rebuilds a cave from the server's open snapshot text (`{"upTo":n,"ops":[...]}`, if any) and the
/// entry texts after it (`{"c":client,"ops":[...]}`), in number order. An op that does not fit is
/// skipped, as the app's own sync does.
pub fn document_from_room(snapshot: Option<&str>, entries: &[&str]) -> Result<Document, String> {
    let mut doc = Document::new(ActorId(1));
    let parts = snapshot
        .map(|s| ("snapshot".to_string(), s))
        .into_iter()
        .chain(
            entries
                .iter()
                .enumerate()
                .map(|(i, e)| (format!("entry {}", i + 1), *e)),
        );
    for (name, text) in parts {
        let o: Ops = serde_json::from_str(text).map_err(|e| format!("{name} unreadable: {e}"))?;
        for op in &o.ops {
            let _ = doc.apply(op);
        }
    }
    Ok(doc)
}

/// Every station the survey places, in survey order. Positions are always the loop-closure
/// adjusted ones, falling back to raw only where the adjustment does not converge. The CaveCAD
/// map's "adjustment off" view is deliberately not followed: a capture room is tied to the
/// adjusted survey.
pub fn stations(doc: &Document) -> Vec<StationSeed> {
    let ex = extract(doc);
    let r = resolve(&ex.legs, &ex.fixed);
    let adj = Some(adjust(&ex.legs, &r, &Params::default())).filter(|a| a.adjusted);
    let mut placed: Vec<_> = r.stations.iter().collect();
    placed.sort_by_key(|(_, p)| p.seq);
    placed
        .into_iter()
        .map(|(id, p)| StationSeed {
            id: *id,
            label: doc
                .entity(*id)
                .map(|e| e.text(Field::StationLabel))
                .unwrap_or_default(),
            pos: adj
                .as_ref()
                .and_then(|a| a.stations.get(id).copied())
                .unwrap_or(p.pos),
        })
        .collect()
}
