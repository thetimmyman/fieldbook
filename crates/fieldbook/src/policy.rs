//! Binary policy coordinator: `PolicyCtx`, `resolve`, `observe`, `PolicySnapshot`,
//! `PolicyProvider`, and the `policy show` / `policy check` subcommand runners
//! (`policy-load` steps 6, 7, 13, 14, 15).

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use fieldbook_core::audit::{
    Count, EventPayload, Kind, LabelSet, Now, Principal, Sha256, Word, ZERO_HASH,
};
use fieldbook_core::check::Code;
use fieldbook_core::egress::{self, Allowed, Decision, Delivery, Dest, Shipped};
use fieldbook_core::labels::Refusal;
use fieldbook_core::policy::{
    ClosedWhy, EffectivePolicy, FileFact, FileObservation, FloorKind, ObservationDigest,
    OwnerSettings, Policy, PolicyError, PolicyState, Resolved, Seen, Source, WriterWhy,
    policy_hash, policy_line_from, resolve as resolve_pure, show_at,
};

use crate::cli::{
    EXIT_FINDINGS, EXIT_OK, EXIT_USAGE, PolicyArgs, PolicyCheckArgs, PolicyShowArgs,
    PolicySubcommand,
};
use crate::io::journal::{Journal, JournalError, read_valid_events_prefix};
use crate::io::policy::{
    PlaceRefusal, PolicySource, company_store_beside, owner_settings, read_seen, write_seen,
};
use crate::{Outcome, World};

/// Warning printed to standard error when `state/policy-seen.toml` exists and cannot be parsed.
pub const UNREADABLE_SEEN_WARNING: &str =
    "warning: state/policy-seen.toml is unreadable; no revision is recorded";

std::thread_local! {
    static ACTIVE_POLICY_CTX: std::cell::RefCell<Option<PolicyCtx>> = const { std::cell::RefCell::new(None) };
}

/// Runs `f` with `ctx` installed as the thread-local active [`PolicyCtx`] so [`Journal::open`]
/// observes the run's resolved policy when a writing verb opens the journal.
pub fn with_active_policy<R>(ctx: &PolicyCtx, f: impl FnOnce() -> R) -> R {
    ACTIVE_POLICY_CTX.with(|cell| {
        let prev = cell.replace(Some(ctx.clone()));
        let res = f();
        cell.replace(prev);
        res
    })
}

/// Returns the active thread-local [`PolicyCtx`], if one was installed for the current CLI run.
#[must_use]
pub fn active_policy() -> Option<PolicyCtx> {
    ACTIVE_POLICY_CTX.with(|cell| cell.borrow().clone())
}

/// Resolved policy context for one run or operation (`policy-load` step 6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyCtx {
    /// Runtime policy state (`Floor`, `Loaded`, or `Closed`).
    pub state: PolicyState,
    /// Policy file path (`empty` when no source is configured or built in-process).
    pub path: PathBuf,
    /// Policy source (`None`, `Machine`, or `Flag`).
    pub source: Source,
    /// Raw byte observation digest copied from [`Resolved`].
    pub digest: ObservationDigest,
    /// Writer check result copied from [`Resolved`].
    pub writers: Result<(), WriterWhy>,
    /// Resulting observation hash (`policy` field of `policy.change`).
    pub hash: Sha256,
    /// The [`Seen`] record read from `state/policy-seen.toml` before resolution.
    pub seen_read: Option<Seen>,
    /// The [`Seen`] record to persist (`new` only for `Loaded`; unchanged for `Closed`).
    pub seen_to_record: Option<Seen>,
    /// Journal `policy.change` outcome word (`loaded`, `absent`, `invalid`), or `None` when
    /// remaining on the Floor or built via [`Self::loaded`] / [`Self::closed`].
    pub outcome: Option<&'static str>,
    /// Short note from [`Resolved`], if any.
    pub note: Option<&'static str>,
    /// First schema-1 [`PolicyError`] finding from [`Resolved`], if any.
    pub finding: Option<PolicyError>,
    /// Complete [`Resolved`] value.
    pub resolved: Resolved,
    /// True when `state/policy-seen.toml` existed on disk but was unreadable or invalid.
    pub seen_unreadable: bool,
}

