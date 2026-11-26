//! NS-07 credential-leakage and single-reader/single-revealer boundary tests
//! (`ai-config-and-keys` step 7, control NS-07).

#![allow(
    clippy::err_expect,
    clippy::manual_let_else,
    clippy::needless_borrow,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "NS-07 tests spawn the real binary with Command::env and scan source and state files"
)]

#[path = "../../../tests/support/rng.rs"]
mod rng;
mod support;

use std::path::{Path, PathBuf};
use std::process::Command;

use fieldbook::adapters::registry::{ADAPTERS, Build, Needs, Port, Row, Sees, State};
use fieldbook::ai_status::run_status;
use fieldbook::cli::AiStatusArgs;
use fieldbook::io::keys::{KeySource, ScriptedKeys};
use fieldbook::policy::PolicyCtx;
use fieldbook::settings::Settings;
use fieldbook_core::policy::Policy;

use support::world::TestWorld;

const ALPHANUM: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
const FIXTURE_TODAY: &str = include_str!("../../../tests/fixtures/corpus-mini/TODAY");

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn synthetic_key() -> String {
    let mut prng = rng::Rng::new(7);
    let mut out = String::from("FBTEST-");
    for _ in 0..40 {
        let idx = usize::try_from(prng.next_u64() % (ALPHANUM.len() as u64)).expect("idx");
        out.push(char::from(ALPHANUM[idx]));
    }
    assert_eq!(out.len(), 47);
    out
}

fn assert_no_key_slice(label: &str, haystack: &str, key: &str) {
    let bytes = key.as_bytes();
    for win in bytes.windows(12) {
        let slice = std::str::from_utf8(win).expect("ascii key");
        assert!(
            !haystack.contains(slice),
            "NS-07 violation in {label}: 12-character slice `{slice}` of planted key appeared"
        );
    }
    assert!(
        !haystack.contains(key),
        "NS-07 violation in {label}: full planted key appeared"
    );
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

fn scan_dir_for_key(dir: &Path, key: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_dir_for_key(&path, key);
        } else if path.is_file() {
            let bytes = std::fs::read(&path).expect("read state file");
            let text = String::from_utf8_lossy(&bytes);
            assert_no_key_slice(&path.display().to_string(), &text, key);
        }
    }
}

