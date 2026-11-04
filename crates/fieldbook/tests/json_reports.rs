#![expect(
    clippy::disallowed_methods,
    reason = "golden comparison reads the tests/golden/json directory"
)]

use std::collections::BTreeSet;
use std::path::PathBuf;

use fieldbook_core::check::{Code, Diagnostic, Report};
use fieldbook_core::date::CivilDate;

type ReportGenerator = fn() -> String;

const REPORTS: &[(&str, ReportGenerator)] = &[("check", generate_check_report)];

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

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/json")
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