impl PolicyCtx {
    /// Builds an in-process [`PolicyCtx`] for a test that runs a verb under a loaded [`Policy`]
    /// without writing a file (path is empty, hash is the hash of the text given to
    /// `Policy::parse`, nothing is recorded, and `outcome` is `None`).
    #[must_use]
    pub fn loaded(policy: Policy) -> Self {
        let hash = Sha256::parse(policy.hash())
            .unwrap_or_else(|_| Sha256::digest(policy.hash().as_bytes()));
        let obs = FileObservation {
            fact: FileFact::Text(String::new()),
            digest: ObservationDigest::Whole(hash.clone()),
            writers: Ok(()),
        };
        let state = PolicyState::Loaded(policy);
        let resolved = Resolved {
            state: state.clone(),
            outcome: None,
            seen: None,
            note: None,
            finding: None,
            observation: obs.clone(),
        };
        Self {
            state,
            path: PathBuf::new(),
            source: Source::None,
            digest: obs.digest,
            writers: obs.writers,
            hash,
            seen_read: None,
            seen_to_record: None,
            outcome: None,
            note: None,
            finding: None,
            resolved,
            seen_unreadable: false,
        }
    }

    /// Builds an in-process [`PolicyCtx`] for a test that runs a verb under a closed policy
    /// (`Closed(why)`) without writing a file (path is empty, nothing is recorded, and `outcome`
    /// is `None`).
    #[must_use]
    pub fn closed(why: ClosedWhy) -> Self {
        let writers = if why == ClosedWhy::Writable {
            Err(WriterWhy::FileOther)
        } else {
            Ok(())
        };
        let obs = FileObservation {
            fact: FileFact::Absent,
            digest: ObservationDigest::Absent,
            writers,
        };
        let state = PolicyState::Closed(why);
        let resolved = Resolved {
            state: state.clone(),
            outcome: None,
            seen: None,
            note: None,
            finding: None,
            observation: obs.clone(),
        };
        let hash = resolved.observation_hash();
        Self {
            state,
            path: PathBuf::new(),
            source: Source::None,
            digest: obs.digest,
            writers: obs.writers,
            hash,
            seen_read: None,
            seen_to_record: None,
            outcome: None,
            note: None,
            finding: None,
            resolved,
            seen_unreadable: false,
        }
    }

    /// Returns the recorded or active `[audit].sink` path, surviving `Closed` runs via `seen_to_record`.
    #[must_use]
    pub fn audit_sink(&self) -> Option<PathBuf> {
        if let PolicyState::Loaded(p) = &self.state
            && let Some(sink) = &p.audit().sink
        {
            return Some(PathBuf::from(sink));
        }
        self.seen_to_record
            .as_ref()
            .and_then(|s| s.audit_sink.clone())
            .or_else(|| self.seen_read.as_ref().and_then(|s| s.audit_sink.clone()))
    }

    /// Formats the single `Closed` warning line for standard error on non-exempt verbs:
    /// `warning: the policy is closed (<CODE>): <why>; every destination is denied except the audit sink`.
    #[must_use]
    pub fn closed_warning_line(&self) -> Option<String> {
        let code = self.resolved.closed_code()?;
        let why = self.resolved.closed_why_text()?;
        Some(format!(
            "warning: the policy is closed ({}): {why}; every destination is denied except the audit sink",
            code.id()
        ))
    }

    /// Builds the [`EffectivePolicy`] for this context and `world`.
    #[must_use]
    pub fn effective_policy(&self, world: &dyn World) -> EffectivePolicy {
        EffectivePolicy::new(&self.state, &owner_settings(world))
    }
}

/// Formats `policy: <floor|loaded|closed> <first eight hex digits of the hash>` (`policy-load` step 4).
#[must_use]
pub fn policy_line(ctx: &PolicyCtx) -> String {
    policy_line_from(&ctx.state, ctx.hash.as_str())
}

