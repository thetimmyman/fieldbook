//! Compiled-in test doubles for the model gate (`model-gate` step 8).
//!
//! Nothing here is used by a shipped verb. The doubles let a test drive the real gate without a
//! network, a key or a clock, and let it obtain an [`Allowed`] token only through `egress::check`
//! over an inline policy.

#![expect(
    clippy::expect_used,
    clippy::panic,
    clippy::type_complexity,
    clippy::needless_pass_by_value,
    clippy::match_same_arms,
    reason = "test doubles use fixed synthetic values and fail loudly when a test is wrong"
)]

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use fieldbook_core::audit::{DestKind, EndpointName, ModelId, Now, Sha256, StoreId, Word};
use fieldbook_core::check::Code;
use fieldbook_core::egress::{Allowed, Operation, Shipped, Target};
use fieldbook_core::labels::{ItemRef, LabelSet};
use fieldbook_core::policy::{EffectivePolicy, OwnerSettings, Policy, PolicyState};
use fieldbook_core::ports::{Caps, HandoffId, HandoffTicket, ModelRequest, Schema};

use crate::adapters::model::{
    AiJournal, Attempt, AttemptError, BeginRecord, CallEvent, CallRecord, CallStore, Deps,
    ErrorKind, Finish, KeepSettings, ModelAdapter, ModelAnswer, ModelError, Pace, PackEvent,
    PackText, Permanent, PolicySnapshot, PreparationContext, PreparedAttempt, Provenance, RawReply,
    Reason, Route, RunBudget, StoredTicket, TicketMeta, Transient,
};
use crate::io::keys::{KeySource, ScriptedKeys};
use crate::io::net::{Avail, HttpError, HttpPost, HttpReply, HttpRequest};

/// Parses an inline policy text and applies `owner` to it, for a test that needs a policy.
///
/// # Panics
///
/// Panics when the inline policy does not parse; a test's own text is under its control.
#[must_use]
pub fn test_policy(text: &str, owner: &OwnerSettings) -> EffectivePolicy {
    let policy = Policy::parse(text, None).expect("inline test policy parses");
    EffectivePolicy::new(&PolicyState::Loaded(policy), owner)
}

/// Builds a usable [`Deps`] from a journal, store and budget, with no transport.
#[must_use]
pub fn deps(journal: Arc<dyn AiJournal>, store: Arc<dyn CallStore>) -> Deps {
    Deps {
        journal,
        keys: Arc::new(ScriptedKeys::new()),
        pace: Arc::new(ScriptedPace::default()),
        policy_provider: Arc::new(|| panic!("a test sets its own policy provider")),
        store,
        keep: KeepSettings {
            calls: true,
            days: 30,
        },
        budget: Arc::new(RunBudget::new(100)),
        avail: Avail::real(),
        retries: 2,
        http: None,
    }
}

/// A scripted model adapter: a queue of results, a record of prepared attempts and their count.
pub struct ScriptedModel {
    id: &'static str,
    caps: Caps,
    replies: Mutex<VecDeque<Result<Attempt, AttemptError>>>,
    attempts: Mutex<Vec<PreparedRecord>>,
    count: AtomicUsize,
    prepare_suffix: &'static str,
}

impl Default for ScriptedModel {
    fn default() -> Self {
        Self::with_caps(Caps::new(true, true, false, false, 1_000_000))
    }
}

/// One attempt as the double saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedRecord {
    /// Exact prepared bytes.
    pub bytes: Vec<u8>,
    /// Whether the token handed to the adapter was delivery-bound.
    pub has_delivery: bool,
    /// Endpoint of the route.
    pub endpoint: String,
}

