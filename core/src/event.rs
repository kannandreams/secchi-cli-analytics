//! The event envelope: schema version 1.
//!
//! This schema is a public contract shared by every shim in every language.
//! It evolves additively — new optional fields may be added at any time,
//! but renaming or repurposing a field requires a `SCHEMA_VERSION` bump and
//! a deprecation window, because local spool and DuckDB files can be
//! arbitrarily old.
//!
//! Two fields are deliberately absent and must never be added: raw argv and
//! free-text error messages. Both are common accidental carriers of secrets
//! and PII; excluding them at the type level means they cannot reach disk.

use std::collections::BTreeMap;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::redact;

/// Version of the event envelope. Bumped only for breaking changes.
pub const SCHEMA_VERSION: u32 = 1;

/// Version of this SDK, recorded on every event it emits.
pub const SDK_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Column names of the `raw_events` table, in envelope order.
///
/// This is the single source of truth binding the Rust struct, the golden
/// serialization test, and the DuckDB `read_json` column map. A field added
/// to [`Event`] without updating this list fails the schema test.
pub const RAW_EVENT_COLUMNS: &[&str] = &[
    "event_id",
    "schema_version",
    "event_name",
    "ts",
    "install_id",
    "session_id",
    "cli_name",
    "cli_version",
    "command_path",
    "flag_names",
    "flag_values",
    "duration_ms",
    "exit_code",
    "error_class",
    "actor",
    "agent_name",
    "agent_session_id",
    "ci",
    "interactive",
    "os",
    "arch",
    "sdk_version",
];

/// Anonymous per-machine identity: a random UUIDv4 generated on first run.
///
/// Never derived from a username, email, hostname, or hardware id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InstallId(Uuid);

impl InstallId {
    #[must_use]
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    #[must_use]
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl std::fmt::Display for InstallId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for InstallId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Per-process-invocation identity: a UUIDv7, so it sorts by start time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(Uuid);

impl SessionId {
    #[must_use]
    pub fn generate() -> Self {
        Self(Uuid::now_v7())
    }

    #[must_use]
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Who drove the invocation. Populated from explicit markers only — never
/// guessed from heuristics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Actor {
    #[default]
    Human,
    Agent,
}

/// Lifecycle phase encoded into `event_name`. Most shims emit only
/// `Completed`/`Failed`; `Started` exists for abandonment tracking on
/// long-running commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Started,
    Completed,
    Failed,
}

impl Phase {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Started => "started",
            Phase::Completed => "completed",
            Phase::Failed => "failed",
        }
    }
}

/// One CLI invocation event. Construct via [`Event::builder`] — the builder
/// is the only path, and it redacts every flag value before it can land in
/// the struct.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub event_id: Uuid,
    pub schema_version: u32,
    pub event_name: String,
    pub ts: Timestamp,
    pub install_id: InstallId,
    pub session_id: SessionId,
    pub cli_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli_version: Option<String>,
    pub command_path: Vec<String>,
    pub flag_names: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub flag_values: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_class: Option<String>,
    pub actor: Actor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
    pub ci: bool,
    pub interactive: bool,
    pub os: String,
    pub arch: String,
    pub sdk_version: String,
}

impl Event {
    /// Start building an event for one resolved command invocation.
    pub fn builder(cli_name: impl Into<String>, command_path: Vec<String>) -> EventBuilder {
        EventBuilder {
            cli_name: cli_name.into(),
            command_path,
            phase: Phase::Completed,
            cli_version: None,
            flag_names: Vec::new(),
            flag_values: BTreeMap::new(),
            duration_ms: None,
            exit_code: None,
            error_class: None,
            actor: Actor::Human,
            agent_name: None,
            agent_session_id: None,
            ci: false,
            interactive: true,
            os: std::env::consts::OS.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
            sdk_version: SDK_VERSION.to_owned(),
        }
    }
}

