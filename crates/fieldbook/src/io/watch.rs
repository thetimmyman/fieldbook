//! Filesystem polling seam ([`Poll`]) and disk implementation ([`DiskPoll`]) for `fieldbook build --watch`.

#![expect(
    clippy::disallowed_methods,
    reason = "io::watch is the metadata-only poll of the Corpus folder"
)]

use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use crate::io::WorldError;

/// Seam for filesystem change polling.
pub trait Poll {
    /// Compute a metadata-only fingerprint of `<tree root>/corpus/`.
    ///
    /// # Errors
    ///
    /// Returns [`WorldError`] if the corpus directory cannot be read or traversed.
    fn fingerprint(&mut self) -> Result<u64, WorldError>;

    /// Pause before the next poll. Returns `false` when the watch must end.
    fn pause(&mut self) -> bool;
}

/// Production disk-based [`Poll`] implementation that polls the Corpus folder every 500 ms.
#[derive(Debug, Clone)]
pub struct DiskPoll {
    /// The tree root containing the `corpus/` folder.
    pub root: PathBuf,
}

impl DiskPoll {
    /// Create a new [`DiskPoll`] for the given `root`.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl Poll for DiskPoll {
    fn fingerprint(&mut self) -> Result<u64, WorldError> {
        corpus_fingerprint(&self.root)
    }

    fn pause(&mut self) -> bool {
        std::thread::sleep(Duration::from_millis(500));
        true
    }
}

struct EntryRecord {
    rel_path: String,
    kind: u8,
    len: u64,
    modified_secs: u64,
    modified_nanos: u32,
}

/// Compute a deterministic metadata-only fingerprint of `<root>/corpus/`.
///
/// # Errors
///
/// Returns [`WorldError`] if `corpus/` is missing, not a directory, or any subfolder cannot be listed.
pub fn corpus_fingerprint(root: &Path) -> Result<u64, WorldError> {
    let corpus_dir = root.join("corpus");
    let meta = std::fs::symlink_metadata(&corpus_dir)
        .map_err(|err| WorldError::new(&corpus_dir, err.to_string()))?;
    if !meta.is_dir() {
        return Err(WorldError::new(&corpus_dir, "not a directory"));
    }

    let mut entries = Vec::new();
    walk_dir(&corpus_dir, "", &mut entries)?;
    entries.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

    let mut hasher = DefaultHasher::new();
    for entry in &entries {
        hasher.write(entry.rel_path.as_bytes());
        hasher.write_u8(0);
        hasher.write_u8(entry.kind);
        hasher.write_u64(entry.len);
        hasher.write_u64(entry.modified_secs);
        hasher.write_u32(entry.modified_nanos);
    }
    Ok(hasher.finish())
}

/// Query symlink metadata for `path`, returning `None` if the entry vanished.
///
/// # Errors
///
/// Returns [`WorldError`] if metadata retrieval fails with any error other than `NotFound`.
fn entry_metadata(path: &Path) -> Result<Option<std::fs::Metadata>, WorldError> {
    match std::fs::symlink_metadata(path) {
        Ok(m) => Ok(Some(m)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(WorldError::new(path, err.to_string())),
    }
}

/// Walk `dir` recursively, collecting metadata records for every entry.
///
/// Below `corpus/`, a `NotFound` entry is skipped and every other error is fatal.
///
/// # Errors
///
/// Returns [`WorldError`] if a directory entry or subfolder cannot be read for
/// reasons other than `NotFound`.
fn walk_dir(
    dir: &Path,
    rel_prefix: &str,
    entries: &mut Vec<EntryRecord>,
) -> Result<(), WorldError> {
    let read_dir = match std::fs::read_dir(dir) {
        Ok(read_dir) => read_dir,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(WorldError::new(dir, err.to_string())),
    };
    for entry_res in read_dir {
        let entry = entry_res.map_err(|err| WorldError::new(dir, err.to_string()))?;
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if name == ".git" {
            continue;
        }
        let path = entry.path();
        let Some(meta) = entry_metadata(&path)? else {
            continue;
        };
        let rel_path = if rel_prefix.is_empty() {
            name.into_owned()
        } else {
            format!("{rel_prefix}/{name}")
        };
        let ft = meta.file_type();
        let kind = if ft.is_symlink() {
            2
        } else if ft.is_dir() {
            1
        } else if ft.is_file() {
            0
        } else {
            3
        };
        let (modified_secs, modified_nanos) = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or((0u64, 0u32), |d| (d.as_secs(), d.subsec_nanos()));

        entries.push(EntryRecord {
            rel_path: rel_path.clone(),
            kind,
            len: meta.len(),
            modified_secs,
            modified_nanos,
        });

        if ft.is_dir() && !ft.is_symlink() {
            walk_dir(&path, &rel_path, entries)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vanished_listed_entry_is_skipped() {
        let dir = tempfile::tempdir().expect("tempdir");
        let vanished = dir.path().join("vanished.txt");
        let meta = entry_metadata(&vanished).expect("entry_metadata");
        assert!(meta.is_none());

        let present = tempfile::NamedTempFile::new_in(dir.path()).expect("temp file");
        let meta = entry_metadata(present.path()).expect("entry_metadata");
        assert!(meta.is_some());
    }

    #[test]
    fn a_vanished_subfolder_is_skipped_by_the_walk() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path_that_does_not_exist = dir.path().join("does_not_exist");
        let mut entries: Vec<EntryRecord> = Vec::new();
        let res = walk_dir(&path_that_does_not_exist, "", &mut entries);
        assert!(res.is_ok());
        assert!(entries.is_empty());

        let subfolder = tempfile::Builder::new()
            .tempdir_in(dir.path())
            .expect("subfolder");
        let _file = tempfile::NamedTempFile::new_in(subfolder.path()).expect("temp file");
        let mut entries = Vec::new();
        walk_dir(dir.path(), "", &mut entries).expect("walk with the subfolder present");
        assert_eq!(entries.len(), 2);

        subfolder.close().expect("remove the subfolder");
        let mut entries = Vec::new();
        walk_dir(dir.path(), "", &mut entries).expect("walk after the subfolder vanished");
        assert!(entries.is_empty());
    }

    #[test]
    fn a_non_notfound_error_stays_fatal() {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        let mut entries = Vec::new();
        let res = walk_dir(file.path(), "", &mut entries);
        assert!(res.is_err());
    }
}
