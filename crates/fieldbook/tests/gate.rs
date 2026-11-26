//! Integration tests for the model gate (`model-gate` step 10).
//!
//! Every test writes its own small policy as text and parses it with `Policy::parse`; an
//! `Allowed` token is obtained only through `egress::check`.

#![allow(
    clippy::err_expect,
    clippy::manual_let_else,
    clippy::needless_borrow,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "integration tests build temporary policies and doubles"
)]

mod support;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use fieldbook::adapters::doubles::{
    BlockingModel, HandoffAdapter, HttpModel, MemCalls, MemJournal, PanicModel, ScriptedModel,
    ScriptedPace, pick_schema, test_policy,
};
use fieldbook::adapters::gate::Guarded;
use fieldbook::adapters::model::{
    AiJournal, AttemptError, Deps, KeepSettings, ModelAdapter, ModelError, ModelOutcome, ModelPort,
    PolicySnapshot, Route, RunBudget,
};
use fieldbook::adapters::router::Router;
use fieldbook::adapters::{Factory, registry};
use fieldbook::io::keys::{KeySource, ScriptedKeys};
use fieldbook::io::net::{Avail, HttpPost, HttpReply, ScriptedHttp};
use fieldbook::settings::Settings;
use fieldbook_core::CivilDate;
use fieldbook_core::audit::{DestKind, Now, RecordRef, Sha256, StoreId};
use fieldbook_core::check::Code as CheckCode;
use fieldbook_core::egress::{self, Decision, Delivery, Dest, Shipped, Target, Transfer};
use fieldbook_core::labels::{
    ItemRef, LabelAllow, LabelGraph, LabelId, LabelNode, LabelSet, Ledger,
};
use fieldbook_core::policy::{ClosedWhy, EffectivePolicy, OwnerSettings, PolicyState};
use fieldbook_core::ports::{
    Caps, Field, Limits, ModelRequest, Part, PartKind, Purpose, Schema, Template,
};
use fieldbook_core::schema_json::JsonValue;

const POLICY: &str = r#"
policy_schema = 1
revision = 1

[[label]]
id = "np-0007"
title = "Synthetic supplier terms"
banner = "NP-0007: synthetic restricted banner"
restricted = true

[[label]]
id = "bid-0003"
title = "Synthetic bid cost"

[[marking]]
match = "NP-0007 NDA"
labels = ["np-0007"]

[[store]]
id = "state"
holds = ["public", "unlabelled", "owner-supplied", "np-0007"]
readers = 'EXAMPLE\Fieldbook-Readers'

[[destination]]
kind = "model"
name = "work"
url = "https://work.example/v1"
allow = ["public", "owner-supplied", "np-0007"]
keep_calls_days_max = 14
ref = "CHG-0001"

[[destination]]
kind = "handoff"
name = "approved-assistant"
allow = ["public", "owner-supplied", "np-0007"]
ref = "CHG-0002"

[audit]
sink = "/srv/company/audit"
max_unshipped_hours = 24
"#;

const POLICY_NO_EXPORT: &str = r#"
policy_schema = 1
revision = 1

[[label]]
id = "np-0007"
title = "Synthetic supplier terms"
restricted = true

[[store]]
id = "state"
holds = ["public", "unlabelled", "owner-supplied", "np-0007"]
readers = 'EXAMPLE\Fieldbook-Readers'

[[destination]]
kind = "model"
name = "work"
url = "https://work.example/v1"
allow = ["public", "owner-supplied", "np-0007"]
keep_calls_days_max = 14

[[destination]]
kind = "handoff"
allow = ["public", "owner-supplied", "np-0007"]

[audit]
require_fresh_export = false
max_unshipped_hours = 24
"#;

fn base_url() -> &'static str {
    "https://work.example/v1"
}

fn now() -> Now {
    Now::new(CivilDate::from_ymd(2026, 3, 1).expect("date"), 43_200).expect("now")
}

fn default_owner() -> OwnerSettings {
    let mut owner = OwnerSettings::unset();
    owner.endpoints.insert(
        "work".to_owned(),
        LabelAllow::from_ids([
            LabelId::parse("public").expect("public"),
            LabelId::parse("owner-supplied").expect("owner-supplied"),
            np(),
        ]),
    );
    owner.keep_calls = Some(true);
    owner.keep_days = Some(30);
    owner
}

fn np() -> LabelId {
    LabelId::parse("np-0007").expect("label")
}

fn bid() -> LabelId {
    LabelId::parse("bid-0003").expect("label")
}

fn np_labels() -> LabelSet {
    LabelSet::from_ids([np()])
}

fn bid_labels() -> LabelSet {
    LabelSet::from_ids([bid()])
}

struct Harness {
    mem: Arc<MemJournal>,
    calls: Arc<MemCalls>,
    pace: Arc<ScriptedPace>,
    budget: Arc<RunBudget>,
    snap: Arc<Mutex<PolicySnapshot>>,
    keys: Arc<dyn KeySource + Send + Sync>,
    http: Option<Arc<dyn HttpPost + Send + Sync>>,
    avail: Avail,
    retries: u32,
    keep: KeepSettings,
}

impl Harness {
    fn new(text: &str, owner: &OwnerSettings, shipped: Shipped) -> Self {
        let mem = Arc::new(MemJournal::new());
        Self {
            mem,
            calls: Arc::new(MemCalls::new()),
            pace: Arc::new(ScriptedPace::default()),
            budget: Arc::new(RunBudget::new(100)),
            snap: Arc::new(Mutex::new(PolicySnapshot {
                policy: test_policy(text, owner),
                now: now(),
                shipped,
            })),
            keys: Arc::new(ScriptedKeys::new()),
            http: None,
            avail: Avail {
                net_built: true,
                bridge_built: false,
                cpu_ok: true,
            },
            retries: 2,
            keep: KeepSettings {
                calls: true,
                days: 30,
            },
        }
    }

    fn shipped(shipped: Shipped) -> Self {
        Self::new(POLICY, &default_owner(), shipped)
    }

    fn set_policy(&self, policy: EffectivePolicy) {
        if let Ok(mut snap) = self.snap.lock() {
            snap.policy = policy;
        }
    }

    fn policy_hash(&self) -> Sha256 {
        Sha256::parse(&self.snap.lock().expect("snapshot").policy.hash()).expect("policy hash")
    }

    fn with_keys(mut self, keys: Arc<dyn KeySource + Send + Sync>) -> Self {
        self.keys = keys;
        self
    }

    fn with_http(mut self, http: Arc<dyn HttpPost + Send + Sync>) -> Self {
        self.http = Some(http);
        self
    }

    fn deps(&self) -> Deps {
        let snap = Arc::clone(&self.snap);
        Deps {
            journal: self.mem.clone(),
            keys: self.keys.clone(),
            pace: self.pace.clone(),
            policy_provider: Arc::new(move || snap.lock().expect("snapshot").clone()),
            store: self.calls.clone(),
            keep: self.keep,
            budget: self.budget.clone(),
            avail: self.avail,
            retries: self.retries,
            http: self.http.clone(),
        }
    }

    fn gate(&self, route: Route, model: Arc<dyn ModelAdapter + Send + Sync>) -> Guarded {
        Guarded::new(Box::new(model), route, self.deps())
    }
}

fn route(name: &str, url: &str) -> Route {
    Route {
        name: fieldbook_core::audit::EndpointName::parse(name).expect("name"),
        base_url: url.to_owned(),
        model: fieldbook_core::audit::ModelId::parse("test-model").expect("model"),
        timeout_s: 5,
        max_reply_bytes: 64 * 1024,
        key_env: None,
    }
}

fn work() -> Route {
    route("work", base_url())
}

fn handoff() -> Route {
    fieldbook::adapters::router::handoff_route()
}

fn question(labels: LabelSet, text: &str) -> ModelRequest {
    ModelRequest::new(
        Purpose::Ask,
        Template::new("ask@1").expect("template"),
        vec![Part::new(PartKind::Question, labels, text, Vec::new())],
        None,
        Limits::new(10, 10),
    )
    .expect("request")
}

fn question_with_schema(labels: LabelSet, text: &str, schema: Schema) -> ModelRequest {
    ModelRequest::new(
        Purpose::Ask,
        Template::new("ask@1").expect("template"),
        vec![Part::new(PartKind::Question, labels, text, Vec::new())],
        Some(schema),
        Limits::new(10, 10),
    )
    .expect("request")
}

fn request_with_inputs(labels: LabelSet, ids: Vec<ItemRef>) -> ModelRequest {
    ModelRequest::new(
        Purpose::Ask,
        Template::new("ask@1").expect("template"),
        vec![
            Part::new(PartKind::Context, labels.clone(), "Synthetic context", ids),
            Part::new(PartKind::Question, labels, "Synthetic question", Vec::new()),
        ],
        None,
        Limits::new(10, 10),
    )
    .expect("request")
}

fn scripted() -> Arc<ScriptedModel> {
    Arc::new(ScriptedModel::new("synthetic answer"))
}

fn run(gate: &Guarded, req: &ModelRequest) -> Result<ModelOutcome, ModelError> {
    gate.run(req, None, &|| false)
}

fn assert_code(result: Result<ModelOutcome, ModelError>, code: CheckCode) {
    let error = result.err().expect("a refusal");
    assert_eq!(error.code, code, "{error}");
}

#[test]
// control: HG-09
fn a_label_outside_the_row_is_refused_and_nothing_is_sent() {
    let h = Harness::shipped(Shipped::Hours(1));
    let model = scripted();
    let gate = h.gate(work(), model.clone());
    let req = question(bid_labels(), "What is the synthetic bid?");
    assert_code(run(&gate, &req), CheckCode::Fb0901);
    assert_eq!(model.count(), 0, "the double records no attempt");
    let refusals = h.mem.refusals();
    assert_eq!(refusals.len(), 1);
    assert_eq!(refusals[0].code, CheckCode::Fb0901);
    assert_eq!(refusals[0].reason, "label");
    assert_eq!(refusals[0].dest, Some(DestKind::Model));
}

#[test]
fn one_label_outside_the_row_refuses_the_whole_request() {
    let h = Harness::shipped(Shipped::Hours(1));
    let model = scripted();
    let gate = h.gate(work(), model.clone());
    let req = question(LabelSet::from_ids([np(), bid()]), "Synthetic question");
    assert_code(run(&gate, &req), CheckCode::Fb0901);
    assert_eq!(model.count(), 0);
}

#[test]
fn an_unlabelled_or_marked_part_is_refused_even_if_a_row_lists_it() {
    for labels in [LabelSet::unlabelled(), LabelSet::marked()] {
        let h = Harness::new(POLICY, &default_owner(), Shipped::Hours(1));
        let model = scripted();
        let gate = h.gate(work(), model.clone());
        let req = question(labels, "Synthetic question");
        assert_code(run(&gate, &req), CheckCode::Fb0901);
        assert_eq!(model.count(), 0);
    }
}

