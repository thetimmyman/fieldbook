//! The built program, run as a process.

#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "these tests exist to start the real binary and prepare fixture directories"
)]

use std::collections::BTreeSet;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use clap::CommandFactory;
use fieldbook_core::check::{Code, Diagnostic, Entry, Report};
use fieldbook_core::date::CivilDate;
use fieldbook_core::strings;

const FIXTURE_TODAY: &str = include_str!("../../../tests/fixtures/corpus-mini/TODAY");
const ADR_0001: &str = include_str!("../../../docs/decisions/0001-foundation.md");

const NOT_BUILT: &[&str] = &[
    "fmt",
    "search",
    "status",
    "verify",
    "new page",
    "new claim",
    "new question",
    "new conflict",
    "new term",
    "new note",
    "audit export",
    "backup",
    "backup verify",
    "backup list",
    "restore",
    "context",
    "cite-check",
    "queue list",
    "queue show",
    "queue review",
    "queue accept",
    "queue reject",
    "queue ingest",
    "queue flag",
    "queue pin",
    "ask",
    "ai status",
    "ai test",
    "policy show",
    "policy check",
    "label add",
    "label reapply",
    "label show",
    "mcp",
    "decide",
    "chat",
    "init",
    "clean",
    "migrate",
    "sync",
];

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/corpus-mini")
}

fn fieldbook(args: &[&str]) -> std::io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_fieldbook"))
        .args(args)
        .output()
}

fn fieldbook_in(dir: &Path, args: &[&str]) -> std::io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_fieldbook"))
        .current_dir(dir)
        .args(args)
        .output()
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            std::fs::copy(entry.path(), dst_path)?;
        }
    }
    Ok(())
}

#[test]
fn version_prints_name_version_and_identity() -> Result<(), Box<dyn Error>> {
    let output = fieldbook(&["--version"])?;
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout)?;
    assert_eq!(stdout, format!("{}\n", fieldbook::version_line()));
    assert!(stdout.starts_with(&format!("{} ", strings::binary())));
    Ok(())
}

#[test]
fn unknown_arguments_print_usage_and_exit_two() -> Result<(), Box<dyn Error>> {
    for args in [&[][..], &["unknown-verb"], &["--version", "extra"]] {
        let output = fieldbook(args)?;
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr)?;
        assert!(
            stderr.to_ascii_lowercase().contains("usage: fieldbook"),
            "unexpected stderr for {args:?}: {stderr}"
        );
    }
    Ok(())
}

#[test]
fn read_tree_of_a_given_folder_matches_the_fixture() -> Result<(), Box<dyn Error>> {
    let root = fixture_root();
    let tree = fieldbook::io::read_tree(&root)?;

    let expected_toml = std::fs::read(root.join("corpus/corpus.toml"))?;
    assert_eq!(
        tree.get_bytes("corpus/corpus.toml"),
        Some(expected_toml.as_slice())
    );

    let page_rel = "corpus/pages/compliance/business-systems/cmp-review-steps.md";
    let expected_page = std::fs::read(root.join(page_rel))?;
    assert_eq!(tree.get_bytes(page_rel), Some(expected_page.as_slice()));

    assert!(tree.contains_dir("corpus"));
    assert!(tree.contains_dir("corpus/pages/compliance/business-systems"));
    assert!(tree.contains_dir("corpus/registers"));

    // Also verify that files under corpus/sources/public/ become Entry::Original hashed with Sha256Stream.
    let tmp = tempfile::tempdir()?;
    copy_dir_all(&root, tmp.path())?;
    let pub_dir = tmp.path().join("corpus/sources/public");
    std::fs::create_dir_all(&pub_dir)?;
    let orig_bytes = b"synthetic public source content\n";
    std::fs::write(pub_dir.join("S-0001-v1.txt"), orig_bytes)?;

    let tmp_tree = fieldbook::io::read_tree(tmp.path())?;
    assert!(tmp_tree.contains_dir("corpus/sources/public"));
    assert_eq!(
        tmp_tree.get("corpus/sources/public/S-0001-v1.txt"),
        Some(&Entry::Original {
            sha256: fieldbook_core::hash::sha256_hex(orig_bytes),
            len: orig_bytes.len() as u64,
        })
    );
    assert!(
        tmp_tree
            .get_bytes("corpus/sources/public/S-0001-v1.txt")
            .is_none()
    );
    Ok(())
}

