//! `Guarded`: the one way a model is called (`model-gate` step 4, `ADR 0001 AD-01`).
//!
//! The eleven stages run in order and in no other order. Stages G3 to G6 run before any adapter
//! code, so a bug in an adapter cannot send what the gate would have refused.

use fieldbook_core::audit::{DestKind, RecordRef, Sha256, Word};
use fieldbook_core::check::Code;
use fieldbook_core::egress::{self, Decision, Delivery, Dest, Shipped, Transfer};
use fieldbook_core::labels::{self, LabelSet, StoreId};
use fieldbook_core::policy::{CutBy, EffectivePolicy};
use fieldbook_core::ports::chat::{PreviewCut, Scan, SendPreview};
use fieldbook_core::ports::{HandoffId, KeyState, ModelRequest, Part};

use crate::adapters::model::{
    Attempt, AttemptError, BeginEvent, BeginRecord, CallEvent, CallRecord, Deps, ErrorKind,
    ModelAdapter, ModelAnswer, ModelError, ModelOutcome, ModelPort, PackEvent, Permanent, Plan,
    PolicySnapshot, PreparedAttempt, Provenance, Reason, Route, RouteInfo, RowState, TicketMeta,
    Transient,
};
use crate::adapters::schema_json::{extract_json, parse_json_value, validate};

/// The maximum number of schema diagnostics a repair may name.
const MAX_REPAIR_DIAGNOSTICS: usize = 8;

/// One gated route: the adapter, its route and the run dependencies.
pub struct Guarded {
    adapter: Box<dyn ModelAdapter + Send + Sync>,
    route: Route,
    deps: Deps,
}

/// The result of one pass through stages G2 to G9.
enum Step {
    /// The pass completed.
    Done(ModelOutcome),
    /// The reply was invalid and a repair may follow.
    Invalid {
        /// The error to return when no repair is left.
        error: ModelError,
        /// Fixed diagnostics for the repair prompt.
        diagnostics: Vec<String>,
    },
    /// A transient failure that another attempt may clear.
    Transient {
        /// The error returned when retries are exhausted.
        error: ModelError,
        /// The endpoint's `Retry-After` value, when it sent one.
        retry_after: Option<u32>,
    },
    /// A permanent failure.
    Failed(ModelError),
}

#[derive(Default)]
struct RunState {
    attempts: u64,
    retry: u32,
    repair_used: bool,
    warnings: Vec<String>,
}

impl Guarded {
    /// Constructs a gate around one adapter and route.
    #[must_use]
    pub fn new(adapter: Box<dyn ModelAdapter + Send + Sync>, route: Route, deps: Deps) -> Self {
        Self {
            adapter,
            route,
            deps,
        }
    }

