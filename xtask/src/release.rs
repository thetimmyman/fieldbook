//! Release preparation and release tag validation.
//!
//! Subcommands:
//! - `cargo xtask release X.Y.Z`: prepares the release commit on a branch without committing.
//! - `cargo xtask release-check TAG --base FILE [--evidence-out DIR] [--output FILE]`: verifies
//!   a tagged release in CI against baseline compatibility and git history.

use std::fs;
use std::path::{Path, PathBuf};

use fieldbook_core::ids::{EditionId, ToolRelease};

use crate::compat::parse_baselines;
use crate::edition_release::{
    DeliveryKind, ReleaseIdentity, collect_release_evidence, validate_edition_tag,
};
use crate::proc;

const USAGE_RELEASE: &str = "usage: cargo xtask release X.Y.Z";
const USAGE_RELEASE_CHECK: &str =
    "usage: cargo xtask release-check TAG --base FILE [--evidence-out DIR] [--output FILE]";

/// Parsed three-component semantic version number (`X.Y.Z`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    /// Major component.
    pub major: u64,
    /// Minor component.
    pub minor: u64,
    /// Patch component.
    pub patch: u64,
}

/// Parse a strict `X.Y.Z` version string (no leading zeros except 0, no prefix, no suffix).
pub fn parse_version(s: &str) -> Result<Version, String> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 3 {
        return Err(format!("expected three dot-separated numbers, got `{s}`"));
    }
    let parse_component = |part: &str| -> Result<u64, String> {
        if part.is_empty() {
            return Err("empty component".to_owned());
        }
        if !part.chars().all(|c| c.is_ascii_digit()) {
            return Err(format!("non-digit in `{part}`"));
        }
        if part.len() > 1 && part.starts_with('0') {
            return Err(format!("leading zero in `{part}`"));
        }
        part.parse::<u64>().map_err(|e| e.to_string())
    };
    let major = parse_component(parts[0])?;
    let minor = parse_component(parts[1])?;
    let patch = parse_component(parts[2])?;
    Ok(Version {
        major,
        minor,
        patch,
    })
}

/// Read the `[workspace.package]` version from `Cargo.toml`.
pub fn get_workspace_version(cargo_toml: &str) -> Result<String, String> {
    let mut in_workspace_package = false;
    for line in cargo_toml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_workspace_package = trimmed == "[workspace.package]";
        }
        if !in_workspace_package {
            continue;
        }
        let Some(prefix) = trimmed.strip_prefix("version") else {
            continue;
        };
        let after = prefix.trim_start();
        let Some(rest) = after.strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim();
        if let Some((ver, _)) = rest.strip_prefix('"').and_then(|s| s.split_once('"')) {
            return Ok(ver.to_owned());
        }
    }
    Err("cannot find `[workspace.package]` version in Cargo.toml".to_owned())
}

/// Set the `version = "..."` line inside `[workspace.package]` in `Cargo.toml`.
pub fn set_workspace_version(cargo_toml: &str, new: &str) -> Result<String, String> {
    let mut in_workspace_package = false;
    let mut replaced = false;
    let mut out = String::with_capacity(cargo_toml.len());

    for line in cargo_toml.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_workspace_package = trimmed == "[workspace.package]";
        }
        if !in_workspace_package || replaced {
            out.push_str(line);
            continue;
        }
        let Some(prefix) = trimmed.strip_prefix("version") else {
            out.push_str(line);
            continue;
        };
        let after = prefix.trim_start();
        let Some(rest) = after.strip_prefix('=') else {
            out.push_str(line);
            continue;
        };
        let rest = rest.trim();
        if rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2 {
            let indent = &line[..line.len() - line.trim_start().len()];
            let line_ending = if line.ends_with("\r\n") {
                "\r\n"
            } else if line.ends_with('\n') {
                "\n"
            } else {
                ""
            };
            out.push_str(indent);
            out.push_str(&format!("version = \"{new}\"{line_ending}"));
            replaced = true;
            continue;
        }
        out.push_str(line);
    }

    if !replaced {
        return Err("cannot find `version` inside `[workspace.package]` in Cargo.toml".to_owned());
    }
    Ok(out)
}

/// Set `version = "<new>"` for every `[[package]]` in `Cargo.lock` that lacks a `source = ` line
/// and whose version matches `old`.
pub fn set_lock_versions(lock: &str, old: &str, new: &str) -> String {
    let mut out = String::with_capacity(lock.len());
    let mut parts = lock.split("[[package]]");
    if let Some(prefix) = parts.next() {
        out.push_str(prefix);
    }
    for block in parts {
        out.push_str("[[package]]");
        let has_source = block.lines().any(|l| {
            let t = l.trim_start();
            t.starts_with("source ") || t.starts_with("source=")
        });
        if has_source {
            out.push_str(block);
        } else {
            let mut block_out = String::with_capacity(block.len());
            for line in block.split_inclusive('\n') {
                let trimmed = line.trim();
                if let Some(prefix) = trimmed.strip_prefix("version") {
                    let after = prefix.trim_start();
                    if let Some(rest) = after.strip_prefix('=') {
                        let val = rest.trim();
                        let target = format!("\"{old}\"");
                        if val == target {
                            let indent = &line[..line.len() - line.trim_start().len()];
                            let line_ending = if line.ends_with("\r\n") {
                                "\r\n"
                            } else if line.ends_with('\n') {
                                "\n"
                            } else {
                                ""
                            };
                            block_out.push_str(indent);
                            block_out.push_str(&format!("version = \"{new}\"{line_ending}"));
                            continue;
                        }
                    }
                }
                block_out.push_str(line);
            }
            out.push_str(&block_out);
        }
    }
    out
}