// control: NS-07
#[test]
fn a_synthetic_key_appears_nowhere() {
    let key_val = synthetic_key();
    let var_name = "FIELDBOOK_TEST_SYNTHETIC_KEY";

    // 1. Check Debug output of Key, ScriptedKeys, Settings, and SettingsError.
    let scripted = ScriptedKeys::new().with_key(var_name, &key_val);
    let key = scripted.key(var_name).expect("read scripted key");
    assert_eq!(key.len(), key_val.len());

    let key_debug = format!("{key:?}");
    assert_no_key_slice("Key Debug", &key_debug, &key_val);
    assert_eq!(key_debug, "Key(<redacted>)");

    let scripted_debug = format!("{scripted:?}");
    assert_no_key_slice("ScriptedKeys Debug", &scripted_debug, &key_val);

    let mut rows = ADAPTERS.to_vec();
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

    let ep_settings_text = format!(
        "role = \"lead\"\n\n[model]\norder = [\"work\", \"handoff\"]\n\n[model.endpoint.work]\nadapter = \"openai-compatible@1\"\nbase_url = \"https://models.example.test/v1\"\nmodel = \"synthetic-model-1\"\nallow = [\"public\"]\nkey_env = \"{var_name}\"\n"
    );
    let parsed_settings = Settings::parse(&ep_settings_text, &rows).expect("parse ep settings");
    assert_no_key_slice("Settings Debug", &format!("{parsed_settings:?}"), &key_val);
    assert_no_key_slice(
        "ModelSettings Debug",
        &format!("{:?}", parsed_settings.model),
        &key_val,
    );
    assert_no_key_slice(
        "EndpointSettings Debug",
        &format!("{:?}", parsed_settings.model.endpoints),
        &key_val,
    );
    assert_no_key_slice(
        "SearchSettings Debug",
        &format!("{:?}", parsed_settings.search),
        &key_val,
    );
    assert_no_key_slice(
        "McpSettings Debug",
        &format!("{:?}", parsed_settings.mcp),
        &key_val,
    );
    assert_no_key_slice(
        "ChatSettings Debug",
        &format!("{:?}", parsed_settings.chat),
        &key_val,
    );

    // 2. Run in-process ai_status with ScriptedKeys holding the planted key.
    let root = repo_root();
    let tmp = tempfile::tempdir().expect("tempdir");
    let corpus = tmp.path().join("corpus-copy");
    let state = tmp.path().join("state-copy");
    copy_dir_all(&root.join("tests/fixtures/corpus-mini"), &corpus);
    copy_dir_all(&root.join("tests/fixtures/state-ai"), &state);
    std::fs::write(state.join("settings.toml"), &ep_settings_text).expect("write ep settings");

    let policy_text = std::fs::read_to_string(root.join("tests/fixtures/policy/ai-status.toml"))
        .expect("read ai-status.toml");
    let policy = Policy::parse(&policy_text, None).expect("parse ai-status.toml");
    let ctx = PolicyCtx::loaded(policy);
    let world = TestWorld::with_state(&corpus, &state);

    let inproc_text = run_status(
        &AiStatusArgs { json: false },
        &ctx,
        &state,
        &world,
        &rows,
        &scripted,
        true,
    );
    assert_eq!(inproc_text.code, 0);
    assert_no_key_slice("in-proc ai status stdout", &inproc_text.stdout, &key_val);
    assert_no_key_slice("in-proc ai status stderr", &inproc_text.stderr, &key_val);

    let inproc_json = run_status(
        &AiStatusArgs { json: true },
        &ctx,
        &state,
        &world,
        &rows,
        &scripted,
        true,
    );
    assert_eq!(inproc_json.code, 0);
    assert_no_key_slice(
        "in-proc ai status --json stdout",
        &inproc_json.stdout,
        &key_val,
    );
    assert_no_key_slice(
        "in-proc ai status --json stderr",
        &inproc_json.stderr,
        &key_val,
    );

    // 3. Spawn the real binary with the planted key in the child environment (`Command::env`),
    //    running `ai status`, `ai status --json`, `status`, `check`, and a deliberately failing parse.
    let bin = env!("CARGO_BIN_EXE_fieldbook");
    let bin_state = tmp.path().join("bin-state");
    copy_dir_all(&root.join("tests/fixtures/state-ai"), &bin_state);
    let today = FIXTURE_TODAY.trim();

    let invocations: &[&[&str]] = &[
        &["ai", "status"],
        &["ai", "status", "--json"],
        &["status", "--today", today],
        &["check", "--today", today],
    ];
    for args in invocations {
        let out = Command::new(bin)
            .current_dir(&corpus)
            .env(var_name, &key_val)
            .args(*args)
            .arg("--state")
            .arg(&bin_state)
            .output()
            .expect("spawn fieldbook");
        assert_eq!(
            out.status.code(),
            Some(0),
            "command {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_no_key_slice(&format!("bin {args:?} stdout"), &stdout, &key_val);
        assert_no_key_slice(&format!("bin {args:?} stderr"), &stderr, &key_val);
    }

    // Deliberately failing parse (`FB0900`) with planted key in environment.
    let bad_state = tmp.path().join("bad-state");
    copy_dir_all(
        &root.join("tests/fixtures/state-known-bad/FB0900"),
        &bad_state,
    );
    let bad_out = Command::new(bin)
        .current_dir(&corpus)
        .env(var_name, &key_val)
        .args(["ai", "status", "--state"])
        .arg(&bad_state)
        .output()
        .expect("spawn failing fieldbook ai status");
    assert_eq!(bad_out.status.code(), Some(2));
    assert_no_key_slice(
        "failing ai status stdout",
        &String::from_utf8_lossy(&bad_out.stdout),
        &key_val,
    );
    assert_no_key_slice(
        "failing ai status stderr",
        &String::from_utf8_lossy(&bad_out.stderr),
        &key_val,
    );

    // Search every file of every state folder (including journal.jsonl).
    scan_dir_for_key(&state, &key_val);
    scan_dir_for_key(&bin_state, &key_val);
    scan_dir_for_key(&bad_state, &key_val);
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

fn strip_comments_and_strings(src: &str) -> String {
    let bytes = src.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len);
    let mut i = 0;

    while i < len {
        let b = bytes[i];
        if b == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
            out.push_str("  ");
            i += 2;
            while i < len && bytes[i] != b'\n' {
                out.push(' ');
                i += 1;
            }
            continue;
        }
        if b == b'/' && i + 1 < len && bytes[i + 1] == b'*' {
            out.push_str("  ");
            i += 2;
            let mut depth = 1usize;
            while i < len && depth > 0 {
                if bytes[i] == b'/' && i + 1 < len && bytes[i + 1] == b'*' {
                    depth += 1;
                    out.push_str("  ");
                    i += 2;
                } else if bytes[i] == b'*' && i + 1 < len && bytes[i + 1] == b'/' {
                    depth -= 1;
                    out.push_str("  ");
                    i += 2;
                } else {
                    out.push(if bytes[i] == b'\n' { '\n' } else { ' ' });
                    i += 1;
                }
            }
            continue;
        }
        let raw_start = if b == b'r' {
            Some(i + 1)
        } else if (b == b'b' || b == b'c') && i + 1 < len && bytes[i + 1] == b'r' {
            Some(i + 2)
        } else {
            None
        };
        if let Some(mut j) = raw_start {
            let mut hashes = 0usize;
            while j < len && bytes[j] == b'#' {
                hashes += 1;
                j += 1;
            }
            if j < len && bytes[j] == b'"' {
                while i <= j {
                    out.push(' ');
                    i += 1;
                }
                while i < len {
                    if bytes[i] == b'"' {
                        out.push(' ');
                        i += 1;
                        let mut matched = 0usize;
                        while matched < hashes && i + matched < len && bytes[i + matched] == b'#' {
                            matched += 1;
                        }
                        if matched == hashes {
                            for _ in 0..hashes {
                                out.push(' ');
                                i += 1;
                            }
                            break;
                        }
                    } else {
                        out.push(if bytes[i] == b'\n' { '\n' } else { ' ' });
                        i += 1;
                    }
                }
                continue;
            }
        }
        if b == b'"' {
            out.push(' ');
            i += 1;
            while i < len {
                if bytes[i] == b'\\' {
                    out.push(' ');
                    i += 1;
                    if i < len {
                        out.push(if bytes[i] == b'\n' { '\n' } else { ' ' });
                        i += 1;
                    }
                } else if bytes[i] == b'"' {
                    out.push(' ');
                    i += 1;
                    break;
                } else {
                    out.push(if bytes[i] == b'\n' { '\n' } else { ' ' });
                    i += 1;
                }
            }
            continue;
        }
        out.push(char::from(b));
        i += 1;
    }

    out
}