/// Resolves the active [`PolicyCtx`] for `world`, `state`, `corpus`, and `laptop`.
///
/// Writes nothing to the state folder or audit journal.
///
/// # Errors
///
/// Returns [`Refusal`] (`FB0414`) when the policy path sits in an owner-editable tree.
pub fn resolve(
    world: &dyn World,
    state: &Path,
    corpus: &Path,
    laptop: bool,
) -> Result<PolicyCtx, Refusal> {
    let source = PolicySource::discover(world, laptop);
    resolve_with_source(world, state, corpus, laptop, source).map_err(|e| e.to_refusal())
}

/// Like [`resolve`], taking an explicit [`Source`] and returning the detailed [`PlaceRefusal`]
/// carrying the offending path for CLI error formatting.
///
/// # Errors
///
/// Returns [`PlaceRefusal`] (`FB0414`) when the policy path sits inside `state`, `corpus`, or a
/// Git work tree.
pub fn resolve_with_source(
    world: &dyn World,
    state: &Path,
    corpus: &Path,
    _laptop: bool,
    source: Source,
) -> Result<PolicyCtx, PlaceRefusal> {
    let obs = PolicySource::read_in(world, &source, state, corpus)?;
    let (seen_read, seen_unreadable) = read_seen(world, state);
    let has_company_store = world.has_company_store() || company_store_beside(world, state);
    let resolved = resolve_pure(obs, seen_read.as_ref(), has_company_store);
    let hash = resolved.observation_hash();
    let path = source.path().map(Path::to_path_buf).unwrap_or_default();

    Ok(PolicyCtx {
        state: resolved.state.clone(),
        path,
        source,
        digest: resolved.observation.digest.clone(),
        writers: resolved.observation.writers,
        hash,
        seen_read,
        seen_to_record: resolved.seen.clone(),
        outcome: resolved.outcome,
        note: resolved.note,
        finding: resolved.finding.clone(),
        resolved,
        seen_unreadable,
    })
}

/// Appends `policy.change` to `journal` when the current policy observation in `ctx` differs from
/// the latest validated `policy.change` event in `journal` (or from `ctx.seen_read` when no
/// `policy.change` event has been recorded yet), and persists `state/policy-seen.toml` on `Loaded`
/// (`policy-load` steps 6, 14, 15).
///
/// # Errors
///
/// Returns [`JournalError`] if the journal event or `policy-seen.toml` cannot be written.
pub fn observe(journal: &mut Journal<'_>, ctx: &PolicyCtx) -> Result<(), JournalError> {
    let Some(outcome_str) = ctx.outcome else {
        return Ok(());
    };

    let state_dir = journal.state_dir().to_path_buf();
    let events = read_valid_events_prefix(&state_dir);
    let latest_event_policy: Option<Sha256> = events.iter().rev().find_map(|ev| {
        if let EventPayload::PolicyChange { policy, .. } = &ev.payload {
            Some(policy.clone())
        } else {
            None
        }
    });

    let zero_hash = Sha256::parse(ZERO_HASH).unwrap_or_else(|_| unreachable!());

    let prev = match latest_event_policy {
        Some(last_hash) => {
            if last_hash == ctx.hash {
                return Ok(());
            }
            if ctx.seen_read.is_none() && matches!(ctx.state, PolicyState::Loaded(_)) {
                zero_hash
            } else {
                last_hash
            }
        }
        None => {
            if let Some(seen) = &ctx.seen_read {
                if seen.hash == ctx.hash && matches!(ctx.state, PolicyState::Loaded(_)) {
                    return Ok(());
                }
                seen.hash.clone()
            } else {
                zero_hash
            }
        }
    };

    let (revision, labels_n, rows_n) = match &ctx.state {
        PolicyState::Loaded(policy) => (
            policy.revision(),
            policy.labels().len() as u64,
            policy.destinations().len() as u64,
        ),
        PolicyState::Closed(_) => {
            let rev = ctx
                .seen_to_record
                .as_ref()
                .or(ctx.seen_read.as_ref())
                .map_or(0, |s| s.revision);
            (rev, 0, 0)
        }
        PolicyState::Floor(_) => return Ok(()),
    };

    let payload = EventPayload::PolicyChange {
        policy: ctx.hash.clone(),
        prev,
        revision: Count::new(revision),
        outcome: Word::new_static(outcome_str),
        labels_n: Count::new(labels_n),
        rows_n: Count::new(rows_n),
    };

    let _ = journal.append_payload(payload)?;

    if matches!(ctx.state, PolicyState::Loaded(_))
        && let Some(new_seen) = &ctx.seen_to_record
    {
        let audited = journal.policy_state_scope()?;
        write_seen(journal.world(), &audited, &state_dir, new_seen)
            .map_err(|err| JournalError::io(err.path(), err.to_string()))?;
    }

    Ok(())
}

