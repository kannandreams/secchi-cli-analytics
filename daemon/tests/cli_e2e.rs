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