/// Builder for [`Event`]. Identity and time are injected at [`build`]
/// (deterministic tests use fixed values; production uses
/// [`build_now`]).
///
/// [`build`]: EventBuilder::build
/// [`build_now`]: EventBuilder::build_now
#[derive(Debug, Clone)]
pub struct EventBuilder {
    cli_name: String,
    command_path: Vec<String>,
    phase: Phase,
    cli_version: Option<String>,
    flag_names: Vec<String>,
    flag_values: BTreeMap<String, String>,
    duration_ms: Option<u64>,
    exit_code: Option<i32>,
    error_class: Option<String>,
    actor: Actor,
    agent_name: Option<String>,
    agent_session_id: Option<String>,
    ci: bool,
    interactive: bool,
    os: String,
    arch: String,
    sdk_version: String,
}

impl EventBuilder {
    #[must_use]
    pub fn phase(mut self, phase: Phase) -> Self {
        self.phase = phase;
        self
    }

    #[must_use]
    pub fn cli_version(mut self, version: impl Into<String>) -> Self {
        self.cli_version = Some(version.into());
        self
    }

    #[must_use]
    pub fn flag_names(mut self, names: Vec<String>) -> Self {
        self.flag_names = names;
        self
    }

    /// Record the value of an allowlisted flag. The value passes through the
    /// redaction engine here, inside the builder — there is no way to place
    /// an unredacted value into an [`Event`].
    #[must_use]
    pub fn flag_value(mut self, name: impl Into<String>, raw_value: &str) -> Self {
        self.flag_values
            .insert(name.into(), redact::redact(raw_value).into_owned());
        self
    }

    #[must_use]
    pub fn duration_ms(mut self, ms: u64) -> Self {
        self.duration_ms = Some(ms);
        self
    }

    #[must_use]
    pub fn exit_code(mut self, code: i32) -> Self {
        self.exit_code = Some(code);
        self
    }

    /// Record the error *type name* for a failed invocation. Never pass an
    /// error message here — messages are free text and excluded from the
    /// schema.
    #[must_use]
    pub fn error_class(mut self, class: impl Into<String>) -> Self {
        self.error_class = Some(class.into());
        self
    }

    #[must_use]
    pub fn actor(mut self, actor: Actor) -> Self {
        self.actor = actor;
        self
    }

    #[must_use]
    pub fn agent_name(mut self, name: impl Into<String>) -> Self {
        self.agent_name = Some(name.into());
        self
    }

    #[must_use]
    pub fn agent_session_id(mut self, id: impl Into<String>) -> Self {
        self.agent_session_id = Some(id.into());
        self
    }

    #[must_use]
    pub fn ci(mut self, ci: bool) -> Self {
        self.ci = ci;
        self
    }

    #[must_use]
    pub fn interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
        self
    }

    /// Override the detected operating system (tests need determinism).
    #[must_use]
    pub fn os(mut self, os: impl Into<String>) -> Self {
        self.os = os.into();
        self
    }

    /// Override the detected architecture (tests need determinism).
    #[must_use]
    pub fn arch(mut self, arch: impl Into<String>) -> Self {
        self.arch = arch.into();
        self
    }

    #[must_use]
    pub fn sdk_version(mut self, version: impl Into<String>) -> Self {
        self.sdk_version = version.into();
        self
    }

    /// Build with explicit identity and time. Test code passes fixed values
    /// so assertions never depend on the clock.
    #[must_use]
    pub fn build(
        self,
        event_id: Uuid,
        ts: Timestamp,
        install_id: InstallId,
        session_id: SessionId,
    ) -> Event {
        let event_name = event_name(&self.cli_name, &self.command_path, self.phase);
        Event {
            event_id,
            schema_version: SCHEMA_VERSION,
            event_name,
            ts,
            install_id,
            session_id,
            cli_name: self.cli_name,
            cli_version: self.cli_version,
            command_path: self.command_path,
            flag_names: self.flag_names,
            flag_values: self.flag_values,
            duration_ms: self.duration_ms,
            exit_code: self.exit_code,
            error_class: self.error_class,
            actor: self.actor,
            agent_name: self.agent_name,
            agent_session_id: self.agent_session_id,
            ci: self.ci,
            interactive: self.interactive,
            os: self.os,
            arch: self.arch,
            sdk_version: self.sdk_version,
        }
    }

    /// Build with a fresh UUIDv7 and the current time — the production path.
    #[must_use]
    pub fn build_now(self, install_id: InstallId, session_id: SessionId) -> Event {
        self.build(Uuid::now_v7(), Timestamp::now(), install_id, session_id)
    }
}

