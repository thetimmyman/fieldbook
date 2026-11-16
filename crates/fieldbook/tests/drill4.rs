//! Integration test for Restore Drill 4 from docs/runbooks/restore-drill.md.

#![cfg(unix)]
#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "drill4 integration test invokes the built binary and shell tools"
)]

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

fn run_drill4_with_outer_tmpdir(
    outer_tmpdir: &Path,
    after_tmpdir_setup: impl FnOnce(),
) -> Result<(), String> {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus_mini = repo_root.join("tests/fixtures/corpus-mini");
    let state_mini = repo_root.join("tests/fixtures/state-mini");

    // 1. Build disposable synthetic/public tree and state
    let tmp_tree = tempfile::tempdir().map_err(|e| format!("create tmp_tree: {e}"))?;
    copy_dir_all(&corpus_mini, tmp_tree.path()).map_err(|e| format!("copy corpus-mini: {e}"))?;

    // The runbook Drill 4 Step 1 calls `fieldbook backup verify --raw .`.
    // Verification of a raw directory looks for state/audit under the specified directory
    // (e.g. `<raw_dir>/.fieldbook/audit`). Thus, the state must live inside the tree at
    // `.fieldbook` for `--raw .` to succeed.
    let tmp_state = tmp_tree.path().join(".fieldbook");
    copy_dir_all(&state_mini, &tmp_state)
        .map_err(|e| format!("copy state-mini into tmp_tree/.fieldbook: {e}"))?;

    let tmp_home = tempfile::tempdir().map_err(|e| format!("create tmp_home: {e}"))?;

    // 2. Build the support catalogue with the pure producer
    let tree =
        fieldbook::io::read_tree(tmp_tree.path()).map_err(|e| format!("read corpus-mini: {e}"))?;
    let cat = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    )
    .map_err(|e| format!("support catalogue from public source: {e}"))?;
    let (cat_bytes, cat_sha) = cat.to_toml_bytes_and_sha256();

    let dist_dir = tmp_tree.path().join("out/dist");
    std::fs::create_dir_all(&dist_dir).map_err(|e| format!("create out/dist: {e}"))?;
    std::fs::write(dist_dir.join("corpus-support.toml"), &cat_bytes)
        .map_err(|e| format!("write corpus-support.toml: {e}"))?;
    let sha_line = format!("{cat_sha}  corpus-support.toml\n");
    std::fs::write(dist_dir.join("corpus-support.sha256"), sha_line)
        .map_err(|e| format!("write corpus-support.sha256: {e}"))?;

    // Isolate TMPDIR per Drill 4 invocation inside `outer_tmpdir` so concurrent test runs
    // sharing an outer TMPDIR never observe or delete each other's `field book drill.XXXXXX` dirs.
    std::fs::create_dir_all(outer_tmpdir).map_err(|e| format!("create outer_tmpdir: {e}"))?;
    let isolated_tmpdir = tempfile::tempdir_in(outer_tmpdir)
        .map_err(|e| format!("create isolated drill4 tmpdir: {e}"))?;

    after_tmpdir_setup();

    // 3. Extract and assemble runbook shell blocks
    let script_body =
        extract_drill4_script(RUNBOOK).map_err(|e| format!("extract Drill 4 script: {e}"))?;
    let preamble = r#"fieldbook() { "$FIELDBOOK_BIN" "$@"; }"#;
    let combined_script = format!("{{\n{preamble}\n\n{script_body}\n}} 2>&1\n");

    let fieldbook_bin = env!("CARGO_BIN_EXE_fieldbook");
    let mut cmd = Command::new("sh");
    cmd.arg("-c")
        .arg(&combined_script)
        .current_dir(tmp_tree.path())
        .env("TMPDIR", isolated_tmpdir.path())
        .env("FIELDBOOK_BIN", fieldbook_bin)
        .env("FIELDBOOK_STATE", &tmp_state)
        .env("HOME", tmp_home.path());

    let output = cmd
        .output()
        .map_err(|e| format!("execute shell drill script: {e}"))?;
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
        "ok: snapshot 1 verified",
        "ok: restored ",
        "ok: check --strict clean",
        "ok: journal ok (seq ",
        "ok: fingerprint ",
        "drill passed: check clean, journal ok, fingerprint",
        "FB0810",
        "exit 1",
        "corpus identical",
        "FB0815",
        "exit 1",
    ];

    let mut cursor = 0;
    for needle in needles {
        let Some(match_pos) = combined_output[cursor..].find(needle) else {
            return Err(format!(
                "Did not find needle {needle:?} after byte offset {cursor} in output.\nFull output:\n{combined_output}"
            ));
        };
        cursor += match_pos + needle.len();
    }

    // Assert on "drill passed:" line formatting
    let Some(drill_line) = combined_output
        .lines()
        .find(|l| l.contains("drill passed: check clean, journal ok, fingerprint"))
    else {
        return Err(format!(
            "Missing 'drill passed:' line in output.\nFull output:\n{combined_output}"
        ));
    };

    let drill_trimmed = drill_line.trim();
    if !drill_trimmed.ends_with(" matches") {
        return Err(format!(
            "Line must end with ' matches', got {drill_line:?}\nFull output:\n{combined_output}"
        ));
    }

    let prefix = "drill passed: check clean, journal ok, fingerprint ";
    let suffix = " matches";
    let Some(after_prefix) = drill_trimmed.strip_prefix(prefix) else {
        return Err(format!(
            "Expected prefix {prefix:?} in {drill_line:?}\nFull output:\n{combined_output}"
        ));
    };
    let Some(hex_part) = after_prefix.strip_suffix(suffix) else {
        return Err(format!(
            "Expected suffix {suffix:?} in {drill_line:?}\nFull output:\n{combined_output}"
        ));
    };

    if hex_part.len() != 12 || !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "Expected 12 hex characters before ' matches', got {hex_part:?}\nFull output:\n{combined_output}"
        ));
    }

    // Assert tmp_state has no drill folder afterwards
    let drill_folder = tmp_state.join("drill");
    if drill_folder.exists() {
        return Err(format!(
            "tmp_state must not contain 'drill' folder afterwards: {}\nFull output:\n{combined_output}",
            drill_folder.display()
        ));
    }

    // Assert Step 7 cleaned up its `field book drill.*` temp directory inside `isolated_tmpdir`
    let remaining_drill_dirs: Vec<PathBuf> = std::fs::read_dir(isolated_tmpdir.path())
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

    if !remaining_drill_dirs.is_empty() {
        return Err(format!(
            "Expected step 7 to clean up temp drill dir, but found leftovers in TMPDIR: {remaining_drill_dirs:?}\nFull output:\n{combined_output}"
        ));
    }

    Ok(())
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

    let tmpdir = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);

    if let Err(err) = run_drill4_with_outer_tmpdir(&tmpdir, || {}) {
        panic!("{err}");
    }
}