#[test]
fn find_root_walks_up() -> Result<(), Box<dyn Error>> {
    let tmp = tempfile::tempdir()?;
    let deep = tmp.path().join("corpus/pages/fnd");
    std::fs::create_dir_all(&deep)?;
    assert_eq!(fieldbook::io::find_root(&deep), tmp.path());

    let outside = tempfile::tempdir()?;
    let leaf = outside.path().join("a/b");
    std::fs::create_dir_all(&leaf)?;
    assert_eq!(fieldbook::io::find_root(&leaf), leaf);
    Ok(())
}

#[test]
fn empty_originals_directory_is_retained() -> Result<(), Box<dyn Error>> {
    let tmp = tempfile::tempdir()?;
    std::fs::create_dir_all(tmp.path().join("corpus/sources/public"))?;
    let tree = fieldbook::io::read_tree(tmp.path())?;
    assert!(tree.contains_dir("corpus"));
    assert!(tree.contains_dir("corpus/sources"));
    assert!(tree.contains_dir("corpus/sources/public"));
    assert_eq!(tree.len(), 0);
    Ok(())
}

#[test]
fn check_process_tests() -> Result<(), Box<dyn Error>> {
    let today = FIXTURE_TODAY.trim();

    let empty = tempfile::tempdir()?;
    let out = fieldbook_in(empty.path(), &["check", "--today", today])?;
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8(out.stdout)?;
    assert!(
        stdout.starts_with("corpus/corpus.toml:1:1: error[FB0100]"),
        "unexpected stdout: {stdout}"
    );

    let root = fixture_root();
    let out = fieldbook_in(&root, &["check", "--today", today])?;
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout)?;
    assert!(stdout.contains("clean"), "unexpected stdout: {stdout}");

    let out = fieldbook_in(&root, &["check", "--json", "--today", today])?;
    assert_eq!(out.status.code(), Some(0));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    assert_eq!(json["report_version"], 1);
    assert_eq!(json["strict"], false);
    assert_eq!(json["today"], today);
    assert_eq!(json["errors"], 0);
    assert_eq!(json["warnings"], 0);
    assert_eq!(json["diagnostics"], serde_json::json!([]));

    let out = fieldbook_in(&root, &["check", "--today", "2025-02-30"])?;
    assert_eq!(out.status.code(), Some(2));
    assert!(!out.stderr.is_empty());

    let out = fieldbook_in(&root, &["check", "--strict", "--today", today])?;
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout)?;
    assert!(stdout.contains("clean"), "unexpected stdout: {stdout}");

    Ok(())
}

fn parse_ad17_verbs(doc: &str) -> Vec<&str> {
    let mut in_block = false;
    let mut out = Vec::new();
    for line in doc.lines() {
        let trimmed = line.trim();
        if !in_block {
            if trimmed == "```verbs" {
                in_block = true;
            }
        } else if trimmed == "```" {
            break;
        } else if !trimmed.is_empty() {
            out.push(trimmed);
        }
    }
    out
}

fn collect_clap_leaves(cmd: &clap::Command, prefix: &str, out: &mut Vec<String>) {
    for sub in cmd.get_subcommands() {
        let name = sub.get_name();
        if name == "help" {
            continue;
        }
        let path = if prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{prefix} {name}")
        };
        if sub.is_subcommand_required_set() || sub.has_subcommands() {
            collect_clap_leaves(sub, &path, out);
        } else {
            out.push(path);
        }
    }
}

