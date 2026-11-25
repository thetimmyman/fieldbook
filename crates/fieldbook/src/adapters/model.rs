//! The model port: request routing, adapter contract, run dependencies and call records
//! (`model-gate` step 1, `ADR 0001 AD-01`, `ADR 0004 §7`).
//!
//! The gate in `adapters/gate.rs` is the one way to a model. An adapter never sees the journal,
//! the clock or the policy; it is built with a [`KeySource`] and receives only the request, the
//! route, a [`PreparationContext`] and the [`Allowed`] token the gate issued.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

pub use crate::io::keys::KeySource;

use fieldbook_core::audit::{DestKind, EndpointName, ModelId, Now, RecordRef, Sha256, Word};
use fieldbook_core::check::Code;
use fieldbook_core::egress::{Allowed, Dest, Shipped, Target};
use fieldbook_core::labels::{ItemRef, LabelId, LabelSet};
use fieldbook_core::policy::{Cut, EffectivePolicy};
use fieldbook_core::ports::chat::SendPreview;
use fieldbook_core::ports::{
    Caps, HandoffId, HandoffTicket, KeyState, ModelRequest, Purpose, Schema,
};

use crate::io::net::{Avail, HttpPost};

/// One configured route to a model endpoint, or the copy-paste Handoff route.
///
/// A route holds no approval: whether a label may go to its endpoint is a row of the company's
/// policy, decided by `egress::check`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    /// Endpoint name (`handoff` for the copy-paste route).
    pub name: EndpointName,
    /// Endpoint base URL (empty for the Handoff route).
    pub base_url: String,
    /// Model identifier sent to the endpoint.
    pub model: ModelId,
    /// Request timeout in seconds.
    pub timeout_s: u32,
    /// Maximum reply body bytes.
    pub max_reply_bytes: u64,
    /// Environment variable that holds the endpoint's key, when it needs one.
    pub key_env: Option<String>,
}

/// How a reply ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finish {
    /// The endpoint stopped because the answer was complete.
    Stop,
    /// The endpoint stopped at a token limit.
    Length,
    /// The endpoint stopped because of a content filter.
    ContentFilter,
}

/// A model reply, before schema validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawReply {
    /// Reply body text.
    pub body: String,
    /// Input token count reported by the endpoint.
    pub tokens_in: u64,
    /// Output token count reported by the endpoint.
    pub tokens_out: u64,
    /// How the reply ended.
    pub finish: Finish,
}

/// The text of a Handoff pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackText {
    /// Exact pack text.
    pub text: String,
}

impl PackText {
    /// Returns the pack bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        self.text.as_bytes()
    }
}

/// What one adapter attempt returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attempt {
    /// A model reply to validate.
    Reply(RawReply),
    /// A Handoff pack to write.
    Pack(PackText),
}

/// A failure that may clear up on another attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transient {
    /// The connection could not be opened.
    Connect,
    /// The request timed out.
    Timeout,
    /// The endpoint rate-limited the request.
    RateLimited {
        /// The endpoint's `Retry-After` value in seconds, when it sent one.
        retry_after_s: Option<u32>,
    },
    /// The endpoint returned a 5xx status.
    Server(u16),
}

/// A failure that will not clear up on another attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permanent {
    /// Authentication was refused.
    Auth,
    /// The endpoint rejected the request with a 4xx status.
    Rejected(u16),
    /// The TLS handshake or certificate check failed.
    Tls,
    /// The reply exceeded the cap.
    TooLarge,
    /// The reply ended before it was complete.
    Truncated,
    /// The endpoint redirected the request.
    Redirect,
    /// The adapter or machine cannot make this call at all.
    Unavailable(Reason),
}

/// Why an adapter is unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// The `net` feature is not compiled in.
    Net,
    /// The CPU lacks a required feature.
    Cpu,
    /// The endpoint's key is missing.
    Key,
    /// The prepared target does not match the token.
    Binding,
}

