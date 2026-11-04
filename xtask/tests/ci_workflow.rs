//! CI workflow and `.github/scripts/changes.sh` tests (`check-ci-and-docs`).

#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "CI workflow test reads .github/workflows/ci.yml and runs git/bash in a tempdir"
)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn check_ci_ok_needs_every_job(yaml: &str) -> Result<(), String> {
    let mut in_jobs = false;
    let mut current_job: Option<String> = None;
    let mut jobs = Vec::new();
    let mut ci_ok_needs_line: Option<String> = None;

    for raw_line in yaml.lines() {
        let line = raw_line.trim_end_matches('\r');
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if !in_jobs {
            if line == "jobs:" {
                in_jobs = true;
            }
            continue;
        }
        if !line.starts_with(' ') {
            break;
        }
        if let Some(rest) = line.strip_prefix("  ")
            && !rest.starts_with(' ')
            && let Some(job_id) = rest.strip_suffix(':')
        {
            let id = job_id.trim().to_owned();
            current_job = Some(id.clone());
            jobs.push(id);
            continue;
        }
        if current_job.as_deref() == Some("ci-ok")
            && let Some(rest) = line.strip_prefix("    needs:")
        {
            ci_ok_needs_line = Some(rest.trim().to_owned());
        }
    }

    let Some(needs_raw) = ci_ok_needs_line else {
        return Err("needs of ci-ok must be a one-line list".to_owned());
    };
    let Some(inner) = needs_raw
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
    else {
        return Err("needs of ci-ok must be a one-line list".to_owned());
    };
    let listed: BTreeSet<&str> = inner
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if listed.is_empty() {
        return Err("needs of ci-ok must be a one-line list".to_owned());
    }

    let missing: Vec<&str> = jobs
        .iter()
        .map(String::as_str)
        .filter(|&j| j != "ci-ok" && !listed.contains(j))
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "ci-ok needs list is missing job(s): {}",
            missing.join(", ")
        ));
    }

    Ok(())
}

fn run_git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to run git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn run_changes_sh(repo: &Path, script: &Path, base: &str) -> String {
    let out = Command::new("bash")
        .current_dir(repo)
        .arg(script)
        .arg(base)
        .output()
        .unwrap_or_else(|e| panic!("failed to run changes.sh: {e}"));
    assert!(
        out.status.success(),
        "changes.sh failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn write_repo_file(repo: &Path, rel: &str, content: &str) {
    let full = repo.join(rel);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).unwrap_or_else(|e| panic!("mkdir {rel}: {e}"));
    }
    std::fs::write(&full, content).unwrap_or_else(|e| panic!("write {rel}: {e}"));
}

#[test]
fn ci_ok_needs_every_job() {
    let path = repo_root().join(".github/workflows/ci.yml");
    let yaml =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    if let Err(msg) = check_ci_ok_needs_every_job(&yaml) {
        panic!("{msg}");
    }

    // Verify multiline needs list fails with the exact required message:
    let multiline = yaml.replace(
        "needs: [rust, corpus, changes]",
        "needs:\n      - rust\n      - corpus\n      - changes",
    );
    let err = check_ci_ok_needs_every_job(&multiline).unwrap_err();
    assert_eq!(err, "needs of ci-ok must be a one-line list");
}

