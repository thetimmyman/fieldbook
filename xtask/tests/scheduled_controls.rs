//! Weekly scheduled control tests (`docs/controls.md`).

#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "weekly control tests spawn cargo, cargo-deny, gitleaks and git and read and write files"
)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| panic!("xtask lives under repo root"))
        .to_path_buf()
}

fn combined(output: &Output) -> String {
    let mut raw = String::from_utf8_lossy(&output.stdout).into_owned();
    raw.push_str(&String::from_utf8_lossy(&output.stderr));
    let mut clean = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            let mut seq = Vec::new();
            let mut matched_m = false;
            for next in chars.by_ref() {
                if next == 'm' {
                    matched_m = true;
                    break;
                }
                if next.is_ascii_digit() || next == ';' || next == '?' {
                    seq.push(next);
                } else {
                    seq.push(next);
                    break;
                }
            }
            if !matched_m {
                clean.push('\x1b');
                clean.push('[');
                for sc in seq {
                    clean.push(sc);
                }
            }
        } else {
            clean.push(c);
        }
    }
    clean
}

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=Synthetic",
            "-c",
            "user.email=synthetic@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap_or_else(|e| panic!("run git: {e}"));

    if !output.status.success() {
        panic!("{}", String::from_utf8_lossy(&output.stderr));
    }
}

// control: DP-05
#[test]
#[ignore = "weekly control test requires cargo-deny and advisory network access"]
fn advisories_fixture_fails_deny() {
    let output = Command::new("cargo")
        .args([
            "deny",
            "--locked",
            "--manifest-path",
            "xtask/tests/advisories_fixture/Cargo.toml",
            "--config",
            "deny.toml",
            "check",
            "advisories",
        ])
        .current_dir(repo_root())
        .output()
        .unwrap_or_else(|e| panic!("run cargo deny: {e}"));

    if output.status.success() {
        panic!("cargo deny passed on the advisory fixture");
    }

    let out = combined(&output);
    assert!(
        out.contains("RUSTSEC-") && out.contains("smallvec"),
        "cargo deny output missing advisory or crate:\n{out}"
    );
}

