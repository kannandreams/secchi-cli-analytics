//! End-to-end tests against the built binary, each in an isolated data
//! directory via SECCHI_ANALYTICS_DIR. Offline and deterministic.

use assert_cmd::Command;
use tempfile::TempDir;

fn cmd(data_dir: &TempDir) -> Command {
    let mut command = Command::cargo_bin("secchi-analytics").unwrap();
    command
        .env("SECCHI_ANALYTICS_DIR", data_dir.path().join("analytics"))
        .env_remove("SECCHI_ANALYTICS")
        .env_remove("CI")
        .env_remove("GITHUB_ACTIONS");
    command
}

fn spool_lines(data_dir: &TempDir) -> Vec<String> {
    let spool = data_dir.path().join("analytics").join("spool");
    let Ok(entries) = std::fs::read_dir(spool) else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    for entry in entries.flatten() {
        let contents = std::fs::read_to_string(entry.path()).unwrap();
        lines.extend(contents.lines().map(str::to_owned));
    }
    lines
}

#[test]
fn init_creates_layout_and_is_idempotent() {
    let dir = TempDir::new().unwrap();

    let first = cmd(&dir).arg("init").assert().success();
    let stdout = String::from_utf8_lossy(&first.get_output().stdout).into_owned();
    assert!(stdout.contains("Initialized"), "notice on first run");
    assert!(stdout.contains("SECCHI_ANALYTICS=0"), "opt-out shown");

    let analytics = dir.path().join("analytics");
    assert!(analytics.join("spool").is_dir());
    assert!(analytics.join("archive").is_dir());
    let id_first = std::fs::read_to_string(analytics.join("install_id")).unwrap();

    let second = cmd(&dir).arg("init").assert().success();
    let stdout = String::from_utf8_lossy(&second.get_output().stdout).into_owned();
    assert!(stdout.contains("already initialized"));
    let id_second = std::fs::read_to_string(analytics.join("install_id")).unwrap();
    assert_eq!(id_first, id_second, "install id is stable");
}

#[test]
fn commands_record_their_own_events() {
    let dir = TempDir::new().unwrap();
    cmd(&dir).arg("init").assert().success();

    cmd(&dir).arg("status").assert().success();
    cmd(&dir).arg("status").assert().success();

    let lines = spool_lines(&dir);
    // init records itself too, so: init + status + status.
    assert_eq!(lines.len(), 3, "one event per invocation");
    let event: serde_json::Value = serde_json::from_str(&lines[1]).unwrap();
    assert_eq!(event["cli_name"], "secchi-analytics");
    assert_eq!(event["event_name"], "secchi-analytics.status.completed");
    assert_eq!(event["command_path"], serde_json::json!(["status"]));
    assert_eq!(event["exit_code"], 0);
    assert_eq!(event["schema_version"], 1);
}

#[test]
fn kill_switch_disables_capture_but_not_the_command() {
    let dir = TempDir::new().unwrap();
    cmd(&dir).arg("init").assert().success();
    let before = spool_lines(&dir).len();

    cmd(&dir)
        .env("SECCHI_ANALYTICS", "0")
        .arg("status")
        .assert()
        .success()
        .stdout(predicates::str::contains("no (SECCHI_ANALYTICS is set)"));

    assert_eq!(spool_lines(&dir).len(), before, "no event captured");
}

#[test]
fn status_before_init_reports_and_exits_zero() {
    let dir = TempDir::new().unwrap();
    cmd(&dir)
        .arg("status")
        .assert()
        .success()
        .stdout(predicates::str::contains("not initialized"));
}

#[test]
fn purge_deletes_everything_and_records_nothing() {
    let dir = TempDir::new().unwrap();
    cmd(&dir).arg("init").assert().success();
    cmd(&dir).arg("status").assert().success();
    assert!(!spool_lines(&dir).is_empty());

    cmd(&dir)
        .args(["purge", "--yes"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "All local analytics data deleted.",
        ));

    let analytics = dir.path().join("analytics");
    assert!(!analytics.join("spool").exists());
    assert!(!analytics.join("archive").exists());
    assert!(!analytics.join("install_id").exists());
    assert!(
        spool_lines(&dir).is_empty(),
        "purge must not respawn the spool"
    );
}

