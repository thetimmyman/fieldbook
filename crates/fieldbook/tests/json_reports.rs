#![allow(
    clippy::expect_used,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "golden comparison reads the tests/golden/json directory"
)]

mod support;

use std::collections::BTreeSet;
use std::path::PathBuf;

use fieldbook_core::audit::{
    AskedVia, Id, Kind, LabelId, Now, Principal, RecordRef, Role, SemVer, Sha256, StoreId,
    StoreUid, Verb, Via,
};
use fieldbook_core::check::{Code, Diagnostic, Report};
use fieldbook_core::date::CivilDate;
use serde_json::Value;

use support::world::TestWorld;

type ReportGenerator = fn() -> String;

const REPORTS: &[(&str, ReportGenerator)] = &[
    ("check", generate_check_report),
    ("audit-show", generate_audit_show_report),
    ("audit-verify", generate_audit_verify_report),
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn generate_check_report() -> String {
    let mut report = Report::new();
    report.push_note("originals check skipped (no corpus/sources/public/)");
    report.push(Diagnostic::new(
        Code::Fb0100,
        false,
        "corpus/corpus.toml",
        1,
        1,
        "missing corpus/corpus.toml",
    ));
    let Ok(today) = CivilDate::from_ymd(2026, 3, 1) else {
        panic!("invalid sample date");
    };
    fieldbook::render_check_json(&report, false, today)
}

fn generate_audit_show_report() -> String {
    let root = repo_root();
    let state_mini = root.join("tests/fixtures/state-mini");
    let world = TestWorld::with_state(&root.join("tests/fixtures/corpus-mini"), &state_mini);
    let args = vec![
        "audit".to_owned(),
        "show".to_owned(),
        "--last".to_owned(),
        "3".to_owned(),
        "--json".to_owned(),
    ];
    let outcome = fieldbook::run(&args, &world);
    assert_eq!(
        outcome.code, 0,
        "audit show --json failed: {}",
        outcome.stderr
    );
    outcome.stdout
}

fn generate_audit_verify_report() -> String {
    let root = repo_root();
    let state_mini = root.join("tests/fixtures/state-mini");
    let world = TestWorld::with_state(&root.join("tests/fixtures/corpus-mini"), &state_mini);
    let args = vec!["audit".to_owned(), "verify".to_owned(), "--json".to_owned()];
    let outcome = fieldbook::run(&args, &world);
    assert_eq!(
        outcome.code, 0,
        "audit verify --json failed: {}",
        outcome.stderr
    );
    outcome.stdout
}

fn golden_dir() -> PathBuf {
    repo_root().join("tests/golden/json")
}

#[test]
fn json_goldens_match_reports() {
    let dir = golden_dir();
    let mut registered = BTreeSet::new();

    for &(name, generator) in REPORTS {
        assert!(
            registered.insert(name),
            "duplicate report entry `{name}` in REPORTS"
        );
        let path = dir.join(format!("{name}.json"));
        let expected = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read golden {}: {e}", path.display()));
        let actual = generator();
        assert_eq!(
            actual,
            expected,
            "JSON report `{name}` differs from golden {}",
            path.display()
        );
    }

    let entries =
        std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("failed to read {}: {e}", dir.display()));
    for entry in entries {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .expect("utf-8 stem");
            assert!(
                registered.contains(stem),
                "golden {} has no row in REPORTS",
                path.display()
            );
        }
    }
}

fn is_typed_audit_string(s: &str) -> bool {
    const CLOSED_WORDS: &[&str] = &[
        "ok",
        "torn",
        "unfinished",
        "error",
        "warning",
        "snapshot",
        "raw",
        "backup",
        "answered",
        "label",
    ];
    s == fieldbook_core::strings::cite_statuses()[0]
        || Sha256::parse(s).is_ok()
        || Now::parse(s).is_ok()
        || Kind::parse(s).is_ok()
        || Code::parse(s).is_ok()
        || RecordRef::parse(s).is_ok()
        || Id::parse(s).is_ok()
        || Principal::parse(s).is_ok()
        || Role::parse(s).is_ok()
        || StoreId::from_root_name(s).is_some()
        || Via::parse(s).is_ok()
        || AskedVia::parse(s).is_ok()
        || Verb::parse(s).is_ok()
        || LabelId::parse(s).is_ok()
        || SemVer::parse(s).is_ok()
        || StoreUid::parse(s).is_ok()
        || CLOSED_WORDS.contains(&s)
}

fn assert_value_has_no_free_text(v: &Value) {
    match v {
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
        Value::String(s) => {
            assert!(
                is_typed_audit_string(s),
                "unexpected free-text string `{s}` in audit JSON golden"
            );
        }
        Value::Array(items) => {
            for item in items {
                assert_value_has_no_free_text(item);
            }
        }
        Value::Object(map) => {
            for val in map.values() {
                assert_value_has_no_free_text(val);
            }
        }
    }
}

#[test]
fn audit_json_has_no_free_text() {
    let dir = golden_dir();
    for name in ["audit-show.json", "audit-verify.json"] {
        let path = dir.join(name);
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        let parsed: Value = serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("invalid JSON in {}: {e}", path.display()));
        assert_value_has_no_free_text(&parsed);
    }
    assert!(!is_typed_audit_string(
        "Free text sentence with punctuation."
    ));
}
