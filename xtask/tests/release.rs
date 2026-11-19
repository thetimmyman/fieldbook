//! Integration tests for release preparation, tag verification, and release workflow
//! (`docs/controls.md` RL-01, `docs/runbooks/release.md`).

#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "integration test builds synthetic git repositories and invokes the xtask binary"
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use fieldbook_core::hash::sha256_hex;

static COUNTER: AtomicU64 = AtomicU64::new(1);

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| panic!("xtask lives under repo root"))
        .to_path_buf()
}

fn xtask_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_xtask"))
}

type CompatSpec<'a> = (&'a str, &'a str, &'a [(String, String)]);

struct Repo {
    dir: PathBuf,
}

impl Repo {
    fn new(label: &str) -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "fieldbook-release-test-{label}-{}-{id}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("create temp repo dir: {e}"));
        Self { dir }
    }

    fn init(&self) {
        self.git(&["init", "-q", "-b", "main"]);
        self.git(&["config", "user.name", "Synthetic Test"]);
        self.git(&["config", "user.email", "synthetic@example.invalid"]);
        self.git(&["config", "commit.gpgsign", "false"]);
        self.git(&["config", "tag.gpgsign", "false"]);
    }

    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(&self.dir)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap_or_else(|e| panic!("git runs: {e}"));
        assert!(
            out.status.success(),
            "git {args:?} failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    fn write(&self, rel: &str, content: &str) {
        let path = self.dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap_or_else(|e| panic!("create parent: {e}"));
        }
        std::fs::write(&path, content).unwrap_or_else(|e| panic!("write file: {e}"));
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.dir.join(rel)).unwrap_or_else(|e| panic!("read file: {e}"))
    }

    fn sha256(&self, rel: &str) -> String {
        let bytes = std::fs::read(self.dir.join(rel)).unwrap_or_else(|e| panic!("read bytes: {e}"));
        sha256_hex(&bytes)
    }

    fn commit_all(&self, msg: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", msg, "--allow-empty"]);
        self.git(&["rev-parse", "HEAD"])
    }

    fn set_origin_main(&self, sha: &str) {
        self.git(&["update-ref", "refs/remotes/origin/main", sha]);
    }

    fn tag(&self, name: &str) -> String {
        self.git(&["tag", name]);
        self.git(&["rev-parse", &format!("refs/tags/{name}^{{commit}}")])
    }

    fn checkout(&self, rev: &str) {
        self.git(&["checkout", "-q", "--detach", rev]);
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(xtask_bin())
            .args(args)
            .current_dir(&self.dir)
            .env("XTASK_REPO_ROOT", &self.dir)
            .output()
            .unwrap_or_else(|e| panic!("run xtask binary: {e}"))
    }

    fn write_minimal_files(&self, version: &str) {
        self.write(
            "Cargo.toml",
            &format!("[workspace.package]\nversion = \"{version}\"\n"),
        );
        self.write("Cargo.lock", "version = 4\n");
        self.write(
            "CHANGELOG.md",
            "# Changelog\n\n## Unreleased\n\n- Added: x.\n",
        );
        self.write_empty_baseline();
    }

    fn write_empty_baseline(&self) {
        self.write(
            "release-baseline.toml",
            "baseline_schema = 1\n\n[compatibility]\nkind = \"empty\"\n\n[tool]\nkind = \"empty\"\n\n[edition]\nkind = \"empty\"\n",
        );
    }

    fn copy_from_workspace(&self) {
        let src = repo_root();
        let files = [
            "compat/published.toml",
            "compat/calculation/build_up@1/contract.toml",
            "compat/calculation/build_up@1/calc-01.in.toml",
            "compat/calculation/build_up@1/calc-01.out.txt",
            "compat/calculation/build_up@1/calc-90.in.toml",
            "compat/calculation/build_up@1/calc-90.out.txt",
            "compat/calculation/children_sum@1/contract.toml",
            "compat/calculation/children_sum@1/calc-04.in.toml",
            "compat/calculation/children_sum@1/calc-04.out.txt",
            "compat/calculation/children_sum@1/calc-91.in.toml",
            "compat/calculation/children_sum@1/calc-91.out.txt",
            "compat/run-schema/canonical-run@1/contract.toml",
            "compat/run-schema/canonical-run@1/calc-01.in.toml",
            "compat/run-schema/canonical-run@1/calc-01.out.txt",
            "crates/fieldbook-calc/src/fns.rs",
            "crates/fieldbook-calc/src/versions/build_up_v1.rs",
            "crates/fieldbook-calc/src/versions/children_sum_v1.rs",
            "crates/fieldbook-calc/src/versions/run_v1.rs",
            "crates/fieldbook-calc/tests/cases/EPOCH",
            "crates/fieldbook-calc/tests/cases/calc-01.toml",
            "crates/fieldbook-calc/tests/cases/calc-02.toml",
            "crates/fieldbook-calc/tests/cases/calc-03.toml",
            "crates/fieldbook-calc/tests/cases/calc-04.toml",
            "crates/fieldbook-calc/tests/cases/calc-90.toml",
        ];
        for rel in files {
            let from = src.join(rel);
            let to = self.dir.join(rel);
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent)
                    .unwrap_or_else(|e| panic!("create parent dirs: {e}"));
            }
            std::fs::copy(&from, &to).unwrap_or_else(|e| panic!("copy {rel}: {e}"));
        }

        for dir in [
            "compat",
            "crates/fieldbook-core/src/versions",
            "crates/fieldbook-core/src/ports/templates",
            "crates/fieldbook/src/io/versions",
            "xtask/src/versions",
        ] {
            copy_tree(&src.join(dir), &self.dir.join(dir));
        }
    }

    fn compatibility_files_from_published(&self) -> Vec<(String, String)> {
        let pub_text = self.read("compat/published.toml");
        let parsed: toml::Value =
            toml::from_str(&pub_text).unwrap_or_else(|e| panic!("valid published.toml: {e}"));
        let mut map = BTreeMap::new();
        map.insert(
            "compat/published.toml".to_owned(),
            self.sha256("compat/published.toml"),
        );
        if let Some(bindings) = parsed.get("binding").and_then(toml::Value::as_array) {
            for b in bindings {
                if let Some(files) = b.get("files").and_then(toml::Value::as_array) {
                    for f in files {
                        let p = f["path"]
                            .as_str()
                            .unwrap_or_else(|| panic!("path string"))
                            .to_owned();
                        let s = f["sha256"]
                            .as_str()
                            .unwrap_or_else(|| panic!("sha256 string"))
                            .to_owned();
                        map.insert(p, s);
                    }
                }
            }
        }
        map.into_iter().collect()
    }

    fn write_release_baseline(
        &self,
        compat: Option<CompatSpec<'_>>,
        tool: Option<(&str, &str)>,
        edition: Option<(&str, &str)>,
    ) {
        let dummy_manifest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let mut out = String::from("baseline_schema = 1\n\n[compatibility]\n");
        if let Some((tag, commit, files)) = compat {
            out.push_str(&format!(
                "kind = \"release\"\ntag = \"{tag}\"\ncommit = \"{commit}\"\narchive_manifest_sha256 = \"{dummy_manifest}\"\n\n"
            ));
            for (path, sha) in files {
                out.push_str(&format!(
                    "[[compatibility.file]]\npath = \"{path}\"\nsha256 = \"{sha}\"\n\n"
                ));
            }
        } else {
            out.push_str("kind = \"empty\"\n\n");
        }

        out.push_str("[tool]\n");
        if let Some((tag, commit)) = tool {
            out.push_str(&format!(
                "kind = \"release\"\ntag = \"{tag}\"\ncommit = \"{commit}\"\narchive_manifest_sha256 = \"{dummy_manifest}\"\n\n"
            ));
        } else {
            out.push_str("kind = \"empty\"\n\n");
        }

        out.push_str("[edition]\n");
        if let Some((tag, commit)) = edition {
            out.push_str(&format!(
                "kind = \"release\"\ntag = \"{tag}\"\ncommit = \"{commit}\"\narchive_manifest_sha256 = \"{dummy_manifest}\"\n"
            ));
        } else {
            out.push_str("kind = \"empty\"\n");
        }

        self.write("release-baseline.toml", &out);
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn assert_refusal(out: &Output, rule: &str, expected_substring: &str) {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected exit code 1:\nstdout: {stdout}\nstderr: {stderr}"
    );
    if !rule.is_empty() {
        assert!(
            stderr.contains(rule),
            "expected stderr to contain rule `{rule}`:\nstdout: {stdout}\nstderr: {stderr}"
        );
    }
    if !expected_substring.is_empty() {
        assert!(
            stderr.contains(expected_substring),
            "expected stderr to contain `{expected_substring}`:\nstdout: {stdout}\nstderr: {stderr}"
        );
    }
}

