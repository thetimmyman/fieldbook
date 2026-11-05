//! [`FaultyWorld`]: fault-injecting [`World`] wrapper around [`TestWorld`] (`add-new-and-edition`,
//! `add-folder-mode`).

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use fieldbook::{Audited, Error, StoreLease, World, WorldError};
use fieldbook_core::CivilDate;
use fieldbook_core::audit::{Now, Principal, Sha256};
use fieldbook_core::check::Tree;
use fieldbook_core::egress::Allowed;

use super::world::TestWorld;

/// Classification of the write failure injected by [`Fault`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultKind {
    /// Fail with [`Error::Io`].
    Io,
    /// Fail with [`Error::InUse`].
    InUse,
}

/// Write fault specification for [`FaultyWorld`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// Fail any write whose target path's last component equals `name`.
    FailOnName {
        /// File basename that triggers the fault.
        name: &'static str,
        /// Error kind to return before writing anything.
        kind: FaultKind,
    },
    /// Fail the `nth` (1-based) write whose target path's last component equals `name`.
    FailOnNthName {
        /// File basename that triggers the counter and fault.
        name: &'static str,
        /// 1-based match count on which to fail (`1` fails on the first matching write).
        nth: usize,
        /// Error kind to return before writing anything on the `nth` match.
        kind: FaultKind,
    },
}

/// [`World`] test double that injects [`Fault`] on matching `write_file` calls and forwards
/// everything else to [`TestWorld`].
pub struct FaultyWorld {
    /// Underlying [`TestWorld`].
    pub inner: TestWorld,
    /// Configured fault.
    pub fault: Fault,
    matches: AtomicUsize,
}

impl FaultyWorld {
    /// Construct a `FaultyWorld` wrapping `inner` with `fault`.
    #[must_use]
    pub const fn new(inner: TestWorld, fault: Fault) -> Self {
        Self {
            inner,
            fault,
            matches: AtomicUsize::new(0),
        }
    }

    fn check_fault(&self, path: &Path) -> Result<(), WorldError> {
        let actual = path.file_name().and_then(|n| n.to_str());
        match self.fault {
            Fault::FailOnName { name, kind } => {
                if actual == Some(name) {
                    return Err(make_fault_error(path, kind));
                }
            }
            Fault::FailOnNthName { name, nth, kind } => {
                if actual == Some(name) {
                    let count = self.matches.fetch_add(1, Ordering::SeqCst) + 1;
                    if count == nth {
                        return Err(make_fault_error(path, kind));
                    }
                }
            }
        }
        Ok(())
    }
}

fn make_fault_error(path: &Path, kind: FaultKind) -> WorldError {
    match kind {
        FaultKind::Io => Error::new(path, "injected I/O write fault"),
        FaultKind::InUse => Error::InUse(path.to_path_buf()),
    }
}

impl World for FaultyWorld {
    fn read_tree(&self) -> Result<Tree, WorldError> {
        self.inner.read_tree()
    }

    fn write_lease(&self) -> Result<StoreLease, WorldError> {
        self.inner.write_lease()
    }

    fn read_tree_under(&self, lease: &StoreLease) -> Result<Tree, WorldError> {
        self.inner.read_tree_under(lease)
    }

    fn write_file(
        &self,
        audited: &Audited,
        path: &Path,
        bytes: &[u8],
    ) -> Result<Sha256, WorldError> {
        self.check_fault(path)?;
        self.inner.write_file(audited, path, bytes)
    }

    fn write_outside(
        &self,
        allowed: &Allowed,
        audited: &Audited,
        path: &Path,
        bytes: &[u8],
    ) -> Result<Sha256, WorldError> {
        self.check_fault(path)?;
        self.inner.write_outside(allowed, audited, path, bytes)
    }

    fn has_company_store(&self) -> bool {
        self.inner.has_company_store()
    }

    fn today(&self) -> CivilDate {
        self.inner.today()
    }

    fn now(&self) -> Now {
        self.inner.now()
    }

    fn principal(&self) -> Principal {
        self.inner.principal()
    }

    fn cwd(&self) -> &Path {
        self.inner.cwd()
    }

    fn tree_root(&self) -> &Path {
        self.inner.tree_root()
    }

    fn state_dir(&self) -> &Path {
        self.inner.state_dir()
    }
}