/// Roll `## Unreleased` into `## Unreleased\n\n## X.Y.Z` in `CHANGELOG.md`.
pub fn roll_changelog(changelog: &str, new: &str) -> Result<String, String> {
    let mut out = String::with_capacity(changelog.len() + new.len() + 16);
    let mut rolled = false;

    for line in changelog.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if !rolled && trimmed == "## Unreleased" {
            let line_ending = if line.ends_with("\r\n") {
                "\r\n"
            } else if line.ends_with('\n') {
                "\n"
            } else {
                ""
            };
            out.push_str(&format!(
                "## Unreleased{line_ending}{line_ending}## {new}{line_ending}"
            ));
            rolled = true;
            continue;
        }
        out.push_str(line);
    }

    if !rolled {
        return Err("missing `## Unreleased` heading in CHANGELOG.md".to_owned());
    }
    Ok(out)
}

/// Check that `CHANGELOG.md` contains an `## Unreleased` heading with non-blank content under it.
pub fn check_changelog_unreleased(changelog: &str) -> Result<bool, &'static str> {
    let mut in_unreleased = false;
    let mut has_non_blank = false;
    for line in changelog.lines() {
        let trimmed = line.trim();
        if trimmed == "## Unreleased" {
            in_unreleased = true;
            continue;
        }
        if in_unreleased {
            if trimmed.starts_with("## ") {
                break;
            }
            if !trimmed.is_empty() {
                has_non_blank = true;
                break;
            }
        }
    }
    if !in_unreleased {
        return Err("missing ## Unreleased in CHANGELOG.md");
    }
    if !has_non_blank {
        return Ok(false);
    }
    Ok(true)
}

/// Parse paths from `git status --porcelain` v1 output.
pub fn parse_porcelain_paths(porcelain: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for line in porcelain.lines() {
        if line.len() < 4 {
            continue;
        }
        let rest = line[3..].trim();
        if rest.is_empty() {
            continue;
        }
        let raw_path = if let Some((_, new_path)) = rest.split_once(" -> ") {
            new_path.trim()
        } else {
            rest
        };
        let unquoted = raw_path
            .strip_prefix('"')
            .and_then(|p| p.strip_suffix('"'))
            .unwrap_or(raw_path);
        paths.push(unquoted.to_owned());
    }
    paths
}

/// Result of a command run through the runner seam.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunResult {
    /// True when the command exited with status 0.
    pub success: bool,
    /// Standard output captured from the command.
    pub stdout: String,
}

fn release_refuse(detail: &str) -> Result<bool, String> {
    eprintln!("release: refused: {detail}");
    Ok(false)
}

/// Runner function seam used by [`release_with`].
pub type RunnerFn<'a> = dyn FnMut(&str, &[&str], &Path) -> Result<RunResult, String> + 'a;

