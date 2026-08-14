//! `secchi-analytics init` — create the data directory, the install
//! identity, and print the first-run notice. Idempotent.

use anyhow::Context;
use secchi_analytics_core::config::Config;
use secchi_analytics_core::install;

pub fn run() -> anyhow::Result<()> {
    let config = Config::load()?;

    std::fs::create_dir_all(config.spool_dir())
        .with_context(|| format!("creating {}", config.spool_dir().display()))?;
    std::fs::create_dir_all(config.archive_dir())
        .with_context(|| format!("creating {}", config.archive_dir().display()))?;

    let (install_id, created) = install::load_or_create(&config.install_id_path())
        .with_context(|| format!("creating {}", config.install_id_path().display()))?;

    if created {
        println!("Initialized analytics data directory.");
    } else {
        println!("Analytics data directory already initialized.");
    }
    println!();
    println!("  data directory  {}", config.data_dir.display());
    println!("  install id      {install_id}");
    println!();
    println!("What is captured: command names, flag names, durations, and");
    println!("exit codes for instrumented CLIs. Never raw arguments, never");
    println!("error messages, never file contents.");
    println!();
    println!("Disable at any time with SECCHI_ANALYTICS=0. Delete everything");
    println!("with: secchi-analytics purge");
    Ok(())
}
