//! [`TestWorld`]: configurable [`World`] wrapper around [`Disk`] (`audit-journal`).

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use fieldbook::io::Disk;
use fieldbook::io::root::Roots;
use fieldbook::{Audited, StoreLease, World, WorldError};
use fieldbook_core::CivilDate;
use fieldbook_core::audit::{Now, Principal, Sha256};
use fieldbook_core::check::Tree;
use fieldbook_core::egress::Allowed;
use fieldbook_core::labels::PlaceFacts;

const FIXTURE_TODAY: &str = include_str!("../../../../tests/fixtures/corpus-mini/TODAY");

/// Test implementation of [`World`] forwarding filesystem operations to [`Disk`] while
/// supplying a deterministic clock (`now`) and principal (`local`).
pub struct TestWorld {
    inner: Disk,
    state_dir: PathBuf,
    now: Mutex<Now>,
    principal: Mutex<Principal>,
    company_store: AtomicBool,
    stdin_terminal: AtomicBool,
    input: Mutex<VecDeque<String>>,
    input_ready: Condvar,
    wait_for_input: AtomicBool,
    prompts: Mutex<Vec<String>>,
    prompt_ready: Condvar,
    place: Mutex<BTreeMap<PathBuf, PlaceFacts>>,
    support_facts: Mutex<Option<fieldbook::SupportFacts>>,
    fault: Mutex<Option<crate::support::faulty::Fault>>,
    fault_matches: std::sync::atomic::AtomicUsize,
    written: Mutex<Vec<PathBuf>>,
    before_write_lease: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    fail_remove_drill: AtomicBool,
    maintenance: Mutex<Option<String>>,
    state_summary: Mutex<Option<fieldbook_core::status::StateSummary>>,
    effective_policy: Mutex<Option<fieldbook_core::policy::EffectivePolicy>>,
}

/// How long a blocking `read_line` or `wait_for_prompts` waits before giving up.
pub const INTERACTION_TIMEOUT: Duration = Duration::from_secs(20);

/// A stand-in for standard error: it keeps what was written and publishes it, as one flushed
/// prompt, only when the writer flushes.
struct FlushRecorder<'w> {
    world: &'w TestWorld,
    buffer: Vec<u8>,
}