fn assert_success(out: &Output) {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "expected command success:\nstdout: {stdout}\nstderr: {stderr}"
    );
}

fn make_edition_repo(edition_str: &str, descriptor_content: Option<&str>) -> Repo {
    let repo = Repo::new(&format!("edition-{edition_str}"));
    repo.init();
    repo.copy_from_workspace();
    repo.write_minimal_files("0.0.0");
    repo.write(
        "corpus/changes.md",
        &format!("# Corpus changes\n\n## {edition_str}\n\n- Added: x.\n"),
    );
    if let Some(desc) = descriptor_content {
        repo.write("edition-release.toml", desc);
    }
    let c = repo.commit_all(&format!("release {edition_str}"));
    repo.set_origin_main(&c);
    repo.tag(edition_str);
    repo.checkout(edition_str);
    repo
}

fn jobs(text: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut in_jobs = false;
    let mut job_starts: Vec<(String, usize)> = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        if line.trim() == "jobs:" {
            in_jobs = true;
            continue;
        }
        if in_jobs {
            if !line.starts_with(' ') && !line.trim().is_empty() && !line.trim().starts_with('#') {
                break;
            }
            if let Some(rest) = line.strip_prefix("  ")
                && !rest.starts_with(' ')
                && !rest.starts_with('#')
                && let Some(name) = rest.strip_suffix(':')
                && name.chars().all(|c| c.is_ascii_lowercase() || c == '-')
            {
                job_starts.push((name.to_owned(), i));
            }
        }
    }

    let mut result = Vec::new();
    for (idx, (name, start_line)) in job_starts.iter().enumerate() {
        let end_line = if idx + 1 < job_starts.len() {
            job_starts[idx + 1].1
        } else {
            lines.len()
        };
        let section = lines[*start_line..end_line].join("\n");
        result.push((name.clone(), section));
    }
    result
}