#[expect(
    clippy::print_stderr,
    reason = "contract requires eprintln! skip message when tools are missing"
)]
#[test]
fn restore_drill4_shared_tmpdir_isolates_concurrent_siblings() {
    if !tool_available("sha256sum") {
        eprintln!("skipping drill4: sha256sum tool not available");
        return;
    }
    if !tool_available("diff") {
        eprintln!("skipping drill4: diff tool not available");
        return;
    }

    let shared_outer = tempfile::tempdir().expect("create shared outer tmpdir");
    let sibling_drill_dir = shared_outer.path().join("field book drill.sibling");
    let sibling_target = sibling_drill_dir.join("target/sentinel.txt");

    let res = run_drill4_with_outer_tmpdir(shared_outer.path(), || {
        std::fs::create_dir_all(sibling_drill_dir.join("target"))
            .expect("create concurrent sibling drill dir");
        std::fs::write(&sibling_target, b"live-sibling\n").expect("write sibling sentinel");
    });

    assert!(
        sibling_target.is_file(),
        "concurrent sibling drill directory in shared TMPDIR was deleted by drill4 cleanup"
    );
    if let Err(err) = res {
        panic!(
            "drill4 failed when a concurrent sibling drill directory existed in shared TMPDIR:\n{err}"
        );
    }

    let outer_path = shared_outer.path();
    let (r1, r2) = std::thread::scope(|s| {
        let t1 = s.spawn(|| run_drill4_with_outer_tmpdir(outer_path, || {}));
        let t2 = s.spawn(|| run_drill4_with_outer_tmpdir(outer_path, || {}));
        (
            t1.join().expect("join first concurrent drill4"),
            t2.join().expect("join second concurrent drill4"),
        )
    });
    if let Err(err) = r1 {
        panic!("first concurrent drill4 failed under shared TMPDIR:\n{err}");
    }
    if let Err(err) = r2 {
        panic!("second concurrent drill4 failed under shared TMPDIR:\n{err}");
    }
    assert!(
        sibling_target.is_file(),
        "concurrent sibling sentinel must remain intact after parallel drill4 runs"
    );
}

