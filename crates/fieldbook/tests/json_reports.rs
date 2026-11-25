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
use fieldbook_core::report::{
    BackupLatestVerification, BackupList, BackupListDrill, BackupListSnapshot, BackupListTarget,
};
use serde_json::Value;

use support::world::TestWorld;

type ReportGenerator = fn() -> String;

const REPORTS: &[(&str, ReportGenerator)] = &[
    ("check", generate_check_report),
    ("audit-show", generate_audit_show_report),
    ("audit-verify", generate_audit_verify_report),
    ("backup-list", generate_backup_list_report),
    ("add", generate_add_report),
    ("search", generate_search_report),
    ("search-pages", generate_search_pages_report),
    ("policy-show", generate_policy_show_report),
    ("ai-status", generate_ai_status_report),
    ("context", generate_context_report),
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

fn generate_backup_list_report() -> String {
    let report = BackupList {
        report_version: 1,
        verb: "backup list".to_string(),
        target: Some(BackupListTarget {
            id: "it".to_string(),
            kind: "it-location".to_string(),
            path: "backups".to_string(),
            company_storage: false,
        }),
        warn_after_days: 7,
        keep_last: 7,
        keep_weekly: 4,
        snapshots: vec![BackupListSnapshot {
            snapshot: 3,
            backup_version: 1,
            tool_version: "0.1.0".to_string(),
            created_utc: "2026-01-01T09:00:00Z".to_string(),
            labels: vec!["public".to_string()],
            roots: vec!["corpus".to_string(), "state".to_string()],
            files: 412,
            bytes: 9120344,
            fingerprint: "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2"
                .to_string(),
            journal_seq: 118,
            journal_head: "b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3"
                .to_string(),
            journal_state: "ok".to_string(),
            manifest_sha256: "c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4"
                .to_string(),
            verified: true,
            verified_utc: Some("2026-01-01T09:05:00Z".to_string()),
            latest_verification: Some(BackupLatestVerification {
                outcome: "failed".to_string(),
                utc: "2026-01-01T10:00:00Z".to_string(),
            }),
        }],
        verified_utc: Some("2026-01-01T09:05:00Z".to_string()),
        drill: Some(BackupListDrill {
            mode: "drill".to_string(),
            outcome: "ok".to_string(),
            utc: "2026-01-02T10:00:00Z".to_string(),
        }),
        findings: vec![],
        exit: 0,
    };
    fieldbook::render_backup_list_json(&report)
}

fn generate_add_report() -> String {
    let root = repo_root();
    let tmp = support::temp_copy(&root.join("tests/fixtures/corpus-mini"));
    let inbox = tmp.path().join("inbox");
    std::fs::create_dir_all(&inbox).expect("create inbox dir");
    for entry in std::fs::read_dir(root.join("tests/fixtures/add-folder")).expect("read add-folder")
    {
        let entry = entry.expect("add-folder entry");
        std::fs::copy(entry.path(), inbox.join(entry.file_name())).expect("copy add-folder file");
    }
    let world = TestWorld::new(tmp.path());
    let args = vec![
        "add".to_owned(),
        "inbox".to_owned(),
        "--confirm-public".to_owned(),
        "--json".to_owned(),
    ];
    let outcome = fieldbook::run(&args, &world);
    assert_eq!(
        outcome.code, 1,
        "add inbox --json expected exit 1: {}",
        outcome.stderr
    );
    outcome.stdout
}

/// A word whose hits show a page row that a section leads, and an other row after it.
fn generate_search_pages_report() -> String {
    let root = repo_root();
    let world = TestWorld::new(&root.join("tests/fixtures/corpus-mini"));
    let args = vec![
        "search".to_owned(),
        "--json".to_owned(),
        "threshold".to_owned(),
    ];
    let outcome = fieldbook::run(&args, &world);
    assert_eq!(outcome.code, 0, "search --json failed: {}", outcome.stderr);
    outcome.stdout
}

fn generate_search_report() -> String {
    let root = repo_root();
    let world = TestWorld::new(&root.join("tests/fixtures/corpus-mini"));
    let args = vec![
        "search".to_owned(),
        "--json".to_owned(),
        "S-0002".to_owned(),
    ];
    let outcome = fieldbook::run(&args, &world);
    assert_eq!(outcome.code, 0, "search --json failed: {}", outcome.stderr);
    outcome.stdout
}

fn generate_policy_show_report() -> String {
    let root = repo_root();
    let world = TestWorld::new(&root.join("tests/fixtures/corpus-mini"));
    let args = vec!["policy".to_owned(), "show".to_owned(), "--json".to_owned()];
    let outcome = fieldbook::run(&args, &world);
    assert_eq!(
        outcome.code, 0,
        "policy show --json failed: {}",
        outcome.stderr
    );
    outcome.stdout
}

fn generate_ai_status_report() -> String {
    let root = repo_root();
    let state_ai = root.join("tests/fixtures/state-ai");
    let world = TestWorld::with_state(&root.join("tests/fixtures/corpus-mini"), &state_ai);
    let args = vec!["ai".to_owned(), "status".to_owned(), "--json".to_owned()];
    let outcome = fieldbook::run(&args, &world);
    assert_eq!(
        outcome.code, 0,
        "ai status --json failed: {}",
        outcome.stderr
    );
    outcome.stdout
}

fn generate_context_report() -> String {
    let root = repo_root();
    let tmp = support::temp_copy(&root.join("tests/fixtures/corpus-mini"));
    let world = TestWorld::new(tmp.path());
    let args = vec![
        "context".to_owned(),
        "synthetic".to_owned(),
        "--scope".to_owned(),
        "area:cmp".to_owned(),
        "--json".to_owned(),
    ];
    let outcome = fieldbook::run(&args, &world);
    assert_eq!(outcome.code, 0, "context --json failed: {}", outcome.stderr);
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
