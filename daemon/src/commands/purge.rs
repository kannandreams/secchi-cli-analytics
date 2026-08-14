//! `secchi-analytics purge` — delete every locally stored event and the
//! install identity in one step. The whole point of local-first data is
//! that the user can always fully delete it.
//!
//! Note for the self-instrumentation in `main`: purge is the one command
//! that does not record its own event — doing so would immediately
//! recreate the spool directory this command just deleted.

use std::io::Write;

use anyhow::Context;
use secchi_analytics_core::config::Config;

pub fn run(assume_yes: bool) -> anyhow::Result<()> {
    let config = Config::load()?;

    if !assume_yes && !confirm(&config)? {
        println!("Aborted; nothing deleted.");
        return Ok(());
    }

    let targets = [
        ("spool", config.spool_dir()),
        ("archive", config.archive_dir()),
    ];
    for (label, dir) in targets {
        if dir.exists() {
            std::fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
            println!("removed {label}/");
        }
    }
    for file in [config.events_db_path(), config.install_id_path()] {
        if file.exists() {
            std::fs::remove_file(&file).with_context(|| format!("removing {}", file.display()))?;
            println!(
                "removed {}",
                file.file_name().unwrap_or_default().to_string_lossy()
            );
        }
    }
    println!("All local analytics data deleted.");
    Ok(())
}

fn confirm(config: &Config) -> anyhow::Result<bool> {
    print!(
        "This deletes all local analytics data at {}. Proceed? [y/N] ",
        config.data_dir.display()
    );
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}
