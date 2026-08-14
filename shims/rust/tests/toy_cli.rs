//! Integration tests: a toy clap application instrumented with the
//! MemorySink transport. No disk, no environment, no network.

use clap::{Arg, ArgAction, Command};
use secchi_analytics_clap::{Analytics, error_class_of};
use secchi_analytics_core::event::Event;
use secchi_analytics_core::sink::MemorySink;

fn toy_command() -> Command {
    Command::new("toyctl").subcommand(
        Command::new("project").subcommand(
            Command::new("add")
                .arg(Arg::new("format").long("format"))
                .arg(Arg::new("name").long("name").default_value("unnamed"))
                .arg(
                    Arg::new("verbose")
                        .long("verbose")
                        .action(ArgAction::SetTrue),
                ),
        ),
    )
}

fn record(argv: &[&str], exit_code: i32, error_class: Option<&str>) -> Event {
    let sink = MemorySink::shared();
    let analytics = Analytics::builder("toyctl")
        .cli_version("9.9.9")
        .allow_values(["format"])
        .sink(sink.clone())
        .start();

    let matches = toy_command().get_matches_from(argv);
    analytics.record_exit(&matches, exit_code, error_class);

    let events = sink.events();
    assert_eq!(events.len(), 1, "exactly one event per invocation");
    events.into_iter().next().unwrap()
}

#[test]
fn records_command_path_flags_and_outcome() {
    let event = record(
        &["toyctl", "project", "add", "--format", "json", "--verbose"],
        0,
        None,
    );

    assert_eq!(event.event_name, "toyctl.project.add.completed");
    assert_eq!(event.command_path, vec!["project", "add"]);
    assert_eq!(event.flag_names, vec!["format", "verbose"]);
    assert_eq!(event.flag_values["format"], "json");
    assert_eq!(event.cli_version.as_deref(), Some("9.9.9"));
    assert_eq!(event.exit_code, Some(0));
    assert!(event.duration_ms.is_some());
    assert_eq!(event.error_class, None);
}

#[test]
fn defaulted_arguments_are_not_flags_the_user_passed() {
    // --name has a default value; the user never typed it.
    let event = record(&["toyctl", "project", "add", "--verbose"], 0, None);
    assert_eq!(event.flag_names, vec!["verbose"]);
    assert!(event.flag_values.is_empty());
}

#[test]
fn non_allowlisted_values_are_never_captured() {
    let sink = MemorySink::shared();
    let analytics = Analytics::builder("toyctl").sink(sink.clone()).start();
    let matches = toy_command().get_matches_from(["toyctl", "project", "add", "--format", "json"]);
    analytics.record_exit(&matches, 0, None);

    let event = &sink.events()[0];
    assert_eq!(event.flag_names, vec!["format"]);
    assert!(
        event.flag_values.is_empty(),
        "no allowlist configured, so no values at all"
    );
}

#[test]
fn secrets_through_allowlisted_flags_arrive_redacted() {
    let event = record(
        &[
            "toyctl",
            "project",
            "add",
            "--format",
            "ghp_FAKE000000000000FAKE",
        ],
        0,
        None,
    );
    assert_eq!(event.flag_values["format"], "[REDACTED:github-token]");
}

#[test]
fn failure_records_phase_and_error_class() {
    let event = record(&["toyctl", "project", "add"], 1, Some("ConfigError"));
    assert_eq!(event.event_name, "toyctl.project.add.failed");
    assert_eq!(event.exit_code, Some(1));
    assert_eq!(event.error_class.as_deref(), Some("ConfigError"));
}

#[test]
fn record_result_maps_ok_and_err() {
    let sink = MemorySink::shared();
    let analytics = Analytics::builder("toyctl").sink(sink.clone()).start();
    let matches = toy_command().get_matches_from(["toyctl", "project", "add"]);

    let ok: Result<(), std::fmt::Error> = Ok(());
    analytics.record_result(&matches, &ok);
    let err: Result<(), std::fmt::Error> = Err(std::fmt::Error);
    analytics.record_result(&matches, &err);

    let events = sink.events();
    assert_eq!(events[0].exit_code, Some(0));
    assert_eq!(events[1].exit_code, Some(1));
    assert_eq!(events[1].error_class.as_deref(), Some("Error"));
}

#[test]
fn disabled_instance_records_nothing() {
    let analytics = Analytics::disabled();
    let matches = toy_command().get_matches_from(["toyctl", "project", "add"]);
    // No sink to observe; the assertion is simply that this is a no-op
    // and does not panic or touch the filesystem.
    analytics.record_exit(&matches, 0, None);
}

#[test]
fn error_class_of_strips_the_module_path() {
    assert_eq!(error_class_of(&std::fmt::Error), "Error");
}