fn check_verbs_match_ad17(
    clap_leaves: &[String],
    ad17_lines: &[&str],
    not_built: &[&str],
) -> Result<(), String> {
    let clap_set: BTreeSet<&str> = clap_leaves.iter().map(String::as_str).collect();
    let block_set: BTreeSet<&str> = ad17_lines.iter().copied().collect();
    let not_built_set: BTreeSet<&str> = not_built.iter().copied().collect();

    for &leaf in &clap_set {
        if !block_set.contains(leaf) {
            return Err(format!(
                "clap leaf command `{leaf}` is not in the AD-17 verbs block"
            ));
        }
    }

    for &nb in not_built {
        if !block_set.contains(nb) {
            return Err(format!(
                "NOT_BUILT entry `{nb}` is not in the AD-17 verbs block"
            ));
        }
        if clap_set.contains(nb) {
            return Err(format!("remove {nb} from NOT_BUILT"));
        }
    }

    for &line in ad17_lines {
        if !clap_set.contains(line) && !not_built_set.contains(line) {
            return Err(format!(
                "AD-17 verb `{line}` is neither built in clap nor listed in NOT_BUILT"
            ));
        }
    }

    Ok(())
}

#[test]
fn verbs_match_ad17() {
    let ad17 = parse_ad17_verbs(ADR_0001);
    assert!(
        !ad17.is_empty(),
        "failed to find ```verbs block in ADR 0001"
    );
    let mut leaves = Vec::new();
    collect_clap_leaves(&fieldbook::Cli::command(), "", &mut leaves);
    if let Err(msg) = check_verbs_match_ad17(&leaves, &ad17, NOT_BUILT) {
        panic!("{msg}");
    }

    // Verify failure modes on synthetic inputs:
    let mut extra_leaves = leaves.clone();
    extra_leaves.push("hello".to_owned());
    let err = check_verbs_match_ad17(&extra_leaves, &ad17, NOT_BUILT).unwrap_err();
    assert!(err.contains("hello"), "{err}");

    let without_check: Vec<&str> = ad17.iter().copied().filter(|&v| v != "check").collect();
    let err = check_verbs_match_ad17(&leaves, &without_check, NOT_BUILT).unwrap_err();
    assert!(err.contains("check"), "{err}");

    let mut stale_not_built = NOT_BUILT.to_vec();
    stale_not_built.push("check");
    let err = check_verbs_match_ad17(&leaves, &ad17, &stale_not_built).unwrap_err();
    assert_eq!(err, "remove check from NOT_BUILT");
}

#[test]
fn pass_notes_survive_json() -> Result<(), Box<dyn Error>> {
    let mut report = Report::new();
    report.push_note("originals check skipped (no corpus/sources/public/)");
    let today = CivilDate::parse(FIXTURE_TODAY.trim())?;
    let rendered = fieldbook::render_check_json(&report, false, today);
    let parsed: serde_json::Value = serde_json::from_str(&rendered)?;
    assert_eq!(parsed["errors"], 0);
    assert_eq!(parsed["warnings"], 0);
    assert_eq!(parsed["diagnostics"], serde_json::json!([]));
    let notes = parsed["notes"].as_array().expect("notes array");
    assert_eq!(notes.len(), 1);
    assert_eq!(
        notes[0],
        "originals check skipped (no corpus/sources/public/)"
    );

    let mut report_with_diag = Report::new();
    report_with_diag.push_note("note-1");
    report_with_diag.push(Diagnostic::new(
        Code::Fb0100,
        false,
        "corpus/corpus.toml",
        1,
        1,
        "missing corpus/corpus.toml",
    ));
    let rendered_diag = fieldbook::render_check_json(&report_with_diag, false, today);
    let parsed_diag: serde_json::Value = serde_json::from_str(&rendered_diag)?;
    assert_eq!(parsed_diag["errors"], 1);
    assert_eq!(parsed_diag["warnings"], 0);
    assert_eq!(parsed_diag["notes"], serde_json::json!(["note-1"]));
    Ok(())
}

