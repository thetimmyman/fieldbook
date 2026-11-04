//! [`Tree`] and [`Entry`]: the in-memory Corpus file and directory map passed to `check`.

use std::collections::{BTreeMap, BTreeSet};

/// One file entry in a [`Tree`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// Raw file bytes.
    Bytes(Vec<u8>),
    /// A file under `corpus/sources/public/` hashed while read without keeping its bytes.
    Original {
        /// 64-character lowercase hexadecimal SHA-256 digest.
        sha256: String,
        /// Byte length of the file.
        len: u64,
    },
}

impl Entry {
    /// Return the file's bytes when this entry is [`Entry::Bytes`], or `None` for [`Entry::Original`].
    #[must_use]
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Bytes(bytes) => Some(bytes),
            Self::Original { .. } => None,
        }
    }
}

/// Ordered map from `/`-separated paths relative to the repository root to [`Entry`] values,
/// plus an independent ordered set of directories (including empty directories).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tree {
    entries: BTreeMap<String, Entry>,
    dirs: BTreeSet<String>,
}

fn normalize_dir_path(path: &str) -> String {
    path.trim_matches('/').to_owned()
}

fn matches_prefix(path: &str, prefix: &str) -> bool {
    if prefix.is_empty() {
        return true;
    }
    path == prefix || (path.starts_with(prefix) && path.as_bytes().get(prefix.len()) == Some(&b'/'))
}

impl Tree {
    /// Create an empty [`Tree`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace a file [`Entry`] at `path` (`/`-separated, relative to the repository root).
    pub fn insert(&mut self, path: impl Into<String>, entry: Entry) {
        self.entries.insert(path.into(), entry);
    }

    /// Insert a [`Entry::Bytes`] file at `path`.
    pub fn insert_bytes(&mut self, path: impl Into<String>, bytes: impl Into<Vec<u8>>) {
        self.insert(path, Entry::Bytes(bytes.into()));
    }

    /// Insert an [`Entry::Original`] file at `path`.
    pub fn insert_original(
        &mut self,
        path: impl Into<String>,
        sha256: impl Into<String>,
        len: u64,
    ) {
        self.insert(
            path,
            Entry::Original {
                sha256: sha256.into(),
                len,
            },
        );
    }

    /// Record a directory path (`/`-separated, trailing `/` optional) in the explicit directory set,
    /// along with its non-empty ancestor directories.
    pub fn insert_dir(&mut self, path: impl AsRef<str>) {
        let norm = normalize_dir_path(path.as_ref());
        if norm.is_empty() {
            return;
        }
        let mut cur = norm.as_str();
        loop {
            self.dirs.insert(cur.to_owned());
            if let Some((parent, _)) = cur.rsplit_once('/') {
                if parent.is_empty() {
                    break;
                }
                cur = parent;
            } else {
                break;
            }
        }
    }

    /// Look up the [`Entry`] at `path`.
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&Entry> {
        self.entries.get(path)
    }

    /// Return the bytes at `path` when the entry exists and is [`Entry::Bytes`].
    #[must_use]
    pub fn get_bytes(&self, path: &str) -> Option<&[u8]> {
        self.get(path).and_then(Entry::as_bytes)
    }

    /// Iterate over all file paths in byte order.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    /// Iterate over all `(path, entry)` pairs in byte order of `path`.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Entry)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Iterate over all explicitly recorded directory paths in byte order.
    pub fn dirs(&self) -> impl Iterator<Item = &str> {
        self.dirs.iter().map(String::as_str)
    }

    /// Number of file entries in the tree.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when the tree holds no file entries and no explicit directories.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.dirs.is_empty()
    }

    /// True when `path` (`/`-separated, trailing `/` optional) was recorded in the directory set.
    #[must_use]
    pub fn contains_dir(&self, path: &str) -> bool {
        let norm = normalize_dir_path(path);
        self.dirs.contains(&norm)
    }

    /// Return a new [`Tree`] retaining every file entry and explicit directory at or under `prefix`.
    #[must_use]
    pub fn with_prefix(&self, prefix: &str) -> Self {
        let norm = normalize_dir_path(prefix);
        let entries = self
            .entries
            .iter()
            .filter(|(k, _)| matches_prefix(k, &norm))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let dirs = self
            .dirs
            .iter()
            .filter(|d| matches_prefix(d, &norm))
            .cloned()
            .collect();
        Self { entries, dirs }
    }
}
