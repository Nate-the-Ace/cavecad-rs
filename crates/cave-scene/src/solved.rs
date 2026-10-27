//! The survey solved once: legs, resolved stations, adjusted or raw positions. Shared by every
//! derived view so they all agree on where the stations are.

use cave_doc::{Document, Field};
use cave_id::Id;
use cave_survey::{
    Adjusted, Extracted, Fixed, Leg, Params, Resolved, Vec3, adjust, extract, resolve,
};
use std::collections::{BTreeMap, HashMap};

pub(crate) struct Solved {
    pub ex: Extracted,
    pub resolved: Resolved,
    pub positions: BTreeMap<Id, Vec3>,
    pub labels: HashMap<Id, String>,
    /// Stations in resolution order.
    pub order: Vec<Id>,
}

/// The loop-closure adjustment of these legs, from a cache of the last one worked out. One redraw
/// asks several views (map, map lines, profile, sections, mesh) and each solved the whole network
/// again: on an 8,000-leg cave that was seconds per redraw. The answer depends only on the legs and
/// the fixed stations, so drawing edits keep the cache and any survey edit replaces it.
pub(crate) fn adjusted(legs: &[Leg], fixed: &[Fixed], resolved: &Resolved) -> Adjusted {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let v = |h: &mut std::collections::hash_map::DefaultHasher, p: Vec3| {
        p.x.to_bits().hash(h);
        p.y.to_bits().hash(h);
        p.z.to_bits().hash(h);
    };
    legs.len().hash(&mut h);
    for l in legs {
        (l.shot, l.from, l.to, l.no_adjust, l.azimuth_omitted).hash(&mut h);
        l.distance.to_bits().hash(&mut h);
        v(&mut h, l.offset);
    }
    fixed.len().hash(&mut h);
    for f in fixed {
        f.station.hash(&mut h);
        v(&mut h, f.pos);
    }
    let key = h.finish();
    thread_local! {
        static LAST: std::cell::RefCell<Option<(u64, Adjusted)>> = const { std::cell::RefCell::new(None) };
    }
    if let Some(hit) = LAST.with(|c| {
        c.borrow()
            .as_ref()
            .filter(|(k, _)| *k == key)
            .map(|(_, a)| a.clone())
    }) {
        return hit;
    }
    let a = adjust(legs, resolved, &Params::default());
    LAST.with(|c| *c.borrow_mut() = Some((key, a.clone())));
    a
}

pub(crate) fn solve(doc: &Document, use_adjustment: bool) -> Solved {
    let ex = extract(doc);
    let resolved = resolve(&ex.legs, &ex.fixed);
    let adj = use_adjustment
        .then(|| adjusted(&ex.legs, &ex.fixed, &resolved))
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
