//! [`TestWorld`]: configurable [`World`] wrapper around [`Disk`] (`audit-journal`).

use std::path::Path;
use std::sync::Mutex;

use fieldbook::io::Disk;
use fieldbook::{StoreLease, World, WorldError};
use fieldbook_core::CivilDate;
use fieldbook_core::audit::{Now, Principal};
use fieldbook_core::check::Tree;

const FIXTURE_TODAY: &str = include_str!("../../../../tests/fixtures/corpus-mini/TODAY");

/// Test implementation of [`World`] forwarding filesystem operations to [`Disk`] while
/// supplying a deterministic clock (`now`) and principal (`local`).
pub struct TestWorld {
    inner: Disk,
    now: Mutex<Now>,
    principal: Mutex<Principal>,
}

impl TestWorld {
    /// Construct a `TestWorld` rooted at `root` with state folder `root/.fieldbook`,
    /// default `Now` at fixture `TODAY` + `43_200` seconds (`12:00:00Z`), and principal `local`.
    #[must_use]
    pub fn new(root: &Path) -> Self {
        let today =
            CivilDate::parse(FIXTURE_TODAY.trim()).expect("valid tests/fixtures/corpus-mini/TODAY");
        let now = Now::new(today, 43_200).expect("valid default Now");
        Self {
            inner: Disk::new(root.to_path_buf()),
            now: Mutex::new(now),
            principal: Mutex::new(Principal::local()),
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
            now: Mutex::new(now),
            principal: Mutex::new(Principal::local()),
        }
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
}

impl World for TestWorld {
    fn read_tree(&self) -> Result<Tree, WorldError> {
        self.inner.read_tree()
    }

    fn write_lease(&self) -> Result<StoreLease, WorldError> {
        self.inner.write_lease()
    }

    fn read_tree_under(&self, lease: &StoreLease) -> Result<Tree, WorldError> {
        self.inner.read_tree_under(lease)
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
        self.inner.state_dir()
    }
}