impl ScriptedModel {
    /// Constructs a scripted adapter that answers with `text`.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self::with_caps(Caps::new(true, true, false, false, 1_000_000)).push(Ok(Attempt::Reply(
            RawReply {
                body: text.into(),
                tokens_in: 10,
                tokens_out: 5,
                finish: Finish::Stop,
            },
        )))
    }

    /// Constructs an empty scripted adapter with explicit capabilities.
    #[must_use]
    pub fn with_caps(caps: Caps) -> Self {
        Self {
            id: "scripted@1",
            caps,
            replies: Mutex::new(VecDeque::new()),
            attempts: Mutex::new(Vec::new()),
            count: AtomicUsize::new(0),
            prepare_suffix: "",
        }
    }

    /// Makes `prepare` add `text` to the body, as an adapter that wraps the request would.
    #[must_use]
    pub const fn with_prepare_suffix(mut self, text: &'static str) -> Self {
        self.prepare_suffix = text;
        self
    }

    /// Queues one result and returns `self` for chaining.
    #[must_use]
    pub fn push(self, result: Result<Attempt, AttemptError>) -> Self {
        self.replies
            .lock()
            .expect("scripted replies")
            .push_back(result);
        self
    }

    /// Queues one reply and returns `self` for chaining.
    #[must_use]
    pub fn push_reply(self, text: impl Into<String>) -> Self {
        self.push(Ok(Attempt::Reply(RawReply {
            body: text.into(),
            tokens_in: 10,
            tokens_out: 5,
            finish: Finish::Stop,
        })))
    }

    /// Returns the prepared attempts the adapter saw.
    #[must_use]
    pub fn attempts(&self) -> Vec<PreparedRecord> {
        self.attempts.lock().map(|a| a.clone()).unwrap_or_default()
    }

    /// Returns how many attempts were made.
    #[must_use]
    pub fn count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}

impl ModelAdapter for ScriptedModel {
    fn id(&self) -> &'static str {
        self.id
    }

    fn caps(&self) -> Caps {
        self.caps
    }

    fn prepare(
        &self,
        req: &ModelRequest,
        route: &Route,
        _context: &PreparationContext,
    ) -> Result<PreparedAttempt, AttemptError> {
        let body =
            format!("{}\n{}{}", req.system(), req.prompt(), self.prepare_suffix).into_bytes();
        let target = if self.caps.network {
            Target::Endpoint(route.base_url.clone())
        } else {
            Target::Reply("handoff".to_owned())
        };
        Ok(PreparedAttempt {
            target,
            bytes: body,
            content_type: "application/json".to_owned(),
        })
    }

    fn attempt(
        &self,
        allowed: &Allowed,
        prepared: &PreparedAttempt,
        route: &Route,
    ) -> Result<Attempt, AttemptError> {
        self.count.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut attempts) = self.attempts.lock() {
            attempts.push(PreparedRecord {
                bytes: prepared.bytes.clone(),
                has_delivery: allowed.has_delivery(),
                endpoint: route.base_url.clone(),
            });
        }
        self.replies
            .lock()
            .ok()
            .and_then(|mut queue| queue.pop_front())
            .unwrap_or(Err(AttemptError::Transient(Transient::Connect)))
    }
}

/// A model adapter whose attempt always fails the test.
#[derive(Debug, Default, Clone, Copy)]
pub struct PanicModel;

impl ModelAdapter for PanicModel {
    fn id(&self) -> &'static str {
        "scripted@1"
    }

    fn caps(&self) -> Caps {
        Caps::new(true, false, false, false, 1_000_000)
    }

    fn prepare(
        &self,
        req: &ModelRequest,
        route: &Route,
        _context: &PreparationContext,
    ) -> Result<PreparedAttempt, AttemptError> {
        Ok(PreparedAttempt {
            target: Target::Endpoint(route.base_url.clone()),
            bytes: format!("{}\n{}", req.system(), req.prompt()).into_bytes(),
            content_type: "application/json".to_owned(),
        })
    }

    fn attempt(
        &self,
        _allowed: &Allowed,
        _prepared: &PreparedAttempt,
        _route: &Route,
    ) -> Result<Attempt, AttemptError> {
        panic!("the gate reached the adapter")
    }
}

/// The copy-paste route's pack adapter: renders a pack and sends nothing.
#[derive(Debug, Default, Clone, Copy)]
pub struct HandoffAdapter;

