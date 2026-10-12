//! Bridge from `tracing` events into a [`LogBuffer`].

use crate::{Level, LogBuffer, redact};
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

/// A `tracing` layer that copies each event into a [`LogBuffer`].
pub struct BufferLayer {
    buffer: LogBuffer,
}

impl BufferLayer {
    /// Feeds `buffer` from whatever subscriber this layer is installed on.
    pub fn new(buffer: LogBuffer) -> Self {
        BufferLayer { buffer }
    }
}

#[derive(Default)]
struct Fields {
    message: String,
    source: Option<String>,
}

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "message" => self.message = value.to_string(),
            "source" => self.source = Some(value.to_string()),
            _ => {}
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
        }
    }
}

/// `cave_adjust::loops` -> `adjust`: the crate name without its `cave_` prefix.
fn default_source(target: &str) -> String {
    let krate = target.split("::").next().unwrap_or("app");
    krate.strip_prefix("cave_").unwrap_or(krate).to_string()
}

fn map_level(l: &tracing::Level) -> Level {
    match *l {
        tracing::Level::ERROR => Level::Error,
        tracing::Level::WARN => Level::Warn,
        tracing::Level::INFO => Level::Info,
        tracing::Level::DEBUG => Level::Detail,
        tracing::Level::TRACE => Level::Debug,
    }
}

impl<S: Subscriber> Layer<S> for BufferLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut f = Fields::default();
        event.record(&mut f);
        let source = f
            .source
            .unwrap_or_else(|| default_source(event.metadata().target()));
        self.buffer.push(
            map_level(event.metadata().level()),
            &source,
            redact(&f.message),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::layer::SubscriberExt;

    fn capture(f: impl FnOnce()) -> Vec<crate::LogEvent> {
        let buf = LogBuffer::new(50);
        let sub = tracing_subscriber::registry().with(BufferLayer::new(buf.clone()));
        tracing::subscriber::with_default(sub, f);
        buf.snapshot(0)
    }

    #[test]
    fn captures_level_source_message() {
        let ev = capture(|| tracing::info!(source = "adjust", "loop 3 of 12"));
        assert_eq!(ev.len(), 1);
        assert_eq!(
            (ev[0].level, ev[0].source.as_str(), ev[0].message.as_str()),
            (Level::Info, "adjust", "loop 3 of 12")
        );
    }

    #[test]
    fn defaults_source_to_module_name() {
        let ev = capture(|| tracing::warn!("slow"));
        assert_eq!(ev[0].source, "log");
        assert_eq!(ev[0].level, Level::Warn);
    }

    #[test]
    fn messages_are_redacted() {
        let ev = capture(|| tracing::info!(source = "import", "entrance 36.12345, -86.54321"));
        assert_eq!(ev[0].message, "entrance [location hidden]");
    }
}
