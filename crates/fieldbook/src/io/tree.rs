//! Corpus root discovery, in-memory [`Tree`] loader and [`Disk`] implementation of [`crate::World`].

#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "io::tree reads the Corpus tree, working directory and clock for Disk"
)]

use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};

use fieldbook_core::CivilDate;
use fieldbook_core::check::Tree;
use fieldbook_core::hash::Sha256Stream;

use crate::World;

/// An I/O error raised while reading the working directory or Corpus tree from disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldError {
    /// The filesystem path whose operation failed.
    pub path: PathBuf,
    /// Human-readable error description.
    pub message: String,
}

impl WorldError {
    /// Construct a [`WorldError`] for `path` and `message`.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for WorldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.message)
    }
}

impl std::error::Error for WorldError {}

/// Walk up from `start` to the nearest ancestor containing a `corpus/` directory, even if
/// `corpus/corpus.toml` is missing; return `start` itself if no ancestor contains `corpus/`.
#[must_use]
pub fn find_root(start: &Path) -> PathBuf {
    for dir in start.ancestors() {
        if dir.join("corpus").is_dir() {
            return dir.to_path_buf();
        }
    }
    start.to_path_buf()
}

/// Read every directory (including empty ones) and every file under `<root>/corpus/` into a
/// [`Tree`], skipping `.git` and streaming files under `corpus/sources/public/` into
/// `Entry::Original` hashes via [`Sha256Stream`].
///
/// If `<root>/corpus` does not exist, returns an empty [`Tree`].
pub fn read_tree(root: &Path) -> Result<Tree, WorldError> {
    let corpus_dir = root.join("corpus");
    let meta = match std::fs::symlink_metadata(&corpus_dir) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Tree::new()),
        Err(err) => return Err(WorldError::new(&corpus_dir, err.to_string())),
    };
    if !meta.is_dir() {
        return Err(WorldError::new(&corpus_dir, "not a directory"));
    }

    let mut tree = Tree::new();
    tree.insert_dir("corpus");
    read_dir_recursive(&corpus_dir, "corpus", &mut tree)?;
    Ok(tree)
}

fn read_dir_recursive(dir: &Path, rel_prefix: &str, tree: &mut Tree) -> Result<(), WorldError> {
    let entries_iter =
        std::fs::read_dir(dir).map_err(|err| WorldError::new(dir, err.to_string()))?;

    let mut entries = Vec::new();
    for item in entries_iter {
        let entry = item.map_err(|err| WorldError::new(dir, err.to_string()))?;
        entries.push(entry);
    }
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if name == ".git" {
            continue;
        }
        let path = entry.path();
        let rel_path = format!("{rel_prefix}/{name}");
        let file_type = entry
            .file_type()
            .map_err(|err| WorldError::new(&path, err.to_string()))?;

        if file_type.is_dir() {
            tree.insert_dir(&rel_path);
            read_dir_recursive(&path, &rel_path, tree)?;
        } else if file_type.is_file() {
            if rel_path.starts_with("corpus/sources/public/") {
                let (sha256, len) = hash_original_file(&path)?;
                tree.insert_original(rel_path, sha256, len);
            } else {
                let bytes =
                    std::fs::read(&path).map_err(|err| WorldError::new(&path, err.to_string()))?;
                tree.insert_bytes(rel_path, bytes);
            }
        }
    }
    Ok(())
}

fn hash_original_file(path: &Path) -> Result<(String, u64), WorldError> {
    let mut file =
        std::fs::File::open(path).map_err(|err| WorldError::new(path, err.to_string()))?;
    let mut hasher = Sha256Stream::new();
    let mut buf = [0u8; 8192];
    let mut total_len = 0u64;
    loop {
        let count = file
            .read(&mut buf)
            .map_err(|err| WorldError::new(path, err.to_string()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buf[..count]);
        total_len = total_len.saturating_add(count as u64);
    }
    Ok((hasher.finish(), total_len))
}

/// Real filesystem and system-clock implementation of [`World`].
#[derive(Debug, Clone)]
pub struct Disk {
    cwd: PathBuf,
    tree_root: PathBuf,
}

impl Disk {
    /// Create a [`Disk`] rooted at `cwd`, discovering `tree_root` once via [`find_root`].
    #[must_use]
    pub fn new(cwd: impl Into<PathBuf>) -> Self {
        let cwd = cwd.into();
        let tree_root = find_root(&cwd);
        Self { cwd, tree_root }
    }

    /// Capture the process working directory once and construct a [`Disk`].
    pub fn current() -> Result<Self, WorldError> {
        let cwd = std::env::current_dir()
            .map_err(|err| WorldError::new(PathBuf::from("."), err.to_string()))?;
        Ok(Self::new(cwd))
    }
}

impl World for Disk {
    fn read_tree(&self) -> Result<Tree, WorldError> {
        read_tree(&self.tree_root)
    }

    fn today(&self) -> CivilDate {
        let secs = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(dur) => i64::try_from(dur.as_secs()).unwrap_or(0),
            Err(err) => -i64::try_from(err.duration().as_secs()).unwrap_or(0),
        };
        CivilDate::from_epoch_seconds(secs).unwrap_or(CivilDate::MIN)
    }

    fn cwd(&self) -> &Path {
        &self.cwd
    }

    fn tree_root(&self) -> &Path {
        &self.tree_root
    }
}
