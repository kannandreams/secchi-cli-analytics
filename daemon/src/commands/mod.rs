//! Command implementations. Each submodule exposes `run(...)`; dispatch
//! and error-class mapping live here.

mod compact;
mod init;
mod purge;
mod stats;
mod status;
mod tail;

use clap::ArgMatches;
use secchi_analytics_core::error::{ConfigError, SpoolError, StoreError};

pub fn run(matches: &ArgMatches) -> anyhow::Result<()> {
    match matches.subcommand() {
        Some(("init", _)) => init::run(),
        Some(("status", _)) => status::run(),
        Some(("stats", sub)) => stats::run(
            sub.get_one::<String>("since").expect("has default"),
            sub.get_one::<String>("command").map(String::as_str),
            sub.get_flag("json"),
        ),
        Some(("tail", sub)) => tail::run(
            *sub.get_one::<usize>("lines").expect("has default"),
            sub.get_flag("json"),
        ),
        Some(("compact", _)) => compact::run(),
        Some(("purge", sub)) => purge::run(sub.get_flag("yes")),
        _ => unreachable!("subcommand_required is set"),
    }
}

/// Map an error chain to a stable class name for the self-instrumentation
/// event. Type names only — never messages.
pub fn error_class(error: &anyhow::Error) -> &'static str {
    if error.downcast_ref::<ConfigError>().is_some() {
        "ConfigError"
    } else if error.downcast_ref::<SpoolError>().is_some() {
        "SpoolError"
    } else if error.downcast_ref::<StoreError>().is_some() {
        "StoreError"
    } else if error.downcast_ref::<std::io::Error>().is_some() {
        "IoError"
    } else {
        "Error"
    }
}
