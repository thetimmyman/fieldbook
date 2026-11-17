//! The raw edition release lifecycle: tag checking, the Raw archive provider and the evidence
//! that the raw workflow uploads (`docs/runbooks/release.md#raw-edition-floor`).
//!
//! `raw-edition-check <tag> --base FILE --evidence-out DIR` validates the owner-created tag
//! with the shared validator, verifies compatibility against the committed baseline and writes
//! the public evidence set. [`raw_archive_provider`] is the payload verifier and restore
//! inspector that `verify-archive` dispatches to for a `raw` edition archive. Nothing here calls
//! a Site handler, a renderer, a browser or an executable.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use fieldbook_core::hash::sha256_hex;

use crate::allowlist::{AllowlistProfile, verify_staged_trees};
use crate::compat::{parse_baselines, parse_published};
use crate::edition_raw::require_clean_tree;
use crate::edition_release::{
    ArchiveManifest, DeliveryKind, RELEASE_IDENTITY_FILE, ReleaseIdentity, ReleaseStream,
    ValidatedEditionTag, collect_all_rel_files, collect_release_evidence,
    parse_edition_release_mode, read_non_symlink_file, register_archive_provider,
    validate_edition_tag, verify_archived_identity, verify_edition_artifact_set,
};
use crate::proc;
use crate::zipdet::read_and_verify_deterministic_zip;

const USAGE: &str = "usage: cargo xtask raw-edition-check TAG --base FILE --evidence-out DIR";

/// The files a raw edition delivery holds beside `archive.toml` and `evidence/`.
const DELIVERED_SIDECARS: [&str; 3] =
    ["SHA256SUMS", "corpus-support.toml", "corpus-support.sha256"];

/// Register the Raw payload verifier with the shared archive framework.
pub fn register_raw_provider() -> Result<(), String> {
    register_archive_provider(
        ReleaseStream::Edition,
        DeliveryKind::Raw,
        raw_archive_provider,
    )
}

/// Verify the archived clean strict-check and regression reports against the edition identity.
pub fn verify_reports(
    check_json: &[u8],
    regression_json: &[u8],
    tag: &str,
    commit: &str,
) -> Result<(), String> {
    let check: serde_json::Value = serde_json::from_slice(check_json)
        .map_err(|e| format!("archived evidence/check.json is not valid JSON: {e}"))?;
    if check
        .get("report_version")
        .and_then(serde_json::Value::as_u64)
        != Some(1)
        || check.get("strict").and_then(serde_json::Value::as_bool) != Some(true)
    {
        return Err("archived evidence/check.json is not a strict version 1 report".to_owned());
    }
    if check.get("errors").and_then(serde_json::Value::as_u64) != Some(0) {
        return Err("archived evidence/check.json records errors".to_owned());
    }
    let hard = check
        .get("diagnostics")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "archived evidence/check.json has no diagnostics list".to_owned())?
        .iter()
        .filter(|d| {
            matches!(
                d.get("code").and_then(serde_json::Value::as_str),
                Some("FB0126" | "FB0128")
            )
        })
        .count();
    if hard != 0 {
        return Err("archived evidence/check.json records a regression failure".to_owned());
    }

    let regression: serde_json::Value = serde_json::from_slice(regression_json)
        .map_err(|e| format!("archived evidence/regression.json is not valid JSON: {e}"))?;
    let field = |k: &str| regression.get(k).and_then(serde_json::Value::as_str);
    let count = |k: &str| regression.get(k).and_then(serde_json::Value::as_u64);
    if regression
        .get("passed")
        .and_then(serde_json::Value::as_bool)
        != Some(true)
        || count("critical_misses") != Some(0)
        || count("cases").is_none()
        || count("critical_cases").is_none_or(|c| Some(c) > count("cases"))
    {
        return Err(
            "archived evidence/regression.json does not record a passed regression".to_owned(),
        );
    }
    if field("edition") != Some(tag) || field("commit") != Some(commit) {
        return Err(
            "archived evidence/regression.json names a different edition or commit".to_owned(),
        );
    }
    // The check report is bound to this regression result by its exact bytes and its date, so a
    // clean report from another commit or edition cannot stand in for it.
    if field("check_sha256") != Some(sha256_hex(check_json).as_str())
        || check
            .get("today")
            .and_then(serde_json::Value::as_str)
            .is_none()
        || check.get("today").and_then(serde_json::Value::as_str) != field("today")
    {
        return Err(
            "archived evidence/check.json is not the report that evidence/regression.json names"
                .to_owned(),
        );
    }
    Ok(())
}

/// Payload verifier and restore inspector for a raw edition archive (`stream = "edition"`,
/// `kind = "raw"`). It reopens the delivered pair, checks the exact file set, the identity, the
/// clean reports and the Raw allowlist, and restores both trees under `restore_dir`.
pub fn raw_archive_provider(
    archive_dir: &Path,
    restore_dir: &Path,
    manifest: &ArchiveManifest,
) -> Result<(), String> {
    if manifest.stream != ReleaseStream::Edition || manifest.kind != DeliveryKind::Raw {
        return Err("the raw provider verifies only `edition` archives of kind `raw`".to_owned());
    }
    let artifacts = verify_edition_artifact_set(archive_dir, DeliveryKind::Raw)?;
    if artifacts.ed_tag != manifest.tag || artifacts.ed_info.commit != manifest.commit {
        return Err("the archived pair does not carry the archive's tag and commit".to_owned());
    }

    // The exact provider file set: the pair, the three sidecars and the retained evidence.
    let number = manifest
        .tag
        .strip_prefix("edition-")
        .ok_or_else(|| format!("invalid raw edition tag `{}`", manifest.tag))?;
    let allowed_fixed: BTreeSet<String> = [
        artifacts.ed_zip_name.clone(),
        artifacts.orig_zip_name.clone(),
        "archive.toml".to_owned(),
    ]
    .into_iter()
    .chain(DELIVERED_SIDECARS.iter().map(|s| (*s).to_owned()))
    .collect();
    for entry in &manifest.files {
        let ok = allowed_fixed.contains(&entry.path) || entry.path.starts_with("evidence/");
        if !ok {
            return Err(format!(
                "`{}` is not part of a raw archive (no package, intake folder or site file is allowed)",
                entry.path
            ));
        }
    }
    for fixed in &allowed_fixed {
        if fixed != "archive.toml" && !manifest.files.iter().any(|f| &f.path == fixed) {
            return Err(format!("raw archive is missing `{fixed}`"));
        }
    }

    // Evidence: exactly the committed descriptors, the manifest and the vectors it binds.
    let mode_bytes = read_non_symlink_file(archive_dir, "evidence/edition-release.toml")
        .map_err(|e| format!("raw archive needs the committed mode descriptor: {e}"))?;
    let mode_text = std::str::from_utf8(&mode_bytes)
        .map_err(|e| format!("evidence/edition-release.toml is not UTF-8: {e}"))?;
    let mode = parse_edition_release_mode(Some(mode_text), Some(number))?;
    if mode.kind != DeliveryKind::Raw {
        return Err("archived mode descriptor does not select `raw`".to_owned());
    }
    let published_bytes = read_non_symlink_file(archive_dir, "evidence/compat/published.toml")?;
    let published = parse_published(
        std::str::from_utf8(&published_bytes)
            .map_err(|e| format!("evidence/compat/published.toml is not UTF-8: {e}"))?,
    )?;
    let mut expected_evidence: BTreeSet<String> = published
        .bound_files()?
        .into_iter()
        .map(|(p, _)| format!("evidence/{p}"))
        .collect();
    for fixed in [
        "evidence/compat/published.toml",
        "evidence/release-baseline.toml",
        "evidence/edition-release.toml",
    ] {
        expected_evidence.insert(fixed.to_owned());
    }
    expected_evidence.insert(format!("evidence/{RELEASE_IDENTITY_FILE}"));
    let actual_evidence: BTreeSet<String> = manifest
        .files
        .iter()
        .filter(|f| f.path.starts_with("evidence/"))
        .map(|f| f.path.clone())
        .collect();
    if actual_evidence != expected_evidence {
        let missing: Vec<&String> = expected_evidence.difference(&actual_evidence).collect();
        let extra: Vec<&String> = actual_evidence.difference(&expected_evidence).collect();
        return Err(format!(
            "raw archive evidence set is not exact (missing {missing:?}, unexpected {extra:?})"
        ));
    }
    let baseline_text = std::str::from_utf8(&read_non_symlink_file(
        archive_dir,
        "evidence/release-baseline.toml",
    )?)
    .map_err(|e| format!("evidence/release-baseline.toml is not UTF-8: {e}"))?
    .to_owned();
    parse_baselines(&baseline_text)?;
    verify_archived_identity(
        archive_dir,
        &artifacts.ed_tag,
        &artifacts.ed_info.commit,
        manifest,
    )?;

    // The clean reports travel inside the edition zip.
    let ed_zip = read_non_symlink_file(archive_dir, &artifacts.ed_zip_name)?;
    let ed_entries = read_and_verify_deterministic_zip(&ed_zip, "edition", None)?;
    let find = |name: &str| -> Result<&Vec<u8>, String> {
        ed_entries
            .iter()
            .find(|(p, _)| p == name)
            .map(|(_, b)| b)
            .ok_or_else(|| format!("edition zip is missing `{name}`"))
    };
    verify_reports(
        find("evidence/check.json")?,
        find("evidence/regression.json")?,
        &manifest.tag,
        &manifest.commit,
    )?;

    // Restore both trees and the delivered sidecars, then verify the restored files on disk.
    let orig_zip = read_non_symlink_file(archive_dir, &artifacts.orig_zip_name)?;
    let orig_entries = read_and_verify_deterministic_zip(&orig_zip, "originals", None)?;
    for (top, entries) in [("edition", &ed_entries), ("originals", &orig_entries)] {
        for (rel, bytes) in entries {
            let target = restore_dir.join(top).join(rel);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("cannot create `{}`: {e}", parent.display()))?;
            }
            fs::write(&target, bytes)
                .map_err(|e| format!("cannot restore `{}`: {e}", target.display()))?;
        }
    }
    fs::create_dir_all(restore_dir.join("delivered"))
        .map_err(|e| format!("cannot create the restore folder: {e}"))?;
    for name in DELIVERED_SIDECARS {
        let bytes = read_non_symlink_file(archive_dir, name)?;
        fs::write(restore_dir.join("delivered").join(name), bytes)
            .map_err(|e| format!("cannot restore `{name}`: {e}"))?;
    }
    verify_staged_trees(
        AllowlistProfile::Raw,
        &restore_dir.join("edition"),
        &restore_dir.join("originals"),
        &[],
    )
    .map_err(|e| format!("restored raw trees fail the allowlist: {e}"))?;
    for representative in [
        "edition/README.txt",
        "edition/TRANSFER-STATEMENT.txt",
        "edition/corpus/corpus.toml",
    ] {
        let text = fs::read_to_string(restore_dir.join(representative))
            .map_err(|e| format!("cannot read restored `{representative}`: {e}"))?;
        if text.trim().is_empty() {
            return Err(format!("restored `{representative}` is empty"));
        }
    }
    Ok(())
}

