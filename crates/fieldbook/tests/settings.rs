//! Unit and integration tests for the fail-closed AI `settings.toml` parser (`ai-config-and-keys`).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "integration tests read fixture files, docs, and spawn the binary"
)]

mod support;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use fieldbook::adapters::registry::{ADAPTERS, Build, Needs, Port, Row, Sees, State};
use fieldbook::settings::{
    AuthKind, DEFAULT_IDLE_MINUTES, DEFAULT_KEEP_CALLS, DEFAULT_KEEP_DAYS,
    DEFAULT_MAX_CALLS_PER_RUN, DEFAULT_MAX_OUT_TOKENS, DEFAULT_RETRIES, DEFAULT_TIMEOUT_S,
    HELP_POLICY_APPROVES, HELP_SECRET_KEY, KEY_TABLE, ProxyMode, Settings,
};
use fieldbook_core::check::Code;
use fieldbook_core::labels::{LabelAllow, LabelId, OWNER_SUPPLIED, PUBLIC};
use fieldbook_core::policy::merge::KEY_TABLE as POLICY_MERGE_KEY_TABLE;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn test_rows_with_model() -> Vec<Row> {
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
    rows
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

#[test]
fn defaults_when_the_file_is_absent() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let missing_state = tmp.path().join("no-state-yet");
    let read = fieldbook::io::ai_settings::read_settings_text(&missing_state)
        .expect("missing settings.toml is Ok(None)");
    assert!(read.is_none());
    assert!(
        !missing_state.exists(),
        "reading missing settings must not create the state folder"
    );

    let defaults = Settings::default();
    assert_eq!(defaults.model.order, vec!["handoff".to_owned()]);
    assert_eq!(defaults.model.retries, DEFAULT_RETRIES);
    assert_eq!(defaults.model.max_calls_per_run, DEFAULT_MAX_CALLS_PER_RUN);
    assert_eq!(defaults.model.max_out_tokens, DEFAULT_MAX_OUT_TOKENS);
    assert_eq!(defaults.model.keep_calls, DEFAULT_KEEP_CALLS);
    assert_eq!(defaults.model.keep_days, DEFAULT_KEEP_DAYS);
    assert_eq!(defaults.model.proxy, ProxyMode::Env);
    assert!(defaults.model.handoff.allow.is_none());
    assert!(defaults.model.endpoints.is_empty());
    assert!(!defaults.search.semantic);
    assert!(defaults.mcp.allow.is_none());
    assert_eq!(defaults.chat.idle_minutes, DEFAULT_IDLE_MINUTES);

    let parsed_empty = Settings::parse("", ADAPTERS).expect("empty settings text parses");
    assert_eq!(parsed_empty, defaults);
}

#[test]
fn example_file_parses() {
    let text = std::fs::read_to_string(repo_root().join("tests/fixtures/state-ai/settings.toml"))
        .expect("read state-ai/settings.toml");
    let parsed = Settings::parse(&text, ADAPTERS).expect("state-ai/settings.toml must parse");
    assert_eq!(parsed.model.order, vec!["handoff".to_owned()]);
    assert_eq!(
        parsed.model.handoff.allow,
        Some(LabelAllow::from_ids([PUBLIC, OWNER_SUPPLIED]))
    );
    assert_eq!(parsed.mcp.allow, Some(LabelAllow::from_ids([PUBLIC])));
    assert_eq!(parsed.chat.idle_minutes, 30);
    assert!(!parsed.search.semantic);
}

#[test]
fn unknown_key_is_fb0900() {
    let text = "[model]\norder = [\"handoff\"]\nunknown_option = true\n";
    let errs = Settings::parse(text, ADAPTERS).expect_err("unknown key must fail");
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].code, Code::Fb0900);
    assert_eq!(errs[0].line, 3);
    assert!(errs[0].key.contains("unknown_option"), "{}", errs[0]);
}

#[test]
fn unknown_table_is_fb0900() {
    for (text, expected_line, expected_key) in [
        ("[decide]\nmode = \"strict\"\n", 1, "decide"),
        ("[bogus]\nflag = 1\n", 1, "bogus"),
        (
            "[model]\norder = [\"handoff\"]\n\n[model.extra]\nx = 1\n",
            4,
            "model.extra",
        ),
    ] {
        let errs = Settings::parse(text, ADAPTERS).expect_err("unknown table must fail");
        assert!(
            errs.iter().any(|e| e.code == Code::Fb0900
                && e.line == expected_line
                && e.key == expected_key),
            "expected FB0900 at line {expected_line} for {expected_key}, got {errs:?}"
        );
    }
}