impl std::io::Write for FlushRecorder<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.buffer.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let flushed = String::from_utf8_lossy(&std::mem::take(&mut self.buffer)).into_owned();
        if let Ok(mut prompts) = self.world.prompts.lock() {
            prompts.push(flushed);
        }
        self.world.prompt_ready.notify_all();
        Ok(())
    }
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
            stdin_terminal: AtomicBool::new(false),
            input: Mutex::new(VecDeque::new()),
            input_ready: Condvar::new(),
            wait_for_input: AtomicBool::new(false),
            prompts: Mutex::new(Vec::new()),
            prompt_ready: Condvar::new(),
            place: Mutex::new(BTreeMap::new()),
            support_facts: Mutex::new(None),
            fault: Mutex::new(None),
            fault_matches: std::sync::atomic::AtomicUsize::new(0),
            written: Mutex::new(Vec::new()),
            before_write_lease: Mutex::new(None),
            fail_remove_drill: AtomicBool::new(false),
            maintenance: Mutex::new(None),
            state_summary: Mutex::new(None),
            effective_policy: Mutex::new(None),
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
            stdin_terminal: AtomicBool::new(false),
            input: Mutex::new(VecDeque::new()),
            input_ready: Condvar::new(),
            wait_for_input: AtomicBool::new(false),
            prompts: Mutex::new(Vec::new()),
            prompt_ready: Condvar::new(),
            place: Mutex::new(BTreeMap::new()),
            support_facts: Mutex::new(None),
            fault: Mutex::new(None),
            fault_matches: std::sync::atomic::AtomicUsize::new(0),
            written: Mutex::new(Vec::new()),
            before_write_lease: Mutex::new(None),
            fail_remove_drill: AtomicBool::new(false),
            maintenance: Mutex::new(None),
            state_summary: Mutex::new(None),
            effective_policy: Mutex::new(None),
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

    /// Set whether standard input counts as a terminal (default `false`).
    pub fn set_stdin_terminal(&self, terminal: bool) {
        self.stdin_terminal.store(terminal, Ordering::Relaxed);
    }

    /// Queue one line for `read_line`.
    pub fn push_input_line(&self, line: &str) {
        if let Ok(mut input) = self.input.lock() {
            input.push_back(line.to_owned());
        }
        self.input_ready.notify_all();
    }

    /// Make `read_line` wait (up to [`INTERACTION_TIMEOUT`]) for a queued line instead of
    /// returning an empty line at once.
    pub fn set_wait_for_input(&self, wait: bool) {
        self.wait_for_input.store(wait, Ordering::Relaxed);
    }

    /// The prompts written and flushed so far, in order.
    #[must_use]
    pub fn prompts(&self) -> Vec<String> {
        self.prompts.lock().map(|p| p.clone()).unwrap_or_default()
    }

    /// Wait until `count` prompts have been flushed; `false` when [`INTERACTION_TIMEOUT`] passes
    /// first.
    #[must_use]
    pub fn wait_for_prompts(&self, count: usize) -> bool {
        let deadline = Instant::now() + INTERACTION_TIMEOUT;
        let Ok(mut prompts) = self.prompts.lock() else {
            return false;
        };
        while prompts.len() < count {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                return false;
            };
            match self.prompt_ready.wait_timeout(prompts, left) {
                Ok((guard, _)) => prompts = guard,
                Err(_) => return false,
            }
        }
        true
    }

    /// Override whether this `TestWorld` reports having a company store (`World::has_company_store`).
    pub fn set_company_store(&self, has_company_store: bool) {
        self.company_store
            .store(has_company_store, Ordering::Relaxed);
    }

    /// Set placement facts for `path` in this test world.
    pub fn set_place(&self, path: &Path, facts: PlaceFacts) {
        if let Ok(mut map) = self.place.lock() {
            map.insert(path.to_path_buf(), facts);
        }
    }

    /// Override the support facts returned by [`World::support_facts`].
    pub fn set_support_facts(&self, facts: fieldbook::SupportFacts) {
        if let Ok(mut g) = self.support_facts.lock() {
            *g = Some(facts);
        }
    }

    /// Set synthetic support catalogue facts for [`World::support_facts`].
    pub fn set_support_catalogue(
        &self,
        cat: fieldbook_core::manifest::SupportCatalogue,
        sha256: impl Into<String>,
    ) {
        let sha = sha256.into();
        let facts = fieldbook::SupportFacts::new(vec![(cat, sha)], BTreeMap::new());
        self.set_support_facts(facts);
    }

    /// Arm or disarm a fault injection on this test world.
    pub fn set_fault(&self, fault: Option<crate::support::faulty::Fault>) {
        if let Ok(mut g) = self.fault.lock() {
            *g = fault;
        }
        self.fault_matches.store(0, Ordering::SeqCst);
    }

    /// Injects a rename blocker directory at `target/fieldbook-backup-NNNN`.
    pub fn inject_rename_fault(&self, target: &Path, snapshot: fieldbook_core::backup::Number) {
        let final_dir = target.join(format!("fieldbook-backup-{snapshot:04}"));
        std::fs::create_dir_all(&final_dir).expect("create rename blocker dir");
        std::fs::write(final_dir.join("blocker.txt"), b"blocker\n").expect("write blocker file");
    }

    /// Clears the rename blocker directory at `target/fieldbook-backup-NNNN`.
    pub fn clear_rename_fault(&self, target: &Path, snapshot: fieldbook_core::backup::Number) {
        let final_dir = target.join(format!("fieldbook-backup-{snapshot:04}"));
        let _ = std::fs::remove_dir_all(&final_dir);
    }

    /// Return in order every path written by a successful `write_file` or `write_outside`.
    #[must_use]
    pub fn written(&self) -> Vec<PathBuf> {
        self.written.lock().map(|w| w.clone()).unwrap_or_default()
    }

    /// Run `action` once, just before the next `write_lease` is acquired, as another process
    /// acting between a handler's first look and its lease does.
    pub fn set_before_write_lease(&self, action: impl FnOnce() + Send + 'static) {
        if let Ok(mut g) = self.before_write_lease.lock() {
            *g = Some(Box::new(action));
        }
    }

    /// Make every `remove_drill` fail without touching the folder, as a file held open does.
    pub fn set_fail_remove_drill(&self, fail: bool) {
        self.fail_remove_drill.store(fail, Ordering::Relaxed);
    }

    /// Override the text [`World::read_maintenance`] returns; `None` forwards to the disk.
    pub fn set_maintenance(&self, text: &str) {
        if let Ok(mut g) = self.maintenance.lock() {
            *g = Some(text.to_owned());
        }
    }

    /// Override the summary [`World::read_state_summary`] returns; `None` forwards to the disk.
    pub fn set_state_summary(&self, summary: fieldbook_core::status::StateSummary) {
        if let Ok(mut g) = self.state_summary.lock() {
            *g = Some(summary);
        }
    }

    /// Override the policy [`World::effective_policy`] returns; `None` uses the floor for this layout.
    pub fn set_effective_policy(&self, policy: fieldbook_core::policy::EffectivePolicy) {
        if let Ok(mut g) = self.effective_policy.lock() {
            *g = Some(policy);
        }
    }

    fn check_fault(&self, path: &Path) -> Result<(), WorldError> {
        let Ok(g) = self.fault.lock() else {
            return Ok(());
        };
        let Some(fault) = g.as_ref() else {
            return Ok(());
        };
        let actual = path.file_name().and_then(|n| n.to_str());
        match fault {
            crate::support::faulty::Fault::FailOnName { name, kind } => {
                if actual == Some(*name) {
                    return Err(match kind {
                        crate::support::faulty::FaultKind::Io => {
                            fieldbook::Error::new(path, "injected I/O write fault")
                        }
                        crate::support::faulty::FaultKind::InUse => {
                            fieldbook::Error::InUse(path.to_path_buf())
                        }
                    });
                }
            }
            crate::support::faulty::Fault::FailOnNthName { name, nth, kind } => {
                if actual == Some(*name) {
                    let count = self.fault_matches.fetch_add(1, Ordering::SeqCst) + 1;
                    if count == *nth {
                        return Err(match kind {
                            crate::support::faulty::FaultKind::Io => {
                                fieldbook::Error::new(path, "injected I/O write fault")
                            }
                            crate::support::faulty::FaultKind::InUse => {
                                fieldbook::Error::InUse(path.to_path_buf())
                            }
                        });
                    }
                }
            }
            crate::support::faulty::Fault::BrokenPrompt
            | crate::support::faulty::Fault::EditAfterTreeRead { .. } => {}
        }
        Ok(())
    }
}

