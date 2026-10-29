//! Fixed, reviewed GS-01 exceptions; this is not general field-level GS-02.

use crate::proc;
use std::path::Path;

const BUNDLE: [(&str, Option<&str>, &str); 3] = [
    (
        "corpus/pages/foundation/onboarding/fnd-onboarding-plan.md",
        None,
        "2f2584e47913ae2feab312b546f6066ef3ad0563",
    ),
    (
        "corpus/registers/glossary.csv",
        Some("f24b36c6d89ab0aa16dabd56ea0a1904bbf7b56f"),
        "6fbc17b6037853e23bdea4ececbbe487b69671dc",
    ),
    (
        "corpus/registers/questions.csv",
        Some("15d01ba5910e1df464fad782809d9e02ed20c290"),
        "febee9a7be2bfbc3d779b9d8bc6303c72fed4128",
    ),
];

pub(super) fn is_protected(path: &str) -> bool {
    path.starts_with("corpus/") || path.rsplit('/').next().unwrap_or(path).starts_with("seed-")
}

/// Read raw objects from the explicit repository, unaffected by hook Git context.
pub(super) fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let root = repo.to_str().ok_or("repository path is not UTF-8")?;
    let mut argv = Vec::new();
    for name in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CONFIG",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_GLOBAL",
        "GIT_CONFIG_SYSTEM",
    ] {
        argv.extend(["-u", name]);
    }
    argv.extend([
        "git",
        "--no-replace-objects",
        "--literal-pathspecs",
        "-C",
        root,
    ]);
    argv.extend_from_slice(args);
    proc::capture("env", &argv, repo)
}

/// Match both the immutable blob and its regular, non-executable tree mode.
fn matches(
    repo: &Path,
    revision: &str,
    path: &str,
    expected: Option<&str>,
) -> Result<bool, String> {
    let entry = git(repo, &["ls-tree", "-z", revision, "--", path])?;
    Ok(match expected {
        Some(blob) => entry == format!("100644 blob {blob}\t{path}\0"),
        None => entry.is_empty(),
    })
}

pub(super) fn allows(
    repo: &Path,
    before: &str,
    after: &str,
    paths: &[String],
) -> Result<bool, String> {
    let protected: Vec<_> = paths.iter().filter(|path| is_protected(path)).collect();
    if protected
        .iter()
        .map(|path| path.as_str())
        .eq(["corpus/AGENTS.md"])
    {
        return Ok(matches(
            repo,
            before,
            "corpus/AGENTS.md",
            Some("091b8d48329272c2325868592dbcef54c59fe931"),
        )? && matches(
            repo,
            after,
            "corpus/AGENTS.md",
            Some("daf65de262f5dc7b4a6240c42e1b04ecaeab9321"),
        )?);
    }
    if paths
        .iter()
        .map(String::as_str)
        .ne(BUNDLE.iter().map(|row| row.0))
    {
        return Ok(false);
    }
    for (path, old, new) in BUNDLE {
        if !matches(repo, before, path, old)? || !matches(repo, after, path, Some(new))? {
            return Ok(false);
        }
    }
    Ok(true)
}
