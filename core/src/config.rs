//! Configuration resolution: environment, then config file, then defaults.
//!
//! Precedence, most specific first:
//!
//! 1. `SECCHI_ANALYTICS=0` (or `false`) disables capture outright.
//! 2. `SECCHI_ANALYTICS_DIR` relocates the data directory (tests and CI
//!    isolation rely on this).
//! 3. `<default data dir>/config.toml`, an `[analytics]` table.
//! 4. Defaults: enabled, `~/.secchi/analytics`, 90-day retention.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::ConfigError;

/// Default retention for compacted local events, in days. A privacy and
/// hygiene default, not a storage concern.
pub const DEFAULT_RETENTION_DAYS: u32 = 90;

/// Resolved analytics configuration plus the derived data-directory layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub enabled: bool,
    pub data_dir: PathBuf,
    pub retention_days: u32,
}

#[derive(Debug, Default, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    analytics: AnalyticsTable,
}

#[derive(Debug, Default, Deserialize)]
struct AnalyticsTable {
    enabled: Option<bool>,
    data_dir: Option<PathBuf>,
    retention_days: Option<u32>,
}

impl Config {
    /// Resolve configuration, reporting config-file problems. The CLI uses
    /// this so a corrupt file is surfaced instead of silently ignored.
    pub fn load() -> Result<Self, ConfigError> {
        Self::resolve(true)
    }

    /// Resolve configuration for the capture path: a missing or corrupt
    /// config file degrades to defaults, never to an error. Instrumentation
    /// must not add failure modes to a host CLI.
    #[must_use]
    pub fn load_or_default() -> Self {
        Self::resolve(false).unwrap_or_else(|_| Self::defaults(default_data_dir()))
    }

    fn resolve(strict: bool) -> Result<Self, ConfigError> {
        let default_dir = default_data_dir();
        let file = match read_config_file(&config_path_in(&default_dir)) {
            Ok(file) => file,
            Err(_) if !strict => ConfigFile::default(),
            Err(err) => return Err(err),
        };

        let mut config = Self::defaults(default_dir);
        if let Some(enabled) = file.analytics.enabled {
            config.enabled = enabled;
        }
        if let Some(dir) = file.analytics.data_dir {
            config.data_dir = dir;
        }
        if let Some(days) = file.analytics.retention_days {
            config.retention_days = days;
        }

        // Environment overrides beat the file.
        if let Some(dir) = std::env::var_os("SECCHI_ANALYTICS_DIR") {
            config.data_dir = PathBuf::from(dir);
        }
        if kill_switch_set() {
            config.enabled = false;
        }
        Ok(config)
    }

    fn defaults(data_dir: PathBuf) -> Self {
        Self {
            enabled: true,
            data_dir,
            retention_days: DEFAULT_RETENTION_DAYS,
        }
    }

    #[must_use]
    pub fn spool_dir(&self) -> PathBuf {
        self.data_dir.join("spool")
    }

    #[must_use]
    pub fn archive_dir(&self) -> PathBuf {
        self.data_dir.join("archive")
    }

    #[must_use]
    pub fn events_db_path(&self) -> PathBuf {
        self.data_dir.join("events.duckdb")
    }

    #[must_use]
    pub fn install_id_path(&self) -> PathBuf {
        self.data_dir.join("install_id")
    }
}

/// True when `SECCHI_ANALYTICS` is set to an explicit off value.
#[must_use]
pub fn kill_switch_set() -> bool {
    matches!(
        std::env::var("SECCHI_ANALYTICS").as_deref(),
        Ok("0" | "false" | "off")
    )
}

/// `~/.secchi/analytics`, or a path-relative fallback when no home
/// directory can be determined (containers with a stripped environment).
#[must_use]
pub fn default_data_dir() -> PathBuf {
    home_dir().map_or_else(
        || PathBuf::from(".secchi-analytics"),
        |home| home.join(".secchi").join("analytics"),
    )
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let var = "USERPROFILE";
    #[cfg(not(windows))]
    let var = "HOME";
    std::env::var_os(var)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn config_path_in(data_dir: &Path) -> PathBuf {
    data_dir.join("config.toml")
}

fn read_config_file(path: &Path) -> Result<ConfigFile, ConfigError> {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ConfigFile::default());
        }
        Err(err) => {
            return Err(ConfigError::Read {
                path: path.to_owned(),
                source: err,
            });
        }
    };
    toml::from_str(&contents).map_err(|err| ConfigError::Parse {
        path: path.to_owned(),
        source: err,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Environment-variable behavior is exercised end-to-end in the binary's
    // CLI tests, where the process environment can be set per invocation
    // without racing other unit tests in this process.

    #[test]
    fn parses_full_config_file() {
        let file: ConfigFile = toml::from_str(
            r#"
            [analytics]
            enabled = false
            data_dir = "/tmp/elsewhere"
            retention_days = 30
            "#,
        )
        .unwrap();
        assert_eq!(file.analytics.enabled, Some(false));
        assert_eq!(
            file.analytics.data_dir,
            Some(PathBuf::from("/tmp/elsewhere"))
        );
        assert_eq!(file.analytics.retention_days, Some(30));
    }

    #[test]
    fn empty_and_partial_files_leave_defaults() {
        let empty: ConfigFile = toml::from_str("").unwrap();
        assert_eq!(empty.analytics.enabled, None);

        let partial: ConfigFile = toml::from_str("[analytics]\nretention_days = 7\n").unwrap();
        assert_eq!(partial.analytics.retention_days, Some(7));
        assert_eq!(partial.analytics.data_dir, None);
    }

    #[test]
    fn layout_paths_derive_from_data_dir() {
        let config = Config::defaults(PathBuf::from("/data"));
        assert_eq!(config.spool_dir(), PathBuf::from("/data/spool"));
        assert_eq!(config.archive_dir(), PathBuf::from("/data/archive"));
        assert_eq!(
            config.events_db_path(),
            PathBuf::from("/data/events.duckdb")
        );
        assert_eq!(config.install_id_path(), PathBuf::from("/data/install_id"));
    }
}