#[test]
fn restore_drill_cli_fails_on_appended_csv_byte_and_leaves_no_drill_folder() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus_mini = repo_root.join("tests/fixtures/corpus-mini");
    let state_mini = repo_root.join("tests/fixtures/state-mini");

    let tmp_tree = tempfile::tempdir().expect("create tmp_tree");
    copy_dir_all(&corpus_mini, tmp_tree.path()).expect("copy corpus-mini");
    let tmp_state = tmp_tree.path().join(".fieldbook");
    copy_dir_all(&state_mini, &tmp_state).expect("copy state-mini");
    let tmp_home = tempfile::tempdir().expect("create tmp_home");
    let target_dir = tmp_tree.path().join("target");
    std::fs::create_dir_all(&target_dir).expect("create target_dir");

    let tree = fieldbook::io::read_tree(tmp_tree.path()).expect("read corpus-mini tree");
    let cat = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    )
    .expect("support catalogue from public source");
    let (cat_bytes, cat_sha) = cat.to_toml_bytes_and_sha256();
    let cat_path = tmp_tree.path().join("corpus-support.toml");
    std::fs::write(&cat_path, &cat_bytes).expect("write corpus-support.toml");

    let fieldbook_bin = env!("CARGO_BIN_EXE_fieldbook");
    let run_cli = |args: &[&str]| {
        Command::new(fieldbook_bin)
            .args(args)
            .current_dir(tmp_tree.path())
            .env("FIELDBOOK_STATE", &tmp_state)
            .env("HOME", tmp_home.path())
            .output()
            .expect("run fieldbook cli")
    };

    let raw_verify = run_cli(&[
        "backup",
        "verify",
        "--raw",
        ".",
        "--support-catalogue",
        cat_path.to_str().expect("utf8 path"),
        "--expect-support-sha256",
        &cat_sha,
    ]);
    assert!(
        raw_verify.status.success(),
        "backup verify --raw failed: {}",
        String::from_utf8_lossy(&raw_verify.stderr)
    );

    let backup_out = run_cli(&["backup", "--to", target_dir.to_str().expect("utf8 path")]);
    assert!(
        backup_out.status.success(),
        "backup --to failed: {}",
        String::from_utf8_lossy(&backup_out.stderr)
    );

    let csv_rel = "data/corpus/registers/claims.csv";
    let csv_path = target_dir.join("fieldbook-backup-0001").join(csv_rel);
    let mut csv_bytes = std::fs::read(&csv_path).expect("read claims.csv");
    csv_bytes.push(b'x');
    std::fs::write(&csv_path, &csv_bytes).expect("append byte to claims.csv");

    let drill_out = run_cli(&[
        "restore",
        "--snapshot",
        "1",
        "--from",
        target_dir.to_str().expect("utf8 path"),
        "--drill",
    ]);
    assert_eq!(
        drill_out.status.code(),
        Some(1),
        "expected exit 1 on appended csv byte, stdout: {}, stderr: {}",
        String::from_utf8_lossy(&drill_out.stdout),
        String::from_utf8_lossy(&drill_out.stderr)
    );
    let stderr = String::from_utf8_lossy(&drill_out.stderr);
    assert!(
        stderr.contains("FB0810") && stderr.contains(csv_rel),
        "expected FB0810 naming {csv_rel}, got stderr:\n{stderr}"
    );
    assert!(
        !tmp_state.join("drill").exists(),
        "drill folder must not exist after failed drill"
    );
}
