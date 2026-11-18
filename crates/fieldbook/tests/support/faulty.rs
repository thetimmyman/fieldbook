//! [`FaultyWorld`]: fault-injecting [`World`] wrapper around [`TestWorld`] (`add-new-and-edition`,
//! `add-folder-mode`).

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use fieldbook::{Audited, Error, StoreLease, World, WorldError};
use fieldbook_core::CivilDate;
use fieldbook_core::audit::{Now, Principal, Sha256};
use fieldbook_core::check::Tree;
use fieldbook_core::egress::Allowed;
use fieldbook_core::labels::PlaceFacts;

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
    /// Fail every prompt write, as a broken standard error does.
    BrokenPrompt,
    /// Append `append` to the file at root-relative `path` once, right after the first locked
    /// tree read, as an editor save landing between planning and writing does.
    EditAfterTreeRead {
        /// Root-relative path of the file an editor saves.
        path: &'static str,
        /// Text the editor appends.
        append: &'static str,
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

    /// Forward fault arming to inner world.
    pub fn set_fault(&self, fault: Option<Fault>) {
        self.inner.set_fault(fault);
    }

    /// Forward rename fault injection to inner world.
    pub fn inject_rename_fault(&self, target: &Path, snapshot: fieldbook_core::backup::Number) {
        self.inner.inject_rename_fault(target, snapshot);
    }

    /// Forward rename fault clearing to inner world.
    pub fn clear_rename_fault(&self, target: &Path, snapshot: fieldbook_core::backup::Number) {
        self.inner.clear_rename_fault(target, snapshot);
    }

    fn check_fault(&self, path: &Path) -> Result<(), WorldError> {
        let actual = path.file_name().and_then(|n| n.to_str());
        match self.fault {
            Fault::FailOnName { name, kind } => {
                if actual == Some(name) {
                    return Err(make_fault_error(path, kind));
                }
            }
            Fault::BrokenPrompt | Fault::EditAfterTreeRead { .. } => {}
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
        let tree = self.inner.read_tree_under(lease)?;
        if let Fault::EditAfterTreeRead { path, append } = self.fault
            && self.matches.fetch_add(1, Ordering::SeqCst) == 0
        {
            use std::io::Write as _;
            let target = self.inner.tree_root().join(path);
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&target)
                .map_err(|e| Error::new(&target, e.to_string()))?;
            file.write_all(append.as_bytes())
                .map_err(|e| Error::new(&target, e.to_string()))?;
        }
        Ok(tree)
    }

    fn read_file(&self, path: &Path) -> Result<Option<Vec<u8>>, WorldError> {
        self.inner.read_file(path)
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

    fn effective_policy(&self) -> fieldbook_core::policy::EffectivePolicy {
        self.inner.effective_policy()
    }

    fn stdin_is_terminal(&self) -> bool {
        self.inner.stdin_is_terminal()
    }

    fn read_line(&self) -> String {
        self.inner.read_line()
    }

    fn prompt(&self, flag: &str) -> Result<(), WorldError> {
        if self.fault == Fault::BrokenPrompt {
            return Err(WorldError::new("stderr", "injected prompt write fault"));
        }
        self.inner.prompt(flag)
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

    fn place(&self, path: &Path) -> PlaceFacts {
        self.inner.place(path)
    }

    fn support_facts(&self) -> fieldbook::SupportFacts {
        self.inner.support_facts()
    }

    fn reset_drill(&self, audited: &Audited) -> Result<(), WorldError> {
        self.inner.reset_drill(audited)
    }

    fn remove_drill(&self, audited: &Audited) -> Result<(), WorldError> {
        self.inner.remove_drill(audited)
    }

    fn store_id(&self) -> &'static str {
        self.inner.store_id()
    }

    fn read_maintenance(&self) -> Result<String, WorldError> {
        self.inner.read_maintenance()
    }

    fn read_state_summary(
        &self,
        today: fieldbook_core::CivilDate,
    ) -> Result<fieldbook_core::status::StateSummary, WorldError> {
        self.inner.read_state_summary(today)
    }
}