impl ModelAdapter for HandoffAdapter {
    fn id(&self) -> &'static str {
        "handoff@1"
    }

    fn caps(&self) -> Caps {
        Caps::new(false, false, false, false, 1_000_000)
    }

    fn prepare(
        &self,
        req: &ModelRequest,
        _route: &Route,
        _context: &PreparationContext,
    ) -> Result<PreparedAttempt, AttemptError> {
        let text = format!(
            "-----BEGIN PACK-----\n{}\n{}\n-----END PACK-----\n",
            req.system(),
            req.prompt()
        );
        Ok(PreparedAttempt {
            target: Target::Reply("handoff".to_owned()),
            bytes: text.into_bytes(),
            content_type: "text/plain".to_owned(),
        })
    }

    fn attempt(
        &self,
        _allowed: &Allowed,
        prepared: &PreparedAttempt,
        _route: &Route,
    ) -> Result<Attempt, AttemptError> {
        Ok(Attempt::Pack(PackText {
            text: String::from_utf8_lossy(&prepared.bytes).into_owned(),
        }))
    }
}

/// A model adapter that blocks inside `attempt` until the test releases it.
#[derive(Debug, Default)]
pub struct BlockingModel {
    state: Arc<(Mutex<BlockState>, std::sync::Condvar)>,
}

#[derive(Debug, Default)]
struct BlockState {
    started: bool,
    released: bool,
}

impl BlockingModel {
    /// Constructs a blocking adapter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Waits until the adapter has entered `attempt`.
    pub fn wait_started(&self) {
        let (lock, condvar) = &*self.state;
        let mut state = lock.lock().expect("blocking state");
        while !state.started {
            state = condvar.wait(state).expect("blocking wait");
        }
    }

    /// Lets the adapter return.
    pub fn release(&self) {
        let (lock, condvar) = &*self.state;
        let mut state = lock.lock().expect("blocking state");
        state.released = true;
        condvar.notify_all();
    }
}

impl ModelAdapter for BlockingModel {
    fn id(&self) -> &'static str {
        "scripted@1"
    }

    fn caps(&self) -> Caps {
        Caps::new(true, false, false, false, 1_000_000)
    }

    fn prepare(
        &self,
        req: &ModelRequest,
        route: &Route,
        _context: &PreparationContext,
    ) -> Result<PreparedAttempt, AttemptError> {
        Ok(PreparedAttempt {
            target: Target::Endpoint(route.base_url.clone()),
            bytes: format!("{}\n{}", req.system(), req.prompt()).into_bytes(),
            content_type: "application/json".to_owned(),
        })
    }

    fn attempt(
        &self,
        _allowed: &Allowed,
        _prepared: &PreparedAttempt,
        _route: &Route,
    ) -> Result<Attempt, AttemptError> {
        let (lock, condvar) = &*self.state;
        let mut state = lock.lock().expect("blocking state");
        state.started = true;
        condvar.notify_all();
        while !state.released {
            state = condvar.wait(state).expect("blocking wait");
        }
        Ok(Attempt::Reply(RawReply {
            body: "ok".to_owned(),
            tokens_in: 1,
            tokens_out: 1,
            finish: Finish::Stop,
        }))
    }
}

/// A pacer that records the sleeps and does not sleep.
#[derive(Debug, Default)]
pub struct ScriptedPace {
    sleeps: Mutex<Vec<u64>>,
}

impl ScriptedPace {
    /// Returns the recorded sleeps in order.
    #[must_use]
    pub fn sleeps(&self) -> Vec<u64> {
        self.sleeps.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

impl Pace for ScriptedPace {
    fn sleep_ms(&self, ms: u64) {
        if let Ok(mut sleeps) = self.sleeps.lock() {
            sleeps.push(ms);
        }
    }
}

/// One recorded refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefusalRecord {
    /// Destination of the refused request, when one is known.
    pub dest: Option<DestKind>,
    /// Request labels.
    pub labels: LabelSet,
    /// Diagnostic code.
    pub code: Code,
    /// Closed reason word.
    pub reason: &'static str,
}

/// One recorded paste.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasteRecord {
    /// Sequence number of the originating pack.
    pub pack_seq: Option<u64>,
    /// SHA-256 of the pasted text.
    pub output_sha256: Sha256,
    /// Labels of the pasted content.
    pub labels: LabelSet,
}