#[test]
fn no_row_an_expired_row_and_a_different_url_are_fb0918() {
    let no_row = POLICY.replace(
        "[[destination]]\nkind = \"model\"\nname = \"work\"\nurl = \"https://work.example/v1\"\nallow = [\"public\", \"owner-supplied\", \"np-0007\"]\nkeep_calls_days_max = 14\nref = \"CHG-0001\"\n\n",
        "",
    );
    let expired = POLICY.replace(
        "keep_calls_days_max = 14",
        "keep_calls_days_max = 14\nexpires = 2020-01-01",
    );
    let wrong_url = POLICY.replace("https://work.example/v1", "https://other.example/v1");
    for text in [no_row, expired, wrong_url] {
        let h = Harness::new(&text, &default_owner(), Shipped::Hours(1));
        let model = scripted();
        let gate = h.gate(work(), model.clone());
        let req = question(LabelSet::public(), "Synthetic question");
        assert_code(run(&gate, &req), CheckCode::Fb0918);
        assert_eq!(model.count(), 0);
    }
}

#[test]
fn a_labelled_model_call_with_no_export_or_a_stale_one_is_fb0925_and_a_public_call_is_not() {
    let h = Harness::shipped(Shipped::Never);
    let model = scripted();
    let gate = h.gate(work(), model.clone());
    assert_code(
        run(&gate, &question(np_labels(), "Synthetic question")),
        CheckCode::Fb0925,
    );
    assert_eq!(model.count(), 0);

    let h = Harness::shipped(Shipped::Hours(9999));
    let model = scripted();
    let gate = h.gate(work(), model.clone());
    assert_code(
        run(&gate, &question(np_labels(), "Synthetic question")),
        CheckCode::Fb0925,
    );
    assert_eq!(model.count(), 0);

    let h = Harness::shipped(Shipped::Never);
    let model = scripted();
    let gate = h.gate(work(), model.clone());
    assert!(run(&gate, &question(LabelSet::public(), "Synthetic question")).is_ok());
    assert_eq!(model.count(), 1);
}

#[test]
fn with_the_requirement_off_a_lapsed_export_allows_the_call_and_the_policy_hash_is_recorded() {
    for shipped in [Shipped::Hours(9999), Shipped::Never] {
        let h = Harness::new(POLICY_NO_EXPORT, &default_owner(), shipped);
        let model = scripted();
        let gate = h.gate(work(), model.clone());
        assert!(run(&gate, &question(np_labels(), "Synthetic question")).is_ok());
        assert_eq!(model.count(), 1);
        let expected = h.policy_hash();
        match &h.mem.begins()[0] {
            fieldbook::adapters::model::BeginRecord::Model(event) => {
                assert_eq!(event.policy, expected);
            }
            _ => panic!("a model begin"),
        }
        assert_eq!(h.calls.calls()[0].policy, expected);
    }
}

#[test]
fn a_closed_policy_refuses_every_model_call() {
    let h = Harness::shipped(Shipped::Hours(1));
    h.set_policy(EffectivePolicy::new(
        &PolicyState::Closed(ClosedWhy::Absent),
        &default_owner(),
    ));
    let model = scripted();
    let gate = h.gate(work(), model.clone());
    assert_code(
        run(&gate, &question(LabelSet::public(), "Synthetic question")),
        CheckCode::Fb0413,
    );
    assert_eq!(model.count(), 0);
}

#[test]
fn the_owner_list_narrows_and_never_widens_the_row() {
    let mut owner = OwnerSettings::unset();
    owner.endpoints.insert(
        "work".to_owned(),
        fieldbook_core::labels::LabelAllow::from_ids([LabelId::parse("public").expect("public")]),
    );
    let h = Harness::new(POLICY, &owner, Shipped::Hours(1));
    let model = scripted();
    let gate = h.gate(work(), model.clone());
    assert_code(
        run(&gate, &question(np_labels(), "Synthetic question")),
        CheckCode::Fb0901,
    );
    assert_eq!(model.count(), 0);
}

#[test]
fn request_labels_are_the_union_of_parts() {
    let req = ModelRequest::new(
        Purpose::Ask,
        Template::new("ask@1").expect("template"),
        vec![
            Part::new(PartKind::Context, LabelSet::public(), "context", Vec::new()),
            Part::new(PartKind::Question, np_labels(), "question", Vec::new()),
        ],
        None,
        Limits::new(10, 10),
    )
    .expect("request");
    assert_eq!(req.labels(), np_labels());
}

#[test]
fn a_refuse_event_holds_ids_and_a_closed_reason_and_no_text() {
    let h = Harness::shipped(Shipped::Hours(1));
    let gate = h.gate(work(), scripted());
    let req = question(bid_labels(), "Sensitive question text");
    assert_code(run(&gate, &req), CheckCode::Fb0901);
    let refusal = &h.mem.refusals()[0];
    assert_eq!(refusal.labels.ids(), &[bid()]);
    assert_eq!(refusal.reason, "label");
    assert!(!format!("{refusal:?}").contains("Sensitive question text"));
}

#[test]
fn ai_begin_carries_dest_labels_and_policy() {
    let h = Harness::shipped(Shipped::Hours(1));
    let gate = h.gate(work(), scripted());
    let req = question(np_labels(), "Synthetic question");
    assert!(run(&gate, &req).is_ok());
    match &h.mem.begins()[0] {
        fieldbook::adapters::model::BeginRecord::Model(event) => {
            assert_eq!(event.dest, DestKind::Model);
            assert_eq!(event.labels, np_labels());
            assert_eq!(event.policy, h.policy_hash());
        }
        _ => panic!("a model begin"),
    }
}

#[test]
fn a_pack_event_carries_dest_labels_and_policy() {
    let h = Harness::shipped(Shipped::Hours(1));
    let gate = h.gate(handoff(), Arc::new(HandoffAdapter));
    let req = question(np_labels(), "Synthetic question");
    assert!(matches!(run(&gate, &req), Ok(ModelOutcome::Deferred(_))));
    match &h.mem.begins()[0] {
        fieldbook::adapters::model::BeginRecord::Handoff(event) => {
            assert_eq!(event.dest, DestKind::Handoff);
            assert_eq!(event.labels, np_labels());
            assert_eq!(event.policy, h.policy_hash());
        }
        _ => panic!("a pack event"),
    }
}

#[test]
fn an_availability_failure_falls_back_to_a_pack_when_handoff_covers_the_request() {
    let h = Harness::shipped(Shipped::Hours(1));
    let error = router_run(&h, &["work", "handoff"], LabelSet::public(), true);
    assert_eq!(error.code, CheckCode::Fb0912);
    assert!(error.ticket.is_some(), "the error carries the pack ticket");
    assert!(
        h.mem
            .begins()
            .iter()
            .any(|record| matches!(record, fieldbook::adapters::model::BeginRecord::Handoff(_)))
    );
}

#[test]
fn no_pack_is_built_when_the_handoff_row_does_not_cover_the_request() {
    let text = POLICY
        .replace("kind = \"handoff\"\nname = \"approved-assistant\"\nallow = [\"public\", \"owner-supplied\", \"np-0007\"]",
                 "kind = \"handoff\"\nname = \"approved-assistant\"\nallow = [\"public\"]");
    let h = Harness::new(&text, &default_owner(), Shipped::Hours(1));
    let error = router_run(&h, &["work", "handoff"], np_labels(), true);
    assert_eq!(error.code, CheckCode::Fb0912);
    assert!(error.ticket.is_none());
}

#[test]
fn leaving_handoff_out_of_order_turns_the_fallback_off() {
    let h = Harness::shipped(Shipped::Hours(1));
    let error = router_run(&h, &["work"], LabelSet::public(), true);
    assert_eq!(error.code, CheckCode::Fb0912);
    assert!(error.ticket.is_none());
}

#[test]
fn a_refusal_never_falls_back() {
    let h = Harness::shipped(Shipped::Hours(1));
    let open = test_policy(POLICY, &default_owner());
    let narrow_text = POLICY.replace(
        "kind = \"model\"\nname = \"work\"\nurl = \"https://work.example/v1\"\nallow = [\"public\", \"owner-supplied\", \"np-0007\"]",
        "kind = \"model\"\nname = \"work\"\nurl = \"https://work.example/v1\"\nallow = [\"public\", \"owner-supplied\"]",
    );
    let narrow = test_policy(&narrow_text, &default_owner());
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let open = Arc::new(Mutex::new(open));
    let narrow = Arc::new(Mutex::new(narrow));
    let deps = Deps {
        journal: h.mem.clone(),
        keys: h.keys.clone(),
        pace: h.pace.clone(),
        policy_provider: Arc::new(move || {
            let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let policy = if n == 0 {
                open.lock().expect("open").clone()
            } else {
                narrow.lock().expect("narrow").clone()
            };
            PolicySnapshot {
                policy,
                now: now(),
                shipped: Shipped::Hours(1),
            }
        }),
        store: h.calls.clone(),
        keep: h.keep,
        budget: h.budget.clone(),
        avail: h.avail,
        retries: h.retries,
        http: h.http.clone(),
    };
    let settings = Settings::parse(
        &format!(
            "[model]\norder = [\"work\", \"handoff\"]\n\n[model.endpoint.work]\nadapter = \"scripted@1\"\nbase_url = \"{}\"\nmodel = \"test-model\"\nallow = [\"public\", \"owner-supplied\", \"np-0007\"]\n",
            base_url()
        ),
        &[model_row()],
    )
    .expect("settings");
    let router =
        Router::from_settings(&settings, &[model_row()], &factories(), deps).expect("router");
    let req = question(np_labels(), "Synthetic question");
    let error = router.run(&req, None, &|| false).err().expect("a refusal");
    assert_eq!(error.code, CheckCode::Fb0901);
    assert!(error.ticket.is_none());
    assert!(h.calls.calls().is_empty());
}

#[test]
fn a_missing_key_env_is_fb0914_before_any_send() {
    let mut route = work();
    route.key_env = Some("SYNTHETIC_UNSET_KEY".to_owned());
    let h = Harness::shipped(Shipped::Hours(1));
    let model = scripted();
    let gate = h.gate(route, model.clone());
    assert_code(
        run(&gate, &question(LabelSet::public(), "Synthetic question")),
        CheckCode::Fb0914,
    );
    assert_eq!(model.count(), 0);
    assert!(h.mem.begins().is_empty());
}

#[test]
fn a_build_without_net_is_fb0912_and_a_missing_cpu_feature_is_fb0911() {
    let mut h = Harness::shipped(Shipped::Hours(1));
    h.avail.net_built = false;
    let model = scripted();
    let gate = h.gate(work(), model.clone());
    assert_code(
        run(&gate, &question(LabelSet::public(), "Synthetic question")),
        CheckCode::Fb0912,
    );
    assert_eq!(model.count(), 0);

    let mut h = Harness::shipped(Shipped::Hours(1));
    h.avail.cpu_ok = false;
    let model = scripted();
    let gate = h.gate(work(), model.clone());
    assert_code(
        run(&gate, &question(LabelSet::public(), "Synthetic question")),
        CheckCode::Fb0911,
    );
    assert_eq!(model.count(), 0);

    let mut h = Harness::shipped(Shipped::Hours(1));
    h.avail.bridge_built = false;
    let model = scripted();
    let gate = h.gate(work(), model.clone());
    assert!(run(&gate, &question(LabelSet::public(), "Synthetic question")).is_ok());
}

