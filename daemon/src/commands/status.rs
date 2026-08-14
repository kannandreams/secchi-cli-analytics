//! `secchi-analytics status` — a plain statement of what is on, what is
//! stored, and where. Absence of data is not an error: exit 0 either way.

use secchi_analytics_core::config::{self, Config};
use secchi_analytics_core::event::SCHEMA_VERSION;
use secchi_analytics_core::install;

pub fn run() -> anyhow::Result<()> {
    let config = Config::load()?;

    let enabled = if config.enabled {
        "yes".to_owned()
    } else if config::kill_switch_set() {
        "no (SECCHI_ANALYTICS is set)".to_owned()
    } else {
        "no (config.toml)".to_owned()
    };

    println!("enabled          {enabled}");
    println!("schema version   {SCHEMA_VERSION}");
    println!("data directory   {}", config.data_dir.display());
    match install::load(&config.install_id_path()) {
        Some(id) => println!("install id       {id}"),
        None => println!("install id       not initialized (run: secchi-analytics init)"),
    }

    let (spool_files, spool_bytes) = spool_summary(&config);
    println!(
        "spool            {spool_files} file(s), {}",
        human_bytes(spool_bytes)
    );

    match std::fs::metadata(config.events_db_path()) {
        Ok(meta) => println!("events.duckdb    {}", human_bytes(meta.len())),
        Err(_) => println!("events.duckdb    not created yet (run: secchi-analytics compact)"),
    }
    println!("retention        {} days", config.retention_days);
    Ok(())
}

fn spool_summary(config: &Config) -> (usize, u64) {
    let Ok(entries) = std::fs::read_dir(config.spool_dir()) else {
        return (0, 0);
    };
    let mut files = 0;
    let mut bytes = 0;
    for entry in entries.flatten() {
        if entry.path().extension().is_some_and(|ext| ext == "jsonl") {
            files += 1;
            bytes += entry.metadata().map_or(0, |meta| meta.len());
        }
    }
    (files, bytes)
}

// Display-only rounding; precision loss above 2^52 bytes is irrelevant.
#[allow(clippy::cast_precision_loss)]
fn human_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    }
}
