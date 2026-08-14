//! Compaction: rotate closed spool days to Parquet and upsert them into
//! `raw_events`, then enforce retention.
//!
//! The step order makes an interruption at any point safe and a re-run
//! idempotent:
//!
//! 1. copy the day file to `archive/<day>.parquet` (overwrite is fine —
//!    the source file is still there),
//! 2. anti-join insert into `raw_events` (already-present event ids are
//!    skipped),
//! 3. delete the spool file (only after both writes landed).
//!
//! Queries stay correct throughout because the unified view anti-joins
//! the spool against `raw_events`.

use std::path::Path;

use jiff::civil::Date;

use super::Store;
use crate::error::StoreError;

/// What one compaction run did.
#[derive(Debug, Default)]
pub struct CompactReport {
    /// Days rotated: (day, events upserted).
    pub compacted: Vec<(String, u64)>,
    /// Rows deleted from `raw_events` by retention.
    pub expired_rows: u64,
    /// Archive files removed by retention.
    pub expired_archives: Vec<String>,
}

impl Store {
    /// Compact every spool day strictly before `today` (UTC), then apply
    /// `retention_days`.
    pub fn compact(&self, today: Date, retention_days: u32) -> Result<CompactReport, StoreError> {
        let mut report = CompactReport::default();

        std::fs::create_dir_all(&self.archive_dir).map_err(|source| StoreError::Io {
            path: self.archive_dir.clone(),
            source,
        })?;

        for file in self.spool_files() {
            let Some(day) = day_of(&file) else {
                continue; // not a day file; leave it alone
            };
            if day >= today {
                continue; // still being written to
            }

            let parquet = self.archive_dir.join(format!("{day}.parquet"));
            let spool_sql = Self::read_spool_sql(std::slice::from_ref(&file));
            self.connection.execute_batch(&format!(
                "copy ({spool_sql}) to '{}' (format parquet);",
                super::escape_single_quotes(&parquet.to_string_lossy())
            ))?;

            let inserted = self.connection.execute(
                &format!(
                    "insert into raw_events \
                     select p.* from read_parquet('{}') p \
                     anti join raw_events r on p.event_id = r.event_id",
                    super::escape_single_quotes(&parquet.to_string_lossy())
                ),
                [],
            )?;

            std::fs::remove_file(&file).map_err(|source| StoreError::Io {
                path: file.clone(),
                source,
            })?;
            report.compacted.push((day.to_string(), inserted as u64));
        }

        self.enforce_retention(today, retention_days, &mut report)?;
        Ok(report)
    }

    fn enforce_retention(
        &self,
        today: Date,
        retention_days: u32,
        report: &mut CompactReport,
    ) -> Result<(), StoreError> {
        let horizon = today.saturating_sub(jiff::Span::new().days(i64::from(retention_days)));

        let deleted = self.connection.execute(
            "delete from raw_events where ts < cast(? as timestamp)",
            [horizon.to_string()],
        )?;
        report.expired_rows = deleted as u64;

        if let Ok(entries) = std::fs::read_dir(&self.archive_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(day) = day_of(&path) else { continue };
                if day < horizon && std::fs::remove_file(&path).is_ok() {
                    report.expired_archives.push(day.to_string());
                }
            }
        }
        Ok(())
    }
}

/// Parse `<YYYY-MM-DD>.<ext>` file names; anything else is not ours.
fn day_of(path: &Path) -> Option<Date> {
    path.file_stem()?.to_str()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_of_parses_day_files_only() {
        assert_eq!(
            day_of(Path::new("/x/2026-08-13.jsonl")),
            Some(Date::constant(2026, 8, 13))
        );
        assert_eq!(day_of(Path::new("/x/notes.jsonl")), None);
    }
}
