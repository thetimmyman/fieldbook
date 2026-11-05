//! [`FaultyWorld`]: fault-injecting [`World`] wrapper around [`TestWorld`] (`add-new-and-edition`).

use std::path::Path;

use fieldbook::{Audited, Error, StoreLease, World, WorldError};
use fieldbook_core::CivilDate;
use fieldbook_core::audit::{Now, Principal, Sha256};
use fieldbook_core::check::Tree;
use fieldbook_core::egress::Allowed;

use super::world::TestWorld;

/// Classification of the write failure injected by [`Fault::FailOnName`].
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
}

/// [`World`] test double that injects [`Fault`] on matching `write_file` calls and forwards
/// everything else to [`TestWorld`].
pub struct FaultyWorld {
    /// Underlying [`TestWorld`].
    pub inner: TestWorld,
    /// Configured fault.
    pub fault: Fault,
}

impl FaultyWorld {
    /// Construct a `FaultyWorld` wrapping `inner` with `fault`.
    #[must_use]
    pub const fn new(inner: TestWorld, fault: Fault) -> Self {
        Self { inner, fault }
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
        let Fault::FailOnName { name, kind } = self.fault;
        if path.file_name().and_then(|n| n.to_str()) == Some(name) {
            return match kind {
                FaultKind::Io => Err(Error::new(path, "injected I/O write fault")),
                FaultKind::InUse => Err(Error::InUse(path.to_path_buf())),
            };
        }
        self.inner.write_file(audited, path, bytes)
    }

    fn write_outside(
        &self,
        allowed: &Allowed,
        audited: &Audited,
        path: &Path,
        bytes: &[u8],
    ) -> Result<Sha256, WorldError> {
        let Fault::FailOnName { name, kind } = self.fault;
        if path.file_name().and_then(|n| n.to_str()) == Some(name) {
            return match kind {
                FaultKind::Io => Err(Error::new(path, "injected I/O write fault")),
                FaultKind::InUse => Err(Error::InUse(path.to_path_buf())),
            };
        }
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
