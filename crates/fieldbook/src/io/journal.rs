//! Append-only hash-chained audit journal, file locking, coherent store leases, and crash
//! recovery (`audit-journal`, AD-16, ADR 0004 §12).

#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "io::journal is the authorized writer for state/audit/, state/lock, state/store.lock, state/pending.toml and state/generation"
)]

use std::collections::BTreeMap;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::ThreadId;
use std::time::{Duration, Instant};

use fieldbook_core::audit::{
    Actor, Count, DestKind, Event, EventEnvelope, EventPayload, Kind, Now, OperationIntent,
    OperationKey, PendingFile, PendingKey, PendingOperation, PendingWrite, RecordRef, RefuseAbout,
    SemVer, Sha256, StoreId, Verb, Via, Word, WriteSet, ZERO_HASH, completed_after_hash,
    compute_file_hashes, normalize_affected, split_hash_suffix, verify_line,
};
use fieldbook_core::check::{Code, Tree};
use fieldbook_core::egress::{Allowed, Operation, Target};

use crate::World;
use crate::io::WorldError;
use crate::io::store_lease::{GenerationStatus, LeaseMode, StoreLease};

/// Default segment rotation threshold (8 MiB).
pub const DEFAULT_SEGMENT_LIMIT: u64 = 8 * 1024 * 1024;

/// Default lock acquisition timeout (3 seconds).
pub const DEFAULT_LOCK_TIMEOUT: Duration = Duration::from_millis(3000);

///Retry interval while waiting for the journal lock (100 ms).
pub const JOURNAL_RETRY_INTERVAL: Duration = Duration::from_millis(100);

/// Retry interval while waiting for the store lock (50 ms).
pub const STORE_RETRY_INTERVAL: Duration = Duration::from_millis(50);

/// Verbs whose writes all land outside the content store (`backup-snapshot`).
pub const OUTSIDE_STORE_VERBS: &[&str] = &["build"];

/// Errors raised by [`Journal`] operations (mapped to exit code 3 at the CLI boundary).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JournalError {
    /// Another process or transaction holds the journal lock past the retry timeout.
    Busy,
    /// The journal hash chain or tail line is broken and refuses appends.
    BrokenChain(String),
    /// A verb string passed to `Journal::refuse` is not a valid [`Verb`].
    InvalidVerb(String),
    /// The supplied [`StoreLease`] is shared, belongs to foreign roots, or violates lock ordering.
    InvalidLease(String),
    /// The pending write descriptor (`state/pending.toml`) is malformed or mismatches recovery.
    InvalidPending(String),
    /// `settings.toml` could not be parsed or has an invalid `role`.
    Settings(String),
    /// A vocabulary or write-set invariant failed.
    Vocab(String),
    /// Underlying filesystem I/O failure.
    Io {
        /// Path that failed.
        path: PathBuf,
        /// Error description.
        message: String,
    },
}

impl JournalError {
    /// Construct a [`JournalError::Io`] for `path` and `message`.
    #[must_use]
    pub fn io(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self::Io {
            path: path.into(),
            message: message.into(),
        }
    }

    /// CLI exit code (`3`) for runtime/I/O/journal errors.
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        3
    }
}

impl fmt::Display for JournalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy => f.write_str("the journal is in use by another fieldbook process"),
            Self::BrokenChain(msg) => write!(f, "broken audit journal chain: {msg}"),
            Self::InvalidVerb(text) => write!(f, "not a verb word: {text}"),
            Self::InvalidLease(msg) => write!(f, "invalid store lease: {msg}"),
            Self::InvalidPending(msg) => write!(f, "pending write error: {msg}"),
            Self::Settings(msg) | Self::Vocab(msg) => f.write_str(msg),
            Self::Io { path, message } => write!(f, "{}: {}", path.display(), message),
        }
    }
}

impl std::error::Error for JournalError {}

/// Outcome of [`Journal::recover`].
#[derive(Debug)]
pub enum Recovery {
    /// No pending write existed (or a pre-begin fault with unchanged before-hashes was cleaned up).
    None,
    /// The original `write.begin` is open and live files match before/after hashes; the caller
    /// must finish writing remaining files and call [`Journal::end`].
    Resume {
        /// Retained transaction token for the original `write.begin`.
        audited: Audited,
        /// Original operation intent (preserving allocated IDs and planning date).
        intent: OperationIntent,
    },
    /// All planned files already matched their `after_sha256`; bookkeeping (`write.end`,
    /// `state/generation`, and removal of `state/pending.toml`) has been completed.
    Completed {
        /// Original operation intent.
        intent: OperationIntent,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InProcessLockMode {
    Shared(usize),
    Exclusive,
}

#[derive(Default)]
struct LockRegistry {
    active_files: BTreeMap<PathBuf, InProcessLockMode>,
    thread_journal_locks: Vec<((ThreadId, PathBuf), usize)>,
}

fn lock_registry() -> &'static Mutex<LockRegistry> {
    static REGISTRY: OnceLock<Mutex<LockRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(LockRegistry::default()))
}

type InitPauseHook = Arc<dyn Fn(&Path) + Send + Sync>;

#[derive(Default)]
struct TestHooks {
    init_pause: Option<InitPauseHook>,
    no_state_read_pause: Option<InitPauseHook>,
    write_pause: Vec<(ThreadId, InitPauseHook)>,
    lock_timeout: Vec<(ThreadId, Duration)>,
    fail_before_begin_append: Option<ThreadId>,
    fail_after_end_append: Option<ThreadId>,
    fail_on_settings_change_append: Option<ThreadId>,
}

fn test_hooks() -> &'static Mutex<TestHooks> {
    static HOOKS: OnceLock<Mutex<TestHooks>> = OnceLock::new();
    HOOKS.get_or_init(|| Mutex::new(TestHooks::default()))
}

/// Set or clear a pause hook called inside `init_state_locks` after creating `state/` and
/// `state/.init` and before creating `state/store.lock` (used by `store_generation` tests).
pub fn set_init_pause_hook(hook: Option<InitPauseHook>) {
    if let Ok(mut guard) = test_hooks().lock() {
        guard.init_pause = hook;
    }
}

/// Set or clear a hook called inside the no-state reader path right after reading the initial
/// snapshot and before confirming `state/` is still absent.
pub fn set_no_state_read_hook(hook: Option<InitPauseHook>) {
    if let Ok(mut guard) = test_hooks().lock() {
        guard.no_state_read_pause = hook;
    }
}

/// Set or clear a pause hook for the current test thread, called inside `io::atomic` right
/// before writing a resolved target (used by `egress_doors` tests).
pub fn set_write_pause_hook(hook: Option<InitPauseHook>) {
    let tid = std::thread::current().id();
    if let Ok(mut guard) = test_hooks().lock() {
        guard.write_pause.retain(|(t, _)| *t != tid);
        if let Some(h) = hook {
            guard.write_pause.push((tid, h));
        }
    }
}

pub(crate) fn run_write_pause_hook(path: &Path) {
    let tid = std::thread::current().id();
    let hook = test_hooks().lock().ok().and_then(|g| {
        g.write_pause
            .iter()
            .find(|(t, _)| *t == tid)
            .map(|(_, h)| h.clone())
    });
    if let Some(h) = hook {
        h(path);
    }
}

/// Override the lock acquisition timeout for the current test thread (`None` restores the 3 s default).
pub fn set_lock_timeout_for_test(timeout: Option<Duration>) {
    let tid = std::thread::current().id();
    if let Ok(mut guard) = test_hooks().lock() {
        guard.lock_timeout.retain(|(t, _)| *t != tid);
        if let Some(dur) = timeout {
            guard.lock_timeout.push((tid, dur));
        }
    }
}

/// Arm or disarm a one-shot fault on the current thread right after `state/pending.toml` is written
/// and before `write.begin` is appended (`intent_faults_before_begin_and_after_end_recover`).
pub fn set_fail_before_begin_append(enabled: bool) {
    let tid = std::thread::current().id();
    if let Ok(mut guard) = test_hooks().lock() {
        guard.fail_before_begin_append = enabled.then_some(tid);
    }
}

/// Arm or disarm a one-shot fault on the current thread right after `write.end` is appended and before
/// `state/generation` and `state/pending.toml` are updated (`intent_faults_before_begin_and_after_end_recover`).
pub fn set_fail_after_end_append(enabled: bool) {
    let tid = std::thread::current().id();
    if let Ok(mut guard) = test_hooks().lock() {
        guard.fail_after_end_append = enabled.then_some(tid);
    }
}

/// Arm or disarm a one-shot fault on the current thread when appending `settings.change`
/// (`settings_baseline_append_failure_keeps_previous`).
pub fn set_fail_on_settings_change_append(enabled: bool) {
    let tid = std::thread::current().id();
    if let Ok(mut guard) = test_hooks().lock() {
        guard.fail_on_settings_change_append = enabled.then_some(tid);
    }
}

fn current_lock_timeout() -> Duration {
    let tid = std::thread::current().id();
    test_hooks()
        .lock()
        .ok()
        .and_then(|g| {
            g.lock_timeout
                .iter()
                .find(|(t, _)| *t == tid)
                .map(|(_, d)| *d)
        })
        .unwrap_or(DEFAULT_LOCK_TIMEOUT)
}