/// Execute release preparation at `root` using the supplied `run` command seam.
pub fn release_with(root: &Path, version: &str, run: &mut RunnerFn<'_>) -> Result<bool, String> {
    // 1. Version argument must be strictly X.Y.Z
    let Ok(new_ver) = parse_version(version) else {
        return release_refuse(&format!("version syntax: {USAGE_RELEASE}"));
    };

    // 2. Branch check
    let branch_out = run("git", &["rev-parse", "--abbrev-ref", "HEAD"], root)?;
    if !branch_out.success {
        return Err("cannot determine current git branch".to_owned());
    }
    let branch = branch_out.stdout.trim();
    if branch == "main" {
        return release_refuse("on main: switch to a card branch");
    }
    if branch == "HEAD" {
        return release_refuse("on HEAD (detached head): switch to a card branch");
    }

    // 3. Tree clean check
    let dirty_out = run(
        "git",
        &["status", "--porcelain", "--untracked-files=all"],
        root,
    )?;
    if !dirty_out.success {
        return Err("cannot determine git working tree status".to_owned());
    }
    if !dirty_out.stdout.trim().is_empty() {
        return release_refuse("the working tree is not clean");
    }

    // 4. Version must be strictly higher than workspace version
    let cargo_path = root.join("Cargo.toml");
    let cargo_content = fs::read_to_string(&cargo_path)
        .map_err(|e| format!("cannot read `{}`: {e}", cargo_path.display()))?;
    let ws_ver_str = get_workspace_version(&cargo_content)?;
    let ws_ver = parse_version(&ws_ver_str)
        .map_err(|e| format!("invalid workspace version `{ws_ver_str}` in Cargo.toml: {e}"))?;
    if new_ver <= ws_ver {
        return release_refuse(&format!(
            "version {version} is not higher than the workspace version {ws_ver_str}"
        ));
    }

    // 5. CHANGELOG.md ## Unreleased check
    let changelog_path = root.join("CHANGELOG.md");
    let changelog_content = fs::read_to_string(&changelog_path)
        .map_err(|e| format!("cannot read `{}`: {e}", changelog_path.display()))?;
    match check_changelog_unreleased(&changelog_content) {
        Ok(true) => {}
        Ok(false) => {
            return release_refuse("nothing under ## Unreleased in CHANGELOG.md");
        }
        Err(e) => {
            return release_refuse(e);
        }
    }

    // a. precheck: `just check`
    let precheck = run("just", &["check"], root)?;
    if !precheck.success {
        return Err("`just check` failed before the edit".to_owned());
    }

    // b. edit files
    let lock_path = root.join("Cargo.lock");
    let lock_content = fs::read_to_string(&lock_path)
        .map_err(|e| format!("cannot read `{}`: {e}", lock_path.display()))?;

    let new_cargo = set_workspace_version(&cargo_content, version)?;
    let new_lock = set_lock_versions(&lock_content, &ws_ver_str, version);
    let new_changelog = roll_changelog(&changelog_content, version)?;

    if new_cargo != cargo_content {
        fs::write(&cargo_path, &new_cargo)
            .map_err(|e| format!("cannot write `{}`: {e}", cargo_path.display()))?;
    }
    if new_lock != lock_content {
        fs::write(&lock_path, &new_lock)
            .map_err(|e| format!("cannot write `{}`: {e}", lock_path.display()))?;
    }
    if new_changelog != changelog_content {
        fs::write(&changelog_path, &new_changelog)
            .map_err(|e| format!("cannot write `{}`: {e}", changelog_path.display()))?;
    }

    // c. lock: `just lock`
    let lock_run = run("just", &["lock"], root)?;
    if !lock_run.success {
        return Err("`just lock` failed".to_owned());
    }

    // d. dependency checks
    let deny_default = run(
        "cargo",
        &["deny", "--locked", "check", "bans", "licenses", "sources"],
        root,
    )?;
    if !deny_default.success {
        return Err("`cargo deny` check failed".to_owned());
    }
    let deny_all = run(
        "cargo",
        &[
            "deny",
            "--locked",
            "--all-features",
            "check",
            "bans",
            "licenses",
            "sources",
        ],
        root,
    )?;
    if !deny_all.success {
        return Err("`cargo deny --all-features` check failed".to_owned());
    }

    // e. postcheck: `just check`
    let postcheck = run("just", &["check"], root)?;
    if !postcheck.success {
        return Err("`just check` failed after the edit".to_owned());
    }

    // f. allowed-change assertion
    let final_status = run(
        "git",
        &["status", "--porcelain", "--untracked-files=all"],
        root,
    )?;
    if !final_status.success {
        return Err("`git status` failed after release preparation".to_owned());
    }
    let changed_paths = parse_porcelain_paths(&final_status.stdout);
    for path in changed_paths {
        if path != "Cargo.toml" && path != "Cargo.lock" && path != "CHANGELOG.md" {
            return Err(format!(
                "unrelated change after release preparation: {path}"
            ));
        }
    }

    // g. on success print
    println!(
        "release: prepared {version} from {ws_ver_str}; review the full Cargo.lock diff and the dependency evidence (docs/reference/dependencies.md) before staging; nothing was committed"
    );
    Ok(true)
}

/// Command handler for `cargo xtask release <X.Y.Z>`.
pub fn command_release(root: &Path, args: &[&str]) -> Result<bool, String> {
    if args.len() != 1 {
        return release_refuse(&format!("version syntax: {USAGE_RELEASE}"));
    }
    let version = args[0];
    let mut runner = |program: &str, p_args: &[&str], dir: &Path| -> Result<RunResult, String> {
        if program == "git" {
            let out = proc::output(program, p_args, dir)?;
            Ok(RunResult {
                success: out.status.success(),
                stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            })
        } else {
            let success = proc::run_visible(program, p_args, dir)?;
            Ok(RunResult {
                success,
                stdout: String::new(),
            })
        }
    };
    release_with(root, version, &mut runner)
}

/// Parsed `release-check` command line arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseCheckArgs {
    /// The tag name (`vX.Y.Z` or `edition-NNN`).
    pub tag: String,
    /// The baseline descriptor file (`release-baseline.toml`).
    pub base: PathBuf,
    /// Optional output folder for public evidence.
    pub evidence_out: Option<PathBuf>,
    /// Optional file to append output key-value lines to.
    pub output: Option<PathBuf>,
}

