//! Instrumentation shim for clap-based CLIs.
//!
//! One call at startup, one at exit:
//!
//! ```no_run
//! use secchi_analytics_clap::Analytics;
//!
//! let analytics = Analytics::builder("myctl")
//!     .cli_version(env!("CARGO_PKG_VERSION"))
//!     .allow_values(["format"])
//!     .start();
//!
//! let matches = clap::Command::new("myctl").get_matches();
//! // ... dispatch and run the command ...
//! let exit_code = 0;
//!
//! analytics.record_exit(&matches, exit_code, None);
//! std::process::exit(exit_code);
//! ```
//!
//! What gets recorded: the resolved subcommand path, the names of flags the
//! user actually passed on the command line (defaulted and env-sourced
//! arguments are excluded), duration, exit code, and — for failures — an
//! error *type name*. Values are recorded only for allowlisted flags, and
//! even those pass the core redaction engine first. Raw argv never leaves
//! the process.
//!
//! Every public entry point is infallible. Configuration problems, missing
//! directories, and internal bugs all degrade to dropped events; an
//! instrumentation failure is never allowed to become a host CLI failure.

use std::collections::BTreeSet;
use std::panic::AssertUnwindSafe;
use std::time::Instant;

use clap::ArgMatches;
use clap::parser::ValueSource;
use secchi_analytics_core::config::Config;
use secchi_analytics_core::context::{self, ExecutionContext};
use secchi_analytics_core::event::{Actor, Event, InstallId, Phase, SessionId};
use secchi_analytics_core::install;
use secchi_analytics_core::sink::{EventSink, SpoolSink};

/// A started instrumentation session for one process invocation.
///
/// Constructed by [`Analytics::builder`]. A disabled or failed start yields
/// an inert instance whose methods are no-ops.
pub struct Analytics {
    inner: Option<Inner>,
}

struct Inner {
    cli_name: String,
    cli_version: Option<String>,
    allow_values: Vec<String>,
    sink: Box<dyn EventSink>,
    context: ExecutionContext,
    install_id: InstallId,
    session_id: SessionId,
    started: Instant,
}

impl Analytics {
    /// Start configuring instrumentation for `cli_name` — the namespace of
    /// every event this CLI emits.
    #[must_use]
    pub fn builder(cli_name: impl Into<String>) -> AnalyticsBuilder {
        AnalyticsBuilder {
            cli_name: cli_name.into(),
            cli_version: None,
            allow_values: Vec::new(),
            sink: None,
        }
    }

    /// An inert instance: every method is a no-op.
    #[must_use]
    pub fn disabled() -> Self {
        Self { inner: None }
    }

    /// Record the outcome of this invocation. Call once, after dispatch,
    /// with the root [`ArgMatches`] and the exit code about to be returned.
    ///
    /// `error_class` is a type name (see [`error_class_of`]) — never pass
    /// an error message.
    pub fn record_exit(&self, matches: &ArgMatches, exit_code: i32, error_class: Option<&str>) {
        let Some(inner) = &self.inner else {
            return;
        };
        // A bug in the walker must not take the host CLI down with it.
        let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
            inner.record(matches, exit_code, error_class);
        }));
    }

    /// Convenience for `Result`-shaped command runners: exit code 0/1 and
    /// the error's type name.
    pub fn record_result<T, E: std::error::Error>(
        &self,
        matches: &ArgMatches,
        result: &Result<T, E>,
    ) {
        match result {
            Ok(_) => self.record_exit(matches, 0, None),
            Err(error) => self.record_exit(matches, 1, Some(error_class_of(error))),
        }
    }
}

impl Inner {
    fn record(&self, matches: &ArgMatches, exit_code: i32, error_class: Option<&str>) {
        let walk = walk_matches(matches, &self.allow_values);

        let phase = if exit_code == 0 && error_class.is_none() {
            Phase::Completed
        } else {
            Phase::Failed
        };

        let mut builder = Event::builder(self.cli_name.clone(), walk.command_path)
            .phase(phase)
            .flag_names(walk.flag_names)
            .exit_code(exit_code)
            .duration_ms(u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX))
            .actor(self.context.actor)
            .ci(self.context.ci)
            .interactive(self.context.interactive);
        for (name, value) in walk.allowlisted_values {
            builder = builder.flag_value(name, &value);
        }
        if let Some(version) = &self.cli_version {
            builder = builder.cli_version(version.clone());
        }
        if let Some(class) = error_class {
            builder = builder.error_class(class);
        }
        if self.context.actor == Actor::Agent {
            if let Some(name) = &self.context.agent_name {
                builder = builder.agent_name(name.clone());
            }
            if let Some(session) = &self.context.agent_session_id {
                builder = builder.agent_session_id(session.clone());
            }
        }

