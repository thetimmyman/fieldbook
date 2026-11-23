//! Feature-enabled (`test-hooks`) integration tests for `fieldbook ai status [--json]`
//! (`ai-config-and-keys` steps 6, 8, 9, 11, 12).

#![cfg(feature = "test-hooks")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "integration tests prepare fixture state/policy copies and inspect files"
)]

#[path = "support/policy.rs"]
mod policy_support;
mod support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use fieldbook::adapters::registry::{ADAPTERS, Build, Needs, Port, Row, Sees, State};
use fieldbook::ai_status::{read_last_calls, run_status};
use fieldbook::cli::AiStatusArgs;
use fieldbook::io::keys::ScriptedKeys;
use fieldbook::io::policy::PolicySource;
use fieldbook::policy::{PolicyCtx, resolve_with_source};
use fieldbook_core::CivilDate;
use fieldbook_core::audit::{
    Count, DestKind, EndpointName, EventPayload, LabelSet, ModelId, Now, RecordRef, Sha256, Word,
};
use fieldbook_core::check::Code;
use fieldbook_core::labels::{LabelAllow, LabelId, PUBLIC};
use fieldbook_core::policy::{ClosedWhy, PolicyState};
use fieldbook_core::ports::{ConnState, KeyState};
use fieldbook_core::report::AiStatus;

use policy_support::write_policy;
use support::state::build_state;
use support::world::TestWorld;

const AI_STATUS_POLICY: &str = include_str!("../../../tests/fixtures/policy/ai-status.toml");

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn copy_dir_all(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("create dst dir");
    for entry in std::fs::read_dir(src).expect("read src dir") {
        let entry = entry.expect("dir entry");
        let dst_path = dst.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_dir_all(&entry.path(), &dst_path);
        } else {
            std::fs::copy(entry.path(), &dst_path).expect("copy file");
        }
    }
}

fn snapshot_tree_bytes(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(base: &Path, cur: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        let Ok(entries) = std::fs::read_dir(cur) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(base, &path, out);
            } else if path.is_file() {
                let rel = path
                    .strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, std::fs::read(&path).unwrap());
            }
        }
    }
    let mut map = BTreeMap::new();
    walk(dir, dir, &mut map);
    map
}

fn sha(byte: u8) -> Sha256 {
    let hex = format!("{byte:02x}").repeat(32);
    Sha256::parse(&hex).expect("valid synthetic sha256")
}

/// The paired `ai.begin` / `ai.call` events that generate `tests/fixtures/state-ai/audit/journal.jsonl`.
///
/// Both endpoints (`handoff` and `work`) share `["public"]` labels but carry different outcomes:
/// `handoff` is `ok` (1 attempt) and `work` is `failed` with `FB0908` (2 attempts).
fn state_ai_events() -> Vec<EventPayload> {
    let pub_labels = LabelSet::public();
    let policy_hash = sha(0xaa);
    let c1 = RecordRef::parse("corpus/CL-CMP-0001").expect("valid ref");

    vec![
        // seq 3: ai.begin for handoff
        EventPayload::AiBegin {
            tx: Count::new(3),
            purpose: Word::new_static("ask"),
            dest: DestKind::Handoff,
            labels: pub_labels.clone(),
            policy: policy_hash.clone(),
            adapter: Word::new_static("handoff@1"),
            endpoint: EndpointName::parse("handoff").expect("handoff"),
            model: ModelId::parse("handoff-browser").expect("model id"),
            template: Word::new_static("ask@1"),
            prompt_sha256: sha(0x11),
            context_ids: vec![c1.clone()],
        },
        // seq 4: ai.call for handoff -> ok
        EventPayload::AiCall {
            tx: Count::new(3),
            outcome: Word::new_static("ok"),
            code: None,
            attempts: Count::new(1),
            output_sha256: Some(sha(0x12)),
            tokens_in: Count::new(120),
            tokens_out: Count::new(45),
        },
        // seq 5: ai.begin for work (same labels, different endpoint & outcome)
        EventPayload::AiBegin {
            tx: Count::new(5),
            purpose: Word::new_static("ask"),
            dest: DestKind::Model,
            labels: pub_labels,
            policy: policy_hash,
            adapter: Word::new_static("openai-compatible@1"),
            endpoint: EndpointName::parse("work").expect("work"),
            model: ModelId::parse("synthetic-model-1").expect("model id"),
            template: Word::new_static("ask@1"),
            prompt_sha256: sha(0x21),
            context_ids: vec![c1],
        },
        // seq 6: ai.call for work -> failed (FB0908)
        EventPayload::AiCall {
            tx: Count::new(5),
            outcome: Word::new_static("failed"),
            code: Some(Code::Fb0908),
            attempts: Count::new(2),
            output_sha256: None,
            tokens_in: Count::new(80),
            tokens_out: Count::new(0),
        },
    ]
}