/// Parse command-line arguments for `cargo xtask release-check`.
pub fn parse_release_check_cli(args: &[&str]) -> Result<ReleaseCheckArgs, String> {
    let mut tag: Option<String> = None;
    let mut base: Option<PathBuf> = None;
    let mut evidence_out: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(&arg) = it.next() {
        match arg {
            "--base" => {
                let v = it
                    .next()
                    .ok_or_else(|| format!("--base needs a value\n{USAGE_RELEASE_CHECK}"))?;
                if base.replace(PathBuf::from(*v)).is_some() {
                    return Err(format!("--base given twice\n{USAGE_RELEASE_CHECK}"));
                }
            }
            "--evidence-out" => {
                let v = it.next().ok_or_else(|| {
                    format!("--evidence-out needs a value\n{USAGE_RELEASE_CHECK}")
                })?;
                if evidence_out.replace(PathBuf::from(*v)).is_some() {
                    return Err(format!("--evidence-out given twice\n{USAGE_RELEASE_CHECK}"));
                }
            }
            "--output" => {
                let v = it
                    .next()
                    .ok_or_else(|| format!("--output needs a value\n{USAGE_RELEASE_CHECK}"))?;
                if output.replace(PathBuf::from(*v)).is_some() {
                    return Err(format!("--output given twice\n{USAGE_RELEASE_CHECK}"));
                }
            }
            flag if flag.starts_with("--") => {
                return Err(format!("unknown flag `{flag}`\n{USAGE_RELEASE_CHECK}"));
            }
            value => {
                if tag.replace(value.to_owned()).is_some() {
                    return Err(format!("only one tag is accepted\n{USAGE_RELEASE_CHECK}"));
                }
            }
        }
    }
    Ok(ReleaseCheckArgs {
        tag: tag.ok_or_else(|| format!("a tag is required\n{USAGE_RELEASE_CHECK}"))?,
        base: base.ok_or_else(|| format!("--base FILE is required\n{USAGE_RELEASE_CHECK}"))?,
        evidence_out,
        output,
    })
}

fn check_refuse(rule: &str, detail: &str) -> Result<bool, String> {
    eprintln!("release-check: refused: {rule}: {detail}");
    Ok(false)
}