/// Failure of one adapter attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptError {
    /// A failure that may clear up.
    Transient(Transient),
    /// A failure that will not clear up.
    Permanent(Permanent),
}

impl AttemptError {
    /// Returns `true` when another attempt may succeed.
    #[must_use]
    pub const fn is_transient(&self) -> bool {
        matches!(self, Self::Transient(_))
    }
}

impl fmt::Display for AttemptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transient(Transient::Connect) => f.write_str("connection failed"),
            Self::Transient(Transient::Timeout) => f.write_str("request timed out"),
            Self::Transient(Transient::RateLimited { .. }) => f.write_str("rate limited"),
            Self::Transient(Transient::Server(_)) => f.write_str("server error"),
            Self::Permanent(Permanent::Auth) => f.write_str("authentication refused"),
            Self::Permanent(Permanent::Rejected(_)) => f.write_str("request rejected"),
            Self::Permanent(Permanent::Tls) => f.write_str("TLS failure"),
            Self::Permanent(Permanent::TooLarge) => f.write_str("reply too large"),
            Self::Permanent(Permanent::Truncated) => f.write_str("reply truncated"),
            Self::Permanent(Permanent::Redirect) => f.write_str("endpoint redirected"),
            Self::Permanent(Permanent::Unavailable(_)) => f.write_str("adapter unavailable"),
        }
    }
}

impl std::error::Error for AttemptError {}

/// Immutable fact set built from the fresh policy snapshot used for final authorization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparationContext {
    /// Policy digest in force for this attempt.
    pub policy_hash: Sha256,
    /// Banner text of every defined label, for a request that must show one.
    pub banners: BTreeMap<LabelId, String>,
}

/// The exact outbound body an adapter prepared for its attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedAttempt {
    /// Canonical target of the prepared bytes.
    pub target: Target,
    /// Exact serialized body bytes (or Handoff text).
    pub bytes: Vec<u8>,
    /// Content type of the body.
    pub content_type: String,
}

/// A model adapter: one implementation of the Model port.
pub trait ModelAdapter: Send + Sync {
    /// Adapter identifier (`<name>@<n>`).
    fn id(&self) -> &'static str;

    /// Declared runtime capabilities.
    fn caps(&self) -> Caps;

    /// Serializes the exact outbound body for this attempt without reading the policy or clock.
    ///
    /// # Errors
    ///
    /// Returns [`AttemptError`] when the request cannot be serialized.
    fn prepare(
        &self,
        req: &ModelRequest,
        route: &Route,
        context: &PreparationContext,
    ) -> Result<PreparedAttempt, AttemptError>;

    /// Sends the prepared body, verifying that `allowed` covers its endpoint and bytes.
    ///
    /// # Errors
    ///
    /// Returns [`AttemptError`] when the send fails.
    fn attempt(
        &self,
        allowed: &Allowed,
        prepared: &PreparedAttempt,
        route: &Route,
    ) -> Result<Attempt, AttemptError>;
}

impl<T: ModelAdapter + ?Sized> ModelAdapter for Arc<T> {
    fn id(&self) -> &'static str {
        (**self).id()
    }

    fn caps(&self) -> Caps {
        (**self).caps()
    }

    fn prepare(
        &self,
        req: &ModelRequest,
        route: &Route,
        context: &PreparationContext,
    ) -> Result<PreparedAttempt, AttemptError> {
        (**self).prepare(req, route, context)
    }

    fn attempt(
        &self,
        allowed: &Allowed,
        prepared: &PreparedAttempt,
        route: &Route,
    ) -> Result<Attempt, AttemptError> {
        (**self).attempt(allowed, prepared, route)
    }
}