fn rows_with_model(include_handoff: bool) -> Vec<Row> {
    let mut rows = ADAPTERS.to_vec();
    if include_handoff {
        rows.push(Row {
            port: Port::Model,
            id: "handoff@1",
            default: true,
            state: State::Built,
            build: Build::Silent,
            krate: env!("CARGO_PKG_NAME"),
            needs: Needs::None,
            sees: Sees::Labelled,
            calls_model: false,
            settings_keys: &[],
            fixtures: "tests/fixtures/ports/model/handoff@1/",
            suite: "model_suite",
            doc: "model-handoff1",
        });
    }
    rows.push(Row {
        port: Port::Model,
        id: "openai-compatible@1",
        default: false,
        state: State::Built,
        build: Build::Ai,
        krate: env!("CARGO_PKG_NAME"),
        needs: Needs::Net,
        sees: Sees::Labelled,
        calls_model: true,
        settings_keys: &[],
        fixtures: "tests/fixtures/ports/model/openai-compatible@1/",
        suite: "model_suite",
        doc: "model-openai-compatible1",
    });
    rows
}

fn setup_temp_env() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
    let root = repo_root();
    let tmp = tempfile::tempdir().expect("tempdir");
    let corpus = tmp.path().join("corpus-copy");
    let state = tmp.path().join("state-ai");
    let policy_dir = tmp.path().join("policy-root");
    copy_dir_all(&root.join("tests/fixtures/corpus-mini"), &corpus);
    copy_dir_all(&root.join("tests/fixtures/state-ai"), &state);
    (tmp, corpus, state, policy_dir)
}

fn load_positive_policy(
    world: &TestWorld,
    state: &Path,
    corpus: &Path,
    policy_dir: &Path,
) -> (PathBuf, PolicyCtx) {
    let policy_path = write_policy(policy_dir, "ai-status.toml", AI_STATUS_POLICY);
    let source = PolicySource::discover_with_flags(world, false, Some(&policy_path), None);
    let ctx = resolve_with_source(world, state, corpus, false, source)
        .expect("positive policy must resolve without FB0414");
    assert!(
        matches!(ctx.state, PolicyState::Loaded(_)),
        "positive policy must assert Loaded before testing behavior, got {:?}",
        ctx.state
    );
    (policy_path, ctx)
}

#[test]
fn state_ai_matches_the_writer() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let built_dir = tmp.path().join("state-ai");
    build_state(&built_dir, &state_ai_events());

    let expected = std::fs::read_to_string(built_dir.join("audit/journal.jsonl"))
        .expect("read freshly built journal.jsonl");
    let checked_in =
        std::fs::read_to_string(repo_root().join("tests/fixtures/state-ai/audit/journal.jsonl"))
            .expect("read checked-in state-ai/audit/journal.jsonl");
    assert_eq!(
        checked_in, expected,
        "tests/fixtures/state-ai/audit/journal.jsonl differs from real writer output"
    );
}

