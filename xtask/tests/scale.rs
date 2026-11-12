//! Integration test for `cargo xtask scale`.

#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "integration test starts processes and reads filesystem"
)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| panic!("xtask lives one directory below the repository root"))
        .to_path_buf()
}

fn scale_folders_in_temp() -> BTreeSet<PathBuf> {
    let mut set = BTreeSet::new();
    if let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name
                .to_str()
                .is_some_and(|s| s.starts_with("fieldbook-scale-"))
            {
                set.insert(entry.path());
            }
        }
    }
    set
}

struct ParsedTable {
    units_1x: String,
    corpus_kb_1x: String,
    index_kb_1x: String,
    units_2x: String,
    corpus_kb_2x: String,
    index_kb_2x: String,
}

fn parse_output(stdout: &str) -> ParsedTable {
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(
        lines.len() >= 4,
        "expected at least 4 lines in output, got:\n{stdout}"
    );
    assert!(
        lines[0].starts_with("scale base=xtask/tests/scale-tiny.toml seed="),
        "line 1 mismatch: {}",
        lines[0]
    );

    let header_tokens: Vec<&str> = lines[1].split_whitespace().collect();
    let units_col = header_tokens
        .iter()
        .position(|&t| t == "units")
        .unwrap_or_else(|| panic!("missing `units` in header: {}", lines[1]));
    let corpus_col = header_tokens
        .iter()
        .position(|&t| t == "corpus_kb")
        .unwrap_or_else(|| panic!("missing `corpus_kb` in header: {}", lines[1]));
    let index_col = header_tokens
        .iter()
        .position(|&t| t == "index_kb")
        .unwrap_or_else(|| panic!("missing `index_kb` in header: {}", lines[1]));

    let mut row_1x = None;
    let mut row_2x = None;

    for line in &lines[2..] {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }
        if tokens[0] == "1x" {
            row_1x = Some(tokens);
        } else if tokens[0] == "2x" {
            row_2x = Some(tokens);
        }
    }

    let r1 = row_1x.unwrap_or_else(|| panic!("missing 1x row in:\n{stdout}"));
    let r2 = row_2x.unwrap_or_else(|| panic!("missing 2x row in:\n{stdout}"));

    ParsedTable {
        units_1x: r1[units_col].to_string(),
        corpus_kb_1x: r1[corpus_col].to_string(),
        index_kb_1x: r1[index_col].to_string(),
        units_2x: r2[units_col].to_string(),
        corpus_kb_2x: r2[corpus_col].to_string(),
        index_kb_2x: r2[index_col].to_string(),
    }
}

#[test]
#[ignore = "needs a built fieldbook"]
fn scale_runs_end_to_end() {
    let root = repo_root();

    let build_status = Command::new("cargo")
        .args(["build", "--locked", "-p", fieldbook_core::strings::binary()])
        .current_dir(&root)
        .status()
        .unwrap_or_else(|e| panic!("failed to run cargo build -p fieldbook: {e}"));
    assert!(build_status.success(), "cargo build -p fieldbook failed");

    let xtask_bin = env!("CARGO_BIN_EXE_xtask");
    let fieldbook_bin = if cfg!(windows) {
        "target/debug/fieldbook.exe"
    } else {
        "target/debug/fieldbook"
    };

    let initial_folders = scale_folders_in_temp();

    // Run 1
    let output1 = Command::new(xtask_bin)
        .args([
            "scale",
            "--sizes",
            "1,2",
            "--base",
            "xtask/tests/scale-tiny.toml",
            "--bin",
            fieldbook_bin,
        ])
        .current_dir(&root)
        .output()
        .unwrap_or_else(|e| panic!("failed to run xtask scale (run 1): {e}"));
    assert!(
        output1.status.success(),
        "run 1 failed with status {:?}, stderr:\n{}",
        output1.status,
        String::from_utf8_lossy(&output1.stderr)
    );
    let stdout1 = String::from_utf8(output1.stdout).unwrap_or_else(|e| panic!("utf8 error: {e}"));
    let parsed1 = parse_output(&stdout1);

    // Run 2
    let output2 = Command::new(xtask_bin)
        .args([
            "scale",
            "--sizes",
            "1,2",
            "--base",
            "xtask/tests/scale-tiny.toml",
            "--bin",
            fieldbook_bin,
        ])
        .current_dir(&root)
        .output()
        .unwrap_or_else(|e| panic!("failed to run xtask scale (run 2): {e}"));
    assert!(
        output2.status.success(),
        "run 2 failed with status {:?}, stderr:\n{}",
        output2.status,
        String::from_utf8_lossy(&output2.stderr)
    );
    let stdout2 = String::from_utf8(output2.stdout).unwrap_or_else(|e| panic!("utf8 error: {e}"));
    let parsed2 = parse_output(&stdout2);

    // Compare deterministic outputs
    assert_eq!(parsed1.units_1x, parsed2.units_1x);
    assert_eq!(parsed1.corpus_kb_1x, parsed2.corpus_kb_1x);
    assert_eq!(parsed1.index_kb_1x, parsed2.index_kb_1x);
    assert_eq!(parsed1.units_2x, parsed2.units_2x);
    assert_eq!(parsed1.corpus_kb_2x, parsed2.corpus_kb_2x);
    assert_eq!(parsed1.index_kb_2x, parsed2.index_kb_2x);

    let final_folders = scale_folders_in_temp();
    let leaked: Vec<_> = final_folders.difference(&initial_folders).collect();
    assert!(
        leaked.is_empty(),
        "temporary scale folders remained in temp dir: {leaked:?}"
    );
}