// control: DP-01
#[test]
#[ignore = "weekly control test requires cargo-deny"]
fn advisories_fixture_git_source_fails_deny() {
    let temp = tempfile::tempdir().unwrap_or_else(|e| panic!("create tempdir: {e}"));
    let temp_path = temp.path();

    let dep_dir = temp_path.join("dep");
    std::fs::create_dir_all(dep_dir.join("src"))
        .unwrap_or_else(|e| panic!("create dep src dir: {e}"));
    git(&dep_dir, &["init", "-q"]);
    std::fs::write(
        dep_dir.join("Cargo.toml"),
        "[package]\nname = \"gitdep\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap_or_else(|e| panic!("write dep Cargo.toml: {e}"));
    std::fs::write(dep_dir.join("src/lib.rs"), "")
        .unwrap_or_else(|e| panic!("write dep src/lib.rs: {e}"));
    git(&dep_dir, &["add", "."]);
    git(&dep_dir, &["commit", "-m", "initial"]);

    let consumer_dir = temp_path.join("consumer");
    std::fs::create_dir_all(consumer_dir.join("src"))
        .unwrap_or_else(|e| panic!("create consumer src dir: {e}"));
    std::fs::write(consumer_dir.join("src/lib.rs"), "")
        .unwrap_or_else(|e| panic!("write consumer src/lib.rs: {e}"));

    let dep_url = format!("file://{}", dep_dir.display());
    let consumer_manifest = consumer_dir.join("Cargo.toml");
    std::fs::write(
        &consumer_manifest,
        format!(
            "[package]\nname = \"consumer\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\ngitdep = {{ git = \"{dep_url}\" }}\n\n[workspace]\n"
        ),
    )
    .unwrap_or_else(|e| panic!("write consumer Cargo.toml: {e}"));

    let output = Command::new("cargo")
        .args([
            "deny",
            "--manifest-path",
            consumer_manifest
                .to_str()
                .unwrap_or_else(|| panic!("manifest path to str")),
            "--config",
            "deny.toml",
            "check",
            "sources",
        ])
        .current_dir(repo_root())
        .output()
        .unwrap_or_else(|e| panic!("run cargo deny: {e}"));

    if output.status.success() {
        panic!("cargo deny passed on a git source");
    }

    let out = combined(&output);
    assert!(
        out.contains("'git' source") && out.contains("gitdep"),
        "cargo deny output missing 'git' source or gitdep:\n{out}"
    );
}

// control: SC-01
#[test]
#[ignore = "weekly control test requires gitleaks"]
fn gitleaks_reports_a_planted_token() {
    let head = "ghp_x7Kq9Zr2Lm4Vb8Nc1T";
    let tail = "d5Wy3Hs6Jf0PaQe2Ru";
    debug_assert_eq!(head.strip_prefix("ghp_").map_or(0, str::len), 18);
    debug_assert_eq!(tail.len(), 18);
    let token = format!("{head}{tail}");
    debug_assert_eq!(token.len(), 40);

    let temp = tempfile::tempdir().unwrap_or_else(|e| panic!("create tempdir: {e}"));
    let temp_path = temp.path();
    git(temp_path, &["init", "-q"]);
    std::fs::write(
        temp_path.join("config.txt"),
        format!("token = \"{token}\"\n"),
    )
    .unwrap_or_else(|e| panic!("write config.txt: {e}"));
    git(temp_path, &["add", "config.txt"]);
    git(temp_path, &["commit", "-m", "commit token"]);

    let justfile_path = repo_root().join("justfile");
    let justfile_content =
        std::fs::read_to_string(&justfile_path).unwrap_or_else(|e| panic!("read justfile: {e}"));
    let line = justfile_content
        .lines()
        .find(|l| l.trim().starts_with("gitleaks git "))
        .unwrap_or_else(|| panic!("justfile lacks gitleaks git line"));

    let mut args: Vec<String> = line.split_whitespace().map(String::from).collect();
    if args.first().map(String::as_str) == Some("gitleaks") {
        args.remove(0);
    }

    for arg in &mut args {
        if arg.starts_with("--log-opts=") {
            *arg = "--log-opts=HEAD".to_string();
        }
    }

    args.push("--verbose".to_string());
    args.push("--config".to_string());
    let config_path = repo_root().join(".gitleaks.toml");
    args.push(config_path.display().to_string());

    let output = Command::new("gitleaks")
        .args(&args)
        .current_dir(temp_path)
        .output()
        .unwrap_or_else(|e| panic!("run gitleaks: {e}"));

    let out = combined(&output);
    if output.status.success() || !out.contains("github-pat") {
        panic!("gitleaks did not report the planted token\n{out}");
    }
}

const PLANTS: &[(&str, &str, &str)] = &[
    (
        "fs_read",
        "pub fn plant_fs_read() { let _ = std::fs::read(\"x\"); }",
        "clippy::disallowed_methods",
    ),
    (
        "ln",
        "pub fn plant_ln(x: f64) -> f64 { x.ln() }",
        "clippy::disallowed_methods",
    ),
    (
        "powf",
        "pub fn plant_powf(x: f64, y: f64) -> f64 { x.powf(y) }",
        "clippy::disallowed_methods",
    ),
    (
        "powi",
        "pub fn plant_powi(x: f64) -> f64 { x.powi(2) }",
        "clippy::disallowed_methods",
    ),
    (
        "exp",
        "pub fn plant_exp(x: f64) -> f64 { x.exp() }",
        "clippy::disallowed_methods",
    ),
    (
        "map",
        "pub fn plant_map(m: Option<std::collections::HashMap<u8, u8>>) -> usize { m.map_or(0, |m| m.len()) }",
        "clippy::disallowed_types",
    ),
    (
        "float",
        "pub fn plant_float(a: f64, b: f64) -> f64 { a * b }",
        "clippy::float_arithmetic",
    ),
    (
        "index",
        "pub fn plant_index(v: &[u8]) -> u8 { v[0] }",
        "clippy::indexing_slicing",
    ),
    (
        "unsafe",
        "pub fn plant_unsafe() { unsafe {} }",
        "unsafe_code",
    ),
];

struct FileRestoreGuard {
    path: PathBuf,
    original: String,
}

impl Drop for FileRestoreGuard {
    fn drop(&mut self) {
        let _ = std::fs::write(&self.path, &self.original);
    }
}

// control: NS-02, CD-01, DT-04
#[test]
#[ignore = "weekly control test runs cargo clippy across plants"]
fn lint_bans_fail_clippy() {
    let clippy_toml_path = repo_root().join("clippy.toml");
    let clippy_toml = std::fs::read_to_string(&clippy_toml_path)
        .unwrap_or_else(|e| panic!("read clippy.toml: {e}"));

    const BANNED_ENTRIES: &[&str] = &[
        "std::fs::read",
        "std::process::Command",
        "std::net::TcpStream",
        "std::env::var",
        "std::time::SystemTime::now",
        "std::process::exit",
        "std::collections::HashMap",
        "f64::ln",
        "f64::powf",
        "f64::powi",
        "f64::exp",
        "rust_decimal::Decimal::round",
        "rust_decimal::Decimal::round_dp",
    ];

    for entry in BANNED_ENTRIES {
        let quoted = format!("\"{entry}\"");
        assert!(
            clippy_toml.contains(&quoted),
            "clippy.toml lacks the ban of `{entry}`"
        );
    }

    let target_path = repo_root().join("crates/fieldbook-calc/src/lib.rs");
    let original = std::fs::read_to_string(&target_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", target_path.display()));
    let _guard = FileRestoreGuard {
        path: target_path.clone(),
        original: original.clone(),
    };

    for &(name, plant_text, lint) in PLANTS {
        let planted = format!("{original}\n{plant_text}\n");
        std::fs::write(&target_path, &planted)
            .unwrap_or_else(|e| panic!("write plant {name}: {e}"));

        let output = Command::new("cargo")
            .args([
                "clippy",
                "--locked",
                "-p",
                "fieldbook-calc",
                "--message-format=json",
                "--",
                "-D",
                "warnings",
            ])
            .current_dir(repo_root())
            .output()
            .unwrap_or_else(|e| panic!("run cargo clippy for plant {name}: {e}"));

        let stdout = String::from_utf8_lossy(&output.stdout);
        let expected_code = format!("\"code\":\"{lint}\"");
        if output.status.success() {
            panic!("the {name} plant passed clippy");
        }
        if !stdout.contains(&expected_code) {
            panic!("the {name} plant passed clippy: failure was for another reason");
        }
    }
}

fn job_names(yaml: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_jobs = false;
    for line in yaml.lines() {
        if !in_jobs {
            if line.trim() == "jobs:" {
                in_jobs = true;
            }
        } else {
            if !line.starts_with(' ') && !line.trim().is_empty() && !line.trim().starts_with('#') {
                break;
            }
            if line.starts_with("  ") && !line.starts_with("   ") {
                let trimmed = line.trim();
                if let Some(name) = trimmed.strip_suffix(':')
                    && !name.is_empty()
                    && !name.contains(' ')
                {
                    names.push(name.to_string());
                }
            }
        }
    }
    names
}

fn job_text(yaml: &str, job: &str) -> Result<String, String> {
    let mut in_jobs = false;
    let job_target = format!("  {job}:");
    let mut found_job = false;
    let mut lines = Vec::new();

    for line in yaml.lines() {
        if !in_jobs {
            if line.trim() == "jobs:" {
                in_jobs = true;
            }
        } else if !found_job {
            if line == job_target {
                found_job = true;
                lines.push(line);
            }
        } else {
            if (line.starts_with("  ") && !line.starts_with("   "))
                || (!line.starts_with(' ')
                    && !line.trim().is_empty()
                    && !line.trim().starts_with('#'))
            {
                break;
            }
            lines.push(line);
        }
    }

    if !found_job {
        return Err(format!("job `{job}` not found in workflow"));
    }

    Ok(lines.join("\n"))
}

fn job_steps(yaml: &str, job: &str) -> Result<Vec<String>, String> {
    let text = job_text(yaml, job)?;
    let mut steps = Vec::new();
    let mut current_step: Vec<&str> = Vec::new();

    for line in text.lines() {
        if line.starts_with("      - ") {
            if !current_step.is_empty() {
                while let Some(last) = current_step.last() {
                    let t = last.trim();
                    if t.is_empty() || t.starts_with('#') {
                        current_step.pop();
                    } else {
                        break;
                    }
                }
                steps.push(current_step.join("\n"));
                current_step.clear();
            }
            current_step.push(line);
        } else if !current_step.is_empty() {
            current_step.push(line);
        }
    }

    if !current_step.is_empty() {
        while let Some(last) = current_step.last() {
            let t = last.trim();
            if t.is_empty() || t.starts_with('#') {
                current_step.pop();
            } else {
                break;
            }
        }
        steps.push(current_step.join("\n"));
    }

    Ok(steps)
}

fn step_id(step: &str) -> Option<&str> {
    for line in step.lines() {
        let t = line.trim();
        let stripped = t.strip_prefix("- ").unwrap_or(t).trim_start();
        if let Some(rest) = stripped.strip_prefix("id:") {
            return Some(rest.trim());
        }
    }
    None
}

fn step_has_heading_if(step: &str) -> bool {
    step.lines()
        .any(|l| l.trim() == "if: steps.heading.outputs.run == 'true'")
}

fn step_has_any_if(step: &str) -> bool {
    step.lines().any(|l| {
        let t = l.trim();
        let stripped = t.strip_prefix("- ").unwrap_or(t).trim_start();
        stripped.starts_with("if:")
    })
}

fn step_uses_line(step: &str) -> Option<&str> {
    for line in step.lines() {
        let t = line.trim();
        if t.starts_with("uses:") || t.starts_with("- uses:") {
            return Some(t);
        }
    }
    None
}

fn step_run(step: &str) -> Option<String> {
    let lines: Vec<&str> = step.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        let stripped = t.strip_prefix("- ").unwrap_or(t).trim_start();
        if let Some(rest) = stripped.strip_prefix("run:") {
            let rest_trimmed = rest.trim();
            if rest_trimmed == "|"
                || rest_trimmed == "|-"
                || rest_trimmed == ">"
                || rest_trimmed == ">-"
            {
                let run_indent = line.len() - line.trim_start().len();
                let mut block_lines = Vec::new();
                for next in &lines[i + 1..] {
                    let next_indent = next.len() - next.trim_start().len();
                    if next.trim().is_empty() {
                        block_lines.push("");
                    } else if next_indent > run_indent {
                        block_lines.push(*next);
                    } else {
                        break;
                    }
                }
                while let Some(last) = block_lines.last() {
                    if last.trim().is_empty() {
                        block_lines.pop();
                    } else {
                        break;
                    }
                }
                return Some(block_lines.join("\n"));
            }
            return Some(rest_trimmed.to_string());
        }
    }
    None
}

fn check_heading_guard(yaml: &str) -> Result<(), String> {
    let jobs = job_names(yaml);
    let mut heading_jobs = Vec::new();

    for job in &jobs {
        let steps = job_steps(yaml, job)?;
        if steps.iter().any(|s| step_id(s) == Some("heading")) {
            heading_jobs.push(job.clone());
        }
    }

    if heading_jobs.is_empty() {
        return Err("expected at least one job with id: heading".to_string());
    }
    if !heading_jobs.contains(&"edition-linux".to_string()) {
        return Err("edition-linux job must have a step with id: heading".to_string());
    }

    const EXPECTED_CHECKOUT: &str =
        "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1";

    for job in heading_jobs {
        let steps = job_steps(yaml, &job)?;
        if steps.len() < 2 {
            return Err(format!("job `{job}` has fewer than 2 steps"));
        }

        let step1 = &steps[0];
        let uses_line = step_uses_line(step1).unwrap_or("");
        let expected_uses = format!("uses: {EXPECTED_CHECKOUT}");
        let expected_dash_uses = format!("- uses: {EXPECTED_CHECKOUT}");
        if uses_line != expected_uses && uses_line != expected_dash_uses {
            return Err(format!(
                "job `{job}` step 1: uses line is not `{EXPECTED_CHECKOUT}`, found `{uses_line}`"
            ));
        }
        if !step1.contains("fetch-depth: 0") {
            return Err(format!("job `{job}` step 1: missing `fetch-depth: 0`"));
        }
        if !step1.contains("persist-credentials: false") {
            return Err(format!(
                "job `{job}` step 1: missing `persist-credentials: false`"
            ));
        }
        if step_has_any_if(step1) {
            return Err(format!(
                "job `{job}` step 1: checkout step must have no `if:` line"
            ));
        }

        let step2 = &steps[1];
        if step_id(step2) != Some("heading") {
            return Err(format!("job `{job}` step 2: must have `id: heading`"));
        }
        let run2 = step_run(step2).unwrap_or_default();
        if !run2.contains("grep -q \"^## edition-\" corpus/changes.md") {
            return Err(format!(
                "job `{job}` step 2: run text missing `grep -q \"^## edition-\" corpus/changes.md`"
            ));
        }
        if !run2.contains("no edition to build") {
            return Err(format!(
                "job `{job}` step 2: run text missing `no edition to build`"
            ));
        }

        for (i, step) in steps[2..].iter().enumerate() {
            let step_num = i + 3;
            if !step_has_heading_if(step) {
                return Err(format!(
                    "job `{job}` step {step_num}: missing `if: steps.heading.outputs.run == 'true'`"
                ));
            }
        }
    }

    Ok(())
}

fn check_edition_artifact(yaml: &str) -> Result<(), String> {
    let steps = job_steps(yaml, "edition-linux")?;
    let artifact_step = steps
        .iter()
        .find(|s| {
            step_uses_line(s).is_some_and(|l| {
                l.starts_with("uses: actions/upload-artifact@")
                    || l.starts_with("- uses: actions/upload-artifact@")
            })
        })
        .ok_or_else(|| "job `edition-linux` missing upload-artifact step".to_string())?;

    if !artifact_step.contains("name: edition-hashes-linux") {
        return Err("artifact step missing `name: edition-hashes-linux`".to_string());
    }
    if !artifact_step.contains("retention-days: 14") {
        return Err("artifact step missing `retention-days: 14`".to_string());
    }
    if !artifact_step.contains("if-no-files-found: error") {
        return Err("artifact step missing `if-no-files-found: error`".to_string());
    }

    let lines: Vec<&str> = artifact_step.lines().collect();
    let mut path_lines = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        let stripped = t.strip_prefix("- ").unwrap_or(t).trim_start();
        if stripped == "path: |" || stripped == "path: |-" {
            let path_indent = line.len() - line.trim_start().len();
            for next in &lines[i + 1..] {
                let next_indent = next.len() - next.trim_start().len();
                if next.trim().is_empty() {
                    continue;
                }
                if next_indent > path_indent {
                    path_lines.push(next.trim());
                } else {
                    break;
                }
            }
            break;
        }
    }

    if path_lines.is_empty() {
        return Err("artifact step missing `path: |` block".to_string());
    }

    let has_edition_zip = path_lines
        .iter()
        .any(|l| l.contains("fieldbook-edition-") && l.ends_with(".zip"));
    if !has_edition_zip {
        return Err(
            "artifact path block missing line with `fieldbook-edition-` ending `.zip`".to_string(),
        );
    }

    let has_originals_zip = path_lines
        .iter()
        .any(|l| l.contains("fieldbook-originals-") && l.ends_with(".zip"));
    if !has_originals_zip {
        return Err(
            "artifact path block missing line with `fieldbook-originals-` ending `.zip`"
                .to_string(),
        );
    }

    let has_sha256 = path_lines.contains(&"out/dist/SHA256SUMS");
    if !has_sha256 {
        return Err("artifact path block missing line `out/dist/SHA256SUMS`".to_string());
    }

    Ok(())
}

