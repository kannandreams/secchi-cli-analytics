//! The DuckDB-backed local store (feature `store`).
//!
//! `raw_events` is the only table written here; everything else is derived
//! downstream (dbt, dashboards). The spool is always a live data source:
//! queries union `raw_events` with the not-yet-compacted spool files, so
//! results are correct before any compaction has run.
//!
//! Complex-typed values (arrays, maps) are never marshalled through the
//! Rust bindings — all ingestion goes JSON/Parquet → SQL inside DuckDB.

mod compact;
mod query;

pub use compact::CompactReport;
pub use query::{CommandStats, StatsReport, TailRow};

use std::path::PathBuf;

use duckdb::Connection;

use crate::config::Config;
use crate::error::StoreError;

/// Column name → DuckDB type, in envelope order. This drives the table
/// DDL, the `read_json` column map, and the Parquet schema, so the three
/// can never drift from each other. Deviations from the design doc DDL:
/// `duration_ms` is BIGINT (u64 in the envelope) and `agent_session_id`
/// is VARCHAR (agent frameworks do not guarantee UUID session ids).
const COLUMN_TYPES: &[(&str, &str)] = &[
    ("event_id", "UUID"),
    ("schema_version", "INTEGER"),
    ("event_name", "VARCHAR"),
    ("ts", "TIMESTAMP"),
    ("install_id", "UUID"),
    ("session_id", "UUID"),
    ("cli_name", "VARCHAR"),
    ("cli_version", "VARCHAR"),
    ("command_path", "VARCHAR[]"),
    ("flag_names", "VARCHAR[]"),
    ("flag_values", "MAP(VARCHAR, VARCHAR)"),
    ("duration_ms", "BIGINT"),
    ("exit_code", "INTEGER"),
    ("error_class", "VARCHAR"),
    ("actor", "VARCHAR"),
    ("agent_name", "VARCHAR"),
    ("agent_session_id", "VARCHAR"),
    ("ci", "BOOLEAN"),
    ("interactive", "BOOLEAN"),
    ("os", "VARCHAR"),
    ("arch", "VARCHAR"),
    ("sdk_version", "VARCHAR"),
];

/// An open handle on `events.duckdb` plus the spool location.
pub struct Store {
    connection: Connection,
    spool_dir: PathBuf,
    archive_dir: PathBuf,
}

impl Store {
    /// Open (creating if needed) the events database and ensure the
    /// `raw_events` table exists.
    pub fn open(config: &Config) -> Result<Self, StoreError> {
        std::fs::create_dir_all(&config.data_dir).map_err(|source| StoreError::Io {
            path: config.data_dir.clone(),
            source,
        })?;
        let connection = Connection::open(config.events_db_path())?;
        connection.execute_batch(&create_table_sql())?;
        Ok(Self {
            connection,
            spool_dir: config.spool_dir(),
            archive_dir: config.archive_dir(),
        })
    }

    /// Number of rows already compacted into `raw_events`.
    pub fn raw_event_count(&self) -> Result<u64, StoreError> {
        let count: u64 =
            self.connection
                .query_row("select count(*) from raw_events", [], |row| row.get(0))?;
        Ok(count)
    }

    /// Spool files not yet compacted, sorted by day.
    fn spool_files(&self) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(&self.spool_dir) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
            .collect();
        files.sort();
        files
    }

    /// A SQL relation reading the given spool files with the frozen column
    /// map — never `read_json_auto`, whose inference would depend on file
    /// contents. `ignore_errors` skips a garbled line instead of failing
    /// the whole query.
    fn read_spool_sql(files: &[PathBuf]) -> String {
        let list = files
            .iter()
            .map(|path| format!("'{}'", escape_single_quotes(&path.to_string_lossy())))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "select * from read_json([{list}], format = 'newline_delimited', \
             ignore_errors = true, columns = {{{}}})",
            COLUMN_TYPES
                .iter()
                .map(|(name, ty)| format!("'{name}': '{ty}'"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }

    /// `raw_events` unioned with the live spool. The anti-join keeps
    /// results correct if a compaction was interrupted after upserting but
    /// before deleting the spool file.
    fn unified_events_sql(&self) -> String {
        let files = self.spool_files();
        if files.is_empty() {
            return "select * from raw_events".to_owned();
        }
        format!(
            "select * from raw_events \
             union all \
             select s.* from ({spool}) s anti join raw_events r on s.event_id = r.event_id",
            spool = Self::read_spool_sql(&files)
        )
    }
}

fn create_table_sql() -> String {
    let columns = COLUMN_TYPES
        .iter()
        .map(|(name, ty)| {
            let constraint = match *name {
                "event_id" => " primary key",
                "schema_version" | "event_name" | "ts" | "install_id" | "session_id"
                | "cli_name" | "command_path" | "flag_names" | "actor" | "ci" | "interactive" => {
                    " not null"
                }
                _ => "",
            };
            format!("    {name} {ty}{constraint}")
        })
        .collect::<Vec<_>>()
        .join(",\n");
    format!("create table if not exists raw_events (\n{columns}\n);")
}

fn escape_single_quotes(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::RAW_EVENT_COLUMNS;

    #[test]
    fn column_types_match_the_envelope_columns() {
        let names: Vec<&str> = COLUMN_TYPES.iter().map(|(name, _)| *name).collect();
        assert_eq!(names, RAW_EVENT_COLUMNS, "DDL and envelope diverged");
    }

    #[test]
    fn paths_with_quotes_are_escaped() {
        assert_eq!(escape_single_quotes("it's"), "it''s");
    }
}
