//! The survey solved once: legs, resolved stations, adjusted or raw positions. Shared by every
//! derived view so they all agree on where the stations are.

use cave_doc::{Document, Field};
use cave_id::Id;
use cave_survey::{Extracted, Params, Resolved, Vec3, adjust, extract, resolve};
use std::collections::{BTreeMap, HashMap};

pub(crate) struct Solved {
    pub ex: Extracted,
    pub resolved: Resolved,
    pub positions: BTreeMap<Id, Vec3>,
    pub labels: HashMap<Id, String>,
    /// Stations in resolution order.
    pub order: Vec<Id>,
}

pub(crate) fn solve(doc: &Document, use_adjustment: bool) -> Solved {
    let ex = extract(doc);
    let resolved = resolve(&ex.legs, &ex.fixed);
    let adj = use_adjustment
        .then(|| adjust(&ex.legs, &resolved, &Params::default()))
        .filter(|a| a.adjusted);
    let positions: BTreeMap<Id, Vec3> = match &adj {
        Some(a) => a.stations.clone(),
        None => resolved
            .stations
            .iter()
            .map(|(id, p)| (*id, p.pos))
            .collect(),
    };
    let mut order: Vec<(usize, Id)> = resolved
        .stations
        .iter()
        .map(|(id, p)| (p.seq, *id))
        .collect();
    order.sort();
    let order: Vec<Id> = order.into_iter().map(|(_, id)| id).collect();
    let labels = order
        .iter()
        .map(|id| {
            (
                *id,
                doc.entity(*id)
                    .map(|e| e.text(Field::StationLabel))
                    .unwrap_or_default(),
            )
        })
        .collect();
    Solved {
        ex,
        resolved,
        positions,
        labels,
        order,
    }
}