#[test]
fn changes_script_classifies_paths() {
    let script = repo_root().join(".github/scripts/changes.sh");
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path();

    run_git(repo, &["init", "-q"]);
    run_git(repo, &["config", "user.name", "Test"]);
    run_git(repo, &["config", "user.email", "test@example.invalid"]);
    run_git(repo, &["config", "diff.renames", "true"]);

    write_repo_file(
        repo,
        "crates/orig/src/lib.rs",
        "pub fn sample() -> u32 { 42 }\n",
    );
    write_repo_file(
        repo,
        "corpus/pages/orig.md",
        "# Original Corpus Page\n\nBody text for rename check.\n",
    );
    run_git(repo, &["add", "."]);
    run_git(repo, &["commit", "-q", "-m", "base"]);
    let base_sha = run_git(repo, &["rev-parse", "HEAD"]);

    type CaseSpec<'a> = (&'a str, &'a [(&'a str, &'a str)], &'a str);
    let cases: &[CaseSpec<'_>] = &[
        (
            "crates/x/src/lib.rs",
            &[("crates/x/src/lib.rs", "// code\n")],
            "code=true\ncorpus=false\n",
        ),
        (
            "docs/a.md and README.md",
            &[("docs/a.md", "# Doc\n"), ("README.md", "# Readme\n")],
            "code=false\ncorpus=false\n",
        ),
        (
            "corpus/pages/a.md",
            &[("corpus/pages/a.md", "# Page\n")],
            "code=false\ncorpus=true\n",
        ),
        (
            "corpus/registers/a.csv with justfile",
            &[
                ("corpus/registers/a.csv", "id\n"),
                ("justfile", "default:\n"),
            ],
            "code=true\ncorpus=true\n",
        ),
        (
            ".github/workflows/ci.yml",
            &[(".github/workflows/ci.yml", "name: ci\n")],
            "code=true\ncorpus=false\n",
        ),
        (
            "tests/fixtures/sample.md",
            &[("tests/fixtures/sample.md", "# Fixture\n")],
            "code=true\ncorpus=false\n",
        ),
        (
            "crates/fieldbook/tests/sample.md",
            &[("crates/fieldbook/tests/sample.md", "# Crate fixture\n")],
            "code=true\ncorpus=false\n",
        ),
        (
            "xtask/tests/sample.md",
            &[("xtask/tests/sample.md", "# Xtask fixture\n")],
            "code=true\ncorpus=false\n",
        ),
    ];

    for &(label, files, expected) in cases {
        run_git(repo, &["reset", "-q", "--hard", &base_sha]);
        run_git(repo, &["clean", "-fdq"]);
        for &(rel, content) in files {
            write_repo_file(repo, rel, content);
        }
        run_git(repo, &["add", "."]);
        run_git(repo, &["commit", "-q", "-m", label]);
        let actual = run_changes_sh(repo, &script, &base_sha);
        assert_eq!(actual, expected, "case `{label}` failed");
    }

    // Unchanged-content code-to-docs rename with diff.renames=true:
    run_git(repo, &["reset", "-q", "--hard", &base_sha]);
    run_git(repo, &["clean", "-fdq"]);
    std::fs::create_dir_all(repo.join("docs")).expect("mkdir docs");
    run_git(
        repo,
        &["mv", "crates/orig/src/lib.rs", "docs/renamed-from-code.md"],
    );
    run_git(repo, &["commit", "-q", "-m", "rename code to docs"]);
    let actual_code_rename = run_changes_sh(repo, &script, &base_sha);
    assert_eq!(
        actual_code_rename, "code=true\ncorpus=false\n",
        "code-to-docs rename failed"
    );

    // Unchanged-content Corpus-to-docs rename with diff.renames=true:
    run_git(repo, &["reset", "-q", "--hard", &base_sha]);
    run_git(repo, &["clean", "-fdq"]);
    std::fs::create_dir_all(repo.join("docs")).expect("mkdir docs");
    run_git(
        repo,
        &["mv", "corpus/pages/orig.md", "docs/renamed-from-corpus.md"],
    );
    run_git(repo, &["commit", "-q", "-m", "rename corpus to docs"]);
    let actual_corpus_rename = run_changes_sh(repo, &script, &base_sha);
    assert_eq!(
        actual_corpus_rename, "code=false\ncorpus=true\n",
        "corpus-to-docs rename failed"
    );
}

#[test]
fn changes_script_fails_open() {
    let script = repo_root().join(".github/scripts/changes.sh");
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path();

    run_git(repo, &["init", "-q"]);
    run_git(repo, &["config", "user.name", "Test"]);
    run_git(repo, &["config", "user.email", "test@example.invalid"]);
    write_repo_file(repo, "README.md", "# Test\n");
    run_git(repo, &["add", "."]);
    run_git(repo, &["commit", "-q", "-m", "init"]);

    for bad_base in [
        "",
        "0000000000000000000000000000000000000000",
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
    ] {
        let out = run_changes_sh(repo, &script, bad_base);
        assert_eq!(
            out, "code=true\ncorpus=true\n",
            "expected fail-open for base `{bad_base}`"
        );
    }
}
