//! Hosted deploy workflow integration tests (`hosted-deploy`).

#![expect(
    clippy::disallowed_methods,
    reason = "Hosted workflow integration tests read workflow and deploy files"
)]

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn workflow_files() -> Vec<(String, String)> {
    let dir = repo_root().join(".github/workflows");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
        .filter_map(|e| {
            let entry = e.unwrap_or_else(|err| panic!("entry in {}: {err}", dir.display()));
            let p = entry.path();
            (p.is_file() && p.extension().is_some_and(|s| s == "yml")).then(|| {
                let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
                    panic!("file name utf8")
                };
                let text = std::fs::read_to_string(&p)
                    .unwrap_or_else(|err| panic!("read {}: {err}", p.display()));
                (format!(".github/workflows/{name}"), text)
            })
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

fn collect_scanned_files(dir: &Path, prefix: &str, files: &mut Vec<(String, String)>) {
    for e in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display())) {
        let entry = e.unwrap_or_else(|err| panic!("entry in {}: {err}", dir.display()));
        let p = entry.path();
        let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
            panic!("file name utf8")
        };
        let rel = format!("{prefix}/{name}");
        if rel == "deploy/package-lock.json"
            || rel.starts_with("deploy/node_modules")
            || rel.starts_with("deploy/.wrangler")
        {
            continue;
        }
        if p.is_dir() {
            collect_scanned_files(&p, &rel, files);
        } else if p.is_file() {
            let text = std::fs::read_to_string(&p)
                .unwrap_or_else(|err| panic!("read {}: {err}", p.display()));
            files.push((rel, text));
        }
    }
}

fn identifier_files() -> Vec<(String, String)> {
    let mut files = Vec::new();
    collect_scanned_files(&repo_root().join(".github"), ".github", &mut files);
    collect_scanned_files(&repo_root().join("deploy"), "deploy", &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

fn check_uses_pinned(action_ref: &str, line_num: usize) {
    let Some((_action, pin_and_comment)) = action_ref.split_once('@') else {
        panic!("line {line_num}: uses line has no '@': {action_ref}");
    };
    let Some((sha, comment)) = pin_and_comment.split_once(" # v") else {
        panic!("line {line_num}: missing ' # v': {action_ref}");
    };
    assert!(
        sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()),
        "line {line_num}: invalid SHA: {sha}"
    );
    assert!(
        !comment.is_empty(),
        "line {line_num}: empty version comment"
    );
}

fn is_valid_secret_env_line(trimmed: &str) -> bool {
    let Some((key, val)) = trimmed.split_once(": ") else {
        return false;
    };
    !key.is_empty()
        && key.chars().all(|c| c.is_ascii_uppercase() || c == '_')
        && val == format!("${{{{ secrets.{key} }}}}")
}

fn check_secret_use(files: &[(String, String)]) -> Result<(), String> {
    let mut hosted_content: Option<&str> = None;
    for (path, content) in files {
        if path == ".github/workflows/hosted.yml" {
            hosted_content = Some(content);
        } else {
            if content.contains("CLOUDFLARE_API_TOKEN") {
                return Err(format!("{path}: contains CLOUDFLARE_API_TOKEN"));
            }
            if content.contains("CLOUDFLARE_ACCOUNT_ID") {
                return Err(format!("{path}: contains CLOUDFLARE_ACCOUNT_ID"));
            }
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("environment:") && trimmed.contains("hosted") {
                    return Err(format!("{path}: names hosted environment: `{line}`"));
                }
            }
        }
    }

    let Some(hosted) = hosted_content else {
        return Err("missing .github/workflows/hosted.yml".to_string());
    };
    let count = hosted
        .lines()
        .filter(|l| l.trim() == "environment: hosted")
        .count();
    if count != 1 {
        return Err(format!(
            ".github/workflows/hosted.yml: expected `environment: hosted` once, found {count}"
        ));
    }
    Ok(())
}

fn check_runner(yml: &str) -> Result<(), String> {
    let mut count = 0;
    for line in yml.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("runs-on:") {
            count += 1;
            if line.contains("self-hosted") {
                return Err(format!("runs-on line contains self-hosted: `{line}`"));
            }
            let val = rest.trim();
            let label = val
                .strip_prefix('[')
                .and_then(|s| s.strip_suffix(']'))
                .unwrap_or(val)
                .trim();
            if label.trim_matches(['\'', '"']) != "ubuntu-24.04" {
                return Err(format!("runs-on line does not name ubuntu-24.04: `{line}`"));
            }
        }
    }
    if count == 0 {
        return Err("no runs-on: line found in workflow".to_string());
    }
    Ok(())
}

