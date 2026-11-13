//! Integration test for Restore Drill 4 from docs/runbooks/restore-drill.md.

#![cfg(unix)]
#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "drill4 integration test invokes the built binary and shell tools"
)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

const RUNBOOK: &str = include_str!("../../../docs/runbooks/restore-drill.md");

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dest_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), dest_path)?;
        }
    }
    Ok(())
}

fn tool_available(tool: &str) -> bool {
    Command::new(tool).arg("--version").output().is_ok()
}

fn extract_drill4_script(markdown: &str) -> Result<String, &'static str> {
    let lines: Vec<&str> = markdown.lines().collect();
    let start_idx = lines
        .iter()
        .position(|l| l.starts_with("## Drill 4"))
        .ok_or("line starting ## Drill 4 not found")?;
    let end_idx = lines[start_idx..]
        .iter()
        .position(|l| l.starts_with("## Drill 5"))
        .map(|pos| start_idx + pos)
        .ok_or("line starting ## Drill 5 not found")?;

    let mut blocks = Vec::new();
    let mut in_sh = false;
    let mut current_block = Vec::new();

    for line in &lines[start_idx..end_idx] {
        let trimmed = line.trim();
        if trimmed == "```sh" {
            in_sh = true;
            current_block.clear();
        } else if in_sh && trimmed.starts_with("```") {
            in_sh = false;
            blocks.push(current_block.join("\n"));
        } else if in_sh {
            current_block.push(*line);
        }
    }

    Ok(blocks.join("\n\n"))
}

struct TempDrillDirGuard<'a> {
    tmpdir: &'a Path,
    initial: &'a BTreeSet<PathBuf>,
}

impl Drop for TempDrillDirGuard<'_> {
    fn drop(&mut self) {
        if let Ok(entries) = std::fs::read_dir(self.tmpdir) {
            for entry in entries.filter_map(Result::ok) {
                let p = entry.path();
                if let Some(name) = p.file_name().and_then(|n| n.to_str())
                    && name.starts_with("field book drill.")
                    && !self.initial.contains(&p)
                {
                    let _ = std::fs::remove_dir_all(&p);
                }
            }
        }
    }
}