/// Called from [`Journal::open_stamped`]: observes the active thread-local [`PolicyCtx`] (or
/// resolves from `journal.world()` if a policy source is configured in `world`).
pub(crate) fn observe_on_open(journal: &mut Journal<'_>) -> Result<(), JournalError> {
    if let Some(ctx) = active_policy() {
        return observe(journal, &ctx);
    }
    let source = PolicySource::discover(journal.world(), false);
    let state_dir = journal.state_dir().to_path_buf();
    let tree_root = journal.world().tree_root().to_path_buf();
    if (source != Source::None || read_seen(journal.world(), &state_dir).0.is_some())
        && let Ok(ctx) = resolve_with_source(journal.world(), &state_dir, &tree_root, false, source)
    {
        return observe(journal, &ctx);
    }
    Ok(())
}

/// Point-in-time policy snapshot produced by [`PolicyProvider::snapshot`] before each outbound
/// operation (`policy-load` step 13).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicySnapshot {
    /// Resolved policy context.
    pub ctx: PolicyCtx,
    /// Merged effective policy (policy state + owner narrowing).
    pub effective: EffectivePolicy,
    /// Current OS or test actor principal.
    pub principal: Principal,
    /// Operating-system writer check result for the policy file and parent folder.
    pub writers: Result<(), WriterWhy>,
    /// Current operation timestamp (`World::now()`, never a captured session clock).
    pub now: Now,
    /// Timestamp of the newest `audit.export` event in the journal, if any.
    pub last_audit_export: Option<Now>,
    /// Computed audit export freshness age (`Shipped`) at `self.now`.
    pub shipped: Shipped,
    /// 64-character lowercase SHA-256 policy or observation hash.
    pub policy_hash: Sha256,
}

impl PolicySnapshot {
    /// Builds a [`PolicySnapshot`] from explicit components (used by [`WorldPolicyProvider`] and
    /// tests constructing queued snapshots).
    #[must_use]
    pub fn from_parts(
        ctx: PolicyCtx,
        owner: &OwnerSettings,
        principal: Principal,
        now: Now,
        last_audit_export: Option<Now>,
    ) -> Self {
        let effective = EffectivePolicy::new(&ctx.state, owner);
        let shipped = egress::shipped(last_audit_export, now);
        let policy_hash = match &ctx.state {
            PolicyState::Floor(_) | PolicyState::Loaded(_) => {
                Sha256::parse(effective.hash()).unwrap_or_else(|_| ctx.hash.clone())
            }
            PolicyState::Closed(_) => ctx.hash.clone(),
        };
        let writers = ctx.writers;
        Self {
            ctx,
            effective,
            principal,
            writers,
            now,
            last_audit_export,
            shipped,
            policy_hash,
        }
    }

    /// Runs `egress::check` against this fresh snapshot's effective policy, clock, and export age.
    #[must_use]
    pub fn authorize(
        &self,
        dest: &Dest,
        labels: Option<&LabelSet>,
        delivery: Option<&Delivery>,
    ) -> Decision {
        egress::check(
            &self.effective,
            dest,
            labels,
            delivery,
            self.now,
            self.shipped,
        )
    }
}