/// Parsed `raw-edition-check` command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawCheckArgs {
    /// The owner-created tag, `edition-NNN`.
    pub tag: String,
    /// The committed `release-baseline.toml`.
    pub base: PathBuf,
    /// A new, empty folder for the public evidence set.
    pub evidence_out: PathBuf,
}

/// Parse `raw-edition-check TAG --base FILE --evidence-out DIR`.
pub fn parse_raw_edition_check_cli(args: &[&str]) -> Result<RawCheckArgs, String> {
    let mut tag: Option<String> = None;
    let mut base: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(&arg) = it.next() {
        match arg {
            "--base" => {
                let v = it
                    .next()
                    .ok_or_else(|| format!("--base needs a value\n{USAGE}"))?;
                if base.replace(PathBuf::from(*v)).is_some() {
                    return Err(format!("--base given twice\n{USAGE}"));
                }
            }
            "--evidence-out" => {
                let v = it
                    .next()
                    .ok_or_else(|| format!("--evidence-out needs a value\n{USAGE}"))?;
                if out.replace(PathBuf::from(*v)).is_some() {
                    return Err(format!("--evidence-out given twice\n{USAGE}"));
                }
            }
            flag if flag.starts_with("--") => {
                return Err(format!("unknown flag `{flag}`\n{USAGE}"));
            }
            value => {
                if tag.replace(value.to_owned()).is_some() {
                    return Err(format!("only one tag is accepted\n{USAGE}"));
                }
            }
        }
    }
    Ok(RawCheckArgs {
        tag: tag.ok_or_else(|| format!("a tag is required\n{USAGE}"))?,
        base: base.ok_or_else(|| format!("--base FILE is required\n{USAGE}"))?,
        evidence_out: out.ok_or_else(|| format!("--evidence-out DIR is required\n{USAGE}"))?,
    })
}

/// Verifies compatibility at `root` against a baseline file (`xtask compat --base`).
pub type CompatVerifier<'a> = &'a dyn Fn(&Path, &Path) -> Result<bool, String>;

/// Validate a raw tag at `root` and write the public evidence set.
///
/// 1. The shared tag validator: grammar, ancestry on main, newest heading, committed mode `raw`.
/// 2. The checkout is the tagged commit, with a clean tree.
/// 3. `--base` is byte-identical to the committed `release-baseline.toml`, and the compatibility
///    verifier accepts it.
/// 4. The committed mode and baseline descriptors, the published manifest and every bound blob
///    are copied, hash-checked, into a new `--evidence-out` folder, with `release-identity.toml`
///    naming the tag and its commit.
pub fn raw_edition_check(
    root: &Path,
    args: &RawCheckArgs,
    verify_compat: CompatVerifier<'_>,
) -> Result<ValidatedEditionTag, String> {
    let validated = validate_edition_tag(root, &args.tag, Some(DeliveryKind::Raw))?;
    let head = proc::git(root, &["rev-parse", "HEAD"])?.trim().to_owned();
    if head != validated.commit {
        return Err(format!(
            "the checkout is at `{head}`, not at the commit `{}` of tag `{}`",
            validated.commit, args.tag
        ));
    }
    require_clean_tree(&mut |a: &[&str]| proc::git(root, a))?;

    let base_path = if args.base.is_absolute() {
        args.base.clone()
    } else {
        root.join(&args.base)
    };
    let base_bytes = fs::read(&base_path)
        .map_err(|e| format!("cannot read `--base {}`: {e}", args.base.display()))?;
    let committed = proc::output(
        "git",
        &[
            "show",
            &format!("{}:release-baseline.toml", validated.commit),
        ],
        root,
    )?;
    if !committed.status.success() || committed.stdout != base_bytes {
        return Err(
            "`--base` is not the `release-baseline.toml` committed at the tagged commit".to_owned(),
        );
    }
    parse_baselines(
        std::str::from_utf8(&base_bytes)
            .map_err(|e| format!("release-baseline.toml is not UTF-8: {e}"))?,
    )?;
    if !verify_compat(root, &base_path)? {
        return Err("compatibility verification failed against the baseline".to_owned());
    }

    if args.evidence_out.exists()
        && fs::read_dir(&args.evidence_out)
            .map_err(|e| format!("cannot read `{}`: {e}", args.evidence_out.display()))?
            .next()
            .is_some()
    {
        return Err(format!(
            "`--evidence-out {}` must be new or empty",
            args.evidence_out.display()
        ));
    }
    let published_bytes = read_non_symlink_file(root, "compat/published.toml")?;
    let published = parse_published(
        std::str::from_utf8(&published_bytes)
            .map_err(|e| format!("compat/published.toml is not UTF-8: {e}"))?,
    )?;
    let bound = published.bound_files()?;
    let mode_bytes = read_non_symlink_file(root, "edition-release.toml")?;
    collect_release_evidence(
        root,
        &published_bytes,
        &bound,
        &base_bytes,
        Some(&mode_bytes),
        &ReleaseIdentity::new(&args.tag, &validated.commit),
        &args.evidence_out,
    )?;
    Ok(validated)
}

/// Command handler for `cargo xtask raw-edition-check`.
pub fn command_raw_edition_check(root: &Path, args: &[&str]) -> Result<bool, String> {
    let parsed = parse_raw_edition_check_cli(args)?;
    let validated = raw_edition_check(root, &parsed, &|r, b| crate::compat::command(r, b))?;
    println!(
        "raw edition edition-{} at {}: tag, mode, baseline and compatibility verified; evidence in {}",
        validated.edition_number,
        validated.commit,
        parsed.evidence_out.display()
    );
    Ok(true)
}

/// Require two delivered sets (a local run and a downloaded workflow artifact) to hold the same
/// files with the same bytes, and the downloaded set to verify as a raw delivery on its own: the
/// zip hashes against `SHA256SUMS`, each zip's internal sums, and the catalogue digest.
pub fn compare_delivered_sets(local: &Path, downloaded: &Path) -> Result<(), String> {
    let a = collect_all_rel_files(local)?;
    let b = collect_all_rel_files(downloaded)?;
    let names_a: Vec<&String> = a.keys().collect();
    let names_b: Vec<&String> = b.keys().collect();
    if names_a != names_b {
        return Err(format!(
            "the local and downloaded file sets differ: {names_a:?} against {names_b:?}"
        ));
    }
    for (name, bytes) in &a {
        if sha256_hex(bytes) != sha256_hex(&b[name]) {
            return Err(format!(
                "`{name}` differs between the local and downloaded sets"
            ));
        }
    }
    if names_a.is_empty() {
        return Err("both delivered sets are empty".to_owned());
    }
    verify_edition_artifact_set(downloaded, DeliveryKind::Raw)
        .map_err(|e| format!("the downloaded set does not verify: {e}"))?;
    Ok(())
}

const COMPARE_USAGE: &str = "usage: cargo xtask raw-edition-compare LOCAL DOWNLOADED";

