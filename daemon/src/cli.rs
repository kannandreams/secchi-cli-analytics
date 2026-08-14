//! The clap command definition for `secchi-analytics`.

use clap::{Arg, ArgAction, Command};

pub fn build() -> Command {
    Command::new("secchi-analytics")
        .about("Local-first CLI usage analytics for the Secchi family")
        .version(env!("CARGO_PKG_VERSION"))
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("init").about("Create the local data directory and install identity"),
        )
        .subcommand(
            Command::new("status")
                .about("Show what is captured, where it lives, and how much there is"),
        )
        .subcommand(
            Command::new("stats")
                .about("Per-command usage aggregates from the local store")
                .arg(
                    Arg::new("since")
                        .long("since")
                        .default_value("30d")
                        .help("Window, e.g. 7d, 24h, 90m"),
                )
                .arg(
                    Arg::new("command")
                        .long("command")
                        .help("Filter to one command path prefix, e.g. \"project add\""),
                )
                .arg(
                    Arg::new("json")
                        .long("json")
                        .action(ArgAction::SetTrue)
                        .help("Emit the report as JSON"),
                ),
        )
        .subcommand(
            Command::new("tail")
                .about("Show the most recent events")
                .arg(
                    Arg::new("lines")
                        .short('n')
                        .long("lines")
                        .default_value("20")
                        .value_parser(clap::value_parser!(usize))
                        .help("Number of events to show"),
                )
                .arg(
                    Arg::new("json")
                        .long("json")
                        .action(ArgAction::SetTrue)
                        .help("Emit full event envelopes as JSON Lines"),
                ),
        )
        .subcommand(
            Command::new("compact")
                .about("Rotate closed spool days into Parquet and the events database"),
        )
        .subcommand(
            Command::new("purge")
                .about("Delete all locally stored analytics data")
                .arg(
                    Arg::new("yes")
                        .long("yes")
                        .action(ArgAction::SetTrue)
                        .help("Skip the confirmation prompt"),
                ),
        )
}