/// The Model port as a caller sees it (`plan`, `run`, `resume`, `describe`, `preview`).
pub trait ModelPort {
    /// Chooses the first candidate route whose effective allow set covers `base`.
    ///
    /// # Errors
    ///
    /// Returns a [`ModelError`] when no candidate covers the labels or a terminal denial wins.
    fn plan(&self, via: Option<&str>, base: &LabelSet) -> Result<Plan, ModelError>;

    /// Runs `req` through the gate.
    ///
    /// # Errors
    ///
    /// Returns a [`ModelError`] when the call is refused or fails.
    fn run(
        &self,
        req: &ModelRequest,
        via: Option<&str>,
        stop: &dyn Fn() -> bool,
    ) -> Result<ModelOutcome, ModelError>;

    /// Validates a pasted reply against its stored ticket.
    ///
    /// # Errors
    ///
    /// Returns a [`ModelError`] when the ticket is unknown, used or the reply is invalid.
    fn resume(
        &self,
        ticket: &str,
        pasted: &str,
        label: Option<&str>,
    ) -> Result<ModelAnswer, ModelError>;

    /// Returns one description per route.
    fn describe(&self) -> Vec<RouteInfo>;

    /// Returns what a send would do, without a network call or a journal event.
    fn preview(&self, req: &ModelRequest, via: Option<&str>) -> SendPreview;
}

/// The gate's public decision: an answer, or a Handoff pack waiting for a paste.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelOutcome {
    /// A validated reply.
    Answer(ModelAnswer),
    /// A Handoff pack was written; the request waits for `resume`.
    Deferred(HandoffTicket),
}

/// Where an answer came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    pub(crate) adapter: String,
    pub(crate) endpoint: String,
    pub(crate) model: String,
}

impl Provenance {
    /// Returns the adapter identifier.
    #[must_use]
    pub fn adapter(&self) -> &str {
        &self.adapter
    }

    /// Returns the endpoint name.
    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Returns the model identifier.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }
}

/// A validated model answer with its provenance and request facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelAnswer {
    pub(crate) text: String,
    pub(crate) request_labels: LabelSet,
    pub(crate) inputs: Vec<ItemRef>,
    pub(crate) provenance: Provenance,
    pub(crate) ticket: Option<HandoffTicket>,
    pub(crate) warnings: Vec<String>,
}

impl ModelAnswer {
    /// Returns the answer text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the request labels the gate saved with this answer.
    #[must_use]
    pub fn request_labels(&self) -> &LabelSet {
        &self.request_labels
    }

    /// Returns the store-qualified input references of the request.
    #[must_use]
    pub fn input_refs(&self) -> &[ItemRef] {
        &self.inputs
    }

    /// Returns where the answer came from.
    #[must_use]
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    /// Returns the Handoff ticket of a resumed answer, when there is one.
    #[must_use]
    pub fn ticket(&self) -> Option<&HandoffTicket> {
        self.ticket.as_ref()
    }

    /// Returns the non-fatal warnings of the call (a skipped call record, for instance).
    #[must_use]
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }
}

/// The route `plan` selected: its endpoint name and destination value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// Selected endpoint name.
    pub name: EndpointName,
    /// Destination value for `egress::check`.
    pub dest: Dest,
}

/// State of the policy row that matches a route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowState {
    /// A live row covers this route.
    Found,
    /// A row matches but has expired.
    Expired,
    /// No row matches.
    Missing,
}

/// Read-only description of one route for `ai status` and the chat header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteInfo {
    /// Endpoint name (`handoff` for the copy-paste route).
    pub name: EndpointName,
    /// Adapter identifier.
    pub adapter: String,
    /// Presence state of the endpoint's key variable.
    pub key: KeyState,
    /// State of the policy row that matches the route.
    pub policy_row: RowState,
    /// Effective allowed label set of the route.
    pub allow: Vec<LabelId>,
    /// Labels cut from the row, and the side that cut each.
    pub cuts: Vec<Cut>,
}