/// An in-memory journal that records every event the gate appends.
#[derive(Debug, Default)]
pub struct MemJournal {
    begins: Mutex<Vec<BeginRecord>>,
    calls: Mutex<Vec<(u64, CallEvent)>>,
    refusals: Mutex<Vec<RefusalRecord>>,
    pastes: Mutex<Vec<PasteRecord>>,
    next: AtomicU64,
    audited: AtomicBool,
    fail_begin: AtomicBool,
    fail_finish_on: AtomicUsize,
    finish_count: AtomicUsize,
}

impl MemJournal {
    /// Constructs an audited in-memory journal.
    #[must_use]
    pub fn new() -> Self {
        Self {
            audited: AtomicBool::new(true),
            ..Self::default()
        }
    }

    /// Sets whether the journal reports that it can take an append.
    pub fn set_audited(&self, audited: bool) {
        self.audited.store(audited, Ordering::SeqCst);
    }

    /// Makes every `begin` append fail.
    pub fn fail_begin(&self) {
        self.fail_begin.store(true, Ordering::SeqCst);
    }

    /// Makes the nth (1-based) `finish` append fail.
    pub fn fail_finish_on(&self, n: usize) {
        self.fail_finish_on.store(n, Ordering::SeqCst);
    }

    /// Returns every recorded first event.
    #[must_use]
    pub fn begins(&self) -> Vec<BeginRecord> {
        self.begins.lock().map(|b| b.clone()).unwrap_or_default()
    }

    /// Returns every recorded `ai.call`.
    #[must_use]
    pub fn calls(&self) -> Vec<(u64, CallEvent)> {
        self.calls.lock().map(|c| c.clone()).unwrap_or_default()
    }

    /// Returns every recorded refusal.
    #[must_use]
    pub fn refusals(&self) -> Vec<RefusalRecord> {
        self.refusals.lock().map(|r| r.clone()).unwrap_or_default()
    }

    /// Returns every recorded paste.
    #[must_use]
    pub fn pastes(&self) -> Vec<PasteRecord> {
        self.pastes.lock().map(|p| p.clone()).unwrap_or_default()
    }

    /// Returns the `tx` values that have a completed first event and `ai.call` pair.
    #[must_use]
    pub fn completed_pairs(&self) -> Vec<u64> {
        self.calls().into_iter().map(|(tx, _)| tx).collect()
    }
}

impl AiJournal for MemJournal {
    fn begin(&self, record: BeginRecord) -> Result<u64, ModelError> {
        if self.fail_begin.load(Ordering::SeqCst) || !self.audited.load(Ordering::SeqCst) {
            return Err(ModelError::new(Code::Fb0903, ErrorKind::Availability));
        }
        let tx = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        if let Ok(mut begins) = self.begins.lock() {
            begins.push(record);
        }
        Ok(tx)
    }

    fn finish(&self, tx: u64, event: CallEvent) -> Result<(), ModelError> {
        let count = self.finish_count.fetch_add(1, Ordering::SeqCst) + 1;
        if count == self.fail_finish_on.load(Ordering::SeqCst) {
            return Err(ModelError::new(Code::Fb0904, ErrorKind::Availability));
        }
        if let Ok(mut calls) = self.calls.lock() {
            calls.push((tx, event));
        }
        Ok(())
    }

    fn refuse(
        &self,
        dest: Option<DestKind>,
        labels: &LabelSet,
        code: Code,
        reason: Word,
    ) -> Result<(), ModelError> {
        if let Ok(mut refusals) = self.refusals.lock() {
            refusals.push(RefusalRecord {
                dest,
                labels: labels.clone(),
                code,
                reason: reason.as_str(),
            });
        }
        Ok(())
    }

    fn paste(
        &self,
        pack_seq: Option<u64>,
        output_sha256: Sha256,
        labels: &LabelSet,
    ) -> Result<(), ModelError> {
        if let Ok(mut pastes) = self.pastes.lock() {
            pastes.push(PasteRecord {
                pack_seq,
                output_sha256,
                labels: labels.clone(),
            });
        }
        Ok(())
    }

    fn audited(&self) -> bool {
        self.audited.load(Ordering::SeqCst)
    }
}

/// A journal that wraps a [`MemJournal`] and fails the appends a test names.
#[derive(Debug, Default)]
pub struct FaultyJournal {
    inner: MemJournal,
}