#[expect(
    clippy::print_stderr,
    reason = "contract requires eprintln! skip message when tools are missing"
)]
#[test]
fn restore_drill4_runs_against_binary() {
    if !tool_available("sha256sum") {
        eprintln!("skipping drill4: sha256sum tool not available");
        return;
    }
    if !tool_available("diff") {
        eprintln!("skipping drill4: diff tool not available");
        return;
    }

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus_mini = repo_root.join("tests/fixtures/corpus-mini");
    let state_mini = repo_root.join("tests/fixtures/state-mini");

    // 1. Build disposable synthetic/public tree and state
    let tmp_tree = tempfile::tempdir().expect("create tmp_tree");
    copy_dir_all(&corpus_mini, tmp_tree.path()).expect("copy corpus-mini");

    // The runbook Drill 4 Step 1 calls `fieldbook backup verify --raw .`.
    // Verification of a raw directory looks for state/audit under the specified directory
    // (e.g. `<raw_dir>/.fieldbook/audit`). Thus, the state must live inside the tree at
    // `.fieldbook` for `--raw .` to succeed.
    let tmp_state = tmp_tree.path().join(".fieldbook");
    copy_dir_all(&state_mini, &tmp_state).expect("copy state-mini into tmp_tree/.fieldbook");

    let tmp_home = tempfile::tempdir().expect("create tmp_home");

    // 2. Build the support catalogue with the pure producer
    let tree = fieldbook::io::read_tree(tmp_tree.path()).expect("read corpus-mini tree");
    let cat = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    )
    .expect("support catalogue from public source");
    let (cat_bytes, cat_sha) = cat.to_toml_bytes_and_sha256();

    let dist_dir = tmp_tree.path().join("out/dist");
    std::fs::create_dir_all(&dist_dir).expect("create out/dist");
    std::fs::write(dist_dir.join("corpus-support.toml"), &cat_bytes)
        .expect("write corpus-support.toml");
    let sha_line = format!("{cat_sha}  corpus-support.toml\n");
    std::fs::write(dist_dir.join("corpus-support.sha256"), sha_line)
        .expect("write corpus-support.sha256");

    // Track TMPDIR entries before execution
    let tmpdir = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);

    let initial_drill_dirs: BTreeSet<PathBuf> = std::fs::read_dir(&tmpdir)
        .ok()
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|name| name.starts_with("field book drill."))
                })
                .collect()
        })
        .unwrap_or_default();

    let _cleanup_guard = TempDrillDirGuard {
        tmpdir: &tmpdir,
        initial: &initial_drill_dirs,
    };

    // 3. Extract and assemble runbook shell blocks
    let script_body = extract_drill4_script(RUNBOOK).expect("extract Drill 4 script blocks");
    let preamble = r#"fieldbook() { "$FIELDBOOK_BIN" "$@"; }"#;
    let combined_script = format!("{{\n{preamble}\n\n{script_body}\n}} 2>&1\n");

    let fieldbook_bin = env!("CARGO_BIN_EXE_fieldbook");
    let mut cmd = Command::new("sh");
    cmd.arg("-c")
        .arg(&combined_script)
        .current_dir(tmp_tree.path())
        .env("FIELDBOOK_BIN", fieldbook_bin)
        .env("FIELDBOOK_STATE", &tmp_state)
        .env("HOME", tmp_home.path());

    let output = cmd.output().expect("execute shell drill script");
    let combined_output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // 4. Assert on combined output in order
    let needles = [
        "journal ok",
        "raw copy consistent",
        "public support catalogue verified:",
        "backup 1 written",
        "snapshot 1 verified",
        "snapshot hashes ok",
        "drill passed: check clean, journal ok, fingerprint",
        "FB0810",
        "exit 1",
        "corpus identical",
        "FB0815",
        "exit 1",
    ];

    let mut cursor = 0;
    for needle in needles {
        let match_pos = combined_output[cursor..].find(needle).unwrap_or_else(|| {
            panic!(
                "Did not find needle {:?} after byte offset {} in output.\nFull output:\n{}",
                needle, cursor, combined_output
            );
        });
        cursor += match_pos + needle.len();
    }

    // Assert on "drill passed:" line formatting
    let drill_line = combined_output
        .lines()
        .find(|l| l.contains("drill passed: check clean, journal ok, fingerprint"))
        .unwrap_or_else(|| {
            panic!(
                "Missing 'drill passed:' line in output.\nFull output:\n{}",
                combined_output
            );
        });

    let drill_trimmed = drill_line.trim();
    assert!(
        drill_trimmed.ends_with(" matches"),
        "Line must end with ' matches', got {:?}\nFull output:\n{}",
        drill_line,
        combined_output
    );

    let prefix = "drill passed: check clean, journal ok, fingerprint ";
    let suffix = " matches";
    let after_prefix = drill_trimmed.strip_prefix(prefix).unwrap_or_else(|| {
        panic!(
            "Expected prefix {:?} in {:?}\nFull output:\n{}",
            prefix, drill_line, combined_output
        );
    });
    let hex_part = after_prefix.strip_suffix(suffix).unwrap_or_else(|| {
        panic!(
            "Expected suffix {:?} in {:?}\nFull output:\n{}",
            suffix, drill_line, combined_output
        );
    });

    assert!(
        hex_part.len() == 12 && hex_part.chars().all(|c| c.is_ascii_hexdigit()),
        "Expected 12 hex characters before ' matches', got {:?}\nFull output:\n{}",
        hex_part,
        combined_output
    );

    // Assert tmp_state has no drill folder afterwards
    let drill_folder = tmp_state.join("drill");
    assert!(
        !drill_folder.exists(),
        "tmp_state must not contain 'drill' folder afterwards: {}\nFull output:\n{}",
        drill_folder.display(),
        combined_output
    );

    // Assert no leftover drill temp directory in TMPDIR
    let remaining_drill_dirs: Vec<PathBuf> = std::fs::read_dir(&tmpdir)
        .ok()
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|name| name.starts_with("field book drill."))
                        && !initial_drill_dirs.contains(p)
                })
                .collect()
        })
        .unwrap_or_default();

    assert!(
        remaining_drill_dirs.is_empty(),
        "Expected step 7 to clean up temp drill dir, but found leftovers in TMPDIR: {:?}\nFull output:\n{}",
        remaining_drill_dirs,
        combined_output
    );
}
