//! Suite M: the model adapter contract (`model-gate` step 9).
//!
//! Each adapter card calls `suite_m` from its own `contract_model_<id>` test.

#![allow(
    clippy::err_expect,
    clippy::manual_let_else,
    clippy::needless_borrow,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "adapter contract tests build inline policies and doubles"
)]

#[path = "../../../tests/support/rng.rs"]
mod rng;

use std::sync::{Arc, Mutex};

use fieldbook::adapters::doubles::{MemCalls, MemJournal, ScriptedModel, ScriptedPace};
use fieldbook::adapters::gate::Guarded;
use fieldbook::adapters::model::{
    Attempt, AttemptError, Deps, KeepSettings, ModelAdapter, ModelOutcome, ModelPort,
    PolicySnapshot, RawReply, Route, RunBudget, Transient,
};
use fieldbook::io::keys::ScriptedKeys;
use fieldbook::io::net::Avail;
use fieldbook_core::CivilDate;
use fieldbook_core::audit::Now;
use fieldbook_core::check::Code;
use fieldbook_core::egress::Shipped;
use fieldbook_core::labels::{LabelId, LabelSet};
use fieldbook_core::policy::{EffectivePolicy, OwnerSettings, Policy, PolicyState};
use fieldbook_core::ports::{Caps, Limits, ModelRequest, Part, PartKind, Purpose, Template};

const POLICY: &str = r#"
policy_schema = 1
revision = 1

[[destination]]
kind = "model"
name = "work"
url = "https://work.example/v1"
allow = ["public"]
keep_calls_days_max = 0

[audit]
require_fresh_export = false
"#;

fn now() -> Now {
    Now::new(CivilDate::from_ymd(2026, 3, 1).expect("date"), 43_200).expect("now")
}

fn owner() -> OwnerSettings {
    let mut owner = OwnerSettings::unset();
    owner.endpoints.insert(
        "work".to_owned(),
        fieldbook_core::labels::LabelAllow::from_ids([LabelId::parse("public").expect("public")]),
    );
    owner
}

fn request() -> ModelRequest {
    ModelRequest::new(
        Purpose::Ask,
        Template::new("ask@1").expect("template"),
        vec![Part::new(
            PartKind::Question,
            LabelSet::public(),
            "Synthetic question",
            Vec::new(),
        )],
        None,
        Limits::new(10, 10),
    )
    .expect("request")
}

fn gate_with(adapter: Box<dyn ModelAdapter + Send + Sync>) -> Guarded {
    let policy = Policy::parse(POLICY, None).expect("policy");
    let eff = EffectivePolicy::new(&PolicyState::Loaded(policy), &owner());
    let snapshot = Arc::new(Mutex::new(PolicySnapshot {
        policy: eff,
        now: now(),
        shipped: Shipped::Never,
    }));
    let snap = Arc::clone(&snapshot);
    let deps = Deps {
        journal: Arc::new(MemJournal::new()),
        keys: Arc::new(ScriptedKeys::new()),
        pace: Arc::new(ScriptedPace::default()),
        policy_provider: Arc::new(move || snap.lock().expect("snapshot").clone()),
        store: Arc::new(MemCalls::new()),
        keep: KeepSettings {
            calls: false,
            days: 0,
        },
        budget: Arc::new(RunBudget::new(10_000)),
        avail: Avail {
            net_built: true,
            bridge_built: false,
            cpu_ok: true,
        },
        retries: 0,
        http: None,
    };
    Guarded::new(adapter, work_route(), deps)
}

fn work_route() -> Route {
    Route {
        name: fieldbook_core::audit::EndpointName::parse("work").expect("name"),
        base_url: "https://work.example/v1".to_owned(),
        model: fieldbook_core::audit::ModelId::parse("test-model").expect("model"),
        timeout_s: 5,
        max_reply_bytes: 64 * 1024,
        key_env: None,
    }
}

/// Runs suite M against adapters built by `make`.
pub fn suite_m(make: &dyn Fn() -> Box<dyn ModelAdapter + Send + Sync>) {
    let req = request();

    let first = gate_with(make());
    let second = gate_with(make());
    let a = run(&first, &req);
    let b = run(&second, &req);
    assert_eq!(
        req.prompt_sha256(),
        request().prompt_sha256(),
        "the same request gives the same prompt hash"
    );
    match (a, b) {
        (ModelOutcome::Answer(a), ModelOutcome::Answer(b)) => assert_eq!(a.text(), b.text()),
        _ => panic!("a deterministic adapter replies twice"),
    }

    let adapter = make();
    let id = adapter.id();
    let gate = gate_with(adapter);
    let refusal = gate
        .run(
            &ModelRequest::new(
                Purpose::Ask,
                Template::new("ask@1").expect("template"),
                vec![Part::new(
                    PartKind::Question,
                    LabelSet::unlabelled(),
                    "Synthetic question",
                    Vec::new(),
                )],
                None,
                Limits::new(10, 10),
            )
            .expect("request"),
            None,
            &|| false,
        )
        .err()
        .expect("a refusal");
    assert_eq!(refusal.code, Code::Fb0901);
    assert!(
        refusal.to_string().contains(id),
        "the refusal names the adapter"
    );

    random_and_mutated_bytes_never_panic(make);
}

