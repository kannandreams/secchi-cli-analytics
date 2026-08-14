//! `secchi-analytics tail` — the most recent events, newest first.

use secchi_analytics_core::config::Config;
use secchi_analytics_core::store::Store;

pub fn run(limit: usize, json: bool) -> anyhow::Result<()> {
    let config = Config::load()?;
    let store = Store::open(&config)?;
    let rows = store.tail(limit)?;

    if json {
        for row in &rows {
            println!("{}", row.json);
        }
        return Ok(());
    }

    if rows.is_empty() {
        println!("No events yet. Instrumented CLIs will show up here.");
        return Ok(());
    }

    for row in &rows {
        println!(
            "{}  {:<40} exit {:>3}  {:>6}  {}",
            row.ts,
            row.event_name,
            row.exit_code.map_or_else(|| "-".into(), |c| c.to_string()),
            row.duration_ms
                .map_or_else(|| "-".into(), |ms| format!("{ms}ms")),
            row.actor
        );
    }
    Ok(())
}