fn has_workers_dev_label(content: &str) -> bool {
    let target = ".workers.dev";
    let mut search_from = 0;
    while let Some(pos) = content[search_from..].find(target) {
        let abs = search_from + pos;
        if abs > 0
            && content[..abs]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return true;
        }
        search_from = abs + target.len();
    }
    false
}

fn has_32_hex_run(text: &str) -> bool {
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_hexdigit() {
            let start = i;
            while i < b.len() && b[i].is_ascii_hexdigit() {
                i += 1;
            }
            if i - start == 32 {
                return true;
            }
        } else {
            i += 1;
        }
    }
    false
}

fn has_email_shape(text: &str) -> bool {
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b != b'@' {
            continue;
        }
        let mut start = i;
        while start > 0
            && (bytes[start - 1].is_ascii_alphanumeric()
                || matches!(bytes[start - 1], b'_' | b'-' | b'.' | b'+'))
        {
            start -= 1;
        }
        let local = text[start..i].trim_matches(['.', '-', '+', '_']);
        if local.is_empty() || !local.chars().any(|c| c.is_ascii_alphabetic()) {
            continue;
        }

        let mut end = i + 1;
        while end < bytes.len()
            && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'-' | b'.'))
        {
            end += 1;
        }
        let domain = text[i + 1..end].trim_matches(['.', '-']);
        let labels: Vec<&str> = domain.split('.').collect();
        if labels.len() >= 2
            && labels
                .last()
                .is_some_and(|l| l.len() >= 2 && l.chars().all(|c| c.is_ascii_alphabetic()))
            && labels
                .iter()
                .all(|l| !l.is_empty() && l.chars().any(|c| c.is_ascii_alphabetic()))
        {
            return true;
        }
    }
    false
}

fn check_identifiers(files: &[(String, String)]) -> Result<(), String> {
    for (path, content) in files {
        if has_workers_dev_label(content) {
            return Err(format!("{path}: contains workers.dev label"));
        }
        let is_exempt = path == "deploy/check-login-gate.sh" || path.starts_with("xtask/tests/");
        if !is_exempt && content.contains("cloudflareaccess.com") {
            return Err(format!("{path}: contains cloudflareaccess.com"));
        }
        if has_32_hex_run(content) {
            return Err(format!("{path}: contains 32-hex identifier"));
        }
        if has_email_shape(content) {
            return Err(format!("{path}: contains email address"));
        }
    }
    Ok(())
}

#[test]
fn workflow_is_pinned_read_only_and_serialised() {
    let yml = std::fs::read_to_string(repo_root().join(".github/workflows/hosted.yml"))
        .unwrap_or_else(|e| panic!("read hosted.yml: {e}"))
        .replace("\r\n", "\n");

    assert!(yml.contains("permissions:\n  contents: read"));
    assert_eq!(
        yml.lines().filter(|l| l.trim() == "permissions:").count(),
        1
    );
    assert!(yml.contains("group: deploy-hosted"));
    assert!(yml.contains("cancel-in-progress: false"));
    assert!(!yml.contains("set -x"));

    let mut uses = 0;
    for (i, line) in yml.lines().enumerate() {
        let t = line.trim();
        if let Some(r) = t
            .strip_prefix("- uses:")
            .or_else(|| t.strip_prefix("uses:"))
        {
            uses += 1;
            check_uses_pinned(r.trim(), i + 1);
        }
        if line.contains("secrets.") {
            assert!(
                is_valid_secret_env_line(t),
                "line {}: invalid secret: {line}",
                i + 1
            );
        }
    }
    assert!(uses >= 4, "expected >= 4 uses lines, found {uses}");
}

// control: HO-04
#[test]
fn only_the_hosted_workflow_names_the_deploy_secrets_and_environment() {
    let files = workflow_files();
    let res = check_secret_use(&files);
    assert!(
        res.is_ok(),
        "check_secret_use failed on real files: {res:?}"
    );
}