/// Classification of a [`ModelError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// The policy refused the call (route, label, marking, budget).
    Policy,
    /// Availability failed (no `net` build, the CPU, an unreachable endpoint).
    Availability,
    /// The adapter or the reply was invalid.
    Invalid,
}

/// Failure of a gated model call.
///
/// Its text names the code, the closed kind and the catalogue's help line; it never includes a
/// byte of an endpoint's reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelError {
    /// Diagnostic code.
    pub code: Code,
    /// Closed error classification.
    pub kind: ErrorKind,
    /// Adapter that raised the error, when the gate knows it.
    pub adapter: Option<String>,
    /// A fallback Handoff ticket, present only after an availability failure.
    pub ticket: Option<HandoffTicket>,
}

impl ModelError {
    /// Constructs an error from a code and classification.
    #[must_use]
    pub const fn new(code: Code, kind: ErrorKind) -> Self {
        Self {
            code,
            kind,
            adapter: None,
            ticket: None,
        }
    }

    /// Attaches the adapter identifier that raised this error.
    #[must_use]
    pub fn with_adapter(mut self, adapter: &str) -> Self {
        self.adapter = Some(adapter.to_owned());
        self
    }

    /// Attaches a fallback ticket.
    #[must_use]
    pub fn with_ticket(mut self, ticket: HandoffTicket) -> Self {
        self.ticket = Some(ticket);
        self
    }

    /// Returns the exit code of a program that ends on this error.
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        match self.kind {
            ErrorKind::Policy | ErrorKind::Invalid => 2,
            ErrorKind::Availability => 3,
        }
    }

    const fn kind_text(&self) -> &'static str {
        match self.kind {
            ErrorKind::Policy => "the policy refused the model call",
            ErrorKind::Availability => "the model endpoint is unavailable",
            ErrorKind::Invalid => "the model reply is invalid",
        }
    }
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.adapter {
            Some(adapter) => write!(
                f,
                "error[{}]: {} ({adapter})\n  help: {}",
                self.code.id(),
                self.kind_text(),
                self.code.help()
            ),
            None => write!(
                f,
                "error[{}]: {}\n  help: {}",
                self.code.id(),
                self.kind_text(),
                self.code.help()
            ),
        }
    }
}

impl std::error::Error for ModelError {}

/// The transaction identifier of an `ai.begin` (its journal sequence number).
pub type Tx = u64;

/// An `ai.begin` payload, without the transaction number the writer assigns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeginEvent {
    /// Request purpose.
    pub purpose: Purpose,
    /// Destination kind (`model`).
    pub dest: DestKind,
    /// Request labels.
    pub labels: LabelSet,
    /// Policy digest in force.
    pub policy: Sha256,
    /// Adapter identifier.
    pub adapter: String,
    /// Endpoint name.
    pub endpoint: EndpointName,
    /// Model identifier.
    pub model: ModelId,
    /// Template identifier.
    pub template: String,
    /// SHA-256 of the exact prepared body.
    pub prompt_sha256: Sha256,
    /// Store-qualified context references.
    pub context_ids: Vec<RecordRef>,
}

/// An `ai.pack` payload, without the sequence number the writer assigns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackEvent {
    /// SHA-256 of the exact prepared pack text.
    pub prompt_sha256: Sha256,
    /// Store-qualified context references.
    pub context_ids: Vec<RecordRef>,
    /// Destination kind (`handoff`).
    pub dest: DestKind,
    /// Pack labels.
    pub labels: LabelSet,
    /// Policy digest in force.
    pub policy: Sha256,
}

/// The first event of a transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BeginRecord {
    /// A network call's `ai.begin`.
    Model(BeginEvent),
    /// The copy-paste route's `ai.pack`.
    Handoff(PackEvent),
}