/// Normalize a path lexically (resolving `.` and `..` without requiring the path to exist).
#[must_use]
pub fn normalize_path(path: &Path) -> PathBuf {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else if let Ok(cwd) = std::env::current_dir() {
        cwd.join(path)
    } else {
        path.to_path_buf()
    };
    let mut out = PathBuf::new();
    for comp in abs.components() {
        match comp {
            Component::Prefix(p) => out.push(p.as_os_str()),
            Component::RootDir => out.push(comp.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                let _ = out.pop();
            }
            Component::Normal(part) => out.push(part),
        }
    }
    out
}

/// Compare two filesystem paths after lexical normalization.
#[must_use]
pub fn same_path(a: &Path, b: &Path) -> bool {
    normalize_path(a) == normalize_path(b)
}

/// Guard holding a shared or exclusive lock on `state/store.lock`.
#[derive(Debug)]
pub struct StoreLockGuard {
    lock_path: PathBuf,
    file: File,
}

impl Drop for StoreLockGuard {
    fn drop(&mut self) {
        let _ = self.file.unlock();
        if let Ok(mut reg) = lock_registry().lock() {
            match reg.active_files.get_mut(&self.lock_path) {
                Some(InProcessLockMode::Exclusive) => {
                    reg.active_files.remove(&self.lock_path);
                }
                Some(InProcessLockMode::Shared(count)) => {
                    if *count <= 1 {
                        reg.active_files.remove(&self.lock_path);
                    } else {
                        *count -= 1;
                    }
                }
                None => {}
            }
        }
    }
}

/// Guard holding an exclusive lock on `state/lock`.
#[derive(Debug)]
struct JournalLockGuard {
    lock_path: PathBuf,
    state_key: PathBuf,
    thread_id: ThreadId,
    file: File,
}

impl Drop for JournalLockGuard {
    fn drop(&mut self) {
        let _ = self.file.unlock();
        if let Ok(mut reg) = lock_registry().lock() {
            reg.active_files.remove(&self.lock_path);
            let key = (self.thread_id, self.state_key.clone());
            if let Some(pos) = reg.thread_journal_locks.iter().position(|(k, _)| *k == key) {
                if reg.thread_journal_locks[pos].1 <= 1 {
                    reg.thread_journal_locks.swap_remove(pos);
                } else {
                    reg.thread_journal_locks[pos].1 -= 1;
                }
            }
        }
    }
}

fn check_store_precedes_journal(state_dir: &Path) -> Result<(), String> {
    let state_key = normalize_path(state_dir);
    let tid = std::thread::current().id();
    let key = (tid, state_key);
    if let Ok(reg) = lock_registry().lock()
        && reg
            .thread_journal_locks
            .iter()
            .any(|(k, c)| *k == key && *c > 0)
    {
        return Err(
            "lock order violation: cannot acquire store lease while holding journal lease"
                .to_owned(),
        );
    }
    Ok(())
}

/// Private initialization helper that creates a missing state folder and stable
/// `state/lock` and `state/store.lock` files before `store.lock` is acquired.
fn init_state_locks(state_dir: &Path) -> Result<(), WorldError> {
    let audit_dir = state_dir.join("audit");
    let journal_lock = state_dir.join("lock");
    let store_lock = state_dir.join("store.lock");
    let init_marker = state_dir.join(".init");

    if audit_dir.is_dir() && journal_lock.is_file() && store_lock.is_file() && !init_marker.exists()
    {
        return Ok(());
    }

    std::fs::create_dir_all(&audit_dir)
        .map_err(|err| WorldError::new(&audit_dir, err.to_string()))?;

    let _ = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&init_marker);

    OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&journal_lock)
        .map_err(|err| WorldError::new(&journal_lock, err.to_string()))?;

    let pause_hook = test_hooks().lock().ok().and_then(|g| g.init_pause.clone());
    if let Some(hook) = pause_hook {
        hook(state_dir);
    }

    OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&store_lock)
        .map_err(|err| WorldError::new(&store_lock, err.to_string()))?;

    let _ = std::fs::remove_file(&init_marker);
    Ok(())
}

fn acquire_store_lock(state_dir: &Path, exclusive: bool) -> Result<StoreLockGuard, WorldError> {
    check_store_precedes_journal(state_dir).map_err(|msg| WorldError::new(state_dir, msg))?;

    let lock_path = state_dir.join("store.lock");
    let lock_key = normalize_path(&lock_path);
    let timeout = current_lock_timeout();
    let start = Instant::now();

    loop {
        let open_res = if exclusive {
            OpenOptions::new().read(true).write(true).open(&lock_path)
        } else {
            OpenOptions::new().read(true).open(&lock_path)
        };
        let file = open_res.map_err(|err| WorldError::new(&lock_path, err.to_string()))?;

        let mut acquired_in_proc = false;
        if let Ok(mut reg) = lock_registry().lock() {
            match (exclusive, reg.active_files.get_mut(&lock_key)) {
                (true, None) => {
                    reg.active_files
                        .insert(lock_key.clone(), InProcessLockMode::Exclusive);
                    acquired_in_proc = true;
                }
                (false, None) => {
                    reg.active_files
                        .insert(lock_key.clone(), InProcessLockMode::Shared(1));
                    acquired_in_proc = true;
                }
                (false, Some(InProcessLockMode::Shared(n))) => {
                    *n += 1;
                    acquired_in_proc = true;
                }
                _ => {}
            }
        }

        if acquired_in_proc {
            let os_res = if exclusive {
                file.try_lock()
            } else {
                file.try_lock_shared()
            };
            match os_res {
                Ok(()) => {
                    return Ok(StoreLockGuard {
                        lock_path: lock_key,
                        file,
                    });
                }
                Err(_) => {
                    if let Ok(mut reg) = lock_registry().lock() {
                        match reg.active_files.get_mut(&lock_key) {
                            Some(InProcessLockMode::Exclusive) => {
                                reg.active_files.remove(&lock_key);
                            }
                            Some(InProcessLockMode::Shared(n)) => {
                                if *n <= 1 {
                                    reg.active_files.remove(&lock_key);
                                } else {
                                    *n -= 1;
                                }
                            }
                            None => {}
                        }
                    }
                }
            }
        }

        if start.elapsed() >= timeout {
            return Err(WorldError::new(
                &lock_path,
                "the store is in use by another fieldbook process",
            ));
        }
        std::thread::sleep(STORE_RETRY_INTERVAL);
    }
}

fn acquire_journal_lock(state_dir: &Path) -> Result<JournalLockGuard, JournalError> {
    let lock_path = state_dir.join("lock");
    let lock_key = normalize_path(&lock_path);
    let state_key = normalize_path(state_dir);
    let tid = std::thread::current().id();
    let timeout = current_lock_timeout();
    let start = Instant::now();

    loop {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|err| JournalError::io(&lock_path, err.to_string()))?;

        let mut acquired_in_proc = false;
        let tkey = (tid, state_key.clone());
        if let Ok(mut reg) = lock_registry().lock()
            && !reg.active_files.contains_key(&lock_key)
        {
            reg.active_files
                .insert(lock_key.clone(), InProcessLockMode::Exclusive);
            if let Some((_, c)) = reg
                .thread_journal_locks
                .iter_mut()
                .find(|(k, _)| *k == tkey)
            {
                *c += 1;
            } else {
                reg.thread_journal_locks.push((tkey.clone(), 1));
            }
            acquired_in_proc = true;
        }

        if acquired_in_proc {
            match file.try_lock() {
                Ok(()) => {
                    return Ok(JournalLockGuard {
                        lock_path: lock_key,
                        state_key,
                        thread_id: tid,
                        file,
                    });
                }
                Err(_) => {
                    if let Ok(mut reg) = lock_registry().lock() {
                        reg.active_files.remove(&lock_key);
                        if let Some(pos) = reg
                            .thread_journal_locks
                            .iter()
                            .position(|(k, _)| *k == tkey)
                        {
                            if reg.thread_journal_locks[pos].1 <= 1 {
                                reg.thread_journal_locks.swap_remove(pos);
                            } else {
                                reg.thread_journal_locks[pos].1 -= 1;
                            }
                        }
                    }
                }
            }
        }

        if start.elapsed() >= timeout {
            return Err(JournalError::Busy);
        }
        std::thread::sleep(JOURNAL_RETRY_INTERVAL);
    }
}

fn read_generation_status(
    tree_root: &Path,
    state_dir: &Path,
) -> Result<GenerationStatus, WorldError> {
    let pending_path = state_dir.join("pending.toml");
    let gen_path = state_dir.join("generation");

    if pending_path.exists() {
        let raw = std::fs::read_to_string(&pending_path)
            .map_err(|err| WorldError::new(&pending_path, err.to_string()))?;
        let pw = PendingWrite::parse_toml(&raw)
            .map_err(|err| WorldError::new(&pending_path, err.to_string()))?;
        return Ok(GenerationStatus::Pending(pw.source_generation));
    }

    if gen_path.exists() {
        let raw = std::fs::read_to_string(&gen_path)
            .map_err(|err| WorldError::new(&gen_path, err.to_string()))?;
        let sha =
            Sha256::parse(raw.trim()).map_err(|err| WorldError::new(&gen_path, err.to_string()))?;
        return Ok(GenerationStatus::Committed(sha));
    }

    let tree = crate::io::tree::read_tree(tree_root)?;
    Ok(GenerationStatus::Committed(tree.generation_hash()))
}