#[test]
fn missing_nested_corpus_config_does_not_select_ancestor() -> Result<(), Box<dyn Error>> {
    let today = FIXTURE_TODAY.trim();
    let outer = tempfile::tempdir()?;
    copy_dir_all(&fixture_root(), outer.path())?;

    let inner = outer.path().join("nested/work");
    std::fs::create_dir_all(inner.join("corpus/pages"))?;

    let out = fieldbook_in(&inner, &["check", "--today", today])?;
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8(out.stdout)?;
    assert!(
        stdout.starts_with("corpus/corpus.toml:1:1: error[FB0100]"),
        "expected inner FB0100, got: {stdout}"
    );
    Ok(())
}

#[test]
fn check_paths_are_relative_to_cwd() -> Result<(), Box<dyn Error>> {
    let today = FIXTURE_TODAY.trim();
    let tmp = tempfile::tempdir()?;
    let corpus_dir = tmp.path().join("corpus");
    std::fs::create_dir_all(&corpus_dir)?;
    std::fs::write(corpus_dir.join("corpus.toml"), b"schema = 999\n")?;

    // Running `check corpus.toml` from `tmp` refers to `tmp/corpus.toml`, not `tmp/corpus/corpus.toml`.
    let out_root_text = fieldbook_in(tmp.path(), &["check", "corpus.toml", "--today", today])?;
    assert_eq!(out_root_text.status.code(), Some(0));
    let stdout_root = String::from_utf8(out_root_text.stdout)?;
    assert!(
        stdout_root.contains("clean"),
        "unexpected root stdout: {stdout_root}"
    );

    let out_root_json = fieldbook_in(
        tmp.path(),
        &["check", "--json", "corpus.toml", "--today", today],
    )?;
    assert_eq!(out_root_json.status.code(), Some(0));
    let json_root: serde_json::Value = serde_json::from_slice(&out_root_json.stdout)?;
    assert_eq!(json_root["errors"], 0);
    assert_eq!(json_root["warnings"], 0);
    assert_eq!(json_root["diagnostics"], serde_json::json!([]));

    // Running `check corpus.toml` from `tmp/corpus` refers to `tmp/corpus/corpus.toml` and includes FB0100.
    let out_sub_text = fieldbook_in(&corpus_dir, &["check", "corpus.toml", "--today", today])?;
    assert_eq!(out_sub_text.status.code(), Some(1));
    let stdout_sub = String::from_utf8(out_sub_text.stdout)?;
    assert!(
        stdout_sub.starts_with("corpus/corpus.toml:1:1: error[FB0100]"),
        "unexpected subdir stdout: {stdout_sub}"
    );

    let out_sub_json = fieldbook_in(
        &corpus_dir,
        &["check", "--json", "corpus.toml", "--today", today],
    )?;
    assert_eq!(out_sub_json.status.code(), Some(1));
    let json_sub: serde_json::Value = serde_json::from_slice(&out_sub_json.stdout)?;
    assert_eq!(json_sub["errors"], 1);
    assert_eq!(json_sub["warnings"], 0);
    assert_eq!(
        json_sub["diagnostics"]
            .as_array()
            .expect("diagnostics array")
            .len(),
        1
    );
    Ok(())
}

#[test]
fn path_filter_uses_components() -> Result<(), Box<dyn Error>> {
    let today = FIXTURE_TODAY.trim();
    let tmp = tempfile::tempdir()?;
    std::fs::create_dir_all(tmp.path().join("corpus"))?;

    // Exact file, parent directory, and lexically normalized `.` / `..` components match.
    for matching in [
        "corpus/corpus.toml",
        "corpus",
        "./corpus/./corpus.toml",
        "corpus/pages/../corpus.toml",
    ] {
        let out = fieldbook_in(tmp.path(), &["check", matching, "--today", today])?;
        assert_eq!(
            out.status.code(),
            Some(1),
            "expected filter `{matching}` to match corpus/corpus.toml"
        );
    }

    // Partial-component prefixes must not match.
    for non_matching in ["cor", "corpus/corpus.tom", "corpus/corpus.toml.bak"] {
        let out = fieldbook_in(tmp.path(), &["check", non_matching, "--today", today])?;
        assert_eq!(
            out.status.code(),
            Some(0),
            "expected filter `{non_matching}` NOT to match corpus/corpus.toml"
        );
    }

    Ok(())
}
