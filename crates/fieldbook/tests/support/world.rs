//! [`TestWorld`]: configurable [`World`] wrapper around [`Disk`] (`audit-journal`).

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use fieldbook::io::Disk;
use fieldbook::io::root::Roots;
use fieldbook::{Audited, StoreLease, World, WorldError};
use fieldbook_core::CivilDate;
use fieldbook_core::audit::{Now, Principal, Sha256};
use fieldbook_core::check::Tree;
use fieldbook_core::egress::Allowed;

const FIXTURE_TODAY: &str = include_str!("../../../../tests/fixtures/corpus-mini/TODAY");

/// Test implementation of [`World`] forwarding filesystem operations to [`Disk`] while
/// supplying a deterministic clock (`now`) and principal (`local`).
pub struct TestWorld {
    inner: Disk,
    state_dir: PathBuf,
    now: Mutex<Now>,
    principal: Mutex<Principal>,
    company_store: AtomicBool,
}

impl TestWorld {
    /// Construct a `TestWorld` rooted at `root` (`Disk::new(Roots::at_home(root, &root.join("state")))`),
    /// default `Now` at fixture `TODAY` + `43_200` seconds (`12:00:00Z`), and principal `local`.
    #[must_use]
    pub fn new(root: &Path) -> Self {
        let today =
            CivilDate::parse(FIXTURE_TODAY.trim()).expect("valid tests/fixtures/corpus-mini/TODAY");
        let now = Now::new(today, 43_200).expect("valid default Now");
        Self {
            inner: Disk::new(Roots::at_home(root, &root.join("state"))),
            state_dir: root.join(".fieldbook"),
            now: Mutex::new(now),
            principal: Mutex::new(Principal::local()),
            company_store: AtomicBool::new(false),
        }
    }

    /// Construct a `TestWorld` with an explicit state folder path.
    #[must_use]
    pub fn with_state(root: &Path, state_dir: &Path) -> Self {
        let today =
            CivilDate::parse(FIXTURE_TODAY.trim()).expect("valid tests/fixtures/corpus-mini/TODAY");
        let now = Now::new(today, 43_200).expect("valid default Now");
        Self {
            inner: Disk::with_state(root.to_path_buf(), state_dir.to_path_buf()),
            state_dir: state_dir.to_path_buf(),
            now: Mutex::new(now),
            principal: Mutex::new(Principal::local()),
            company_store: AtomicBool::new(false),
        }
    }

    /// Return the underlying [`Disk`] roots.
    #[must_use]
    pub const fn roots(&self) -> &Roots {
        self.inner.roots()
    }

    /// Override the current event timestamp returned by [`World::now`].
    pub fn set_now(&self, now: Now) {
        if let Ok(mut g) = self.now.lock() {
            *g = now;
        }
    }

    /// Override the actor principal returned by [`World::principal`].
    pub fn set_principal(&self, principal: Principal) {
        if let Ok(mut g) = self.principal.lock() {
            *g = principal;
        }
    }

    /// Override whether this `TestWorld` reports having a company store (`World::has_company_store`).
    pub fn set_company_store(&self, has_company_store: bool) {
        self.company_store
            .store(has_company_store, Ordering::Relaxed);
    }
}

impl World for TestWorld {
    fn read_tree(&self) -> Result<Tree, WorldError> {
        fieldbook::io::journal::read_tree_coherent(self.tree_root(), &self.state_dir)
    }

    fn write_lease(&self) -> Result<StoreLease, WorldError> {
        fieldbook::io::journal::acquire_write_lease(self.tree_root(), &self.state_dir)
    }

    fn read_tree_under(&self, lease: &StoreLease) -> Result<Tree, WorldError> {
        fieldbook::io::journal::read_tree_under_lease(self.tree_root(), &self.state_dir, lease)
    }

    fn write_file(
        &self,
        audited: &Audited,
        path: &Path,
        bytes: &[u8],
    ) -> Result<Sha256, WorldError> {
        self.inner.write_file(audited, path, bytes)
    }

    fn write_outside(
        &self,
        allowed: &Allowed,
        audited: &Audited,
        path: &Path,
        bytes: &[u8],
    ) -> Result<Sha256, WorldError> {
        self.inner.write_outside(allowed, audited, path, bytes)
    }

    fn has_company_store(&self) -> bool {
        self.company_store.load(Ordering::Relaxed)
    }

    fn today(&self) -> CivilDate {
        self.now
            .lock()
            .map(|n| n.date)
            .unwrap_or_else(|_| self.inner.today())
    }

    fn now(&self) -> Now {
        *self.now.lock().expect("now lock")
    }

    fn principal(&self) -> Principal {
        self.principal.lock().expect("principal lock").clone()
    }

    fn cwd(&self) -> &Path {
        self.inner.cwd()
    }

    fn tree_root(&self) -> &Path {
        self.inner.tree_root()
    }

    fn state_dir(&self) -> &Path {
        &self.state_dir
    }
}