// control: HO-04
#[test]
fn a_planted_secret_or_environment_in_another_workflow_is_refused() {
    let real_files = workflow_files();

    // Plant 1: append CLOUDFLARE_API_TOKEN to ci.yml
    let mut plant1 = real_files.clone();
    let ci1 = plant1
        .iter_mut()
        .find(|(p, _)| p == ".github/workflows/ci.yml")
        .expect("ci.yml");
    ci1.1.push_str("\n      CLOUDFLARE_API_TOKEN: x\n");
    let err1 = check_secret_use(&plant1).unwrap_err();
    assert!(
        err1.contains(".github/workflows/ci.yml"),
        "expected ci.yml in {err1}"
    );

    // Plant 2: append environment: hosted to ci.yml
    let mut plant2 = real_files.clone();
    let ci2 = plant2
        .iter_mut()
        .find(|(p, _)| p == ".github/workflows/ci.yml")
        .expect("ci.yml");
    ci2.1.push_str("\n    environment: hosted\n");
    assert!(
        check_secret_use(&plant2)
            .unwrap_err()
            .contains(".github/workflows/ci.yml")
    );

    // Plant 3: remove environment: hosted from hosted.yml
    let mut plant3 = real_files;
    let hosted = &mut plant3
        .iter_mut()
        .find(|(p, _)| p == ".github/workflows/hosted.yml")
        .expect("hosted.yml")
        .1;
    *hosted = hosted
        .lines()
        .filter(|l| l.trim() != "environment: hosted")
        .collect::<Vec<_>>()
        .join("\n");
    assert!(check_secret_use(&plant3).is_err());
}

// control: HO-05
#[test]
fn the_deploy_job_runs_on_a_github_hosted_runner() {
    let yml = std::fs::read_to_string(repo_root().join(".github/workflows/hosted.yml"))
        .unwrap_or_else(|e| panic!("read hosted.yml: {e}"));
    let count = yml
        .lines()
        .filter(|l| l.trim().starts_with("runs-on:"))
        .count();
    assert_eq!(
        count, 1,
        "hosted.yml must contain exactly one runs-on: line"
    );
    assert!(check_runner(&yml).is_ok());
}

// control: HO-05
#[test]
fn the_self_hosted_labels_of_ci_are_refused() {
    let hosted_yml = std::fs::read_to_string(repo_root().join(".github/workflows/hosted.yml"))
        .unwrap_or_else(|e| panic!("read hosted.yml: {e}"));
    let planted = hosted_yml
        .lines()
        .map(|l| {
            if l.trim().starts_with("runs-on:") {
                "    runs-on: [self-hosted, Linux, X64, fieldbook]"
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(check_runner(&planted).is_err());

    let ci_yml = std::fs::read_to_string(repo_root().join(".github/workflows/ci.yml"))
        .unwrap_or_else(|e| panic!("read ci.yml: {e}"));
    assert!(check_runner(&ci_yml).is_err());
}

// control: HO-06
#[test]
fn no_file_names_an_identifier_shape() {
    let files = identifier_files();
    let res = check_identifiers(&files);
    assert!(
        res.is_ok(),
        "check_identifiers failed on scanned files: {res:?}"
    );
}

// control: HO-06
#[test]
fn a_planted_identifier_shape_is_refused() {
    let real_files = identifier_files();
    let plants = [
        ("deploy/extra1.txt", "my-app.workers.dev"),
        ("deploy/extra2.txt", "team.cloudflareaccess.com"),
        ("deploy/extra3.txt", "0123456789abcdef0123456789abcdef"),
        ("deploy/extra4.txt", "someone@example.org"),
    ];
    for (path, content) in plants {
        let mut files = real_files.clone();
        files.push((path.to_string(), content.to_string()));
        let err = check_identifiers(&files).unwrap_err();
        assert!(err.contains(path), "error must name {path}: {err}");
    }

    let mut near_miss = real_files;
    near_miss.push((
        "deploy/near_miss.txt".to_string(),
        "e9b56f8a44bbfbcaee5de5a4980a3a9bf8e053a4\njust@1.58.0\nworkers_dev\n".to_string(),
    ));
    assert!(check_identifiers(&near_miss).is_ok());
}

#[test]
fn the_deploy_workflow_probes_the_gate_before_and_after_the_upload() {
    let text = std::fs::read_to_string(repo_root().join(".github/workflows/hosted.yml"))
        .unwrap_or_else(|e| panic!("read hosted.yml: {e}"));

    let step = |name: &str| {
        text.lines()
            .position(|l| l.contains(name))
            .unwrap_or_else(|| panic!("missing step `{name}`"))
    };
    assert!(step("Check the login gate before the upload") < step("Deploy"));
    assert!(step("Check the login gate after the upload") > step("Deploy"));
    assert!(step("Check the login gate after the upload") > step("Roll back"));
    assert!(
        text.find("hosted-limits").expect("hosted-limits")
            < text.find("wrangler deploy").expect("wrangler deploy")
    );
    assert!(text.contains("npm ci --ignore-scripts"));
    assert!(text.contains("--message \"$GITHUB_SHA\""));
}