#[test]
fn stop_check_ends_retries() {
    let h = Harness::shipped(Shipped::Hours(1));
    let model = Arc::new(
        ScriptedModel::with_caps(Caps::new(true, false, false, false, 1_000_000))
            .push(Err(AttemptError::Transient(
                fieldbook::adapters::model::Transient::Connect,
            )))
            .push(Err(AttemptError::Transient(
                fieldbook::adapters::model::Transient::Connect,
            )))
            .push_reply("third"),
    );
    let gate = h.gate(work(), model.clone());
    let checks = Mutex::new(0);
    let stop = || {
        let mut n = checks.lock().expect("checks");
        *n += 1;
        *n > 2
    };
    let req = question(LabelSet::public(), "Synthetic question");
    let result = gate.run(&req, None, &stop);
    assert!(result.is_err());
    assert_eq!(model.count(), 2, "no third attempt");
}

#[test]
fn retry_only_transient() {
    let h = Harness::shipped(Shipped::Hours(1));
    let model = Arc::new(
        ScriptedModel::with_caps(Caps::new(true, false, false, false, 1_000_000))
            .push(Err(AttemptError::Transient(
                fieldbook::adapters::model::Transient::Server(503),
            )))
            .push(Err(AttemptError::Transient(
                fieldbook::adapters::model::Transient::Connect,
            )))
            .push_reply("answer"),
    );
    let gate = h.gate(work(), model.clone());
    assert!(run(&gate, &question(LabelSet::public(), "Synthetic question")).is_ok());
    assert_eq!(model.count(), 3);
    assert_eq!(h.pace.sleeps(), vec![1000, 4000]);
}

#[test]
fn retry_after_is_capped_at_30_seconds() {
    let h = Harness::shipped(Shipped::Hours(1));
    let model = Arc::new(
        ScriptedModel::with_caps(Caps::new(true, false, false, false, 1_000_000))
            .push(Err(AttemptError::Transient(
                fieldbook::adapters::model::Transient::RateLimited {
                    retry_after_s: Some(120),
                },
            )))
            .push_reply("answer"),
    );
    let gate = h.gate(work(), model.clone());
    assert!(run(&gate, &question(LabelSet::public(), "Synthetic question")).is_ok());
    assert_eq!(h.pace.sleeps(), vec![30_000]);
}

#[test]
fn auth_is_not_retried() {
    let h = Harness::shipped(Shipped::Hours(1));
    let model = Arc::new(
        ScriptedModel::with_caps(Caps::new(true, false, false, false, 1_000_000)).push(Err(
            AttemptError::Permanent(fieldbook::adapters::model::Permanent::Auth),
        )),
    );
    let gate = h.gate(work(), model.clone());
    assert_code(
        run(&gate, &question(LabelSet::public(), "Synthetic question")),
        CheckCode::Fb0905,
    );
    assert_eq!(model.count(), 1);
    assert!(h.pace.sleeps().is_empty());
}

#[test]
fn a_404_and_a_redirect_are_fb0906() {
    for error in [
        AttemptError::Permanent(fieldbook::adapters::model::Permanent::Rejected(404)),
        AttemptError::Permanent(fieldbook::adapters::model::Permanent::Redirect),
    ] {
        let h = Harness::shipped(Shipped::Hours(1));
        let model = Arc::new(
            ScriptedModel::with_caps(Caps::new(true, false, false, false, 1_000_000))
                .push(Err(error)),
        );
        let gate = h.gate(work(), model.clone());
        assert_code(
            run(&gate, &question(LabelSet::public(), "Synthetic question")),
            CheckCode::Fb0906,
        );
        assert_eq!(model.count(), 1);
    }
}

#[test]
fn a_tls_failure_is_fb0910() {
    let h = Harness::shipped(Shipped::Hours(1));
    let model = Arc::new(
        ScriptedModel::with_caps(Caps::new(true, false, false, false, 1_000_000)).push(Err(
            AttemptError::Permanent(fieldbook::adapters::model::Permanent::Tls),
        )),
    );
    let gate = h.gate(work(), model.clone());
    assert_code(
        run(&gate, &question(LabelSet::public(), "Synthetic question")),
        CheckCode::Fb0910,
    );
}

#[test]
fn truncated_reply_is_fb0909() {
    let h = Harness::shipped(Shipped::Hours(1));
    let model = Arc::new(
        ScriptedModel::with_caps(Caps::new(true, false, false, false, 1_000_000)).push(Err(
            AttemptError::Permanent(fieldbook::adapters::model::Permanent::Truncated),
        )),
    );
    let gate = h.gate(work(), model.clone());
    assert_code(
        run(&gate, &question(LabelSet::public(), "Synthetic question")),
        CheckCode::Fb0909,
    );
    assert_eq!(h.mem.calls()[0].1.outcome, "truncated");
}

#[test]
fn invalid_reply_gets_one_repair() {
    let h = Harness::shipped(Shipped::Hours(1));
    let model = Arc::new(
        ScriptedModel::with_caps(Caps::new(true, false, false, false, 1_000_000))
            .push_reply("not json")
            .push_reply("{\"answer\":\"yes\"}"),
    );
    let gate = h.gate(work(), model.clone());
    let req = question_with_schema(LabelSet::public(), "Synthetic question", pick_schema());
    let outcome = run(&gate, &req).expect("a repaired answer");
    match outcome {
        ModelOutcome::Answer(answer) => assert_eq!(answer.text(), "{\"answer\":\"yes\"}"),
        _ => panic!("an answer"),
    }
    assert_eq!(model.count(), 2);
    assert_eq!(h.mem.begins().len(), 2);
    assert_eq!(h.mem.calls().len(), 2);
    assert_eq!(h.mem.calls()[0].1.outcome, "invalid");
    assert_eq!(h.mem.calls()[1].1.outcome, "ok");
}

#[test]
fn budget_stops_the_101st_call() {
    let h = Harness::shipped(Shipped::Hours(1));
    let mut model = ScriptedModel::with_caps(Caps::new(true, false, false, false, 1_000_000));
    for _ in 0..=100 {
        model = model.push_reply("answer");
    }
    let model = Arc::new(model);
    let gate = h.gate(work(), model.clone());
    let req = question(LabelSet::public(), "Synthetic question");
    for _ in 0..100 {
        assert!(run(&gate, &req).is_ok());
    }
    assert_code(run(&gate, &req), CheckCode::Fb0915);
    assert_eq!(model.count(), 100);
}

#[test]
fn the_budget_is_shared_by_every_endpoint() {
    let h = Harness::shipped(Shipped::Hours(1));
    let mut first_model = ScriptedModel::with_caps(Caps::new(true, false, false, false, 1_000_000));
    let mut second_model =
        ScriptedModel::with_caps(Caps::new(true, false, false, false, 1_000_000));
    for _ in 0..150 {
        first_model = first_model.push_reply("answer");
        second_model = second_model.push_reply("answer");
    }
    let first = Arc::new(first_model);
    let second = Arc::new(second_model);
    let a = h.gate(work(), first.clone());
    let b = h.gate(work(), second.clone());
    let req = question(LabelSet::public(), "Synthetic question");
    for _ in 0..100 {
        // Alternate the two routes; one shared budget drains.
        let gate = if first.count() <= second.count() {
            &a
        } else {
            &b
        };
        let _ = run(gate, &req);
    }
    let error = run(&a, &req).err().expect("the budget is spent");
    assert_eq!(error.code, CheckCode::Fb0915);
}

#[test]
fn prompt_over_max_prompt_chars_is_fb0907() {
    let h = Harness::shipped(Shipped::Hours(1));
    let model = Arc::new(ScriptedModel::with_caps(Caps::new(
        true, false, false, false, 5,
    )));
    let gate = h.gate(work(), model.clone());
    assert_code(
        run(
            &gate,
            &question(LabelSet::public(), "A question longer than five characters"),
        ),
        CheckCode::Fb0907,
    );
    assert_eq!(model.count(), 0);
}

#[test]
fn reply_over_the_cap_is_fb0919() {
    let h = Harness::shipped(Shipped::Hours(1));
    let model = scripted();
    let mut route = work();
    route.max_reply_bytes = 5;
    let gate = h.gate(route, model.clone());
    assert_code(
        run(&gate, &question(LabelSet::public(), "Synthetic question")),
        CheckCode::Fb0919,
    );
}

#[test]
fn stages_g3_to_g6_run_before_the_adapter() {
    let h = Harness::shipped(Shipped::Hours(1));
    let gate = h.gate(work(), Arc::new(PanicModel));
    let req = question(bid_labels(), "Synthetic question");
    assert_code(run(&gate, &req), CheckCode::Fb0901);
}

#[test]
fn the_adapter_receives_the_token_of_the_second_check() {
    let h = Harness::shipped(Shipped::Hours(1));
    let model = scripted();
    let gate = h.gate(work(), model.clone());
    assert!(run(&gate, &question(LabelSet::public(), "Synthetic question")).is_ok());
    assert!(model.attempts()[0].has_delivery);
}

#[test]
fn journal_refusing_begin_sends_nothing() {
    let h = Harness::shipped(Shipped::Hours(1));
    h.mem.fail_begin();
    let model = scripted();
    let gate = h.gate(work(), model.clone());
    assert_code(
        run(&gate, &question(LabelSet::public(), "Synthetic question")),
        CheckCode::Fb0903,
    );
    assert_eq!(model.count(), 0);
}

#[test]
// control: AL-04
fn ai_output_needs_event() {
    let h = Harness::shipped(Shipped::Hours(1));
    h.mem.fail_finish_on(1);
    let model = scripted();
    let gate = h.gate(work(), model.clone());
    assert_code(
        run(&gate, &question(LabelSet::public(), "Synthetic question")),
        CheckCode::Fb0904,
    );
}

#[test]
fn journal_failing_after_the_reply_discards_the_output() {
    let h = Harness::shipped(Shipped::Hours(1));
    h.mem.fail_finish_on(1);
    let model = scripted();
    let gate = h.gate(work(), model.clone());
    let error = run(&gate, &question(LabelSet::public(), "Synthetic question"))
        .err()
        .expect("a discarded output");
    assert_eq!(error.code, CheckCode::Fb0904);
    assert!(!error.to_string().contains("synthetic answer"));
}

#[test]
// control: HG-10
fn marking_in_a_context_part_is_refused() {
    let h = Harness::shipped(Shipped::Hours(1));
    let model = scripted();
    let gate = h.gate(work(), model.clone());
    let req = ModelRequest::new(
        Purpose::Ask,
        Template::new("ask@1").expect("template"),
        vec![Part::new(
            PartKind::Context,
            LabelSet::public(),
            "NP-0007 NDA synthetic context",
            Vec::new(),
        )],
        None,
        Limits::new(10, 10),
    )
    .expect("request");
    assert_code(run(&gate, &req), CheckCode::Fb0902);
    assert_eq!(model.count(), 0);
    assert_eq!(h.mem.refusals()[0].reason, "mismatch");
}