    /// Returns the adapter identifier.
    #[must_use]
    pub fn adapter_id(&self) -> &'static str {
        self.adapter.id()
    }

    /// Returns the route.
    #[must_use]
    pub const fn route(&self) -> &Route {
        &self.route
    }

    /// Returns the dependencies.
    #[must_use]
    pub const fn deps(&self) -> &Deps {
        &self.deps
    }

    /// Returns the destination this route sends to.
    #[must_use]
    pub fn destination(&self) -> Dest {
        if self.adapter.caps().network {
            Dest::Model {
                name: self.route.name.as_str().to_owned(),
                base_url: self.route.base_url.clone(),
            }
        } else {
            Dest::Handoff
        }
    }

    fn dest(&self) -> Dest {
        self.destination()
    }

    /// Returns `true` when this route sends over the network.
    #[must_use]
    pub fn is_network(&self) -> bool {
        self.adapter.caps().network
    }

    fn context_refs(req: &ModelRequest) -> Vec<RecordRef> {
        req.context_refs()
            .into_iter()
            .filter_map(|item| RecordRef::parse(&format!("{}/{}", item.store, item.id)).ok())
            .collect()
    }

    fn policy_sha(snapshot: &PolicySnapshot) -> Sha256 {
        Sha256::parse(snapshot.policy.hash()).unwrap_or_else(|_| Sha256::digest(b""))
    }

    fn refuse(&self, dest: DestKind, labels: &LabelSet, code: Code, reason: &'static str) {
        let _ = self
            .deps
            .journal
            .refuse(Some(dest), labels, code, Word::new_static(reason));
    }

    fn unavailable(&self, code: Code) -> ModelError {
        ModelError::new(code, ErrorKind::Availability)
    }

    fn policy_error(&self, code: Code) -> ModelError {
        ModelError::new(code, ErrorKind::Policy)
    }

    fn invalid_error(&self, code: Code) -> ModelError {
        ModelError::new(code, ErrorKind::Invalid)
    }

    fn denial_error(denial: &egress::Denial) -> ModelError {
        ModelError::new(denial.code, ErrorKind::Policy)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the ai.call record has six typed fields and a transaction"
    )]
    fn finish_call(
        &self,
        tx: u64,
        outcome: &'static str,
        code: Option<Code>,
        attempts: u64,
        output_sha256: Option<Sha256>,
        tokens_in: u64,
        tokens_out: u64,
    ) -> Result<(), ModelError> {
        self.deps.journal.finish(
            tx,
            CallEvent {
                outcome,
                code,
                attempts,
                output_sha256,
                tokens_in,
                tokens_out,
            },
        )
    }

    /// Runs `req` through the gate, retrying only what can be retried.
    ///
    /// # Errors
    ///
    /// Returns a [`ModelError`] when the call is refused or fails.
    pub fn run_inner(
        &self,
        req: &ModelRequest,
        stop: &dyn Fn() -> bool,
    ) -> Result<ModelOutcome, ModelError> {
        self.run_inner_impl(req, stop)
            .map_err(|error| error.with_adapter(self.adapter.id()))
    }

    fn run_inner_impl(
        &self,
        req: &ModelRequest,
        stop: &dyn Fn() -> bool,
    ) -> Result<ModelOutcome, ModelError> {
        let mut state = RunState::default();
        let mut current = req.clone();
        loop {
            match self.one_attempt(&current, stop, &mut state) {
                Step::Done(outcome) => return Ok(outcome),
                Step::Invalid { error, diagnostics } => {
                    if state.repair_used {
                        return Err(error);
                    }
                    state.repair_used = true;
                    state.retry = 0;
                    current = repair_request(&current, &diagnostics);
                }
                Step::Transient { error, retry_after } => {
                    if state.retry < self.deps.retries && !stop() {
                        let delay = retry_after.map_or_else(
                            || if state.retry == 0 { 1000 } else { 4000 },
                            |seconds| u64::from(seconds.min(30)) * 1000,
                        );
                        self.deps.pace.sleep_ms(delay);
                        state.retry += 1;
                        continue;
                    }
                    return Err(if stop() {
                        error
                    } else {
                        ModelError::new(Code::Fb0908, ErrorKind::Availability)
                    });
                }
                Step::Failed(error) => return Err(error),
            }
        }
    }

    fn one_attempt(
        &self,
        req: &ModelRequest,
        stop: &dyn Fn() -> bool,
        state: &mut RunState,
    ) -> Step {
        // G1 happens in the Router; every attempt starts again at G2 with a fresh snapshot.
        let snapshot = (self.deps.policy_provider)();
        let dest = self.dest();

        // G2 Availability.
        if self.adapter.caps().network {
            if !self.deps.avail.net_built {
                return Step::Failed(self.unavailable(Code::Fb0912));
            }
            if !self.deps.avail.cpu_ok {
                return Step::Failed(self.unavailable(Code::Fb0911));
            }
            if let Some(name) = &self.route.key_env
                && self.deps.keys.key(name).is_err()
            {
                return Step::Failed(self.unavailable(Code::Fb0914));
            }
        }

        // G3 Egress, before any adapter code and without the text.
        match egress::check(
            &snapshot.policy,
            &dest,
            Some(&req.labels()),
            None,
            snapshot.now,
            snapshot.shipped,
        ) {
            Decision::Allowed(_) => {}
            Decision::Denied(denial) => {
                self.refuse(
                    dest.kind(),
                    &req.labels(),
                    denial.code,
                    denial.reason.as_str(),
                );
                return Step::Failed(Self::denial_error(&denial));
            }
        }

        // G4 Mismatch: scan the final text, prepare the exact body, and bind the delivery.
        let rules = snapshot.policy.marking_rules();
        let text = format!("{}\n{}", req.system(), req.prompt());
        let markings = labels::markings_in(&text, &rules);

        let context = snapshot.preparation_context();
        let prepared = match self.adapter.prepare(req, &self.route, &context) {
            Ok(prepared) => prepared,
            Err(error) => {
                return Step::Failed(self.attempt_error(error));
            }
        };
        let delivery = Delivery {
            entries: vec![transfer_for(&self.route, &prepared)],
            markings,
        };
        let allowed = match egress::check(
            &snapshot.policy,
            &dest,
            Some(&req.labels()),
            Some(&delivery),
            snapshot.now,
            snapshot.shipped,
        ) {
            Decision::Allowed(allowed) => allowed,
            Decision::Denied(denial) => {
                self.refuse(
                    dest.kind(),
                    &req.labels(),
                    denial.code,
                    denial.reason.as_str(),
                );
                return Step::Failed(Self::denial_error(&denial));
            }
        };

        // G5 Budget and prompt length.
        let prompt_chars = req.system().chars().count() + req.prompt().chars().count();
        if prompt_chars > self.adapter.caps().max_prompt_chars as usize {
            return Step::Failed(self.policy_error(Code::Fb0907));
        }
        if !self.deps.budget.try_take() {
            return Step::Failed(self.policy_error(Code::Fb0915));
        }

        // G6 Audit, before anything is sent or written.
        let prompt_sha256 = Sha256::digest(&prepared.bytes);
        let context_ids = Self::context_refs(req);
        let policy_sha = Self::policy_sha(&snapshot);
        let tx = if self.adapter.caps().network {
            self.deps.journal.begin(BeginRecord::Model(BeginEvent {
                purpose: req.purpose(),
                dest: DestKind::Model,
                labels: allowed.labels().cloned().unwrap_or_else(|| req.labels()),
                policy: policy_sha,
                adapter: self.adapter.id().to_owned(),
                endpoint: self.route.name.clone(),
                model: self.route.model.clone(),
                template: req.template().id().to_owned(),
                prompt_sha256: prompt_sha256.clone(),
                context_ids,
            }))
        } else {
            self.deps.journal.begin(BeginRecord::Handoff(PackEvent {
                prompt_sha256: prompt_sha256.clone(),
                context_ids,
                dest: DestKind::Handoff,
                labels: allowed.labels().cloned().unwrap_or_else(|| req.labels()),
                policy: policy_sha,
            }))
        };
        let Ok(tx) = tx else {
            return Step::Failed(self.unavailable(Code::Fb0903));
        };

        // G7 Attempt.
        state.attempts += 1;
        let attempt = self.adapter.attempt(&allowed, &prepared, &self.route);
        match attempt {
            Ok(Attempt::Pack(pack)) => self.finish_pack(req, &pack, tx, state),
            Ok(Attempt::Reply(raw)) => {
                if raw.body.len() as u64 > self.route.max_reply_bytes {
                    if self
                        .finish_call(
                            tx,
                            "failed",
                            Some(Code::Fb0919),
                            state.attempts,
                            Some(Sha256::digest(raw.body.as_bytes())),
                            raw.tokens_in,
                            raw.tokens_out,
                        )
                        .is_err()
                    {
                        return Step::Failed(self.invalid_error(Code::Fb0904));
                    }
                    return Step::Failed(self.invalid_error(Code::Fb0919));
                }

                // G8 Validate.
                if let Some(schema) = req.schema()
                    && let Err(diagnostics) = validate_reply(&raw.body, schema)
                {
                    let output = Sha256::digest(raw.body.as_bytes());
                    if self
                        .finish_call(
                            tx,
                            "invalid",
                            Some(Code::Fb0909),
                            state.attempts,
                            Some(output),
                            raw.tokens_in,
                            raw.tokens_out,
                        )
                        .is_err()
                    {
                        return Step::Failed(self.invalid_error(Code::Fb0904));
                    }
                    return Step::Invalid {
                        error: self.invalid_error(Code::Fb0909),
                        diagnostics,
                    };
                }

                // G9 Audit.
                let output_sha = Sha256::digest(raw.body.as_bytes());
                if self
                    .finish_call(
                        tx,
                        "ok",
                        None,
                        state.attempts,
                        Some(output_sha),
                        raw.tokens_in,
                        raw.tokens_out,
                    )
                    .is_err()
                {
                    return Step::Failed(self.invalid_error(Code::Fb0904));
                }

                // G10 Keep.
                self.keep_call(req, &snapshot, &raw, prompt_sha256, state);

                // G11 Answer.
                Step::Done(ModelOutcome::Answer(ModelAnswer {
                    text: raw.body,
                    request_labels: req.labels(),
                    inputs: req.input_refs(),
                    provenance: Provenance {
                        adapter: self.adapter.id().to_owned(),
                        endpoint: self.route.name.as_str().to_owned(),
                        model: self.route.model.as_str().to_owned(),
                    },
                    ticket: None,
                    warnings: std::mem::take(&mut state.warnings),
                }))
            }
            Err(error) => {
                let code = attempt_code(error);
                let cancelled = stop();
                let outcome = if matches!(error, AttemptError::Permanent(Permanent::Truncated)) {
                    "truncated"
                } else if cancelled {
                    "cancelled"
                } else {
                    "failed"
                };
                if self
                    .finish_call(tx, outcome, Some(code), state.attempts, None, 0, 0)
                    .is_err()
                {
                    return Step::Failed(self.invalid_error(Code::Fb0904));
                }
                if cancelled || matches!(error, AttemptError::Permanent(Permanent::Truncated)) {
                    return Step::Failed(ModelError::new(code, ErrorKind::Availability));
                }
                match error {
                    AttemptError::Transient(transient) => Step::Transient {
                        error: ModelError::new(code, ErrorKind::Availability),
                        retry_after: match transient {
                            Transient::RateLimited { retry_after_s } => retry_after_s,
                            _ => None,
                        },
                    },
                    AttemptError::Permanent(permanent) => Step::Failed(ModelError::new(
                        attempt_code(AttemptError::Permanent(permanent)),
                        ErrorKind::Availability,
                    )),
                }
            }
        }
    }

    fn finish_pack(
        &self,
        req: &ModelRequest,
        pack: &crate::adapters::model::PackText,
        tx: u64,
        state: &mut RunState,
    ) -> Step {
        let meta = TicketMeta {
            response_schema: req.schema().cloned(),
            pack_seq: tx,
            template: req.template().id().to_owned(),
            labels: req.labels(),
            context_ids: req.context_ids(),
            inputs: req.input_refs(),
        };
        match self.deps.store.put_pack(pack.bytes(), &meta) {
            Ok(ticket) => Step::Done(ModelOutcome::Deferred(ticket)),
            Err(warning) => {
                // No ticket is invented: an id that was not stored could name another pack.
                state.warnings.push(warning);
                Step::Failed(self.invalid_error(Code::Fb0916))
            }
        }
    }

    fn keep_call(
        &self,
        req: &ModelRequest,
        snapshot: &PolicySnapshot,
        raw: &crate::adapters::model::RawReply,
        prompt_sha256: Sha256,
        state: &mut RunState,
    ) {
        let name = self.route.name.as_str();
        let keep = snapshot.policy.effective_keep_calls(name);
        let days = snapshot.policy.effective_keep_days(name);
        let rule = snapshot.policy.store_rule(&StoreId::State);
        if !keep || days == 0 {
            state
                .warnings
                .push("warning: the call record was not kept (retention is off)".to_owned());
            return;
        }
        if labels::admit(&req.labels(), &rule).is_err() {
            state.warnings.push(
                "warning: the call record was not kept (the state store cannot hold the request's labels)"
                    .to_owned(),
            );
            return;
        }
        let record = CallRecord {
            adapter: self.adapter.id().to_owned(),
            endpoint: self.route.name.as_str().to_owned(),
            model: self.route.model.as_str().to_owned(),
            template: req.template().id().to_owned(),
            purpose: req.purpose().as_str().to_owned(),
            labels: req.labels(),
            inputs: req.input_refs(),
            policy: Self::policy_sha(snapshot),
            prompt_sha256,
            output_sha256: Some(Sha256::digest(raw.body.as_bytes())),
            system: req.system(),
            prompt: req.prompt(),
            reply: raw.body.clone(),
            tokens_in: raw.tokens_in,
            tokens_out: raw.tokens_out,
            attempts: state.attempts,
            outcome: "ok".to_owned(),
            keep_days: days,
            now: snapshot.now,
        };
        if let Err(warning) = self.deps.store.put_call(&record) {
            state.warnings.push(warning);
        }
    }

    fn attempt_error(&self, error: AttemptError) -> ModelError {
        ModelError::new(attempt_code(error), ErrorKind::Availability)
    }

    /// Validates a pasted reply against its stored ticket and marks the ticket used.
    ///
    /// # Errors
    ///
    /// Returns `FB0916` for an unknown, used or empty ticket and `FB0919` for an oversized reply;
    /// an invalid reply is `FB0909`.
    pub fn resume_inner(
        &self,
        ticket: &str,
        pasted: &str,
        label: Option<&str>,
    ) -> Result<ModelAnswer, ModelError> {
        let id = HandoffId::parse(ticket).map_err(|_| self.invalid_error(Code::Fb0916))?;
        let stored = self
            .deps
            .store
            .get_ticket(id)
            .map_err(|_| self.invalid_error(Code::Fb0916))?
            .ok_or_else(|| self.invalid_error(Code::Fb0916))?;
        if stored.used || pasted.trim().is_empty() {
            return Err(self.invalid_error(Code::Fb0916));
        }
        if pasted.len() as u64 > self.route.max_reply_bytes {
            return Err(self.invalid_error(Code::Fb0919));
        }
        if let Some(schema) = &stored.meta.response_schema
            && validate_reply(pasted, schema).is_err()
        {
            return Err(self.invalid_error(Code::Fb0909));
        }
        let output = Sha256::digest(pasted.as_bytes());
        self.deps
            .journal
            .paste(Some(stored.meta.pack_seq), output, &stored.meta.labels)
            .map_err(|_| self.invalid_error(Code::Fb0904))?;
        self.deps
            .store
            .mark_used(id)
            .map_err(|_warning| ModelError::new(Code::Fb0916, ErrorKind::Invalid))?;
        Ok(ModelAnswer {
            text: pasted.to_owned(),
            request_labels: stored.meta.labels,
            inputs: stored.meta.inputs,
            provenance: Provenance {
                adapter: self.adapter.id().to_owned(),
                endpoint: self.route.name.as_str().to_owned(),
                model: label.unwrap_or("unrecorded").to_owned(),
            },
            ticket: None,
            warnings: Vec::new(),
        })
    }

    /// Builds a send preview without a network call or a journal event.
    #[must_use]
    pub fn preview_inner(&self, req: &ModelRequest) -> SendPreview {
        let snapshot = (self.deps.policy_provider)();
        let dest = self.dest();
        let (verdict, reason) = match egress::check(
            &snapshot.policy,
            &dest,
            Some(&req.labels()),
            None,
            snapshot.now,
            snapshot.shipped,
        ) {
            Decision::Allowed(_) => (None, None),
            Decision::Denied(denial) => (
                Some(denial.code.id().to_owned()),
                Some(denial.reason.as_str().to_owned()),
            ),
        };
        let cuts = snapshot
            .policy
            .explain(&dest)
            .into_iter()
            .map(|cut| PreviewCut {
                label: cut.label,
                by_owner: matches!(cut.by, CutBy::Owner),
            })
            .collect();
        let rules = snapshot.policy.marking_rules();
        let text = format!("{}\n{}", req.system(), req.prompt());
        let scan = Scan::from_markings(labels::markings_in(&text, &rules));
        let key = match &self.route.key_env {
            Some(name) if self.deps.keys.key(name).is_ok() => KeyState::Set,
            Some(_) => KeyState::Missing,
            None => KeyState::NotNeeded,
        };
        let generation = format!(
            "{}|{}|{}|{}",
            snapshot.policy.hash(),
            req.labels().to_cell(),
            self.route.name.as_str(),
            req.prompt_sha256()
        );
        SendPreview {
            route: Some(self.route.name.as_str().to_owned()),
            adapter: Some(self.adapter.id().to_owned()),
            labels: req.labels().ids().to_vec(),
            verdict,
            reason,
            cuts,
            characters: req.system().chars().count() + req.prompt().chars().count(),
            items: req.input_refs().len(),
            scan,
            key,
            budget_left: self.deps.budget.left(),
            audited: self.deps.journal.audited(),
            generation,
        }
    }

    /// Builds one route description.
    #[must_use]
    pub fn describe_route(&self) -> RouteInfo {
        let snapshot = (self.deps.policy_provider)();
        let dest = self.dest();
        let key = match &self.route.key_env {
            Some(name) if self.deps.keys.key(name).is_ok() => KeyState::Set,
            Some(_) => KeyState::Missing,
            None => KeyState::NotNeeded,
        };
        RouteInfo {
            name: self.route.name.clone(),
            adapter: self.adapter.id().to_owned(),
            key,
            policy_row: row_state(&snapshot.policy, &self.route),
            allow: snapshot.policy.allow_set(&dest).ids().to_vec(),
            cuts: snapshot.policy.explain(&dest),
        }
    }
}