// control: PL-02
#[test]
fn company_approved_is_fb0900() {
    let root = repo_root();
    let bin = env!("CARGO_BIN_EXE_fieldbook");

    // 1. In `[model.handoff]` on a copy of `state-ai`.
    let tmp1 = tempfile::tempdir().expect("tempdir 1");
    let state1 = tmp1.path().join("state");
    copy_dir_all(&root.join("tests/fixtures/state-ai"), &state1);
    let orig = std::fs::read_to_string(state1.join("settings.toml")).expect("read settings");
    let with_handoff = orig.replace(
        "[model.handoff]\n",
        "[model.handoff]\ncompany_approved = true\n",
    );
    let expected_line_handoff = with_handoff
        .lines()
        .position(|l| l.trim() == "company_approved = true")
        .expect("find company_approved line")
        + 1;
    std::fs::write(state1.join("settings.toml"), &with_handoff).expect("write settings 1");

    let out1 = Command::new(bin)
        .current_dir(root.join("tests/fixtures/corpus-mini"))
        .args(["ai", "status", "--state"])
        .arg(&state1)
        .output()
        .expect("run fieldbook ai status");
    assert_eq!(out1.status.code(), Some(2));
    let stderr1 = String::from_utf8_lossy(&out1.stderr);
    let stdout1 = String::from_utf8_lossy(&out1.stdout);
    assert!(stderr1.contains("FB0900"), "stderr: {stderr1}");
    assert!(
        stderr1.contains(&format!("line {expected_line_handoff}")),
        "stderr must name line {expected_line_handoff}: {stderr1}"
    );
    assert!(stderr1.contains("company_approved"), "stderr: {stderr1}");
    assert!(
        !stdout1.contains("Ready"),
        "no endpoint may be listed as accepted: {stdout1}"
    );

    // 2. At top level on a copy of `state-ai`.
    let tmp2 = tempfile::tempdir().expect("tempdir 2");
    let state2 = tmp2.path().join("state");
    copy_dir_all(&root.join("tests/fixtures/state-ai"), &state2);
    let with_top = format!("company_approved = true\n{orig}");
    std::fs::write(state2.join("settings.toml"), &with_top).expect("write settings 2");

    let out2 = Command::new(bin)
        .current_dir(root.join("tests/fixtures/corpus-mini"))
        .args(["ai", "status", "--state"])
        .arg(&state2)
        .output()
        .expect("run fieldbook ai status");
    assert_eq!(out2.status.code(), Some(2));
    let stderr2 = String::from_utf8_lossy(&out2.stderr);
    let stdout2 = String::from_utf8_lossy(&out2.stdout);
    assert!(stderr2.contains("FB0900"), "stderr: {stderr2}");
    assert!(stderr2.contains("line 1"), "stderr: {stderr2}");
    assert!(stderr2.contains("company_approved"), "stderr: {stderr2}");
    assert!(
        !stdout2.contains("Ready"),
        "no endpoint may be listed as accepted: {stdout2}"
    );

    // 3. Step 11: otherwise valid `[model.endpoint.work]` with an injected Built Model row.
    let rows = test_rows_with_model();
    let valid_endpoint = "\
[model]
order = [\"work\", \"handoff\"]

[model.endpoint.work]
adapter = \"openai-compatible@1\"
base_url = \"https://models.example.test/v1\"
model = \"synthetic-model-1\"
allow = [\"public\"]
";
    assert!(
        Settings::parse(valid_endpoint, &rows).is_ok(),
        "endpoint settings must parse cleanly without company_approved"
    );

    let bad_endpoint = format!("{valid_endpoint}company_approved = true\n");
    let errs = Settings::parse(&bad_endpoint, &rows)
        .expect_err("company_approved on endpoint must fail with FB0900");
    assert!(
        errs.iter()
            .any(|e| e.code == Code::Fb0900 && e.key.contains("company_approved") && e.line == 9),
        "expected FB0900 naming company_approved at line 9, got {errs:?}"
    );
}

#[test]
fn secret_named_key_is_refused_with_the_help_line() {
    let rows = test_rows_with_model();
    for secret_key in [
        "key", "api_key", "API_KEY", "token", "Token", "secret", "SECRET", "password", "Password",
    ] {
        let in_model = format!("[model]\n{secret_key} = \"not-real\"\n");
        let errs = Settings::parse(&in_model, &rows).expect_err("secret key in [model] must fail");
        assert!(
            errs.iter().any(|e| {
                e.code == Code::Fb0900
                    && e.line == 2
                    && e.help == HELP_SECRET_KEY
                    && e.key.contains(secret_key)
            }),
            "expected secret help for `{secret_key}` in [model], got {errs:?}"
        );

        let in_endpoint = format!(
            "[model.endpoint.work]\nadapter = \"openai-compatible@1\"\nbase_url = \"https://models.example.test/v1\"\nmodel = \"m1\"\n{secret_key} = \"not-real\"\n"
        );
        let errs_ep =
            Settings::parse(&in_endpoint, &rows).expect_err("secret key in endpoint must fail");
        assert!(
            errs_ep.iter().any(|e| {
                e.code == Code::Fb0900
                    && e.line == 5
                    && e.help == HELP_SECRET_KEY
                    && e.key.contains(secret_key)
            }),
            "expected secret help for `{secret_key}` in endpoint, got {errs_ep:?}"
        );
    }
}

#[test]
fn base_url_with_user_info_is_refused() {
    let rows = test_rows_with_model();
    let text = "\
[model.endpoint.work]
adapter = \"openai-compatible@1\"
base_url = \"https://user:pass@models.example.test/v1\"
model = \"m1\"
";
    let errs = Settings::parse(text, &rows).expect_err("base_url with userinfo must fail");
    assert!(
        errs.iter().any(|e| e.code == Code::Fb0900
            && e.line == 3
            && e.key == "model.endpoint.work.base_url"),
        "expected FB0900 on line 3 base_url, got {errs:?}"
    );
}

#[test]
fn a_word_that_is_not_a_label_id_is_refused() {
    for bad in ["CUI", "org data"] {
        let text = format!("[model.handoff]\nallow = [\"{bad}\"]\n");
        let errs = Settings::parse(&text, ADAPTERS).expect_err("non-label word must fail");
        assert!(
            errs.iter()
                .any(|e| e.code == Code::Fb0900 && e.line == 2 && e.key == "model.handoff.allow"),
            "expected FB0900 for `{bad}`, got {errs:?}"
        );
    }
}

#[test]
fn a_well_formed_id_is_accepted_by_shape() {
    let text = "[model.handoff]\nallow = [\"public\", \"zz-9999\"]\n";
    let parsed = Settings::parse(text, ADAPTERS).expect("well-formed opaque id accepted by shape");
    let allow = parsed.model.handoff.allow.expect("allow list present");
    assert!(allow.contains(&PUBLIC));
    assert!(allow.contains(&LabelId::parse("zz-9999").expect("valid id")));
}

#[test]
fn unlabelled_and_marked_cannot_be_allowed() {
    for forbidden in ["unlabelled", "marked"] {
        let text = format!("[mcp]\nallow = [\"{forbidden}\"]\n");
        let errs = Settings::parse(&text, ADAPTERS).expect_err("forbidden builtin must fail");
        assert!(
            errs.iter()
                .any(|e| e.code == Code::Fb0900 && e.line == 2 && e.key == "mcp.allow"),
            "expected FB0900 for `{forbidden}`, got {errs:?}"
        );
    }
}

#[test]
fn the_removed_keys_are_refused_with_the_help_line() {
    let rows = test_rows_with_model();
    let cases = [
        ("[model]\napproved_for = [\"public\"]\n", 2),
        ("[model]\ndefault = \"handoff\"\n", 2),
        ("[model]\nfallback = \"handoff\"\n", 2),
        ("[model.route]\nask = \"handoff\"\n", 1),
        (
            "[model.endpoint.work]\nadapter = \"openai-compatible@1\"\nbase_url = \"https://models.example.test/v1\"\nmodel = \"m1\"\napproved_for = [\"public\"]\n",
            5,
        ),
    ];

    for (text, expected_line) in cases {
        let errs = Settings::parse(text, &rows).expect_err("removed key must fail");
        assert!(
            errs.iter().any(|e| {
                e.code == Code::Fb0900 && e.line == expected_line && e.help == HELP_POLICY_APPROVES
            }),
            "expected removed-key help at line {expected_line} for:\n{text}\ngot: {errs:?}"
        );
    }
}

#[test]
fn order_defaults_to_handoff() {
    let parsed = Settings::parse("[model]\nretries = 3\n", ADAPTERS).expect("parse");
    assert_eq!(parsed.model.order, vec!["handoff".to_owned()]);
    assert_eq!(parsed.model.retries, 3);
}

#[test]
fn order_naming_an_unknown_endpoint_is_fb0900() {
    let text = "[model]\norder = [\"ghost\", \"handoff\"]\n";
    let errs = Settings::parse(text, ADAPTERS).expect_err("unknown endpoint in order must fail");
    assert!(
        errs.iter()
            .any(|e| e.code == Code::Fb0900 && e.line == 2 && e.key == "model.order"),
        "got {errs:?}"
    );
}

#[test]
fn order_with_a_repeat_is_fb0900() {
    let text = "[model]\norder = [\"handoff\", \"handoff\"]\n";
    let errs = Settings::parse(text, ADAPTERS).expect_err("repeated route in order must fail");
    assert!(
        errs.iter()
            .any(|e| e.code == Code::Fb0900 && e.line == 2 && e.key == "model.order"),
        "got {errs:?}"
    );
}

#[test]
fn endpoint_with_no_allow_is_valid_and_accepts_nothing() {
    let rows = test_rows_with_model();
    let text = "\
[model]
order = [\"work\", \"handoff\"]

[model.endpoint.work]
adapter = \"openai-compatible@1\"
base_url = \"https://models.example.test/v1\"
model = \"m1\"
";
    let parsed = Settings::parse(text, &rows).expect("endpoint with no allow is valid");
    let ep = parsed
        .model
        .endpoints
        .values()
        .next()
        .expect("work endpoint");
    assert!(ep.allow.is_none());
    assert_eq!(ep.timeout_s, DEFAULT_TIMEOUT_S);
    assert_eq!(ep.auth, AuthKind::Bearer);

    let owner = parsed.owner();
    assert!(
        !owner.endpoints.contains_key("work"),
        "no allow -> no entry in OwnerSettings.endpoints"
    );
}

#[test]
fn absent_handoff_and_mcp_lists_mean_no_narrowing() {
    let parsed = Settings::parse("[model]\nretries = 2\n", ADAPTERS).expect("parse");
    let owner = parsed.owner();
    assert!(owner.handoff.is_none());
    assert!(owner.mcp.is_none());
}

#[test]
fn owner_settings_carry_the_lists() {
    let rows = test_rows_with_model();
    let text = "\
[model]
order = [\"work\", \"handoff\"]
keep_calls = false
keep_days = 7

[model.handoff]
allow = [\"public\"]

[model.endpoint.work]
adapter = \"openai-compatible@1\"
base_url = \"https://models.example.test/v1\"
model = \"m1\"
allow = [\"public\", \"np-0007\"]

[mcp]
allow = [\"public\"]
";
    let parsed = Settings::parse(text, &rows).expect("parse");
    let owner = parsed.owner();
    assert_eq!(owner.order, vec!["work".to_owned(), "handoff".to_owned()]);
    assert_eq!(owner.keep_calls, Some(false));
    assert_eq!(owner.keep_days, Some(7));
    assert_eq!(owner.handoff, Some(LabelAllow::from_ids([PUBLIC])));
    assert_eq!(owner.mcp, Some(LabelAllow::from_ids([PUBLIC])));
    assert_eq!(
        owner.endpoints.get("work"),
        Some(&LabelAllow::from_ids([
            PUBLIC,
            LabelId::parse("np-0007").expect("valid")
        ]))
    );
}

#[test]
fn settings_keys_have_merge_rules() {
    let from_settings: BTreeSet<String> = KEY_TABLE
        .iter()
        .filter(|spec| spec.meets_policy)
        .map(|spec| format!("{} {}", spec.table, spec.key))
        .collect();
    let from_policy: BTreeSet<String> = POLICY_MERGE_KEY_TABLE
        .iter()
        .map(|row| row.key.to_owned())
        .collect();

    assert_eq!(
        from_settings, from_policy,
        "settings::KEY_TABLE policy-meeting keys and policy::merge::KEY_TABLE must match 1-to-1"
    );
}

#[test]
fn settings_keys_match_docs() {
    let root = repo_root();

    // 1. Every settings key named by a registry row must be in KEY_TABLE.
    let key_names: BTreeSet<&str> = KEY_TABLE.iter().map(|s| s.key).collect();
    for row in ADAPTERS {
        for &k in row.settings_keys {
            assert!(
                key_names.contains(k),
                "registry row {} names settings key `{k}` missing from KEY_TABLE",
                row.id
            );
        }
    }

    // 2. docs/reference/ports.md must contain the exact Settings keys table generated from KEY_TABLE.
    let ports_md = std::fs::read_to_string(root.join("docs/reference/ports.md"))
        .expect("read docs/reference/ports.md");
    assert!(
        ports_md.contains("## Settings keys"),
        "docs/reference/ports.md must contain `## Settings keys`"
    );
    for spec in KEY_TABLE {
        let expected_row = format!(
            "| {} | {} | {} | {} | {} |",
            spec.table, spec.key, spec.ty, spec.default, spec.adapter
        );
        assert!(
            ports_md.contains(&expected_row),
            "docs/reference/ports.md is stale: missing row `{expected_row}`"
        );
    }

    // 3. KEY_TABLE (non-endpoint keys) and tests/fixtures/state-ai/settings.toml must agree.
    let example_text = std::fs::read_to_string(root.join("tests/fixtures/state-ai/settings.toml"))
        .expect("read state-ai/settings.toml");
    let example_tbl: toml::Table = toml::from_str(&example_text).expect("parse example toml");

    let mut example_keys = BTreeSet::new();
    if let Some(m) = example_tbl.get("model").and_then(toml::Value::as_table) {
        for (k, v) in m {
            if k == "handoff" {
                if let Some(h) = v.as_table() {
                    for hk in h.keys() {
                        example_keys.insert(("[model.handoff]".to_owned(), hk.clone()));
                    }
                }
            } else if k != "endpoint" {
                example_keys.insert(("[model]".to_owned(), k.clone()));
            }
        }
    }
    for top in ["search", "mcp", "chat"] {
        if let Some(sub) = example_tbl.get(top).and_then(toml::Value::as_table) {
            for k in sub.keys() {
                example_keys.insert((format!("[{top}]"), k.clone()));
            }
        }
    }

    let expected_non_endpoint: BTreeSet<(String, String)> = KEY_TABLE
        .iter()
        .filter(|s| s.table != "[model.endpoint.<name>]")
        .map(|s| (s.table.to_owned(), s.key.to_owned()))
        .collect();

    assert_eq!(
        example_keys, expected_non_endpoint,
        "tests/fixtures/state-ai/settings.toml keys and KEY_TABLE non-endpoint keys disagree"
    );
}

#[test]
fn adapter_must_be_built() {
    let mut rows = ADAPTERS.to_vec();
    rows.push(Row {
        port: Port::Model,
        id: "openai-compatible@1",
        default: false,
        state: State::Later,
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

    let text = "\
[model.endpoint.work]
adapter = \"openai-compatible@1\"
base_url = \"https://models.example.test/v1\"
model = \"m1\"
";
    let errs = Settings::parse(text, &rows).expect_err("Later adapter must be rejected");
    assert!(
        errs.iter().any(|e| e.code == Code::Fb0900
            && e.line == 2
            && e.key == "model.endpoint.work.adapter"),
        "got {errs:?}"
    );
}

#[test]
fn adapter_version_defaults_to_the_highest_built() {
    let mut rows = test_rows_with_model();
    rows.push(Row {
        port: Port::Model,
        id: "openai-compatible@2",
        default: false,
        state: State::Built,
        build: Build::Ai,
        krate: env!("CARGO_PKG_NAME"),
        needs: Needs::Net,
        sees: Sees::Labelled,
        calls_model: true,
        settings_keys: &[],
        fixtures: "tests/fixtures/ports/model/openai-compatible@2/",
        suite: "model_suite",
        doc: "model-openai-compatible2",
    });
    rows.push(Row {
        port: Port::Model,
        id: "openai-compatible@3",
        default: false,
        state: State::Later,
        build: Build::Ai,
        krate: env!("CARGO_PKG_NAME"),
        needs: Needs::Net,
        sees: Sees::Labelled,
        calls_model: true,
        settings_keys: &[],
        fixtures: "tests/fixtures/ports/model/openai-compatible@3/",
        suite: "model_suite",
        doc: "model-openai-compatible3",
    });

    let text = "\
[model.endpoint.work]
adapter = \"openai-compatible\"
base_url = \"https://models.example.test/v1\"
model = \"m1\"
";
    let parsed = Settings::parse(text, &rows).expect("unversioned adapter picks highest built");
    let ep = parsed.model.endpoints.values().next().expect("work");
    assert_eq!(ep.adapter, "openai-compatible@2");
}

#[test]
fn semantic_true_needs_a_built_row() {
    let text = "[search]\nsemantic = true\n";
    let errs = Settings::parse(text, ADAPTERS).expect_err("semantic=true without semantic@1 fails");
    assert!(
        errs.iter()
            .any(|e| e.code == Code::Fb0900 && e.line == 2 && e.key == "search.semantic"),
        "got {errs:?}"
    );

    let mut rows = ADAPTERS.to_vec();
    rows.push(Row {
        port: Port::Index,
        id: "semantic@1",
        default: false,
        state: State::Built,
        build: Build::Ai,
        krate: env!("CARGO_PKG_NAME"),
        needs: Needs::Files,
        sees: Sees::Labelled,
        calls_model: false,
        settings_keys: &[],
        fixtures: "tests/fixtures/ports/index/semantic@1/",
        suite: "index_suite",
        doc: "index-semantic1",
    });
    let parsed = Settings::parse(text, &rows).expect("semantic=true with built semantic@1 passes");
    assert!(parsed.search.semantic);
}

#[test]
fn owned_keys_are_not_unknown() {
    let text = "\
role = \"owner\"

[asked]
mode = \"hash\"
keep_days = 90

[backup]
default = \"main\"
warn_after_days = 7

[[backup.target]]
id = \"main\"
kind = \"folder\"
path = \"/tmp/backups\"

[model]
order = [\"handoff\"]
";
    let parsed = Settings::parse(text, ADAPTERS).expect("owned keys must be ignored cleanly");
    assert_eq!(parsed.model.order, vec!["handoff".to_owned()]);
}

#[test]
fn every_error_has_a_line_number() {
    let rows = test_rows_with_model();
    let bad_inputs = [
        "[model]\nretries = 0\n",
        "[model]\nretries = 11\n",
        "[chat]\nidle_minutes = 0\n",
        "[chat]\nidle_minutes = 1441\n",
        "[model.endpoint.work]\nadapter = \"openai-compatible@1\"\nbase_url = \"https://models.example.test/v1\"\nmodel = \"m1\"\ntimeout_s = 0\n",
        "[model.endpoint.work]\nadapter = \"openai-compatible@1\"\nbase_url = \"https://models.example.test/v1\"\nmodel = \"m1\"\ntimeout_s = 601\n",
        "[model.endpoint.work]\nadapter = \"openai-compatible@1\"\nbase_url = \"https://models.example.test/v1\"\nmodel = \"m1\"\nkey_env = \"bad_lower\"\n",
        "[model.endpoint.handoff]\nadapter = \"openai-compatible@1\"\nbase_url = \"https://models.example.test/v1\"\nmodel = \"m1\"\n",
        "not valid toml = = =\n",
    ];

    for input in bad_inputs {
        let errs = Settings::parse(input, &rows).expect_err("bad input must fail");
        assert!(!errs.is_empty());
        for err in &errs {
            assert_eq!(err.code, Code::Fb0900);
            assert!(err.line >= 1, "line must be >= 1 for {err:?}");
            assert!(!err.help.is_empty(), "help must not be empty for {err:?}");
        }
    }
}

// control: PL-03
#[test]
fn ai_status_has_no_today_flag() {
    let root = repo_root();
    let out = Command::new(env!("CARGO_BIN_EXE_fieldbook"))
        .current_dir(root.join("tests/fixtures/corpus-mini"))
        .args(["ai", "status", "--today", "2000-01-01"])
        .output()
        .expect("run fieldbook ai status --today");
    assert_eq!(
        out.status.code(),
        Some(2),
        "ai status --today must be a usage error (exit 2), got stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
