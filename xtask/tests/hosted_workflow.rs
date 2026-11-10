//! Hosted deploy workflow integration tests (`hosted-deploy`).

#![expect(
    clippy::disallowed_methods,
    reason = "Hosted workflow integration tests read workflow and deploy files"
)]

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn workflow_files_in(dir: &Path, prefix: &str) -> Vec<(String, String)> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
        .filter_map(|e| {
            let entry = e.unwrap_or_else(|err| panic!("entry in {}: {err}", dir.display()));
            let p = entry.path();
            let is_workflow =
                p.is_file() && p.extension().is_some_and(|s| s == "yml" || s == "yaml");
            is_workflow.then(|| {
                let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
                    panic!("file name utf8")
                };
                let text = std::fs::read_to_string(&p)
                    .unwrap_or_else(|err| panic!("read {}: {err}", p.display()));
                let rel = format!("{prefix}{name}");
                (rel, text)
            })
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

fn workflow_files() -> Vec<(String, String)> {
    workflow_files_in(&repo_root().join(".github/workflows"), ".github/workflows/")
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

fn check_uses_pinned(action_ref: &str, line_num: usize) -> Result<(), String> {
    let Some((_action, pin_and_comment)) = action_ref.split_once('@') else {
        return Err(format!(
            "line {line_num}: uses line has no '@': {action_ref}"
        ));
    };
    let Some((sha, comment)) = pin_and_comment.split_once(" # v") else {
        return Err(format!("line {line_num}: missing ' # v': {action_ref}"));
    };
    if sha.len() != 40 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("line {line_num}: invalid SHA: {sha}"));
    }
    if comment.is_empty() {
        return Err(format!("line {line_num}: empty version comment"));
    }
    Ok(())
}

