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

fn walk_dir(
    dir: &Path,
    rel_prefix: &str,
    entries: &mut Vec<EntryRecord>,
) -> Result<(), WorldError> {
    let read_dir = std::fs::read_dir(dir).map_err(|err| WorldError::new(dir, err.to_string()))?;
    for entry_res in read_dir {
        let entry = entry_res.map_err(|err| WorldError::new(dir, err.to_string()))?;
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if name == ".git" {
            continue;
        }
        let path = entry.path();
        let meta = std::fs::symlink_metadata(&path)
            .map_err(|err| WorldError::new(&path, err.to_string()))?;
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