impl FaultyJournal {
    /// Constructs an audited faulty journal.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: MemJournal::new(),
        }
    }

    /// Makes every `begin` append fail.
    pub fn fail_begin(&self) {
        self.inner.fail_begin();
    }

    /// Makes the nth (1-based) `finish` append fail.
    pub fn fail_finish_on(&self, n: usize) {
        self.inner.fail_finish_on(n);
    }

    /// Returns the wrapped journal.
    #[must_use]
    pub const fn inner(&self) -> &MemJournal {
        &self.inner
    }
}

impl AiJournal for FaultyJournal {
    fn begin(&self, record: BeginRecord) -> Result<u64, ModelError> {
        self.inner.begin(record)
    }

    fn finish(&self, tx: u64, event: CallEvent) -> Result<(), ModelError> {
        self.inner.finish(tx, event)
    }

    fn refuse(
        &self,
        dest: Option<DestKind>,
        labels: &LabelSet,
        code: Code,
        reason: Word,
    ) -> Result<(), ModelError> {
        self.inner.refuse(dest, labels, code, reason)
    }

    fn paste(
        &self,
        pack_seq: Option<u64>,
        output_sha256: Sha256,
        labels: &LabelSet,
    ) -> Result<(), ModelError> {
        self.inner.paste(pack_seq, output_sha256, labels)
    }

    fn audited(&self) -> bool {
        self.inner.audited()
    }
}

/// In-memory call and pack storage.
#[derive(Debug, Default)]
pub struct MemCalls {
    calls: Mutex<Vec<CallRecord>>,
    packs: Mutex<BTreeMap<u32, (Vec<u8>, TicketMeta, bool)>>,
    next: Mutex<u32>,
    fail_put_call: AtomicBool,
    fail_put_pack: AtomicBool,
}

impl MemCalls {
    /// Constructs empty in-memory storage.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes every call-record write fail with a warning.
    pub fn fail_put_call(&self) {
        self.fail_put_call.store(true, Ordering::SeqCst);
    }

    /// Makes every Handoff pack write fail with a warning.
    pub fn fail_put_pack(&self) {
        self.fail_put_pack.store(true, Ordering::SeqCst);
    }

    /// Returns how many Handoff packs are stored.
    #[must_use]
    pub fn pack_count(&self) -> usize {
        self.packs.lock().map(|p| p.len()).unwrap_or(0)
    }

    /// Returns the recorded calls.
    #[must_use]
    pub fn calls(&self) -> Vec<CallRecord> {
        self.calls.lock().map(|c| c.clone()).unwrap_or_default()
    }

    /// Returns the stored pack bytes for a ticket.
    #[must_use]
    pub fn pack(&self, id: HandoffId) -> Option<Vec<u8>> {
        self.packs
            .lock()
            .ok()
            .and_then(|p| p.get(&id.value()).map(|(bytes, _, _)| bytes.clone()))
    }
}

impl CallStore for MemCalls {
    fn put_call(&self, record: &CallRecord) -> Result<(), String> {
        if self.fail_put_call.load(Ordering::SeqCst) {
            return Err("warning: the call record could not be written".to_owned());
        }
        if let Ok(mut calls) = self.calls.lock() {
            let today = days_from_civil(record.now.date);
            calls.retain(|old| {
                let age = today.saturating_sub(days_from_civil(old.now.date));
                age <= u64::from(old.keep_days)
            });
            calls.push(record.clone());
        }
        Ok(())
    }

    fn put_pack(&self, pack: &[u8], meta: &TicketMeta) -> Result<HandoffTicket, String> {
        if self.fail_put_pack.load(Ordering::SeqCst) {
            return Err("warning: the Handoff pack could not be written".to_owned());
        }
        let mut next = self.next.lock().map_err(|_| "pack lock".to_owned())?;
        *next += 1;
        let id = HandoffId::new(*next).ok_or_else(|| "no ticket number left".to_owned())?;
        let ticket = HandoffTicket::new(id, Sha256::digest(pack));
        if let Ok(mut packs) = self.packs.lock() {
            packs.insert(id.value(), (pack.to_vec(), meta.clone(), false));
        }
        Ok(ticket)
    }