/// Acquire an exclusive [`StoreLease`] over `state/store.lock` after initializing missing state
/// lock files (`World::write_lease`).
///
/// # Errors
///
/// Returns [`WorldError`] if lock ordering is violated, state lock files cannot be initialized,
/// or the lock cannot be acquired within the timeout.
pub fn acquire_write_lease(tree_root: &Path, state_dir: &Path) -> Result<StoreLease, WorldError> {
    check_store_precedes_journal(state_dir).map_err(|msg| WorldError::new(state_dir, msg))?;
    init_state_locks(state_dir)?;
    let guard = acquire_store_lock(state_dir, true)?;
    let generation = match read_generation_status(tree_root, state_dir) {
        Ok(gen_status) => gen_status,
        Err(_) => {
            let tree = crate::io::tree::read_tree(tree_root)?;
            GenerationStatus::Pending(tree.generation_hash())
        }
    };
    Ok(StoreLease::new(
        tree_root.to_path_buf(),
        state_dir.to_path_buf(),
        LeaseMode::Exclusive,
        generation,
        guard,
    ))
}

/// Acquire a shared [`StoreLease`] over an existing `state/store.lock` without creating state.
///
/// # Errors
///
/// Returns [`WorldError`] if lock ordering is violated, `store.lock` cannot be locked, or
/// the store is in a pending state.
pub fn acquire_read_lease(tree_root: &Path, state_dir: &Path) -> Result<StoreLease, WorldError> {
    check_store_precedes_journal(state_dir).map_err(|msg| WorldError::new(state_dir, msg))?;
    let guard = acquire_store_lock(state_dir, false)?;
    let generation = read_generation_status(tree_root, state_dir)?;
    if generation.is_pending() {
        return Err(WorldError::new(
            state_dir.join("pending.toml"),
            "store has an unfinished pending write that requires recovery",
        ));
    }
    Ok(StoreLease::new(
        tree_root.to_path_buf(),
        state_dir.to_path_buf(),
        LeaseMode::Shared,
        generation,
        guard,
    ))
}

/// Read a coherent [`Tree`] snapshot using a shared [`StoreLease`] or the no-state retry protocol
/// (`World::read_tree`). Never creates a state folder or lock files.
///
/// # Errors
///
/// Returns [`WorldError`] if lock ordering is violated, initialization is incomplete, a pending
/// write requires recovery, or the filesystem cannot be read.
pub fn read_tree_coherent(tree_root: &Path, state_dir: &Path) -> Result<Tree, WorldError> {
    check_store_precedes_journal(state_dir).map_err(|msg| WorldError::new(state_dir, msg))?;

    let timeout = current_lock_timeout();
    let start = Instant::now();
    let init_marker = state_dir.join(".init");
    let store_lock = state_dir.join("store.lock");
    let pending_path = state_dir.join("pending.toml");
    let gen_path = state_dir.join("generation");

    loop {
        if !state_dir.exists() {
            let tree = crate::io::tree::read_tree(tree_root)?;
            let hook = test_hooks()
                .lock()
                .ok()
                .and_then(|g| g.no_state_read_pause.clone());
            if let Some(h) = hook {
                h(state_dir);
            }
            if !state_dir.exists() {
                return Ok(tree);
            }
            if start.elapsed() >= timeout {
                return Err(WorldError::new(
                    state_dir,
                    "concurrent state initialization timed out during read",
                ));
            }
            std::thread::sleep(STORE_RETRY_INTERVAL);
            continue;
        }

        if init_marker.exists() {
            if start.elapsed() >= timeout {
                return Err(WorldError::new(
                    state_dir,
                    "state initialization is incomplete",
                ));
            }
            std::thread::sleep(STORE_RETRY_INTERVAL);
            continue;
        }

        if !store_lock.exists() {
            if pending_path.exists() {
                return Err(WorldError::new(
                    &pending_path,
                    "store has an unfinished pending write that requires recovery",
                ));
            }
            let tree = crate::io::tree::read_tree(tree_root)?;
            if !store_lock.exists() && !init_marker.exists() && !pending_path.exists() {
                return Ok(tree);
            }
            if start.elapsed() >= timeout {
                return Err(WorldError::new(
                    state_dir,
                    "concurrent state lock creation timed out during read",
                ));
            }
            std::thread::sleep(STORE_RETRY_INTERVAL);
            continue;
        }

        let lease = acquire_read_lease(tree_root, state_dir)?;
        let tree = crate::io::tree::read_tree(tree_root)?;
        if gen_path.exists() {
            let actual_gen = tree.generation_hash();
            if &actual_gen != lease.generation_hash() {
                return Err(WorldError::new(
                    &gen_path,
                    "store generation does not match committed generation marker",
                ));
            }
        }
        drop(lease);
        return Ok(tree);
    }
}

/// Read the [`Tree`] under an already-held [`StoreLease`] without reacquiring `store.lock`
/// (`World::read_tree_under`).
///
/// # Errors
///
/// Returns [`WorldError`] if `lease` belongs to foreign roots, or if `lease` is a shared lease
/// and a pending write exists.
pub fn read_tree_under_lease(
    tree_root: &Path,
    state_dir: &Path,
    lease: &StoreLease,
) -> Result<Tree, WorldError> {
    if !lease.matches_roots(tree_root, state_dir) {
        return Err(WorldError::new(
            tree_root,
            "store lease does not match the target Corpus and state roots",
        ));
    }
    let pending_path = state_dir.join("pending.toml");
    if !lease.is_exclusive() && (lease.is_pending() || pending_path.exists()) {
        return Err(WorldError::new(
            &pending_path,
            "pending store generation cannot be read under a shared lease",
        ));
    }
    crate::io::tree::read_tree(tree_root)
}

#[derive(Debug)]
struct ActiveTx {
    _lock: JournalLockGuard,
    has_write_begin: bool,
    state_scope_count: usize,
}

/// Proof token returned by [`Journal::begin`], [`Journal::begin_export`], [`Journal::recover`],
/// or [`Journal::state_scope`].
///
/// Can only be constructed inside `io::journal`. Dropping an unfinished [`Audited`] token
/// releases its journal lease immediately.
#[derive(Debug)]
pub struct Audited {
    state_only: bool,
    state_dir: PathBuf,
    begin_seq: Option<u64>,
    verb: Option<Verb>,
    labels: Option<fieldbook_core::audit::LabelSet>,
    affected: Vec<RecordRef>,
    export_context: Option<(DestKind, Sha256, Sha256)>,
    active_tx: Arc<Mutex<Option<ActiveTx>>>,
    ended: bool,
}

impl Audited {
    /// Return `true` when this token was produced by [`Journal::state_scope`] (state-only).
    #[must_use]
    pub const fn is_state_only(&self) -> bool {
        self.state_only
    }

    /// Return the state folder bound to this token.
    #[must_use]
    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    /// Return the `write.begin` sequence number when this token represents a store write transaction.
    #[must_use]
    pub const fn begin_seq(&self) -> Option<u64> {
        self.begin_seq
    }

    pub(crate) fn has_active_lease(&self) -> bool {
        if self.ended {
            return false;
        }
        self.active_tx
            .lock()
            .ok()
            .and_then(|g| {
                g.as_ref().map(|tx| {
                    if self.state_only {
                        tx.state_scope_count > 0
                    } else {
                        tx.has_write_begin
                    }
                })
            })
            .unwrap_or(false)
    }
}

impl Drop for Audited {
    fn drop(&mut self) {
        if self.ended {
            return;
        }
        if let Ok(mut guard) = self.active_tx.lock()
            && let Some(tx) = guard.as_mut()
        {
            if self.state_only {
                tx.state_scope_count = tx.state_scope_count.saturating_sub(1);
            } else {
                tx.has_write_begin = false;
            }
            if !tx.has_write_begin && tx.state_scope_count == 0 {
                *guard = None;
            }
        }
    }
}

/// Handle to the append-only hash-chained journal under `<state>/audit/`.
///
/// An idle `Journal` holds no file lock; locks are held only during `open`, individual
/// appends, or an active [`Audited`] transaction.
pub struct Journal<'a> {
    world: &'a dyn World,
    state_dir: PathBuf,
    actor: Actor,
    store: StoreId,
    segment_limit: u64,
    active_tx: Arc<Mutex<Option<ActiveTx>>>,
    wrote_store: bool,
}

impl fmt::Debug for Journal<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Journal")
            .field("state_dir", &self.state_dir)
            .field("actor", &self.actor)
            .field("store", &self.store)
            .field("segment_limit", &self.segment_limit)
            .field("wrote_store", &self.wrote_store)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
struct TailState {
    last_seq: u64,
    last_hash: Sha256,
    active_segment: PathBuf,
    active_segment_num: u32,
    active_segment_len: u64,
}

