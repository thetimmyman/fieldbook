//! The Site edition and tool package archive providers (`docs/runbooks/release.md`).
//!
//! [`site_archive_provider`] verifies a Site edition archive: reopening the paired zips,
//! checking the exact file set, verifying browser network silence evidence, allowlist checking
//! the restored trees, and verifying public compatibility evidence.
//! [`tool_archive_provider`] verifies a tool package archive: checking the site pair and evidence,
//! validating the packaged executable and descriptor, restoring the package, and executing
//! the smoke rehearsal.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use fieldbook_core::hash::sha256_hex;
use serde::Deserialize;

use crate::allowlist::{AllowlistProfile, parse_sha256sums, verify_staged_trees};
use crate::compat::{parse_baselines, parse_published};
use crate::edition::{REQUIRED_FILES, parse_summary};
use crate::edition_release::{
    ArchiveManifest, DeliveryKind, ReleaseStream, VerifiedEditionArtifacts,
    parse_edition_release_mode, read_non_symlink_file, register_archive_provider,
    verify_edition_artifact_set,
};
use crate::proc;
use crate::zipdet::read_and_verify_deterministic_zip;

/// The files a site edition delivery holds beside `archive.toml` and `evidence/`.
const DELIVERED_SIDECARS: [&str; 3] =
    ["SHA256SUMS", "corpus-support.toml", "corpus-support.sha256"];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolPackageDescriptor {
    tag: String,
    commit: String,
}

/// Register the Site edition and tool package payload verifiers with the shared archive framework.
pub fn register_release_providers() -> Result<(), String> {
    register_archive_provider(
        ReleaseStream::Edition,
        DeliveryKind::Site,
        site_archive_provider,
    )?;
    register_archive_provider(
        ReleaseStream::Tool,
        DeliveryKind::Site,
        tool_archive_provider,
    )?;
    Ok(())
}