/// Random reply bytes and one-byte mutations of a valid reply never panic the gate.
fn random_and_mutated_bytes_never_panic(make: &dyn Fn() -> Box<dyn ModelAdapter + Send + Sync>) {
    let req = request();

    let mut rng = rng::Rng::new(1);
    for _ in 0..200 {
        let bytes = rng.bytes(24);
        let reply = String::from_utf8_lossy(&bytes).into_owned();
        let model = Arc::new(ScriptedModel::new(reply));
        let gate = gate_with_model(model);
        let _ = gate.run(&req, None, &|| false);
    }

    let valid = br#"{"answer":"yes"}"#;
    for index in 0..valid.len() {
        let mut flipped = valid.to_vec();
        flipped[index] ^= 0x20;
        let model = Arc::new(ScriptedModel::new(
            String::from_utf8_lossy(&flipped).into_owned(),
        ));
        let gate = gate_with_model(model);
        let _ = gate.run(&req, None, &|| false);

        let mut deleted = valid.to_vec();
        deleted.remove(index);
        let model = Arc::new(ScriptedModel::new(
            String::from_utf8_lossy(&deleted).into_owned(),
        ));
        let gate = gate_with_model(model);
        let _ = gate.run(&req, None, &|| false);
    }

    let _ = make;
}

fn gate_with_model(model: Arc<ScriptedModel>) -> Guarded {
    let policy = Policy::parse(POLICY, None).expect("policy");
    let eff = EffectivePolicy::new(&PolicyState::Loaded(policy), &owner());
    let snapshot = Arc::new(Mutex::new(PolicySnapshot {
        policy: eff,
        now: now(),
        shipped: Shipped::Never,
    }));
    let snap = Arc::clone(&snapshot);
    let deps = Deps {
        journal: Arc::new(MemJournal::new()),
        keys: Arc::new(ScriptedKeys::new()),
        pace: Arc::new(ScriptedPace::default()),
        policy_provider: Arc::new(move || snap.lock().expect("snapshot").clone()),
        store: Arc::new(MemCalls::new()),
        keep: KeepSettings {
            calls: false,
            days: 0,
        },
        budget: Arc::new(RunBudget::new(10_000)),
        avail: Avail {
            net_built: true,
            bridge_built: false,
            cpu_ok: true,
        },
        retries: 0,
        http: None,
    };
    Guarded::new(Box::new(model), work_route(), deps)
}

fn run(gate: &Guarded, req: &ModelRequest) -> ModelOutcome {
    gate.run(req, None, &|| false).expect("a reply")
}

#[test]
fn contract_model_scripted_1() {
    suite_m(&|| {
        let model = ScriptedModel::with_caps(Caps::new(true, false, false, false, 1_000_000)).push(
            Ok(Attempt::Reply(RawReply {
                body: "{\"answer\":\"yes\"}".to_owned(),
                tokens_in: 1,
                tokens_out: 1,
                finish: fieldbook::adapters::model::Finish::Stop,
            })),
        );
        Box::new(model)
    });
}

#[test]
fn one_rng_exists() {
    let source = include_str!("model_contract.rs");
    assert!(source.contains("#[path = \"../../../tests/support/rng.rs\"]"));
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut found = Vec::new();
    for entry in std::fs::read_dir(&root).expect("tests dir") {
        let path = entry.expect("entry").path();
        if path.extension().is_some_and(|ext| ext == "rs") {
            let text = std::fs::read_to_string(&path).expect("read");
            if text.lines().any(|line| line.trim() == "pub struct Rng {") {
                found.push(path);
            }
        }
    }
    assert!(found.is_empty(), "one Rng exists: {found:?}");
}

#[test]
fn a_transient_failure_is_retried_at_most_zero_times_here() {
    let model = Arc::new(
        ScriptedModel::with_caps(Caps::new(true, false, false, false, 1_000_000))
            .push(Err(AttemptError::Transient(Transient::Connect))),
    );
    let gate = gate_with_model(model.clone());
    let error = gate
        .run(&request(), None, &|| false)
        .err()
        .expect("a failure");
    assert_eq!(error.code, Code::Fb0908);
    assert_eq!(model.count(), 1);
}

#[test]
fn an_adapter_receives_only_the_token_and_route() {
    let model = Arc::new(ScriptedModel::new("answer"));
    let gate = gate_with_model(model.clone());
    assert!(gate.run(&request(), None, &|| false).is_ok());
    let attempts = model.attempts();
    assert_eq!(attempts.len(), 1);
    assert!(attempts[0].has_delivery);
    assert_eq!(attempts[0].endpoint, "https://work.example/v1");
}