fn contains_token(stripped: &str, needle: &str) -> bool {
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    for (idx, _) in stripped.match_indices(needle) {
        let before = stripped[..idx].chars().next_back();
        let after = stripped[idx + needle.len()..].chars().next();
        let before_ok = !before.is_some_and(is_ident);
        let after_ok = !after.is_some_and(is_ident);
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

#[test]
fn env_is_read_only_in_io_keys() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rs_files(&src_dir, &mut files);

    let mut offenders = Vec::new();
    for file in files {
        let rel = file
            .strip_prefix(&src_dir)
            .expect("strip prefix")
            .to_string_lossy()
            .replace('\\', "/");
        if rel == "io/keys.rs" {
            continue;
        }
        let src = std::fs::read_to_string(&file).expect("read rs");
        let stripped = strip_comments_and_strings(&src);
        if contains_token(&stripped, "env::var") {
            offenders.push(rel);
        }
    }

    assert!(
        offenders.is_empty(),
        "env::var is allowed only in crates/fieldbook/src/io/keys.rs, found in: {offenders:?}"
    );
}

#[test]
fn reveal_is_called_only_in_io_net() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rs_files(&src_dir, &mut files);

    let mut offenders = Vec::new();
    for file in files {
        let rel = file
            .strip_prefix(&src_dir)
            .expect("strip prefix")
            .to_string_lossy()
            .replace('\\', "/");
        if rel == "io/keys.rs" || rel == "io/net.rs" || rel.starts_with("io/net/") {
            continue;
        }
        let src = std::fs::read_to_string(&file).expect("read rs");
        let stripped = strip_comments_and_strings(&src);
        if stripped.contains(".reveal(") {
            offenders.push(rel);
        }
    }

    assert!(
        offenders.is_empty(),
        ".reveal( is allowed only in crates/fieldbook/src/io/keys.rs and io/net*, found in: {offenders:?}"
    );
}