/// Provider of a fresh [`PolicySnapshot`] before each outbound operation (`policy-load` step 13).
pub trait PolicyProvider {
    /// Reads and resolves a fresh [`PolicySnapshot`].
    ///
    /// # Errors
    ///
    /// Returns [`Refusal`] (`FB0414`) if policy placement is invalid.
    fn snapshot(&self) -> Result<PolicySnapshot, Refusal>;
}

/// Production [`PolicyProvider`] that reads through [`World`] on every [`Self::snapshot`] call.
pub struct WorldPolicyProvider<'a> {
    world: &'a dyn World,
    state: PathBuf,
    corpus: PathBuf,
    laptop: bool,
    source: Option<Source>,
}

impl<'a> WorldPolicyProvider<'a> {
    /// Creates a production [`WorldPolicyProvider`] reading from `world`.
    #[must_use]
    pub fn new(
        world: &'a dyn World,
        state: impl Into<PathBuf>,
        corpus: impl Into<PathBuf>,
        laptop: bool,
    ) -> Self {
        Self {
            world,
            state: state.into(),
            corpus: corpus.into(),
            laptop,
            source: None,
        }
    }

    /// Creates a [`WorldPolicyProvider`] with an explicit policy [`Source`].
    #[must_use]
    pub fn with_source(
        world: &'a dyn World,
        state: impl Into<PathBuf>,
        corpus: impl Into<PathBuf>,
        laptop: bool,
        source: Source,
    ) -> Self {
        Self {
            world,
            state: state.into(),
            corpus: corpus.into(),
            laptop,
            source: Some(source),
        }
    }
}

impl PolicyProvider for WorldPolicyProvider<'_> {
    fn snapshot(&self) -> Result<PolicySnapshot, Refusal> {
        let src = self
            .source
            .clone()
            .unwrap_or_else(|| PolicySource::discover(self.world, self.laptop));
        let ctx = resolve_with_source(self.world, &self.state, &self.corpus, self.laptop, src)
            .map_err(|e| e.to_refusal())?;
        let owner = owner_settings(self.world);
        let now = self.world.now();
        let events = read_valid_events_prefix(&self.state);
        let last_audit_export = events
            .iter()
            .rev()
            .find(|e| e.kind() == Kind::AuditExport)
            .map(|e| e.time);
        Ok(PolicySnapshot::from_parts(
            ctx,
            &owner,
            self.world.principal(),
            now,
            last_audit_export,
        ))
    }
}

/// Test [`PolicyProvider`] returning a pre-queued sequence of [`PolicySnapshot`] values in order.
pub struct QueuedPolicyProvider {
    queue: Mutex<VecDeque<Result<PolicySnapshot, Refusal>>>,
    last: Mutex<Option<Result<PolicySnapshot, Refusal>>>,
}

impl QueuedPolicyProvider {
    /// Creates a [`QueuedPolicyProvider`] from a sequence of [`PolicySnapshot`] items.
    #[must_use]
    pub fn new(snapshots: impl IntoIterator<Item = PolicySnapshot>) -> Self {
        let queue = snapshots.into_iter().map(Ok).collect();
        Self {
            queue: Mutex::new(queue),
            last: Mutex::new(None),
        }
    }

    /// Pushes another [`PolicySnapshot`] onto the back of the queue.
    pub fn push(&self, snapshot: PolicySnapshot) {
        if let Ok(mut q) = self.queue.lock() {
            q.push_back(Ok(snapshot));
        }
    }
}