fn check_just_installed(yaml: &str, job: &str) -> Result<(), String> {
    let tools_toml_path = repo_root().join(".config/tools.toml");
    let tools_text = std::fs::read_to_string(&tools_toml_path)
        .map_err(|e| format!("read {}: {e}", tools_toml_path.display()))?;

    let mut in_cargo = false;
    let mut just_version = None;
    for line in tools_text.lines() {
        let t = line.trim();
        if t == "[cargo]" {
            in_cargo = true;
        } else if in_cargo {
            if t.starts_with('[') {
                break;
            }
            if let Some(rest) = t.strip_prefix("just = ") {
                just_version = Some(rest.trim_matches('"'));
            }
        }
    }

    let expected_version = just_version
        .ok_or_else(|| "missing `just = ` in [cargo] of .config/tools.toml".to_string())?;
    let expected_tool_item = format!("just@{expected_version}");

    let steps = job_steps(yaml, job)?;
    let mut install_idx = None;
    let mut just_invocation_idx = None;

    const INSTALL_ACTION_REF: &str =
        "taiki-e/install-action@ab6895351a88b908e96a4697860d4ffea5e57dbe # v2.87.27";

    for (idx, step) in steps.iter().enumerate() {
        let uses = step_uses_line(step).unwrap_or("");
        let matches_install_action = uses.starts_with(&format!("uses: {INSTALL_ACTION_REF}"))
            || uses.starts_with(&format!("- uses: {INSTALL_ACTION_REF}"));

        if matches_install_action {
            let has_just = step.lines().any(|l| {
                let t = l.trim();
                if let Some(tools) = t.strip_prefix("tool:") {
                    tools
                        .split(',')
                        .any(|item| item.trim() == expected_tool_item)
                } else {
                    false
                }
            });
            if has_just && install_idx.is_none() {
                install_idx = Some(idx);
            }
        }

        if let Some(run_text) = step_run(step) {
            for line in run_text.lines() {
                let mut t = line.trim();
                if let Some(rest) = t.strip_prefix("FIELDBOOK_BASE=") {
                    t = rest
                        .split_once(char::is_whitespace)
                        .map_or("", |(_, r)| r.trim());
                }
                if t.starts_with("just ") && just_invocation_idx.is_none() {
                    just_invocation_idx = Some(idx);
                    break;
                }
            }
        }
    }

    let inst = install_idx.ok_or_else(|| {
        format!("job `{job}` missing install-action step with `{expected_tool_item}`")
    })?;
    let inv =
        just_invocation_idx.ok_or_else(|| format!("job `{job}` missing step invoking just"))?;

    if inst >= inv {
        return Err(format!(
            "job `{job}` installs just at step {} but invokes just at step {}",
            inst + 1,
            inv + 1
        ));
    }

    Ok(())
}

