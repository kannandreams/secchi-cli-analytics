//! `secchi-analytics stats` — per-command aggregates over the unified
//! event set (compacted store plus live spool).

use anyhow::Context;
use jiff::{Span, Timestamp};
use secchi_analytics_core::config::Config;
use secchi_analytics_core::store::Store;

pub fn run(since: &str, command: Option<&str>, json: bool) -> anyhow::Result<()> {
    let config = Config::load()?;
    let cutoff = Timestamp::now()
        .checked_sub(parse_since(since)?)
        .context("window reaches before representable time")?;

    let store = Store::open(&config)?;
    let report = store.stats(cutoff, command, since)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    if report.total_invocations == 0 {
        println!("No events in the last {since}. Instrumented CLIs will show up here.");
        return Ok(());
    }

    println!(
        "{} invocation(s), {} failure(s) in the last {since}",
        report.total_invocations, report.total_failures
    );
    println!();
    println!(
        "{:<28} {:>11} {:>9} {:>9} {:>9}",
        "COMMAND", "INVOCATIONS", "AVG MS", "P95 MS", "FAILURES"
    );
    for row in &report.commands {
        println!(
            "{:<28} {:>11} {:>9} {:>9} {:>9}",
            row.command,
            row.invocations,
            format_ms(row.avg_duration_ms),
            format_ms(row.p95_duration_ms),
            row.failures
        );
    }
    Ok(())
}

fn format_ms(value: Option<f64>) -> String {
    value.map_or_else(|| "-".to_owned(), |ms| format!("{ms:.0}"))
}

/// Parse windows like `7d`, `24h`, `90m`, `2w`; a bare number means days.
fn parse_since(input: &str) -> anyhow::Result<Span> {
    let input = input.trim();
    let (number, unit) = match input.find(|c: char| !c.is_ascii_digit()) {
        Some(split) => input.split_at(split),
        None => (input, "d"),
    };
    let amount: i64 = number
        .parse()
        .with_context(|| format!("invalid --since value: {input:?}"))?;
    let span = match unit {
        "m" => Span::new().minutes(amount),
        "h" => Span::new().hours(amount),
        "d" => Span::new().hours(amount * 24),
        "w" => Span::new().hours(amount * 24 * 7),
        _ => anyhow::bail!("invalid --since unit {unit:?}; use m, h, d, or w"),
    };
    Ok(span)
}

#[cfg(test)]
mod tests {
    use super::parse_since;

    #[test]
    fn parses_units_and_bare_days() {
        assert_eq!(parse_since("90m").unwrap().get_minutes(), 90);
        assert_eq!(parse_since("24h").unwrap().get_hours(), 24);
        assert_eq!(parse_since("7d").unwrap().get_hours(), 7 * 24);
        assert_eq!(parse_since("2w").unwrap().get_hours(), 2 * 7 * 24);
        assert_eq!(parse_since("30").unwrap().get_hours(), 30 * 24);
    }

    #[test]
    fn rejects_nonsense() {
        assert!(parse_since("soon").is_err());
        assert!(parse_since("7y").is_err());
        assert!(parse_since("").is_err());
    }
}
