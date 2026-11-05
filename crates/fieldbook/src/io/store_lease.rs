//! Coherent store read/write lease over `state/store.lock` (`audit-journal`, `policy.md#consistent-reads`).

use std::path::{Path, PathBuf};

use fieldbook_core::audit::Sha256;

use crate::io::journal::{StoreLockGuard, same_path};

/// Lock mode held by a [`StoreLease`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseMode {
    /// Shared read lock on `state/store.lock`.
    Shared,
    /// Exclusive write lock on `state/store.lock`.
    Exclusive,
}

/// Generation state observed under a [`StoreLease`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenerationStatus {
    /// Committed generation hash (`state/pending.toml` is absent).
    Committed(Sha256),
    /// Unfinished write batch (`state/pending.toml` is present; source generation hash is carried).
    Pending(Sha256),
}

impl GenerationStatus {
    /// Return the underlying generation [`Sha256`].
    #[must_use]
    pub const fn sha256(&self) -> &Sha256 {
        match self {
            Self::Committed(h) | Self::Pending(h) => h,
        }
    }

    /// Return `true` when the store has an unfinished pending write (`state/pending.toml`).
    #[must_use]
    pub const fn is_pending(&self) -> bool {
        matches!(self, Self::Pending(_))
    }
}

/// Read or write guard over `state/store.lock` carrying root identity and generation state.
#[derive(Debug)]
pub struct StoreLease {
    tree_root: PathBuf,
    state_dir: PathBuf,
    mode: LeaseMode,
    generation: GenerationStatus,
    _guard: StoreLockGuard,
}

impl StoreLease {
    pub(crate) fn new(
        tree_root: PathBuf,
        state_dir: PathBuf,
        mode: LeaseMode,
        generation: GenerationStatus,
        guard: StoreLockGuard,
    ) -> Self {
        Self {
            tree_root,
            state_dir,
            mode,
            generation,
            _guard: guard,
        }
    }

    /// Return the Corpus root directory bound to this lease.
    #[must_use]
    pub fn tree_root(&self) -> &Path {
        &self.tree_root
    }

    /// Return the state folder bound to this lease.
    #[must_use]
    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    /// Return the [`LeaseMode`] of this lease.
    #[must_use]
    pub const fn mode(&self) -> LeaseMode {
        self.mode
    }

    /// Return `true` when this lease is an exclusive write guard.
    #[must_use]
    pub const fn is_exclusive(&self) -> bool {
        matches!(self.mode, LeaseMode::Exclusive)
    }

    /// Return `true` when the store is in a pending write state.
    #[must_use]
    pub const fn is_pending(&self) -> bool {
        self.generation.is_pending()
    }

    /// Return the [`GenerationStatus`] observed when this lease was acquired.
    #[must_use]
    pub const fn generation(&self) -> &GenerationStatus {
        &self.generation
    }

    /// Return the generation [`Sha256`].
    #[must_use]
    pub const fn generation_hash(&self) -> &Sha256 {
        self.generation.sha256()
    }

    /// Check whether this lease belongs to `tree_root` and `state_dir`.
    #[must_use]
    pub fn matches_roots(&self, tree_root: &Path, state_dir: &Path) -> bool {
        same_path(&self.tree_root, tree_root) && same_path(&self.state_dir, state_dir)
    }
}