fn check_one_edition_operation(yaml: &str) -> Result<(), String> {
    let steps = job_steps(yaml, "edition-linux")?;
    const FORBIDDEN: &[&str] = &[
        "just site",
        "just browser",
        "just edition",
        "cargo xtask harness",
        concat!("field", "book build"),
        "build --out",
        "cargo run",
    ];

    let mut edition_ops = 0;

    for (i, step) in steps.iter().enumerate() {
        if let Some(run_text) = step_run(step) {
            for bad in FORBIDDEN {
                if run_text.contains(bad) {
                    return Err(format!(
                        "job `edition-linux` step {}: run text contains forbidden `{bad}`",
                        i + 1
                    ));
                }
            }
            if run_text.lines().any(|l| l.trim() == "cargo xtask edition") {
                edition_ops += 1;
            }
        }
    }

    if edition_ops != 1 {
        return Err(format!(
            "job `edition-linux`: expected exactly one `cargo xtask edition` step, found {edition_ops}"
        ));
    }

    Ok(())
}

fn check_security_contact(yaml: &str) -> Result<(), String> {
    const BINDING: &str = "FIELDBOOK_SECURITY_CONTACT: ${{ secrets.FIELDBOOK_SECURITY_CONTACT }}";

    let jobs = job_names(yaml);
    let mut edition_steps = 0;

    for job in &jobs {
        let steps = job_steps(yaml, job)?;
        for (i, step) in steps.iter().enumerate() {
            if let Some(run_text) = step_run(step)
                && run_text.lines().any(|l| l.trim() == "cargo xtask edition")
            {
                edition_steps += 1;
                let has_binding = step.lines().any(|l| l.trim() == BINDING);
                if !has_binding {
                    return Err(format!(
                        "job `{job}` step {}: missing `{BINDING}` in step text",
                        i + 1
                    ));
                }
            }
        }
    }

    if edition_steps == 0 {
        return Err("expected at least one `cargo xtask edition` step".to_string());
    }

    for (i, line) in yaml.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.contains("vars.") {
            return Err(format!("line {}: contains `vars.`: `{line}`", i + 1));
        }
        if trimmed.contains("secrets.") && trimmed != BINDING {
            return Err(format!(
                "line {}: contains secret binding other than `{BINDING}`: `{line}`",
                i + 1
            ));
        }
    }

    Ok(())
}

fn check_uses_pinned(yaml: &str) -> Result<(), String> {
    let ci = std::fs::read_to_string(repo_root().join(".github/workflows/ci.yml"))
        .map_err(|e| format!("read ci.yml: {e}"))?;
    let release = std::fs::read_to_string(repo_root().join(".github/workflows/release.yml"))
        .map_err(|e| format!("read release.yml: {e}"))?;
    let hosted = std::fs::read_to_string(repo_root().join(".github/workflows/hosted.yml"))
        .map_err(|e| format!("read hosted.yml: {e}"))?;
    let deps = std::fs::read_to_string(repo_root().join("docs/reference/dependencies.md"))
        .map_err(|e| format!("read dependencies.md: {e}"))?;

    for (i, line) in yaml.lines().enumerate() {
        let t = line.trim();
        let action_ref = if let Some(r) = t.strip_prefix("- uses:") {
            r.trim()
        } else if let Some(r) = t.strip_prefix("uses:") {
            r.trim()
        } else {
            continue;
        };

        let Some((owner_name, pin_comment)) = action_ref.split_once('@') else {
            return Err(format!("line {}: uses line lacks '@': `{line}`", i + 1));
        };
        if owner_name.is_empty() || !owner_name.contains('/') {
            return Err(format!(
                "line {}: invalid action name `{owner_name}`",
                i + 1
            ));
        }

        let Some((sha, comment)) = pin_comment.split_once(" # v") else {
            return Err(format!(
                "line {}: missing ' # v' version comment: `{line}`",
                i + 1
            ));
        };
        if sha.len() != 40
            || !sha
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
        {
            return Err(format!(
                "line {}: SHA must be 40 lowercase hex digits: `{sha}`",
                i + 1
            ));
        }
        if comment.trim().is_empty() {
            return Err(format!("line {}: empty version comment: `{line}`", i + 1));
        }

        let verbatim = action_ref;
        if !ci.contains(verbatim) && !release.contains(verbatim) && !hosted.contains(verbatim) {
            return Err(format!(
                "line {}: action `{verbatim}` not found in ci.yml, release.yml or hosted.yml",
                i + 1
            ));
        }

        if !deps.contains(sha) {
            return Err(format!(
                "line {}: SHA `{sha}` not found in docs/reference/dependencies.md",
                i + 1
            ));
        }
    }

    Ok(())
}