impl ModelPort for Guarded {
    fn plan(&self, _via: Option<&str>, base: &LabelSet) -> Result<Plan, ModelError> {
        let snapshot = (self.deps.policy_provider)();
        match egress::check(
            &snapshot.policy,
            &self.dest(),
            Some(base),
            None,
            snapshot.now,
            snapshot.shipped,
        ) {
            Decision::Allowed(_) => Ok(Plan {
                name: self.route.name.clone(),
                dest: self.dest(),
            }),
            Decision::Denied(denial) => Err(Self::denial_error(&denial)),
        }
    }

    fn run(
        &self,
        req: &ModelRequest,
        _via: Option<&str>,
        stop: &dyn Fn() -> bool,
    ) -> Result<ModelOutcome, ModelError> {
        self.run_inner(req, stop)
    }

    fn resume(
        &self,
        ticket: &str,
        pasted: &str,
        label: Option<&str>,
    ) -> Result<ModelAnswer, ModelError> {
        self.resume_inner(ticket, pasted, label)
    }

    fn describe(&self) -> Vec<RouteInfo> {
        vec![self.describe_route()]
    }

    fn preview(&self, req: &ModelRequest, _via: Option<&str>) -> SendPreview {
        self.preview_inner(req)
    }
}

fn transfer_for(route: &Route, prepared: &PreparedAttempt) -> Transfer {
    if prepared.content_type.starts_with("text/plain") {
        Transfer::reply("handoff", &prepared.bytes)
    } else {
        Transfer::post_endpoint(route.base_url.clone(), &prepared.bytes)
    }
}