fn is_lower_hex(s: &str, len: usize) -> bool {
    s.len() == len
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Execute release verification checks for `args` at repository `root`.
pub fn release_check(root: &Path, args: &ReleaseCheckArgs) -> Result<bool, String> {
    // 1. `tag-grammar`: tag is vX.Y.Z or edition-NNN
    let is_tool = ToolRelease::parse(&args.tag).is_ok();
    let is_edition = EditionId::parse(&args.tag).is_ok();
    if !is_tool && !is_edition {
        return check_refuse(
            "tag-grammar",
            &format!("tag `{}` must be `vX.Y.Z` or `edition-NNN`", args.tag),
        );
    }

    // 2. `origin-main`: refs/remotes/origin/main must verify
    let origin_main = proc::output(
        "git",
        &["rev-parse", "--verify", "refs/remotes/origin/main"],
        root,
    );
    match origin_main {
        Ok(out) if out.status.success() => {}
        _ => {
            return check_refuse(
                "origin-main",
                "`origin/main` is missing from this clone; fetch full history",
            );
        }
    }

    // 3. `tag-commit`: tag resolves, equals HEAD, tracked tree is clean
    let tag_ref = format!("refs/tags/{}^{{commit}}", args.tag);
    let tag_commit_out = proc::output("git", &["rev-parse", "--verify", &tag_ref], root);
    let tag_commit = match tag_commit_out {
        Ok(out) if out.status.success() => {
            let c = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            if !is_lower_hex(&c, 40) {
                return check_refuse(
                    "tag-commit",
                    &format!("tag `{}` resolved to invalid commit `{c}`", args.tag),
                );
            }
            c
        }
        _ => {
            return check_refuse(
                "tag-commit",
                &format!("tag `{}` does not resolve to a commit", args.tag),
            );
        }
    };

    let head_out = proc::output("git", &["rev-parse", "HEAD"], root);
    let head_commit = match head_out {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_owned(),
        _ => return check_refuse("tag-commit", "cannot determine HEAD commit"),
    };
    if head_commit != tag_commit {
        return check_refuse("tag-commit", "the checkout is not the tagged commit");
    }

    let status_out = proc::output(
        "git",
        &["status", "--porcelain", "--untracked-files=no"],
        root,
    );
    match status_out {
        Ok(out) if out.status.success() => {
            if !out.stdout.is_empty() && !out.stdout.trim_ascii().is_empty() {
                return check_refuse("tag-commit", "the tracked tree is not clean");
            }
        }
        _ => return check_refuse("tag-commit", "cannot check working tree status"),
    }

    // 4. `tag-on-main`: ancestor of refs/remotes/origin/main
    let ancestor_out = proc::output(
        "git",
        &[
            "merge-base",
            "--is-ancestor",
            &tag_commit,
            "refs/remotes/origin/main",
        ],
        root,
    );
    match ancestor_out {
        Ok(out) if out.status.success() => {}
        _ => {
            return check_refuse(
                "tag-on-main",
                &format!(
                    "tag `{}` commit `{tag_commit}` is not an ancestor of `origin/main`",
                    args.tag
                ),
            );
        }
    }

    // 5. version-mismatch (tool) or edition-tag (edition)
    let (kind_str, stream_str, edition_number, edition_kind) = if is_tool {
        let show_cargo = proc::output("git", &["show", &format!("{tag_commit}:Cargo.toml")], root);
        let cargo_text = match show_cargo {
            Ok(out) if out.status.success() => match std::str::from_utf8(&out.stdout) {
                Ok(s) => s.to_owned(),
                Err(e) => {
                    return check_refuse(
                        "version-mismatch",
                        &format!("Cargo.toml at `{tag_commit}` is not UTF-8: {e}"),
                    );
                }
            },
            _ => {
                return check_refuse(
                    "version-mismatch",
                    &format!("cannot read `Cargo.toml` from commit `{tag_commit}`"),
                );
            }
        };
        let ws_ver = match get_workspace_version(&cargo_text) {
            Ok(v) => v,
            Err(e) => return check_refuse("version-mismatch", &e),
        };
        let Some(expected_ver) = args.tag.strip_prefix('v') else {
            return check_refuse(
                "tag-grammar",
                &format!("tool tag `{}` lacks `v` prefix", args.tag),
            );
        };
        if ws_ver != expected_ver {
            return check_refuse(
                "version-mismatch",
                &format!(
                    "tag {} names {expected_ver} but the workspace version is {ws_ver}",
                    args.tag
                ),
            );
        }
        ("tool", "tool", None, None)
    } else {
        let validated = match validate_edition_tag(root, &args.tag, None) {
            Ok(v) => v,
            Err(e) => return check_refuse("edition-tag", &e),
        };
        (
            validated.kind.as_str(),
            "edition",
            Some(validated.edition_number),
            Some(validated.kind),
        )
    };

    // 6. `baseline`: byte-identical to committed release-baseline.toml and parses
    let base_path = if args.base.is_absolute() {
        args.base.clone()
    } else {
        root.join(&args.base)
    };
    let base_bytes = match fs::read(&base_path) {
        Ok(b) => b,
        Err(e) => {
            return check_refuse(
                "baseline",
                &format!("cannot read `--base {}`: {e}", args.base.display()),
            );
        }
    };
    let committed_base = proc::output(
        "git",
        &["show", &format!("{tag_commit}:release-baseline.toml")],
        root,
    );
    let committed_bytes = match committed_base {
        Ok(out) if out.status.success() => out.stdout,
        _ => {
            return check_refuse(
                "baseline",
                &format!("cannot read `release-baseline.toml` from commit `{tag_commit}`"),
            );
        }
    };
    if base_bytes != committed_bytes {
        return check_refuse(
            "baseline",
            "`--base` is not byte-identical to committed `release-baseline.toml`",
        );
    }
    let base_text = match std::str::from_utf8(&base_bytes) {
        Ok(s) => s,
        Err(e) => {
            return check_refuse(
                "baseline",
                &format!("release-baseline.toml is not UTF-8: {e}"),
            );
        }
    };
    if let Err(e) = parse_baselines(base_text) {
        return check_refuse(
            "baseline",
            &format!("release-baseline.toml failed to parse: {e}"),
        );
    }

    // 7. `compatibility`: crate::compat::command returns Ok(true)
    match crate::compat::command(root, &base_path) {
        Ok(true) => {}
        Ok(false) => {
            return check_refuse("compatibility", "verification failed against the baseline");
        }
        Err(e) => return check_refuse("compatibility", &format!("verification failed: {e}")),
    }

    // 8. `evidence`: if --evidence-out DIR and edition tag of kind Site
    if let (Some(evidence_out), true, Some(DeliveryKind::Site)) =
        (&args.evidence_out, is_edition, edition_kind)
    {
        let evidence_dir = if evidence_out.is_absolute() {
            evidence_out.clone()
        } else {
            root.join(evidence_out)
        };
        if evidence_dir.exists() {
            let mut entries = match fs::read_dir(&evidence_dir) {
                Ok(e) => e,
                Err(e) => {
                    return check_refuse(
                        "evidence",
                        &format!("cannot read `{}`: {e}", evidence_dir.display()),
                    );
                }
            };
            if entries.next().is_some() {
                return check_refuse(
                    "evidence",
                    &format!(
                        "`--evidence-out {}` must be new or empty",
                        evidence_dir.display()
                    ),
                );
            }
        }
        let published_bytes =
            match crate::edition_release::read_non_symlink_file(root, "compat/published.toml") {
                Ok(b) => b,
                Err(e) => return check_refuse("evidence", &e),
            };
        let published_text = match std::str::from_utf8(&published_bytes) {
            Ok(s) => s,
            Err(e) => {
                return check_refuse(
                    "evidence",
                    &format!("compat/published.toml is not UTF-8: {e}"),
                );
            }
        };
        let published = match crate::compat::parse_published(published_text) {
            Ok(p) => p,
            Err(e) => return check_refuse("evidence", &e),
        };
        let bound = match published.bound_files() {
            Ok(b) => b,
            Err(e) => return check_refuse("evidence", &e),
        };
        let mode_show = proc::output(
            "git",
            &["show", &format!("{tag_commit}:edition-release.toml")],
            root,
        );
        let mode_bytes_opt = match &mode_show {
            Ok(out) if out.status.success() => Some(out.stdout.as_slice()),
            _ => None,
        };
        if let Err(e) = collect_release_evidence(
            root,
            &published_bytes,
            &bound,
            &base_bytes,
            mode_bytes_opt,
            &ReleaseIdentity::new(&args.tag, &tag_commit),
            &evidence_dir,
        ) {
            return check_refuse("evidence", &e);
        }
    }

    // 9. output file appending
    if let Some(output_file) = &args.output {
        let output_path = if output_file.is_absolute() {
            output_file.clone()
        } else {
            root.join(output_file)
        };
        if let Some(parent) = output_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&output_path)
            .map_err(|e| format!("cannot open `--output {}`: {e}", output_path.display()))?;
        let mut out_str = format!("kind={kind_str}\nstream={stream_str}\n");
        if let Some(num) = &edition_number {
            out_str.push_str(&format!("number={num}\n"));
        }
        file.write_all(out_str.as_bytes())
            .map_err(|e| format!("cannot write `--output {}`: {e}", output_path.display()))?;
    }

    // 10. Success output
    println!(
        "release-check: ok: {} is a {kind_str} release at {tag_commit}",
        args.tag
    );
    Ok(true)
}

