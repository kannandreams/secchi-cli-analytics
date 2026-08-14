//! The spool: append-only JSON Lines, one file per UTC day.
//!
//! This is the capture path's only I/O, and it is synchronous by design
//! (architecture decision 4): one open, one `write_all`, no locks, no
//! retries, no network. That construction — not a watchdog timer — is what
//! keeps capture within its latency budget. A background flush thread
//! would silently lose the last events of a process that exits right after
//! its final command, which is the common case for CLIs.
//!
//! Concurrent appends need no locking: each event is a single `O_APPEND`
//! write well under the 4 KiB pipe-atomicity threshold (the envelope has
//! no free-text fields, so line length is bounded by construction). The
//! documented worst case on exotic filesystems is one garbled line, which
//! readers skip.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::SpoolError;
use crate::event::Event;

/// Upper bound a serialized event line is expected to stay under; the
/// debug assertion in [`SpoolWriter::append`] guards the invariant.
pub const MAX_LINE_BYTES: usize = 4096;

/// Appends events to `<spool_dir>/<YYYY-MM-DD>.jsonl`.
#[derive(Debug, Clone)]
pub struct SpoolWriter {
    spool_dir: PathBuf,
}

impl SpoolWriter {
    #[must_use]
    pub fn new(spool_dir: PathBuf) -> Self {
        Self { spool_dir }
    }

    /// Append one event to the day file named by the event's own UTC
    /// timestamp, creating the directory and file as needed. Returns the
    /// file written, mainly for tests and diagnostics.
    pub fn append(&self, event: &Event) -> Result<PathBuf, SpoolError> {
        let mut line =
            serde_json::to_string(event).map_err(|source| SpoolError::Serialize { source })?;
        line.push('\n');
        debug_assert!(
            line.len() <= MAX_LINE_BYTES,
            "event line exceeds the atomic-append bound"
        );

        let path = self.day_file(event);
        std::fs::create_dir_all(&self.spool_dir).map_err(|source| SpoolError::Append {
            path: self.spool_dir.clone(),
            source,
        })?;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&path)
            .map_err(|source| SpoolError::Append {
                path: path.clone(),
                source,
            })?;
        file.write_all(line.as_bytes())
            .map_err(|source| SpoolError::Append {
                path: path.clone(),
                source,
            })?;
        Ok(path)
    }

    fn day_file(&self, event: &Event) -> PathBuf {
        self.spool_dir
            .join(format!("{}.jsonl", event.ts.strftime("%Y-%m-%d")))
    }

    #[must_use]
    pub fn spool_dir(&self) -> &Path {
        &self.spool_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{InstallId, SessionId};
    use uuid::Uuid;

    fn event_at(ts: &str) -> Event {
        Event::builder("myctl", vec!["run".into()])
            .os("linux")
            .arch("x86_64")
            .build(
                Uuid::now_v7(),
                ts.parse().unwrap(),
                InstallId::generate(),
                SessionId::generate(),
            )
    }

    #[test]
    fn appends_parseable_lines_to_the_day_file() {
        let dir = tempfile::tempdir().unwrap();
        let writer = SpoolWriter::new(dir.path().join("spool"));

        for _ in 0..3 {
            writer.append(&event_at("2026-08-14T12:00:00Z")).unwrap();
        }

        let contents =
            std::fs::read_to_string(dir.path().join("spool").join("2026-08-14.jsonl")).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 3);
        for line in lines {
            let parsed: Event = serde_json::from_str(line).unwrap();
            assert_eq!(parsed.event_name, "myctl.run.completed");
        }
    }

    #[test]
    fn day_file_follows_the_event_timestamp_in_utc() {
        let dir = tempfile::tempdir().unwrap();
        let writer = SpoolWriter::new(dir.path().to_owned());

        // 23:30 UTC and 00:30 UTC the next day land in different files,
        // regardless of the machine's local timezone.
        let first = writer.append(&event_at("2026-08-13T23:30:00Z")).unwrap();
        let second = writer.append(&event_at("2026-08-14T00:30:00Z")).unwrap();
        assert!(first.ends_with("2026-08-13.jsonl"));
        assert!(second.ends_with("2026-08-14.jsonl"));
    }

    #[cfg(unix)]
    #[test]
    fn unwritable_directory_returns_an_error() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let spool = dir.path().join("spool");
        std::fs::create_dir(&spool).unwrap();
        std::fs::set_permissions(&spool, std::fs::Permissions::from_mode(0o000)).unwrap();

        let writer = SpoolWriter::new(spool.clone());
        let result = writer.append(&event_at("2026-08-14T12:00:00Z"));
        assert!(result.is_err());

        // Restore so the tempdir can be cleaned up.
        std::fs::set_permissions(&spool, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}