/// An `ai.call` payload, without the transaction number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallEvent {
    /// Closed outcome word (`ok`, `failed`, `invalid`, `truncated` or `cancelled`).
    pub outcome: &'static str,
    /// Diagnostic code when the call did not succeed.
    pub code: Option<Code>,
    /// Number of transport attempts made.
    pub attempts: u64,
    /// SHA-256 of the reply when one was received.
    pub output_sha256: Option<Sha256>,
    /// Input token count.
    pub tokens_in: u64,
    /// Output token count.
    pub tokens_out: u64,
}

/// The journal operations the gate needs.
pub trait AiJournal: Send + Sync {
    /// Appends the first event of a transaction and returns its sequence number.
    ///
    /// # Errors
    ///
    /// Returns `FB0903` when the append fails.
    fn begin(&self, record: BeginRecord) -> Result<Tx, ModelError>;

    /// Appends the `ai.call` of `tx`.
    ///
    /// # Errors
    ///
    /// Returns `FB0904` when the append fails.
    fn finish(&self, tx: Tx, event: CallEvent) -> Result<(), ModelError>;

    /// Appends a `refuse` event for a policy denial.
    ///
    /// # Errors
    ///
    /// Returns a [`ModelError`] when the append fails.
    fn refuse(
        &self,
        dest: Option<DestKind>,
        labels: &LabelSet,
        code: Code,
        reason: Word,
    ) -> Result<(), ModelError>;

    /// Appends an `ai.paste` event for a resumed Handoff round trip.
    ///
    /// # Errors
    ///
    /// Returns a [`ModelError`] when the append fails.
    fn paste(
        &self,
        pack_seq: Option<u64>,
        output_sha256: Sha256,
        labels: &LabelSet,
    ) -> Result<(), ModelError>;

    /// Returns `true` when the journal can take the record a route writes.
    fn audited(&self) -> bool;
}

/// A pacer for retry delays.
pub trait Pace: Send + Sync {
    /// Sleeps for `ms` milliseconds.
    fn sleep_ms(&self, ms: u64);
}

/// Metadata stored beside a Handoff pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketMeta {
    /// The response schema retained at pack time (`None` only for an unconstrained request).
    pub response_schema: Option<Schema>,
    /// Sequence number of the successful `ai.pack` append.
    pub pack_seq: u64,
    /// Template identifier.
    pub template: String,
    /// Complete request labels.
    pub labels: LabelSet,
    /// Sorted store-qualified context identifiers.
    pub context_ids: Vec<String>,
    /// Request input references.
    pub inputs: Vec<ItemRef>,
}

/// A stored Handoff ticket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredTicket {
    /// Ticket identifier.
    pub id: HandoffId,
    /// Ticket metadata.
    pub meta: TicketMeta,
    /// Whether the ticket has been resumed.
    pub used: bool,
}

/// The call and pack storage the gate needs.
pub trait CallStore: Send + Sync {
    /// Writes one call record. A failure is a warning, never a failure of the call.
    ///
    /// # Errors
    ///
    /// Returns a warning line when the record cannot be written.
    fn put_call(&self, record: &CallRecord) -> Result<(), String>;

    /// Writes a pack and its ticket, allocating the next ticket number.
    ///
    /// # Errors
    ///
    /// Returns a warning line when the files cannot be written.
    fn put_pack(&self, pack: &[u8], meta: &TicketMeta) -> Result<HandoffTicket, String>;

    /// Loads a ticket by identifier.
    ///
    /// # Errors
    ///
    /// Returns a warning line when the stored ticket cannot be read.
    fn get_ticket(&self, id: HandoffId) -> Result<Option<StoredTicket>, String>;

    /// Marks a ticket used.
    ///
    /// # Errors
    ///
    /// Returns a warning line when the ticket cannot be updated.
    fn mark_used(&self, id: HandoffId) -> Result<(), String>;
}