/// Verify the site pair, public evidence set, browser silence attestation, and restored trees
/// for both Site edition and tool package archives.
fn verify_site_pair_and_evidence(
    archive_dir: &Path,
    restore_dir: &Path,
    manifest: &ArchiveManifest,
    allowed_extra_prefixes: &[&str],
) -> Result<VerifiedEditionArtifacts, String> {
    // 1. Verify edition artifact set (DeliveryKind::Site).
    let artifacts = verify_edition_artifact_set(archive_dir, DeliveryKind::Site)?;
    if manifest.stream == ReleaseStream::Edition {
        if artifacts.ed_tag != manifest.tag || artifacts.ed_info.commit != manifest.commit {
            return Err("the archived pair does not carry the archive's tag and commit".to_owned());
        }
    } else if artifacts.ed_info.commit != manifest.commit {
        return Err("the archived pair does not carry the archive's commit".to_owned());
    }

    // 2. Exact provider file set: pair zips, archive.toml, SHA256SUMS, corpus-support.toml,
    // corpus-support.sha256, files under evidence/, plus allowed extra prefixes.
    let allowed_fixed: BTreeSet<String> = [
        artifacts.ed_zip_name.clone(),
        artifacts.orig_zip_name.clone(),
        "archive.toml".to_owned(),
    ]
    .into_iter()
    .chain(DELIVERED_SIDECARS.iter().map(|s| (*s).to_owned()))
    .collect();

    for entry in &manifest.files {
        let ok = allowed_fixed.contains(&entry.path)
            || entry.path.starts_with("evidence/")
            || allowed_extra_prefixes
                .iter()
                .any(|prefix| entry.path.starts_with(prefix));
        if !ok {
            let msg = if manifest.stream == ReleaseStream::Tool {
                format!("`{}` is not part of a tool archive", entry.path)
            } else {
                format!("`{}` is not part of a site edition archive", entry.path)
            };
            return Err(msg);
        }
    }

    for fixed in &allowed_fixed {
        if fixed != "archive.toml" && !manifest.files.iter().any(|f| &f.path == fixed) {
            return Err(format!("site archive is missing `{fixed}`"));
        }
    }

    // 3. Evidence set exact: release-baseline.toml, compat/published.toml, bound files,
    // and optional edition-release.toml.
    let published_bytes = read_non_symlink_file(archive_dir, "evidence/compat/published.toml")?;
    let published = parse_published(
        std::str::from_utf8(&published_bytes)
            .map_err(|e| format!("evidence/compat/published.toml is not UTF-8: {e}"))?,
    )?;
    let bound = published.bound_files()?;

    let mut expected_evidence: BTreeSet<String> =
        bound.iter().map(|(p, _)| format!("evidence/{p}")).collect();
    expected_evidence.insert("evidence/compat/published.toml".to_owned());
    expected_evidence.insert("evidence/release-baseline.toml".to_owned());

    let actual_evidence: BTreeSet<String> = manifest
        .files
        .iter()
        .filter(|f| f.path.starts_with("evidence/"))
        .map(|f| f.path.clone())
        .collect();

    let ed_num = artifacts
        .ed_tag
        .strip_prefix("edition-")
        .ok_or_else(|| format!("invalid edition tag `{}`", artifacts.ed_tag))?;

    if actual_evidence.contains("evidence/edition-release.toml") {
        expected_evidence.insert("evidence/edition-release.toml".to_owned());
        let mode_bytes = read_non_symlink_file(archive_dir, "evidence/edition-release.toml")?;
        let mode_text = std::str::from_utf8(&mode_bytes)
            .map_err(|e| format!("evidence/edition-release.toml is not UTF-8: {e}"))?;
        let mode = parse_edition_release_mode(Some(mode_text), Some(ed_num))?;
        if mode.kind != DeliveryKind::Site {
            return Err("archived mode descriptor does not select `site`".to_owned());
        }
    }

    if actual_evidence != expected_evidence {
        let missing: Vec<&String> = expected_evidence.difference(&actual_evidence).collect();
        let extra: Vec<&String> = actual_evidence.difference(&expected_evidence).collect();
        return Err(format!(
            "evidence set is not exact (missing {missing:?}, unexpected {extra:?})"
        ));
    }

    for (p, expected_sha) in &bound {
        let rel_path = format!("evidence/{p}");
        let bytes = read_non_symlink_file(archive_dir, &rel_path)?;
        let actual_sha = sha256_hex(&bytes);
        if &actual_sha != expected_sha {
            return Err(format!(
                "bound compatibility file `{rel_path}` sha256 `{actual_sha}` differs from declared `{expected_sha}`"
            ));
        }
    }

    let baseline_bytes = read_non_symlink_file(archive_dir, "evidence/release-baseline.toml")?;
    let baseline_text = std::str::from_utf8(&baseline_bytes)
        .map_err(|e| format!("evidence/release-baseline.toml is not UTF-8: {e}"))?;
    parse_baselines(baseline_text)?;
    let actual_baseline_sha = sha256_hex(&baseline_bytes);
    if actual_baseline_sha != manifest.baseline_sha256 {
        return Err(format!(
            "evidence/release-baseline.toml sha256 `{actual_baseline_sha}` differs from manifest baseline_sha256 `{}`",
            manifest.baseline_sha256
        ));
    }

    // 4. Browser evidence: network-silence-summary.txt inside edition zip.
    let ed_zip = read_non_symlink_file(archive_dir, &artifacts.ed_zip_name)?;
    let ed_entries = read_and_verify_deterministic_zip(&ed_zip, "edition", None)?;
    let summary_bytes = ed_entries
        .iter()
        .find(|(p, _)| p == "intake/network-silence-summary.txt")
        .map(|(_, b)| b)
        .ok_or_else(|| "edition zip is missing `intake/network-silence-summary.txt`".to_owned())?;
    let summary_text = std::str::from_utf8(summary_bytes)
        .map_err(|e| format!("intake/network-silence-summary.txt is not UTF-8: {e}"))?;
    parse_summary(summary_text, &artifacts.ed_tag)?;

    // 5. Restore both trees and delivered sidecars, allowlist-check, verify non-empty index.html.
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
        AllowlistProfile::Site,
        &restore_dir.join("edition"),
        &restore_dir.join("originals"),
        REQUIRED_FILES,
    )
    .map_err(|e| format!("restored site trees fail the allowlist: {e}"))?;

    let index_html = fs::read_to_string(restore_dir.join("edition/index.html"))
        .map_err(|e| format!("cannot read restored `edition/index.html`: {e}"))?;
    if index_html.trim().is_empty() {
        return Err("restored `edition/index.html` is empty".to_owned());
    }

    Ok(artifacts)
}

/// Payload verifier and restore inspector for a site edition archive (`stream = "edition"`,
/// `kind = "site"`).
pub fn site_archive_provider(
    archive_dir: &Path,
    restore_dir: &Path,
    manifest: &ArchiveManifest,
) -> Result<(), String> {
    if manifest.stream != ReleaseStream::Edition || manifest.kind != DeliveryKind::Site {
        return Err("the site provider verifies only `edition` archives of kind `site`".to_owned());
    }
    verify_site_pair_and_evidence(archive_dir, restore_dir, manifest, &[])?;
    Ok(())
}

fn version_from_tag(tag: &str) -> &str {
    tag.strip_prefix('v').unwrap_or(tag)
}

fn expected_smoke_version_line(tag: &str, commit: &str) -> String {
    format!(
        "{} {} ({})",
        fieldbook_core::strings::binary(),
        version_from_tag(tag),
        commit
    )
}