fn attempt_code(error: AttemptError) -> Code {
    match error {
        AttemptError::Transient(_) => Code::Fb0908,
        AttemptError::Permanent(Permanent::Auth) => Code::Fb0905,
        AttemptError::Permanent(Permanent::Rejected(413)) => Code::Fb0907,
        AttemptError::Permanent(
            Permanent::Rejected(_) | Permanent::Redirect | Permanent::Unavailable(Reason::Binding),
        ) => Code::Fb0906,
        AttemptError::Permanent(Permanent::Tls) => Code::Fb0910,
        AttemptError::Permanent(Permanent::Truncated) => Code::Fb0909,
        AttemptError::Permanent(Permanent::TooLarge) => Code::Fb0919,
        AttemptError::Permanent(Permanent::Unavailable(Reason::Net)) => Code::Fb0912,
        AttemptError::Permanent(Permanent::Unavailable(Reason::Cpu)) => Code::Fb0911,
        AttemptError::Permanent(Permanent::Unavailable(Reason::Key)) => Code::Fb0914,
    }
}

fn validate_reply(body: &str, schema: &fieldbook_core::ports::Schema) -> Result<(), Vec<String>> {
    let json = extract_json(body).map_err(|_| vec!["not_json at 0".to_owned()])?;
    let value = parse_json_value(json).map_err(|_| vec!["not_json at 0".to_owned()])?;
    validate(schema, &value)
        .map_err(|failure| vec![format!("{} at {}", failure.code.as_str(), failure.node)])
}