/// `<namespace>.<command_path>.<phase>`, e.g. `myctl.project.add.completed`.
/// A root invocation with no subcommand yields `<namespace>.<phase>`.
fn event_name(cli_name: &str, command_path: &[String], phase: Phase) -> String {
    let mut name = String::from(cli_name);
    for segment in command_path {
        name.push('.');
        name.push_str(segment);
    }
    name.push('.');
    name.push_str(phase.as_str());
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_ids() -> (Uuid, Timestamp, InstallId, SessionId) {
        (
            Uuid::parse_str("0198aaaa-0000-7000-8000-000000000001").unwrap(),
            "2026-08-14T12:00:00Z".parse().unwrap(),
            InstallId::from_uuid(Uuid::parse_str("11111111-2222-4333-8444-555555555555").unwrap()),
            SessionId::from_uuid(Uuid::parse_str("0198aaaa-0000-7000-8000-000000000002").unwrap()),
        )
    }

    #[test]
    fn event_name_joins_namespace_path_and_phase() {
        let (id, ts, install, session) = fixed_ids();
        let event = Event::builder("myctl", vec!["project".into(), "add".into()])
            .phase(Phase::Failed)
            .build(id, ts, install, session);
        assert_eq!(event.event_name, "myctl.project.add.failed");
    }

    #[test]
    fn event_name_for_root_invocation_omits_path() {
        let (id, ts, install, session) = fixed_ids();
        let event = Event::builder("myctl", vec![]).build(id, ts, install, session);
        assert_eq!(event.event_name, "myctl.completed");
    }

    #[test]
    fn flag_values_are_redacted_by_the_builder() {
        let (id, ts, install, session) = fixed_ids();
        let event = Event::builder("myctl", vec!["login".into()])
            .flag_value("token", "ghp_0123456789abcdef0123456789abcdef0123")
            .flag_value("format", "json")
            .build(id, ts, install, session);
        assert_eq!(
            event.flag_values["token"], "[REDACTED:github-token]",
            "a secret must never survive the builder"
        );
        assert_eq!(event.flag_values["format"], "json");
    }

    #[test]
    fn missing_optionals_are_omitted_from_json() {
        let (id, ts, install, session) = fixed_ids();
        let event = Event::builder("myctl", vec![]).build(id, ts, install, session);
        let json = serde_json::to_value(&event).unwrap();
        let object = json.as_object().unwrap();
        for absent in ["cli_version", "duration_ms", "error_class", "flag_values"] {
            assert!(!object.contains_key(absent), "{absent} should be omitted");
        }
    }

    #[test]
    fn deserializes_with_unknown_fields_and_missing_optionals() {
        // Consumers must treat unknown fields as ignorable and missing
        // optionals as null — this is the additive-evolution contract.
        let json = r#"{
            "event_id": "0198aaaa-0000-7000-8000-000000000001",
            "schema_version": 1,
            "event_name": "myctl.completed",
            "ts": "2026-08-14T12:00:00Z",
            "install_id": "11111111-2222-4333-8444-555555555555",
            "session_id": "0198aaaa-0000-7000-8000-000000000002",
            "cli_name": "myctl",
            "command_path": [],
            "flag_names": [],
            "actor": "human",
            "ci": false,
            "interactive": true,
            "os": "macos",
            "arch": "aarch64",
            "sdk_version": "0.1.0",
            "some_future_field": "ignored"
        }"#;
        let event: Event = serde_json::from_str(json).unwrap();
        assert_eq!(event.cli_version, None);
        assert!(event.flag_values.is_empty());
    }
}