#[test]
// control: HG-10
fn marking_added_by_the_adapter_to_the_prepared_body_is_refused() {
    let h = Harness::shipped(Shipped::Hours(1));
    let model = Arc::new(
        ScriptedModel::new("synthetic answer")
            .with_prepare_suffix("\nNP-0007 NDA added by prepare"),
    );
    let gate = h.gate(work(), model.clone());
    let req = question(LabelSet::public(), "Synthetic question");
    assert_code(run(&gate, &req), CheckCode::Fb0902);
    assert_eq!(model.count(), 0);
}

#[test]
fn marking_in_the_question_is_refused() {
    let h = Harness::shipped(Shipped::Hours(1));
    let gate = h.gate(work(), scripted());
    let req = question(LabelSet::public(), "NP-0007 NDA synthetic question");
    assert_code(run(&gate, &req), CheckCode::Fb0902);
}

#[test]
fn a_marking_the_request_already_carries_is_no_mismatch() {
    let h = Harness::shipped(Shipped::Hours(1));
    let gate = h.gate(work(), scripted());
    let req = question(np_labels(), "NP-0007 NDA synthetic question");
    assert!(run(&gate, &req).is_ok());
}

#[test]
fn preview_makes_no_call_and_no_event() {
    let h = Harness::shipped(Shipped::Hours(1));
    let model = scripted();
    let gate = h.gate(work(), model.clone());
    let preview = gate.preview_inner(&question(LabelSet::public(), "Synthetic question"));
    assert!(preview.audited);
    assert_eq!(preview.route.as_deref(), Some("work"));
    assert_eq!(model.count(), 0);
    assert!(h.mem.begins().is_empty());
    assert!(h.mem.calls().is_empty());
}

#[test]
fn preview_audited_is_false_when_the_journal_cannot_append() {
    let h = Harness::shipped(Shipped::Hours(1));
    h.mem.set_audited(false);
    let model = scripted();
    let gate = h.gate(work(), model.clone());
    let preview = gate.preview_inner(&question(LabelSet::public(), "Synthetic question"));
    assert!(!preview.audited);
    assert_code(
        run(&gate, &question(LabelSet::public(), "Synthetic question")),
        CheckCode::Fb0903,
    );
    assert_eq!(model.count(), 0);
}

#[test]
fn resume_twice_is_fb0916() {
    let h = Harness::shipped(Shipped::Hours(1));
    let gate = h.gate(handoff(), Arc::new(HandoffAdapter));
    let req = question(LabelSet::public(), "Synthetic question");
    let ticket = match run(&gate, &req).expect("a pack") {
        ModelOutcome::Deferred(ticket) => ticket,
        _ => panic!("deferred"),
    };
    assert!(gate.resume(&ticket.id.format(), "pasted", None).is_ok());
    assert_code(
        gate.resume(&ticket.id.format(), "pasted again", None)
            .map(ModelOutcome::Answer),
        CheckCode::Fb0916,
    );
}

#[test]
fn a_failed_pack_write_is_an_error_and_never_an_invented_ticket() {
    let h = Harness::shipped(Shipped::Hours(1));
    h.calls.fail_put_pack();
    let gate = h.gate(handoff(), Arc::new(HandoffAdapter));
    let req = question(LabelSet::public(), "Synthetic question");
    assert_code(run(&gate, &req), CheckCode::Fb0916);
    assert_eq!(h.calls.pack_count(), 0);
}

#[test]
fn resume_with_an_empty_file_is_fb0916() {
    let h = Harness::shipped(Shipped::Hours(1));
    let gate = h.gate(handoff(), Arc::new(HandoffAdapter));
    let ticket =
        match run(&gate, &question(LabelSet::public(), "Synthetic question")).expect("pack") {
            ModelOutcome::Deferred(ticket) => ticket,
            _ => panic!("deferred"),
        };
    assert_code(
        gate.resume(&ticket.id.format(), "   ", None)
            .map(ModelOutcome::Answer),
        CheckCode::Fb0916,
    );
}

#[test]
fn call_record_is_written_and_expires() {
    let h = Harness::shipped(Shipped::Hours(1));
    let gate = h.gate(work(), scripted());
    let mut snap = h.snap.lock().expect("snapshot");
    snap.now = now();
    drop(snap);
    assert!(run(&gate, &question(LabelSet::public(), "Synthetic question")).is_ok());
    assert_eq!(h.calls.calls().len(), 1);

    let mut snap = h.snap.lock().expect("snapshot");
    snap.now = Now::new(CivilDate::from_ymd(2026, 4, 1).expect("date"), 0).expect("now");
    drop(snap);
    let gate = h.gate(work(), scripted());
    assert!(run(&gate, &question(LabelSet::public(), "Synthetic question")).is_ok());
    assert_eq!(h.calls.calls().len(), 1, "the first record expired");
}

#[test]
fn no_call_record_when_state_cannot_hold_the_labels() {
    let text = POLICY.replace(
        "holds = [\"public\", \"unlabelled\", \"owner-supplied\", \"np-0007\"]",
        "holds = [\"public\", \"unlabelled\", \"np-0007\"]",
    );
    let h = Harness::new(&text, &default_owner(), Shipped::Hours(1));
    let gate = h.gate(work(), scripted());
    let outcome = run(
        &gate,
        &question(LabelSet::owner_supplied(), "Synthetic question"),
    )
    .expect("answer");
    match outcome {
        ModelOutcome::Answer(answer) => {
            assert!(answer.warnings().iter().any(|w| w.contains("state store")));
        }
        _ => panic!("answer"),
    }
    assert!(h.calls.calls().is_empty());
}

#[test]
fn no_call_record_when_the_row_caps_retention_at_zero() {
    let text = POLICY.replace("keep_calls_days_max = 14", "keep_calls_days_max = 0");
    let h = Harness::new(&text, &default_owner(), Shipped::Hours(1));
    let gate = h.gate(work(), scripted());
    let outcome = run(&gate, &question(LabelSet::public(), "Synthetic question")).expect("answer");
    match outcome {
        ModelOutcome::Answer(answer) => assert!(!answer.warnings().is_empty()),
        _ => panic!("answer"),
    }
    assert!(h.calls.calls().is_empty());
}

#[test]
fn keep_days_is_capped_by_the_row() {
    let h = Harness::shipped(Shipped::Hours(1));
    let gate = h.gate(work(), scripted());
    assert!(run(&gate, &question(LabelSet::public(), "Synthetic question")).is_ok());
    assert_eq!(h.calls.calls()[0].keep_days, 14, "the row caps 30 at 14");
}

#[test]
fn call_record_failure_is_a_warning() {
    let h = Harness::shipped(Shipped::Hours(1));
    h.calls.fail_put_call();
    let gate = h.gate(work(), scripted());
    let outcome = run(&gate, &question(LabelSet::public(), "Synthetic question")).expect("answer");
    match outcome {
        ModelOutcome::Answer(answer) => assert!(!answer.warnings().is_empty()),
        _ => panic!("answer"),
    }
}

#[test]
fn router_picks_the_first_entry_of_order_that_covers_the_request() {
    let text = POLICY.replace(
        "kind = \"handoff\"\nname = \"approved-assistant\"\nallow = [\"public\", \"owner-supplied\", \"np-0007\"]",
        "kind = \"handoff\"\nname = \"approved-assistant\"\nallow = [\"public\"]",
    );
    let h = Harness::new(&text, &default_owner(), Shipped::Hours(1));
    let router = router(&h, &["handoff", "work"]);
    let plan = router.plan(None, &np_labels()).expect("a plan");
    assert_eq!(plan.name.as_str(), "work");
}

#[test]
fn no_entry_covers_the_request_is_fb0901() {
    let text = POLICY.replace(
        "allow = [\"public\", \"owner-supplied\", \"np-0007\"]",
        "allow = [\"public\", \"owner-supplied\"]",
    );
    let h = Harness::new(&text, &default_owner(), Shipped::Hours(1));
    let router = router(&h, &["work", "handoff"]);
    let error = router.plan(None, &np_labels()).err().expect("no candidate");
    assert_eq!(error.code, CheckCode::Fb0901);
    assert_eq!(h.mem.refusals()[0].dest, None);
}

#[test]
fn via_names_one_candidate_and_never_falls_through() {
    let h = Harness::shipped(Shipped::Hours(1));
    let router = router(&h, &["work", "handoff"]);
    let error = router
        .plan(Some("work"), &bid_labels())
        .err()
        .expect("work does not cover");
    assert_eq!(error.code, CheckCode::Fb0901);
}

#[test]
fn a_policy_refusal_never_moves_to_the_next_entry() {
    let h = Harness::shipped(Shipped::Never);
    let router = router(&h, &["work", "handoff"]);
    let error = router.plan(None, &np_labels()).err().expect("refused");
    assert_eq!(error.code, CheckCode::Fb0925);
}

#[test]
fn router_describe_shows_no_secret() {
    let keys = Arc::new(ScriptedKeys::new().with_key("SYNTHETIC_KEY", "super-secret-value"));
    let h = Harness::shipped(Shipped::Hours(1)).with_keys(keys);
    let router = router(&h, &["work", "handoff"]);
    let described = router.describe();
    let rendered = format!("{described:?}");
    assert!(!rendered.contains("super-secret-value"));
    assert_eq!(described.len(), 2);
}

#[test]
fn default_route_stale_export_does_not_fall_back() {
    let h = Harness::shipped(Shipped::Never);
    let router = router(&h, &["work", "handoff"]);
    let req = question(np_labels(), "Synthetic question");
    let plan = router
        .plan(None, &req.labels())
        .err()
        .expect("stale export");
    assert_eq!(plan.code, CheckCode::Fb0925);
    let error = router
        .run(&req, None, &|| false)
        .err()
        .expect("stale export");
    assert_eq!(error.code, CheckCode::Fb0925);
    assert!(error.ticket.is_none());
}