fn job(text: &str, name: &str) -> String {
    for (jname, section) in jobs(text) {
        if jname == name {
            return section;
        }
    }
    panic!("job `{name}` not found in workflow");
}

fn edition_job_requires_site(workflow_text: &str) -> bool {
    let edition_section = job(workflow_text, "edition");
    for line in edition_section.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("if:") {
            return trimmed.contains("needs.tag-check.outputs.kind == 'site'");
        }
    }
    false
}

fn binary_binds_commit(job_text: &str) -> Result<(), String> {
    for line in job_text.lines() {
        if line.contains("unreleased") {
            return Err("contains `unreleased`".to_owned());
        }
    }

    let lines: Vec<&str> = job_text.lines().collect();

    let sha_lines: Vec<(usize, &str)> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.contains("FIELDBOOK_GIT_SHA="))
        .map(|(i, l)| (i, *l))
        .collect();

    if sha_lines.len() != 1 {
        return Err(format!(
            "expected exactly one line setting FIELDBOOK_GIT_SHA, found {}",
            sha_lines.len()
        ));
    }

    let (build_line_idx, build_line) = sha_lines[0];
    let valid_build = (build_line.contains("FIELDBOOK_GIT_SHA=\"$sha\"")
        || build_line.contains("FIELDBOOK_GIT_SHA=$(git rev-parse HEAD)"))
        && build_line.contains("cargo build")
        && build_line.contains("--locked")
        && build_line.contains("--profile dist");

    if !valid_build {
        return Err(
            "FIELDBOOK_GIT_SHA is not set on the `cargo build --locked --profile dist` command"
                .to_owned(),
        );
    }

    let has_sha_from_git = lines[..build_line_idx]
        .iter()
        .any(|l| l.contains("git rev-parse HEAD"))
        || build_line.contains("$(git rev-parse HEAD)");
    if !has_sha_from_git {
        return Err("sha does not come from `git rev-parse HEAD`".to_owned());
    }

    let version_line_idx = lines
        .iter()
        .position(|l| l.contains("target/dist/fieldbook --version"))
        .ok_or_else(|| "missing step running `target/dist/fieldbook --version`".to_owned())?;

    if version_line_idx <= build_line_idx {
        return Err("version check step is not after build step".to_owned());
    }

    let end_idx = (version_line_idx + 10).min(lines.len());
    let version_step_lines = &lines[version_line_idx..end_idx];
    if !version_step_lines.iter().any(|l| l.contains("$sha")) {
        return Err("version check does not compare to sha".to_owned());
    }

    let sha256_idx = lines
        .iter()
        .position(|l| l.contains("sha256sum"))
        .ok_or_else(|| "missing sha256sum line".to_owned())?;

    let upload_idx = lines
        .iter()
        .position(|l| l.contains("upload-artifact"))
        .ok_or_else(|| "missing upload-artifact line".to_owned())?;

    if build_line_idx >= sha256_idx || build_line_idx >= upload_idx {
        return Err("build step is not before sha256sum and upload-artifact".to_owned());
    }

    if version_line_idx >= sha256_idx || version_line_idx >= upload_idx {
        return Err("version check step is not before sha256sum and upload-artifact".to_owned());
    }

    Ok(())
}

// =========================================================================
// Group A: release X.Y.Z
// =========================================================================