impl<'a> Journal<'a> {
    /// Open the audit journal in `state_dir`, recovering any torn tail, appending `journal.open`,
    /// and recording `settings.change` if `settings.toml` changed since the last recorded baseline.
    ///
    /// Releases the initialization lock before returning the idle [`Journal`] handle.
    ///
    /// # Errors
    ///
    /// Returns [`JournalError`] if the journal lock is busy, the existing tail hash is broken,
    /// `settings.toml` is invalid, or an event cannot be written.
    pub fn open(world: &'a dyn World, state_dir: &Path, via: Via) -> Result<Self, JournalError> {
        init_state_locks(state_dir).map_err(|err| JournalError::io(err.path(), err.to_string()))?;
        let (role, current_baseline) = crate::io::state::read_role_and_baseline(state_dir)?;
        let actor = Actor::new(world.principal(), role, via);
        let store = StoreId::Corpus;

        let lock = acquire_journal_lock(state_dir)?;
        let mut tail = inspect_and_recover_tail(state_dir, &actor, store, world)?;

        let open_payload = EventPayload::JournalOpen {
            writer: Count::new(0),
            event_schema: Count::new(1),
            tool_version: SemVer::parse(env!("CARGO_PKG_VERSION"))
                .unwrap_or_else(|_| SemVer::parse("0.0.0").unwrap_or_else(|_| unreachable!())),
            stores: Vec::new(),
        };
        tail = append_event_locked(
            state_dir,
            &tail,
            DEFAULT_SEGMENT_LIMIT,
            world.now(),
            &actor,
            store,
            &open_payload,
        )?;

        let prev_baseline = last_recorded_settings_baseline(state_dir)?;
        if let Some(changed_groups) = current_baseline.diff(prev_baseline.as_ref()) {
            let tid = std::thread::current().id();
            let should_fail = test_hooks()
                .lock()
                .ok()
                .map(|mut g| {
                    if g.fail_on_settings_change_append == Some(tid) {
                        g.fail_on_settings_change_append = None;
                        true
                    } else {
                        false
                    }
                })
                .unwrap_or(false);
            if should_fail {
                return Err(JournalError::io(
                    &tail.active_segment,
                    "injected settings.change append failure",
                ));
            }
            let settings_payload = EventPayload::SettingsChange {
                names: changed_groups,
                baseline: current_baseline,
            };
            let _ = append_event_locked(
                state_dir,
                &tail,
                DEFAULT_SEGMENT_LIMIT,
                world.now(),
                &actor,
                store,
                &settings_payload,
            )?;
        }

        drop(lock);

        Ok(Self {
            world,
            state_dir: state_dir.to_path_buf(),
            actor,
            store,
            segment_limit: DEFAULT_SEGMENT_LIMIT,
            active_tx: Arc::new(Mutex::new(None)),
            wrote_store: false,
        })
    }

    /// Repair a broken journal chain by moving all existing segments unchanged into
    /// `state/audit/journal.broken-<first_bad_seq>/` and starting a fresh chain with
    /// `journal.open` followed by `journal.recover` (`reason = "broken_chain"`).
    ///
    /// Caller must hold an exclusive [`StoreLease`] covering `state_dir`.
    ///
    /// # Errors
    ///
    /// Returns [`JournalError`] if `lease` is not exclusive or does not match roots, if the
    /// journal lock cannot be acquired, or if segment files cannot be moved or written.
    pub fn open_for_repair(
        world: &'a dyn World,
        state_dir: &Path,
        via: Via,
        lease: &StoreLease,
        first_bad_seq: u64,
        last_good_seq: u64,
        last_good_hash: Sha256,
    ) -> Result<PathBuf, JournalError> {
        if !lease.is_exclusive() || !lease.matches_roots(world.tree_root(), state_dir) {
            return Err(JournalError::InvalidLease(
                "open_for_repair requires an exclusive matching StoreLease".to_owned(),
            ));
        }
        init_state_locks(state_dir).map_err(|err| JournalError::io(err.path(), err.to_string()))?;
        let (role, _) = crate::io::state::read_role_and_baseline(state_dir).unwrap_or_else(|_| {
            (
                fieldbook_core::audit::Role::Agent,
                fieldbook_core::audit::SettingsBaseline::absent(),
            )
        });
        let actor = Actor::new(world.principal(), role, via);
        let store = StoreId::Corpus;

        let _lock = acquire_journal_lock(state_dir)?;
        let audit_dir = state_dir.join("audit");
        let broken_dir = audit_dir.join(format!("journal.broken-{first_bad_seq}"));
        if broken_dir.exists() {
            return Err(JournalError::io(
                &broken_dir,
                "repair destination folder already exists",
            ));
        }
        std::fs::create_dir_all(&broken_dir)
            .map_err(|err| JournalError::io(&broken_dir, err.to_string()))?;

        for (_, seg_path) in list_segments(&audit_dir)? {
            if let Some(name) = seg_path.file_name() {
                let dst = broken_dir.join(name);
                std::fs::rename(&seg_path, &dst)
                    .map_err(|err| JournalError::io(&seg_path, err.to_string()))?;
            }
        }

        let first_seg = audit_dir.join("journal.jsonl");
        let zero_hash = Sha256::parse(ZERO_HASH).unwrap_or_else(|_| unreachable!());
        let tail = TailState {
            last_seq: 0,
            last_hash: zero_hash,
            active_segment: first_seg,
            active_segment_num: 1,
            active_segment_len: 0,
        };

        let open_payload = EventPayload::JournalOpen {
            writer: Count::new(0),
            event_schema: Count::new(1),
            tool_version: SemVer::parse(env!("CARGO_PKG_VERSION"))
                .unwrap_or_else(|_| SemVer::parse("0.0.0").unwrap_or_else(|_| unreachable!())),
            stores: Vec::new(),
        };
        let tail = append_event_locked(
            state_dir,
            &tail,
            DEFAULT_SEGMENT_LIMIT,
            world.now(),
            &actor,
            store,
            &open_payload,
        )?;

        let recover_payload = EventPayload::JournalRecover {
            reason: Word::new_static("broken_chain"),
            last_seq: Count::new(last_good_seq),
            last_hash: last_good_hash,
        };
        let _ = append_event_locked(
            state_dir,
            &tail,
            DEFAULT_SEGMENT_LIMIT,
            world.now(),
            &actor,
            store,
            &recover_payload,
        )?;

        Ok(broken_dir)
    }

    /// Override the segment rotation byte limit on this [`Journal`] handle (used in tests).
    pub fn set_segment_limit(&mut self, limit: u64) {
        self.segment_limit = limit;
    }

    /// Return `true` if a `write.begin` for an in-store verb was appended on this handle.
    #[must_use]
    pub const fn wrote_store(&self) -> bool {
        self.wrote_store
    }

    /// Return the event timestamp (`Now`) of the newest event of `kind` across all segments,
    /// or `None` when no event of `kind` has been recorded.
    #[must_use]
    pub fn last_time_of(&self, kind: Kind) -> Option<Now> {
        let audit_dir = self.state_dir.join("audit");
        let segments = list_segments(&audit_dir).ok()?;
        let mut prev_hex = ZERO_HASH.to_owned();
        let mut expected_seq = 1u64;
        let mut newest = None;

        for (_, seg_path) in segments {
            let content = std::fs::read_to_string(&seg_path).ok()?;
            for line in content.lines() {
                if line.is_empty() {
                    continue;
                }
                let Ok(verified) = verify_line(&prev_hex, line) else {
                    break;
                };
                let Ok((event, _)) = Event::parse_line(line) else {
                    break;
                };
                if event.seq != expected_seq {
                    break;
                }
                if event.kind() == kind {
                    newest = Some(event.time);
                }
                prev_hex = verified.hash.as_str().to_owned();
                expected_seq += 1;
            }
        }
        newest
    }

