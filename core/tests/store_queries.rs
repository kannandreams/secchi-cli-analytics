//! Store integration tests: fixture events written through the real spool
//! writer, queried and compacted in a tempdir. Fixed timestamps and ids
//! throughout; `today` is always injected.

#![cfg(feature = "store")]

use jiff::civil::Date;
use secchi_analytics_core::config::Config;
use secchi_analytics_core::event::{Event, InstallId, Phase, SessionId};
use secchi_analytics_core::spool::SpoolWriter;
use secchi_analytics_core::store::Store;
use uuid::Uuid;

const TODAY: Date = Date::constant(2026, 8, 14);

fn config(dir: &tempfile::TempDir) -> Config {
    Config {
        enabled: true,
        data_dir: dir.path().join("analytics"),
        retention_days: 90,
    }
}

fn event(seq: u128, ts: &str, path: &[&str], exit_code: i32) -> Event {
    let phase = if exit_code == 0 {
        Phase::Completed
    } else {
        Phase::Failed
    };
    Event::builder("myctl", path.iter().map(ToString::to_string).collect())
        .phase(phase)
        .exit_code(exit_code)
        .duration_ms(100)
        .os("linux")
        .arch("x86_64")
        .build(
            Uuid::from_u128(seq),
            ts.parse().unwrap(),
            InstallId::from_uuid(Uuid::from_u128(1)),
            SessionId::from_uuid(Uuid::from_u128(2)),
        )
}

/// Two closed days plus today: 3 × `run`, 1 failing `sync`, 1 root event.
fn seed(config: &Config) {
    let writer = SpoolWriter::new(config.spool_dir());
    writer
        .append(&event(10, "2026-08-12T10:00:00Z", &["run"], 0))
        .unwrap();
    writer
        .append(&event(11, "2026-08-13T10:00:00Z", &["run"], 0))
        .unwrap();
    writer
        .append(&event(12, "2026-08-13T11:00:00Z", &["sync"], 1))
        .unwrap();
    writer
        .append(&event(13, "2026-08-14T09:00:00Z", &["run"], 0))
        .unwrap();
    writer
        .append(&event(14, "2026-08-14T10:00:00Z", &[], 0))
        .unwrap();
}

fn cutoff(days_back: i64) -> jiff::Timestamp {
    let base: jiff::Timestamp = "2026-08-14T12:00:00Z".parse().unwrap();
    base - jiff::Span::new().hours(days_back * 24)
}

#[test]
fn stats_read_the_live_spool_before_any_compaction() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(&dir);
    seed(&config);

    let store = Store::open(&config).unwrap();
    let report = store.stats(cutoff(30), None, "30d").unwrap();

    assert_eq!(report.total_invocations, 5);
    assert_eq!(report.total_failures, 1);
    let run = report.commands.iter().find(|c| c.command == "run").unwrap();
    assert_eq!(run.invocations, 3);
    assert_eq!(run.failures, 0);
    assert_eq!(run.avg_duration_ms, Some(100.0));
    let sync = report
        .commands
        .iter()
        .find(|c| c.command == "sync")
        .unwrap();
    assert_eq!(sync.failures, 1);
    assert!(report.commands.iter().any(|c| c.command == "(root)"));
}

#[test]
fn stats_respect_the_cutoff_and_command_filter() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(&dir);
    seed(&config);

    let store = Store::open(&config).unwrap();

    // Only today's events fall inside a 1-day window.
    let recent = store.stats(cutoff(1), None, "1d").unwrap();
    assert_eq!(recent.total_invocations, 2);

    let filtered = store.stats(cutoff(30), Some("run"), "30d").unwrap();
    assert_eq!(filtered.total_invocations, 3);
    assert!(filtered.commands.iter().all(|c| c.command == "run"));
}

#[test]
fn tail_returns_newest_first_with_full_envelopes() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(&dir);
    seed(&config);

    let store = Store::open(&config).unwrap();
    let rows = store.tail(3).unwrap();

    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].event_name, "myctl.completed");
    assert_eq!(rows[1].event_name, "myctl.run.completed");
    assert_eq!(rows[0].ts, "2026-08-14 10:00:00");
    let envelope: serde_json::Value = serde_json::from_str(&rows[0].json).unwrap();
    assert_eq!(envelope["cli_name"], "myctl");
    assert_eq!(envelope["schema_version"], 1);
}

#[test]
fn compact_rotates_closed_days_and_leaves_today() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(&dir);
    seed(&config);

    let store = Store::open(&config).unwrap();
    let report = store.compact(TODAY, config.retention_days).unwrap();

    let days: Vec<&str> = report.compacted.iter().map(|(d, _)| d.as_str()).collect();
    assert_eq!(days, ["2026-08-12", "2026-08-13"]);
    assert_eq!(report.compacted[1].1, 2, "two events on the 13th");

    assert!(config.archive_dir().join("2026-08-12.parquet").exists());
    assert!(!config.spool_dir().join("2026-08-12.jsonl").exists());
    assert!(
        config.spool_dir().join("2026-08-14.jsonl").exists(),
        "today's file is still being written to"
    );
    assert_eq!(store.raw_event_count().unwrap(), 3);

    // The unified view sees exactly the same data as before.
    let stats = store.stats(cutoff(30), None, "30d").unwrap();
    assert_eq!(stats.total_invocations, 5);
}

#[test]
fn compact_is_idempotent_and_survives_interruption() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(&dir);
    seed(&config);

    let store = Store::open(&config).unwrap();
    store.compact(TODAY, config.retention_days).unwrap();

    // Simulate a crash after upsert but before spool deletion: put the
    // already-compacted day back.
    let writer = SpoolWriter::new(config.spool_dir());
    writer
        .append(&event(11, "2026-08-13T10:00:00Z", &["run"], 0))
        .unwrap();
    writer
        .append(&event(12, "2026-08-13T11:00:00Z", &["sync"], 1))
        .unwrap();

    // Queries must not double count while the duplicate spool file exists…
    let stats = store.stats(cutoff(30), None, "30d").unwrap();
    assert_eq!(stats.total_invocations, 5);

    // …and a re-run upserts nothing new.
    let report = store.compact(TODAY, config.retention_days).unwrap();
    let redone = report
        .compacted
        .iter()
        .find(|(d, _)| d == "2026-08-13")
        .unwrap();
    assert_eq!(redone.1, 0, "anti-join skips already-present events");
    assert_eq!(store.raw_event_count().unwrap(), 3);
}

#[test]
fn retention_expires_old_rows_and_archives() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(&dir);
    let writer = SpoolWriter::new(config.spool_dir());
    writer
        .append(&event(20, "2026-01-05T10:00:00Z", &["old"], 0))
        .unwrap();
    writer
        .append(&event(21, "2026-08-13T10:00:00Z", &["new"], 0))
        .unwrap();

    let store = Store::open(&config).unwrap();
    let report = store.compact(TODAY, 90).unwrap();

    assert_eq!(report.expired_rows, 1, "January is past a 90-day horizon");
    assert_eq!(report.expired_archives, ["2026-01-05"]);
    assert!(!config.archive_dir().join("2026-01-05.parquet").exists());
    assert_eq!(store.raw_event_count().unwrap(), 1);
}