/// Command handler for `cargo xtask raw-edition-compare LOCAL DOWNLOADED` (release runbook, raw
/// step 2): the pair built locally at the tagged commit against the downloaded workflow artifact.
pub fn command_raw_edition_compare(args: &[&str]) -> Result<bool, String> {
    let [local, downloaded] = args else {
        return Err(format!(
            "two folders are required, the local pair and the downloaded one\n{COMPARE_USAGE}"
        ));
    };
    if local.starts_with("--") || downloaded.starts_with("--") {
        return Err(format!("no flag is accepted\n{COMPARE_USAGE}"));
    }
    compare_delivered_sets(Path::new(local), Path::new(downloaded))?;
    println!("the local and downloaded raw delivery sets are identical");
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};

    use fieldbook_core::hash::sha256_hex;
    use tempfile::TempDir;

    use super::{
        RawCheckArgs, command_raw_edition_compare, compare_delivered_sets,
        parse_raw_edition_check_cli, raw_archive_provider, raw_edition_check,
        register_raw_provider, verify_reports,
    };
    use crate::compat::{Baselines, parse_baselines, parse_published};
    use crate::edition_raw::sums_of;
    use crate::edition_raw::tests::{
        CONTACT, Scripted, clean_repo, commit_all, dir_listing, git, package, write,
    };
    use crate::edition_release::{
        ArchiveArgs, ArchiveFileEntry, ArchiveManifest, DeliveryKind, RELEASE_IDENTITY_FILE,
        ReleaseIdentity, ReleaseStream, advance_baselines_from_archive, archive_release,
        collect_all_rel_files, register_archive_provider, serialize_baselines, verify_archive,
    };
    use crate::proc;
    use crate::zipdet::{read_and_verify_deterministic_zip, write_deterministic_zip_bytes};

    const EMPTY_BASE: &str = "baseline_schema = 1\n\n[compatibility]\nkind = \"empty\"\n\n[tool]\nkind = \"empty\"\n\n[edition]\nkind = \"empty\"\n";

    fn mode(number: &str, kind: &str) -> String {
        format!("release_schema = 1\nedition = \"{number}\"\nkind = \"{kind}\"\n")
    }

    fn copy_tree(src: &Path, dst: &Path) {
        fs::create_dir_all(dst).expect("mkdir");
        for entry in fs::read_dir(src).expect("read_dir") {
            let entry = entry.expect("entry");
            let to = dst.join(entry.file_name());
            if entry.file_type().expect("type").is_dir() {
                copy_tree(&entry.path(), &to);
            } else {
                fs::copy(entry.path(), to).expect("copy");
            }
        }
    }

    /// A clean synthetic repository whose newest commit is tagged `edition-001`, with the
    /// published compatibility files and the committed raw mode and an empty baseline.
    fn lab() -> TempDir {
        let repo = clean_repo();
        let root = repo.path();
        let real = proc::repo_root();
        copy_tree(&real.join("compat"), &root.join("compat"));
        let published = parse_published(
            &fs::read_to_string(real.join("compat/published.toml")).expect("published"),
        )
        .expect("parses");
        for (path, _) in published.bound_files().expect("bound") {
            let to = root.join(&path);
            fs::create_dir_all(to.parent().expect("parent")).expect("mkdir");
            fs::copy(real.join(&path), to).expect("copy bound file");
        }
        copy_tree(
            &real.join("crates/fieldbook-calc/tests/cases"),
            &root.join("crates/fieldbook-calc/tests/cases"),
        );
        write(root, "edition-release.toml", mode("001", "raw").as_bytes());
        write(root, "release-baseline.toml", EMPTY_BASE.as_bytes());
        commit_all(root);
        git(root, &["tag", "edition-001"]);
        repo
    }

    fn head(root: &Path) -> String {
        proc::git(root, &["rev-parse", "HEAD"])
            .expect("head")
            .trim()
            .to_owned()
    }

    fn accept_compat(_: &Path, _: &Path) -> Result<bool, String> {
        Ok(true)
    }

    fn check_args(tag: &str, evidence_out: &Path) -> RawCheckArgs {
        RawCheckArgs {
            tag: tag.to_owned(),
            base: PathBuf::from("release-baseline.toml"),
            evidence_out: evidence_out.to_path_buf(),
        }
    }

    /// Check the tag, collect the evidence, package the pair, and return both folders.
    fn deliver(root: &Path, work: &Path, tag: &str) -> (PathBuf, PathBuf) {
        let evidence = work.join(format!("evidence-{tag}"));
        raw_edition_check(root, &check_args(tag, &evidence), &accept_compat).expect("tag check");
        let out = work.join(format!("out-{tag}"));
        package(root, &out, &mut Scripted::clean()).expect("packaging");
        (out, evidence)
    }

    fn archive(
        pair: &Path,
        evidence: &Path,
        base: &Path,
        to: &Path,
    ) -> Result<(PathBuf, crate::edition_release::ArchiveManifest, String), String> {
        archive_release(&ArchiveArgs {
            kind: Some(DeliveryKind::Raw),
            package: None,
            edition: pair.to_path_buf(),
            intake: None,
            evidence: Some(evidence.to_path_buf()),
            base: Some(base.to_path_buf()),
            to: to.to_path_buf(),
        })
    }

    fn base_of(root: &Path) -> PathBuf {
        root.join("release-baseline.toml")
    }

    fn restore(name: &str, work: &Path) -> PathBuf {
        work.join(format!("restore-{name}"))
    }

    fn read_baselines(path: &Path) -> Baselines {
        parse_baselines(&fs::read_to_string(path).expect("baseline")).expect("parses")
    }

    /// Rewrite the edition zip of a delivered pair after `edit`, keeping every other file.
    fn repack(pair: &Path, to: &Path, edit: impl FnOnce(&mut BTreeMap<String, Vec<u8>>)) {
        copy_tree(pair, to);
        let name = "fieldbook-edition-001.zip";
        let mut files: BTreeMap<String, Vec<u8>> = read_and_verify_deterministic_zip(
            &fs::read(pair.join(name)).expect("zip"),
            "edition",
            None,
        )
        .expect("a verified zip")
        .into_iter()
        .collect();
        edit(&mut files);
        files.remove("SHA256SUMS");
        let sums = sums_of(&files);
        files.insert("SHA256SUMS".to_owned(), sums.into_bytes());
        // The synthetic commits are dated 2026-03-02T12:00:00Z.
        let epoch: i64 = 1_772_452_800;
        let zip = write_deterministic_zip_bytes("edition", epoch, &files).expect("zip");
        fs::write(to.join(name), &zip).expect("write zip");
        let orig = fs::read(to.join("fieldbook-originals-001.zip")).expect("orig");
        let outer = fieldbook_core::manifest::write_sums(&[
            (name.to_owned(), sha256_hex(&zip)),
            ("fieldbook-originals-001.zip".to_owned(), sha256_hex(&orig)),
        ]);
        fs::write(to.join("SHA256SUMS"), outer).expect("sums");
    }

    #[test]
    fn command_line_needs_a_tag_a_base_and_a_new_evidence_folder() {
        let ok = parse_raw_edition_check_cli(&[
            "edition-001",
            "--base",
            "release-baseline.toml",
            "--evidence-out",
            "out/raw-evidence",
        ])
        .expect("valid");
        assert_eq!(ok.tag, "edition-001");
        assert_eq!(ok.base, PathBuf::from("release-baseline.toml"));
        assert_eq!(ok.evidence_out, PathBuf::from("out/raw-evidence"));
        for bad in [
            vec!["--base", "b", "--evidence-out", "o"],
            vec!["edition-001", "--evidence-out", "o"],
            vec!["edition-001", "--base", "b"],
            vec![
                "edition-001",
                "edition-002",
                "--base",
                "b",
                "--evidence-out",
                "o",
            ],
            vec!["edition-001", "--base", "b", "--evidence-out", "o", "--x"],
            vec!["edition-001", "--base"],
        ] {
            assert!(parse_raw_edition_check_cli(&bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn tag_selection_refuses_wrong_mode_heading_and_ancestry() {
        let repo = lab();
        let root = repo.path();
        let work = TempDir::new().expect("work");

        // A valid raw tag passes.
        raw_edition_check(
            root,
            &check_args("edition-001", &work.path().join("ok")),
            &accept_compat,
        )
        .expect("valid tag");

        // The committed mode selects the site edition: the raw command refuses it.
        write(root, "edition-release.toml", mode("002", "site").as_bytes());
        write(
            root,
            "corpus/changes.md",
            b"# Corpus changes\n\nSynthetic changes log.\n\n## edition-002\n\n- Added: synthetic.\n\n## edition-001\n\n- Added: synthetic.\n",
        );
        commit_all(root);
        git(root, &["tag", "edition-002"]);
        let err = raw_edition_check(
            root,
            &check_args("edition-002", &work.path().join("m")),
            &accept_compat,
        )
        .expect_err("a site tag is not a raw tag");
        assert!(
            err.contains("raw") || err.contains("site") || err.contains("kind"),
            "{err}"
        );

        // A tag that is not the newest heading is refused.
        write(root, "edition-release.toml", mode("002", "raw").as_bytes());
        commit_all(root);
        git(root, &["tag", "edition-003"]);
        let err = raw_edition_check(
            root,
            &check_args("edition-003", &work.path().join("h")),
            &accept_compat,
        )
        .expect_err("heading mismatch");
        assert!(
            err.contains("heading") || err.contains("edition-002"),
            "{err}"
        );

        // A tag outside the main history is refused.
        git(root, &["checkout", "-q", "-b", "side"]);
        write(
            root,
            "corpus/changes.md",
            b"# Corpus changes\n\nSynthetic changes log.\n\n## edition-004\n\n- Added: side.\n",
        );
        write(root, "edition-release.toml", mode("004", "raw").as_bytes());
        commit_all(root);
        git(root, &["tag", "edition-004"]);
        let err = raw_edition_check(
            root,
            &check_args("edition-004", &work.path().join("a")),
            &accept_compat,
        )
        .expect_err("not on main");
        assert!(err.contains("ancestor"), "{err}");

        // A tag that does not exist is refused.
        assert!(
            raw_edition_check(
                root,
                &check_args("edition-099", &work.path().join("n")),
                &accept_compat
            )
            .is_err()
        );
    }

    #[test]
    fn check_refuses_a_dirty_tree_a_moved_checkout_and_a_missing_or_different_baseline() {
        let repo = lab();
        let root = repo.path();
        let work = TempDir::new().expect("work");
        let run = |name: &str, base: &str| {
            raw_edition_check(
                root,
                &RawCheckArgs {
                    tag: "edition-001".to_owned(),
                    base: PathBuf::from(base),
                    evidence_out: work.path().join(name),
                },
                &accept_compat,
            )
        };

        // Missing baseline file.
        let err = run("a", "no-such-baseline.toml").expect_err("missing");
        assert!(err.contains("cannot read"), "{err}");

        // A baseline that is not the committed one.
        write(
            work.path(),
            "other.toml",
            format!("{EMPTY_BASE}# changed\n").as_bytes(),
        );
        let other = work.path().join("other.toml");
        let err = run("b", other.to_str().expect("utf8")).expect_err("different baseline");
        assert!(err.contains("committed"), "{err}");

        // A failed compatibility verification.
        let err = raw_edition_check(
            root,
            &check_args("edition-001", &work.path().join("c")),
            &|_, _| Ok(false),
        )
        .expect_err("compat failure");
        assert!(err.contains("compatibility"), "{err}");

        // An untracked file makes the tree dirty.
        write(root, "stray.txt", b"x\n");
        let err = run("d", "release-baseline.toml").expect_err("dirty");
        assert!(err.contains("clean"), "{err}");
        fs::remove_file(root.join("stray.txt")).expect("rm");

        // A skip-worktree edit that `git status` does not report is still unclean.
        write(root, "corpus/changes.md", b"# hidden edit\n");
        git(
            root,
            &["update-index", "--skip-worktree", "corpus/changes.md"],
        );
        let err = run("d2", "release-baseline.toml").expect_err("skip-worktree");
        assert!(err.contains("not clean"), "{err}");
        git(
            root,
            &["update-index", "--no-skip-worktree", "corpus/changes.md"],
        );
        git(root, &["checkout", "--", "corpus/changes.md"]);

        // A checkout that is not at the tagged commit.
        write(root, "later.txt", b"x\n");
        commit_all(root);
        let err = run("e", "release-baseline.toml").expect_err("moved");
        assert!(err.contains("not at the commit"), "{err}");

        // An evidence folder that is not empty.
        git(root, &["checkout", "-q", "edition-001"]);
        write(work.path(), "full/x.txt", b"x");
        let err = run("full", "release-baseline.toml").expect_err("not empty");
        assert!(err.contains("new or empty"), "{err}");
    }

    #[test]
    fn check_collects_exactly_the_committed_public_evidence() {
        let repo = lab();
        let root = repo.path();
        let work = TempDir::new().expect("work");
        let evidence = work.path().join("ev");
        raw_edition_check(root, &check_args("edition-001", &evidence), &accept_compat)
            .expect("check");

        let files = collect_all_rel_files(&evidence).expect("files");
        let published = parse_published(
            &fs::read_to_string(root.join("compat/published.toml")).expect("published"),
        )
        .expect("parses");
        let mut expected: Vec<String> = published
            .bound_files()
            .expect("bound")
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        expected.extend([
            "compat/published.toml".to_owned(),
            "edition-release.toml".to_owned(),
            "release-baseline.toml".to_owned(),
            "release-identity.toml".to_owned(),
        ]);
        expected.sort();
        assert_eq!(files.keys().cloned().collect::<Vec<_>>(), expected);
        assert_eq!(files["edition-release.toml"], mode("001", "raw").as_bytes());
        assert_eq!(files["release-baseline.toml"], EMPTY_BASE.as_bytes());
        assert_eq!(
            files["release-identity.toml"],
            ReleaseIdentity::new("edition-001", &head(root))
                .to_toml()
                .as_bytes()
        );

        // A changed bound blob refuses before any evidence is written.
        let some = expected
            .iter()
            .find(|p| p.starts_with("compat/") && !p.ends_with("published.toml"))
            .expect("a bound file")
            .clone();
        fs::write(root.join(&some), b"changed\n").expect("tamper");
        commit_all(root);
        git(root, &["tag", "-f", "edition-001"]);
        let err = raw_edition_check(
            root,
            &check_args("edition-001", &work.path().join("ev2")),
            &accept_compat,
        )
        .expect_err("changed blob");
        assert!(err.contains("sha256"), "{err}");
        assert!(
            !work.path().join("ev2").exists() || dir_listing(&work.path().join("ev2")).is_empty()
        );
    }

    #[test]
    fn full_raw_lifecycle_runs_without_any_site_provider() {
        static SITE_CALLED: AtomicBool = AtomicBool::new(false);
        fn site_hook(
            _: &Path,
            _: &Path,
            _: &crate::edition_release::ArchiveManifest,
        ) -> Result<(), String> {
            SITE_CALLED.store(true, Ordering::SeqCst);
            Err("a site provider must not run for a raw archive".to_owned())
        }
        register_raw_provider().expect("raw provider");
        register_archive_provider(ReleaseStream::Edition, DeliveryKind::Site, site_hook)
            .expect("site");
        register_archive_provider(ReleaseStream::Tool, DeliveryKind::Site, site_hook)
            .expect("tool");

        let repo = lab();
        let root = repo.path();
        let work = TempDir::new().expect("work");
        let archives = work.path().join("archives");

        // Tag selection, clean packaging and the published artifact file set.
        let (out1, ev1) = deliver(root, work.path(), "edition-001");
        // A second, independent packaging run of the same tagged commit, kept for the
        // retained-pair comparison at the end of this test.
        let out1_again = work.path().join("out-edition-001-again");
        package(root, &out1_again, &mut Scripted::clean()).expect("second packaging");
        let mut expected = vec![
            "SHA256SUMS".to_owned(),
            "corpus-support.sha256".to_owned(),
            "corpus-support.toml".to_owned(),
            "fieldbook-edition-001.zip".to_owned(),
            "fieldbook-originals-001.zip".to_owned(),
        ];
        expected.sort();
        assert_eq!(dir_listing(&out1), expected);

        // Downloaded-pair verification and local comparison.
        let downloaded = work.path().join("downloaded");
        copy_tree(&out1, &downloaded);
        let compare = |local: &Path, down: &Path| {
            command_raw_edition_compare(&[
                local.to_str().expect("utf8"),
                down.to_str().expect("utf8"),
            ])
        };
        assert_eq!(compare(&out1, &downloaded), Ok(true));
        assert!(command_raw_edition_compare(&[out1.to_str().expect("utf8")]).is_err());
        assert!(command_raw_edition_compare(&["--local", "a", "b"]).is_err());
        let victim = downloaded.join("fieldbook-originals-001.zip");
        let mut bytes = fs::read(&victim).expect("zip");
        bytes[10] ^= 1;
        fs::write(&victim, bytes).expect("flip");
        assert!(compare(&out1, &downloaded).is_err());
        fs::remove_file(downloaded.join("corpus-support.sha256")).expect("rm");
        assert!(compare(&out1, &downloaded).is_err());
        // Two equal sets that are both damaged still fail: the downloaded set must verify.
        let (bad_local, bad_down) = (work.path().join("bad-local"), work.path().join("bad-down"));
        for dir in [&bad_local, &bad_down] {
            copy_tree(&out1, dir);
            fs::write(
                dir.join("corpus-support.sha256"),
                format!("{}  corpus-support.toml\n", "0".repeat(64)),
            )
            .expect("damage");
        }
        let err = compare(&bad_local, &bad_down).expect_err("unverified set");
        assert!(err.contains("does not verify"), "{err}");

        // Durable archival and the first-delivery restore.
        let (arch1, manifest1, _) =
            archive(&out1, &ev1, &base_of(root), &archives).expect("archive 1");
        assert_eq!(manifest1.stream, ReleaseStream::Edition);
        assert_eq!(manifest1.kind, DeliveryKind::Raw);
        assert_eq!(manifest1.tag, "edition-001");
        assert_eq!(manifest1.commit, head(root));
        let paths: Vec<&str> = manifest1.files.iter().map(|f| f.path.as_str()).collect();
        for needed in [
            "fieldbook-edition-001.zip",
            "fieldbook-originals-001.zip",
            "SHA256SUMS",
            "corpus-support.toml",
            "corpus-support.sha256",
            "evidence/edition-release.toml",
            "evidence/release-baseline.toml",
            "evidence/release-identity.toml",
            "evidence/compat/published.toml",
        ] {
            assert!(paths.contains(&needed), "{needed} in {paths:?}");
        }
        let restored1 = restore("1", work.path());
        let report = verify_archive(&arch1, &restored1, None).expect("first delivery verifies");
        assert_eq!(report.verified_predecessor_tag, None);
        assert!(!report.used_first_tool_rehearsal);
        for rel in [
            "current/edition/README.txt",
            "current/edition/TRANSFER-STATEMENT.txt",
            "current/edition/evidence/check.json",
            "current/edition/evidence/regression.json",
            "current/originals/SHA256SUMS",
            "current/delivered/corpus-support.toml",
            "current/delivered/corpus-support.sha256",
        ] {
            assert!(restored1.join(rel).is_file(), "{rel}");
        }

        // Baseline advancement keeps the tool stream empty and retains the checked bytes.
        let b1 = advance_baselines_from_archive(&read_baselines(&base_of(root)), &arch1)
            .expect("advance");
        assert_eq!(
            b1.edition.as_ref().map(|e| e.tag.as_str()),
            Some("edition-001")
        );
        assert_eq!(
            b1.compatibility.as_ref().map(|c| c.tag.as_str()),
            Some("edition-001")
        );
        assert!(b1.tool.is_none());

        // The next edition needs the previous pair; rollback keeps the first pair verifiable.
        write(
            root,
            "corpus/changes.md",
            b"# Corpus changes\n\nSynthetic changes log.\n\n## edition-002\n\n- Added: synthetic.\n\n## edition-001\n\n- Added: synthetic.\n",
        );
        write(root, "edition-release.toml", mode("002", "raw").as_bytes());
        write(
            root,
            "release-baseline.toml",
            serialize_baselines(&b1).as_bytes(),
        );
        commit_all(root);
        git(root, &["tag", "edition-002"]);
        let (out2, ev2) = deliver(root, work.path(), "edition-002");
        let (arch2, _, _) = archive(&out2, &ev2, &base_of(root), &archives).expect("archive 2");
        assert!(
            verify_archive(&arch2, &restore("2-none", work.path()), None).is_err(),
            "a missing previous pair is never an empty baseline"
        );
        assert!(
            verify_archive(&arch2, &restore("2-self", work.path()), Some(&arch2)).is_err(),
            "the wrong previous pair is refused"
        );
        let report2 = verify_archive(&arch2, &restore("2", work.path()), Some(&arch1))
            .expect("with predecessor");
        assert_eq!(
            report2.verified_predecessor_tag.as_deref(),
            Some("edition-001")
        );
        verify_archive(&arch1, &restore("rollback", work.path()), None)
            .expect("rollback pair verifies alone");
        compare_delivered_sets(&out1, &out1_again).expect("two independent runs give one set");

        assert!(
            !SITE_CALLED.load(Ordering::SeqCst),
            "no site provider was invoked"
        );
    }

    #[test]
    fn a_tool_binding_survives_an_intervening_raw_edition() {
        register_raw_provider().expect("raw provider");
        let repo = lab();
        let root = repo.path();
        let work = TempDir::new().expect("work");
        let archives = work.path().join("archives");
        let (out1, ev1) = deliver(root, work.path(), "edition-001");
        let (arch1, _, _) = archive(&out1, &ev1, &base_of(root), &archives).expect("archive");

        // The baseline already holds a retained tool release.
        let prior = parse_baselines(&format!(
            "baseline_schema = 1\n\n[compatibility]\nkind = \"empty\"\n\n[tool]\nkind = \"release\"\ntag = \"v0.1.0\"\ncommit = \"{}\"\narchive_manifest_sha256 = \"{}\"\n\n[edition]\nkind = \"empty\"\n",
            "2".repeat(40),
            "3".repeat(64)
        ))
        .expect("prior");
        let next = advance_baselines_from_archive(&prior, &arch1).expect("advance");
        assert_eq!(
            next.tool, prior.tool,
            "the edition does not replace the tool binding"
        );
        assert_eq!(
            next.edition.as_ref().map(|e| e.tag.as_str()),
            Some("edition-001")
        );
        assert_eq!(
            next.tool_predecessor().map(|t| t.tag.as_str()),
            Some("v0.1.0")
        );
        assert_eq!(
            next.edition_predecessor().map(|t| t.tag.as_str()),
            Some("edition-001")
        );
    }

    #[test]
    fn accepted_baseline_is_protected_by_the_ordinary_compatibility_check() {
        register_raw_provider().expect("raw provider");
        let repo = lab();
        let root = repo.path();
        let work = TempDir::new().expect("work");
        let archives = work.path().join("archives");

        // The ordinary compatibility check, not a stub, accepts the tagged commit.
        let evidence = work.path().join("ev");
        raw_edition_check(root, &check_args("edition-001", &evidence), &|r, b| {
            crate::compat::command(r, b)
        })
        .expect("real compatibility verification");
        let out = work.path().join("out");
        package(root, &out, &mut Scripted::clean()).expect("packaging");
        let (arch, _, _) = archive(&out, &evidence, &base_of(root), &archives).expect("archive");

        let accepted = advance_baselines_from_archive(&read_baselines(&base_of(root)), &arch)
            .expect("advance");
        write(
            root,
            "release-baseline.toml",
            serialize_baselines(&accepted).as_bytes(),
        );
        commit_all(root);
        let accepted_path = base_of(root);
        assert!(
            crate::compat::command(root, &accepted_path).expect("runs"),
            "accepted baseline holds"
        );

        // Changing an accepted binding is caught by the same ordinary check.
        let published = fs::read_to_string(root.join("compat/published.toml")).expect("published");
        let changed = published.replacen("calculation_epoch = ", "calculation_epoch = 9", 1);
        assert_ne!(published, changed);
        fs::write(root.join("compat/published.toml"), changed).expect("edit");
        assert!(!crate::compat::command(root, &accepted_path).expect("runs"));
        fs::write(root.join("compat/published.toml"), &published).expect("restore");
        assert!(crate::compat::command(root, &accepted_path).expect("runs"));

        // A bound vector that no longer matches its archived hash is caught too.
        let bound = parse_published(&published)
            .expect("parses")
            .bound_files()
            .expect("bound");
        let (path, _) = bound
            .iter()
            .find(|(p, _)| p.ends_with(".toml") && p != "compat/published.toml")
            .expect("a vector");
        let original = fs::read(root.join(path)).expect("vector");
        let mut tampered = original.clone();
        tampered.extend_from_slice(b"\n# tampered\n");
        fs::write(root.join(path), tampered).expect("tamper");
        assert!(!crate::compat::command(root, &accepted_path).expect("runs"));
    }

    #[test]
    fn archive_refuses_missing_vectors_changed_hashes_and_bad_sidecars() {
        register_raw_provider().expect("raw provider");
        let repo = lab();
        let root = repo.path();
        let work = TempDir::new().expect("work");
        let archives = work.path().join("archives");
        let (out, evidence) = deliver(root, work.path(), "edition-001");

        // A missing bound vector in the downloaded evidence.
        let published = parse_published(
            &fs::read_to_string(evidence.join("compat/published.toml")).expect("published"),
        )
        .expect("parses");
        let bound = published.bound_files().expect("bound");
        let vector = bound.first().expect("a vector").0.clone();
        let broken = work.path().join("ev-missing");
        copy_tree(&evidence, &broken);
        fs::remove_file(broken.join(&vector)).expect("rm");
        assert!(archive(&out, &broken, &base_of(root), &archives).is_err());

        // A changed bound vector: its bytes no longer match the published manifest.
        let changed = work.path().join("ev-changed");
        copy_tree(&evidence, &changed);
        fs::write(changed.join(&vector), b"changed\n").expect("edit");
        let err = archive(&out, &changed, &base_of(root), &archives).expect_err("changed");
        assert!(err.contains("sha256"), "{err}");

        // A missing or changed support sidecar.
        for name in ["corpus-support.toml", "corpus-support.sha256"] {
            let missing = work.path().join(format!("pair-no-{name}"));
            copy_tree(&out, &missing);
            fs::remove_file(missing.join(name)).expect("rm");
            assert!(
                archive(&missing, &evidence, &base_of(root), &archives).is_err(),
                "{name}"
            );

            let altered = work.path().join(format!("pair-bad-{name}"));
            copy_tree(&out, &altered);
            let mut bytes = fs::read(altered.join(name)).expect("sidecar");
            bytes.extend_from_slice(b"\n");
            fs::write(altered.join(name), bytes).expect("edit");
            assert!(
                archive(&altered, &evidence, &base_of(root), &archives).is_err(),
                "{name}"
            );
        }

        // A digest that names other bytes.
        let wrong = work.path().join("pair-wrong-digest");
        copy_tree(&out, &wrong);
        fs::write(
            wrong.join("corpus-support.sha256"),
            format!("{}  corpus-support.toml\n", "0".repeat(64)),
        )
        .expect("edit");
        assert!(archive(&wrong, &evidence, &base_of(root), &archives).is_err());

        // Evidence whose identity names another tag or another commit is refused by
        // archive-release itself, before the Raw provider runs.
        for (name, tag, commit) in [
            ("tag", "edition-002".to_owned(), head(root)),
            ("commit", "edition-001".to_owned(), "0".repeat(40)),
        ] {
            let other = work.path().join(format!("ev-identity-{name}"));
            copy_tree(&evidence, &other);
            fs::write(
                other.join(RELEASE_IDENTITY_FILE),
                ReleaseIdentity::new(&tag, &commit).to_toml(),
            )
            .expect("edit identity");
            let err = archive(&out, &other, &base_of(root), &archives)
                .expect_err("another identity is refused");
            assert!(
                err.contains("differs from the paired edition"),
                "{name}: {err}"
            );
        }

        // A mismatched mode descriptor (another kind) is refused.
        let site = work.path().join("ev-site");
        copy_tree(&evidence, &site);
        fs::write(site.join("edition-release.toml"), mode("001", "site")).expect("edit");
        assert!(archive(&out, &site, &base_of(root), &archives).is_err());
        assert!(
            !archives.exists()
                || dir_listing(&archives).iter().all(|n| n.starts_with('.'))
                || dir_listing(&archives).is_empty()
        );
    }

    #[test]
    fn changed_archive_bytes_and_unexpected_files_are_refused() {
        register_raw_provider().expect("raw provider");
        let repo = lab();
        let root = repo.path();
        let work = TempDir::new().expect("work");
        let archives = work.path().join("archives");
        let (out, evidence) = deliver(root, work.path(), "edition-001");
        let (arch, _, _) = archive(&out, &evidence, &base_of(root), &archives).expect("archive");
        verify_archive(&arch, &restore("ok", work.path()), None).expect("verifies");

        // Damaged payload bytes in every kind of file.
        for name in [
            "fieldbook-edition-001.zip",
            "fieldbook-originals-001.zip",
            "corpus-support.toml",
            "evidence/release-baseline.toml",
            "evidence/edition-release.toml",
        ] {
            let copy = work.path().join(format!("copy-{}", name.replace('/', "_")));
            copy_tree(&arch, &copy);
            let mut bytes = fs::read(copy.join(name)).expect("file");
            let last = bytes.len() - 1;
            bytes[last] ^= 0x01;
            fs::write(copy.join(name), bytes).expect("damage");
            assert!(
                verify_archive(
                    &copy,
                    &restore(&format!("bad-{}", name.replace('/', "_")), work.path()),
                    None
                )
                .is_err(),
                "{name}"
            );
        }

        // A same-size edit of an archived evidence file that no other check reads: one hex digit
        // of the compatibility case-set digest. Only the per-file comparison against the
        // archive manifest can see it.
        let probe = "evidence/compat/published.toml";
        let probe_copy = work.path().join("copy-probe");
        copy_tree(&arch, &probe_copy);
        let text = fs::read_to_string(probe_copy.join(probe)).expect("published copy");
        let key = "case_set_sha256 = \"";
        let at = text.find(key).expect("the case-set digest") + key.len();
        let mut edited = text.clone().into_bytes();
        edited[at] = if edited[at] == b'0' { b'1' } else { b'0' };
        assert_eq!(edited.len(), text.len());
        fs::write(probe_copy.join(probe), edited).expect("edit");
        assert!(
            verify_archive(&probe_copy, &restore("probe", work.path()), None).is_err(),
            "a same-size edit of an archived evidence file must be refused"
        );

        // An extra file, and a missing file.
        let extra = work.path().join("copy-extra");
        copy_tree(&arch, &extra);
        fs::write(extra.join("site.html"), b"<html></html>").expect("plant");
        assert!(verify_archive(&extra, &restore("extra", work.path()), None).is_err());
        let gone = work.path().join("copy-gone");
        copy_tree(&arch, &gone);
        fs::remove_file(gone.join("corpus-support.sha256")).expect("rm");
        assert!(verify_archive(&gone, &restore("gone", work.path()), None).is_err());
    }

    #[test]
    fn failed_or_missing_clean_reports_in_the_pair_are_refused() {
        register_raw_provider().expect("raw provider");
        let repo = lab();
        let root = repo.path();
        let work = TempDir::new().expect("work");
        let archives = work.path().join("archives");
        let (out, evidence) = deliver(root, work.path(), "edition-001");

        type Edit = fn(&mut BTreeMap<String, Vec<u8>>);
        let cases: [(&str, Edit); 6] = [
            ("missing-check", |f| {
                f.remove("evidence/check.json");
            }),
            ("missing-regression", |f| {
                f.remove("evidence/regression.json");
            }),
            ("errors", |f| {
                f.insert(
                    "evidence/check.json".to_owned(),
                    b"{\"report_version\":1,\"strict\":true,\"errors\":2,\"diagnostics\":[]}\n"
                        .to_vec(),
                );
            }),
            ("regression-failed", |f| {
                f.insert(
                    "evidence/regression.json".to_owned(),
                    b"{\"passed\":false,\"critical_misses\":1}\n".to_vec(),
                );
            }),
            ("foreign-clean-check", |f| {
                // A clean strict report, but not the bytes this regression result names.
                let mut other = f["evidence/check.json"].clone();
                other.insert(0, b' ');
                f.insert("evidence/check.json".to_owned(), other);
            }),
            ("other-commit", |f| {
                let text = String::from_utf8(f["evidence/regression.json"].clone()).expect("utf8");
                let head: String = text
                    .lines()
                    .map(|l| {
                        if l.contains("\"commit\"") {
                            "  \"commit\": \"0000000000000000000000000000000000000000\",".to_owned()
                        } else {
                            l.to_owned()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                f.insert("evidence/regression.json".to_owned(), head.into_bytes());
            }),
        ];
        for (name, edit) in cases {
            let pair = work.path().join(format!("pair-{name}"));
            repack(&out, &pair, edit);
            let to = archives.join(name);
            let err = archive(&pair, &evidence, &base_of(root), &to)
                .expect_err(&format!("{name} must be refused at archival"));
            assert!(err.contains("evidence/"), "{name}: {err}");
            if !name.starts_with("missing-") {
                // The shared Raw allowlist requires both reports; only the Raw provider, run
                // before publication, reads what they say.
                assert!(err.contains("nothing was published"), "{name}: {err}");
            }
            // No immutable folder (and no staging or restore folder) is left behind.
            assert!(
                !to.exists() || dir_listing(&to).is_empty(),
                "{name}: {:?}",
                dir_listing(&to)
            );
        }
        // The repack helper itself is neutral: an unedited repack verifies.
        let same = work.path().join("pair-same");
        repack(&out, &same, |_| {});
        let (dir, _, _) =
            archive(&same, &evidence, &base_of(root), &archives.join("same")).expect("archive");
        verify_archive(&dir, &restore("same", work.path()), None).expect("verifies");
    }

    #[test]
    fn report_checks_accept_only_clean_reports_for_this_edition_and_commit() {
        let commit = "a".repeat(40);
        let check = CHECK_SAMPLE.as_bytes();
        let regression = regression_sample(check, &commit);
        verify_reports(check, regression.as_bytes(), "edition-001", &commit).expect("clean");
        assert!(verify_reports(check, regression.as_bytes(), "edition-002", &commit).is_err());
        assert!(
            verify_reports(check, regression.as_bytes(), "edition-001", &"b".repeat(40)).is_err()
        );
        // A regression failure recorded only as a diagnostic: FB0128 and the FB0126 malformed set.
        for code in ["FB0128", "FB0126"] {
            let bad = CHECK_SAMPLE.replace("FB0127", code);
            let reg = regression_sample(bad.as_bytes(), &commit);
            let err = verify_reports(bad.as_bytes(), reg.as_bytes(), "edition-001", &commit)
                .expect_err(code);
            assert!(err.contains("regression failure"), "{code}: {err}");
        }
        let not_strict = CHECK_SAMPLE.replace("\"strict\":true", "\"strict\":false");
        let reg = regression_sample(not_strict.as_bytes(), &commit);
        assert!(
            verify_reports(
                not_strict.as_bytes(),
                reg.as_bytes(),
                "edition-001",
                &commit
            )
            .is_err()
        );
        assert!(verify_reports(b"x", regression.as_bytes(), "edition-001", &commit).is_err());
        assert!(verify_reports(check, b"x", "edition-001", &commit).is_err());

        // The case counts must be present and consistent.
        let no_critical = regression.replace("\"critical_cases\":0,", "");
        assert_ne!(no_critical, regression);
        assert!(verify_reports(check, no_critical.as_bytes(), "edition-001", &commit).is_err());
        let too_many = regression.replace("\"critical_cases\":0", "\"critical_cases\":28");
        assert!(verify_reports(check, too_many.as_bytes(), "edition-001", &commit).is_err());

        // A clean report from another run is not the one this regression result names.
        let foreign = CHECK_SAMPLE.replace("[{", "[ {");
        let err = verify_reports(
            foreign.as_bytes(),
            regression.as_bytes(),
            "edition-001",
            &commit,
        )
        .expect_err("foreign check report");
        assert!(err.contains("names"), "{err}");
        let other_day = regression_sample(check, &commit).replace("2026-03-02", "2026-03-03");
        assert!(verify_reports(check, other_day.as_bytes(), "edition-001", &commit).is_err());
        let _ = CONTACT;
    }

    const CHECK_SAMPLE: &str = "{\"report_version\":1,\"strict\":true,\"today\":\"2026-03-02\",\"errors\":0,\"diagnostics\":[{\"code\":\"FB0127\"}]}";

    /// A passed regression result for `edition-001` that names `check` by its bytes.
    fn regression_sample(check: &[u8], commit: &str) -> String {
        format!(
            "{{\"passed\":true,\"cases\":27,\"critical_cases\":0,\"critical_misses\":0,\"edition\":\"edition-001\",\"commit\":\"{commit}\",\"today\":\"2026-03-02\",\"check_sha256\":\"{}\"}}",
            sha256_hex(check)
        )
    }

    /// Seeded random bytes and single-byte mutations of valid examples never panic a parser.
    fn never_panics<T>(name: &str, valid: &[&str], reader: impl Fn(&[u8]) -> Result<T, String>) {
        use std::panic::{AssertUnwindSafe, catch_unwind};
        for sample in valid {
            assert!(
                catch_unwind(AssertUnwindSafe(|| reader(sample.as_bytes())))
                    .unwrap_or_else(|_| panic!("`{name}` panicked on its own valid example"))
                    .is_ok(),
                "`{name}` rejected its own valid example"
            );
        }
        let mut gen_ = crate::compat::rng::Rng::new(0xFB05_2026_0001_0007);
        for _ in 0..200 {
            let bytes = gen_.bytes(256);
            assert!(
                catch_unwind(AssertUnwindSafe(|| reader(&bytes))).is_ok(),
                "`{name}` panicked on seeded random bytes"
            );
        }
        for sample in valid {
            let base = sample.as_bytes();
            for idx in 0..base.len() {
                let mut flipped = base.to_vec();
                flipped[idx] ^= 0x5A;
                assert!(
                    catch_unwind(AssertUnwindSafe(|| reader(&flipped))).is_ok(),
                    "`{name}` panicked on a single-byte flip at {idx}"
                );
                let mut deleted = base.to_vec();
                deleted.remove(idx);
                assert!(
                    catch_unwind(AssertUnwindSafe(|| reader(&deleted))).is_ok(),
                    "`{name}` panicked on a single-byte deletion at {idx}"
                );
            }
        }
    }

    #[test]
    fn report_and_command_line_parsers_never_panic_on_seeded_input() {
        let commit = "a".repeat(40);
        let check = CHECK_SAMPLE;
        let regression = regression_sample(check.as_bytes(), &commit);
        // The check report is mutated against a fixed regression result, and the reverse.
        never_panics("verify_reports(check)", &[check], |bytes| {
            verify_reports(bytes, regression.as_bytes(), "edition-001", &commit)
        });
        never_panics(
            "verify_reports(regression)",
            &[regression.as_str()],
            |bytes| verify_reports(check.as_bytes(), bytes, "edition-001", &commit),
        );
        // A mutation that still verifies must keep every field the verifier reads.
        let base = regression.as_bytes();
        for idx in 0..base.len() {
            let mut flipped = base.to_vec();
            flipped[idx] ^= 0x5A;
            if verify_reports(check.as_bytes(), &flipped, "edition-001", &commit).is_ok() {
                let value: serde_json::Value =
                    serde_json::from_slice(&flipped).expect("an accepted report is JSON");
                assert_eq!(value["passed"], true);
                assert_eq!(value["critical_misses"], 0);
                assert_eq!(value["edition"], "edition-001");
                assert_eq!(value["commit"], commit.as_str());
                assert_eq!(value["cases"], 27);
                assert_eq!(value["critical_cases"], 0);
            }
        }
        let line = "edition-001 --base release-baseline.toml --evidence-out out/raw-evidence";
        never_panics("parse_raw_edition_check_cli", &[line], |bytes| {
            let text = String::from_utf8_lossy(bytes).into_owned();
            let words: Vec<&str> = text.split_whitespace().collect();
            parse_raw_edition_check_cli(&words).map(|_| ())
        });
    }

    #[test]
    fn immutable_archive_folder_collision_is_refused_and_left_alone() {
        register_raw_provider().expect("raw provider");
        let repo = lab();
        let root = repo.path();
        let work = TempDir::new().expect("work");
        let archives = work.path().join("archives");
        let (out, evidence) = deliver(root, work.path(), "edition-001");
        let (arch, _, sha) = archive(&out, &evidence, &base_of(root), &archives).expect("first");
        let before = collect_all_rel_files(&arch).expect("files");

        // The identical archive is accepted again.
        let (_, _, again) = archive(&out, &evidence, &base_of(root), &archives).expect("identical");
        assert_eq!(sha, again);

        // When the Raw provider refuses the identical existing folder (here its restore folder
        // cannot be made), the message says the folder was left unchanged, not that nothing
        // was published.
        let folder = arch.file_name().expect("folder").to_string_lossy();
        let blocker = archives.join(format!(".restore-{folder}-{}", std::process::id()));
        fs::write(&blocker, b"not a folder\n").expect("blocker");
        let err = archive(&out, &evidence, &base_of(root), &archives).expect_err("blocked");
        assert!(err.contains("left unchanged"), "{err}");
        assert!(!err.contains("nothing was published"), "{err}");
        assert_eq!(collect_all_rel_files(&arch).expect("files"), before);
        fs::remove_file(&blocker).expect("rm blocker");

        // Different bytes for the same tag and commit are refused: here another evidence
        // baseline, given consistently as `--base` too.
        let other_evidence = work.path().join("ev-other");
        copy_tree(&evidence, &other_evidence);
        let other_base = other_evidence.join("release-baseline.toml");
        fs::write(&other_base, format!("{EMPTY_BASE}# another descriptor\n")).expect("write");
        let err = archive(&out, &other_evidence, &other_base, &archives).expect_err("collision");
        assert!(err.contains("immutable"), "{err}");
        assert_eq!(collect_all_rel_files(&arch).expect("files"), before);
    }

    #[test]
    fn a_base_that_is_not_the_committed_evidence_baseline_is_refused_before_archival() {
        register_raw_provider().expect("raw provider");
        let repo = lab();
        let root = repo.path();
        let work = TempDir::new().expect("work");

        // edition-002 commits a baseline that binds edition-001; a stale local `--base` that
        // still says `empty` would turn the predecessor check off.
        let archives = work.path().join("archives");
        let (out1, ev1) = deliver(root, work.path(), "edition-001");
        let (arch1, _, _) = archive(&out1, &ev1, &base_of(root), &archives).expect("archive 1");
        let b1 = advance_baselines_from_archive(&read_baselines(&base_of(root)), &arch1)
            .expect("advance");
        write(
            root,
            "corpus/changes.md",
            b"# Corpus changes\n\nSynthetic changes log.\n\n## edition-002\n\n- Added: synthetic.\n\n## edition-001\n\n- Added: synthetic.\n",
        );
        write(root, "edition-release.toml", mode("002", "raw").as_bytes());
        write(
            root,
            "release-baseline.toml",
            serialize_baselines(&b1).as_bytes(),
        );
        commit_all(root);
        git(root, &["tag", "edition-002"]);
        let (out2, ev2) = deliver(root, work.path(), "edition-002");

        let stale = work.path().join("stale-baseline.toml");
        fs::write(&stale, EMPTY_BASE).expect("stale");
        let fresh = work.path().join("fresh-archives");
        let err = archive(&out2, &ev2, &stale, &fresh).expect_err("stale --base");
        assert!(err.contains("differs from the committed"), "{err}");
        assert!(!fresh.exists() || dir_listing(&fresh).is_empty());

        // The evidence copy is required even when `--base` is given.
        let no_base = work.path().join("ev-no-base");
        copy_tree(&ev2, &no_base);
        fs::remove_file(no_base.join("release-baseline.toml")).expect("rm");
        let err = archive(&out2, &no_base, &base_of(root), &fresh).expect_err("no evidence copy");
        assert!(err.contains("release-baseline.toml"), "{err}");

        // The committed copy archives, and its predecessor binding holds.
        let (arch2, _, _) = archive(&out2, &ev2, &base_of(root), &fresh).expect("archive 2");
        assert!(verify_archive(&arch2, &restore("2-none", work.path()), None).is_err());
    }

    #[test]
    fn archive_release_refuses_an_intake_folder_for_a_raw_archive() {
        register_raw_provider().expect("raw provider");
        let repo = lab();
        let root = repo.path();
        let work = TempDir::new().expect("work");
        let (out, evidence) = deliver(root, work.path(), "edition-001");
        let intake = work.path().join("intake");
        write(&intake, "x.html", b"<p>x</p>\n");
        let to = work.path().join("archives");
        let err = archive_release(&ArchiveArgs {
            kind: Some(DeliveryKind::Raw),
            package: None,
            edition: out,
            intake: Some(intake),
            evidence: Some(evidence),
            base: None,
            to: to.clone(),
        })
        .expect_err("intake");
        assert!(err.contains("not part of a raw archive"), "{err}");
        assert!(!to.exists());
    }

    /// Copy an archive, apply `edit`, and rewrite `archive.toml` so that every row names the
    /// true bytes. Only the Raw provider can then refuse the edited archive.
    fn relisted(arch: &Path, to: &Path, edit: impl FnOnce(&Path)) {
        copy_tree(arch, to);
        edit(to);
        let mut manifest =
            ArchiveManifest::parse(&fs::read_to_string(to.join("archive.toml")).expect("toml"))
                .expect("manifest");
        manifest.files = collect_all_rel_files(to)
            .expect("files")
            .into_iter()
            .filter(|(p, _)| p != "archive.toml")
            .map(|(path, bytes)| ArchiveFileEntry {
                path,
                size: bytes.len() as u64,
                sha256: sha256_hex(&bytes),
            })
            .collect();
        fs::write(to.join("archive.toml"), manifest.to_toml()).expect("relist");
    }

    #[test]
    fn the_raw_provider_refuses_extra_listed_files_an_inexact_evidence_set_and_another_kind() {
        register_raw_provider().expect("raw provider");
        let repo = lab();
        let root = repo.path();
        let work = TempDir::new().expect("work");
        let archives = work.path().join("archives");
        let (out, evidence) = deliver(root, work.path(), "edition-001");
        let (arch, _, _) = archive(&out, &evidence, &base_of(root), &archives).expect("archive");

        // The helper is neutral: an unedited, re-listed copy verifies.
        let same = work.path().join("same");
        relisted(&arch, &same, |_| {});
        verify_archive(&same, &restore("same", work.path()), None).expect("neutral");

        type Plant = fn(&Path);
        let cases: [(&str, Plant, &str); 4] = [
            (
                "site",
                |d| write(d, "site.html", b"<html></html>\n"),
                "is not part of a raw archive",
            ),
            (
                "intake",
                |d| write(d, "intake/x.html", b"<p>x</p>\n"),
                "is not part of a raw archive",
            ),
            (
                "extra-evidence",
                |d| write(d, "evidence/extra.toml", b"x = 1\n"),
                "evidence set is not exact",
            ),
            (
                "site-mode",
                |d| {
                    write(
                        d,
                        "evidence/edition-release.toml",
                        mode("001", "site").as_bytes(),
                    );
                },
                "does not select `raw`",
            ),
        ];
        for (name, plant, expected) in cases {
            let copy = work.path().join(format!("copy-{name}"));
            relisted(&arch, &copy, plant);
            let err = verify_archive(&copy, &restore(name, work.path()), None)
                .expect_err(&format!("{name} must be refused"));
            assert!(err.contains(expected), "{name}: {err}");
        }
    }

    #[test]
    fn the_raw_provider_refuses_a_retained_identity_of_another_tag_or_commit() {
        register_raw_provider().expect("raw provider");
        let repo = lab();
        let root = repo.path();
        let work = TempDir::new().expect("work");
        let archives = work.path().join("archives");
        let (out, evidence) = deliver(root, work.path(), "edition-001");
        let (arch, _, _) = archive(&out, &evidence, &base_of(root), &archives).expect("archive");
        let rel = format!("evidence/{RELEASE_IDENTITY_FILE}");
        assert_eq!(
            fs::read(arch.join(&rel)).expect("identity"),
            ReleaseIdentity::new("edition-001", &head(root))
                .to_toml()
                .as_bytes()
        );

        for (name, tag, commit) in [
            ("tag", "edition-002".to_owned(), head(root)),
            ("commit", "edition-001".to_owned(), "0".repeat(40)),
        ] {
            let copy = work.path().join(format!("copy-identity-{name}"));
            relisted(&arch, &copy, |d| {
                write(
                    d,
                    &rel,
                    ReleaseIdentity::new(&tag, &commit).to_toml().as_bytes(),
                );
            });
            let err = verify_archive(
                &copy,
                &restore(&format!("identity-{name}"), work.path()),
                None,
            )
            .expect_err("another identity is refused");
            assert!(err.contains("not the archived pair"), "{name}: {err}");
        }
    }

    const WORKFLOW: &str = include_str!("../../.github/workflows/raw-edition.yml");

    fn workflow_code() -> String {
        WORKFLOW
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn position(haystack: &str, needle: &str) -> usize {
        haystack
            .find(needle)
            .unwrap_or_else(|| panic!("workflow lacks `{needle}`"))
    }

    /// The text of one job, from its `  name:` line to the next job at the same indentation.
    fn job_section<'a>(code: &'a str, job: &str) -> &'a str {
        let start = position(code, &format!("\n  {job}:\n")) + 1;
        let rest = &code[start..];
        let end = rest
            .match_indices('\n')
            .map(|(i, _)| i + 1)
            .find(|&i| {
                let line = &rest[i..];
                line.starts_with("  ")
                    && !line.starts_with("   ")
                    && line
                        .split('\n')
                        .next()
                        .is_some_and(|l| l.trim_end().ends_with(':'))
            })
            .unwrap_or(rest.len());
        &rest[..end]
    }

    /// A structural YAML check with no parser crate: no tab, two-space indentation, and every
    /// single-line `key: value` either a block indicator, a quoted string, a flow collection, or
    /// a plain value that YAML cannot read as a nested mapping (no `: ` inside, no trailing `:`).
    /// Lines inside a block scalar (`|` or `>`) are skipped.
    fn yaml_shape_errors(text: &str) -> Vec<String> {
        let mut errors = Vec::new();
        let mut block_indent: Option<usize> = None;
        for (n, line) in text.lines().enumerate() {
            let n = n + 1;
            if line.contains('\t') {
                errors.push(format!("line {n}: tab"));
            }
            let indent = line.len() - line.trim_start().len();
            if let Some(b) = block_indent {
                if line.trim().is_empty() || indent > b {
                    continue;
                }
                block_indent = None;
            }
            let trimmed = line.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if indent % 2 != 0 {
                errors.push(format!("line {n}: odd indentation"));
            }
            let item = trimmed.strip_prefix("- ").unwrap_or(trimmed);
            let Some((key, value)) = item.split_once(':') else {
                continue;
            };
            if !key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            {
                continue;
            }
            if !value.is_empty() && !value.starts_with(' ') {
                continue;
            }
            // Drop a trailing comment (` #`) that sits outside quotes.
            let value = value.trim();
            let value = match value.find(" #") {
                Some(i) if !value.starts_with('"') && !value.starts_with('\'') => value[..i].trim(),
                _ => value,
            };
            if value.is_empty() {
                continue;
            }
            if value.starts_with('|') || value.starts_with('>') {
                block_indent = Some(indent);
                continue;
            }
            if let Some(q) = value.chars().next().filter(|c| *c == '"' || *c == '\'') {
                if value.len() < 2 || !value.ends_with(q) {
                    errors.push(format!("line {n}: unterminated quoted value"));
                }
                continue;
            }
            if value.starts_with('[') || value.starts_with('{') {
                if !(value.ends_with(']') || value.ends_with('}')) {
                    errors.push(format!("line {n}: unterminated flow value"));
                }
                continue;
            }
            if value.contains(": ") || value.ends_with(':') {
                errors.push(format!(
                    "line {n}: plain value `{value}` reads as a mapping; quote it or use `|`"
                ));
            }
        }
        errors
    }

    #[test]
    fn workflow_is_well_formed_yaml_in_shape() {
        assert_eq!(yaml_shape_errors(WORKFLOW), Vec::<String>::new());
        // The check catches the error GitHub rejected: `::` then a space in a plain `run:` value.
        let bad = "jobs:\n  trial:\n    steps:\n      - name: x\n        run: cargo nextest run --locked -p xtask edition_raw:: raw_release::\n";
        assert_eq!(
            yaml_shape_errors(bad).len(),
            1,
            "{:?}",
            yaml_shape_errors(bad)
        );
        // The same command in a block scalar or a quoted string is fine.
        let block = "      - run: |\n          cargo nextest run edition_raw:: raw_release::\n";
        assert!(yaml_shape_errors(block).is_empty());
        let quoted = "      - run: 'cargo nextest run edition_raw:: raw_release::'\n";
        assert!(yaml_shape_errors(quoted).is_empty());
        assert!(
            yaml_shape_errors("a:\n\tb: c\n")
                .iter()
                .any(|e| e.contains("tab"))
        );
    }

    #[test]
    fn workflow_pins_actions_limits_permissions_and_orders_the_commands() {
        let code = workflow_code();

        // Every action is pinned by a full commit SHA, wherever its `uses:` key sits.
        let mut uses = 0;
        for line in code.lines().filter(|l| {
            let t = l.trim_start();
            t.strip_prefix("- ").unwrap_or(t).starts_with("uses:")
        }) {
            uses += 1;
            let reference = line.split('@').nth(1).expect("an @ reference");
            let sha = reference.split_whitespace().next().expect("a ref");
            assert!(
                sha.len() == 40
                    && sha
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "{line}"
            );
        }
        assert!(uses >= 6);

        // Read-only token, no other permission key, bounded jobs.
        assert_eq!(code.matches("permissions:").count(), 1);
        assert!(code.contains("permissions:\n  contents: read\n"));
        assert!(!code.contains(": write"));
        let jobs = ["select", "trial", "deliver"];
        for job in jobs {
            assert!(code.contains(&format!("\n  {job}:\n")), "{job}");
        }
        assert_eq!(code.matches("timeout-minutes:").count(), jobs.len());

        // Full history without persisted credentials at every checkout.
        let checkouts = code.matches("actions/checkout@").count();
        assert_eq!(code.matches("fetch-depth: 0").count(), checkouts);
        assert_eq!(
            code.matches("persist-credentials: false").count(),
            checkouts
        );

        // Triggers: edition tags, its own pull request, manual runs. Only a raw tag delivers.
        assert!(code.contains("tags: [\"edition-*\"]"));
        assert!(code.contains("pull_request:"));
        assert!(code.contains("workflow_dispatch:"));
        assert!(code.contains("grep -qx 'kind = \"raw\"'"));
        assert!(code.contains("git show \"$GITHUB_SHA:edition-release.toml\""));
        // Any other mention of raw, in any case, fails the run instead of skipping delivery.
        let select = job_section(&code, "select");
        assert!(
            select.contains("elif grep -qi 'raw' <<<\"$mode\"; then"),
            "{select}"
        );
        assert!(position(select, "elif grep -qi 'raw'") < position(select, "exit 1"));

        // Each job's own condition, dependency and time limit, read from that job alone.
        for (job, condition, needs, minutes) in [
            ("select", None, None, 10),
            (
                "trial",
                Some("if: ${{ github.event_name != 'push' }}"),
                Some("needs: [select]"),
                30,
            ),
            (
                "deliver",
                Some("if: ${{ needs.select.outputs.raw == 'true' }}"),
                Some("needs: [select]"),
                60,
            ),
        ] {
            let section = job_section(&code, job);
            let header: Vec<&str> = section
                .lines()
                .skip(1)
                .take_while(|l| !l.starts_with("    steps:"))
                .collect();
            assert!(
                header.contains(&"    runs-on: ubuntu-24.04"),
                "{job}: {header:?}"
            );
            assert!(
                header.contains(&format!("    timeout-minutes: {minutes}").as_str()),
                "{job}: {header:?}"
            );
            match condition {
                Some(c) => assert!(header.contains(&format!("    {c}").as_str()), "{job}"),
                None => assert!(!header.iter().any(|l| l.starts_with("    if:")), "{job}"),
            }
            match needs {
                Some(n) => assert!(header.contains(&format!("    {n}").as_str()), "{job}"),
                None => assert!(!header.iter().any(|l| l.starts_with("    needs:")), "{job}"),
            }
        }

        // Command order inside the delivery job: tag check, then packaging, then the uploads.
        let deliver = job_section(&code, "deliver");
        let check = position(
            deliver,
            "cargo xtask raw-edition-check \"$EDITION_TAG\" --base release-baseline.toml --evidence-out out/raw-evidence",
        );
        let package = position(deliver, "cargo xtask edition-raw --out out/dist");
        let upload = position(deliver, "actions/upload-artifact@");
        let evidence_upload = position(deliver, "-raw-evidence");
        assert!(check < package && package < upload && upload < evidence_upload);
        assert!(
            position(
                deliver,
                "cargo build --locked --release --package fieldbook"
            ) < check
        );

        // Exactly the pair, the external sums and the two support sidecars, then the evidence.
        for file in [
            "fieldbook-edition-${{ steps.names.outputs.number }}.zip",
            "fieldbook-originals-${{ steps.names.outputs.number }}.zip",
            "out/dist/SHA256SUMS",
            "out/dist/corpus-support.toml",
            "out/dist/corpus-support.sha256",
        ] {
            assert!(deliver.contains(file), "{file}");
        }
        assert!(
            deliver.contains("name: fieldbook-edition-${{ steps.names.outputs.number }}-raw\n")
        );
        assert!(deliver.contains("path: out/raw-evidence"));
        assert_eq!(deliver.matches("if-no-files-found: error").count(), 2);
    }

    #[test]
    fn workflow_uses_no_secret_and_calls_no_site_or_browser() {
        let code = workflow_code();
        // The security contact binding is an owner item; no secret is referenced in this file.
        assert!(!code.contains("secrets."), "no secret reference");
        assert!(!code.contains("${{ vars."), "no variable reference");
        assert!(code.contains("permissions:\n  contents: read"));
        assert_eq!(code.matches("permissions:").count(), 1);
        position(&code, "cargo xtask edition-raw --out out/dist");

        for banned in [
            concat!("field", "book build"),
            "cargo xtask edition ",
            "release-check",
            "playwright",
            "msedge",
            "chrome",
            "site",
            "GITHUB_TOKEN",
            "sign",
            "gh release",
            "git push",
            "git tag",
        ] {
            assert!(
                !code.contains(banned),
                "workflow must not contain `{banned}`"
            );
        }
    }

    /// One archived edition-001 and its manifest, for calling the Raw provider directly.
    fn archived_manifest(work: &Path) -> (TempDir, PathBuf, ArchiveManifest) {
        register_raw_provider().expect("raw provider");
        let repo = lab();
        let root = repo.path();
        let (out, evidence) = deliver(root, work, "edition-001");
        let (arch, manifest, _) =
            archive(&out, &evidence, &base_of(root), &work.join("archives")).expect("archive");
        (repo, arch, manifest)
    }

    #[test]
    fn raw_provider_refuses_a_manifest_of_another_stream_or_kind() {
        let work = TempDir::new().expect("work");
        let (_repo, arch, manifest) = archived_manifest(work.path());
        raw_archive_provider(&arch, &restore("ok", work.path()), &manifest)
            .expect("the archived manifest is accepted");
        for (name, stream, kind) in [
            ("site", ReleaseStream::Edition, DeliveryKind::Site),
            ("tool", ReleaseStream::Tool, DeliveryKind::Raw),
        ] {
            let mut other = manifest.clone();
            other.stream = stream;
            other.kind = kind;
            let err =
                raw_archive_provider(&arch, &restore(name, work.path()), &other).expect_err(name);
            assert!(err.contains("verifies only"), "{name}: {err}");
        }
    }

    #[test]
    fn raw_provider_refuses_a_manifest_whose_tag_or_commit_is_not_the_pair() {
        let work = TempDir::new().expect("work");
        let (_repo, arch, manifest) = archived_manifest(work.path());
        let mut other_tag = manifest.clone();
        other_tag.tag = "edition-002".to_owned();
        let mut other_commit = manifest.clone();
        other_commit.commit = "0".repeat(40);
        for (name, other) in [("tag", other_tag), ("commit", other_commit)] {
            let err =
                raw_archive_provider(&arch, &restore(name, work.path()), &other).expect_err(name);
            assert!(
                err.contains("does not carry the archive's tag and commit"),
                "{name}: {err}"
            );
        }
    }

    #[test]
    fn raw_provider_refuses_a_manifest_missing_a_fixed_file() {
        let work = TempDir::new().expect("work");
        let (_repo, arch, manifest) = archived_manifest(work.path());
        for fixed in ["SHA256SUMS", "corpus-support.toml", "corpus-support.sha256"] {
            let mut other = manifest.clone();
            other.files.retain(|f| f.path != fixed);
            assert_eq!(other.files.len() + 1, manifest.files.len(), "{fixed}");
            let err =
                raw_archive_provider(&arch, &restore(fixed, work.path()), &other).expect_err(fixed);
            assert!(
                err.contains(&format!("missing `{fixed}`")),
                "{fixed}: {err}"
            );
        }
    }

    #[test]
    fn raw_provider_refuses_restored_trees_that_fail_the_raw_allowlist() {
        let work = TempDir::new().expect("work");
        let (_repo, arch, manifest) = archived_manifest(work.path());
        // A stray file already in the restore folder lands in the restored edition tree.
        let target = restore("stray", work.path());
        fs::create_dir_all(target.join("edition")).expect("mkdir");
        fs::write(
            target.join("edition/stray.exe"),
            b"not part of a raw edition\n",
        )
        .expect("stray");
        let err = raw_archive_provider(&arch, &target, &manifest).expect_err("stray file");
        assert!(
            err.contains("restored raw trees fail the allowlist"),
            "{err}"
        );
    }

    /// The select step's shell script, as the workflow runs it.
    fn select_script() -> String {
        let step = &WORKFLOW[position(
            WORKFLOW,
            "- name: Read the delivery kind committed at the tagged commit",
        )..];
        let body = &step[position(step, "run: |\n") + "run: |\n".len()..];
        body.lines()
            .take_while(|l| l.is_empty() || l.starts_with("          "))
            .map(|l| l.get(10..).unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Run the select step for `descriptor` (None: no file) at a pushed tag; returns the exit
    /// success and the `raw=` output line.
    fn run_select(repo: &Path, descriptor: Option<&str>, event: &str) -> (bool, String) {
        let file = repo.join("edition-release.toml");
        match descriptor {
            Some(text) => fs::write(&file, text).expect("descriptor"),
            None => {
                if file.exists() {
                    fs::remove_file(&file).expect("rm descriptor");
                }
            }
        }
        let n = fs::read_dir(repo).expect("ls").count();
        fs::write(repo.join("step.txt"), format!("{n} {descriptor:?}\n")).expect("step");
        commit_all(repo);
        let sha = proc::git(repo, &["rev-parse", "HEAD"]).expect("sha");
        let output = repo.join(".git/step-output");
        let _ = fs::remove_file(&output);
        let run = std::process::Command::new("bash")
            .args(["-c", &select_script()])
            .env("EVENT_NAME", event)
            .env("REF_TYPE", "tag")
            .env("GITHUB_SHA", sha.trim())
            .env("GITHUB_OUTPUT", &output)
            .current_dir(repo)
            .output()
            .expect("bash");
        let raw = fs::read_to_string(&output).unwrap_or_default();
        (run.status.success(), raw.trim().to_owned())
    }

    #[test]
    fn select_step_delivers_the_exact_raw_line_and_fails_any_other_mention_of_raw() {
        let repo = TempDir::new().expect("repo");
        let root = repo.path();
        git(root, &["init", "-q"]);
        assert_eq!(
            run_select(root, None, "push"),
            (true, "raw=false".to_owned())
        );
        let exact = mode("001", "raw");
        assert_eq!(
            run_select(root, Some(&exact), "push"),
            (true, "raw=true".to_owned())
        );
        assert_eq!(
            run_select(root, Some(&mode("001", "site")), "push"),
            (true, "raw=false".to_owned())
        );
        // A pull request or manual run never delivers.
        assert_eq!(
            run_select(root, Some(&exact), "pull_request"),
            (true, "raw=false".to_owned())
        );
        for other in [
            "release_schema = 1\nedition = \"001\"\nkind = 'raw'\n",
            "release_schema = 1\nedition = \"001\"\nkind=\"raw\"\n",
            "release_schema = 1\nedition = \"001\"\nkind = \"RAW\"\n",
            "release_schema = 1\nedition = \"001\"\n  kind = \"Raw\" # raw floor\n",
            "release_schema = 1\nedition = \"001\"\nkind = raw\n",
            "release_schema = 1\nedition = \"001\"\nkind = \"site\" # was Raw\n",
        ] {
            let (ok, raw) = run_select(root, Some(other), "push");
            assert!(!ok, "must fail, not skip: {other:?}");
            assert!(raw.is_empty(), "no output on failure: {other:?} {raw}");
        }
    }
}
