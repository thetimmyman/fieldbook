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
        if content.contains("vars.") {
            return Err(format!(
                "{path}: reads a repository or environment variable (`vars.`)"
            ));
        }
        if path == ".github/workflows/hosted.yml" {
            hosted_content = Some(content);
        } else {
            if content.contains("FIELDBOOK_HOST") {
                return Err(format!("{path}: contains FIELDBOOK_HOST"));
            }
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

fn runs_on_values(yml: &str) -> Vec<String> {
    yml.lines()
        .filter_map(|l| {
            l.trim()
                .strip_prefix("runs-on:")
                .map(|rest| rest.chars().filter(|c| !c.is_whitespace()).collect())
        })
        .collect()
}

fn check_runner(yml: &str) -> Result<(), String> {
    let mut count = 0;
    let mut found_line = "";
    let mut found_val = "";
    for line in yml.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("runs-on:") {
            count += 1;
            found_line = line;
            found_val = rest.trim();
        }
    }
    if count == 0 {
        return Err("no runs-on: line found in workflow".to_string());
    }
    if count != 1 {
        return Err(format!("expected exactly one runs-on: line, found {count}"));
    }
    let compact: String = found_val.chars().filter(|c| !c.is_whitespace()).collect();
    if compact != "[self-hosted,Linux,X64,fieldbook]" {
        return Err(format!("unexpected runs-on value: `{found_line}`"));
    }
    Ok(())
}

fn check_unreleased_gate(yml: &str) -> Result<(), String> {
    let is_grep_line = |l: &str| {
        l.contains("grep")
            && l.contains(fieldbook_core::strings::unreleased_build())
            && l.contains(" -i ")
            && l.contains("out/hosted/index.html")
    };
    let Some(grep_line) = yml.lines().find(|l| is_grep_line(l)) else {
        return Err("missing grep -i for `unreleased build` in out/hosted/index.html".to_string());
    };
    if !grep_line.contains("! grep") {
        return Err(format!(
            "the grep for `unreleased build` must be negated (`! grep`): `{grep_line}`"
        ));
    }
    let Some(grep_offset) = yml.find(grep_line) else {
        return Err("grep line not found".to_string());
    };

    let Some(sha_offset) = yml.find("FIELDBOOK_GIT_SHA+x") else {
        return Err("missing check for `FIELDBOOK_GIT_SHA+x`".to_string());
    };

    let Some(deploy_offset) = yml.find("wrangler deploy") else {
        return Err("missing `wrangler deploy` in workflow".to_string());
    };
    if sha_offset >= deploy_offset {
        return Err("`FIELDBOOK_GIT_SHA+x` check must appear before `wrangler deploy`".to_string());
    }
    if grep_offset >= deploy_offset {
        return Err("the `unreleased build` grep must appear before `wrangler deploy`".to_string());
    }

    for line in yml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("FIELDBOOK_GIT_SHA:") {
            return Err(format!("workflow sets FIELDBOOK_GIT_SHA as key: `{line}`"));
        }
        if line.contains("export FIELDBOOK_GIT_SHA") {
            return Err(format!("workflow exports FIELDBOOK_GIT_SHA: `{line}`"));
        }
        if line.contains("FIELDBOOK_GIT_SHA=") {
            return Err(format!("workflow assigns FIELDBOOK_GIT_SHA: `{line}`"));
        }
    }

    Ok(())
}

/// The flags that would point a hosted-build step at a data root, store, overlay, policy file or state folder.
const FORBIDDEN_FLAGS: [&str; 7] = [
    "--data-root",
    "--store",
    "--overlay",
    "--policy",
    "--state",
    "--program-data",
    "--local-app-data",
];