    /// Begin a store write transaction: validate the exclusive [`StoreLease`], persist
    /// `state/pending.toml` with live before-hashes and planned after-hashes, append
    /// `write.begin`, and return an [`Audited`] token retaining the journal lock.
    ///
    /// # Errors
    ///
    /// Returns [`JournalError`] if `lease` is not exclusive or mismatches roots, `intent` or
    /// `writes` is invalid, the journal lock is busy, the tail hash is broken, or `pending.toml`
    /// or `write.begin` cannot be written.
    pub fn begin(
        &mut self,
        lease: &StoreLease,
        intent: &OperationIntent,
        writes: &WriteSet,
    ) -> Result<Audited, JournalError> {
        if !lease.is_exclusive() || !lease.matches_roots(self.world.tree_root(), &self.state_dir) {
            return Err(JournalError::InvalidLease(
                "Journal::begin requires an exclusive StoreLease for the same roots".to_owned(),
            ));
        }
        intent
            .validate()
            .map_err(|err| JournalError::Vocab(err.to_string()))?;
        if writes.files.is_empty() {
            return Err(JournalError::Vocab(
                "WriteSet must contain at least one file".to_owned(),
            ));
        }
        let file_hashes = compute_file_hashes(&writes.files)
            .map_err(|err| JournalError::Vocab(err.to_string()))?;
        let affected = normalize_affected(&writes.affected);

        let already_active = self.active_tx.lock().map(|g| g.is_some()).unwrap_or(false);
        if already_active {
            let _ = acquire_journal_lock(&self.state_dir)?;
            return Err(JournalError::Busy);
        }

        let lock = acquire_journal_lock(&self.state_dir)?;
        let tail = inspect_tail_strict(&self.state_dir)?;
        let pending_path = self.state_dir.join("pending.toml");
        if pending_path.exists() {
            return Err(JournalError::InvalidPending(
                "cannot begin a new write while a pending transaction exists; recover it first"
                    .to_owned(),
            ));
        }
        let begin_seq = tail.last_seq + 1;

        let mut pending_files = Vec::with_capacity(writes.files.len());
        for wf in &writes.files {
            let canon_path = fieldbook_core::audit::CorpusPath::parse(&wf.path)
                .map_err(|err| JournalError::Vocab(err.to_string()))?;
            let after_sha256 =
                Sha256::parse(&wf.sha256).map_err(|err| JournalError::Vocab(err.to_string()))?;
            let disk_path = self.world.tree_root().join(canon_path.as_str());
            let before_sha256 = if disk_path.is_dir() {
                None
            } else {
                match std::fs::read(&disk_path) {
                    Ok(bytes) => Some(Sha256::digest(&bytes)),
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
                    Err(err) => return Err(JournalError::io(&disk_path, err.to_string())),
                }
            };
            pending_files.push(PendingFile {
                path: canon_path,
                before_sha256,
                after_sha256,
            });
        }

        let source_generation = lease.generation_hash().clone();
        let pending_write = PendingWrite {
            schema: 1,
            begin_seq,
            source_generation,
            labels: writes.labels.clone(),
            affected: affected.clone(),
            intent: intent.clone(),
            files: pending_files,
        };
        pending_write
            .validate()
            .map_err(|err| JournalError::InvalidPending(err.to_string()))?;

        write_sync_file(&pending_path, pending_write.to_toml().as_bytes())?;

        let tid = std::thread::current().id();
        let fail_before = test_hooks()
            .lock()
            .ok()
            .map(|mut g| {
                if g.fail_before_begin_append == Some(tid) {
                    g.fail_before_begin_append = None;
                    true
                } else {
                    false
                }
            })
            .unwrap_or(false);
        if fail_before {
            return Err(JournalError::io(
                &pending_path,
                "injected fault before write.begin append",
            ));
        }

        let payload = EventPayload::WriteBegin {
            verb: intent.key.verb.clone(),
            labels: writes.labels.clone(),
            files: Count::new(writes.files.len() as u64),
            file_hashes: Some(file_hashes),
            affected: affected.clone(),
            dest: None,
            path_sha256: None,
            policy: None,
        };
        let _ = append_event_locked(
            &self.state_dir,
            &tail,
            self.segment_limit,
            self.world.now(),
            &self.actor,
            self.store,
            &payload,
        )?;

        if !OUTSIDE_STORE_VERBS.contains(&intent.key.verb.as_str()) {
            self.wrote_store = true;
        }

        if let Ok(mut guard) = self.active_tx.lock() {
            *guard = Some(ActiveTx {
                _lock: lock,
                has_write_begin: true,
                state_scope_count: 0,
            });
        }

        Ok(Audited {
            state_only: false,
            state_dir: self.state_dir.clone(),
            begin_seq: Some(begin_seq),
            verb: Some(intent.key.verb.clone()),
            labels: Some(writes.labels.clone()),
            affected,
            export_context: None,
            active_tx: Arc::clone(&self.active_tx),
            ended: false,
        })
    }

    /// Begin an outward export/backup write transaction (`core-policy` step 19).
    ///
    /// Requires a delivery-bound [`Allowed`] token, verifies `set.labels` and every file path
    /// and digest in `set.files` against `allowed`'s delivery under the canonical `target_root`,
    /// appends `write.begin` with `dest`, `labels`, `policy`, and `path_sha256` of the canonical
    /// target root (or single-file path), and retains that context in [`Audited`] for [`Journal::end`].
    /// Never creates `state/pending.toml`.
    ///
    /// # Errors
    ///
    /// Returns [`JournalError`] if `verb` is invalid, `allowed` has no delivery or mismatches
    /// `set` or `target_root`, the journal lock is busy, or the event cannot be written.
    pub fn begin_export(
        &mut self,
        verb: &str,
        set: &WriteSet,
        allowed: &Allowed,
        target_root: &Path,
    ) -> Result<Audited, JournalError> {
        let parsed_verb =
            Verb::parse(verb).map_err(|_| JournalError::InvalidVerb(verb.to_owned()))?;
        if !allowed.has_delivery() {
            return Err(JournalError::Vocab(
                "begin_export requires a delivery-bound Allowed token".to_owned(),
            ));
        }
        if allowed.labels() != Some(&set.labels) {
            return Err(JournalError::Vocab(
                "WriteSet labels do not match Allowed token labels".to_owned(),
            ));
        }
        if set.files.is_empty() {
            return Err(JournalError::Vocab(
                "WriteSet must contain at least one file".to_owned(),
            ));
        }

        let canon_root = crate::io::root::resolve_outside_path(target_root)
            .map_err(|err| JournalError::Vocab(err.to_string()))?;

        let mut bound_writes: Vec<(PathBuf, String)> = Vec::new();
        for transfer in allowed.transfers().unwrap_or(&[]) {
            if transfer.operation == Operation::Write {
                let Target::Path(bound_path) = &transfer.target else {
                    return Err(JournalError::Vocab(
                        "export delivery write entry must target a path".to_owned(),
                    ));
                };
                let canon_bound = crate::io::root::resolve_outside_path(bound_path)
                    .map_err(|err| JournalError::Vocab(err.to_string()))?;
                if !canon_bound.starts_with(&canon_root) {
                    return Err(JournalError::Vocab(
                        "bound delivery path lies outside target_root".to_owned(),
                    ));
                }
                bound_writes.push((canon_bound, transfer.sha256.clone()));
            }
        }

        if bound_writes.len() != set.files.len() {
            return Err(JournalError::Vocab(
                "WriteSet file count does not match bound delivery writes".to_owned(),
            ));
        }

        let mut set_writes: Vec<(PathBuf, String)> = Vec::with_capacity(set.files.len());
        for wf in &set.files {
            let _ =
                Sha256::parse(&wf.sha256).map_err(|err| JournalError::Vocab(err.to_string()))?;
            let wf_path = Path::new(&wf.path);
            for comp in wf_path.components() {
                if matches!(comp, Component::ParentDir) {
                    return Err(JournalError::Vocab(
                        "export WriteFile path cannot contain `..`".to_owned(),
                    ));
                }
            }
            let resolved_wf = if wf_path.is_absolute() {
                crate::io::root::resolve_outside_path(wf_path)
                    .map_err(|err| JournalError::Vocab(err.to_string()))?
            } else if set.files.len() == 1
                && bound_writes.len() == 1
                && bound_writes[0].0 == canon_root
                && (wf.path.is_empty()
                    || canon_root
                        .file_name()
                        .is_some_and(|n| n == wf_path.as_os_str()))
            {
                canon_root.clone()
            } else {
                crate::io::root::resolve_outside_path(&canon_root.join(wf_path))
                    .map_err(|err| JournalError::Vocab(err.to_string()))?
            };
            if !resolved_wf.starts_with(&canon_root) {
                return Err(JournalError::Vocab(
                    "WriteFile path lies outside target_root".to_owned(),
                ));
            }
            set_writes.push((resolved_wf, wf.sha256.clone()));
        }

        bound_writes.sort();
        set_writes.sort();
        if bound_writes != set_writes {
            return Err(JournalError::Vocab(
                "WriteSet files do not match bound delivery transfers".to_owned(),
            ));
        }

        let canon_hash_target = if set.files.len() == 1 {
            &set_writes[0].0
        } else {
            &canon_root
        };
        let path_sha256 = Sha256::digest(canon_hash_target.to_string_lossy().as_bytes());
        let policy = Sha256::parse(allowed.policy_hash())
            .map_err(|err| JournalError::Vocab(err.to_string()))?;
        let dest = allowed.kind();
        let affected = normalize_affected(&set.affected);

        let already_active = self.active_tx.lock().map(|g| g.is_some()).unwrap_or(false);
        if already_active {
            let _ = acquire_journal_lock(&self.state_dir)?;
            return Err(JournalError::Busy);
        }

        let lock = acquire_journal_lock(&self.state_dir)?;
        let tail = inspect_tail_strict(&self.state_dir)?;
        let begin_seq = tail.last_seq + 1;

        let payload = EventPayload::WriteBegin {
            verb: parsed_verb.clone(),
            labels: set.labels.clone(),
            files: Count::new(set.files.len() as u64),
            file_hashes: None,
            affected: affected.clone(),
            dest: Some(dest),
            path_sha256: Some(path_sha256.clone()),
            policy: Some(policy.clone()),
        };
        let _ = append_event_locked(
            &self.state_dir,
            &tail,
            self.segment_limit,
            self.world.now(),
            &self.actor,
            self.store,
            &payload,
        )?;

        if let Ok(mut guard) = self.active_tx.lock() {
            *guard = Some(ActiveTx {
                _lock: lock,
                has_write_begin: true,
                state_scope_count: 0,
            });
        }

        Ok(Audited {
            state_only: false,
            state_dir: self.state_dir.clone(),
            begin_seq: Some(begin_seq),
            verb: Some(parsed_verb),
            labels: Some(set.labels.clone()),
            affected,
            export_context: Some((dest, path_sha256, policy)),
            active_tx: Arc::clone(&self.active_tx),
            ended: false,
        })
    }