        let event = builder.build_now(self.install_id, self.session_id);
        self.sink.record(&event);
    }
}

/// Builder for [`Analytics`].
pub struct AnalyticsBuilder {
    cli_name: String,
    cli_version: Option<String>,
    allow_values: Vec<String>,
    sink: Option<Box<dyn EventSink>>,
}

impl AnalyticsBuilder {
    /// The host CLI's own version, recorded as `cli_version`.
    #[must_use]
    pub fn cli_version(mut self, version: impl Into<String>) -> Self {
        self.cli_version = Some(version.into());
        self
    }

    /// Flags whose *values* may be recorded. Everything else contributes
    /// its name only. Allowlisted values still pass redaction.
    #[must_use]
    pub fn allow_values<I, S>(mut self, names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.allow_values = names.into_iter().map(Into::into).collect();
        self
    }

    /// Replace the spool with a custom sink — the testing transport.
    ///
    /// With a sink injected, `start` touches neither configuration nor
    /// disk: events carry an ephemeral install id and go only to the sink.
    #[must_use]
    pub fn sink(mut self, sink: impl EventSink + 'static) -> Self {
        self.sink = Some(Box::new(sink));
        self
    }

    /// Start the session. Infallible: any problem — capture disabled, no
    /// writable data directory, internal bug — yields a no-op instance.
    #[must_use]
    pub fn start(self) -> Analytics {
        std::panic::catch_unwind(AssertUnwindSafe(|| self.try_start()))
            .unwrap_or_else(|_| Analytics::disabled())
    }

    fn try_start(self) -> Analytics {
        let started = Instant::now();

        if let Some(sink) = self.sink {
            // Test transport: no config, no disk.
            return Analytics {
                inner: Some(Inner {
                    cli_name: self.cli_name,
                    cli_version: self.cli_version,
                    allow_values: self.allow_values,
                    sink,
                    context: context::detect(),
                    install_id: InstallId::generate(),
                    session_id: SessionId::generate(),
                    started,
                }),
            };
        }

        let config = Config::load_or_default();
        if !config.enabled {
            return Analytics::disabled();
        }
        let Ok((install_id, created)) = install::load_or_create(&config.install_id_path()) else {
            return Analytics::disabled();
        };
        if created {
            first_run_notice(&self.cli_name);
        }

        Analytics {
            inner: Some(Inner {
                cli_name: self.cli_name,
                cli_version: self.cli_version,
                allow_values: self.allow_values,
                sink: Box::new(SpoolSink::new(config.spool_dir())),
                context: context::detect(),
                install_id,
                session_id: SessionId::generate(),
                started,
            }),
        }
    }
}

/// The one time instrumentation is allowed to print: a first run is never
/// silent. One line, stderr, stated once per install.
fn first_run_notice(cli_name: &str) {
    eprintln!(
        "{cli_name}: local usage analytics enabled (command names and flag names only, \
         stored in ~/.secchi/analytics; set SECCHI_ANALYTICS=0 to disable)"
    );
}

/// The error's type name with module path stripped — a safe `error_class`.
#[must_use]
pub fn error_class_of<E: std::error::Error>(_error: &E) -> &'static str {
    let full = std::any::type_name::<E>();
    full.rsplit("::").next().unwrap_or(full)
}

struct WalkedMatches {
    command_path: Vec<String>,
    flag_names: Vec<String>,
    allowlisted_values: Vec<(String, String)>,
}

/// Collect the resolved subcommand path and explicitly passed flag names
/// from the root matches down. Only `ValueSource::CommandLine` arguments
/// count — defaults and env-sourced values are not something the user
/// typed. Values are extracted only for allowlisted names, as raw strings
/// (typed arguments are skipped rather than risked).
fn walk_matches(root: &ArgMatches, allow_values: &[String]) -> WalkedMatches {
    let mut command_path = Vec::new();
    let mut flag_names = BTreeSet::new();
    let mut allowlisted_values = Vec::new();

    let mut current = root;
    loop {
        for id in current.ids() {
            let name = id.as_str();
            if current.value_source(name) != Some(ValueSource::CommandLine) {
                continue;
            }
            let newly_seen = flag_names.insert(name.to_owned());
            if !newly_seen || !allow_values.iter().any(|allowed| allowed == name) {
                continue;
            }
            if let Ok(Some(mut raw)) = current.try_get_raw(name) {
                if let Some(value) = raw.next() {
                    allowlisted_values
                        .push((name.to_owned(), value.to_string_lossy().into_owned()));
                }
            }
        }
        match current.subcommand() {
            Some((name, sub)) => {
                command_path.push(name.to_owned());
                current = sub;
            }
            None => break,
        }
    }

    WalkedMatches {
        command_path,
        flag_names: flag_names.into_iter().collect(),
        allowlisted_values,
    }
}