#[test]
fn answer_keeps_request_provenance_on_call_and_resume() {
    let h = Harness::shipped(Shipped::Hours(1));
    let ids = vec![
        ItemRef::new(StoreId::Corpus, "CL-CMP-0001"),
        ItemRef::new(StoreId::Overlay, "CL-CMP-0001"),
    ];
    let req = request_with_inputs(LabelSet::public(), ids.clone());
    let gate = h.gate(work(), scripted());
    match run(&gate, &req).expect("answer") {
        ModelOutcome::Answer(answer) => {
            assert_eq!(answer.request_labels(), &LabelSet::public());
            assert_eq!(answer.input_refs(), ids.as_slice());
        }
        _ => panic!("answer"),
    }
    match &h.mem.begins()[0] {
        fieldbook::adapters::model::BeginRecord::Model(event) => {
            assert_eq!(
                event.context_ids,
                vec![
                    RecordRef::parse("corpus/CL-CMP-0001").expect("ref"),
                    RecordRef::parse("overlay/CL-CMP-0001").expect("ref"),
                ]
            );
        }
        _ => panic!("model begin"),
    }

    let handoff_gate = h.gate(handoff(), Arc::new(HandoffAdapter));
    let ticket = match run(&handoff_gate, &req).expect("pack") {
        ModelOutcome::Deferred(ticket) => ticket,
        _ => panic!("deferred"),
    };
    let answer = handoff_gate
        .resume(&ticket.id.format(), "pasted", Some("assistant"))
        .expect("resume");
    assert_eq!(answer.request_labels(), &LabelSet::public());
    assert_eq!(answer.input_refs(), ids.as_slice());
}

#[test]
fn identical_packs_resume_with_their_own_sequence() {
    let h = Harness::shipped(Shipped::Hours(1));
    let gate = h.gate(handoff(), Arc::new(HandoffAdapter));
    let req = question(LabelSet::public(), "Synthetic question");
    let first = match run(&gate, &req).expect("first pack") {
        ModelOutcome::Deferred(ticket) => ticket,
        _ => panic!("deferred"),
    };
    let second = match run(&gate, &req).expect("second pack") {
        ModelOutcome::Deferred(ticket) => ticket,
        _ => panic!("deferred"),
    };
    assert_ne!(first.id, second.id);
    assert!(
        gate.resume(&second.id.format(), "second paste", None)
            .is_ok()
    );
    assert!(gate.resume(&first.id.format(), "first paste", None).is_ok());
    let pastes = h.mem.pastes();
    assert_eq!(pastes[0].pack_seq, Some(2));
    assert_eq!(pastes[1].pack_seq, Some(1));
}

#[test]
fn repair_repeats_scan_and_budget() {
    use fieldbook::adapters::doubles::FaultyJournal;

    // 1. Repair re-enters G2..G7: draws a second budget unit, records two distinct completed
    //    ai.begin/ai.call pairs with different tx (the first ending `invalid` before the repair
    //    begins), and sends two requests through ScriptedHttp.
    let http = Arc::new(ScriptedHttp::new());
    http.push(HttpReply::new(200, b"not json".to_vec()));
    http.push(HttpReply::new(200, b"{\"answer\":\"yes\"}".to_vec()));
    let keys: Arc<dyn KeySource + Send + Sync> = Arc::new(ScriptedKeys::new());
    let h = Harness::shipped(Shipped::Hours(1)).with_http(http.clone());
    let model = Arc::new(HttpModel::new(http.clone(), keys.clone()));
    let gate = h.gate(work(), model);
    let req = question_with_schema(LabelSet::public(), "Synthetic question", pick_schema());
    assert!(run(&gate, &req).is_ok());
    assert_eq!(h.budget.left(), 98);
    assert_eq!(http.requests().len(), 2);
    let pairs = h.mem.completed_pairs();
    assert_eq!(pairs.len(), 2);
    assert_ne!(pairs[0], pairs[1]);
    assert_eq!(h.mem.calls()[0].1.outcome, "invalid");
    assert_eq!(h.mem.calls()[0].1.code, Some(CheckCode::Fb0909));
    assert_eq!(h.mem.calls()[1].1.outcome, "ok");

    // 2. Repair repeats the marking scan and policy check: if a marking rule added before the
    //    repair matches the prompt, the repair is refused at G4 with FB0902 and no second HTTP
    //    request is sent.
    let http_scan = Arc::new(ScriptedHttp::new());
    http_scan.push(HttpReply::new(200, b"not json".to_vec()));
    http_scan.push(HttpReply::new(200, b"{\"answer\":\"yes\"}".to_vec()));
    let initial_policy = test_policy(POLICY, &default_owner());
    let repair_policy_text =
        format!("{POLICY}\n[[marking]]\nmatch = \"Synthetic question\"\nlabels = [\"np-0007\"]\n");
    let repair_policy = test_policy(&repair_policy_text, &default_owner());
    let snap_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let snap_count_for_provider = Arc::clone(&snap_count);
    let mut deps_scan = h.deps();
    deps_scan.journal = Arc::new(MemJournal::new());
    deps_scan.budget = Arc::new(RunBudget::new(100));
    deps_scan.http = Some(http_scan.clone());
    deps_scan.policy_provider = Arc::new(move || {
        let idx = snap_count_for_provider.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        PolicySnapshot {
            policy: if idx == 0 {
                initial_policy.clone()
            } else {
                repair_policy.clone()
            },
            now: now(),
            shipped: Shipped::Hours(1),
        }
    });
    let gate_scan = Guarded::new(
        Box::new(HttpModel::new(http_scan.clone(), keys.clone())),
        work(),
        deps_scan,
    );
    let scan_err = run(&gate_scan, &req)
        .err()
        .expect("repair marking scan must refuse");
    assert_eq!(scan_err.code, CheckCode::Fb0902);
    assert_eq!(http_scan.requests().len(), 1);

    // 3. FaultyJournal whose first ai.call append fails: the run keeps FB0904, makes no repair
    //    request, and discards the unaudited reply while the single initial transport request
    //    stands in ScriptedHttp's record, with no reply text in the diagnostic.
    let http_faulty = Arc::new(ScriptedHttp::new());
    http_faulty.push(HttpReply::new(
        200,
        b"not json secret-invalid-reply".to_vec(),
    ));
    http_faulty.push(HttpReply::new(200, b"{\"answer\":\"yes\"}".to_vec()));
    let faulty = Arc::new(FaultyJournal::new());
    faulty.fail_finish_on(1);
    let mut deps_faulty = h.deps();
    deps_faulty.journal = faulty.clone();
    deps_faulty.budget = Arc::new(RunBudget::new(100));
    deps_faulty.http = Some(http_faulty.clone());
    let gate_faulty = Guarded::new(
        Box::new(HttpModel::new(http_faulty.clone(), keys)),
        work(),
        deps_faulty,
    );
    let faulty_err = run(&gate_faulty, &req)
        .err()
        .expect("failed first ai.call must abort before repair");
    assert_eq!(faulty_err.code, CheckCode::Fb0904);
    assert_eq!(
        http_faulty.requests().len(),
        1,
        "only the initial transport request was sent; no repair request was made"
    );
    assert_eq!(faulty.inner().begins().len(), 1);
    assert!(faulty.inner().calls().is_empty());
    assert!(!faulty_err.to_string().contains("secret-invalid-reply"));
    assert!(!format!("{faulty_err:?}").contains("secret-invalid-reply"));
}

#[test]
fn repair_hash_is_actual_payload_hash() {
    let http = Arc::new(ScriptedHttp::new());
    http.push(HttpReply::new(200, b"not json".to_vec()));
    http.push(HttpReply::new(200, b"{\"answer\":\"yes\"}".to_vec()));
    let keys: Arc<dyn KeySource + Send + Sync> = Arc::new(ScriptedKeys::new());
    let h = Harness::shipped(Shipped::Hours(1)).with_http(http.clone());
    let model = Arc::new(HttpModel::new(http.clone(), keys));
    let gate = h.gate(work(), model);
    let req = question_with_schema(LabelSet::public(), "Synthetic question", pick_schema());
    assert!(run(&gate, &req).is_ok());
    let sent = http.requests();
    assert_eq!(sent.len(), 2);
    let begins = h.mem.begins();
    assert_eq!(begins.len(), 2);
    let fieldbook::adapters::model::BeginRecord::Model(first) = &begins[0] else {
        panic!("first model begin");
    };
    let fieldbook::adapters::model::BeginRecord::Model(second) = &begins[1] else {
        panic!("second model begin");
    };
    assert_eq!(first.prompt_sha256, Sha256::digest(&sent[0].body));
    assert_eq!(second.prompt_sha256, Sha256::digest(&sent[1].body));
    assert_ne!(
        first.prompt_sha256, second.prompt_sha256,
        "repair payload includes schema diagnostics and must have its own distinct payload hash"
    );
}

#[test]
fn repair_paths_cannot_inject_text() {
    let h = Harness::shipped(Shipped::Hours(1));
    let model = Arc::new(
        ScriptedModel::with_caps(Caps::new(true, false, false, false, 1_000_000))
            .push_reply("canary text that must not be copied")
            .push_reply("{\"answer\":\"yes\"}"),
    );
    let gate = h.gate(work(), model.clone());
    let req = question_with_schema(LabelSet::public(), "Synthetic question", pick_schema());
    assert!(run(&gate, &req).is_ok());
    let attempts = model.attempts();
    assert_eq!(attempts.len(), 2);
    assert!(!String::from_utf8_lossy(&attempts[1].bytes).contains("canary"));
}

#[test]
fn retry_after_revocation_sends_nothing() {
    let h = Harness::shipped(Shipped::Hours(1));
    let model = Arc::new(
        ScriptedModel::with_caps(Caps::new(true, false, false, false, 1_000_000))
            .push(Err(AttemptError::Transient(
                fieldbook::adapters::model::Transient::Connect,
            )))
            .push_reply("must not be sent"),
    );
    let snapshot = Arc::clone(&h.snap);
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count_for_provider = Arc::clone(&count);
    let deps = Deps {
        journal: h.mem.clone(),
        keys: h.keys.clone(),
        pace: h.pace.clone(),
        policy_provider: Arc::new(move || {
            if count_for_provider.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                snapshot.lock().expect("snapshot").clone()
            } else {
                PolicySnapshot {
                    policy: EffectivePolicy::new(
                        &PolicyState::Closed(ClosedWhy::Absent),
                        &default_owner(),
                    ),
                    now: now(),
                    shipped: Shipped::Hours(1),
                }
            }
        }),
        store: h.calls.clone(),
        keep: h.keep,
        budget: h.budget.clone(),
        avail: h.avail,
        retries: 2,
        http: None,
    };
    let gate = Guarded::new(Box::new(model.clone()), work(), deps);
    let error = run(&gate, &question(LabelSet::public(), "Synthetic question"))
        .err()
        .expect("refused after revocation");
    assert_eq!(error.code, CheckCode::Fb0413);
    assert_eq!(model.count(), 1, "the retry sends nothing");
}

#[test]
fn preview_generation_change_requires_new_preview() {
    let h = Harness::shipped(Shipped::Hours(1));
    let gate = h.gate(work(), scripted());
    let req = question(LabelSet::public(), "Synthetic question");
    let first = gate.preview_inner(&req);
    h.set_policy(EffectivePolicy::new(
        &PolicyState::Closed(ClosedWhy::Absent),
        &default_owner(),
    ));
    let second = gate.preview_inner(&req);
    assert_ne!(first.generation, second.generation);
}