#[test]
fn a_synthetic_key_and_prompt_stay_out_of_gate_failure_paths() {
    use std::sync::{Arc, Mutex};

    use fieldbook::adapters::doubles::{
        MemCalls, MemJournal, ScriptedModel, ScriptedPace, pick_schema, test_policy,
    };
    use fieldbook::adapters::gate::Guarded;
    use fieldbook::adapters::model::{
        Attempt, AttemptError, Deps, KeepSettings, ModelPort, Permanent, PolicySnapshot, RawReply,
        Route, RunBudget, Transient,
    };
    use fieldbook::io::net::Avail;
    use fieldbook_core::audit::{ModelId, Now};
    use fieldbook_core::egress::Shipped;
    use fieldbook_core::labels::{LabelAllow, LabelId, LabelSet};
    use fieldbook_core::policy::OwnerSettings;
    use fieldbook_core::ports::{Caps, Limits, ModelRequest, Part, PartKind, Purpose, Template};

    const POLICY: &str = r#"
policy_schema = 1
revision = 1
[[destination]]
kind = "model"
name = "work"
url = "https://work.example/v1"
allow = ["public"]
keep_calls_days_max = 14
[audit]
require_fresh_export = false
"#;

    let key_val = synthetic_key();
    let var_name = "FIELDBOOK_TEST_SYNTHETIC_KEY";
    let prompt_canary = "SYNTHETIC PROMPT CANARY";
    let keys = ScriptedKeys::new().with_key(var_name, &key_val);

    let mut owner = OwnerSettings::unset();
    owner.endpoints.insert(
        "work".to_owned(),
        LabelAllow::from_ids([LabelId::parse("public").expect("public")]),
    );
    owner.keep_calls = Some(true);
    owner.keep_days = Some(30);
    let snapshot = Arc::new(Mutex::new(PolicySnapshot {
        policy: test_policy(POLICY, &owner),
        now: Now::new(
            fieldbook_core::CivilDate::from_ymd(2026, 3, 1).expect("date"),
            43_200,
        )
        .expect("now"),
        shipped: Shipped::Hours(1),
    }));

    let route = Route {
        name: fieldbook_core::audit::EndpointName::parse("work").expect("name"),
        base_url: "https://work.example/v1".to_owned(),
        model: ModelId::parse("test-model").expect("model"),
        timeout_s: 5,
        max_reply_bytes: 64 * 1024,
        key_env: Some(var_name.to_owned()),
    };
    let request = || {
        ModelRequest::new(
            Purpose::Ask,
            Template::new("ask@1").expect("template"),
            vec![Part::new(
                PartKind::Question,
                LabelSet::public(),
                prompt_canary,
                Vec::new(),
            )],
            Some(pick_schema()),
            Limits::new(10, 10),
        )
        .expect("request")
    };

    let scenarios: Vec<Result<Attempt, AttemptError>> = vec![
        Err(AttemptError::Permanent(Permanent::Auth)),
        Err(AttemptError::Permanent(Permanent::Rejected(404))),
        Err(AttemptError::Permanent(Permanent::Tls)),
        Err(AttemptError::Permanent(Permanent::Truncated)),
        Err(AttemptError::Permanent(Permanent::TooLarge)),
        Err(AttemptError::Transient(Transient::Connect)),
        Ok(Attempt::Reply(RawReply {
            body: "not json".to_owned(),
            tokens_in: 1,
            tokens_out: 1,
            finish: fieldbook::adapters::model::Finish::Stop,
        })),
    ];

    let mut texts: Vec<String> = Vec::new();
    let mut all_calls = Vec::new();
    for scenario in scenarios {
        let model = Arc::new(
            ScriptedModel::with_caps(Caps::new(true, false, false, false, 1_000_000))
                .push(scenario)
                .push_reply("{\"answer\":\"yes\"}"),
        );
        let journal = Arc::new(MemJournal::new());
        let calls = Arc::new(MemCalls::new());
        let snap = Arc::clone(&snapshot);
        let deps = Deps {
            journal: journal.clone(),
            keys: Arc::new(keys.clone()),
            pace: Arc::new(ScriptedPace::default()),
            policy_provider: Arc::new(move || snap.lock().expect("snapshot").clone()),
            store: calls.clone(),
            keep: KeepSettings {
                calls: true,
                days: 30,
            },
            budget: Arc::new(RunBudget::new(100)),
            avail: Avail {
                net_built: true,
                bridge_built: false,
                cpu_ok: true,
            },
            retries: 1,
            http: None,
        };
        let gate = Guarded::new(Box::new(model), route.clone(), deps);
        let outcome = gate.run(&request(), None, &|| false);
        if let Err(error) = outcome {
            texts.push(error.to_string());
            texts.push(format!("{error:?}"));
        }
        texts.push(format!("{:?}", journal.begins()));
        texts.push(format!("{:?}", journal.calls()));
        texts.push(format!("{:?}", journal.refusals()));
        all_calls.extend(calls.calls());
    }

    for text in &texts {
        assert_no_key_slice("gate failure path", text, &key_val);
        assert!(
            !text.contains(prompt_canary),
            "NS-07 violation: the prompt text reached an error, journal or Debug output"
        );
    }
    let calls_debug = format!("{all_calls:?}");
    assert_no_key_slice("call records", &calls_debug, &key_val);
}