#[test]
fn shows_handoff_as_the_default() {
    let (_tmp, corpus, state, _policy_dir) = setup_temp_env();
    let world = TestWorld::with_state(&corpus, &state);

    // 1. Production ADAPTERS (no Model adapter built yet): shows configured order only,
    //    and reports no unbuilt Model adapter as usable.
    let out = fieldbook::run(
        &[
            "ai".to_owned(),
            "status".to_owned(),
            "--state".to_owned(),
            state.to_string_lossy().into_owned(),
        ],
        &world,
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(out.stdout.contains("order: handoff\n"), "{}", out.stdout);
    assert!(
        !out.stdout.contains("model handoff@1"),
        "production status must not report unbuilt handoff@1 as usable: {}",
        out.stdout
    );

    // 2. Injected Built handoff@1 row: shows Handoff state with effective allow set.
    let ctx = resolve_with_source(
        &world,
        &state,
        &corpus,
        false,
        PolicySource::discover(&world, false),
    )
    .expect("floor policy");
    let rows = rows_with_model(true);
    let out_json = run_status(
        &AiStatusArgs { json: true },
        &ctx,
        &state,
        &world,
        &rows,
        &ScriptedKeys::new(),
        false,
    );
    assert_eq!(out_json.code, 0);
    let parsed: AiStatus = serde_json::from_str(&out_json.stdout).expect("parse AiStatus");
    let handoff = parsed
        .adapters
        .iter()
        .find(|a| a.port == "model" && a.adapter == "handoff@1")
        .expect("handoff@1 entry");
    assert_eq!(handoff.endpoint.as_deref(), Some("handoff"));
    assert_eq!(handoff.state, ConnState::Handoff);
    assert_eq!(handoff.key, KeyState::NotNeeded);
}

#[test]
fn shows_key_state_without_the_value() {
    let (_tmp, corpus, state, policy_dir) = setup_temp_env();
    let settings_text = "\
role = \"lead\"

[model]
order = [\"work\", \"handoff\"]

[model.endpoint.work]
adapter = \"openai-compatible@1\"
base_url = \"https://models.example.test/v1\"
model = \"synthetic-model-1\"
allow = [\"public\"]
key_env = \"SYNTHETIC_MODEL_KEY\"
";
    std::fs::write(state.join("settings.toml"), settings_text).expect("write settings");
    let world = TestWorld::with_state(&corpus, &state);
    let (_policy_path, ctx) = load_positive_policy(&world, &state, &corpus, &policy_dir);
    let rows = rows_with_model(true);

    let secret = "FBTEST-SyntheticKeyValueNeverPrinted99";
    let keys_set = ScriptedKeys::new().with_key("SYNTHETIC_MODEL_KEY", secret);

    let text_set = run_status(
        &AiStatusArgs { json: false },
        &ctx,
        &state,
        &world,
        &rows,
        &keys_set,
        true,
    );
    assert_eq!(text_set.code, 0);
    assert!(
        text_set.stdout.contains(&format!(
            "SYNTHETIC_MODEL_KEY: set ({} characters)",
            secret.len()
        )),
        "stdout: {}",
        text_set.stdout
    );
    assert!(!text_set.stdout.contains(secret));
    assert!(!text_set.stderr.contains(secret));

    let json_set = run_status(
        &AiStatusArgs { json: true },
        &ctx,
        &state,
        &world,
        &rows,
        &keys_set,
        true,
    );
    assert_eq!(json_set.code, 0);
    assert!(!json_set.stdout.contains(secret));
    let parsed_set: AiStatus = serde_json::from_str(&json_set.stdout).expect("parse json");
    let work = parsed_set
        .adapters
        .iter()
        .find(|a| a.endpoint.as_deref() == Some("work"))
        .expect("work adapter");
    assert_eq!(work.key, KeyState::Set);
    assert_eq!(work.state, ConnState::Ready);

    // Missing key -> Missing (and never fails).
    let text_missing = run_status(
        &AiStatusArgs { json: false },
        &ctx,
        &state,
        &world,
        &rows,
        &ScriptedKeys::new(),
        true,
    );
    assert_eq!(text_missing.code, 0);
    assert!(
        text_missing.stdout.contains("SYNTHETIC_MODEL_KEY: missing"),
        "stdout: {}",
        text_missing.stdout
    );

    // Also verify Unavailable { FB0912 } when `has_net` is false for a `Needs::Net` adapter.
    let json_no_net = run_status(
        &AiStatusArgs { json: true },
        &ctx,
        &state,
        &world,
        &rows,
        &keys_set,
        false,
    );
    let parsed_no_net: AiStatus = serde_json::from_str(&json_no_net.stdout).expect("parse json");
    let work_no_net = parsed_no_net
        .adapters
        .iter()
        .find(|a| a.endpoint.as_deref() == Some("work"))
        .expect("work adapter");
    assert_eq!(
        work_no_net.state,
        ConnState::Unavailable {
            code: "FB0912".to_owned()
        }
    );
}

#[test]
fn shows_the_last_call() {
    let (_tmp, corpus, state, policy_dir) = setup_temp_env();
    let settings_text = "\
role = \"lead\"

[model]
order = [\"work\", \"handoff\"]

[model.endpoint.work]
adapter = \"openai-compatible@1\"
base_url = \"https://models.example.test/v1\"
model = \"synthetic-model-1\"
allow = [\"public\"]
";
    std::fs::write(state.join("settings.toml"), settings_text).expect("write settings");
    let world = TestWorld::with_state(&corpus, &state);
    let (_policy_path, ctx) = load_positive_policy(&world, &state, &corpus, &policy_dir);
    let rows = rows_with_model(true);

    // Fixture journal has paired events for two endpoints (`handoff` ok, `work` failed) sharing `public`.
    let out = run_status(
        &AiStatusArgs { json: true },
        &ctx,
        &state,
        &world,
        &rows,
        &ScriptedKeys::new(),
        true,
    );
    assert_eq!(out.code, 0);
    let parsed: AiStatus = serde_json::from_str(&out.stdout).expect("parse json");

    let handoff = parsed
        .adapters
        .iter()
        .find(|a| a.endpoint.as_deref() == Some("handoff"))
        .expect("handoff");
    let handoff_last = handoff.last.as_ref().expect("handoff last call");
    assert_eq!(handoff_last.outcome, "ok");
    assert_eq!(handoff_last.attempts, 1);

    let work = parsed
        .adapters
        .iter()
        .find(|a| a.endpoint.as_deref() == Some("work"))
        .expect("work");
    let work_last = work.last.as_ref().expect("work last call");
    assert_eq!(work_last.outcome, "failed");
    assert_eq!(work_last.attempts, 2);

    // Unmatched `ai.begin` produces no last call; later completion sequence wins per endpoint.
    let tmp_extra = tempfile::tempdir().expect("tempdir extra");
    let extra_state = tmp_extra.path().join("state");
    let mut events = state_ai_events();
    let c1 = RecordRef::parse("corpus/CL-CMP-0001").expect("valid ref");
    // seq 7: unmatched ai.begin for `orphan`
    events.push(EventPayload::AiBegin {
        tx: Count::new(7),
        purpose: Word::new_static("ask"),
        dest: DestKind::Model,
        labels: LabelSet::public(),
        policy: sha(0xaa),
        adapter: Word::new_static("openai-compatible@1"),
        endpoint: EndpointName::parse("orphan").expect("orphan"),
        model: ModelId::parse("synthetic-model-1").expect("model"),
        template: Word::new_static("ask@1"),
        prompt_sha256: sha(0x31),
        context_ids: vec![c1.clone()],
    });
    // seq 8 + seq 9: newer completed call for `work` with outcome `ok`, attempts 1
    events.push(EventPayload::AiBegin {
        tx: Count::new(8),
        purpose: Word::new_static("ask"),
        dest: DestKind::Model,
        labels: LabelSet::public(),
        policy: sha(0xaa),
        adapter: Word::new_static("openai-compatible@1"),
        endpoint: EndpointName::parse("work").expect("work"),
        model: ModelId::parse("synthetic-model-1").expect("model"),
        template: Word::new_static("ask@1"),
        prompt_sha256: sha(0x32),
        context_ids: vec![c1],
    });
    events.push(EventPayload::AiCall {
        tx: Count::new(8),
        outcome: Word::new_static("ok"),
        code: None,
        attempts: Count::new(1),
        output_sha256: Some(sha(0x33)),
        tokens_in: Count::new(50),
        tokens_out: Count::new(20),
    });
    build_state(&extra_state, &events);
    let last_map = read_last_calls(&extra_state);
    assert!(
        !last_map.contains_key("orphan"),
        "unmatched ai.begin must have no last-call entry"
    );
    assert_eq!(last_map.get("work").expect("work").outcome, "ok");
    assert_eq!(last_map.get("handoff").expect("handoff").outcome, "ok");
}

#[test]
fn shows_the_policy_state() {
    let (_tmp, corpus, state, policy_dir) = setup_temp_env();
    let world = TestWorld::with_state(&corpus, &state);

    // 1. Floor
    let out_floor = fieldbook::run(
        &[
            "ai".to_owned(),
            "status".to_owned(),
            "--state".to_owned(),
            state.to_string_lossy().into_owned(),
        ],
        &world,
    );
    assert_eq!(out_floor.code, 0);
    assert!(
        out_floor.stdout.contains("policy: none (floor)\n"),
        "{}",
        out_floor.stdout
    );

    // 2. Loaded (via write_policy outside state & corpus)
    let (policy_path, _ctx) = load_positive_policy(&world, &state, &corpus, &policy_dir);
    let out_loaded = fieldbook::run(
        &[
            "ai".to_owned(),
            "status".to_owned(),
            "--state".to_owned(),
            state.to_string_lossy().into_owned(),
            "--policy".to_owned(),
            policy_path.to_string_lossy().into_owned(),
        ],
        &world,
    );
    assert_eq!(out_loaded.code, 0, "stderr: {}", out_loaded.stderr);
    assert!(
        out_loaded.stdout.contains("policy: revision 1\n"),
        "{}",
        out_loaded.stdout
    );

    // 3. Closed (invalid policy file -> Closed(Invalid))
    let bad_policy = policy_dir.join("bad-policy.toml");
    std::fs::write(&bad_policy, "policy_schema = 99\n").expect("write bad policy");
    let out_closed = fieldbook::run(
        &[
            "ai".to_owned(),
            "status".to_owned(),
            "--state".to_owned(),
            state.to_string_lossy().into_owned(),
            "--policy".to_owned(),
            bad_policy.to_string_lossy().into_owned(),
        ],
        &world,
    );
    assert_eq!(out_closed.code, 0, "stderr: {}", out_closed.stderr);
    assert!(
        out_closed.stdout.contains("policy: closed (invalid)\n"),
        "{}",
        out_closed.stdout
    );
}

#[test]
fn shows_the_limits_in_force() {
    let (_tmp, corpus, state, policy_dir) = setup_temp_env();
    let world = TestWorld::with_state(&corpus, &state);

    // Defaults
    let out_default = fieldbook::run(
        &[
            "ai".to_owned(),
            "status".to_owned(),
            "--state".to_owned(),
            state.to_string_lossy().into_owned(),
        ],
        &world,
    );
    assert_eq!(out_default.code, 0);
    assert!(
        out_default
            .stdout
            .contains("Limits: retries 2, max_calls_per_run 100, max_out_tokens 1500\n"),
        "{}",
        out_default.stdout
    );

    // Custom retries = 3 and endpoint work timeout_s = 90
    let custom_settings = "\
role = \"lead\"

[model]
order = [\"work\", \"handoff\"]
retries = 3

[model.endpoint.work]
adapter = \"openai-compatible@1\"
base_url = \"https://models.example.test/v1\"
model = \"synthetic-model-1\"
allow = [\"public\"]
timeout_s = 90
";
    std::fs::write(state.join("settings.toml"), custom_settings).expect("write custom settings");
    let (_policy_path, ctx) = load_positive_policy(&world, &state, &corpus, &policy_dir);
    let rows = rows_with_model(true);

    let out_custom = run_status(
        &AiStatusArgs { json: false },
        &ctx,
        &state,
        &world,
        &rows,
        &ScriptedKeys::new(),
        true,
    );
    assert_eq!(out_custom.code, 0);
    assert!(
        out_custom.stdout.contains(
            "Limits: retries 3, max_calls_per_run 100, max_out_tokens 1500, timeout_s work 90\n"
        ),
        "{}",
        out_custom.stdout
    );

    let out_json = run_status(
        &AiStatusArgs { json: true },
        &ctx,
        &state,
        &world,
        &rows,
        &ScriptedKeys::new(),
        true,
    );
    assert_eq!(out_json.code, 0);
    let val: serde_json::Value = serde_json::from_str(&out_json.stdout).expect("parse json");
    assert!(
        val.get("limits").is_none(),
        "ai status --json must have no `limits` key: {}",
        out_json.stdout
    );
}

#[test]
fn shows_both_sets_and_the_side_that_cut_each_label() {
    let (_tmp, corpus, state, policy_dir) = setup_temp_env();
    let settings_text = "\
role = \"lead\"

[model]
order = [\"work\", \"handoff\"]

[model.endpoint.work]
adapter = \"openai-compatible@1\"
base_url = \"https://models.example.test/v1\"
model = \"synthetic-model-1\"
allow = [\"public\"]
";
    std::fs::write(state.join("settings.toml"), settings_text).expect("write settings");
    let world = TestWorld::with_state(&corpus, &state);
    let (_policy_path, ctx) = load_positive_policy(&world, &state, &corpus, &policy_dir);
    let rows = rows_with_model(true);

    let out = run_status(
        &AiStatusArgs { json: false },
        &ctx,
        &state,
        &world,
        &rows,
        &ScriptedKeys::new(),
        true,
    );
    assert_eq!(out.code, 0);
    // Policy allows [np-0007, public], owner allows [public] -> np-0007 cut by owner, owner-supplied cut by policy.
    assert!(
        out.stdout
            .contains("allow: policy [np-0007,public] owner [public]"),
        "expected both sets in output:\n{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("cut np-0007: by owner"),
        "expected np-0007 cut by owner:\n{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("cut owner-supplied: by policy"),
        "expected owner-supplied cut by policy:\n{}",
        out.stdout
    );
}

#[test]
fn an_undefined_label_in_allow_is_a_fb0411_warning() {
    let (_tmp, corpus, state, policy_dir) = setup_temp_env();
    let settings_text = "\
role = \"lead\"

[model]
order = [\"handoff\"]

[model.handoff]
allow = [\"public\", \"zz-9999\"]
";
    std::fs::write(state.join("settings.toml"), settings_text).expect("write settings");
    let world = TestWorld::with_state(&corpus, &state);
    let (_policy_path, ctx) = load_positive_policy(&world, &state, &corpus, &policy_dir);
    let rows = rows_with_model(true);

    let out = run_status(
        &AiStatusArgs { json: false },
        &ctx,
        &state,
        &world,
        &rows,
        &ScriptedKeys::new(),
        true,
    );
    assert_eq!(out.code, 0);
    assert!(
        out.stdout.contains("FB0411") && out.stdout.contains("zz-9999"),
        "expected FB0411 warning naming zz-9999:\n{}",
        out.stdout
    );
}

#[test]
fn an_endpoint_with_no_row_shows_blocked_fb0918() {
    let (_tmp, corpus, state, policy_dir) = setup_temp_env();
    let world = TestWorld::with_state(&corpus, &state);
    let (_policy_path, ctx) = load_positive_policy(&world, &state, &corpus, &policy_dir);
    let rows = rows_with_model(true);

    // 1. Endpoint with no policy row -> Blocked { FB0918 }.
    let no_row_settings = "\
role = \"lead\"

[model]
order = [\"unlisted\", \"handoff\"]

[model.endpoint.unlisted]
adapter = \"openai-compatible@1\"
base_url = \"https://models.example.test/v1\"
model = \"synthetic-model-1\"
allow = [\"public\"]
";
    std::fs::write(state.join("settings.toml"), no_row_settings).expect("write settings");
    let out = run_status(
        &AiStatusArgs { json: true },
        &ctx,
        &state,
        &world,
        &rows,
        &ScriptedKeys::new(),
        true,
    );
    assert_eq!(out.code, 0);
    let parsed: AiStatus = serde_json::from_str(&out.stdout).expect("parse json");
    let unlisted = parsed
        .adapters
        .iter()
        .find(|a| a.endpoint.as_deref() == Some("unlisted"))
        .expect("unlisted");
    assert_eq!(
        unlisted.state,
        ConnState::Blocked {
            code: "FB0918".to_owned()
        }
    );

    // 2. Endpoint with mismatched URL -> Blocked { FB0918 }.
    let wrong_url_settings = "\
role = \"lead\"

[model]
order = [\"work\"]

[model.endpoint.work]
adapter = \"openai-compatible@1\"
base_url = \"https://other-host.example.test/v1\"
model = \"synthetic-model-1\"
allow = [\"public\"]
";
    std::fs::write(state.join("settings.toml"), wrong_url_settings).expect("write settings");
    let out_url = run_status(
        &AiStatusArgs { json: true },
        &ctx,
        &state,
        &world,
        &rows,
        &ScriptedKeys::new(),
        true,
    );
    let parsed_url: AiStatus = serde_json::from_str(&out_url.stdout).expect("parse json");
    let work_url = parsed_url
        .adapters
        .iter()
        .find(|a| a.endpoint.as_deref() == Some("work"))
        .expect("work");
    assert_eq!(
        work_url.state,
        ConnState::Blocked {
            code: "FB0918".to_owned()
        }
    );

    // 3. Endpoint whose policy row expired (`expires = 2099-12-31`) when `world.now()` is past expiry -> Blocked { FB0918 }.
    let matching_url_settings = "\
role = \"lead\"

[model]
order = [\"work\"]

[model.endpoint.work]
adapter = \"openai-compatible@1\"
base_url = \"https://models.example.test/v1\"
model = \"synthetic-model-1\"
allow = [\"public\"]
";
    std::fs::write(state.join("settings.toml"), matching_url_settings).expect("write settings");
    let past_expiry = Now::new(CivilDate::from_ymd(2100, 1, 1).expect("date"), 0).expect("now");
    world.set_now(past_expiry);
    let out_exp = run_status(
        &AiStatusArgs { json: true },
        &ctx,
        &state,
        &world,
        &rows,
        &ScriptedKeys::new(),
        true,
    );
    let parsed_exp: AiStatus = serde_json::from_str(&out_exp.stdout).expect("parse json");
    let work_exp = parsed_exp
        .adapters
        .iter()
        .find(|a| a.endpoint.as_deref() == Some("work"))
        .expect("work");
    assert_eq!(
        work_exp.state,
        ConnState::Blocked {
            code: "FB0918".to_owned()
        }
    );
}

#[test]
fn a_closed_policy_shows_blocked_fb0413() {
    let (_tmp, corpus, state, _policy_dir) = setup_temp_env();
    let world = TestWorld::with_state(&corpus, &state);
    let closed_ctx = PolicyCtx::closed(ClosedWhy::LowerRevision);
    let rows = rows_with_model(true);

    let out = run_status(
        &AiStatusArgs { json: true },
        &closed_ctx,
        &state,
        &world,
        &rows,
        &ScriptedKeys::new(),
        true,
    );
    assert_eq!(out.code, 0);
    let parsed: AiStatus = serde_json::from_str(&out.stdout).expect("parse json");
    assert_eq!(parsed.policy, "closed");
    assert!(!parsed.adapters.is_empty());
    for adapter in &parsed.adapters {
        assert_eq!(
            adapter.state,
            ConnState::Blocked {
                code: "FB0413".to_owned()
            },
            "every adapter must be Blocked(FB0413) when policy is closed: {adapter:?}"
        );
        assert!(adapter.allow.is_empty());
    }
}

#[test]
fn an_owner_allow_list_narrows_the_effective_policy() {
    let (_tmp, corpus, state, policy_dir) = setup_temp_env();
    let world = TestWorld::with_state(&corpus, &state);
    let (_policy_path, ctx) = load_positive_policy(&world, &state, &corpus, &policy_dir);
    let rows = rows_with_model(true);

    // 1. Settings file whose `allow` for `work` omits `np-0007` (which the policy row allows):
    //    `ai status --json` shows `allow: ["public"]` and text shows `np-0007` cut by owner.
    let narrowed = "\
role = \"lead\"

[model]
order = [\"work\", \"handoff\"]

[model.handoff]
allow = [\"public\"]

[model.endpoint.work]
adapter = \"openai-compatible@1\"
base_url = \"https://models.example.test/v1\"
model = \"synthetic-model-1\"
allow = [\"public\"]
";
    std::fs::write(state.join("settings.toml"), narrowed).expect("write narrowed settings");

    let out_json = run_status(
        &AiStatusArgs { json: true },
        &ctx,
        &state,
        &world,
        &rows,
        &ScriptedKeys::new(),
        true,
    );
    assert_eq!(out_json.code, 0);
    let parsed: AiStatus = serde_json::from_str(&out_json.stdout).expect("parse json");
    let work = parsed
        .adapters
        .iter()
        .find(|a| a.endpoint.as_deref() == Some("work"))
        .expect("work");
    assert_eq!(work.allow, vec![PUBLIC]);
    assert_eq!(work.state, ConnState::Ready);

    let out_text = run_status(
        &AiStatusArgs { json: false },
        &ctx,
        &state,
        &world,
        &rows,
        &ScriptedKeys::new(),
        true,
    );
    assert!(
        out_text.stdout.contains("cut np-0007: by owner"),
        "expected np-0007 cut by owner:\n{}",
        out_text.stdout
    );

    // 2. Same file with no `allow` on `work`: endpoint accepts nothing (`allow: []`, Blocked FB0901).
    let no_allow = "\
role = \"lead\"

[model]
order = [\"work\", \"handoff\"]

[model.endpoint.work]
adapter = \"openai-compatible@1\"
base_url = \"https://models.example.test/v1\"
model = \"synthetic-model-1\"
";
    std::fs::write(state.join("settings.toml"), no_allow).expect("write no_allow settings");

    let out_none = run_status(
        &AiStatusArgs { json: true },
        &ctx,
        &state,
        &world,
        &rows,
        &ScriptedKeys::new(),
        true,
    );
    assert_eq!(out_none.code, 0);
    let parsed_none: AiStatus = serde_json::from_str(&out_none.stdout).expect("parse json");
    let work_none = parsed_none
        .adapters
        .iter()
        .find(|a| a.endpoint.as_deref() == Some("work"))
        .expect("work");
    assert!(work_none.allow.is_empty());
    assert_eq!(
        work_none.state,
        ConnState::Blocked {
            code: "FB0901".to_owned()
        }
    );

    // 3. Also verify `io::policy::owner_settings` reads `settings.toml` via `Settings::owner()`.
    let handoff_narrow_only = "role = \"lead\"\n[model.handoff]\nallow = [\"public\"]\n";
    std::fs::write(state.join("settings.toml"), handoff_narrow_only).expect("write handoff narrow");
    let owner = fieldbook::io::policy::owner_settings(&world);
    assert_eq!(owner.handoff, Some(LabelAllow::from_ids([PUBLIC])));
}

#[test]
fn the_owner_cannot_widen_the_row() {
    let (_tmp, corpus, state, policy_dir) = setup_temp_env();
    // Policy `work` row allows ["public", "np-0007"] only — NOT "owner-supplied".
    // Owner tries to list "owner-supplied" as well.
    let widen_attempt = "\
role = \"lead\"

[model]
order = [\"work\"]

[model.endpoint.work]
adapter = \"openai-compatible@1\"
base_url = \"https://models.example.test/v1\"
model = \"synthetic-model-1\"
allow = [\"public\", \"np-0007\", \"owner-supplied\"]
";
    std::fs::write(state.join("settings.toml"), widen_attempt).expect("write widen settings");
    let world = TestWorld::with_state(&corpus, &state);
    let (_policy_path, ctx) = load_positive_policy(&world, &state, &corpus, &policy_dir);
    let rows = rows_with_model(true);

    let out_json = run_status(
        &AiStatusArgs { json: true },
        &ctx,
        &state,
        &world,
        &rows,
        &ScriptedKeys::new(),
        true,
    );
    assert_eq!(out_json.code, 0);
    let parsed: AiStatus = serde_json::from_str(&out_json.stdout).expect("parse json");
    let work = parsed
        .adapters
        .iter()
        .find(|a| a.endpoint.as_deref() == Some("work"))
        .expect("work");
    let expected_allow = vec![LabelId::parse("np-0007").expect("valid"), PUBLIC];
    assert_eq!(
        work.allow, expected_allow,
        "owner must never widen the policy row"
    );

    let out_text = run_status(
        &AiStatusArgs { json: false },
        &ctx,
        &state,
        &world,
        &rows,
        &ScriptedKeys::new(),
        true,
    );
    assert!(
        out_text.stdout.contains("cut owner-supplied: by policy"),
        "owner-supplied must be cut by policy:\n{}",
        out_text.stdout
    );
    assert!(
        !out_text
            .stdout
            .contains("Ready [np-0007,owner-supplied,public]"),
        "text output must show the effective allow set, not the owner's widened list:\n{}",
        out_text.stdout
    );
}

#[test]
fn bad_settings_exit_2() {
    let root = repo_root();
    let (_tmp, corpus, _state, _policy_dir) = setup_temp_env();
    let bad_state = root.join("tests/fixtures/state-known-bad/FB0900");
    let world = TestWorld::with_state(&corpus, &bad_state);

    let out = fieldbook::run(
        &[
            "ai".to_owned(),
            "status".to_owned(),
            "--state".to_owned(),
            bad_state.to_string_lossy().into_owned(),
        ],
        &world,
    );
    assert_eq!(out.code, 2);
    assert!(out.stderr.contains("FB0900"), "stderr: {}", out.stderr);
    assert!(
        out.stderr
            .contains("put the key in an environment variable and name it with `key_env`"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn json_has_report_version_1() {
    let (_tmp, corpus, state, _policy_dir) = setup_temp_env();
    let world = TestWorld::with_state(&corpus, &state);
    let out = fieldbook::run(
        &[
            "ai".to_owned(),
            "status".to_owned(),
            "--json".to_owned(),
            "--state".to_owned(),
            state.to_string_lossy().into_owned(),
        ],
        &world,
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        out.stdout.starts_with("{\n  \"report_version\": 1,"),
        "report_version must be the first key in JSON output:\n{}",
        out.stdout
    );
    let val: serde_json::Value = serde_json::from_str(&out.stdout).expect("valid JSON");
    let obj = val.as_object().expect("JSON object");
    let keys: Vec<&str> = obj.keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        vec!["adapters", "exit", "policy", "report_version"],
        "ai status --json must have only report_version, policy, adapters, exit"
    );
    assert_eq!(val["report_version"], 1);
    assert_eq!(val["policy"], "floor");
    assert_eq!(val["exit"], 0);
    assert!(
        out.stdout.contains("\"streaming\": false"),
        "caps.streaming must be false: {}",
        out.stdout
    );
}

#[test]
fn opens_no_connection_and_writes_nothing() {
    let (_tmp, corpus, state, _policy_dir) = setup_temp_env();
    let before = snapshot_tree_bytes(&state);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o555))
            .expect("chmod state read-only");
    }

    let world = TestWorld::with_state(&corpus, &state);
    let out = fieldbook::run(
        &[
            "ai".to_owned(),
            "status".to_owned(),
            "--state".to_owned(),
            state.to_string_lossy().into_owned(),
        ],
        &world,
    );
    let out_json = fieldbook::run(
        &[
            "ai".to_owned(),
            "status".to_owned(),
            "--json".to_owned(),
            "--state".to_owned(),
            state.to_string_lossy().into_owned(),
        ],
        &world,
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o755));
    }

    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out_json.code, 0, "stderr: {}", out_json.stderr);
    assert!(world.written().is_empty(), "ai status must write nothing");
    assert_eq!(
        snapshot_tree_bytes(&state),
        before,
        "state folder must be byte-for-byte unchanged"
    );
}

#[test]
fn works_with_no_state_folder() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let corpus = tmp.path().join("corpus-copy");
    copy_dir_all(&repo_root().join("tests/fixtures/corpus-mini"), &corpus);
    let absent_state = tmp.path().join("does-not-exist");
    assert!(!absent_state.exists());

    let world = TestWorld::with_state(&corpus, &absent_state);
    let out = fieldbook::run(
        &[
            "ai".to_owned(),
            "status".to_owned(),
            "--state".to_owned(),
            absent_state.to_string_lossy().into_owned(),
        ],
        &world,
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        !absent_state.exists(),
        "ai status must not create a missing state folder"
    );
    assert!(
        out.stdout
            .contains("Limits: retries 2, max_calls_per_run 100, max_out_tokens 1500"),
        "{}",
        out.stdout
    );
}
