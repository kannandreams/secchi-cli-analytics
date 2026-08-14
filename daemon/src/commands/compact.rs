//! `secchi-analytics compact` — rotate closed spool days and enforce
//! retention. Safe to interrupt and re-run at any point.

use secchi_analytics_core::config::Config;
use secchi_analytics_core::store::Store;

pub fn run() -> anyhow::Result<()> {
    let config = Config::load()?;
    let store = Store::open(&config)?;
    let today = jiff::Zoned::now()
        .with_time_zone(jiff::tz::TimeZone::UTC)
        .date();
    let report = store.compact(today, config.retention_days)?;

    if report.compacted.is_empty() {
        println!("Nothing to compact; the spool holds only today's events.");
    }
    for (day, events) in &report.compacted {
        println!("compacted {day}: {events} new event(s)");
    }
    if report.expired_rows > 0 {
        println!(
            "retention: removed {} event(s) older than {} days",
            report.expired_rows, config.retention_days
        );
    }
    for day in &report.expired_archives {
        println!("retention: removed archive {day}.parquet");
    }
    Ok(())
}
