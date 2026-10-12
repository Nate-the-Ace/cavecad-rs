//! The bounded log store the console panel reads from.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// How loud an event is. The console filter hides everything below its chosen level.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Level {
    /// Developer detail, hidden by default.
    Debug,
    /// Extra detail for the curious.
    Detail,
    /// Normal progress the user should see.
    Info,
    /// Something odd that did not stop the work.
    Warn,
    /// Something failed; stays visible until cleared.
    Error,
}

/// One line in the console.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LogEvent {
    /// Increases by one per event for the life of the buffer; survives `clear`.
    pub seq: u64,
    /// Severity.
    pub level: Level,
    /// Which part of the app spoke (`import`, `adjust`, `save`, ...).
    pub source: String,
    /// Plain-language text, already redacted.
    pub message: String,
}

struct Inner {
    events: VecDeque<LogEvent>,
    capacity: usize,
    next_seq: u64,
}

/// A shared, fixed-size ring of events. Cloning shares the same buffer.
#[derive(Clone)]
pub struct LogBuffer {
    inner: Arc<Mutex<Inner>>,
}

impl LogBuffer {
    /// A buffer that keeps the most recent `capacity` events.
    pub fn new(capacity: usize) -> Self {
        LogBuffer {
            inner: Arc::new(Mutex::new(Inner {
                events: VecDeque::with_capacity(capacity.min(1024)),
                capacity: capacity.max(1),
                next_seq: 0,
            })),
        }
    }

    /// Appends an event, dropping the oldest if full, and returns its sequence number.
    pub fn push(&self, level: Level, source: &str, message: String) -> u64 {
        let mut g = self.inner.lock().expect("log lock");
        let seq = g.next_seq;
        g.next_seq += 1;
        if g.events.len() == g.capacity {
            g.events.pop_front();
        }
        g.events.push_back(LogEvent {
            seq,
            level,
            source: source.to_string(),
            message,
        });
        seq
    }

    /// Every kept event with `seq >= since`, oldest first. The panel passes the last seq it saw
    /// plus one, so each refresh only copies what is new.
    pub fn snapshot(&self, since: u64) -> Vec<LogEvent> {
        let g = self.inner.lock().expect("log lock");
        g.events
            .iter()
            .filter(|e| e.seq >= since)
            .cloned()
            .collect()
    }

    /// Empties the console. Sequence numbers keep counting.
    pub fn clear(&self) {
        self.inner.lock().expect("log lock").events.clear();
    }

    /// Number of events currently kept.
    pub fn len(&self) -> usize {
        self.inner.lock().expect("log lock").events.len()
    }

    /// True when nothing is kept.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// One text line for the per-cave log file: `LEVEL source: message`. No timestamp, because this
/// crate has no clock; the file writer prepends one.
pub fn format_line(e: &LogEvent) -> String {
    let lvl = match e.level {
        Level::Debug => "DEBUG",
        Level::Detail => "DETAIL",
        Level::Info => "INFO",
        Level::Warn => "WARN",
        Level::Error => "ERROR",
    };
    format!("{lvl} {}: {}", e.source, e.message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_drops_oldest() {
        let b = LogBuffer::new(3);
        for i in 0..5 {
            b.push(Level::Info, "t", format!("m{i}"));
        }
        let all = b.snapshot(0);
        assert_eq!(all.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![2, 3, 4]);
    }

    #[test]
    fn snapshot_since_returns_only_new() {
        let b = LogBuffer::new(10);
        b.push(Level::Info, "t", "a".into());
        let next = b.push(Level::Info, "t", "b".into());
        assert_eq!(b.snapshot(next).len(), 1);
    }

    #[test]
    fn clear_keeps_counting() {
        let b = LogBuffer::new(10);
        b.push(Level::Info, "t", "a".into());
        b.clear();
        assert!(b.is_empty());
        assert_eq!(b.push(Level::Info, "t", "b".into()), 1);
    }

    #[test]
    fn levels_order_and_line_format() {
        assert!(Level::Error > Level::Warn && Level::Info > Level::Detail);
        let e = LogEvent {
            seq: 0,
            level: Level::Warn,
            source: "save".into(),
            message: "slow".into(),
        };
        assert_eq!(format_line(&e), "WARN save: slow");
    }
}