/// Payload verifier and restore inspector for a tool package archive (`stream = "tool"`,
/// `kind = "site"`).
pub fn tool_archive_provider(
    archive_dir: &Path,
    restore_dir: &Path,
    manifest: &ArchiveManifest,
) -> Result<(), String> {
    if manifest.stream != ReleaseStream::Tool || manifest.kind != DeliveryKind::Site {
        return Err("the tool provider verifies only `tool` archives of kind `site`".to_owned());
    }
    verify_site_pair_and_evidence(archive_dir, restore_dir, manifest, &["package/", "intake/"])?;

    // a. Check package/package.toml, package/fieldbook, package/SHA256SUMS.
    for required in [
        "package/package.toml",
        "package/fieldbook",
        "package/SHA256SUMS",
    ] {
        if !manifest.files.iter().any(|f| f.path == required) {
            return Err(format!("tool archive is missing `{required}`"));
        }
    }

    let pkg_toml_bytes = read_non_symlink_file(archive_dir, "package/package.toml")?;
    let pkg_toml_str = std::str::from_utf8(&pkg_toml_bytes)
        .map_err(|e| format!("package/package.toml is not UTF-8: {e}"))?;
    let desc: ToolPackageDescriptor =
        toml::from_str(pkg_toml_str).map_err(|e| format!("invalid package/package.toml: {e}"))?;
    if desc.tag != manifest.tag || desc.commit != manifest.commit {
        return Err(format!(
            "package/package.toml tag `{}` and commit `{}` do not match manifest (`{}`, `{}`)",
            desc.tag, desc.commit, manifest.tag, manifest.commit
        ));
    }

    let sums_bytes = read_non_symlink_file(archive_dir, "package/SHA256SUMS")?;
    let sums_str = std::str::from_utf8(&sums_bytes)
        .map_err(|e| format!("package/SHA256SUMS is not UTF-8: {e}"))?;
    let sums_map = parse_sha256sums(sums_str, "package/SHA256SUMS").map_err(|e| e.to_string())?;

    if !sums_map.contains_key(fieldbook_core::strings::binary()) {
        return Err("package/SHA256SUMS does not list `fieldbook`".to_owned());
    }

    for (name, expected_sha) in &sums_map {
        let rel_path = format!("package/{name}");
        let file_bytes = read_non_symlink_file(archive_dir, &rel_path)
            .map_err(|e| format!("package file `{name}` in package/SHA256SUMS is missing: {e}"))?;
        let actual_sha = sha256_hex(&file_bytes);
        if &actual_sha != expected_sha {
            return Err(format!(
                "package file `{name}` sha256 `{actual_sha}` differs from package/SHA256SUMS (`{expected_sha}`)"
            ));
        }
    }

    // b. Restore: copy every package/<rel> to restore_dir/package/<rel> byte for byte.
    for entry in &manifest.files {
        if let Some(rel) = entry.path.strip_prefix("package/") {
            let archive_bytes = read_non_symlink_file(archive_dir, &entry.path)?;
            let target = restore_dir.join("package").join(rel);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("cannot create `{}`: {e}", parent.display()))?;
            }
            fs::write(&target, &archive_bytes)
                .map_err(|e| format!("cannot restore `{}`: {e}", target.display()))?;
            let restored_bytes = fs::read(&target)
                .map_err(|e| format!("cannot read restored `{}`: {e}", target.display()))?;
            if restored_bytes != archive_bytes {
                return Err(format!(
                    "restored `{}` does not match archive bytes",
                    target.display()
                ));
            }
        }
    }

    // c. Smoke rehearsal.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let exe_path = restore_dir.join("package/fieldbook");
        let mut perms = fs::metadata(&exe_path)
            .map_err(|e| format!("cannot stat `{}`: {e}", exe_path.display()))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&exe_path, perms)
            .map_err(|e| format!("cannot set permissions on `{}`: {e}", exe_path.display()))?;

        let exe_abs = if exe_path.is_absolute() {
            exe_path
        } else {
            std::env::current_dir().map_or_else(|_| exe_path.clone(), |c| c.join(&exe_path))
        };
        let exe_str = exe_abs
            .to_str()
            .ok_or_else(|| "executable path is not valid UTF-8".to_owned())?;

        let expected_stdout = expected_smoke_version_line(&manifest.tag, &manifest.commit);

        let output = proc::output(exe_str, &["--version"], restore_dir)?;
        let actual_stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !output.status.success() || actual_stdout != expected_stdout {
            return Err(format!(
                "packaged executable smoke failed:\nexpected: `{expected_stdout}`\nactual: `{actual_stdout}`"
            ));
        }
    }
    #[cfg(not(unix))]
    {
        return Err(
            "the packaged executable smoke runs on Linux only until the Windows package arrives (R2)"
                .to_owned(),
        );
    }

    // d. Write restore_dir/rehearsal.txt.
    let rehearsal_text = format!(
        "tool {}\ncommit {}\nsmoke pass\nrestore verified\n",
        manifest.tag, manifest.commit
    );
    fs::write(restore_dir.join("rehearsal.txt"), rehearsal_text)
        .map_err(|e| format!("cannot write `rehearsal.txt`: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{expected_smoke_version_line, register_release_providers, version_from_tag};

    #[test]
    fn version_from_tag_strips_leading_v() {
        assert_eq!(version_from_tag("v0.1.0"), "0.1.0");
        assert_eq!(version_from_tag("0.1.0"), "0.1.0");
    }

    #[test]
    fn expected_smoke_version_line_matches_format() {
        let line =
            expected_smoke_version_line("v1.2.3", "abcdef0123456789abcdef0123456789abcdef01");
        assert_eq!(
            line,
            "fieldbook 1.2.3 (abcdef0123456789abcdef0123456789abcdef01)"
        );
    }

    #[test]
    fn registration_succeeds() {
        register_release_providers().expect("register release providers");
    }
}
