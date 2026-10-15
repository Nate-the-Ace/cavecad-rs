//! The state of a pick in progress, and what finishing a pick does to it.

use crate::mode::Mode;
use crate::scene::{Acquired, Session};

/// What a finished pick came to.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PickOutcome {
    /// The point to use, or `None` when the pick only set something up (a track point, a base point, the first
    /// point of Mid Between 2 Points) and the tool keeps waiting.
    pub point: Option<[f64; 3]>,
    /// The session after the pick.
    pub session: Session,
}

/// The most things a hover may hold at once.
const MAX_ACQUIRED: usize = 8;

impl Session {
    /// A fresh session: nothing acquired, no override.
    pub fn new() -> Session {
        Session::default()
    }

    /// Acquires what the cursor hovered over. Hovering over one that is already acquired releases it.
    pub fn acquire(&mut self, a: Acquired) {
        if let Some(i) = self.acquired.iter().position(|x| same(x, &a)) {
            self.acquired.remove(i);
            return;
        }
        if self.acquired.len() >= MAX_ACQUIRED {
            self.acquired.remove(0);
        }
        self.acquired.push(a);
    }

    /// The next pick will search only `mode` (or, for From, Temporary Track Point and Mid Between 2 Points, set that
    /// up). `None` goes back to the running modes.
    pub fn one_pick(&mut self, mode: Option<Mode>) {
        self.override_mode = mode;
        if mode != Some(Mode::MidBetween) {
            self.m2p_first = None;
        }
    }

    /// Escape: drop everything a pick had built up (track points, acquired points, the override). The last point of
    /// the pick in progress stays: it belongs to the tool.
    pub fn cancel(&mut self) {
        let last = self.last_point;
        *self = Session {
            last_point: last,
            ..Session::default()
        };
    }

    /// A pick landed on `p`. Applies the override, and returns the point to use (if the pick finished one).
    pub fn pick(&self, p: [f64; 3]) -> PickOutcome {
        let mut s = self.clone();
        match self.override_mode {
            Some(Mode::TrackPoint) => {
                s.track_points.push(p);
                s.override_mode = None;
                PickOutcome {
                    point: None,
                    session: s,
                }
            }
            Some(Mode::From) => {
                s.from_base = Some(p);
                s.override_mode = None;
                PickOutcome {
                    point: None,
                    session: s,
                }
            }
            Some(Mode::MidBetween) => match self.m2p_first {
                None => {
                    s.m2p_first = Some(p);
                    PickOutcome {
                        point: None,
                        session: s,
                    }
                }
                Some(a) => finish(
                    s,
                    [
                        (a[0] + p[0]) / 2.0,
                        (a[1] + p[1]) / 2.0,
                        (a[2] + p[2]) / 2.0,
                    ],
                ),
            },
            _ => finish(s, p),
        }
    }
}

/// A pick produced `p`: transient state goes, the last point becomes it.
fn finish(mut s: Session, p: [f64; 3]) -> PickOutcome {
    s.override_mode = None;
    s.m2p_first = None;
    s.from_base = None;
    s.track_points.clear();
    s.acquired.clear();
    s.last_point = Some(p);
    PickOutcome {
        point: Some(p),
        session: s,
    }
}

fn same(a: &Acquired, b: &Acquired) -> bool {
    let near = |p: &[f64; 3], q: &[f64; 3]| {
        (p[0] - q[0]).abs() < 1e-9 && (p[1] - q[1]).abs() < 1e-9 && (p[2] - q[2]).abs() < 1e-9
    };
    match (a, b) {
        (Acquired::Extension { at: p, id: i, .. }, Acquired::Extension { at: q, id: j, .. }) => {
            i == j && near(p, q)
        }
        (Acquired::Parallel { dir: p, id: i }, Acquired::Parallel { dir: q, id: j }) => {
            i == j && near(p, q)
        }
        (Acquired::Track { at: p }, Acquired::Track { at: q }) => near(p, q),
        _ => false,
    }
}