/// Command handler for `cargo xtask release-check`.
pub fn command_release_check(root: &Path, args: &[&str]) -> Result<bool, String> {
    let parsed = parse_release_check_cli(args)?;
    release_check(root, &parsed)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn release_refreshes_before_verifying() {
        let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("cannot create temp dir: {e}"));
        let root = tmp.path();

        let initial_cargo = "[workspace.package]\nversion = \"0.0.0\"\n";
        let initial_lock = r#"# This file is automatically @generated by Cargo.
version = 4

[[package]]
name = "path-crate-a"
version = "0.0.0"

[[package]]
name = "path-crate-b"
version = "0.0.0"

[[package]]
name = "registry-crate"
version = "0.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
"#;
        let initial_changelog = r#"# Changelog

## Unreleased

- Initial work
"#;

        fs::write(root.join("Cargo.toml"), initial_cargo).unwrap_or_else(|e| panic!("{e}"));
        fs::write(root.join("Cargo.lock"), initial_lock).unwrap_or_else(|e| panic!("{e}"));
        fs::write(root.join("CHANGELOG.md"), initial_changelog).unwrap_or_else(|e| panic!("{e}"));

        let mut recorded: Vec<(String, Vec<String>)> = Vec::new();
        let mut git_status_count = 0;

        let res = release_with(root, "0.1.0", &mut |prog, args, _dir| {
            recorded.push((
                prog.to_owned(),
                args.iter().map(|s| (*s).to_owned()).collect(),
            ));
            if prog == "git" {
                if args == ["rev-parse", "--abbrev-ref", "HEAD"] {
                    return Ok(RunResult {
                        success: true,
                        stdout: "card/x\n".to_owned(),
                    });
                }
                if args == ["status", "--porcelain", "--untracked-files=all"] {
                    git_status_count += 1;
                    if git_status_count == 1 {
                        return Ok(RunResult {
                            success: true,
                            stdout: String::new(),
                        });
                    }
                    return Ok(RunResult {
                        success: true,
                        stdout: " M Cargo.toml\n M Cargo.lock\n M CHANGELOG.md\n".to_owned(),
                    });
                }
            }
            Ok(RunResult {
                success: true,
                stdout: String::new(),
            })
        });

        assert!(res.as_ref().is_ok_and(|&ok| ok));

        // Assert recorded order
        assert_eq!(recorded.len(), 8);
        assert_eq!(recorded[0].0, "git");
        assert_eq!(recorded[0].1, ["rev-parse", "--abbrev-ref", "HEAD"]);
        assert_eq!(recorded[1].0, "git");
        assert_eq!(
            recorded[1].1,
            ["status", "--porcelain", "--untracked-files=all"]
        );
        assert_eq!(recorded[2].0, "just");
        assert_eq!(recorded[2].1, ["check"]);
        assert_eq!(recorded[3].0, "just");
        assert_eq!(recorded[3].1, ["lock"]);
        assert_eq!(recorded[4].0, "cargo");
        assert_eq!(
            recorded[4].1,
            ["deny", "--locked", "check", "bans", "licenses", "sources"]
        );
        assert_eq!(recorded[5].0, "cargo");
        assert_eq!(
            recorded[5].1,
            [
                "deny",
                "--locked",
                "--all-features",
                "check",
                "bans",
                "licenses",
                "sources"
            ]
        );
        assert_eq!(recorded[6].0, "just");
        assert_eq!(recorded[6].1, ["check"]);
        assert_eq!(recorded[7].0, "git");
        assert_eq!(
            recorded[7].1,
            ["status", "--porcelain", "--untracked-files=all"]
        );

        // Assert files edited correctly
        let edited_cargo =
            fs::read_to_string(root.join("Cargo.toml")).unwrap_or_else(|e| panic!("{e}"));
        assert!(edited_cargo.contains("version = \"0.1.0\""));

        let edited_lock =
            fs::read_to_string(root.join("Cargo.lock")).unwrap_or_else(|e| panic!("{e}"));
        assert!(edited_lock.contains("name = \"path-crate-a\"\nversion = \"0.1.0\""));
        assert!(edited_lock.contains("name = \"path-crate-b\"\nversion = \"0.1.0\""));
        assert!(
            edited_lock
                .contains("name = \"registry-crate\"\nversion = \"0.0.0\"\nsource = \"registry+")
        );

        let edited_changelog =
            fs::read_to_string(root.join("CHANGELOG.md")).unwrap_or_else(|e| panic!("{e}"));
        assert!(edited_changelog.contains("## Unreleased\n\n## 0.1.0\n\n- Initial work"));

        // Variant: just lock fails
        fs::write(root.join("Cargo.toml"), initial_cargo).unwrap_or_else(|e| panic!("{e}"));
        fs::write(root.join("Cargo.lock"), initial_lock).unwrap_or_else(|e| panic!("{e}"));
        fs::write(root.join("CHANGELOG.md"), initial_changelog).unwrap_or_else(|e| panic!("{e}"));

        recorded.clear();
        let res_lock_fail = release_with(root, "0.1.0", &mut |prog, args, _dir| {
            recorded.push((
                prog.to_owned(),
                args.iter().map(|s| (*s).to_owned()).collect(),
            ));
            if prog == "git" {
                return Ok(RunResult {
                    success: true,
                    stdout: if args[0] == "rev-parse" {
                        "card/x\n".to_owned()
                    } else {
                        String::new()
                    },
                });
            }
            if prog == "just" && args == ["lock"] {
                return Ok(RunResult {
                    success: false,
                    stdout: String::new(),
                });
            }
            Ok(RunResult {
                success: true,
                stdout: String::new(),
            })
        });
        assert!(res_lock_fail.is_err());
        let just_check_count = recorded
            .iter()
            .filter(|(p, a)| p == "just" && a.as_slice() == ["check"])
            .count();
        assert_eq!(just_check_count, 1);

        // Variant: first just check fails
        fs::write(root.join("Cargo.toml"), initial_cargo).unwrap_or_else(|e| panic!("{e}"));
        fs::write(root.join("Cargo.lock"), initial_lock).unwrap_or_else(|e| panic!("{e}"));
        fs::write(root.join("CHANGELOG.md"), initial_changelog).unwrap_or_else(|e| panic!("{e}"));

        recorded.clear();
        let res_check_fail = release_with(root, "0.1.0", &mut |prog, args, _dir| {
            recorded.push((
                prog.to_owned(),
                args.iter().map(|s| (*s).to_owned()).collect(),
            ));
            if prog == "git" {
                return Ok(RunResult {
                    success: true,
                    stdout: if args[0] == "rev-parse" {
                        "card/x\n".to_owned()
                    } else {
                        String::new()
                    },
                });
            }
            if prog == "just" && args == ["check"] {
                return Ok(RunResult {
                    success: false,
                    stdout: String::new(),
                });
            }
            Ok(RunResult {
                success: true,
                stdout: String::new(),
            })
        });
        assert!(res_check_fail.is_err());
        let unchanged_cargo =
            fs::read_to_string(root.join("Cargo.toml")).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(unchanged_cargo, initial_cargo);
        let unchanged_lock =
            fs::read_to_string(root.join("Cargo.lock")).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(unchanged_lock, initial_lock);
        let unchanged_changelog =
            fs::read_to_string(root.join("CHANGELOG.md")).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(unchanged_changelog, initial_changelog);
    }

    #[test]
    fn release_rejects_unrelated_changes() {
        let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("cannot create temp dir: {e}"));
        let root = tmp.path();

        fs::write(
            root.join("Cargo.toml"),
            "[workspace.package]\nversion = \"0.0.0\"\n",
        )
        .unwrap_or_else(|e| panic!("{e}"));
        fs::write(root.join("Cargo.lock"), "version = 4\n").unwrap_or_else(|e| panic!("{e}"));
        fs::write(root.join("CHANGELOG.md"), "## Unreleased\n\n- Fix\n")
            .unwrap_or_else(|e| panic!("{e}"));

        let mut git_status_count = 0;
        let res = release_with(root, "0.1.0", &mut |prog, args, _dir| {
            if prog == "git" {
                if args == ["rev-parse", "--abbrev-ref", "HEAD"] {
                    return Ok(RunResult {
                        success: true,
                        stdout: "card/test\n".to_owned(),
                    });
                }
                if args == ["status", "--porcelain", "--untracked-files=all"] {
                    git_status_count += 1;
                    if git_status_count == 1 {
                        return Ok(RunResult {
                            success: true,
                            stdout: String::new(),
                        });
                    }
                    return Ok(RunResult {
                        success: true,
                        stdout: " M Cargo.toml\n M src/other.rs\n".to_owned(),
                    });
                }
            }
            Ok(RunResult {
                success: true,
                stdout: String::new(),
            })
        });

        match res {
            Err(msg) => assert!(msg.contains("src/other.rs")),
            Ok(_) => panic!("expected error for unrelated changes"),
        }

        // Allowed set passes. The first run edited the files, so restore them.
        fs::write(
            root.join("Cargo.toml"),
            "[workspace.package]\nversion = \"0.0.0\"\n",
        )
        .unwrap_or_else(|e| panic!("{e}"));
        fs::write(root.join("Cargo.lock"), "version = 4\n").unwrap_or_else(|e| panic!("{e}"));
        fs::write(root.join("CHANGELOG.md"), "## Unreleased\n\n- Fix\n")
            .unwrap_or_else(|e| panic!("{e}"));
        git_status_count = 0;
        let res_allowed = release_with(root, "0.2.0", &mut |prog, args, _dir| {
            if prog == "git" {
                if args == ["rev-parse", "--abbrev-ref", "HEAD"] {
                    return Ok(RunResult {
                        success: true,
                        stdout: "card/test\n".to_owned(),
                    });
                }
                if args == ["status", "--porcelain", "--untracked-files=all"] {
                    git_status_count += 1;
                    if git_status_count == 1 {
                        return Ok(RunResult {
                            success: true,
                            stdout: String::new(),
                        });
                    }
                    return Ok(RunResult {
                        success: true,
                        stdout: " M Cargo.toml\n M Cargo.lock\n M CHANGELOG.md\n".to_owned(),
                    });
                }
            }
            Ok(RunResult {
                success: true,
                stdout: String::new(),
            })
        });
        assert!(res_allowed.is_ok_and(|ok| ok));
    }

    #[test]
    fn pure_set_workspace_version() {
        let input = "[workspace]\nresolver = \"3\"\n\n[workspace.package]\n# comment\nversion = \"0.0.0\"\nedition = \"2024\"\n";
        let updated = set_workspace_version(input, "1.2.3").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            updated,
            "[workspace]\nresolver = \"3\"\n\n[workspace.package]\n# comment\nversion = \"1.2.3\"\nedition = \"2024\"\n"
        );

        let missing = "[workspace]\nresolver = \"3\"\n";
        assert!(set_workspace_version(missing, "1.0.0").is_err());
    }

    #[test]
    fn pure_set_lock_versions() {
        let input = r#"# This file is automatically @generated by Cargo.
version = 4

[[package]]
name = "workspace-a"
version = "0.0.0"

[[package]]
name = "workspace-b"
version = "0.0.0"

[[package]]
name = "other-workspace"
version = "0.9.0"

[[package]]
name = "external"
version = "0.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
"#;
        let updated = set_lock_versions(input, "0.0.0", "1.0.0");
        assert!(updated.contains("name = \"workspace-a\"\nversion = \"1.0.0\""));
        assert!(updated.contains("name = \"workspace-b\"\nversion = \"1.0.0\""));
        assert!(updated.contains("name = \"other-workspace\"\nversion = \"0.9.0\""));
        assert!(updated.contains("name = \"external\"\nversion = \"0.0.0\"\nsource = \"registry+"));
    }

    #[test]
    fn pure_roll_changelog() {
        let input = "# Changelog\n\n## Unreleased\n\n- Note 1\n- Note 2\n";
        let updated = roll_changelog(input, "1.0.0").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            updated,
            "# Changelog\n\n## Unreleased\n\n## 1.0.0\n\n- Note 1\n- Note 2\n"
        );

        let missing = "# Changelog\n\n- Note\n";
        assert!(roll_changelog(missing, "1.0.0").is_err());
    }

    #[test]
    fn version_parsing_and_comparison() {
        let v1 = parse_version("1.0.0").unwrap_or_else(|e| panic!("{e}"));
        let v2 = parse_version("0.9.9").unwrap_or_else(|e| panic!("{e}"));
        let v3 = parse_version("0.0.0").unwrap_or_else(|e| panic!("{e}"));
        let v4 = parse_version("0.10.0").unwrap_or_else(|e| panic!("{e}"));
        let v5 = parse_version("0.9.0").unwrap_or_else(|e| panic!("{e}"));

        assert!(v1 > v2);
        assert!(!(v3 > v3));
        assert!(v4 > v5);

        // Invalid versions
        assert!(parse_version("v1.0.0").is_err());
        assert!(parse_version("1.0").is_err());
        assert!(parse_version("1.0.0.0").is_err());
        assert!(parse_version("01.0.0").is_err());
        assert!(parse_version("0.00.0").is_err());
        assert!(parse_version("1.0.0-beta").is_err());
        assert!(parse_version("1.0.0+patch").is_err());
    }

    #[test]
    fn porcelain_parser() {
        let input = " M Cargo.toml\nM  Cargo.lock\n?? new-file.txt\nR  old.rs -> new.rs\n";
        let paths = parse_porcelain_paths(input);
        assert_eq!(
            paths,
            vec!["Cargo.toml", "Cargo.lock", "new-file.txt", "new.rs"]
        );
    }

    #[test]
    fn release_check_argument_parsing() {
        let basic = parse_release_check_cli(&["v1.0.0", "--base", "release-baseline.toml"])
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(basic.tag, "v1.0.0");
        assert_eq!(basic.base, PathBuf::from("release-baseline.toml"));
        assert_eq!(basic.evidence_out, None);
        assert_eq!(basic.output, None);

        let full = parse_release_check_cli(&[
            "edition-001",
            "--base",
            "base.toml",
            "--evidence-out",
            "evidence_dir",
            "--output",
            "github_output",
        ])
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(full.tag, "edition-001");
        assert_eq!(full.base, PathBuf::from("base.toml"));
        assert_eq!(full.evidence_out, Some(PathBuf::from("evidence_dir")));
        assert_eq!(full.output, Some(PathBuf::from("github_output")));

        // Errors
        assert!(parse_release_check_cli(&["--base", "base.toml"]).is_err());
        assert!(parse_release_check_cli(&["v1.0.0"]).is_err());
        assert!(parse_release_check_cli(&["v1.0.0", "--base"]).is_err());
        assert!(
            parse_release_check_cli(&["v1.0.0", "--base", "b1.toml", "--base", "b2.toml"]).is_err()
        );
        assert!(parse_release_check_cli(&["v1.0.0", "--base", "b.toml", "--unknown"]).is_err());
        assert!(
            parse_release_check_cli(&["v1.0.0", "v2.0.0", "--base", "release-baseline.toml"])
                .is_err()
        );
    }
}