    /// Finish a store or export write transaction: append `write.end` with the retained `verb`,
    /// `labels`, `affected` references, and optional export context, update `state/generation` and
    /// remove `state/pending.toml` when `outcome == "ok"` for in-store transactions, and release
    /// the journal lock.
    ///
    /// # Errors
    ///
    /// Returns [`JournalError`] if `token` is state-only, `outcome` is not `ok` or `failed`,
    /// or the event or generation marker cannot be written.
    pub fn end(&mut self, mut token: Audited, outcome: Word) -> Result<(), JournalError> {
        if token.is_state_only() {
            return Err(JournalError::InvalidLease(
                "state_scope token cannot be passed to Journal::end".to_owned(),
            ));
        }
        if !matches!(outcome.as_str(), "ok" | "failed") {
            return Err(JournalError::Vocab(format!(
                "invalid write.end outcome `{outcome}`"
            )));
        }
        let verb = token.verb.clone().ok_or_else(|| {
            JournalError::InvalidLease("missing verb on Audited token".to_owned())
        })?;
        let labels = token.labels.clone().ok_or_else(|| {
            JournalError::InvalidLease("missing labels on Audited token".to_owned())
        })?;
        let affected = token.affected.clone();
        let (dest, path_sha256, policy) = match &token.export_context {
            Some((d, p, pol)) => (Some(*d), Some(p.clone()), Some(pol.clone())),
            None => (None, None, None),
        };
        let is_export = token.export_context.is_some();

        let tail = inspect_tail_strict(&self.state_dir)?;
        let payload = EventPayload::WriteEnd {
            verb,
            labels,
            affected,
            outcome,
            dest,
            path_sha256,
            policy,
        };
        let _ = append_event_locked(
            &self.state_dir,
            &tail,
            self.segment_limit,
            self.world.now(),
            &self.actor,
            self.store,
            &payload,
        )?;

        if outcome.as_str() == "ok" && !is_export {
            let tid = std::thread::current().id();
            let fail_after = test_hooks()
                .lock()
                .ok()
                .map(|mut g| {
                    if g.fail_after_end_append == Some(tid) {
                        g.fail_after_end_append = None;
                        true
                    } else {
                        false
                    }
                })
                .unwrap_or(false);
            if fail_after {
                token.ended = true;
                if let Ok(mut guard) = self.active_tx.lock() {
                    *guard = None;
                }
                return Err(JournalError::io(
                    self.state_dir.join("generation"),
                    "injected fault after write.end append",
                ));
            }
            publish_committed_generation(self.world.tree_root(), &self.state_dir)?;
        }

        token.ended = true;
        if let Ok(mut guard) = self.active_tx.lock()
            && let Some(tx) = guard.as_mut()
        {
            tx.has_write_begin = false;
            if tx.state_scope_count == 0 {
                *guard = None;
            }
        }
        Ok(())
    }

    /// Append a `refuse` event to the journal, reusing the active transaction lease if one is
    /// held on this [`Journal`] handle or taking a short lock otherwise.
    ///
    /// # Errors
    ///
    /// Returns [`JournalError::InvalidVerb`] (`not a verb word: <text>`) if `verb` is not a valid
    /// [`Verb`], or [`JournalError`] if the lock or append fails.
    pub fn refuse(
        &self,
        verb: Option<&'static str>,
        code: Code,
        reason: Word,
        about: &RefuseAbout,
    ) -> Result<(), JournalError> {
        let parsed_verb = match verb {
            Some(v) => Some(Verb::parse(v).map_err(|_| JournalError::InvalidVerb(v.to_owned()))?),
            None => None,
        };
        if !fieldbook_core::audit::kind::REFUSE_REASONS.contains(&reason.as_str()) {
            return Err(JournalError::Vocab(format!(
                "invalid refuse reason `{reason}`"
            )));
        }

        let has_active = self.active_tx.lock().map(|g| g.is_some()).unwrap_or(false);
        let _short_lock = if has_active {
            None
        } else {
            Some(acquire_journal_lock(&self.state_dir)?)
        };

        let tail = inspect_tail_strict(&self.state_dir)?;
        let payload = EventPayload::Refuse {
            code,
            sha256: about.sha256.clone(),
            name_sha256: about.name_sha256.clone(),
            labels: about.labels.clone(),
            dest: about.dest,
            reason,
            verb: parsed_verb,
            system: about.system.clone(),
            stream: about.stream.clone(),
        };
        let _ = append_event_locked(
            &self.state_dir,
            &tail,
            self.segment_limit,
            self.world.now(),
            &self.actor,
            self.store,
            &payload,
        )?;
        Ok(())
    }

    /// Acquire a state-only [`Audited`] token (`Journal::state_scope(&self, lease)`) without
    /// appending any journal event.
    ///
    /// Validates that `lease` is exclusive and belongs to the same roots, acquires (or reuses
    /// an active operation's) journal lock, validates the journal head, and retains the journal
    /// lock until the returned [`Audited`] token is dropped.
    ///
    /// # Errors
    ///
    /// Returns [`JournalError`] if `lease` is shared or foreign, the journal lock times out,
    /// or the journal head is broken.
    pub fn state_scope(&self, lease: &StoreLease) -> Result<Audited, JournalError> {
        if !lease.is_exclusive() || !lease.matches_roots(self.world.tree_root(), &self.state_dir) {
            return Err(JournalError::InvalidLease(
                "Journal::state_scope requires an exclusive StoreLease for the same roots"
                    .to_owned(),
            ));
        }

        let mut guard = self
            .active_tx
            .lock()
            .map_err(|_| JournalError::InvalidLease("poisoned transaction mutex".to_owned()))?;

        if let Some(tx) = guard.as_mut() {
            let _ = inspect_tail_strict(&self.state_dir)?;
            tx.state_scope_count += 1;
        } else {
            let lock = acquire_journal_lock(&self.state_dir)?;
            let _ = inspect_tail_strict(&self.state_dir)?;
            *guard = Some(ActiveTx {
                _lock: lock,
                has_write_begin: false,
                state_scope_count: 1,
            });
        }
        drop(guard);

        Ok(Audited {
            state_only: true,
            state_dir: self.state_dir.clone(),
            begin_seq: None,
            verb: None,
            labels: None,
            affected: Vec::new(),
            export_context: None,
            active_tx: Arc::clone(&self.active_tx),
            ended: false,
        })
    }

    /// Read-only discovery of a pending write's [`PendingKey`] under an exclusive matching
    /// [`StoreLease`].
    ///
    /// # Errors
    ///
    /// Returns [`JournalError`] if `lease` is not exclusive or belongs to foreign roots, or if
    /// `state/pending.toml` is malformed.
    pub fn pending_key(&self, lease: &StoreLease) -> Result<Option<PendingKey>, JournalError> {
        if !lease.is_exclusive() || !lease.matches_roots(self.world.tree_root(), &self.state_dir) {
            return Err(JournalError::InvalidLease(
                "pending_key requires an exclusive matching StoreLease".to_owned(),
            ));
        }
        let pending_path = self.state_dir.join("pending.toml");
        let raw = match std::fs::read_to_string(&pending_path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(JournalError::io(&pending_path, err.to_string())),
        };
        let pw = PendingWrite::parse_toml(&raw)
            .map_err(|err| JournalError::InvalidPending(err.to_string()))?;
        Ok(Some(PendingKey {
            key: pw.intent.key,
            request_sha256: pw.intent.request_sha256,
        }))
    }

