//! The `secchi-analytics` binary. In v0 it runs in agent mode only: a
//! one-shot CLI over the local spool and DuckDB store. The crate directory
//! is named `daemon/` for the planned long-lived socket-listener mode,
//! which is deferred.
//!
//! The binary instruments itself with the clap shim — this `main` is the
//! reference integration the README quotes.

mod cli;
mod commands;

use secchi_analytics_clap::Analytics;

fn main() {
    let analytics = Analytics::builder("secchi-analytics")
        .cli_version(env!("CARGO_PKG_VERSION"))
        .allow_values(["since"])
        .start();

    let matches = cli::build().get_matches();
    let outcome = commands::run(&matches);

    let (exit_code, error_class) = match &outcome {
        Ok(()) => (0, None),
        Err(error) => {
            eprintln!("error: {error:#}");
            (1, Some(commands::error_class(error)))
        }
    };

    // `purge` deliberately does not record itself: its event would recreate
    // the spool directory that was just deleted.
    if matches.subcommand_name() != Some("purge") {
        analytics.record_exit(&matches, exit_code, error_class);
    }
    std::process::exit(exit_code);
}
