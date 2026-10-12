//! Logical time: who edited, and in what order, without a wall clock.

use serde::{Deserialize, Serialize};

/// One person's (or one device's) identity as an editor. Chosen by the app, unique per editor.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub struct ActorId(pub u64);

/// A point in logical time. Ordered by counter first, then actor, so two edits made at the same
/// counter by different people still have one agreed winner.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub struct Stamp {
    /// Lamport counter: larger means later.
    pub counter: u64,
    /// Tie-breaker.
    pub actor: ActorId,
}

impl Stamp {
    /// Older than every real stamp; the starting value of an untouched register.
    pub const ZERO: Stamp = Stamp {
        counter: 0,
        actor: ActorId(0),
    };
}

/// Hands out stamps for one editor and moves forward when it sees other editors' stamps.
#[derive(Clone, Debug)]
pub struct Clock {
    actor: ActorId,
    counter: u64,
}

impl Clock {
    /// A fresh clock for `actor`.
    pub fn new(actor: ActorId) -> Self {
        Clock { actor, counter: 0 }
    }

    /// The editor this clock stamps for.
    pub fn actor(&self) -> ActorId {
        self.actor
    }

    /// Stamps as a different editor from now on (the counter keeps going).
    pub fn set_actor(&mut self, actor: ActorId) {
        self.actor = actor;
    }

    /// The next stamp for a local edit.
    pub fn tick(&mut self) -> Stamp {
        self.counter += 1;
        Stamp {
            counter: self.counter,
            actor: self.actor,
        }
    }

    /// Moves past a stamp seen from elsewhere, so later local edits sort after it.
    pub fn observe(&mut self, s: Stamp) {
        self.counter = self.counter.max(s.counter);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_order_by_counter_then_actor() {
        let a = Stamp {
            counter: 2,
            actor: ActorId(1),
        };
        let b = Stamp {
            counter: 2,
            actor: ActorId(2),
        };
        let c = Stamp {
            counter: 3,
            actor: ActorId(0),
        };
        assert!(a < b && b < c && Stamp::ZERO < a);
    }

    #[test]
    fn observe_moves_clock_forward_only() {
        let mut k = Clock::new(ActorId(1));
        k.observe(Stamp {
            counter: 10,
            actor: ActorId(2),
        });
        assert_eq!(k.tick().counter, 11);
        k.observe(Stamp {
            counter: 3,
            actor: ActorId(2),
        });
        assert_eq!(k.tick().counter, 12);
    }
}