    /// Read-only lookup of a pending write's [`PendingOperation`] (intent, allocations, planning
    /// date, and stored [`WriteSet`]) matching `key` and `request_sha256`.
    ///
    /// # Errors
    ///
    /// Returns [`JournalError`] if `lease` is not exclusive or belongs to foreign roots, if
    /// `state/pending.toml` is malformed, or if a pending write exists for a different
    /// `OperationKey` or `request_sha256`.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "card signature specifies Sha256 by value"
    )]
    pub fn pending_intent(
        &self,
        lease: &StoreLease,
        key: &OperationKey,
        request_sha256: Sha256,
    ) -> Result<Option<PendingOperation>, JournalError> {
        if !lease.is_exclusive() || !lease.matches_roots(self.world.tree_root(), &self.state_dir) {
            return Err(JournalError::InvalidLease(
                "pending_intent requires an exclusive matching StoreLease".to_owned(),
            ));
        }
        let pending_path = self.state_dir.join("pending.toml");
        let raw = match std::fs::read_to_string(&pending_path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(JournalError::io(&pending_path, err.to_string())),
        };
        let pw = PendingWrite::parse_toml(&raw)
            .map_err(|err| JournalError::InvalidPending(err.to_string()))?;
        if &pw.intent.key != key || pw.intent.request_sha256 != request_sha256 {
            return Err(JournalError::InvalidPending(
                "pending write identity or request_sha256 does not match".to_owned(),
            ));
        }
        Ok(Some(PendingOperation {
            intent: pw.intent.clone(),
            writes: pw.to_write_set(),
        }))
    }

    /// Recover a pending write under an exclusive matching [`StoreLease`].
    ///
    /// Verifies `key`, `request_sha256`, the full path/hash plan in `writes`, and every live file
    /// on disk against the stored `before_sha256` and `after_sha256`.
    ///
    /// # Errors
    ///
    /// Returns [`JournalError`] if `lease` is invalid, `key`/`request_sha256` or `writes`
    /// differs from `pending.toml`, or any live file on disk has a third hash matching neither
    /// `before_sha256` nor `after_sha256`.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "card signature specifies Sha256 by value"
    )]
    pub fn recover(
        &mut self,
        lease: &StoreLease,
        key: &OperationKey,
        request_sha256: Sha256,
        writes: &WriteSet,
    ) -> Result<Recovery, JournalError> {
        if !lease.is_exclusive() || !lease.matches_roots(self.world.tree_root(), &self.state_dir) {
            return Err(JournalError::InvalidLease(
                "recover requires an exclusive matching StoreLease".to_owned(),
            ));
        }
        let pending_path = self.state_dir.join("pending.toml");
        let raw = match std::fs::read_to_string(&pending_path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Recovery::None),
            Err(err) => return Err(JournalError::io(&pending_path, err.to_string())),
        };
        let pw = PendingWrite::parse_toml(&raw)
            .map_err(|err| JournalError::InvalidPending(err.to_string()))?;

        if &pw.intent.key != key || pw.intent.request_sha256 != request_sha256 {
            return Err(JournalError::InvalidPending(
                "recovery key or request_sha256 does not match pending.toml".to_owned(),
            ));
        }

        let expected_ws = pw.to_write_set();
        let norm_supplied_affected = normalize_affected(&writes.affected);
        if writes.labels != expected_ws.labels
            || norm_supplied_affected != expected_ws.affected
            || writes.files != expected_ws.files
        {
            return Err(JournalError::InvalidPending(
                "supplied WriteSet differs from persisted pending.toml plan".to_owned(),
            ));
        }

        let mut all_before = true;
        let mut all_after = true;
        for pf in &pw.files {
            let disk_path = self.world.tree_root().join(pf.path.as_str());
            let live_sha = match std::fs::read(&disk_path) {
                Ok(bytes) => Some(Sha256::digest(&bytes)),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
                Err(err) => return Err(JournalError::io(&disk_path, err.to_string())),
            };
            let matches_before = live_sha == pf.before_sha256;
            let matches_after = live_sha.as_ref() == Some(&pf.after_sha256);
            if !matches_before && !matches_after {
                return Err(JournalError::InvalidPending(format!(
                    "live file `{}` matches neither before nor after hash during recovery",
                    pf.path
                )));
            }
            if !matches_before {
                all_before = false;
            }
            if !matches_after {
                all_after = false;
            }
        }

        let lock = acquire_journal_lock(&self.state_dir)?;
        let tail = inspect_tail_strict(&self.state_dir)?;
        let events = read_verified_events(&self.state_dir)?;

        let has_begin = events.iter().any(|e| {
            e.seq == pw.begin_seq
                && matches!(&e.payload, EventPayload::WriteBegin { verb, .. } if verb == &pw.intent.key.verb)
        });
        let has_end = events.iter().any(|e| {
            e.seq > pw.begin_seq
                && matches!(&e.payload, EventPayload::WriteEnd { verb, outcome, .. } if verb == &pw.intent.key.verb && outcome.as_str() == "ok")
        });

        if !has_begin {
            if all_before {
                std::fs::remove_file(&pending_path)
                    .map_err(|err| JournalError::io(&pending_path, err.to_string()))?;
                drop(lock);
                return Ok(Recovery::None);
            }
            return Err(JournalError::InvalidPending(
                "pending.toml has modified files without a recorded write.begin".to_owned(),
            ));
        }

        if all_after {
            if !has_end {
                let end_payload = EventPayload::WriteEnd {
                    verb: pw.intent.key.verb.clone(),
                    labels: pw.labels.clone(),
                    affected: pw.affected.clone(),
                    outcome: Word::new_static("ok"),
                    dest: None,
                    path_sha256: None,
                    policy: None,
                };
                let _ = append_event_locked(
                    &self.state_dir,
                    &tail,
                    self.segment_limit,
                    self.world.now(),
                    &self.actor,
                    self.store,
                    &end_payload,
                )?;
            }
            publish_committed_generation(self.world.tree_root(), &self.state_dir)?;
            drop(lock);
            return Ok(Recovery::Completed { intent: pw.intent });
        }

        if has_end {
            return Err(JournalError::InvalidPending(
                "write.end is recorded in journal but not all files have after_sha256".to_owned(),
            ));
        }

        if let Ok(mut guard) = self.active_tx.lock() {
            *guard = Some(ActiveTx {
                _lock: lock,
                has_write_begin: true,
                state_scope_count: 0,
            });
        }

        Ok(Recovery::Resume {
            audited: Audited {
                state_only: false,
                state_dir: self.state_dir.clone(),
                begin_seq: Some(pw.begin_seq),
                verb: Some(pw.intent.key.verb.clone()),
                labels: Some(pw.labels.clone()),
                affected: pw.affected,
                export_context: None,
                active_tx: Arc::clone(&self.active_tx),
                ended: false,
            },
            intent: pw.intent,
        })
    }

    /// Return the latest completed after-content hash for `path` in this journal's store from
    /// verified `write.begin` / `write.end (outcome = "ok")` pairs.
    ///
    /// # Errors
    ///
    /// Returns [`JournalError`] if the journal segments cannot be read or verified.
    pub fn completed_after_hash(&self, path: &str) -> Result<Option<Sha256>, JournalError> {
        let events = read_verified_events(&self.state_dir)?;
        Ok(completed_after_hash(&events, self.store, path))
    }

    /// Append an arbitrary [`EventPayload`] under a short journal lock.
    ///
    /// Used by `build_state` in `tests/support/state.rs` and by later verb cards to emit their
    /// own typed event kinds through the real [`Journal`] writer.
    ///
    /// # Errors
    ///
    /// Returns [`JournalError`] if the lock cannot be acquired, the tail is broken, or the
    /// event cannot be written.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "callers pass owned EventPayload"
    )]
    pub fn append_payload(&self, payload: EventPayload) -> Result<Sha256, JournalError> {
        let has_active = self.active_tx.lock().map(|g| g.is_some()).unwrap_or(false);
        let _short_lock = if has_active {
            None
        } else {
            Some(acquire_journal_lock(&self.state_dir)?)
        };
        let tail = inspect_tail_strict(&self.state_dir)?;
        let next = append_event_locked(
            &self.state_dir,
            &tail,
            self.segment_limit,
            self.world.now(),
            &self.actor,
            self.store,
            &payload,
        )?;
        Ok(next.last_hash)
    }
}

/// Remove transient lock files (`state/lock` and `state/store.lock`) after building a static
/// test fixture directory (`build_state`).
///
/// # Errors
///
/// Returns [`JournalError`] if a lock file exists and cannot be removed.
pub fn cleanup_fixture_locks(state_dir: &Path) -> Result<(), JournalError> {
    for name in ["lock", "store.lock", ".init"] {
        let p = state_dir.join(name);
        if p.exists() {
            std::fs::remove_file(&p).map_err(|err| JournalError::io(&p, err.to_string()))?;
        }
    }
    Ok(())
}

/// Write raw bytes to `path` and call `sync_data` (used by `build_state` and `plant` in test support
/// and internal control files so no raw writer exists outside `journal.rs`).
///
/// # Errors
///
/// Returns [`JournalError`] if parent directories or the file cannot be created or written.
pub fn write_sync_file(path: &Path, bytes: &[u8]) -> Result<(), JournalError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| JournalError::io(parent, err.to_string()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(|err| JournalError::io(path, err.to_string()))?;
    file.write_all(bytes)
        .map_err(|err| JournalError::io(path, err.to_string()))?;
    file.sync_data()
        .map_err(|err| JournalError::io(path, err.to_string()))?;
    Ok(())
}

fn publish_committed_generation(tree_root: &Path, state_dir: &Path) -> Result<(), JournalError> {
    let tree = crate::io::tree::read_tree(tree_root)
        .map_err(|err| JournalError::io(err.path(), err.to_string()))?;
    let gen_hash = tree.generation_hash();
    let gen_path = state_dir.join("generation");
    write_sync_file(&gen_path, format!("{gen_hash}\n").as_bytes())?;
    let pending_path = state_dir.join("pending.toml");
    if pending_path.exists() {
        std::fs::remove_file(&pending_path)
            .map_err(|err| JournalError::io(&pending_path, err.to_string()))?;
    }
    Ok(())
}

/// Read raw bytes of a journal segment file (`io::journal` owns segment file I/O).
///
/// # Errors
///
/// Returns [`JournalError::Io`] if `seg_path` cannot be read.
pub fn read_segment_bytes(seg_path: &Path) -> Result<Vec<u8>, JournalError> {
    std::fs::read(seg_path).map_err(|err| JournalError::io(seg_path, err.to_string()))
}

/// List journal segment files (`journal.jsonl`, `journal-0002.jsonl`, ...) in segment-number order.
///
/// # Errors
///
/// Returns [`JournalError`] if `audit_dir` exists and cannot be read.
pub fn list_segments(audit_dir: &Path) -> Result<Vec<(u32, PathBuf)>, JournalError> {
    if !audit_dir.exists() {
        return Ok(Vec::new());
    }
    let iter =
        std::fs::read_dir(audit_dir).map_err(|err| JournalError::io(audit_dir, err.to_string()))?;
    let mut segments = Vec::new();
    for entry in iter {
        let entry = entry.map_err(|err| JournalError::io(audit_dir, err.to_string()))?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if let Some(num) = classify_segment_name(name) {
            segments.push((num, path));
        }
    }
    segments.sort_by_key(|(num, _)| *num);
    Ok(segments)
}

fn classify_segment_name(name: &str) -> Option<u32> {
    if name == "journal.jsonl" {
        return Some(1);
    }
    let rest = name.strip_prefix("journal-")?.strip_suffix(".jsonl")?;
    if rest.len() != 4 || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let num: u32 = rest.parse().ok()?;
    if num >= 2 { Some(num) } else { None }
}

fn segment_filename(num: u32) -> String {
    if num <= 1 {
        "journal.jsonl".to_owned()
    } else {
        format!("journal-{num:04}.jsonl")
    }
}