fn repair_request(req: &ModelRequest, diagnostics: &[String]) -> ModelRequest {
    let listed = diagnostics
        .iter()
        .take(MAX_REPAIR_DIAGNOSTICS)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let mut parts = req.parts().to_vec();
    parts.push(Part::rules(format!(
        "The previous reply did not match the required schema ({listed}). Reply with only the JSON value."
    )));
    ModelRequest::new(
        req.purpose(),
        req.template(),
        parts,
        req.schema().cloned(),
        req.limits(),
    )
    .unwrap_or_else(|_| req.clone())
}

fn row_state(policy: &EffectivePolicy, route: &Route) -> RowState {
    let Some(policy_value) = policy.state().policy() else {
        return RowState::Missing;
    };
    let kind = if route.base_url.is_empty() {
        DestKind::Handoff
    } else {
        DestKind::Model
    };
    let matching = policy_value
        .destinations()
        .iter()
        .filter(|row| row.kind == kind)
        .filter(|row| kind == DestKind::Handoff || row.name.as_deref() == Some(route.name.as_str()))
        .collect::<Vec<_>>();
    if matching.is_empty() {
        return RowState::Missing;
    }
    RowState::Found
}

/// Returns `true` when a denial is terminal for a routing decision.
#[must_use]
pub fn denial_is_terminal(code: Code) -> bool {
    matches!(code, Code::Fb0413 | Code::Fb0925)
}

/// Returns the labels a route would allow, for the Router's coverage test.
pub fn covers(
    policy: &EffectivePolicy,
    dest: &Dest,
    labels: &LabelSet,
    now: fieldbook_core::audit::Now,
    shipped: Shipped,
) -> Result<bool, ModelError> {
    match egress::check(policy, dest, Some(labels), None, now, shipped) {
        Decision::Allowed(_) => Ok(true),
        Decision::Denied(denial) if denial_is_terminal(denial.code) => {
            Err(ModelError::new(denial.code, ErrorKind::Policy))
        }
        Decision::Denied(_) => Ok(false),
    }
}