// The edition job steps after heading check are guarded by the heading output.
#[test]
fn edition_job_is_guarded_by_the_heading() {
    let yaml = std::fs::read_to_string(repo_root().join(".github/workflows/scheduled.yml"))
        .unwrap_or_else(|e| panic!("read scheduled.yml: {e}"))
        .replace("\r\n", "\n");
    assert!(check_heading_guard(&yaml).is_ok());

    let temp = tempfile::tempdir().unwrap_or_else(|e| panic!("create tempdir: {e}"));
    let with_heading = temp.path().join("with_heading.md");
    std::fs::write(&with_heading, "# Title\n\n## edition-001\n- note\n")
        .unwrap_or_else(|e| panic!("write with_heading: {e}"));
    let status_with = Command::new("grep")
        .args(["-q", "^## edition-"])
        .arg(&with_heading)
        .status()
        .unwrap_or_else(|e| panic!("run grep with_heading: {e}"));
    assert!(status_with.success());

    let without_heading = temp.path().join("without_heading.md");
    std::fs::write(&without_heading, "# Title\n\nNo edition heading here\n")
        .unwrap_or_else(|e| panic!("write without_heading: {e}"));
    let status_without = Command::new("grep")
        .args(["-q", "^## edition-"])
        .arg(&without_heading)
        .status()
        .unwrap_or_else(|e| panic!("run grep without_heading: {e}"));
    assert_eq!(status_without.code(), Some(1));
}

// Planted mutations dropping the heading guard or reordering checkout are rejected.
#[test]
fn edition_guard_rejects_each_plant() {
    let real_yml = std::fs::read_to_string(repo_root().join(".github/workflows/scheduled.yml"))
        .unwrap_or_else(|e| panic!("read scheduled.yml: {e}"))
        .replace("\r\n", "\n");

    let target_a =
        "      - name: Package the edition\n        if: steps.heading.outputs.run == 'true'\n";
    let replace_a = "      - name: Package the edition\n";
    let plant_a = real_yml.replace(target_a, replace_a);
    assert_ne!(plant_a, real_yml, "plant (a) must change text");
    assert!(
        check_heading_guard(&plant_a).is_err(),
        "check_heading_guard must fail on plant (a)"
    );

    let checkout_block = "      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1\n        with:\n          fetch-depth: 0\n          persist-credentials: false\n";
    let heading_block = "      - name: Look for an edition heading\n        id: heading\n        run: |\n          set -euo pipefail\n          if grep -q \"^## edition-\" corpus/changes.md; then\n            echo \"run=true\" >> \"$GITHUB_OUTPUT\"\n            number=\"$(grep -m 1 \"^## edition-\" corpus/changes.md | sed 's/^## edition-//' | tr -d '[:space:]')\"\n            echo \"number=$number\" >> \"$GITHUB_OUTPUT\"\n          else\n            echo \"::notice::no edition to build\"\n          fi\n";

    let target_b = format!("{checkout_block}{heading_block}");
    let replace_b = format!("{heading_block}{checkout_block}");
    let plant_b = real_yml.replace(&target_b, &replace_b);
    assert_ne!(plant_b, real_yml, "plant (b) must change text");
    assert!(
        check_heading_guard(&plant_b).is_err(),
        "check_heading_guard must fail on plant (b)"
    );
}