fn check_job_hardening(yml: &str) -> Result<(), String> {
    let lines: Vec<&str> = yml.lines().collect();
    let code: Vec<(usize, &str)> = lines
        .iter()
        .enumerate()
        .map(|(i, l)| (i, l.trim()))
        .filter(|(_, t)| !t.starts_with('#'))
        .collect();

    // The checkout step is clean and keeps no credentials.
    let mut checkouts = 0;
    for (i, t) in &code {
        if !t.contains("actions/checkout@") {
            continue;
        }
        checkouts += 1;
        let step: Vec<&str> = lines[i + 1..]
            .iter()
            .map(|l| l.trim())
            .take_while(|l| !l.starts_with("- "))
            .collect();
        for needed in ["clean: true", "persist-credentials: false"] {
            if !step.contains(&needed) {
                return Err(format!("the checkout step lacks `{needed}`"));
            }
        }
    }
    if checkouts == 0 {
        return Err("no actions/checkout step".to_string());
    }

    // out/ is rebuilt from nothing: removed before anything builds.
    let Some(rm_line) = code
        .iter()
        .find(|(_, t)| *t == "rm -rf out")
        .map(|(i, _)| *i)
    else {
        return Err("no `rm -rf out` step".to_string());
    };
    let is_build = |t: &str| t.starts_with("just ") || t.split_whitespace().any(|w| w == "cargo");
    if let Some((first_build, _)) = code.iter().find(|(_, t)| is_build(t))
        && *first_build < rm_line
    {
        return Err("`rm -rf out` must come before the first build call".to_string());
    }

    // Every cargo call is locked; the deploy tool installs from the lock without scripts.
    let mut npm_ci = 0;
    for (_, t) in &code {
        if t.split_whitespace().any(|w| w == "cargo") && !t.contains("--locked") {
            return Err(format!("a cargo call without `--locked`: `{t}`"));
        }
        if t.contains("npm install") || t.contains("npm i ") {
            return Err(format!("npm must install with `npm ci`: `{t}`"));
        }
        if t.starts_with("npm ci") {
            npm_ci += 1;
            if !t.contains("--ignore-scripts") {
                return Err(format!("`npm ci` without `--ignore-scripts`: `{t}`"));
            }
        }
        for flag in FORBIDDEN_FLAGS {
            if t.contains(flag) {
                return Err(format!("a step passes the flag `{flag}`: `{t}`"));
            }
        }
    }
    if npm_ci == 0 {
        return Err("no `npm ci --ignore-scripts` step".to_string());
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
    let mut plant3 = real_files.clone();
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

    // Plant 4: the host as a secret in another workflow
    let mut plant4 = real_files.clone();
    let ci4 = plant4
        .iter_mut()
        .find(|(p, _)| p == ".github/workflows/ci.yml")
        .expect("ci.yml");
    ci4.1
        .push_str("\n      FIELDBOOK_HOST: ${{ secrets.FIELDBOOK_HOST }}\n");
    assert!(
        check_secret_use(&plant4)
            .unwrap_err()
            .contains(".github/workflows/ci.yml")
    );

    // Plant 5: the host as a variable, in another workflow and in hosted.yml
    for file in [".github/workflows/ci.yml", ".github/workflows/hosted.yml"] {
        let mut plant5 = real_files.clone();
        let target = plant5
            .iter_mut()
            .find(|(p, _)| p == file)
            .expect("workflow file");
        target
            .1
            .push_str("\n      FIELDBOOK_HOST: ${{ vars.FIELDBOOK_HOST }}\n");
        let err = check_secret_use(&plant5).unwrap_err();
        assert!(err.contains(file), "expected {file} in {err}");
    }
}

// control: HO-05
#[test]
fn the_deploy_job_runs_on_the_self_hosted_runner_and_declares_the_environment() {
    let yml = std::fs::read_to_string(repo_root().join(".github/workflows/hosted.yml"))
        .unwrap_or_else(|e| panic!("read hosted.yml: {e}"));
    assert!(check_runner(&yml).is_ok());

    let env_count = yml
        .lines()
        .filter(|l| l.trim() == "environment: hosted")
        .count();
    assert_eq!(
        env_count, 1,
        "hosted.yml must contain exactly one line `environment: hosted`"
    );
    assert!(
        yml.lines()
            .any(|l| l.trim() == "if: github.ref == 'refs/heads/main'"),
        "missing `if: github.ref == 'refs/heads/main'`"
    );
    assert!(
        yml.contains("persist-credentials: false"),
        "missing `persist-credentials: false`"
    );
    assert!(yml.contains("clean: true"), "missing `clean: true`");
    assert!(
        yml.contains("secrets.FIELDBOOK_HOST"),
        "missing `secrets.FIELDBOOK_HOST`"
    );
    assert!(
        !yml.contains("vars."),
        "hosted.yml must not contain `vars.`"
    );
}

// control: HO-05
#[test]
fn a_hosted_runner_label_or_a_partial_label_set_is_refused() {
    let hosted_yml = std::fs::read_to_string(repo_root().join(".github/workflows/hosted.yml"))
        .unwrap_or_else(|e| panic!("read hosted.yml: {e}"));

    let plant = |replacement: &str| {
        hosted_yml
            .lines()
            .map(|l| {
                if l.trim().starts_with("runs-on:") {
                    replacement
                } else {
                    l
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert!(check_runner(&plant("    runs-on: ubuntu-24.04")).is_err());
    assert!(check_runner(&plant("    runs-on: [self-hosted]")).is_err());
    assert!(check_runner(&plant("    runs-on: [self-hosted, Linux, X64]")).is_err());

    let hosted_vals = runs_on_values(&hosted_yml);
    assert_eq!(
        hosted_vals.len(),
        1,
        "expected exactly one runs-on: line in hosted.yml"
    );
    let target = &hosted_vals[0];

    let ci_yml = std::fs::read_to_string(repo_root().join(".github/workflows/ci.yml"))
        .unwrap_or_else(|e| panic!("read ci.yml: {e}"));
    let ci_vals = runs_on_values(&ci_yml);
    assert!(
        !ci_vals.is_empty(),
        "ci.yml must contain at least one runs-on: line"
    );
    for val in &ci_vals {
        assert_eq!(
            val, target,
            "ci.yml runs-on `{val}` differs from hosted.yml `{target}`"
        );
    }
}

// control: HO-05
#[test]
fn the_deploy_job_is_hardened_on_the_persistent_runner() {
    let yml = std::fs::read_to_string(repo_root().join(".github/workflows/hosted.yml"))
        .unwrap_or_else(|e| panic!("read hosted.yml: {e}"));
    let res = check_job_hardening(&yml);
    assert!(res.is_ok(), "check_job_hardening failed: {res:?}");
}

// control: HO-05
#[test]
fn a_planted_job_hardening_violation_is_refused() {
    let real_yml = std::fs::read_to_string(repo_root().join(".github/workflows/hosted.yml"))
        .unwrap_or_else(|e| panic!("read hosted.yml: {e}"));
    let planted = |from: &str, to: &str| {
        assert!(
            real_yml.contains(from),
            "the workflow no longer holds `{from}`"
        );
        real_yml.replace(from, to)
    };
    let refused = |text: String, what: &str| {
        let err = check_job_hardening(&text).expect_err(what);
        assert!(!err.is_empty(), "{what}: the error must say what failed");
    };

    refused(
        planted("persist-credentials: false", "persist-credentials: true"),
        "stored credentials",
    );
    refused(
        planted("clean: true", "clean: false"),
        "an unclean checkout",
    );
    refused(
        planted("cargo --locked xtask", "cargo xtask"),
        "a cargo call without --locked",
    );
    refused(
        planted("npm ci --ignore-scripts", "npm ci"),
        "npm ci without --ignore-scripts",
    );
    refused(
        planted("npm ci --ignore-scripts", "npm install"),
        "npm install in place of npm ci",
    );
    refused(
        real_yml
            .lines()
            .filter(|l| l.trim() != "rm -rf out")
            .collect::<Vec<_>>()
            .join("\n"),
        "no fresh out/ step",
    );
    for flag in FORBIDDEN_FLAGS {
        refused(
            planted("just site", &format!("just site {flag} /somewhere")),
            flag,
        );
    }
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

// control: HO-07
#[test]
fn the_hosted_build_is_checked_to_be_unreleased_before_the_upload() {
    let yml = std::fs::read_to_string(repo_root().join(".github/workflows/hosted.yml"))
        .unwrap_or_else(|e| panic!("read hosted.yml: {e}"));
    assert!(check_unreleased_gate(&yml).is_ok());
}

// control: HO-07
#[test]
fn a_planted_release_stamp_or_a_removed_check_is_refused() {
    let real_yml = std::fs::read_to_string(repo_root().join(".github/workflows/hosted.yml"))
        .unwrap_or_else(|e| panic!("read hosted.yml: {e}"));

    // Plant (i): a line `      FIELDBOOK_GIT_SHA: ${{ github.sha }}` after the line `      CI: "true"` -> Err
    let mut plant1 = Vec::new();
    for line in real_yml.lines() {
        plant1.push(line);
        if line.contains("CI: \"true\"") {
            plant1.push("      FIELDBOOK_GIT_SHA: ${{ github.sha }}");
        }
    }
    let plant1_text = plant1.join("\n");
    assert!(
        check_unreleased_gate(&plant1_text).is_err(),
        "planted FIELDBOOK_GIT_SHA in env must fail"
    );

    // Plant (ii): the grep line removed (filter out lines containing `unreleased build`) -> Err
    let plant2_text = real_yml
        .lines()
        .filter(|l| !l.contains(fieldbook_core::strings::unreleased_build()))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        check_unreleased_gate(&plant2_text).is_err(),
        "removing grep line for unreleased build must fail"
    );

    // Plant (iii): the lines containing `FIELDBOOK_GIT_SHA+x` removed -> Err
    let plant3_text = real_yml
        .lines()
        .filter(|l| !l.contains("FIELDBOOK_GIT_SHA+x"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        check_unreleased_gate(&plant3_text).is_err(),
        "removing FIELDBOOK_GIT_SHA+x check must fail"
    );

    // Plant (v): the grep moved after the deploy step -> Err
    let mut plant5 = real_yml
        .lines()
        .filter(|l| !l.contains(fieldbook_core::strings::unreleased_build()))
        .collect::<Vec<_>>();
    plant5.push("          ! grep -q -i -F 'unreleased build' out/hosted/index.html");
    assert!(
        check_unreleased_gate(&plant5.join("\n")).is_err(),
        "a grep after the deploy step must fail"
    );

    // Plant (vi): the grep not negated, so a missing text never fails the step -> Err
    let plant6 = real_yml.replace("! grep -q -i -F", "grep -q -i -F");
    assert_ne!(plant6, real_yml, "the real grep line must be negated");
    assert!(
        check_unreleased_gate(&plant6).is_err(),
        "a grep that is not negated must fail"
    );

    // Plant (iv): a line `          export FIELDBOOK_GIT_SHA=abc` appended inside the Deploy run script (append at the end of the text with proper indentation is fine) -> Err
    let mut plant4_text = real_yml;
    plant4_text.push_str("\n          export FIELDBOOK_GIT_SHA=abc\n");
    assert!(
        check_unreleased_gate(&plant4_text).is_err(),
        "exporting FIELDBOOK_GIT_SHA must fail"
    );
}