fn inspect_and_recover_tail(
    state_dir: &Path,
    actor: &Actor,
    store: StoreId,
    world: &dyn World,
) -> Result<TailState, JournalError> {
    let audit_dir = state_dir.join("audit");
    let segments = list_segments(&audit_dir)?;
    let zero_hash = Sha256::parse(ZERO_HASH).unwrap_or_else(|_| unreachable!());

    if segments.is_empty() {
        let first_seg = audit_dir.join("journal.jsonl");
        return Ok(TailState {
            last_seq: 0,
            last_hash: zero_hash,
            active_segment: first_seg,
            active_segment_num: 1,
            active_segment_len: 0,
        });
    }

    let mut all_lines: Vec<(u32, PathBuf, String)> = Vec::new();
    let mut last_seg_raw = String::new();
    for (seg_num, seg_path) in &segments {
        let raw = std::fs::read_to_string(seg_path)
            .map_err(|err| JournalError::io(seg_path, err.to_string()))?;
        for line in raw.lines() {
            if !line.is_empty() {
                all_lines.push((*seg_num, seg_path.clone(), line.to_owned()));
            }
        }
        last_seg_raw = raw;
    }

    let &(last_seg_num, ref last_seg_path) = segments.last().unwrap_or_else(|| unreachable!());
    let last_seg_len = std::fs::metadata(last_seg_path)
        .map(|m| m.len())
        .unwrap_or(0);

    if all_lines.is_empty() {
        return Ok(TailState {
            last_seq: 0,
            last_hash: zero_hash,
            active_segment: last_seg_path.clone(),
            active_segment_num: last_seg_num,
            active_segment_len: last_seg_len,
        });
    }

    let last_idx = all_lines.len() - 1;
    let last_line_str = &all_lines[last_idx].2;

    if split_hash_suffix(last_line_str).is_err() {
        let mut prev_hex = ZERO_HASH.to_owned();
        let mut last_valid_seq = 0u64;
        for (_, _, line) in &all_lines[..last_idx] {
            let verified = verify_line(&prev_hex, line)
                .map_err(|err| JournalError::BrokenChain(err.to_string()))?;
            let (event, _) = Event::parse_line(line)
                .map_err(|err| JournalError::BrokenChain(err.to_string()))?;
            if event.seq != last_valid_seq + 1 {
                return Err(JournalError::BrokenChain(format!(
                    "sequence mismatch at line {}",
                    last_valid_seq + 1
                )));
            }
            last_valid_seq = event.seq;
            prev_hex = verified.hash.as_str().to_owned();
        }

        let torn_seq = last_valid_seq + 1;
        let torn_path = audit_dir.join(format!("journal.torn-{torn_seq}"));
        write_sync_file(&torn_path, format!("{last_line_str}\n").as_bytes())?;

        let mut kept = String::new();
        for (seg_n, _, line) in &all_lines[..last_idx] {
            if *seg_n == last_seg_num {
                kept.push_str(line);
                kept.push('\n');
            }
        }
        write_sync_file(last_seg_path, kept.as_bytes())?;

        let last_valid_hash = Sha256::parse(&prev_hex).unwrap_or_else(|_| unreachable!());
        let tail_before_recover = TailState {
            last_seq: last_valid_seq,
            last_hash: last_valid_hash.clone(),
            active_segment: last_seg_path.clone(),
            active_segment_num: last_seg_num,
            active_segment_len: kept.len() as u64,
        };

        let recover_payload = EventPayload::JournalRecover {
            reason: Word::new_static("torn_tail"),
            last_seq: Count::new(last_valid_seq),
            last_hash: last_valid_hash,
        };
        return append_event_locked(
            state_dir,
            &tail_before_recover,
            DEFAULT_SEGMENT_LIMIT,
            world.now(),
            actor,
            store,
            &recover_payload,
        );
    }

    if !last_seg_raw.is_empty() && !last_seg_raw.ends_with('\n') {
        last_seg_raw.push('\n');
        write_sync_file(last_seg_path, last_seg_raw.as_bytes())?;
    }

    inspect_tail_strict(state_dir)
}

fn inspect_tail_strict(state_dir: &Path) -> Result<TailState, JournalError> {
    let audit_dir = state_dir.join("audit");
    let segments = list_segments(&audit_dir)?;
    let zero_hash = Sha256::parse(ZERO_HASH).unwrap_or_else(|_| unreachable!());

    if segments.is_empty() {
        let first_seg = audit_dir.join("journal.jsonl");
        return Ok(TailState {
            last_seq: 0,
            last_hash: zero_hash,
            active_segment: first_seg,
            active_segment_num: 1,
            active_segment_len: 0,
        });
    }

    let mut all_lines: Vec<String> = Vec::new();
    for (_, seg_path) in &segments {
        let raw = std::fs::read_to_string(seg_path)
            .map_err(|err| JournalError::io(seg_path, err.to_string()))?;
        if !raw.is_empty() && !raw.ends_with('\n') {
            return Err(JournalError::BrokenChain(
                "unterminated journal segment line".to_owned(),
            ));
        }
        for line in raw.lines() {
            if !line.is_empty() {
                all_lines.push(line.to_owned());
            }
        }
    }

    let &(last_seg_num, ref last_seg_path) = segments.last().unwrap_or_else(|| unreachable!());
    let last_seg_len = std::fs::metadata(last_seg_path)
        .map(|m| m.len())
        .unwrap_or(0);

    if all_lines.is_empty() {
        return Ok(TailState {
            last_seq: 0,
            last_hash: zero_hash,
            active_segment: last_seg_path.clone(),
            active_segment_num: last_seg_num,
            active_segment_len: last_seg_len,
        });
    }

    let mut prev_hex = ZERO_HASH.to_owned();
    let mut last_seq = 0u64;
    let mut last_hash = zero_hash;

    for (idx, line) in all_lines.iter().enumerate() {
        let verified = verify_line(&prev_hex, line)
            .map_err(|err| JournalError::BrokenChain(err.to_string()))?;
        let (event, _) =
            Event::parse_line(line).map_err(|err| JournalError::BrokenChain(err.to_string()))?;
        let expected_seq = (idx as u64) + 1;
        if event.seq != expected_seq {
            return Err(JournalError::BrokenChain(format!(
                "sequence {} does not match line number {expected_seq}",
                event.seq
            )));
        }
        prev_hex = verified.hash.as_str().to_owned();
        last_seq = event.seq;
        last_hash = verified.hash;
    }

    Ok(TailState {
        last_seq,
        last_hash,
        active_segment: last_seg_path.clone(),
        active_segment_num: last_seg_num,
        active_segment_len: last_seg_len,
    })
}

fn last_recorded_settings_baseline(
    state_dir: &Path,
) -> Result<Option<fieldbook_core::audit::SettingsBaseline>, JournalError> {
    let events = read_verified_events(state_dir)?;
    let mut latest = None;
    for event in events {
        if let EventPayload::SettingsChange { baseline, .. } = event.payload {
            latest = Some(baseline);
        }
    }
    Ok(latest)
}

fn read_verified_events(state_dir: &Path) -> Result<Vec<Event>, JournalError> {
    let audit_dir = state_dir.join("audit");
    let segments = list_segments(&audit_dir)?;
    let mut events = Vec::new();
    let mut prev_hex = ZERO_HASH.to_owned();
    let mut expected_seq = 1u64;

    for (_, seg_path) in segments {
        let raw = std::fs::read_to_string(&seg_path)
            .map_err(|err| JournalError::io(&seg_path, err.to_string()))?;
        for line in raw.lines() {
            if line.is_empty() {
                continue;
            }
            let verified = verify_line(&prev_hex, line)
                .map_err(|err| JournalError::BrokenChain(err.to_string()))?;
            let (event, _) = Event::parse_line(line)
                .map_err(|err| JournalError::BrokenChain(err.to_string()))?;
            if event.seq != expected_seq {
                return Err(JournalError::BrokenChain(format!(
                    "sequence {} does not match expected {expected_seq}",
                    event.seq
                )));
            }
            prev_hex = verified.hash.as_str().to_owned();
            expected_seq = expected_seq.saturating_add(1);
            events.push(event);
        }
    }
    Ok(events)
}

fn append_event_locked(
    state_dir: &Path,
    tail: &TailState,
    segment_limit: u64,
    at: Now,
    actor: &Actor,
    store: StoreId,
    payload: &EventPayload,
) -> Result<TailState, JournalError> {
    let seq = tail.last_seq + 1;
    let env = EventEnvelope {
        actor: actor.principal.clone(),
        role: actor.role,
        store,
        via: actor.via,
    };
    let (mut line, new_hash) = payload.format_line(seq, at, &env, tail.last_hash.as_str());
    line.push('\n');

    let (target_num, target_path, base_len) =
        if tail.active_segment_len > 0 && tail.active_segment_len >= segment_limit {
            let next_num = tail.active_segment_num + 1;
            let next_path = state_dir.join("audit").join(segment_filename(next_num));
            (next_num, next_path, 0u64)
        } else {
            (
                tail.active_segment_num,
                tail.active_segment.clone(),
                tail.active_segment_len,
            )
        };

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&target_path)
        .map_err(|err| JournalError::io(&target_path, err.to_string()))?;
    file.write_all(line.as_bytes())
        .map_err(|err| JournalError::io(&target_path, err.to_string()))?;
    file.sync_data()
        .map_err(|err| JournalError::io(&target_path, err.to_string()))?;

    Ok(TailState {
        last_seq: seq,
        last_hash: new_hash,
        active_segment: target_path,
        active_segment_num: target_num,
        active_segment_len: base_len + (line.len() as u64),
    })
}