#[test]
fn cross_endpoint_token_reuse_is_refused() {
    let http = Arc::new(ScriptedHttp::new());
    let keys: Arc<dyn KeySource + Send + Sync> = Arc::new(ScriptedKeys::new());
    let model = HttpModel::new(http.clone(), keys);
    let policy = test_policy(POLICY, &default_owner());
    let body = b"{}".to_vec();
    let allowed = match egress::check(
        &policy,
        &Dest::Model {
            name: "work".to_owned(),
            base_url: base_url().to_owned(),
        },
        Some(&LabelSet::public()),
        Some(&Delivery::new(
            vec![Transfer::post_endpoint(base_url(), &body)],
            None,
        )),
        now(),
        Shipped::Hours(1),
    ) {
        Decision::Allowed(allowed) => allowed,
        Decision::Denied(denial) => panic!("{denial}"),
    };
    let prepared = fieldbook::adapters::model::PreparedAttempt {
        target: Target::Endpoint("https://other.example/v1".to_owned()),
        bytes: body,
        content_type: "application/json".to_owned(),
    };
    let error = model
        .attempt(
            &allowed,
            &prepared,
            &route("work", "https://other.example/v1"),
        )
        .err()
        .expect("refused");
    assert!(matches!(
        error,
        AttemptError::Permanent(fieldbook::adapters::model::Permanent::Unavailable(_))
    ));
}

#[test]
fn changed_body_token_reuse_is_refused() {
    let http = Arc::new(ScriptedHttp::new());
    let keys: Arc<dyn KeySource + Send + Sync> = Arc::new(ScriptedKeys::new());
    let model = HttpModel::new(http.clone(), keys);
    let policy = test_policy(POLICY, &default_owner());
    let allowed = match egress::check(
        &policy,
        &Dest::Model {
            name: "work".to_owned(),
            base_url: base_url().to_owned(),
        },
        Some(&LabelSet::public()),
        Some(&Delivery::new(
            vec![Transfer::post_endpoint(base_url(), b"{}")],
            None,
        )),
        now(),
        Shipped::Hours(1),
    ) {
        Decision::Allowed(allowed) => allowed,
        Decision::Denied(denial) => panic!("{denial}"),
    };
    let prepared = fieldbook::adapters::model::PreparedAttempt {
        target: Target::Endpoint(base_url().to_owned()),
        bytes: b"changed".to_vec(),
        content_type: "application/json".to_owned(),
    };
    assert!(model.attempt(&allowed, &prepared, &work()).is_err());
}

fn token_for(body: &[u8]) -> fieldbook_core::egress::Allowed {
    let policy = test_policy(POLICY, &default_owner());
    match egress::check(
        &policy,
        &Dest::Model {
            name: "work".to_owned(),
            base_url: base_url().to_owned(),
        },
        Some(&LabelSet::public()),
        Some(&Delivery::new(
            vec![Transfer::post_endpoint(base_url(), body)],
            None,
        )),
        now(),
        Shipped::Hours(1),
    ) {
        Decision::Allowed(allowed) => allowed,
        Decision::Denied(denial) => panic!("{denial}"),
    }
}

#[test]
fn the_transport_door_refuses_a_token_that_does_not_cover_the_request() {
    use fieldbook::io::net::{HttpError, HttpPost, HttpReply, HttpRequest};
    let http = ScriptedHttp::new();
    http.push(HttpReply::new(200, b"{}".to_vec()));
    let allowed = token_for(b"{}");
    // Another body, another URL, and a URL with a query are each refused with no send.
    let other_body = HttpRequest::new(base_url(), b"changed".to_vec(), 5);
    let other_url = HttpRequest::new("https://other.example/v1", b"{}".to_vec(), 5);
    let with_query = HttpRequest::new(format!("{}?x=1", base_url()), b"{}".to_vec(), 5);
    let with_fragment = HttpRequest::new(format!("{}#x", base_url()), b"{}".to_vec(), 5);
    let with_user = HttpRequest::new(base_url().replacen("://", "://user@", 1), b"{}".to_vec(), 5);
    for request in [other_body, other_url, with_query, with_fragment, with_user] {
        assert!(matches!(
            http.post(&allowed, &request),
            Err(HttpError::Other)
        ));
    }
    assert_eq!(http.sends(), 0);
    let exact = HttpRequest::new(base_url(), b"{}".to_vec(), 5);
    assert!(http.post(&allowed, &exact).is_ok());
    assert_eq!(http.sends(), 1);
}

#[test]
fn the_bound_transfer_follows_the_adapters_declared_target() {
    // A network adapter whose prepared body is `text/plain` still binds a post to the endpoint it
    // declared, not a reply: binding `Transfer::reply` for a `Dest::Model` destination is refused
    // by `validate_delivery_binding` as FB0902 and blames the request's labels for the gate's own
    // choice.
    let http = Arc::new(ScriptedHttp::new());
    http.push(HttpReply::new(200, b"{\"answer\":\"yes\"}".to_vec()));
    let keys: Arc<dyn KeySource + Send + Sync> = Arc::new(ScriptedKeys::new());
    let h = Harness::shipped(Shipped::Hours(1)).with_http(http.clone());
    let model =
        Arc::new(HttpModel::new(http.clone(), keys).with_content_type("text/plain; charset=utf-8"));
    let gate = h.gate(work(), model);
    assert!(run(&gate, &question(LabelSet::public(), "Synthetic question")).is_ok());
    assert_eq!(http.sends(), 1);
    assert_eq!(http.requests()[0].url, base_url());
}

#[test]
fn a_suffixed_endpoint_is_bound_as_the_adapter_declares_it() {
    // `egress::check` allows an endpoint that carries a path suffix of the row's `base_url`, so an
    // OpenAI-compatible `/chat/completions` is reachable. Binding `base_url` while the adapter
    // sends the suffix makes the door's exact-target test refuse the send (FB0908) with a token
    // the policy granted.
    let http = Arc::new(ScriptedHttp::new());
    http.push(HttpReply::new(200, b"{\"answer\":\"yes\"}".to_vec()));
    let keys: Arc<dyn KeySource + Send + Sync> = Arc::new(ScriptedKeys::new());
    let h = Harness::shipped(Shipped::Hours(1)).with_http(http.clone());
    let model =
        Arc::new(HttpModel::new(http.clone(), keys).with_endpoint_suffix("/chat/completions"));
    let gate = h.gate(work(), model);
    assert!(run(&gate, &question(LabelSet::public(), "Synthetic question")).is_ok());
    assert_eq!(http.sends(), 1);
    assert_eq!(
        http.requests()[0].url,
        format!("{}/chat/completions", base_url())
    );
}

#[cfg(unix)]
#[test]
fn a_read_only_journal_is_not_audited() {
    // `SendPreview.audited` claims the journal can take the record this route writes before any
    // byte leaves. Probing the directory's shape answered `true` on every real state folder; this
    // asks the segment the next append would open.
    use fieldbook_core::audit::Via;
    use std::os::unix::fs::PermissionsExt as _;

    let dir = tempfile::tempdir().expect("tempdir");
    let world: Arc<dyn fieldbook::World + Send + Sync> =
        Arc::new(support::world::TestWorld::new(dir.path()));
    let state_dir = dir.path().join(".fieldbook");
    fieldbook::io::journal::Journal::open(&*world, &state_dir, Via::Cli).expect("open journal");
    let journal: Arc<dyn AiJournal> = Arc::new(fieldbook::io::ai_journal::AiJournalWriter::new(
        Arc::clone(&world),
        state_dir.clone(),
    ));
    assert!(journal.audited(), "a fresh journal can take an append");

    let segment = state_dir.join("audit/journal.jsonl");
    let original = std::fs::metadata(&segment).expect("meta").permissions();
    let mut read_only = original.clone();
    read_only.set_mode(0o400);
    std::fs::set_permissions(&segment, read_only).expect("set ro");

    let audited = journal.audited();
    std::fs::set_permissions(&segment, original).expect("restore perms");
    assert!(
        !audited,
        "a segment that cannot open for append is not audited"
    );
}

#[cfg(unix)]
#[test]
fn a_read_only_journal_lock_is_not_audited() {
    // The append opens `state/lock` read and write before it touches a segment. Probing only the
    // segment reported `audited=true` for a journal whose next append would fail FB0903, so the
    // readiness check asks the writer's lock prerequisite too.
    use fieldbook_core::audit::Via;
    use std::os::unix::fs::PermissionsExt as _;

    let dir = tempfile::tempdir().expect("tempdir");
    let world: Arc<dyn fieldbook::World + Send + Sync> =
        Arc::new(support::world::TestWorld::new(dir.path()));
    let state_dir = dir.path().join(".fieldbook");
    fieldbook::io::journal::Journal::open(&*world, &state_dir, Via::Cli).expect("open journal");
    let journal: Arc<dyn AiJournal> = Arc::new(fieldbook::io::ai_journal::AiJournalWriter::new(
        Arc::clone(&world),
        state_dir.clone(),
    ));
    assert!(journal.audited(), "a fresh journal can take an append");

    let lock = state_dir.join("lock");
    let original = std::fs::metadata(&lock).expect("meta").permissions();
    let mut read_only = original.clone();
    read_only.set_mode(0o400);
    std::fs::set_permissions(&lock, read_only).expect("set ro");

    let audited = journal.audited();
    std::fs::set_permissions(&lock, original).expect("restore perms");
    assert!(
        !audited,
        "a lock that cannot open for writing is not audited"
    );
}

#[test]
fn schema_goldens_match_emitted_json() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/ports/model/schema");
    let pick = Schema::object(vec![Field::required(
        "answer",
        Schema::string_enum(vec!["yes".to_owned(), "no".to_owned()]),
    )]);
    let list = Schema::array(Schema::string(1, 40), 5);
    let nested = Schema::object(vec![Field::required(
        "meta",
        Schema::object(vec![
            Field::optional("note", Schema::boolean()),
            Field::required("count", Schema::integer(0, 10)),
        ]),
    )]);
    for (name, schema) in [("pick", pick), ("list", list), ("nested", nested)] {
        let expected = std::fs::read_to_string(root.join(format!("{name}.json"))).expect("golden");
        let expected: serde_json::Value =
            serde_json::from_str(expected.trim()).expect("golden json");
        let emitted = to_serde(&fieldbook_core::schema_json::to_json_schema(&schema));
        assert_eq!(emitted, expected, "{name}");
    }
}

fn to_serde(value: &JsonValue) -> serde_json::Value {
    match value {
        JsonValue::Null => serde_json::Value::Null,
        JsonValue::Bool(flag) => serde_json::Value::Bool(*flag),
        JsonValue::Integer(number) => serde_json::Value::Number((*number).into()),
        JsonValue::String(text) => serde_json::Value::String(text.clone()),
        JsonValue::Array(items) => serde_json::Value::Array(items.iter().map(to_serde).collect()),
        JsonValue::Object(entries) => serde_json::Value::Object(
            entries
                .iter()
                .map(|(key, item)| (key.clone(), to_serde(item)))
                .collect(),
        ),
    }
}

struct Graph {
    nodes: BTreeMap<ItemRef, LabelNode>,
    ledger: Ledger,
}