/// One recorded model call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallRecord {
    /// Adapter identifier.
    pub adapter: String,
    /// Endpoint name.
    pub endpoint: String,
    /// Model identifier.
    pub model: String,
    /// Template identifier.
    pub template: String,
    /// Request purpose.
    pub purpose: String,
    /// Request labels.
    pub labels: LabelSet,
    /// Request input references.
    pub inputs: Vec<ItemRef>,
    /// Policy digest in force.
    pub policy: Sha256,
    /// SHA-256 of the exact prepared body.
    pub prompt_sha256: Sha256,
    /// SHA-256 of the reply, when one was received.
    pub output_sha256: Option<Sha256>,
    /// Rendered system text.
    pub system: String,
    /// Rendered prompt text.
    pub prompt: String,
    /// Reply text.
    pub reply: String,
    /// Input token count.
    pub tokens_in: u64,
    /// Output token count.
    pub tokens_out: u64,
    /// Number of transport attempts.
    pub attempts: u64,
    /// Closed outcome word.
    pub outcome: String,
    /// Effective retention in days.
    pub keep_days: u32,
    /// Event timestamp of the call.
    pub now: Now,
}

/// The owner's call-record retention settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeepSettings {
    /// Whether the owner keeps call records at all.
    pub calls: bool,
    /// The owner's retention limit in days.
    pub days: u32,
}

/// A shared per-run call budget, counted across every endpoint.
#[derive(Debug)]
pub struct RunBudget {
    used: AtomicU32,
    max: u32,
}

impl RunBudget {
    /// Constructs a budget of `max` calls.
    #[must_use]
    pub const fn new(max: u32) -> Self {
        Self {
            used: AtomicU32::new(0),
            max,
        }
    }

    /// Draws one call from the budget, returning `false` when none is left.
    #[must_use]
    pub fn try_take(&self) -> bool {
        let mut current = self.used.load(Ordering::SeqCst);
        loop {
            if current >= self.max {
                return false;
            }
            match self.used.compare_exchange(
                current,
                current + 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return true,
                Err(seen) => current = seen,
            }
        }
    }

    /// Returns the number of calls left.
    #[must_use]
    pub fn left(&self) -> u32 {
        self.max.saturating_sub(self.used.load(Ordering::SeqCst))
    }
}

/// Everything the gate needs from the run.
#[derive(Clone)]
pub struct Deps {
    /// The journal writer.
    pub journal: Arc<dyn AiJournal>,
    /// The endpoint key source (never a key value in a log).
    pub keys: Arc<dyn KeySource + Send + Sync>,
    /// The retry pacer.
    pub pace: Arc<dyn Pace>,
    /// Supplies a fresh policy snapshot before every attempt.
    pub policy_provider: Arc<dyn Fn() -> PolicySnapshot + Send + Sync>,
    /// Call and pack storage.
    pub store: Arc<dyn CallStore>,
    /// Owner retention settings.
    pub keep: KeepSettings,
    /// The shared run budget.
    pub budget: Arc<RunBudget>,
    /// What this build can do.
    pub avail: Avail,
    /// Retries after a transient failure.
    pub retries: u32,
    /// Optional transport, for an adapter that sends (tests and later cards).
    pub http: Option<Arc<dyn HttpPost + Send + Sync>>,
}

/// A coherent policy read used for one attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicySnapshot {
    /// The merged effective policy.
    pub policy: EffectivePolicy,
    /// The event timestamp of this read.
    pub now: Now,
    /// Age of the newest `audit.export`, from the same read.
    pub shipped: Shipped,
}

impl PolicySnapshot {
    /// Builds the immutable preparation context of this snapshot.
    #[must_use]
    pub fn preparation_context(&self) -> PreparationContext {
        let table = self.policy.label_table();
        let mut banners = BTreeMap::new();
        for label in table.known_ids() {
            if let Some(banner) = self.policy.banner(label) {
                banners.insert(label.clone(), banner.to_owned());
            }
        }
        PreparationContext {
            policy_hash: Sha256::parse(self.policy.hash()).unwrap_or_else(|_| Sha256::digest(b"")),
            banners,
        }
    }
}