impl PolicyProvider for QueuedPolicyProvider {
    fn snapshot(&self) -> Result<PolicySnapshot, Refusal> {
        if let Ok(mut q) = self.queue.lock()
            && let Some(item) = q.pop_front()
        {
            if let Ok(mut last) = self.last.lock() {
                *last = Some(item.clone());
            }
            return item;
        }
        if let Ok(last) = self.last.lock()
            && let Some(item) = last.as_ref()
        {
            return item.clone();
        }
        let floor_state = PolicyState::Floor(FloorKind::A);
        let hex = policy_hash(&floor_state);
        let hash = Sha256::parse(&hex).unwrap_or_else(|_| Sha256::digest(hex.as_bytes()));
        let resolved = resolve_pure(FileObservation::absent(), None, false);
        let ctx = PolicyCtx {
            state: floor_state,
            path: PathBuf::new(),
            source: Source::None,
            digest: ObservationDigest::Absent,
            writers: Ok(()),
            hash,
            seen_read: None,
            seen_to_record: None,
            outcome: None,
            note: None,
            finding: None,
            resolved,
            seen_unreadable: false,
        };
        let now = Now::from_unix_secs(1_772_323_200).unwrap_or_else(|_| unreachable!());
        Ok(PolicySnapshot::from_parts(
            ctx,
            &OwnerSettings::unset(),
            Principal::local(),
            now,
            None,
        ))
    }
}

/// Executes one outbound session operation by acquiring a fresh [`PolicySnapshot`] from `provider`,
/// running `egress::check`, and invoking `write_sink` only when authorized (`policy-load` step 13).
///
/// # Errors
///
/// Returns the denial [`Code`] without invoking `write_sink` when refused.
pub fn execute_session_operation<F>(
    provider: &dyn PolicyProvider,
    dest: &Dest,
    labels: &LabelSet,
    delivery: &Delivery,
    write_sink: F,
) -> Result<Allowed, Code>
where
    F: FnOnce(&Allowed),
{
    let snap = provider.snapshot().map_err(|r| r.code)?;
    match snap.authorize(dest, Some(labels), Some(delivery)) {
        Decision::Allowed(allowed) => {
            write_sink(&allowed);
            Ok(allowed)
        }
        Decision::Denied(denial) => Err(denial.code),
    }
}

/// Dispatches `fieldbook policy show` and `fieldbook policy check`.
#[must_use]
pub fn run(args: &PolicyArgs, ctx: &PolicyCtx, state_dir: &Path, world: &dyn World) -> Outcome {
    match &args.command {
        PolicySubcommand::Show(show_args) => run_show(show_args, ctx, world),
        PolicySubcommand::Check(check_args) => run_check(check_args, state_dir, world),
    }
}

/// Runs `fieldbook policy show [--json]`.
#[must_use]
pub fn run_show(args: &PolicyShowArgs, ctx: &PolicyCtx, world: &dyn World) -> Outcome {
    let eff = ctx.effective_policy(world);
    let today = world.now().date;
    let shown = show_at(
        &ctx.resolved,
        &eff,
        ctx.seen_read.as_ref(),
        &ctx.source,
        today,
    );
    let code = if ctx.state.is_closed() {
        EXIT_FINDINGS
    } else {
        EXIT_OK
    };
    let stdout = if args.json {
        let mut json = serde_json::to_string_pretty(&shown.report).unwrap_or_default();
        json.push('\n');
        json
    } else {
        shown.text
    };
    let mut stderr = String::new();
    if ctx.seen_unreadable {
        stderr.push_str(UNREADABLE_SEEN_WARNING);
        stderr.push('\n');
    }
    Outcome {
        stdout,
        stderr,
        code,
    }
}

