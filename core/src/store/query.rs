//! Read queries over the unified event set (raw_events ∪ live spool).

use jiff::Timestamp;
use serde::Serialize;

use super::Store;
use crate::error::StoreError;

/// Aggregates for `secchi-analytics stats`.
#[derive(Debug, Serialize)]
pub struct StatsReport {
    pub since: String,
    pub total_invocations: u64,
    pub total_failures: u64,
    pub commands: Vec<CommandStats>,
}

/// One row of the per-command table.
#[derive(Debug, Serialize)]
pub struct CommandStats {
    pub command: String,
    pub invocations: u64,
    pub avg_duration_ms: Option<f64>,
    pub p95_duration_ms: Option<f64>,
    pub failures: u64,
}

/// One row of `secchi-analytics tail`: display fields plus the full
/// envelope as JSON (rendered by DuckDB, so complex types never cross the
/// bindings).
#[derive(Debug)]
pub struct TailRow {
    pub ts: String,
    pub event_name: String,
    pub duration_ms: Option<i64>,
    pub exit_code: Option<i32>,
    pub actor: String,
    pub json: String,
}

impl Store {
    /// Per-command aggregates for events at or after `cutoff`, optionally
    /// filtered to one dot-joined command path prefix.
    pub fn stats(
        &self,
        cutoff: Timestamp,
        command_filter: Option<&str>,
        since_label: &str,
    ) -> Result<StatsReport, StoreError> {
        let sql = format!(
            "with events as ({unified}) \
             select coalesce(nullif(array_to_string(command_path, ' '), ''), '(root)') as command, \
                    count(*) as invocations, \
                    avg(duration_ms) as avg_ms, \
                    quantile_cont(duration_ms, 0.95) as p95_ms, \
                    coalesce(sum(case when exit_code != 0 then 1 else 0 end), 0) as failures \
             from events \
             where ts >= cast(? as timestamp) \
               and (? is null or starts_with(array_to_string(command_path, ' '), ?)) \
             group by 1 \
             order by invocations desc, command",
            unified = self.unified_events_sql()
        );
        let cutoff = sql_timestamp(cutoff);
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement.query_map(
            duckdb::params![cutoff, command_filter, command_filter],
            |row| {
                Ok(CommandStats {
                    command: row.get(0)?,
                    invocations: row.get(1)?,
                    avg_duration_ms: row.get(2)?,
                    p95_duration_ms: row.get(3)?,
                    failures: row.get::<_, i64>(4)?.unsigned_abs(),
                })
            },
        )?;
        let commands: Vec<CommandStats> = rows.collect::<Result<_, _>>()?;

        Ok(StatsReport {
            since: since_label.to_owned(),
            total_invocations: commands.iter().map(|c| c.invocations).sum(),
            total_failures: commands.iter().map(|c| c.failures).sum(),
            commands,
        })
    }

    /// The most recent `limit` events, newest first. UUIDv7 event ids sort
    /// by creation time, so they are the tiebreaker within a timestamp.
    pub fn tail(&self, limit: usize) -> Result<Vec<TailRow>, StoreError> {
        let sql = format!(
            "with events as ({unified}) \
             select strftime(ts, '%Y-%m-%d %H:%M:%S') as ts, \
                    event_name, duration_ms, exit_code, actor, \
                    to_json(e)::varchar as json \
             from events e \
             order by ts desc, event_id desc \
             limit ?",
            unified = self.unified_events_sql()
        );
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement.query_map([limit], |row| {
            Ok(TailRow {
                ts: row.get(0)?,
                event_name: row.get(1)?,
                duration_ms: row.get(2)?,
                exit_code: row.get(3)?,
                actor: row.get(4)?,
                json: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

/// RFC 3339 with the trailing `Z` swapped for a plain UTC timestamp
/// literal DuckDB casts unambiguously.
fn sql_timestamp(ts: Timestamp) -> String {
    ts.strftime("%Y-%m-%d %H:%M:%S%.f").to_string()
}
