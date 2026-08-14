//! Error taxonomy for the core crate: one small enum per boundary.
//!
//! Callers on the capture path never see these — the sink contract is
//! drop-not-crash, so capture errors are swallowed at the sink. The CLI
//! surfaces them with context via `anyhow`; unexpected panics propagate.

use std::path::PathBuf;

use thiserror::Error;

/// Failures loading or parsing the analytics configuration.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config at {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid config at {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
}

/// Failures appending events to the JSONL spool.
#[derive(Debug, Error)]
pub enum SpoolError {
    #[error("failed to serialize event: {source}")]
    Serialize { source: serde_json::Error },
    #[error("failed to append to spool file {path}: {source}")]
    Append {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Failures in the DuckDB-backed store.
#[cfg(feature = "store")]
#[derive(Debug, Error)]
pub enum StoreError {
    #[error(transparent)]
    Db(#[from] duckdb::Error),
    #[error("store i/o error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}