    fn get_ticket(&self, id: HandoffId) -> Result<Option<StoredTicket>, String> {
        let packs = self.packs.lock().map_err(|_| "pack lock".to_owned())?;
        Ok(packs.get(&id.value()).map(|(_, meta, used)| StoredTicket {
            id,
            meta: meta.clone(),
            used: *used,
        }))
    }

    fn mark_used(&self, id: HandoffId) -> Result<(), String> {
        let mut packs = self.packs.lock().map_err(|_| "pack lock".to_owned())?;
        if let Some((_, _, used)) = packs.get_mut(&id.value()) {
            *used = true;
        }
        Ok(())
    }
}

/// A model adapter that sends through [`HttpPost`], for the gate's transport tests.
pub struct HttpModel {
    id: &'static str,
    http: Arc<dyn HttpPost + Send + Sync>,
    keys: Arc<dyn KeySource + Send + Sync>,
    content_type: &'static str,
    endpoint_suffix: &'static str,
}

impl HttpModel {
    /// Constructs a transport-backed adapter.
    #[must_use]
    pub fn new(
        http: Arc<dyn HttpPost + Send + Sync>,
        keys: Arc<dyn KeySource + Send + Sync>,
    ) -> Self {
        Self {
            id: "scripted@1",
            http,
            keys,
            content_type: "application/json",
            endpoint_suffix: "",
        }
    }

    /// Declares the prepared body's content type, as an adapter that sends plain text would.
    #[must_use]
    pub const fn with_content_type(mut self, content_type: &'static str) -> Self {
        self.content_type = content_type;
        self
    }

    /// Appends `suffix` to the route's `base_url`, as an OpenAI-compatible adapter would.
    #[must_use]
    pub const fn with_endpoint_suffix(mut self, suffix: &'static str) -> Self {
        self.endpoint_suffix = suffix;
        self
    }

    fn endpoint(&self, route: &Route) -> String {
        format!("{}{}", route.base_url, self.endpoint_suffix)
    }

    fn map_reply(&self, route: &Route, reply: HttpReply) -> Result<Attempt, AttemptError> {
        match reply.status {
            200..=299 => {
                if reply.truncated {
                    return Err(AttemptError::Permanent(Permanent::Truncated));
                }
                if reply.body.len() as u64 > route.max_reply_bytes {
                    return Err(AttemptError::Permanent(Permanent::TooLarge));
                }
                Ok(Attempt::Reply(RawReply {
                    body: String::from_utf8_lossy(&reply.body).into_owned(),
                    tokens_in: reply.tokens_in,
                    tokens_out: reply.tokens_out,
                    finish: if reply.truncated {
                        Finish::Length
                    } else {
                        Finish::Stop
                    },
                }))
            }
            300..=399 => Err(AttemptError::Permanent(Permanent::Redirect)),
            401 | 403 => Err(AttemptError::Permanent(Permanent::Auth)),
            413 => Err(AttemptError::Permanent(Permanent::Rejected(413))),
            429 => Err(AttemptError::Transient(Transient::RateLimited {
                retry_after_s: reply.retry_after_s,
            })),
            500..=599 => Err(AttemptError::Transient(Transient::Server(reply.status))),
            other => Err(AttemptError::Permanent(Permanent::Rejected(other))),
        }
    }
}

