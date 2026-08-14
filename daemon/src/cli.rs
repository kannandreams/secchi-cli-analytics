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