impl World for TestWorld {
    fn read_tree(&self) -> Result<Tree, WorldError> {
        fieldbook::io::journal::read_tree_coherent(self.tree_root(), &self.state_dir)
    }

    fn write_lease(&self) -> Result<StoreLease, WorldError> {
        let action = self
            .before_write_lease
            .lock()
            .ok()
            .and_then(|mut g| g.take());
        if let Some(action) = action {
            action();
        }
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
        self.check_fault(path)?;
        let sha = self.inner.write_file(audited, path, bytes)?;
        if let Ok(mut w) = self.written.lock() {
            w.push(path.to_path_buf());
        }
        Ok(sha)
    }

    fn write_outside(
        &self,
        allowed: &Allowed,
        audited: &Audited,
        path: &Path,
        bytes: &[u8],
    ) -> Result<Sha256, WorldError> {
        self.check_fault(path)?;
        let sha = self.inner.write_outside(allowed, audited, path, bytes)?;
        if let Ok(mut w) = self.written.lock() {
            w.push(path.to_path_buf());
        }
        Ok(sha)
    }

    fn has_company_store(&self) -> bool {
        self.company_store.load(Ordering::Relaxed)
    }

    fn stdin_is_terminal(&self) -> bool {
        self.stdin_terminal.load(Ordering::Relaxed)
    }

    fn read_line(&self) -> String {
        let deadline = Instant::now() + INTERACTION_TIMEOUT;
        let Ok(mut input) = self.input.lock() else {
            return String::new();
        };
        loop {
            if let Some(line) = input.pop_front() {
                return line;
            }
            if !self.wait_for_input.load(Ordering::Relaxed) {
                return String::new();
            }
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                return String::new();
            };
            match self.input_ready.wait_timeout(input, left) {
                Ok((guard, _)) => input = guard,
                Err(_) => return String::new(),
            }
        }
    }

    fn prompt(&self, flag: &str) -> Result<(), WorldError> {
        let mut recorder = FlushRecorder {
            world: self,
            buffer: Vec::new(),
        };
        fieldbook::io::write_prompt_to(&mut recorder, flag)
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

    fn place(&self, path: &Path) -> PlaceFacts {
        if let Ok(map) = self.place.lock() {
            map.get(path).copied().unwrap_or_else(PlaceFacts::local)
        } else {
            PlaceFacts::local()
        }
    }

    fn support_facts(&self) -> fieldbook::SupportFacts {
        if let Ok(g) = self.support_facts.lock()
            && let Some(facts) = g.as_ref()
        {
            return facts.clone();
        }
        let disk = Disk::with_state(self.tree_root(), &self.state_dir);
        disk.support_facts()
    }

    fn reset_drill(&self, audited: &Audited) -> Result<(), WorldError> {
        self.inner.reset_drill(audited)
    }

    fn remove_drill(&self, audited: &Audited) -> Result<(), WorldError> {
        if self.fail_remove_drill.load(Ordering::Relaxed) {
            return Err(fieldbook::Error::new(
                self.state_dir.join("drill"),
                "injected drill removal fault",
            ));
        }
        self.inner.remove_drill(audited)
    }

    /// Return the test double store registry id.
    ///
    /// Not a registry id; the drift test skips it.
    fn store_id(&self) -> &'static str {
        "memory@0"
    }

    fn effective_policy(&self) -> fieldbook_core::policy::EffectivePolicy {
        if let Ok(g) = self.effective_policy.lock()
            && let Some(policy) = g.as_ref()
        {
            return policy.clone();
        }
        fieldbook_core::policy::EffectivePolicy::floor(self.has_company_store())
    }

    fn read_maintenance(&self) -> Result<String, WorldError> {
        if let Ok(g) = self.maintenance.lock()
            && let Some(text) = g.as_ref()
        {
            return Ok(text.clone());
        }
        self.inner.read_maintenance()
    }

    fn read_state_summary(
        &self,
        today: fieldbook_core::CivilDate,
    ) -> Result<fieldbook_core::status::StateSummary, WorldError> {
        if let Ok(g) = self.state_summary.lock()
            && let Some(summary) = g.as_ref()
        {
            return Ok(summary.clone());
        }
        Ok(fieldbook::io::state_summary(&self.state_dir, today))
    }
}