/// Runs `fieldbook policy check FILE [--json]`.
///
/// Reads `FILE` with [`PolicySource::read_draft`], skips `resolve_place` and the writer check,
/// reads `policy-seen.toml` if the state folder exists (creating no state folder), and validates
/// the draft without reading the clock.
#[must_use]
pub fn run_check(args: &PolicyCheckArgs, state_dir: &Path, world: &dyn World) -> Outcome {
    let draft_path = if args.file.is_absolute() {
        args.file.clone()
    } else {
        world.cwd().join(&args.file)
    };
    let obs = PolicySource::read_draft(&draft_path);
    match &obs.fact {
        FileFact::Absent | FileFact::Unreadable => Outcome {
            stdout: String::new(),
            stderr: format!("error: cannot read policy file {}\n", args.file.display()),
            code: EXIT_USAGE,
        },
        FileFact::TooLarge => Outcome {
            stdout: String::new(),
            stderr: "error[FB0413]: file exceeds 1 MiB limit at policy\n".to_owned(),
            code: EXIT_FINDINGS,
        },
        FileFact::NotUtf8 { utf16: true } => Outcome {
            stdout: String::new(),
            stderr: "error[FB0413]: the file is UTF-16; save it as UTF-8 at policy\n".to_owned(),
            code: EXIT_FINDINGS,
        },
        FileFact::NotUtf8 { utf16: false } => Outcome {
            stdout: String::new(),
            stderr: "error[FB0413]: text is not valid UTF-8 at policy\n".to_owned(),
            code: EXIT_FINDINGS,
        },
        FileFact::Text(text) => {
            let (seen, seen_unreadable) = if world.exists(state_dir) {
                read_seen(world, state_dir)
            } else {
                (None, false)
            };
            let recorded_rev = seen.as_ref().map(|s| s.revision);
            let mut stderr = String::new();
            if seen_unreadable {
                stderr.push_str(UNREADABLE_SEEN_WARNING);
                stderr.push('\n');
            }
            for (line_no, msg) in find_placeholder_warnings(text) {
                let _ = writeln!(stderr, "warning: line {line_no}: {msg}");
            }

            // First validate the draft's schema and rules without recorded revision so we can
            // distinguish a schema error from `would close: revision <N> is lower than the recorded <M>`.
            match Policy::parse(text, None) {
                Ok(policy) => {
                    if let Some(recorded) = recorded_rev
                        && policy.revision() < recorded
                    {
                        let _ = writeln!(
                            stderr,
                            "error[FB0413]: would close: revision {} is lower than the recorded {recorded} at revision",
                            policy.revision()
                        );
                        return Outcome {
                            stdout: String::new(),
                            stderr,
                            code: EXIT_FINDINGS,
                        };
                    }
                    let mut stdout = String::new();
                    let _ = writeln!(
                        stdout,
                        "policy ok: revision {}, {} labels, {} rows",
                        policy.revision(),
                        policy.labels().len(),
                        policy.destinations().len()
                    );
                    let _ = writeln!(stdout, "hash: {}", policy.hash());
                    Outcome {
                        stdout,
                        stderr,
                        code: EXIT_OK,
                    }
                }
                Err(err) => {
                    match err.label_id() {
                        Some(id) => {
                            let _ = writeln!(
                                stderr,
                                "error[FB0413]: {} at {} (label {})",
                                err.rule(),
                                err.key_path(),
                                id.as_str()
                            );
                        }
                        None => {
                            let _ = writeln!(
                                stderr,
                                "error[FB0413]: {} at {}",
                                err.rule(),
                                err.key_path()
                            );
                        }
                    }
                    Outcome {
                        stdout: String::new(),
                        stderr,
                        code: EXIT_FINDINGS,
                    }
                }
            }
        }
    }
}

fn find_placeholder_warnings(text: &str) -> Vec<(usize, &'static str)> {
    const PLACEHOLDER_TOKENS: &[(&str, &str)] = &[
        ("(example)", "value looks like an example placeholder"),
        ("c:\\example\\", "path looks like an example placeholder"),
        ("chg-example", "reference looks like an example placeholder"),
        ("example-gateway", "name looks like an example placeholder"),
        (
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "client hash looks like a placeholder",
        ),
        ("todo", "value contains a TODO placeholder"),
        ("changeme", "value contains a CHANGEME placeholder"),
    ];

    let mut warnings = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let value_part = match trimmed.split_once('=') {
            Some((_, rhs)) => rhs.trim(),
            None => trimmed,
        };
        let lower = value_part.to_ascii_lowercase();
        for &(needle, msg) in PLACEHOLDER_TOKENS {
            if lower.contains(needle) {
                warnings.push((idx + 1, msg));
                break;
            }
        }
    }
    warnings
}