// The edition artifact upload includes packages and hashes for cross-OS comparison.
#[test]
fn edition_artifact_contains_both_packages_and_sums() {
    let real_yml = std::fs::read_to_string(repo_root().join(".github/workflows/scheduled.yml"))
        .unwrap_or_else(|e| panic!("read scheduled.yml: {e}"))
        .replace("\r\n", "\n");

    assert!(check_edition_artifact(&real_yml).is_ok());

    let plant_no_edition = real_yml
        .lines()
        .filter(|l| !l.contains("fieldbook-edition-"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_ne!(plant_no_edition, real_yml);
    assert!(check_edition_artifact(&plant_no_edition).is_err());

    let plant_no_originals = real_yml
        .lines()
        .filter(|l| !l.contains("fieldbook-originals-"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_ne!(plant_no_originals, real_yml);
    assert!(check_edition_artifact(&plant_no_originals).is_err());

    let plant_no_sums = real_yml
        .lines()
        .filter(|l| !l.contains("out/dist/SHA256SUMS"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_ne!(plant_no_sums, real_yml);
    assert!(check_edition_artifact(&plant_no_sums).is_err());
}

// Secrets and lock jobs install just at the pinned version before invoking it.
#[test]
fn secrets_and_lock_jobs_install_just_before_using_it() {
    let real_yml = std::fs::read_to_string(repo_root().join(".github/workflows/scheduled.yml"))
        .unwrap_or_else(|e| panic!("read scheduled.yml: {e}"))
        .replace("\r\n", "\n");

    assert!(check_just_installed(&real_yml, "secrets").is_ok());
    assert!(check_just_installed(&real_yml, "lock").is_ok());

    let line_sec = "          tool: just@1.58.0,cargo-nextest@0.9.148\n";
    let repl_sec = "          tool: cargo-nextest@0.9.148\n";
    let plant_sec = real_yml.replace(line_sec, repl_sec);
    assert_ne!(plant_sec, real_yml);
    assert!(check_just_installed(&plant_sec, "secrets").is_err());

    let line_lock =
        "          tool: just@1.58.0,cargo-nextest@0.9.148,cargo-deny@0.20.2,typos@1.51.1\n";
    let repl_lock = "          tool: cargo-nextest@0.9.148,cargo-deny@0.20.2,typos@1.51.1\n";
    let plant_lock = real_yml.replace(line_lock, repl_lock);
    assert_ne!(plant_lock, real_yml);
    assert!(check_just_installed(&plant_lock, "lock").is_err());
}

// The edition job executes cargo xtask edition exactly once with no extraneous build commands.
#[test]
fn edition_job_has_one_edition_operation() {
    let real_yml = std::fs::read_to_string(repo_root().join(".github/workflows/scheduled.yml"))
        .unwrap_or_else(|e| panic!("read scheduled.yml: {e}"))
        .replace("\r\n", "\n");

    assert!(check_one_edition_operation(&real_yml).is_ok());

    let step_build = "      - name: Build the site\n        if: steps.heading.outputs.run == 'true'\n        run: just site\n";
    let plant1 = real_yml.replace(
        "      - name: Package the edition\n",
        &format!("{step_build}      - name: Package the edition\n"),
    );
    assert_ne!(plant1, real_yml);
    assert!(check_one_edition_operation(&plant1).is_err());

    let step_harness = "      - name: Harness\n        if: steps.heading.outputs.run == 'true'\n        run: cargo xtask harness\n";
    let plant2 = real_yml.replace(
        "        run: cargo xtask edition\n",
        &format!("        run: cargo xtask edition\n{step_harness}"),
    );
    assert_ne!(plant2, real_yml);
    assert!(check_one_edition_operation(&plant2).is_err());

    let plant3 = real_yml.replace("run: cargo xtask edition", "run: echo skipped");
    assert_ne!(plant3, real_yml);
    assert!(check_one_edition_operation(&plant3).is_err());
}

// The security contact secret is bound only to the cargo xtask edition packaging step.
#[test]
fn security_contact_is_bound_only_where_the_edition_is_packaged() {
    let real_yml = std::fs::read_to_string(repo_root().join(".github/workflows/scheduled.yml"))
        .unwrap_or_else(|e| panic!("read scheduled.yml: {e}"))
        .replace("\r\n", "\n");

    assert!(check_security_contact(&real_yml).is_ok());

    let binding =
        "          FIELDBOOK_SECURITY_CONTACT: ${{ secrets.FIELDBOOK_SECURITY_CONTACT }}\n";
    let plant1 = real_yml.replace(binding, "");
    assert_ne!(plant1, real_yml);
    assert!(check_security_contact(&plant1).is_err());

    let other = "          OTHER_TOKEN: ${{ secrets.OTHER_TOKEN }}\n";
    let plant2 = real_yml.replace(binding, &format!("{binding}{other}"));
    assert_ne!(plant2, real_yml);
    assert!(check_security_contact(&plant2).is_err());

    let plant3 = real_yml.replace(
        "secrets.FIELDBOOK_SECURITY_CONTACT",
        "vars.FIELDBOOK_SECURITY_CONTACT",
    );
    assert_ne!(plant3, real_yml);
    assert!(check_security_contact(&plant3).is_err());
}

// Packaging an edition without a populated security contact fails closed with IN-01.
#[test]
fn edition_without_the_security_contact_is_refused_with_in_01() {
    let temp = tempfile::tempdir().unwrap_or_else(|e| panic!("create tempdir: {e}"));
    let temp_path = temp.path();
    git(temp_path, &["init", "-q"]);

    let corpus_dir = temp_path.join("corpus");
    std::fs::create_dir_all(&corpus_dir).unwrap_or_else(|e| panic!("create corpus dir: {e}"));
    std::fs::write(
        corpus_dir.join("changes.md"),
        "# Corpus changes\n\n## edition-001\n\n- Added: synthetic\n",
    )
    .unwrap_or_else(|e| panic!("write changes.md: {e}"));

    git(temp_path, &["add", "corpus/changes.md"]);
    git(temp_path, &["commit", "-m", "synthetic changes"]);

    let cases: &[Option<&str>] = &[None, Some(""), Some("   ")];
    for &case in cases {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_xtask"));
        cmd.arg("edition")
            .env("XTASK_REPO_ROOT", temp_path)
            .current_dir(temp_path);
        match case {
            None => {
                cmd.env_remove("FIELDBOOK_SECURITY_CONTACT");
            }
            Some(val) => {
                cmd.env("FIELDBOOK_SECURITY_CONTACT", val);
            }
        }
        let output = cmd
            .output()
            .unwrap_or_else(|e| panic!("run xtask edition: {e}"));
        assert_eq!(
            output.status.code(),
            Some(1),
            "expected exit code 1 for case {case:?}"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("IN-01") && stderr.contains("FIELDBOOK_SECURITY_CONTACT"),
            "stderr for case {case:?} missing IN-01 or FIELDBOOK_SECURITY_CONTACT:\n{stderr}"
        );
    }
}

// Scheduled workflow permissions, triggers, runner, and action pins match repository policy.
#[test]
fn scheduled_workflow_is_read_only_pinned_and_runs_where_ci_does() {
    let real_yml = std::fs::read_to_string(repo_root().join(".github/workflows/scheduled.yml"))
        .unwrap_or_else(|e| panic!("read scheduled.yml: {e}"))
        .replace("\r\n", "\n");

    // 1. permissions: exactly one non-comment line, contents: read next, no write in block
    let non_comment: Vec<&str> = real_yml
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with('#')
        })
        .collect();
    let perm_indices: Vec<usize> = non_comment
        .iter()
        .enumerate()
        .filter(|(_, l)| l.starts_with("permissions:"))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        perm_indices.len(),
        1,
        "expected exactly one non-comment line starting `permissions:`"
    );
    let p_idx = perm_indices[0];
    assert_eq!(
        non_comment[p_idx + 1],
        "  contents: read",
        "line after `permissions:` must be `  contents: read`"
    );

    for line in &non_comment[p_idx..] {
        if line.starts_with("permissions:") || line.starts_with("  ") {
            assert!(
                !line.contains("write"),
                "permissions block must not contain `write`: `{line}`"
            );
        } else {
            break;
        }
    }

    // 2. on: keys schedule:, workflow_dispatch:, pull_request:, one - cron:, paths: list
    let on_idx = non_comment
        .iter()
        .position(|l| *l == "on:")
        .expect("missing on: line");
    let mut on_block = Vec::new();
    for line in &non_comment[on_idx + 1..] {
        if !line.starts_with(' ') {
            break;
        }
        on_block.push(*line);
    }
    let two_space_keys: Vec<&str> = on_block
        .iter()
        .filter(|l| l.starts_with("  ") && !l.starts_with("   "))
        .map(|l| l.trim())
        .collect();
    assert_eq!(
        two_space_keys,
        ["schedule:", "workflow_dispatch:", "pull_request:"]
    );
    let cron_lines: Vec<&str> = on_block
        .iter()
        .filter(|l| l.trim().starts_with("- cron:"))
        .copied()
        .collect();
    assert_eq!(cron_lines.len(), 1, "expected exactly one `- cron:` line");

    let paths_line = on_block
        .iter()
        .find(|l| l.trim().starts_with("paths:"))
        .expect("missing paths: line under pull_request");
    for expected_path in [
        ".github/workflows/scheduled.yml",
        "xtask/tests/advisories_fixture/**",
        "xtask/tests/scheduled_controls.rs",
        "clippy.toml",
    ] {
        assert!(
            paths_line.contains(expected_path),
            "paths line missing `{expected_path}`: `{paths_line}`"
        );
    }

    // 3. job ids exactly, in order: advisories, secrets, lint-controls, lock, edition-linux,
    // scale, freshness
    let expected_jobs = [
        "advisories",
        "secrets",
        "lint-controls",
        "lock",
        "edition-linux",
        "scale",
        "freshness",
    ];
    let jobs = job_names(&real_yml);
    assert_eq!(jobs, expected_jobs);

    // 4. every job has runs-on: [self-hosted, Linux, X64, fieldbook] and event filter
    const EXPECTED_RUNS_ON_COMPACT: &str = "runs-on:[self-hosted,Linux,X64,fieldbook]";
    const EXPECTED_IF: &str = "if: github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository";

    for job in &jobs {
        let text = job_text(&real_yml, job).expect("job text");
        let has_runs_on = text.lines().any(|l| {
            l.chars().filter(|c| !c.is_whitespace()).collect::<String>() == EXPECTED_RUNS_ON_COMPACT
        });
        assert!(
            has_runs_on,
            "job `{job}` missing runs-on: [self-hosted, Linux, X64, fieldbook]"
        );
        let has_if = text.lines().any(|l| l.trim() == EXPECTED_IF);
        assert!(has_if, "job `{job}` missing expected `if:` line");
    }

    // 5. every uses: line is pinned, in other workflows, and SHA in dependencies.md
    assert!(check_uses_pinned(&real_yml).is_ok());

    // 6. every actions/checkout step has persist-credentials: false and fetch-depth: 0
    for job in &jobs {
        let steps = job_steps(&real_yml, job).expect("steps");
        for step in &steps {
            if step.contains("actions/checkout@") {
                assert!(
                    step.contains("persist-credentials: false"),
                    "job `{job}` checkout missing `persist-credentials: false`"
                );
                assert!(
                    step.contains("fetch-depth: 0"),
                    "job `{job}` checkout missing `fetch-depth: 0`"
                );
            }
        }
    }

    // 7. no line contains continue-on-error outside the advisory freshness step, set -x or
    // pull_request_target; the freshness step is advisory by design and carries exactly one.
    let freshness = job_text(&real_yml, "freshness").expect("freshness job text");
    let mut continue_on_error_outside_freshness = 0usize;
    for line in real_yml.lines() {
        if line.contains("continue-on-error") && !freshness.lines().any(|own| own == line) {
            continue_on_error_outside_freshness += 1;
        }
        assert!(!line.contains("set -x"), "line contains set -x: `{line}`");
        assert!(
            !line.contains("pull_request_target"),
            "line contains pull_request_target: `{line}`"
        );
    }
    assert_eq!(
        continue_on_error_outside_freshness, 0,
        "continue-on-error belongs to the freshness job only"
    );
    assert_eq!(
        freshness
            .lines()
            .filter(|line| line.trim() == "continue-on-error: true")
            .count(),
        1,
        "the freshness step must be advisory"
    );

    // 8. every tool: pin name@version equals .config/tools.toml [cargo] version
    let tools_toml_path = repo_root().join(".config/tools.toml");
    let tools_toml = std::fs::read_to_string(&tools_toml_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", tools_toml_path.display()));

    let mut cargo_tools = Vec::new();
    let mut in_cargo = false;
    for line in tools_toml.lines() {
        let t = line.trim();
        if t == "[cargo]" {
            in_cargo = true;
        } else if in_cargo {
            if t.starts_with('[') {
                break;
            }
            if let Some((k, v)) = t.split_once('=') {
                cargo_tools.push((k.trim().to_string(), v.trim().trim_matches('"').to_string()));
            }
        }
    }

    for line in real_yml.lines() {
        let t = line.trim();
        if let Some(tool_list) = t.strip_prefix("tool:") {
            for item in tool_list.split(',') {
                let item_trimmed = item.trim();
                let (name, ver) = item_trimmed
                    .split_once('@')
                    .unwrap_or_else(|| panic!("invalid tool pin `{item_trimmed}`"));
                let key = if name == "typos" { "typos-cli" } else { name };
                let expected_ver = cargo_tools
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v.as_str())
                    .unwrap_or_else(|| panic!("tool `{key}` not in .config/tools.toml [cargo]"));
                assert_eq!(
                    ver, expected_ver,
                    "tool pin `{name}@{ver}` disagrees with .config/tools.toml `{expected_ver}`"
                );
            }
        }
    }

    // 9. scale job: cargo xtask scale step has shell: bash and upload is name: scale-table, retention-days: 14
    let scale_steps = job_steps(&real_yml, "scale").expect("scale steps");
    let scale_step = scale_steps
        .iter()
        .find(|s| {
            step_run(s).is_some_and(|r| r.contains("cargo xtask scale | tee scale-table.txt"))
        })
        .expect("missing cargo xtask scale step");
    assert!(
        scale_step.lines().any(|l| l.trim() == "shell: bash"),
        "scale command step missing `shell: bash`"
    );

    let scale_upload = scale_steps
        .iter()
        .find(|s| {
            step_uses_line(s).is_some_and(|l| {
                l.starts_with("uses: actions/upload-artifact@")
                    || l.starts_with("- uses: actions/upload-artifact@")
            })
        })
        .expect("missing scale upload-artifact step");
    assert!(
        scale_upload.contains("name: scale-table"),
        "scale upload missing `name: scale-table`"
    );
    assert!(
        scale_upload.contains("retention-days: 14"),
        "scale upload missing `retention-days: 14`"
    );

    // 10. Planted check: unpinned action must fail check_uses_pinned
    let plant_unpinned = real_yml.replace(
        "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1",
        "actions/checkout@v7",
    );
    assert_ne!(plant_unpinned, real_yml);
    assert!(check_uses_pinned(&plant_unpinned).is_err());
}

fn check_freshness_job(yaml: &str) -> Result<String, String> {
    let steps = job_steps(yaml, "freshness")?;
    let report_step = steps
        .iter()
        .find(|s| {
            s.lines()
                .any(|l| l.trim() == "- name: Report the stale items")
        })
        .ok_or_else(|| "job `freshness` missing `Report the stale items` step".to_string())?;

    if !report_step.lines().any(|l| l.trim() == "shell: bash") {
        return Err("`Report the stale items` missing `shell: bash`".to_string());
    }
    if !report_step
        .lines()
        .any(|l| l.trim() == "continue-on-error: true")
    {
        return Err("`Report the stale items` missing `continue-on-error: true`".to_string());
    }

    let run_text = step_run(report_step)
        .ok_or_else(|| "`Report the stale items` missing `run:` script".to_string())?;

    const STATUS_TO_FILE: &str = "cargo run --locked -p fieldbook -- status --today \"$(git log -1 --format=%cs)\" > freshness-report.txt";
    if !run_text.contains(STATUS_TO_FILE) {
        return Err(format!(
            "`Report the stale items` must write full output to `freshness-report.txt` via `{STATUS_TO_FILE}`"
        ));
    }
    if run_text
        .contains("status --today \"$(git log -1 --format=%cs)\" >> \"$GITHUB_STEP_SUMMARY\"")
    {
        return Err(
            "`Report the stale items` must not stream unbounded `fieldbook status` output directly to `$GITHUB_STEP_SUMMARY`"
                .to_string(),
        );
    }
    if !run_text.contains("GITHUB_STEP_SUMMARY") || !run_text.contains("max_bytes=") {
        return Err(
            "`Report the stale items` must bound bytes written to `$GITHUB_STEP_SUMMARY`"
                .to_string(),
        );
    }

    let upload_step = steps
        .iter()
        .find(|s| {
            step_uses_line(s).is_some_and(|l| {
                l.starts_with("uses: actions/upload-artifact@")
                    || l.starts_with("- uses: actions/upload-artifact@")
            })
        })
        .ok_or_else(|| "job `freshness` missing `actions/upload-artifact` step".to_string())?;

    if !upload_step.contains("name: freshness-report") {
        return Err("freshness upload step missing `name: freshness-report`".to_string());
    }
    if !upload_step.contains("path: freshness-report.txt") {
        return Err("freshness upload step missing `path: freshness-report.txt`".to_string());
    }
    if !upload_step.contains("if-no-files-found: ignore") {
        return Err("freshness upload step missing `if-no-files-found: ignore`".to_string());
    }
    if !upload_step.contains("retention-days: 14") {
        return Err("freshness upload step missing `retention-days: 14`".to_string());
    }

    Ok(run_text)
}

#[test]
fn freshness_job_bounds_step_summary_and_uploads_full_report() {
    const GITHUB_STEP_SUMMARY_LIMIT: usize = 1_048_576;
    const STATUS_TO_FILE: &str = "cargo run --locked -p fieldbook -- status --today \"$(git log -1 --format=%cs)\" > freshness-report.txt";

    let real_yml = std::fs::read_to_string(repo_root().join(".github/workflows/scheduled.yml"))
        .unwrap_or_else(|e| panic!("read scheduled.yml: {e}"))
        .replace("\r\n", "\n");

    let run_text = check_freshness_job(&real_yml)
        .unwrap_or_else(|e| panic!("freshness job check failed: {e}"));

    // Planted regressions: streaming unbounded output or dropping the artifact upload must fail.
    let plant_unbounded = real_yml.replace(
        STATUS_TO_FILE,
        "cargo run --locked -p fieldbook -- status --today \"$(git log -1 --format=%cs)\" >> \"$GITHUB_STEP_SUMMARY\"",
    );
    assert_ne!(plant_unbounded, real_yml);
    assert!(check_freshness_job(&plant_unbounded).is_err());

    let plant_no_artifact = real_yml.replace("name: freshness-report", "name: wrong-artifact");
    assert_ne!(plant_no_artifact, real_yml);
    assert!(check_freshness_job(&plant_no_artifact).is_err());

    // Execute the real workflow bash logic against small, >1.5 MiB multiline UTF-8, and
    // >1.5 MiB single-line UTF-8 reports.
    let harness_script = run_text.replace(
        STATUS_TO_FILE,
        "cp \"$SYNTHETIC_REPORT\" freshness-report.txt",
    );
    assert_ne!(harness_script, run_text);

    let temp = tempfile::tempdir().unwrap_or_else(|e| panic!("create tempdir: {e}"));

    // 1. Small report stays byte-for-byte identical.
    let small_dir = temp.path().join("small");
    std::fs::create_dir_all(&small_dir).unwrap_or_else(|e| panic!("mkdir small: {e}"));
    let small_input = small_dir.join("input.txt");
    let small_summary = small_dir.join("summary.md");
    let small_body = "Counts\n  2 claims\n\nNext actions\n  none\n";
    std::fs::write(&small_input, small_body).unwrap_or_else(|e| panic!("write small: {e}"));
    std::fs::write(&small_summary, "").unwrap_or_else(|e| panic!("init summary: {e}"));

    let out_small = Command::new("bash")
        .arg("-c")
        .arg(&harness_script)
        .current_dir(&small_dir)
        .env("SYNTHETIC_REPORT", &small_input)
        .env("GITHUB_STEP_SUMMARY", &small_summary)
        .output()
        .unwrap_or_else(|e| panic!("run bash small: {e}"));
    assert!(
        out_small.status.success(),
        "small report script failed:\n{}",
        combined(&out_small)
    );
    let small_written =
        std::fs::read_to_string(&small_summary).unwrap_or_else(|e| panic!("read small sum: {e}"));
    assert_eq!(small_written, small_body);

    // 2. Large (>1.5 MiB) multiline UTF-8 report stays well below 1 MiB, valid UTF-8, preserves
    // full freshness-report.txt, keeps header + Next actions tail, and prints truncation notice.
    let large_dir = temp.path().join("large");
    std::fs::create_dir_all(&large_dir).unwrap_or_else(|e| panic!("mkdir large: {e}"));
    let large_input = large_dir.join("input.txt");
    let large_summary = large_dir.join("summary.md");

    let mut large_body = String::from("Counts\n  26000 stale claims\n\nStale\n");
    for i in 0..26_000 {
        large_body.push_str(&format!(
            "  C-{i:05} — review_by 2026-01-01 passed (café naïve résumé 🦀)\n"
        ));
    }
    large_body.push_str("\nNext actions\n  fieldbook verify <claim-id> --on YYYY-MM-DD\n");
    assert!(
        large_body.len() > 1_500_000,
        "synthetic report must exceed 1.5 MiB, got {}",
        large_body.len()
    );
    std::fs::write(&large_input, &large_body).unwrap_or_else(|e| panic!("write large: {e}"));
    std::fs::write(&large_summary, "").unwrap_or_else(|e| panic!("init large sum: {e}"));

    let out_large = Command::new("bash")
        .arg("-c")
        .arg(&harness_script)
        .current_dir(&large_dir)
        .env("SYNTHETIC_REPORT", &large_input)
        .env("GITHUB_STEP_SUMMARY", &large_summary)
        .output()
        .unwrap_or_else(|e| panic!("run bash large: {e}"));
    assert!(
        out_large.status.success(),
        "large report script failed:\n{}",
        combined(&out_large)
    );

    let full_artifact = std::fs::read(large_dir.join("freshness-report.txt"))
        .unwrap_or_else(|e| panic!("read freshness-report.txt: {e}"));
    assert_eq!(
        full_artifact,
        large_body.as_bytes(),
        "freshness-report.txt must retain the complete untruncated output"
    );

    let summary_bytes =
        std::fs::read(&large_summary).unwrap_or_else(|e| panic!("read large summary: {e}"));
    assert!(
        summary_bytes.len() < GITHUB_STEP_SUMMARY_LIMIT,
        "step summary must stay below GitHub's 1 MiB limit, got {} bytes",
        summary_bytes.len()
    );
    let summary_text = std::str::from_utf8(&summary_bytes)
        .unwrap_or_else(|e| panic!("step summary must remain valid UTF-8: {e}"));
    assert!(summary_text.starts_with("Counts\n  26000 stale claims\n"));
    assert!(
        summary_text.contains("freshness-report") && summary_text.contains("truncated"),
        "step summary must explain truncation and point to freshness-report artifact"
    );
    assert!(
        summary_text.ends_with("\nNext actions\n  fieldbook verify <claim-id> --on YYYY-MM-DD\n"),
        "step summary must retain trailing Next actions section"
    );

    // 3. Pathological >1.5 MiB single-line multi-byte UTF-8 report (no newlines) still stays
    // valid UTF-8 and below 1 MiB.
    let single_dir = temp.path().join("single");
    std::fs::create_dir_all(&single_dir).unwrap_or_else(|e| panic!("mkdir single: {e}"));
    let single_input = single_dir.join("input.txt");
    let single_summary = single_dir.join("summary.md");
    let single_body = "🦀é—".repeat(200_000);
    assert!(single_body.len() > 1_500_000);
    std::fs::write(&single_input, &single_body).unwrap_or_else(|e| panic!("write single: {e}"));
    std::fs::write(&single_summary, "").unwrap_or_else(|e| panic!("init single sum: {e}"));

    let out_single = Command::new("bash")
        .arg("-c")
        .arg(&harness_script)
        .current_dir(&single_dir)
        .env("SYNTHETIC_REPORT", &single_input)
        .env("GITHUB_STEP_SUMMARY", &single_summary)
        .output()
        .unwrap_or_else(|e| panic!("run bash single: {e}"));
    assert!(
        out_single.status.success(),
        "single-line report script failed:\n{}",
        combined(&out_single)
    );
    let single_bytes =
        std::fs::read(&single_summary).unwrap_or_else(|e| panic!("read single summary: {e}"));
    assert!(single_bytes.len() < GITHUB_STEP_SUMMARY_LIMIT);
    let single_text = std::str::from_utf8(&single_bytes)
        .unwrap_or_else(|e| panic!("single-line summary must remain valid UTF-8: {e}"));
    assert!(single_text.contains("freshness-report") && single_text.contains("truncated"));
}