fn check_all_uses_pinned(yml: &str) -> Result<(), String> {
    let mut uses = 0;
    for (i, line) in yml.lines().enumerate() {
        let t = line.trim();
        if let Some(r) = t
            .strip_prefix("- uses:")
            .or_else(|| t.strip_prefix("uses:"))
        {
            uses += 1;
            check_uses_pinned(r.trim(), i + 1)?;
        }
    }
    if uses < 4 {
        return Err(format!("expected at least 4 uses lines, found {uses}"));
    }
    Ok(())
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
                if trimmed.starts_with("environment:") {
                    return Err(format!("{path}: names an environment: `{line}`"));
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
    let unreleased_script = "bash deploy/check-unreleased.sh";
    let unreleased_count = yml
        .lines()
        .filter(|l| l.trim() == unreleased_script)
        .count();
    if unreleased_count != 1 {
        return Err(format!(
            "expected `{unreleased_script}` exactly once, found {unreleased_count}"
        ));
    }

    let no_identifiers_script = "bash deploy/check-no-identifiers.sh";
    let no_identifiers_count = yml
        .lines()
        .filter(|l| l.trim() == no_identifiers_script)
        .count();
    if no_identifiers_count != 1 {
        return Err(format!(
            "expected `{no_identifiers_script}` exactly once, found {no_identifiers_count}"
        ));
    }

    let Some(deploy_offset) = yml.find("wrangler deploy") else {
        return Err("missing `wrangler deploy` in workflow".to_string());
    };

    let Some(unreleased_offset) = yml.find(unreleased_script) else {
        return Err(format!("missing `{unreleased_script}` in workflow"));
    };
    if unreleased_offset >= deploy_offset {
        return Err(format!(
            "`{unreleased_script}` must appear before `wrangler deploy`"
        ));
    }

    let Some(no_identifiers_offset) = yml.find(no_identifiers_script) else {
        return Err(format!("missing `{no_identifiers_script}` in workflow"));
    };
    if no_identifiers_offset >= deploy_offset {
        return Err(format!(
            "`{no_identifiers_script}` must appear before `wrangler deploy`"
        ));
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

fn check_triggers(yml: &str) -> Result<(), String> {
    let on_count = yml.lines().filter(|l| *l == "on:").count();
    if on_count == 0 {
        return Err("missing `on:` line at column 0".to_string());
    }
    if on_count != 1 {
        return Err(format!(
            "expected exactly one `on:` line at column 0, found {on_count}"
        ));
    }

    let mut on_lines: Vec<&str> = Vec::new();
    let mut after_on = false;
    for line in yml.lines() {
        if !after_on {
            if line == "on:" {
                after_on = true;
            }
        } else {
            let trimmed = line.trim();
            let is_blank_or_comment = trimmed.is_empty() || trimmed.starts_with('#');
            let is_col0 = !line.starts_with(' ') && !line.starts_with('\t') && !line.is_empty();
            if is_col0 && !is_blank_or_comment {
                break;
            }
            on_lines.push(line);
        }
    }

    let non_comment_lines: Vec<&str> = on_lines
        .into_iter()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with('#')
        })
        .collect();

    let two_space_keys: Vec<&str> = non_comment_lines
        .iter()
        .filter(|l| l.starts_with("  ") && l.chars().nth(2).is_some_and(|c| !c.is_whitespace()))
        .map(|l| l.trim())
        .collect();

    if two_space_keys != ["push:", "workflow_dispatch:"] {
        return Err(format!(
            "trigger keys at two spaces indent must be exactly `push:` and `workflow_dispatch:`, found {two_space_keys:?}"
        ));
    }

    let Some(push_idx) = non_comment_lines.iter().position(|l| l.trim() == "push:") else {
        return Err("missing `push:` trigger".to_string());
    };
    let Some(dispatch_idx) = non_comment_lines
        .iter()
        .position(|l| l.trim() == "workflow_dispatch:")
    else {
        return Err("missing `workflow_dispatch:` trigger".to_string());
    };

    if push_idx >= dispatch_idx {
        return Err("`push:` must appear before `workflow_dispatch:`".to_string());
    }

    let between: Vec<&str> = non_comment_lines[push_idx + 1..dispatch_idx]
        .iter()
        .map(|l| l.trim())
        .collect();

    if between != ["branches: [main]"] {
        return Err(format!(
            "lines strictly between `push:` and `workflow_dispatch:` must be exactly [\"branches: [main]\"], found {between:?}"
        ));
    }

    Ok(())
}

fn check_read_only_token(yml: &str) -> Result<(), String> {
    for line in yml.lines() {
        if line.contains(": write") || line.contains("write-all") {
            return Err(format!("workflow contains write permission: `{line}`"));
        }
    }

    let mut lines = yml.lines();
    let mut found_permissions = false;
    let mut block_lines: Vec<&str> = Vec::new();
    while let Some(line) = lines.next() {
        if line == "permissions:" {
            found_permissions = true;
            for next_line in lines.by_ref() {
                let trimmed = next_line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                if !next_line.starts_with(' ') {
                    break;
                }
                block_lines.push(trimmed);
            }
            break;
        }
    }

    if !found_permissions {
        return Err("missing `permissions:` line at column 0".to_string());
    }
    if block_lines.is_empty() {
        return Err("missing permissions block".to_string());
    }
    if block_lines != ["contents: read"] {
        return Err(format!(
            "permissions block must contain only `contents: read`, found: {block_lines:?}"
        ));
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

    for (_, t) in &code {
        if t.contains("FIELDBOOK_STATE") {
            return Err(format!("workflow references FIELDBOOK_STATE: `{t}`"));
        }
    }

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

    // Gating steps (HO-05)
    for (_, t) in &code {
        let stripped = t.strip_prefix("- ").unwrap_or(t).trim_start();
        if stripped.starts_with("continue-on-error") {
            return Err(format!("workflow contains continue-on-error: `{t}`"));
        }
        if stripped.starts_with("if:")
            && stripped != "if: github.ref == 'refs/heads/main'"
            && stripped != "if: inputs.rollback_to == ''"
            && stripped != "if: inputs.rollback_to != ''"
        {
            return Err(format!("unexpected `if:` line: `{t}`"));
        }
    }

    let mut deploy_count = 0;
    let mut rollback_count = 0;
    for (_, t) in &code {
        if t.contains("wrangler deploy") {
            deploy_count += 1;
            if !t.contains("--env-file=/dev/null") {
                return Err(format!(
                    "`wrangler deploy` line lacks `--env-file=/dev/null`: `{t}`"
                ));
            }
        }
        if t.contains("wrangler rollback") {
            rollback_count += 1;
            if !t.contains("--env-file=/dev/null") {
                return Err(format!(
                    "`wrangler rollback` line lacks `--env-file=/dev/null`: `{t}`"
                ));
            }
        }
    }
    if deploy_count == 0 {
        return Err("missing `wrangler deploy` in workflow".to_string());
    }
    if rollback_count == 0 {
        return Err("missing `wrangler rollback` in workflow".to_string());
    }

    Ok(())
}

fn check_metrics(yml: &str) -> Result<(), String> {
    let mut exact_count = 0;
    let mut contain_count = 0;
    for line in yml.lines() {
        if line.contains("WRANGLER_SEND_METRICS") {
            contain_count += 1;
            if line.trim() == "WRANGLER_SEND_METRICS: \"false\"" {
                exact_count += 1;
            }
        }
    }
    if exact_count != 1 || contain_count != 1 {
        return Err(format!(
            "expected exactly one `WRANGLER_SEND_METRICS: \"false\"` line and no other lines mentioning WRANGLER_SEND_METRICS, found {exact_count} exact matches out of {contain_count} total"
        ));
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

fn has_access_team_domain(content: &str) -> bool {
    let lower = content.to_ascii_lowercase();
    let target = "cloudflareaccess";
    let mut search_from = 0;
    while let Some(pos) = lower[search_from..].find(target) {
        let abs = search_from + pos;
        let prefix = &lower[..abs];
        let before_dot = prefix
            .strip_suffix(r"\\.")
            .or_else(|| prefix.strip_suffix(r"\."))
            .or_else(|| prefix.strip_suffix('.'));
        if before_dot
            .and_then(|b| b.chars().next_back())
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
        if has_access_team_domain(content) {
            return Err(format!("{path}: contains an Access team domain"));
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
fn workflow_files_reads_yml_and_yaml() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("a.yml"), "name: a\n").expect("write a.yml");
    std::fs::write(dir.path().join("b.yaml"), "name: b\n").expect("write b.yaml");
    std::fs::write(dir.path().join("c.txt"), "ignored\n").expect("write c.txt");

    let files = workflow_files_in(dir.path(), "");
    let names: Vec<_> = files.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(names, ["a.yml", "b.yaml"]);
}

// control: HO-09
#[test]
fn the_workflow_triggers_are_push_to_main_and_manual_dispatch() {
    let yml = std::fs::read_to_string(repo_root().join(".github/workflows/hosted.yml"))
        .unwrap_or_else(|e| panic!("read hosted.yml: {e}"))
        .replace("\r\n", "\n");
    let res = check_triggers(&yml);
    assert!(res.is_ok(), "check_triggers failed: {res:?}");
}

// control: HO-09
#[test]
fn a_planted_extra_trigger_is_refused() {
    let real_yml = std::fs::read_to_string(repo_root().join(".github/workflows/hosted.yml"))
        .unwrap_or_else(|e| panic!("read hosted.yml: {e}"))
        .replace("\r\n", "\n");

    let extra_triggers = [
        "  pull_request:",
        "  pull_request_target:",
        "  workflow_run:\n    workflows: [ci]\n    types: [completed]",
        "  repository_dispatch:",
    ];

    for extra in extra_triggers {
        let plant = real_yml.replace(
            "  workflow_dispatch:",
            &format!("{extra}\n  workflow_dispatch:"),
        );
        assert_ne!(
            plant, real_yml,
            "planted trigger `{extra}` must change text"
        );
        assert!(
            check_triggers(&plant).is_err(),
            "check_triggers must fail for planted trigger `{extra}`"
        );
    }

    let plant_branches = real_yml.replace("    branches: [main]", "    branches: [main, dev]");
    assert_ne!(plant_branches, real_yml, "plant_branches must change text");
    assert!(
        check_triggers(&plant_branches).is_err(),
        "check_triggers must fail when branches are changed"
    );

    let plant_no_on = real_yml
        .lines()
        .filter(|l| *l != "on:")
        .collect::<Vec<_>>()
        .join("\n");
    assert_ne!(plant_no_on, real_yml, "plant_no_on must change text");
    assert!(
        check_triggers(&plant_no_on).is_err(),
        "check_triggers must fail when on: line is removed"
    );
}

// control: HO-09
#[test]
fn workflow_is_pinned_read_only_and_serialised() {
    let yml = std::fs::read_to_string(repo_root().join(".github/workflows/hosted.yml"))
        .unwrap_or_else(|e| panic!("read hosted.yml: {e}"))
        .replace("\r\n", "\n");

    assert!(check_read_only_token(&yml).is_ok());
    assert_eq!(
        yml.lines().filter(|l| l.trim() == "permissions:").count(),
        1
    );
    assert!(yml.contains("group: deploy-hosted"));
    assert!(yml.contains("cancel-in-progress: false"));
    assert!(!yml.contains("set -x"));
    assert!(check_all_uses_pinned(&yml).is_ok());

    for (i, line) in yml.lines().enumerate() {
        let t = line.trim();
        if line.contains("secrets.") {
            assert!(
                is_valid_secret_env_line(t),
                "line {}: invalid secret: {line}",
                i + 1
            );
        }
    }
}

// control: HO-09
#[test]
fn a_planted_unpinned_action_is_refused() {
    let real_yml = std::fs::read_to_string(repo_root().join(".github/workflows/hosted.yml"))
        .unwrap_or_else(|e| panic!("read hosted.yml: {e}"))
        .replace("\r\n", "\n");

    // Plant 1: replace checkout reference with actions/checkout@v4
    let plant1 = real_yml.replace(
        "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1",
        "actions/checkout@v4",
    );
    assert_ne!(plant1, real_yml, "plant1 must change text");
    assert!(check_all_uses_pinned(&plant1).is_err());

    // Plant 2: shorten the SHA to 39 hex chars
    let plant2 = real_yml.replace(
        "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1",
        "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b # v7.0.1",
    );
    assert_ne!(plant2, real_yml, "plant2 must change text");
    assert!(check_all_uses_pinned(&plant2).is_err());
}

// control: HO-05
#[test]
fn a_planted_extra_token_scope_is_refused() {
    let real_yml = std::fs::read_to_string(repo_root().join(".github/workflows/hosted.yml"))
        .unwrap_or_else(|e| panic!("read hosted.yml: {e}"))
        .replace("\r\n", "\n");

    // Plant 1: id-token: write right after contents: read
    let plant1 = real_yml.replace(
        "  contents: read\n",
        "  contents: read\n  id-token: write\n",
    );
    assert_ne!(plant1, real_yml, "plant1 must change the text");
    assert!(check_read_only_token(&plant1).is_err());

    // Plant 2: contents: write replacing contents: read
    let plant2 = real_yml.replace("  contents: read", "  contents: write");
    assert_ne!(plant2, real_yml, "plant2 must change the text");
    assert!(check_read_only_token(&plant2).is_err());

    // Plant 3: permissions: write-all appended on its own line
    let mut plant3 = real_yml.clone();
    plant3.push_str("\npermissions: write-all\n");
    assert!(check_read_only_token(&plant3).is_err());

    // Plant 4: extra token scope separated by blank line
    let plant4 = real_yml.replace(
        "  contents: read\n",
        "  contents: read\n\n  actions: read\n",
    );
    assert_ne!(plant4, real_yml, "plant4 must change the text");
    assert!(check_read_only_token(&plant4).is_err());
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
    let err1 = check_secret_use(&plant1).expect_err("CLOUDFLARE_API_TOKEN");
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
            .expect_err("environment: hosted")
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
            .expect_err("FIELDBOOK_HOST")
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
        let err = check_secret_use(&plant5).expect_err("vars.FIELDBOOK_HOST");
        assert!(err.contains(file), "expected {file} in {err}");
    }

    // Plant (a): extra workflow declaring an environment
    let mut plant_a = real_files.clone();
    plant_a.push((
        ".github/workflows/extra.yaml".to_string(),
        "name: extra\njobs:\n  a:\n    environment: hosted\n".to_string(),
    ));
    let err_a = check_secret_use(&plant_a).expect_err("extra.yaml environment");
    assert!(
        err_a.contains(".github/workflows/extra.yaml"),
        "expected .github/workflows/extra.yaml in {err_a}"
    );

    // Plant (b): block-form environment in ci.yml
    let mut plant_b = real_files;
    let ci_b = plant_b
        .iter_mut()
        .find(|(p, _)| p == ".github/workflows/ci.yml")
        .expect("ci.yml");
    ci_b.1.push_str("\n    environment:\n      name: hosted\n");
    let err_b = check_secret_use(&plant_b).expect_err("ci.yml block environment");
    assert!(
        err_b.contains(".github/workflows/ci.yml"),
        "expected ci.yml in {err_b}"
    );
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
        .unwrap_or_else(|e| panic!("read hosted.yml: {e}"))
        .replace("\r\n", "\n");
    let planted = |from: &str, to: &str| {
        assert!(
            real_yml.contains(from),
            "the workflow no longer holds `{from}`"
        );
        let res = real_yml.replace(from, to);
        assert_ne!(res, real_yml, "replacement of `{from}` must change text");
        res
    };
    let insert_after = |needle: &str, new_line: &str| {
        let mut found = false;
        let mut out = Vec::new();
        for line in real_yml.lines() {
            out.push(line);
            if !found && line.contains(needle) {
                out.push(new_line);
                found = true;
            }
        }
        assert!(found, "needle `{needle}` not found in hosted.yml");
        let res = out.join("\n");
        assert_ne!(res, real_yml, "inserting after `{needle}` must change text");
        res
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
    refused(
        insert_after(
            "- name: Check the build is unreleased",
            "        continue-on-error: true",
        ),
        "continue-on-error",
    );
    refused(
        planted(
            "      - name: Deploy\n        if: inputs.rollback_to == ''",
            "      - name: Deploy\n        if: always()",
        ),
        "if: always()",
    );
    refused(
        insert_after(
            "- name: Check the login gate before the upload",
            "        if: false",
        ),
        "if: false",
    );
    refused(
        planted("wrangler deploy --env-file=/dev/null", "wrangler deploy"),
        "deploy without --env-file=/dev/null",
    );
    refused(
        planted("\"$ROLLBACK_TO\" --env-file=/dev/null", "\"$ROLLBACK_TO\""),
        "rollback without --env-file=/dev/null",
    );
    for (text, what) in [
        (
            insert_after("WRANGLER_SEND_METRICS", "      FIELDBOOK_STATE: /srv/state"),
            "FIELDBOOK_STATE in the job env",
        ),
        (
            planted(
                "          just site",
                "          export FIELDBOOK_STATE=/srv/state\n          just site",
            ),
            "FIELDBOOK_STATE exported before the build",
        ),
    ] {
        let err = check_job_hardening(&text).expect_err(what);
        assert!(
            err.contains("FIELDBOOK_STATE"),
            "{what}: the error must name FIELDBOOK_STATE: {err}"
        );
    }
}

// control: HO-09
#[test]
fn a_planted_metrics_change_is_refused() {
    let real_yml = std::fs::read_to_string(repo_root().join(".github/workflows/hosted.yml"))
        .unwrap_or_else(|e| panic!("read hosted.yml: {e}"))
        .replace("\r\n", "\n");

    assert!(check_metrics(&real_yml).is_ok());

    // Plant 1: delete the metrics line
    let plant1 = real_yml
        .lines()
        .filter(|l| !l.contains("WRANGLER_SEND_METRICS"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_ne!(plant1, real_yml, "plant1 must change text");
    assert!(check_metrics(&plant1).is_err());

    // Plant 2: replace WRANGLER_SEND_METRICS: "false" with WRANGLER_SEND_METRICS: "true"
    let plant2 = real_yml.replace(
        "WRANGLER_SEND_METRICS: \"false\"",
        "WRANGLER_SEND_METRICS: \"true\"",
    );
    assert_ne!(plant2, real_yml, "plant2 must change text");
    assert!(check_metrics(&plant2).is_err());

    // Plant 3: duplicate the line
    let mut plant3_lines = Vec::new();
    for line in real_yml.lines() {
        plant3_lines.push(line);
        if line.contains("WRANGLER_SEND_METRICS: \"false\"") {
            plant3_lines.push(line);
        }
    }
    let plant3 = plant3_lines.join("\n");
    assert_ne!(plant3, real_yml, "plant3 must change text");
    assert!(check_metrics(&plant3).is_err());
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
        ("deploy/extra6.txt", r"team\\.cloudflareaccess\\.com"),
    ];
    for (path, content) in plants {
        let mut files = real_files.clone();
        files.push((path.to_string(), content.to_string()));
        let err = check_identifiers(&files).expect_err(path);
        assert!(err.contains(path), "error must name {path}: {err}");
    }

    // Plant (a): deploy/check-login-gate.sh with Access team domain appended
    let mut plant_gate = real_files.clone();
    let gate_tuple = plant_gate
        .iter_mut()
        .find(|(p, _)| p == "deploy/check-login-gate.sh");
    assert!(gate_tuple.is_some(), "deploy/check-login-gate.sh found");
    let gate_tuple = gate_tuple.expect("deploy/check-login-gate.sh");
    gate_tuple
        .1
        .push_str("\n# https://team.cloudflareaccess.com\n");
    let err_gate = check_identifiers(&plant_gate).expect_err("planted gate must fail");
    assert!(
        err_gate.contains("deploy/check-login-gate.sh"),
        "error must name deploy/check-login-gate.sh: {err_gate}"
    );

    // Plant (b): deploy/extra5.txt with team\\.cloudflareaccess\\.com
    let mut plant_extra5 = real_files.clone();
    plant_extra5.push((
        "deploy/extra5.txt".to_string(),
        r"team\.cloudflareaccess\.com".to_string(),
    ));
    let err_extra5 = check_identifiers(&plant_extra5).expect_err("extra5 must fail");
    assert!(
        err_extra5.contains("deploy/extra5.txt"),
        "error must name deploy/extra5.txt: {err_extra5}"
    );

    // Assert that check_identifiers on unmodified deploy/check-login-gate.sh is Ok
    let real_gate = real_files
        .iter()
        .find(|(p, _)| p == "deploy/check-login-gate.sh")
        .expect("deploy/check-login-gate.sh");
    assert!(
        check_identifiers(std::slice::from_ref(real_gate)).is_ok(),
        "unmodified deploy/check-login-gate.sh must be Ok"
    );

    // Unit assertions for has_access_team_domain
    assert!(has_access_team_domain("team.cloudflareaccess.com"));
    assert!(has_access_team_domain(r"team\.cloudflareaccess\.com"));
    assert!(has_access_team_domain(r"team\\.cloudflareaccess\\.com"));
    assert!(!has_access_team_domain(
        r"([A-Za-z0-9-]+\.)*cloudflareaccess\.com"
    ));
    assert!(!has_access_team_domain(
        r"([A-Za-z0-9-]+\\.)*cloudflareaccess\.com"
    ));
    assert!(!has_access_team_domain("cloudflareaccess.com"));
    assert!(!has_access_team_domain("cloudflareaccess"));
    assert!(!has_access_team_domain("\ncloudflareaccess.com"));
    assert!(has_access_team_domain("TEAM.CLOUDFLAREACCESS.COM"));

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
        .unwrap_or_else(|e| panic!("read hosted.yml: {e}"))
        .replace("\r\n", "\n");

    // Plant 1: FIELDBOOK_GIT_SHA key in env after CI: "true"
    let mut plant1 = Vec::new();
    for line in real_yml.lines() {
        plant1.push(line);
        if line.contains("CI: \"true\"") {
            plant1.push("      FIELDBOOK_GIT_SHA: ${{ github.sha }}");
        }
    }
    let plant1_text = plant1.join("\n");
    assert_ne!(plant1_text, real_yml, "plant1 must change text");
    assert!(
        check_unreleased_gate(&plant1_text).is_err(),
        "planted FIELDBOOK_GIT_SHA in env must fail"
    );

    // Plant 2: export FIELDBOOK_GIT_SHA=abc appended
    let mut plant2 = real_yml.clone();
    plant2.push_str("\nexport FIELDBOOK_GIT_SHA=abc\n");
    assert!(
        check_unreleased_gate(&plant2).is_err(),
        "exporting FIELDBOOK_GIT_SHA must fail"
    );

    // Plant 3: bare FIELDBOOK_GIT_SHA=abc appended
    let mut plant3 = real_yml.clone();
    plant3.push_str("\nFIELDBOOK_GIT_SHA=abc\n");
    assert!(
        check_unreleased_gate(&plant3).is_err(),
        "bare FIELDBOOK_GIT_SHA assignment must fail"
    );

    // Plant 4: bash deploy/check-unreleased.sh removed
    let plant4 = real_yml
        .lines()
        .filter(|l| l.trim() != "bash deploy/check-unreleased.sh")
        .collect::<Vec<_>>()
        .join("\n");
    assert_ne!(plant4, real_yml, "check-unreleased.sh must exist");
    assert!(
        check_unreleased_gate(&plant4).is_err(),
        "removing check-unreleased.sh must fail"
    );

    // Plant 5: bash deploy/check-no-identifiers.sh removed
    let plant5 = real_yml
        .lines()
        .filter(|l| l.trim() != "bash deploy/check-no-identifiers.sh")
        .collect::<Vec<_>>()
        .join("\n");
    assert_ne!(plant5, real_yml, "check-no-identifiers.sh must exist");
    assert!(
        check_unreleased_gate(&plant5).is_err(),
        "removing check-no-identifiers.sh must fail"
    );

    // Plant 6: bash deploy/check-unreleased.sh moved after deploy
    let mut plant6_lines: Vec<&str> = real_yml
        .lines()
        .filter(|l| l.trim() != "bash deploy/check-unreleased.sh")
        .collect();
    assert_ne!(
        plant6_lines.len(),
        real_yml.lines().count(),
        "check-unreleased.sh must be removed"
    );
    plant6_lines.push("          bash deploy/check-unreleased.sh");
    let plant6 = plant6_lines.join("\n");
    assert!(
        check_unreleased_gate(&plant6).is_err(),
        "moving check-unreleased.sh after deploy must fail"
    );

    // Plant 7: bash deploy/check-no-identifiers.sh moved after deploy
    let mut plant7_lines: Vec<&str> = real_yml
        .lines()
        .filter(|l| l.trim() != "bash deploy/check-no-identifiers.sh")
        .collect();
    assert_ne!(
        plant7_lines.len(),
        real_yml.lines().count(),
        "check-no-identifiers.sh must be removed"
    );
    plant7_lines.push("          bash deploy/check-no-identifiers.sh");
    let plant7 = plant7_lines.join("\n");
    assert!(
        check_unreleased_gate(&plant7).is_err(),
        "moving check-no-identifiers.sh after deploy must fail"
    );
}