#[test]
fn stats_and_tail_see_self_recorded_events() {
    let dir = TempDir::new().unwrap();
    cmd(&dir).arg("init").assert().success();
    cmd(&dir).arg("status").assert().success();

    // init + status are in the spool; stats reads them without compaction.
    let stats = cmd(&dir)
        .args(["stats", "--since", "1d", "--json"])
        .assert()
        .success();
    let report: serde_json::Value = serde_json::from_slice(&stats.get_output().stdout).unwrap();
    assert_eq!(report["total_invocations"], 2);
    assert_eq!(report["total_failures"], 0);

    let tail = cmd(&dir).args(["tail", "--json"]).assert().success();
    let stdout = String::from_utf8_lossy(&tail.get_output().stdout).into_owned();
    // init + status + the stats invocation just above.
    assert_eq!(stdout.lines().count(), 3);
    let newest: serde_json::Value = serde_json::from_str(stdout.lines().next().unwrap()).unwrap();
    assert_eq!(newest["event_name"], "secchi-analytics.stats.completed");
    assert_eq!(newest["flag_names"], serde_json::json!(["json", "since"]));
    assert_eq!(newest["flag_values"]["since"], "1d");

    // Human-readable variants render without error.
    cmd(&dir)
        .arg("stats")
        .assert()
        .success()
        .stdout(predicates::str::contains("COMMAND"));
    cmd(&dir)
        .arg("tail")
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "secchi-analytics.status.completed",
        ));
}

#[test]
fn compact_reports_and_stats_stay_correct() {
    let dir = TempDir::new().unwrap();
    cmd(&dir).arg("init").assert().success();

    // Everything in the spool is from today, so there is nothing to rotate.
    cmd(&dir)
        .arg("compact")
        .assert()
        .success()
        .stdout(predicates::str::contains("Nothing to compact"));

    // A closed day planted in the spool gets rotated on the next run.
    // Yesterday, so it is closed but comfortably inside retention.
    let yesterday = (jiff::Zoned::now().with_time_zone(jiff::tz::TimeZone::UTC)
        - jiff::Span::new().hours(24))
    .date();
    let spool = dir.path().join("analytics").join("spool");
    let old_line = format!(
        r#"{{"event_id":"0198aaaa-0000-7000-8000-00000000000f","schema_version":1,"event_name":"otherctl.build.completed","ts":"{yesterday}T10:00:00Z","install_id":"11111111-2222-4333-8444-555555555555","session_id":"0198aaaa-0000-7000-8000-000000000002","cli_name":"otherctl","command_path":["build"],"flag_names":[],"actor":"human","ci":false,"interactive":true,"os":"linux","arch":"x86_64","sdk_version":"0.1.0"}}"#
    );
    std::fs::write(
        spool.join(format!("{yesterday}.jsonl")),
        format!("{old_line}\n"),
    )
    .unwrap();

    cmd(&dir)
        .arg("compact")
        .assert()
        .success()
        .stdout(predicates::str::contains(format!(
            "compacted {yesterday}: 1 new event(s)"
        )));
    assert!(!spool.join(format!("{yesterday}.jsonl")).exists());
    assert!(
        dir.path()
            .join("analytics")
            .join("archive")
            .join(format!("{yesterday}.parquet"))
            .exists()
    );
}

#[test]
fn purge_without_confirmation_aborts() {
    let dir = TempDir::new().unwrap();
    cmd(&dir).arg("init").assert().success();

    cmd(&dir)
        .arg("purge")
        .write_stdin("n\n")
        .assert()
        .success()
        .stdout(predicates::str::contains("Aborted"));

    assert!(dir.path().join("analytics").join("install_id").exists());
}
