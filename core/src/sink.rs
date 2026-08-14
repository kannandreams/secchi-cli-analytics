//! Event sinks: where a built event goes.
//!
//! The trait is the seam between capture and delivery. Its contract is
//! drop-not-crash: `record` is infallible at the API level, and an
//! implementation that cannot deliver an event discards it. Dropping is
//! the correct failure mode for instrumentation — a host CLI must never
//! slow down, crash, or print because telemetry had a bad day.
//!
//! `SpoolSink` is the production sink; `MemorySink` is the testing
//! transport every shim exposes so integrators can assert on emitted
//! events without touching disk.

use std::sync::{Arc, Mutex};

use crate::event::Event;
use crate::spool::SpoolWriter;

/// Destination for built events. Implementations swallow their own errors.
pub trait EventSink: Send + Sync {
    fn record(&self, event: &Event);
}

impl<S: EventSink + ?Sized> EventSink for Arc<S> {
    fn record(&self, event: &Event) {
        (**self).record(event);
    }
}

/// Production sink: synchronous bounded append to the JSONL spool. Any
/// spool error is discarded here, at the boundary — never surfaced into
/// the host CLI.
#[derive(Debug, Clone)]
pub struct SpoolSink {
    writer: SpoolWriter,
}

impl SpoolSink {
    #[must_use]
    pub fn new(spool_dir: std::path::PathBuf) -> Self {
        Self {
            writer: SpoolWriter::new(spool_dir),
        }
    }
}

impl EventSink for SpoolSink {
    fn record(&self, event: &Event) {
        let _ = self.writer.append(event);
    }
}

/// Testing sink: collects events in memory. Clone the `Arc` into a shim,
/// keep one handle, and assert on [`MemorySink::events`] afterwards.
#[derive(Debug, Default)]
pub struct MemorySink {
    events: Mutex<Vec<Event>>,
}

impl MemorySink {
    #[must_use]
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Snapshot of everything recorded so far.
    #[must_use]
    pub fn events(&self) -> Vec<Event> {
        self.events.lock().expect("sink poisoned").clone()
    }
}

impl EventSink for MemorySink {
    fn record(&self, event: &Event) {
        self.events
            .lock()
            .expect("sink poisoned")
            .push(event.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{InstallId, SessionId};
    use uuid::Uuid;

    fn sample_event() -> Event {
        Event::builder("myctl", vec![]).build(
            Uuid::now_v7(),
            "2026-08-14T12:00:00Z".parse().unwrap(),
            InstallId::generate(),
            SessionId::generate(),
        )
    }

    #[test]
    fn memory_sink_collects_events() {
        let sink = MemorySink::shared();
        sink.record(&sample_event());
        sink.record(&sample_event());
        assert_eq!(sink.events().len(), 2);
    }

    #[test]
    fn spool_sink_swallows_write_failures() {
        // A file where the spool directory should be makes every append
        // fail; record must neither panic nor report.
        let dir = tempfile::tempdir().unwrap();
        let blocked = dir.path().join("spool");
        std::fs::write(&blocked, "occupied").unwrap();

        let sink = SpoolSink::new(blocked);
        sink.record(&sample_event());
    }

    #[test]
    fn spool_sink_writes_through_to_the_spool() {
        let dir = tempfile::tempdir().unwrap();
        let sink = SpoolSink::new(dir.path().join("spool"));
        sink.record(&sample_event());
        let file = dir.path().join("spool").join("2026-08-14.jsonl");
        assert!(file.exists());
    }
}