impl LabelGraph for Graph {
    fn node(&self, item: &ItemRef) -> Option<LabelNode> {
        self.nodes.get(item).cloned()
    }

    fn ledger(&self, _store: StoreId) -> &Ledger {
        &self.ledger
    }
}

#[test]
fn a_pack_with_a_company_label_is_not_stored_in_the_state_folder() {
    use fieldbook::adapters::model::{CallStore, TicketMeta};
    use fieldbook::io::call_store::DiskCallStore;

    let tmp = tempfile::tempdir().expect("tempdir");
    let state = tmp.path().join(".fieldbook");
    std::fs::create_dir_all(&state).expect("create state dir");
    let store = DiskCallStore::new(fieldbook::io::Disk::with_state(tmp.path(), &state));
    let meta = |labels: LabelSet| TicketMeta {
        response_schema: None,
        pack_seq: 1,
        template: "ask@1".to_owned(),
        labels,
        context_ids: vec!["corpus/S-0001-v1".to_owned()],
        inputs: Vec::new(),
    };
    assert!(
        store
            .put_pack(b"synthetic pack", &meta(np_labels()))
            .is_err()
    );
    assert!(!state.join("handoff").exists(), "nothing was written");
    assert!(
        store
            .put_pack(b"synthetic pack", &meta(LabelSet::public()))
            .is_ok()
    );
}

#[test]
fn an_unreadable_handoff_folder_never_yields_ticket_one() {
    use fieldbook::adapters::model::{CallStore, TicketMeta};
    use fieldbook::io::call_store::DiskCallStore;

    let tmp = tempfile::tempdir().expect("tempdir");
    let state = tmp.path().join(".fieldbook");
    std::fs::create_dir_all(&state).expect("create state dir");
    // A file where the folder belongs makes the folder unreadable (not merely absent).
    std::fs::write(state.join("handoff"), b"not a folder").expect("write blocker");
    let store = DiskCallStore::new(fieldbook::io::Disk::with_state(tmp.path(), &state));
    let meta = TicketMeta {
        response_schema: None,
        pack_seq: 1,
        template: "ask@1".to_owned(),
        labels: LabelSet::public(),
        context_ids: vec!["corpus/S-0001-v1".to_owned()],
        inputs: Vec::new(),
    };
    assert!(store.put_pack(b"synthetic pack", &meta).is_err());
    assert!(
        state.join("handoff").is_file(),
        "nothing replaced the blocker"
    );
}

#[cfg(feature = "test-hooks")]
#[test]
fn a_call_record_id_is_chosen_under_the_write_lease() {
    use fieldbook::adapters::model::{CallRecord, CallStore};
    use fieldbook::io::call_store::DiskCallStore;

    // A record another writer committed between this one's look at the folder and its lease.
    const COMPETING: &[u8] = br#"{
        "seq": 1,
        "adapter": "scripted@1",
        "endpoint": "work",
        "model": "test-model",
        "template": "ask@1",
        "purpose": "ask",
        "labels": ["public"],
        "inputs": [],
        "policy": "synthetic-policy",
        "prompt_sha256": "synthetic prompt",
        "output_sha256": null,
        "system": "synthetic system",
        "prompt": "synthetic prompt",
        "reply": "synthetic reply",
        "tokens_in": 1,
        "tokens_out": 1,
        "attempts": 1,
        "outcome": "ok",
        "keep_days": null,
        "written_on": null
    }"#;

    fn competing_writer(state: &std::path::Path) {
        std::fs::write(state.join("ai/calls/C-0001.json"), COMPETING).expect("competing record");
    }

    let tmp = tempfile::tempdir().expect("tempdir");
    let state = tmp.path().join(".fieldbook");
    std::fs::create_dir_all(state.join("ai/calls")).expect("create calls dir");
    let store = DiskCallStore::new(fieldbook::io::Disk::with_state(tmp.path(), &state));
    store.set_before_write_lease(competing_writer);

    let record = CallRecord {
        adapter: "scripted@1".to_owned(),
        endpoint: "work".to_owned(),
        model: "test-model".to_owned(),
        template: "ask@1".to_owned(),
        purpose: "ask".to_owned(),
        labels: LabelSet::public(),
        inputs: Vec::new(),
        policy: Sha256::digest(b"synthetic-policy"),
        prompt_sha256: Sha256::digest(b"synthetic prompt"),
        output_sha256: Some(Sha256::digest(b"synthetic reply")),
        system: "synthetic system".to_owned(),
        prompt: "synthetic prompt".to_owned(),
        reply: "synthetic reply".to_owned(),
        tokens_in: 1,
        tokens_out: 1,
        attempts: 1,
        outcome: "ok".to_owned(),
        keep_days: 14,
        now: now(),
    };
    store.put_call(&record).expect("put_call succeeds");

    assert_eq!(
        std::fs::read(state.join("ai/calls/C-0001.json")).expect("the competing record"),
        COMPETING,
        "the competing record was replaced: the id was chosen outside the lease"
    );
    assert!(
        state.join("ai/calls/C-0002.json").is_file(),
        "the record was written under the id the other writer had already taken"
    );
}

#[cfg(feature = "test-hooks")]
#[test]
fn a_handoff_ticket_id_is_chosen_under_the_write_lease() {
    use fieldbook::adapters::model::{CallStore, TicketMeta};
    use fieldbook::io::call_store::DiskCallStore;

    // A ticket another writer committed between this one's look at the folder and its lease.
    const COMPETING: &[u8] = b"ticket_version = 1\nid = \"H-0001\"\n";

    fn competing_writer(state: &std::path::Path) {
        std::fs::write(state.join("handoff/H-0001.toml"), COMPETING).expect("competing ticket");
    }

    let tmp = tempfile::tempdir().expect("tempdir");
    let state = tmp.path().join(".fieldbook");
    std::fs::create_dir_all(state.join("handoff")).expect("create handoff dir");
    let store = DiskCallStore::new(fieldbook::io::Disk::with_state(tmp.path(), &state));
    store.set_before_write_lease(competing_writer);

    let meta = TicketMeta {
        response_schema: None,
        pack_seq: 1,
        template: "ask@1".to_owned(),
        labels: LabelSet::public(),
        context_ids: vec!["corpus/S-0001-v1".to_owned()],
        inputs: Vec::new(),
    };
    let ticket = store.put_pack(b"synthetic pack", &meta).expect("put_pack");

    assert_eq!(
        std::fs::read(state.join("handoff/H-0001.toml")).expect("the competing ticket"),
        COMPETING,
        "the competing ticket was replaced: the id was chosen outside the lease"
    );
    assert_eq!(ticket.id.format(), "H-0002");
    assert!(state.join("handoff/H-0002.txt").is_file());
}

#[test]
fn saved_request_rechecks_relabelled_input() {
    use fieldbook::io::call_store::DiskCallStore;

    let tmp = tempfile::tempdir().expect("tempdir");
    let state = tmp.path().join(".fieldbook");
    std::fs::create_dir_all(&state).expect("create state dir");
    let disk = fieldbook::io::Disk::with_state(tmp.path(), &state);
    let store = Arc::new(DiskCallStore::new(disk));

    let public_only_policy_text = POLICY.replace(
        "allow = [\"public\", \"owner-supplied\", \"np-0007\"]",
        "allow = [\"public\"]",
    );
    let public_only_policy = test_policy(&public_only_policy_text, &default_owner());

    let h = Harness::new(
        &public_only_policy_text,
        &default_owner(),
        Shipped::Hours(1),
    );
    let mut deps = h.deps();
    deps.store = store.clone();

    let source_ref = ItemRef::new(StoreId::Corpus, "S-0001-v1");
    let req = request_with_inputs(LabelSet::public(), vec![source_ref.clone()]);

    // 1. Direct model call saves call record and returns ModelAnswer with request_labels + inputs.
    let call_gate = Guarded::new(Box::new(scripted()), work(), deps.clone());
    let call_answer = match run(&call_gate, &req).expect("public call succeeds") {
        ModelOutcome::Answer(answer) => answer,
        _ => panic!("expected answer"),
    };
    assert_eq!(call_answer.request_labels(), &LabelSet::public());
    assert_eq!(call_answer.input_refs(), std::slice::from_ref(&source_ref));

    // 2. Handoff pack + resume over reopened DiskCallStore restores request_labels + inputs.
    let handoff_gate = Guarded::new(Box::new(HandoffAdapter), handoff(), deps);
    let ticket = match run(&handoff_gate, &req).expect("public pack succeeds") {
        ModelOutcome::Deferred(ticket) => ticket,
        _ => panic!("expected deferred ticket"),
    };
    let reopened_store = Arc::new(DiskCallStore::new(fieldbook::io::Disk::with_state(
        tmp.path(),
        &state,
    )));
    let mut reopened_deps = h.deps();
    reopened_deps.store = reopened_store;
    let reopened_handoff = Guarded::new(Box::new(HandoffAdapter), handoff(), reopened_deps);
    let resumed_answer = reopened_handoff
        .resume(&ticket.id.format(), "pasted answer", None)
        .expect("resume succeeds");
    assert_eq!(resumed_answer.request_labels(), &LabelSet::public());
    assert_eq!(
        resumed_answer.input_refs(),
        std::slice::from_ref(&source_ref)
    );

    // 3. Before relabelling S-0001-v1, both answers evaluate to `public` and are allowed to the
    //    old destination; after relabelling ONLY S-0001-v1 to `np-0007`, re-evaluating each
    //    saved answer's inputs refuses the old destination with FB0901.
    let mut initial_nodes = BTreeMap::new();
    initial_nodes.insert(
        source_ref.clone(),
        LabelNode::new(LabelSet::public(), Vec::new()),
    );
    let initial_graph = Graph {
        nodes: initial_nodes,
        ledger: Ledger::default(),
    };

    let mut relabelled_nodes = BTreeMap::new();
    relabelled_nodes.insert(source_ref, LabelNode::new(np_labels(), Vec::new()));
    let relabelled_graph = Graph {
        nodes: relabelled_nodes,
        ledger: Ledger::default(),
    };

    let old_dest = Dest::Model {
        name: "work".to_owned(),
        base_url: base_url().to_owned(),
    };
    for answer in [&call_answer, &resumed_answer] {
        let before = fieldbook_core::asked::resolve(
            answer.request_labels(),
            answer.input_refs(),
            &initial_graph,
        );
        assert_eq!(before, LabelSet::public());
        assert!(
            egress::check(
                &public_only_policy,
                &old_dest,
                Some(&before),
                None,
                now(),
                Shipped::Hours(1),
            )
            .is_allowed()
        );

        let after = fieldbook_core::asked::resolve(
            answer.request_labels(),
            answer.input_refs(),
            &relabelled_graph,
        );
        assert!(after.contains(&np()));
        match egress::check(
            &public_only_policy,
            &old_dest,
            Some(&after),
            None,
            now(),
            Shipped::Hours(1),
        ) {
            Decision::Denied(denial) => assert_eq!(denial.code, CheckCode::Fb0901),
            Decision::Allowed(_) => {
                panic!("relabelled input must be refused under old destination")
            }
        }
    }
}

