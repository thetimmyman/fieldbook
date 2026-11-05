//! Outbound delivery binding types (`Delivery`, `Transfer`, `Target`, `Operation`,
//! `docs/reference/policy.md#the-decision`, `core-policy` step 16).

use std::path::PathBuf;

use crate::hash::{Sha256Stream, sha256_hex};
use crate::labels::LabelSet;

/// Target of a bound outbound [`Transfer`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Target {
    /// Filesystem path outside the data root (`export`, `backup`, `audit`).
    Path(PathBuf),
    /// Remote or local HTTP(S) endpoint URL (`model`).
    Endpoint(String),
    /// In-memory reply channel (`handoff`, `mcp`, `bridge`, `print`, `clipboard`, `edition`).
    Reply(String),
}

/// Outbound operation bound to a [`Transfer`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Operation {
    /// Write exact bytes to a file (`Target::Path`).
    Write,
    /// Return exact bytes in a reply (`Target::Reply`).
    Reply,
    /// Send exact bytes in an HTTP POST body (`Target::Endpoint`).
    Post,
    /// Create a directory (`Target::Path`).
    CreateDir,
    /// Remove a directory tree whose manifest matches (`Target::Path`).
    RemoveTree,
    /// Rename a directory tree whose manifest matches (`Target::Path`).
    RenameTree {
        /// Destination path of the tree rename.
        to: PathBuf,
    },
}

/// One bound outbound transfer operation within a [`Delivery`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Transfer {
    /// Canonical target of the transfer.
    pub target: Target,
    /// Operation to perform.
    pub operation: Operation,
    /// 64-character lowercase hex SHA-256 of the payload bytes (or sorted tree manifest).
    pub sha256: String,
    /// Exact byte length of the payload (or total byte length of the tree manifest).
    pub bytes: u64,
}

impl Transfer {
    /// Constructs a [`Transfer`] for writing `payload` to `path`.
    #[must_use]
    pub fn write_path(path: impl Into<PathBuf>, payload: &[u8]) -> Self {
        Self {
            target: Target::Path(path.into()),
            operation: Operation::Write,
            sha256: sha256_hex(payload),
            bytes: payload.len() as u64,
        }
    }

    /// Constructs a [`Transfer`] for posting `payload` to `endpoint`.
    #[must_use]
    pub fn post_endpoint(endpoint: impl Into<String>, payload: &[u8]) -> Self {
        Self {
            target: Target::Endpoint(endpoint.into()),
            operation: Operation::Post,
            sha256: sha256_hex(payload),
            bytes: payload.len() as u64,
        }
    }

    /// Constructs a [`Transfer`] for returning `payload` on `reply` channel.
    #[must_use]
    pub fn reply(channel: impl Into<String>, payload: &[u8]) -> Self {
        Self {
            target: Target::Reply(channel.into()),
            operation: Operation::Reply,
            sha256: sha256_hex(payload),
            bytes: payload.len() as u64,
        }
    }

    /// Returns `true` when this [`Transfer`] has a well-formed target, operation, and 64-hex digest.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        if self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return false;
        }
        match (&self.target, &self.operation) {
            (Target::Path(p), Operation::Write | Operation::CreateDir | Operation::RemoveTree) => {
                !p.as_os_str().is_empty()
            }
            (Target::Path(p), Operation::RenameTree { to }) => {
                !p.as_os_str().is_empty() && !to.as_os_str().is_empty()
            }
            (Target::Endpoint(u), Operation::Post) => !u.is_empty(),
            (Target::Reply(r), Operation::Reply) => !r.is_empty(),
            _ => false,
        }
    }
}

/// Complete outbound delivery passed to [`crate::egress::check`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivery {
    /// Bound transfer entries (files, directories, endpoints, or replies).
    pub entries: Vec<Transfer>,
    /// Markings detected in the outbound text via `labels::markings_in` (`None` if none found).
    pub markings: Option<LabelSet>,
}

impl Delivery {
    /// Constructs a new [`Delivery`] with `entries` and optional detected `markings`.
    #[must_use]
    pub const fn new(entries: Vec<Transfer>, markings: Option<LabelSet>) -> Self {
        Self { entries, markings }
    }

    /// Constructs a single-file write [`Delivery`] with optional `markings`.
    #[must_use]
    pub fn single_write(
        path: impl Into<PathBuf>,
        payload: &[u8],
        markings: Option<LabelSet>,
    ) -> Self {
        Self {
            entries: vec![Transfer::write_path(path, payload)],
            markings,
        }
    }
}

/// Hashes a sorted manifest of `(relative_path, sha256, byte_len)` entries for tree operations
/// (`Operation::RemoveTree`, `Operation::RenameTree`), returning `(manifest_sha256, total_bytes)`.
#[must_use]
pub fn hash_tree_manifest(entries: &[(String, String, u64)]) -> (String, u64) {
    let mut sorted = entries.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = Sha256Stream::new();
    hasher.update(b"fieldbook-tree-manifest-1\n");
    let mut total_bytes = 0u64;
    for (rel, sha, len) in &sorted {
        hasher.update(&(rel.len() as u64).to_be_bytes());
        hasher.update(rel.as_bytes());
        hasher.update(sha.as_bytes());
        hasher.update(&len.to_be_bytes());
        total_bytes = total_bytes.saturating_add(*len);
    }
    (hasher.finish(), total_bytes)
}