#[test]
fn refuses_on_main() {
    let repo = Repo::new("refuses-on-main");
    repo.init();
    repo.write_minimal_files("0.0.0");
    repo.commit_all("initial");
    let out = repo.run(&["release", "0.1.0"]);
    assert_refusal(&out, "", "on main");
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn refuses_dirty_tree() {
    let repo = Repo::new("refuses-dirty-tree");
    repo.init();
    repo.write_minimal_files("0.0.0");
    repo.commit_all("initial");
    repo.git(&["checkout", "-b", "card/x"]);
    repo.write("untracked.txt", "untracked content");
    let out = repo.run(&["release", "0.1.0"]);
    assert_refusal(&out, "", "not clean");
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn refuses_version_not_higher() {
    let repo = Repo::new("refuses-ver-not-higher");
    repo.init();
    repo.write_minimal_files("0.0.0");
    repo.commit_all("initial");
    repo.git(&["checkout", "-b", "card/x"]);
    let out = repo.run(&["release", "0.0.0"]);
    assert_refusal(&out, "", "not higher");
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn refuses_empty_unreleased() {
    let repo = Repo::new("refuses-empty-unreleased");
    repo.init();
    repo.write("Cargo.toml", "[workspace.package]\nversion = \"0.0.0\"\n");
    repo.write("Cargo.lock", "version = 4\n");
    repo.write(
        "CHANGELOG.md",
        "# Changelog\n\n## Unreleased\n\n## 0.0.0\n\n- Initial.\n",
    );
    repo.write_empty_baseline();
    repo.commit_all("initial");
    repo.git(&["checkout", "-b", "card/x"]);
    let out = repo.run(&["release", "0.1.0"]);
    assert_refusal(&out, "", "## Unreleased");
    assert_eq!(out.status.code(), Some(1));
}

// =========================================================================
// Group B: release-check
// =========================================================================

#[test]
fn check_refuses_tag_off_main() {
    let repo = Repo::new("tag-off-main");
    repo.init();
    repo.write_minimal_files("0.0.0");
    let c1 = repo.commit_all("main commit");
    repo.set_origin_main(&c1);
    repo.git(&["checkout", "-b", "side"]);
    repo.write("side.txt", "side work");
    let _c2 = repo.commit_all("side commit");
    repo.tag("v0.0.0");
    repo.checkout("v0.0.0");
    let out = repo.run(&["release-check", "v0.0.0", "--base", "release-baseline.toml"]);
    assert_refusal(&out, "tag-on-main", "tag-on-main");
}

#[test]
// control: RL-01
fn check_refuses_version_mismatch() {
    let repo = Repo::new("version-mismatch");
    repo.init();
    repo.write_minimal_files("0.0.0");
    let c1 = repo.commit_all("initial");
    repo.set_origin_main(&c1);
    repo.tag("v9.9.9");
    repo.checkout("v9.9.9");
    let out = repo.run(&["release-check", "v9.9.9", "--base", "release-baseline.toml"]);
    assert_refusal(&out, "version-mismatch", "version-mismatch");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("v9.9.9") && stderr.contains("0.0.0"),
        "stderr should contain v9.9.9 and 0.0.0:\nstdout: {}\nstderr: {stderr}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn check_refuses_edition_heading_mismatch() {
    let repo = Repo::new("edition-heading-mismatch");
    repo.init();
    repo.write_minimal_files("0.0.0");
    repo.write(
        "corpus/changes.md",
        "# Corpus changes\n\n## edition-002\n\n- Added: x.\n",
    );
    let c1 = repo.commit_all("initial");
    repo.set_origin_main(&c1);
    repo.tag("edition-003");
    repo.checkout("edition-003");
    let out = repo.run(&[
        "release-check",
        "edition-003",
        "--base",
        "release-baseline.toml",
    ]);
    assert_refusal(&out, "edition-tag", "edition-002");
}

#[test]
fn check_refuses_when_origin_main_is_missing() {
    let repo = Repo::new("missing-origin-main");
    repo.init();
    repo.write_minimal_files("0.0.0");
    repo.commit_all("initial");
    repo.tag("v0.0.0");
    repo.checkout("v0.0.0");
    let out = repo.run(&["release-check", "v0.0.0", "--base", "release-baseline.toml"]);
    assert_refusal(&out, "origin-main", "origin/main");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("ok"),
        "stdout must not contain ok:\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn check_refuses_bad_tag_grammar() {
    let repo = Repo::new("bad-tag-grammar");
    repo.init();
    repo.write_minimal_files("0.0.0");
    repo.commit_all("initial");
    let out = repo.run(&[
        "release-check",
        "release-1",
        "--base",
        "release-baseline.toml",
    ]);
    assert_refusal(&out, "tag-grammar", "tag-grammar");
}

#[test]
fn check_refuses_a_base_that_is_not_the_committed_baseline() {
    let repo = Repo::new("base-not-committed");
    repo.init();
    repo.write_minimal_files("0.0.0");
    let c1 = repo.commit_all("initial");
    repo.set_origin_main(&c1);
    repo.tag("v0.0.0");
    repo.checkout("v0.0.0");
    repo.write(
        "other-baseline.toml",
        "baseline_schema = 1\n# altered\n[compatibility]\nkind = \"empty\"\n\n[tool]\nkind = \"empty\"\n\n[edition]\nkind = \"empty\"\n",
    );
    let out = repo.run(&["release-check", "v0.0.0", "--base", "other-baseline.toml"]);
    assert_refusal(&out, "baseline", "baseline");
}

// =========================================================================
// Group C: compatibility lifecycle
// =========================================================================

#[test]
fn check_accepts_empty_bindings_for_a_first_release() {
    let repo = Repo::new("first-release-empty");
    repo.init();
    repo.copy_from_workspace();
    repo.write_minimal_files("0.1.0");
    let c1 = repo.commit_all("v0.1.0 initial");
    repo.set_origin_main(&c1);
    repo.tag("v0.1.0");
    repo.checkout("v0.1.0");
    let out = repo.run(&[
        "release-check",
        "v0.1.0",
        "--base",
        "release-baseline.toml",
        "--output",
        "github_output",
    ]);
    assert_success(&out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("ok"),
        "stdout should contain ok:\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out_file_text = repo.read("github_output");
    assert!(
        out_file_text.contains("kind=tool"),
        "expected kind=tool in output: {out_file_text}"
    );
    assert!(
        out_file_text.contains("stream=tool"),
        "expected stream=tool in output: {out_file_text}"
    );
}

#[test]
fn check_accepts_unchanged_old_vectors() {
    let repo = Repo::new("accepts-unchanged-old");
    repo.init();
    repo.copy_from_workspace();
    repo.write_minimal_files("0.1.0");
    let c1 = repo.commit_all("v0.1.0 initial");
    repo.tag("v0.1.0");
    let files = repo.compatibility_files_from_published();

    repo.write("Cargo.toml", "[workspace.package]\nversion = \"0.2.0\"\n");
    repo.write_release_baseline(Some(("v0.1.0", &c1, &files)), Some(("v0.1.0", &c1)), None);
    let c2 = repo.commit_all("v0.2.0 bump");
    repo.set_origin_main(&c2);
    repo.tag("v0.2.0");
    repo.checkout("v0.2.0");
    let out = repo.run(&["release-check", "v0.2.0", "--base", "release-baseline.toml"]);
    assert_success(&out);
}

#[test]
fn check_refuses_changed_old_vectors() {
    let repo = Repo::new("refuses-changed-old");
    repo.init();
    repo.copy_from_workspace();
    repo.write_minimal_files("0.1.0");
    let c1 = repo.commit_all("v0.1.0 initial");
    repo.tag("v0.1.0");
    let files = repo.compatibility_files_from_published();

    repo.write("Cargo.toml", "[workspace.package]\nversion = \"0.2.0\"\n");
    repo.write_release_baseline(Some(("v0.1.0", &c1, &files)), Some(("v0.1.0", &c1)), None);
    let mut vec_content = repo.read("compat/calculation/build_up@1/calc-01.out.txt");
    vec_content.push_str("\n# tampered line\n");
    repo.write(
        "compat/calculation/build_up@1/calc-01.out.txt",
        &vec_content,
    );
    // Make the v0.2.0 tree self-consistent: rewrite only the build_up@1 row of
    // compat/published.toml to the new bytes' hash, so the sole possible
    // refusal is the baseline comparison against the archived binding. The
    // run-schema row shares the old hash and stays untouched.
    let path_line = "path = \"compat/calculation/build_up@1/calc-01.out.txt\"\n";
    let new_sha = repo.sha256("compat/calculation/build_up@1/calc-01.out.txt");
    let published = repo.read("compat/published.toml");
    let at = published.find(path_line).expect("build_up@1 row present") + path_line.len();
    let rest = &published[at..];
    let sha_start =
        rest.find("sha256 = \"").expect("sha line follows the path") + "sha256 = \"".len();
    let sha_end = sha_start + rest[sha_start..].find('"').expect("sha closes");
    let patched = format!(
        "{}{}{}{}",
        &published[..at],
        &rest[..sha_start],
        new_sha,
        &rest[sha_end..]
    );
    repo.write("compat/published.toml", &patched);

    let c2 = repo.commit_all("v0.2.0 with tampered vector");
    repo.set_origin_main(&c2);
    repo.tag("v0.2.0");
    repo.checkout("v0.2.0");
    let out = repo.run(&["release-check", "v0.2.0", "--base", "release-baseline.toml"]);
    assert_refusal(&out, "compatibility", "");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("archived binding `calculation:build_up@1` was modified"),
        "stderr should name the modified archived binding:\nstdout: {}\nstderr: {stderr}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn check_refuses_unavailable_history_and_wrong_identity() {
    // (a) baseline names tool v0.1.0 with a commit that does not exist in this repo
    let repo_a = Repo::new("unavail-history");
    repo_a.init();
    repo_a.copy_from_workspace();
    repo_a.write_minimal_files("0.2.0");
    repo_a.write_release_baseline(
        None,
        Some(("v0.1.0", "0123456789abcdef0123456789abcdef01234567")),
        None,
    );
    let ca = repo_a.commit_all("v0.2.0");
    repo_a.set_origin_main(&ca);
    repo_a.tag("v0.2.0");
    repo_a.checkout("v0.2.0");
    let out_a = repo_a.run(&["release-check", "v0.2.0", "--base", "release-baseline.toml"]);
    assert_refusal(&out_a, "compatibility", "");

    // (b) baseline names tool v0.1.0 but gives a commit that is not v0.1.0's commit
    let repo_b = Repo::new("wrong-identity");
    repo_b.init();
    repo_b.copy_from_workspace();
    repo_b.write_minimal_files("0.1.0");
    let _c1 = repo_b.commit_all("v0.1.0 initial");
    repo_b.tag("v0.1.0");

    repo_b.write("Cargo.toml", "[workspace.package]\nversion = \"0.2.0\"\n");
    let c2 = repo_b.commit_all("intermediate");
    repo_b.write_release_baseline(None, Some(("v0.1.0", &c2)), None);
    let c3 = repo_b.commit_all("v0.2.0 with baseline pointing to c2");
    repo_b.set_origin_main(&c3);
    repo_b.tag("v0.2.0");
    repo_b.checkout("v0.2.0");
    let out_b = repo_b.run(&["release-check", "v0.2.0", "--base", "release-baseline.toml"]);
    assert_refusal(&out_b, "compatibility", "");
}

// =========================================================================
// Group D: edition modes
// =========================================================================

#[test]
fn release_yml_raw_mode_skips_site() {
    // (1) descriptor for edition-001 raw -> exit 0, kind=raw, no evidence directory created
    let repo1 = make_edition_repo(
        "edition-001",
        Some("release_schema = 1\nedition = \"001\"\nkind = \"raw\"\n"),
    );
    let out1 = repo1.run(&[
        "release-check",
        "edition-001",
        "--base",
        "release-baseline.toml",
        "--evidence-out",
        "out/evidence",
        "--output",
        "out/github_output",
    ]);
    assert_success(&out1);
    let out1_text = repo1.read("out/github_output");
    assert!(
        out1_text.contains("kind=raw"),
        "expected kind=raw in {out1_text}"
    );
    assert!(
        !repo1.dir.join("out/evidence").exists(),
        "evidence directory should not be created for raw mode"
    );

    // (2) edition-002 with NO descriptor -> exit 0, kind=site, number=002, writes evidence
    let repo2 = make_edition_repo("edition-002", None);
    let out2 = repo2.run(&[
        "release-check",
        "edition-002",
        "--base",
        "release-baseline.toml",
        "--evidence-out",
        "out/evidence",
        "--output",
        "out/github_output",
    ]);
    assert_success(&out2);
    let out2_text = repo2.read("out/github_output");
    assert!(
        out2_text.contains("kind=site"),
        "expected kind=site in {out2_text}"
    );
    assert!(
        out2_text.contains("number=002"),
        "expected number=002 in {out2_text}"
    );
    assert!(
        repo2
            .dir
            .join("out/evidence/release-baseline.toml")
            .exists(),
        "expected evidence release-baseline.toml"
    );
    assert!(
        repo2
            .dir
            .join("out/evidence/compat/published.toml")
            .exists(),
        "expected evidence published.toml"
    );

    // (3) edition-002 with matching explicit site descriptor -> kind=site
    let repo3 = make_edition_repo(
        "edition-002",
        Some("release_schema = 1\nedition = \"002\"\nkind = \"site\"\n"),
    );
    let out3 = repo3.run(&[
        "release-check",
        "edition-002",
        "--base",
        "release-baseline.toml",
        "--output",
        "out/github_output",
    ]);
    assert_success(&out3);
    let out3_text = repo3.read("out/github_output");
    assert!(
        out3_text.contains("kind=site"),
        "expected kind=site in {out3_text}"
    );

    // (4) edition-002 that retains edition-001 raw descriptor -> exit 1, rule edition-tag
    let repo4 = make_edition_repo(
        "edition-002",
        Some("release_schema = 1\nedition = \"001\"\nkind = \"raw\"\n"),
    );
    let out4 = repo4.run(&[
        "release-check",
        "edition-002",
        "--base",
        "release-baseline.toml",
    ]);
    assert_refusal(&out4, "edition-tag", "");

    // (5) edition-002 with malformed descriptor:
    // (5a) kind = "weird"
    let repo5a = make_edition_repo(
        "edition-002",
        Some("release_schema = 1\nedition = \"002\"\nkind = \"weird\"\n"),
    );
    let out5a = repo5a.run(&[
        "release-check",
        "edition-002",
        "--base",
        "release-baseline.toml",
    ]);
    assert_refusal(&out5a, "edition-tag", "");

    // (5b) unknown key
    let repo5b = make_edition_repo(
        "edition-002",
        Some("release_schema = 1\nedition = \"002\"\nkind = \"site\"\nextra = 1\n"),
    );
    let out5b = repo5b.run(&[
        "release-check",
        "edition-002",
        "--base",
        "release-baseline.toml",
    ]);
    assert_refusal(&out5b, "edition-tag", "");

    // Workflow check
    let workflow_text = std::fs::read_to_string(repo_root().join(".github/workflows/release.yml"))
        .unwrap_or_else(|e| panic!("read release.yml: {e}"));
    assert!(
        edition_job_requires_site(&workflow_text),
        "real workflow edition job should require site"
    );
    let planted = workflow_text.replace(" && needs.tag-check.outputs.kind == 'site'", "");
    assert!(
        !edition_job_requires_site(&planted),
        "planted workflow without site check should fail"
    );
}

// =========================================================================
// Group E: workflow text tests
// =========================================================================

#[test]
fn release_yml_is_read_only_and_pinned() {
    let workflow_text = std::fs::read_to_string(repo_root().join(".github/workflows/release.yml"))
        .unwrap_or_else(|e| panic!("read release.yml: {e}"));

    // Exactly one non-comment line starting with permissions: and followed by contents: read
    let perm_lines: Vec<(usize, &str)> = workflow_text
        .lines()
        .enumerate()
        .filter(|(_, l)| {
            let t = l.trim_start();
            !t.starts_with('#') && t.starts_with("permissions:")
        })
        .collect();
    assert_eq!(
        perm_lines.len(),
        1,
        "expected exactly one top-level permissions: line"
    );
    let (idx, line) = perm_lines[0];
    assert_eq!(line.trim(), "permissions:");
    let next_line = workflow_text.lines().nth(idx + 1).unwrap_or("");
    assert_eq!(next_line, "  contents: read");

    // No non-comment line contains write
    for (i, line) in workflow_text.lines().enumerate() {
        if !line.trim_start().starts_with('#') {
            assert!(!line.contains("write"), "line {i} contains write: {line}");
        }
    }

    // Only the trusted Site packaging step receives the contact route.
    let packaging_step = concat!(
        "      - name: Package the edition\n",
        "        env:\n",
        "          FIELDBOOK_SECURITY_CONTACT: ${{ secrets.FIELDBOOK_SECURITY_CONTACT }}\n",
        "        run: cargo xtask edition\n",
    );
    assert!(job(&workflow_text, "edition").contains(packaging_step));
    let other_steps = workflow_text.replacen(packaging_step, "", 1);
    for line in other_steps
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
    {
        let lower = line.to_ascii_lowercase();
        assert!(
            !lower.contains("secrets") && !lower.contains("fieldbook_security_contact"),
            "unexpected contact or secret reference outside the packaging step"
        );
    }

    // Every non-comment uses: line is owner/repo@<40 lowercase hex> # v<version>
    let mut uses_count = 0;
    for (i, line) in workflow_text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            continue;
        }
        if let Some(rest) = trimmed
            .strip_prefix("- uses:")
            .or_else(|| trimmed.strip_prefix("uses:"))
        {
            uses_count += 1;
            let u = rest.trim();
            let (action, comment) = u
                .split_once(" # ")
                .unwrap_or_else(|| panic!("line {i} missing ' # ': {line}"));
            assert!(
                comment.starts_with('v'),
                "line {i} version comment must start with v: {comment}"
            );
            let (owner_repo, sha) = action
                .split_once('@')
                .unwrap_or_else(|| panic!("line {i} missing '@': {line}"));
            assert!(
                owner_repo.contains('/'),
                "line {i} owner/repo invalid: {owner_repo}"
            );
            assert_eq!(sha.len(), 40, "line {i} sha length != 40: {sha}");
            assert!(
                sha.chars()
                    .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
                "line {i} sha not lowercase hex: {sha}"
            );
        }
    }
    assert!(uses_count > 0, "expected at least one uses: line");

    // tag-check job's checkout step has fetch-depth: 0 and persist-credentials: false
    let tc = job(&workflow_text, "tag-check");
    assert!(tc.contains("fetch-depth: 0"));
    assert!(tc.contains("persist-credentials: false"));

    // Every job has runs-on: ubuntu-24.04 and timeout-minutes: 30
    for (name, sec) in jobs(&workflow_text) {
        assert!(
            sec.contains("runs-on: ubuntu-24.04"),
            "job {name} missing runs-on: ubuntu-24.04"
        );
        assert!(
            sec.contains("timeout-minutes: 30"),
            "job {name} missing timeout-minutes: 30"
        );
    }

    // No job has its own permissions:
    for (name, sec) in jobs(&workflow_text) {
        for line in sec.lines() {
            if !line.trim_start().starts_with('#') {
                assert!(
                    !line.trim_start().starts_with("permissions:"),
                    "job {name} has permissions:"
                );
            }
        }
    }

    // No continue-on-error
    for line in workflow_text.lines() {
        if !line.trim_start().starts_with('#') {
            assert!(!line.contains("continue-on-error"));
        }
    }
}

#[test]
fn release_yml_edition_runs_the_harness() {
    let workflow_text = std::fs::read_to_string(repo_root().join(".github/workflows/release.yml"))
        .unwrap_or_else(|e| panic!("read release.yml: {e}"));
    let ed = job(&workflow_text, "edition");
    assert!(
        ed.lines().any(|l| l.trim() == "run: cargo xtask edition"),
        "expected step run: cargo xtask edition"
    );
    let fieldbook_build = format!("{} build", fieldbook_core::strings::binary());
    for forbidden in [
        fieldbook_build.as_str(),
        "just site",
        "just browser",
        "harness",
        "cargo build",
        "cargo run",
        "edition-raw",
        "network-silence-summary",
    ] {
        assert!(
            !ed.contains(forbidden),
            "edition job contains forbidden string: {forbidden}"
        );
    }
    assert!(ed.contains("out/dist/fieldbook-edition-${{ needs.tag-check.outputs.number }}.zip"));
    assert!(ed.contains("out/dist/fieldbook-originals-${{ needs.tag-check.outputs.number }}.zip"));
    assert!(ed.contains("out/dist/SHA256SUMS"));
    assert!(ed.contains("out/dist/corpus-support.toml"));
    assert!(ed.contains("out/dist/corpus-support.sha256"));
}

#[test]
fn release_yml_binary_scans_the_binary() {
    let workflow_text = std::fs::read_to_string(repo_root().join(".github/workflows/release.yml"))
        .unwrap_or_else(|e| panic!("read release.yml: {e}"));
    let bin = job(&workflow_text, "binary");
    let lines: Vec<&str> = bin.lines().collect();
    let dist_idx = lines
        .iter()
        .position(|l| l.contains("--profile dist"))
        .unwrap_or_else(|| panic!("missing --profile dist"));
    let binscan_idx = lines
        .iter()
        .position(|l| l.contains("cargo xtask binscan target/dist/fieldbook --build silent"))
        .unwrap_or_else(|| panic!("missing binscan line"));
    assert!(
        binscan_idx > dist_idx,
        "binscan index ({binscan_idx}) must be greater than --profile dist index ({dist_idx})"
    );
}

#[test]
fn release_yml_triggers() {
    let workflow_text = std::fs::read_to_string(repo_root().join(".github/workflows/release.yml"))
        .unwrap_or_else(|e| panic!("read release.yml: {e}"));

    assert!(workflow_text.contains("workflow_dispatch:"));
    assert!(
        workflow_text.contains("pull_request:\n    paths: [\".github/workflows/release.yml\"]")
    );
    assert!(workflow_text.contains("tags: [\"v*\", \"edition-*\"]"));

    assert!(
        workflow_text
            .lines()
            .any(|l| l == "    if: github.ref_type == 'tag'")
    );

    let bin = job(&workflow_text, "binary");
    let bin_if = bin
        .lines()
        .find(|l| l.trim_start().starts_with("if:"))
        .unwrap_or("");
    assert!(bin_if.contains("needs.tag-check.result != 'failure'"));
    assert!(bin_if.contains("!startsWith(github.ref_name, 'edition-')"));
    assert!(bin_if.contains("!cancelled()"));

    assert!(!workflow_text.contains("dry_run"));
}

#[test]
fn release_yml_artifact_name_uses_the_workspace_version() {
    let workflow_text = std::fs::read_to_string(repo_root().join(".github/workflows/release.yml"))
        .unwrap_or_else(|e| panic!("read release.yml: {e}"));
    let bin = job(&workflow_text, "binary");
    assert!(bin.contains("id: version"));
    assert!(bin.contains("cargo metadata"));
    assert!(bin.contains("jq"));
    let upload_name_line = bin
        .lines()
        .find(|l| l.trim_start().starts_with("name: fieldbook-v"))
        .unwrap_or_else(|| panic!("missing artifact name: fieldbook-v"));
    assert_eq!(
        upload_name_line.trim(),
        "name: fieldbook-v${{ steps.version.outputs.version }}-linux"
    );
    assert!(!upload_name_line.contains("github.ref_name"));
    assert!(!bin.contains("retention-days"));
}

#[test]
fn linux_release_binds_and_checks_commit() {
    let workflow_text = std::fs::read_to_string(repo_root().join(".github/workflows/release.yml"))
        .unwrap_or_else(|e| panic!("read release.yml: {e}"));
    let job_text = job(&workflow_text, "binary");

    assert!(
        binary_binds_commit(&job_text).is_ok(),
        "real binary job should pass binary_binds_commit"
    );

    // Plant (i): FIELDBOOK_GIT_SHA renamed to GIT_SHA in the build line
    let plant1 = job_text.replace("FIELDBOOK_GIT_SHA=", "GIT_SHA=");
    assert!(
        binary_binds_commit(&plant1).is_err(),
        "plant 1 (renamed FIELDBOOK_GIT_SHA) should fail"
    );

    // Plant (ii): the --version step removed
    let version_step = "\
      - name: Check that the program reports the commit it was built from
        run: |
          set -euo pipefail
          sha=\"$(git rev-parse HEAD)\"
          version_out=\"$(target/dist/fieldbook --version)\"
          case \"$version_out\" in
            *\"($sha)\") ;;
            *)
              echo \"unexpected version output: $version_out\" >&2
              exit 1
              ;;
          esac
";
    assert!(
        job_text.contains(version_step),
        "job_text must contain the exact version_step"
    );
    let plant2 = job_text.replace(version_step, "");
    assert!(
        binary_binds_commit(&plant2).is_err(),
        "plant 2 (removed version step) should fail"
    );

    // Plant (iii): the --version step moved after the sha256sum line
    let plant3 = format!("{}\n{version_step}", plant2);
    assert!(
        binary_binds_commit(&plant3).is_err(),
        "plant 3 (version step moved after sha256sum) should fail"
    );
}

#[test]
fn release_yml_has_no_release_page_or_signing() {
    let workflow_text = std::fs::read_to_string(repo_root().join(".github/workflows/release.yml"))
        .unwrap_or_else(|e| panic!("read release.yml: {e}"));
    for line in workflow_text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            continue;
        }
        assert!(!line.contains("gh release"), "contains gh release: {line}");
        assert!(!line.contains("softprops"), "contains softprops: {line}");
        assert!(
            !line.contains("action-gh-release"),
            "contains action-gh-release: {line}"
        );
        assert!(
            !line.to_ascii_lowercase().contains("windows"),
            "contains windows: {line}"
        );
    }
    for (name, _) in jobs(&workflow_text) {
        assert!(!name.contains("sign"), "job name contains sign: {name}");
    }
}

fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap_or_else(|e| panic!("create {}: {e}", to.display()));
    for entry in std::fs::read_dir(from).unwrap_or_else(|e| panic!("read {}: {e}", from.display()))
    {
        let entry = entry.unwrap_or_else(|e| panic!("entry: {e}"));
        let path = entry.path();
        let dest = to.join(entry.file_name());
        if path.is_dir() {
            copy_tree(&path, &dest);
        } else {
            std::fs::copy(&path, &dest).unwrap_or_else(|e| panic!("copy {}: {e}", path.display()));
        }
    }
}
