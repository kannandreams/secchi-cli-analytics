//! Schema-freeze test for event envelope v1.
//!
//! The serialized form of a fully populated event is compared byte-for-byte
//! against a committed golden file. If this test fails, the schema changed.
//! Do not "fix" it by regenerating the golden file: additive optional
//! fields require updating `RAW_EVENT_COLUMNS` and the golden together in a
//! reviewed change, and anything else requires a `SCHEMA_VERSION` decision.

use secchi_analytics_core::event::{Actor, Event, InstallId, Phase, RAW_EVENT_COLUMNS, SessionId};
use uuid::Uuid;

fn golden_event() -> Event {
    Event::builder("myctl", vec!["project".into(), "add".into()])
        .phase(Phase::Failed)
        .cli_version("2.3.1")
        .flag_names(vec!["format".into(), "verbose".into()])
        .flag_value("format", "json")
        .duration_ms(142)
        .exit_code(1)
        .error_class("ConfigError")
        .actor(Actor::Agent)
        .agent_name("claude-code")
        .agent_session_id("sess-42")
        .ci(true)
        .interactive(false)
        .os("macos")
        .arch("aarch64")
        .sdk_version("0.1.0")
        .build(
            Uuid::parse_str("0198aaaa-0000-7000-8000-000000000001").unwrap(),
            "2026-08-14T12:00:00Z".parse().unwrap(),
            InstallId::from_uuid(Uuid::parse_str("11111111-2222-4333-8444-555555555555").unwrap()),
            SessionId::from_uuid(Uuid::parse_str("0198aaaa-0000-7000-8000-000000000002").unwrap()),
        )
}

#[test]
fn serialization_matches_golden_file() {
    let serialized = serde_json::to_string(&golden_event()).unwrap();
    let golden = include_str!("golden/event_v1.json").trim_end();
    assert_eq!(serialized, golden, "event schema v1 is frozen");
}

#[test]
fn round_trips_through_json() {
    let event = golden_event();
    let json = serde_json::to_string(&event).unwrap();
    let back: Event = serde_json::from_str(&json).unwrap();
    assert_eq!(back, event);
}

#[test]
fn serialized_keys_match_the_raw_events_columns() {
    let value = serde_json::to_value(golden_event()).unwrap();
    let mut keys: Vec<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    let mut columns: Vec<&str> = RAW_EVENT_COLUMNS.to_vec();
    columns.sort_unstable();
    assert_eq!(keys, columns, "struct fields and DDL columns diverged");
}