#[test]
fn reopened_tickets_keep_distinct_schemas() {
    use fieldbook::adapters::model::CallStore;
    use fieldbook::io::call_store::DiskCallStore;

    let tmp = tempfile::tempdir().expect("tempdir");
    let state = tmp.path().join(".fieldbook");
    std::fs::create_dir_all(&state).expect("create state dir");

    // Two distinct schemas on the same template `ask@1`, including non-alphabetical field order
    // (`zeta` before `alpha`), enum strings, and integer bounds.
    let schema_a = Schema::object(vec![
        Field::required(
            "zeta",
            Schema::Str {
                min_len: 2,
                max_len: 3,
                one_of: vec!["yes".to_owned(), "no".to_owned()],
            },
        ),
        Field::required("alpha", Schema::integer(1, 5)),
    ]);
    let schema_b = Schema::object(vec![Field::required("count", Schema::integer(10, 20))]);

    let h = Harness::shipped(Shipped::Hours(1));
    let (ticket_a, ticket_b, ticket_c) = {
        let store = Arc::new(DiskCallStore::new(fieldbook::io::Disk::with_state(
            tmp.path(),
            &state,
        )));
        let mut deps = h.deps();
        deps.store = store;
        let gate = Guarded::new(Box::new(HandoffAdapter), handoff(), deps);

        let req_a = question_with_schema(LabelSet::public(), "Question A", schema_a.clone());
        let req_b = question_with_schema(LabelSet::public(), "Question B", schema_b.clone());
        let req_c = question_with_schema(LabelSet::public(), "Question C", pick_schema());

        let t_a = match run(&gate, &req_a).expect("pack A") {
            ModelOutcome::Deferred(t) => t,
            _ => panic!("expected deferred A"),
        };
        let t_b = match run(&gate, &req_b).expect("pack B") {
            ModelOutcome::Deferred(t) => t,
            _ => panic!("expected deferred B"),
        };
        let t_c = match run(&gate, &req_c).expect("pack C") {
            ModelOutcome::Deferred(t) => t,
            _ => panic!("expected deferred C"),
        };
        (t_a, t_b, t_c)
    };

    // Reopen storage from disk.
    let reopened_store = Arc::new(DiskCallStore::new(fieldbook::io::Disk::with_state(
        tmp.path(),
        &state,
    )));
    let stored_a = reopened_store
        .get_ticket(ticket_a.id)
        .expect("read ticket A")
        .expect("ticket A exists");
    let stored_b = reopened_store
        .get_ticket(ticket_b.id)
        .expect("read ticket B")
        .expect("ticket B exists");
    assert_eq!(
        stored_a.meta.response_schema,
        Some(schema_a),
        "reopened ticket A must preserve exact schema including strings, limits and key order"
    );
    assert_eq!(
        stored_b.meta.response_schema,
        Some(schema_b),
        "reopened ticket B must preserve exact schema"
    );

    let mut reopened_deps = h.deps();
    reopened_deps.store = reopened_store;
    let reopened_gate = Guarded::new(Box::new(HandoffAdapter), handoff(), reopened_deps);

    let reply_a = "{\"zeta\":\"yes\",\"alpha\":3}";
    let reply_b = "{\"count\":15}";
    let reply_current_template = "{\"answer\":\"yes\"}";

    // Ticket A rejects Ticket B's reply and a current-template reply (`pick_schema`), then accepts its own.
    assert_code(
        reopened_gate
            .resume(&ticket_a.id.format(), reply_b, None)
            .map(ModelOutcome::Answer),
        CheckCode::Fb0909,
    );
    assert_code(
        reopened_gate
            .resume(&ticket_a.id.format(), reply_current_template, None)
            .map(ModelOutcome::Answer),
        CheckCode::Fb0909,
    );
    assert!(
        reopened_gate
            .resume(&ticket_a.id.format(), reply_a, None)
            .is_ok()
    );

    // Ticket B rejects Ticket A's reply and a current-template reply, then accepts its own.
    assert_code(
        reopened_gate
            .resume(&ticket_b.id.format(), reply_a, None)
            .map(ModelOutcome::Answer),
        CheckCode::Fb0909,
    );
    assert_code(
        reopened_gate
            .resume(&ticket_b.id.format(), reply_current_template, None)
            .map(ModelOutcome::Answer),
        CheckCode::Fb0909,
    );
    assert!(
        reopened_gate
            .resume(&ticket_b.id.format(), reply_b, None)
            .is_ok()
    );

    // Corrupt Ticket C's schema metadata on disk: resume must fail with FB0916, never treating it
    // as an unconstrained request.
    let ticket_c_path = state.join(format!("handoff/{}.toml", ticket_c.id.format()));
    let orig_c = std::fs::read_to_string(&ticket_c_path).expect("read ticket C");
    let malformed_c = orig_c.replace("type = \"object\"", "type = \"unknown_kind\"");
    assert_ne!(malformed_c, orig_c);
    std::fs::write(&ticket_c_path, malformed_c).expect("write malformed ticket C");
    assert_code(
        reopened_gate
            .resume(&ticket_c.id.format(), "unconstrained text reply", None)
            .map(ModelOutcome::Answer),
        CheckCode::Fb0916,
    );
    assert_code(
        reopened_gate
            .resume(&ticket_c.id.format(), reply_current_template, None)
            .map(ModelOutcome::Answer),
        CheckCode::Fb0916,
    );
}

fn router(h: &Harness, order: &[&str]) -> Router {
    let settings_text = format!(
        "[model]\norder = [{}]\n\n[model.endpoint.work]\nadapter = \"scripted@1\"\nbase_url = \"{}\"\nmodel = \"test-model\"\nallow = [\"public\", \"owner-supplied\", \"np-0007\"]\n",
        order
            .iter()
            .map(|name| format!("\"{name}\""))
            .collect::<Vec<_>>()
            .join(", "),
        base_url()
    );
    let settings = Settings::parse(&settings_text, &[model_row()]).expect("settings");
    Router::from_settings(&settings, &[model_row()], &factories(), h.deps()).expect("router")
}

fn router_run(h: &Harness, order: &[&str], labels: LabelSet, no_net: bool) -> ModelError {
    let mut harness = Harness {
        mem: h.mem.clone(),
        calls: h.calls.clone(),
        pace: h.pace.clone(),
        budget: h.budget.clone(),
        snap: Arc::clone(&h.snap),
        keys: h.keys.clone(),
        http: h.http.clone(),
        avail: h.avail,
        retries: h.retries,
        keep: h.keep,
    };
    if no_net {
        harness.avail.net_built = false;
    }
    let router = router(&harness, order);
    router
        .run(&question(labels, "Synthetic question"), None, &|| false)
        .err()
        .expect("a refusal")
}

fn model_row() -> registry::Row {
    registry::Row {
        port: fieldbook_core::ports::Port::Model,
        id: "scripted@1",
        default: false,
        state: registry::State::Built,
        build: registry::Build::Ai,
        krate: env!("CARGO_PKG_NAME"),
        needs: registry::Needs::Net,
        sees: registry::Sees::Labelled,
        calls_model: true,
        settings_keys: &[],
        fixtures: "tests/fixtures/ports/model/",
        suite: "model_contract",
        doc: "model-scripted1",
    }
}

fn factories() -> Vec<Factory> {
    vec![Factory {
        id: "scripted@1",
        build: |_deps| Box::new(ScriptedModel::new("router answer")),
    }]
}

#[test]
fn lock_is_free_during_a_model_call() {
    use fieldbook::io::ai_journal::AiJournalWriter;
    use fieldbook::io::journal::Journal;
    use fieldbook_core::audit::{Count, EventPayload, Via, Word};

    let dir = tempfile::tempdir().expect("tempdir");
    let world = Arc::new(support::world::TestWorld::new(dir.path()));
    let journal: Arc<dyn AiJournal> = Arc::new(AiJournalWriter::new(
        world.clone(),
        dir.path().join(".fieldbook"),
    ));
    let snap = Arc::new(Mutex::new(PolicySnapshot {
        policy: test_policy(POLICY, &default_owner()),
        now: now(),
        shipped: Shipped::Hours(1),
    }));
    let snap_for_deps = Arc::clone(&snap);
    let deps = Deps {
        journal: journal.clone(),
        keys: Arc::new(ScriptedKeys::new()),
        pace: Arc::new(ScriptedPace::default()),
        policy_provider: Arc::new(move || snap_for_deps.lock().expect("snap").clone()),
        store: Arc::new(MemCalls::new()),
        keep: KeepSettings {
            calls: false,
            days: 0,
        },
        budget: Arc::new(RunBudget::new(10)),
        avail: Avail {
            net_built: true,
            bridge_built: false,
            cpu_ok: true,
        },
        retries: 0,
        http: None,
    };
    let blocking = Arc::new(BlockingModel::new());
    let gate = Guarded::new(Box::new(blocking.clone()), work(), deps);
    let req = question(LabelSet::public(), "Synthetic question");
    let handle = std::thread::spawn(move || gate.run_inner(&req, &|| false));
    blocking.wait_started();

    let state = dir.path().join(".fieldbook");
    fieldbook::io::journal::set_lock_timeout_for_test(Some(std::time::Duration::from_millis(100)));
    let open_res = Journal::open(&*world, &state, Via::Cli);
    fieldbook::io::journal::set_lock_timeout_for_test(None);
    let journal = open_res.expect("open journal while model call is in flight");
    journal
        .append_payload(EventPayload::Ask {
            asked: None,
            asked_via: fieldbook_core::audit::AskedVia::Ask,
            question_sha256: Sha256::digest(b"unrelated"),
            result_count: Count(1),
            outcome: Word::new_static("answered"),
            labels: LabelSet::public(),
        })
        .expect("the journal lock is free during a model call");
    drop(journal);

    blocking.release();
    assert!(handle.join().expect("thread").is_ok());
    assert!(
        fieldbook::io::journal::is_journal_chain_clean(&state),
        "journal hash chain must verify afterai.call is appended"
    );
    let events = fieldbook::io::journal::read_valid_events_prefix(&state);
    let kinds: Vec<&str> = events.iter().map(|e| e.kind().as_str()).collect();
    assert!(kinds.contains(&"ai.begin"), "{kinds:?}");
    assert!(kinds.contains(&"ask"), "{kinds:?}");
    assert!(kinds.contains(&"ai.call"), "{kinds:?}");
    // The transaction id is the real sequence number of the `ai.begin` line, and `ai.call`
    // names it, even though an unrelated writer appended between them.
    let begin = events
        .iter()
        .find_map(|e| match &e.payload {
            EventPayload::AiBegin { tx, .. } => Some((e.seq, tx.0)),
            _ => None,
        })
        .expect("ai.begin event");
    assert_eq!(begin.0, begin.1, "tx must equal the line's own seq");
    assert!(events.iter().any(|e| matches!(
        &e.payload,
        EventPayload::AiCall { tx, .. } if tx.0 == begin.0
    )));
}
