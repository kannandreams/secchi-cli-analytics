//! The anonymous install identity: a random UUID in a plain text file.

use std::io::Write;
use std::path::Path;

use crate::event::InstallId;

/// Read the install id if the file exists and parses.
#[must_use]
pub fn load(path: &Path) -> Option<InstallId> {
    let contents = std::fs::read_to_string(path).ok()?;
    contents.trim().parse().ok()
}

/// Load the install id, creating it (and the data directory) on first run.
///
/// Returns the id and whether it was newly created — callers use the flag
/// to print the one-time first-run notice, so a first run is never silent.
pub fn load_or_create(path: &Path) -> std::io::Result<(InstallId, bool)> {
    if let Some(existing) = load(path) {
        return Ok((existing, false));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let id = InstallId::generate();
    // Write-then-rename so a crash cannot leave a half-written id behind.
    let tmp = path.with_extension("tmp");
    {
        let mut file = std::fs::File::create(&tmp)?;
        writeln!(file, "{id}")?;
    }
    std::fs::rename(&tmp, path)?;
    Ok((id, true))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_then_reloads_the_same_id() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install_id");

        let (created, was_new) = load_or_create(&path).unwrap();
        assert!(was_new);

        let (loaded, was_new) = load_or_create(&path).unwrap();
        assert!(!was_new);
        assert_eq!(created, loaded);
        assert_eq!(load(&path), Some(created));
    }

    #[test]
    fn creates_missing_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("deeper").join("install_id");
        let (_, was_new) = load_or_create(&path).unwrap();
        assert!(was_new);
        assert!(path.exists());
    }

    #[test]
    fn garbage_file_reads_as_absent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install_id");
        std::fs::write(&path, "not a uuid").unwrap();
        assert_eq!(load(&path), None);
    }
}