impl ModelAdapter for HttpModel {
    fn id(&self) -> &'static str {
        self.id
    }

    fn caps(&self) -> Caps {
        Caps::new(true, false, false, false, 1_000_000)
    }

    fn prepare(
        &self,
        req: &ModelRequest,
        route: &Route,
        _context: &PreparationContext,
    ) -> Result<PreparedAttempt, AttemptError> {
        let body = serde_json::json!({
            "system": req.system(),
            "prompt": req.prompt(),
        })
        .to_string()
        .into_bytes();
        Ok(PreparedAttempt {
            target: Target::Endpoint(self.endpoint(route)),
            bytes: body,
            content_type: self.content_type.to_owned(),
        })
    }

    fn attempt(
        &self,
        allowed: &Allowed,
        prepared: &PreparedAttempt,
        route: &Route,
    ) -> Result<Attempt, AttemptError> {
        let endpoint = self.endpoint(route);
        let target = Target::Endpoint(endpoint.clone());
        if !allowed.permits_transfer(&target, &Operation::Post, &prepared.bytes) {
            return Err(AttemptError::Permanent(Permanent::Unavailable(
                Reason::Binding,
            )));
        }
        let mut request = HttpRequest::new(endpoint, prepared.bytes.clone(), route.timeout_s);
        if let Some(name) = &route.key_env {
            let key = self
                .keys
                .key(name)
                .map_err(|_| AttemptError::Permanent(Permanent::Unavailable(Reason::Key)))?;
            request = request.secret_header("Authorization", key);
        }
        match self.http.post(allowed, &request) {
            Ok(reply) => self.map_reply(route, reply),
            Err(HttpError::Connect) => Err(AttemptError::Transient(Transient::Connect)),
            Err(HttpError::Timeout) => Err(AttemptError::Transient(Transient::Timeout)),
            Err(HttpError::Tls) => Err(AttemptError::Permanent(Permanent::Tls)),
            Err(HttpError::TooLarge) => Err(AttemptError::Permanent(Permanent::TooLarge)),
            Err(HttpError::Other) => Err(AttemptError::Transient(Transient::Connect)),
        }
    }
}

/// Constructs an [`ItemRef`] in the Corpus store for tests.
#[must_use]
pub fn corpus_item(id: impl Into<String>) -> ItemRef {
    ItemRef::new(StoreId::Corpus, id)
}

/// Constructs a model answer for a direct unit test.
#[must_use]
pub fn answer(text: impl Into<String>, labels: LabelSet) -> ModelAnswer {
    ModelAnswer {
        text: text.into(),
        request_labels: labels,
        inputs: Vec::new(),
        provenance: Provenance {
            adapter: "scripted@1".to_owned(),
            endpoint: "work".to_owned(),
            model: "test-model".to_owned(),
        },
        ticket: None,
        warnings: Vec::new(),
    }
}

/// Builds a route for a scripted test endpoint.
#[must_use]
pub fn route(name: &str, base_url: &str) -> Route {
    Route {
        name: EndpointName::parse(name).expect("endpoint name"),
        base_url: base_url.to_owned(),
        model: ModelId::parse("test-model").expect("model id"),
        timeout_s: 5,
        max_reply_bytes: 64 * 1024,
        key_env: None,
    }
}

/// Builds the Handoff route.
#[must_use]
pub fn handoff_route() -> Route {
    Route {
        name: EndpointName::parse("handoff").expect("endpoint name"),
        base_url: String::new(),
        model: ModelId::parse("handoff").expect("model id"),
        timeout_s: 0,
        max_reply_bytes: 64 * 1024,
        key_env: None,
    }
}

/// Builds a snapshot from an inline policy text and a fixed time.
#[must_use]
pub fn snapshot(text: &str, owner: &OwnerSettings, now: Now, shipped: Shipped) -> PolicySnapshot {
    PolicySnapshot {
        policy: test_policy(text, owner),
        now,
        shipped,
    }
}

/// An empty `ai.pack` payload, for tests that only need a well-formed record.
#[must_use]
pub fn empty_pack_event() -> PackEvent {
    PackEvent {
        prompt_sha256: Sha256::digest(b"pack"),
        context_ids: Vec::new(),
        dest: DestKind::Handoff,
        labels: LabelSet::public(),
        policy: Sha256::digest(b"policy"),
    }
}

const fn days_from_civil(date: fieldbook_core::CivilDate) -> u64 {
    let y = date.year() as i64 - if date.month() <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = date.month() as i64;
    let d = date.day() as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468).cast_unsigned()
}

/// A default model identifier, for tests that only need one.
#[must_use]
pub fn model_id() -> ModelId {
    ModelId::parse("test-model").expect("model id")
}

/// A default schema for tests that need a response schema.
#[must_use]
pub fn pick_schema() -> Schema {
    Schema::object(vec![fieldbook_core::ports::Field::required(
        "answer",
        Schema::string(1, 40),
    )])
}
