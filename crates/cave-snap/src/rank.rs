//! Choosing among candidates.

use crate::scene::Candidate;
use std::cmp::Ordering;

fn order(a: &Candidate, b: &Candidate) -> Ordering {
    a.mode
        .tier()
        .cmp(&b.mode.tier())
        .then(a.dist.total_cmp(&b.dist))
        .then(a.mode.cmp(&b.mode))
        .then_with(|| {
            a.source
                .as_ref()
                .map(|s| (&s.id, s.seg))
                .cmp(&b.source.as_ref().map(|s| (&s.id, s.seg)))
        })
        .then(a.world[0].total_cmp(&b.world[0]))
        .then(a.world[1].total_cmp(&b.world[1]))
        .then(a.world[2].total_cmp(&b.world[2]))
}

/// The candidates within `aperture` pixels, best first, without repeats of the same place by the same mode.
pub(crate) fn ranked(mut all: Vec<Candidate>, aperture: f64) -> Vec<Candidate> {
    all.retain(|c| c.dist <= aperture && c.screen.iter().all(|v| v.is_finite()));
    all.sort_by(order);
    let mut out: Vec<Candidate> = Vec::new();
    for c in all {
        let repeat = out.iter().any(|o| {
            o.mode == c.mode
                && (o.screen[0] - c.screen[0]).abs() < 0.5
                && (o.screen[1] - c.screen[1]).abs() < 0.5
        });
        if !repeat {
            out.push(c);
        }
    }
    out
}
