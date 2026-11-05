//! Release mode descriptor, edition tag validator, shared archive manifest and dispatcher,
//! compatibility evidence collector, stream baseline advancement, and `edition-impact`
//! inspection (`docs/cards/edition-manifest.md` steps 5–8,
//! `docs/runbooks/release.md#durable-release-archive`, `#raw-edition-floor`,
//! `#put-a-source-version-on-hold-after-issue`).
//!
//! **Rules.** Does not import any renderer, browser harness, or executable smoke module;
//! `raw-edition` and `release-workflow` register their respective payload provider hooks.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use fieldbook_core::edition::{EditionInfo, latest_edition_heading};
use fieldbook_core::hash::sha256_hex;
use fieldbook_core::ids::{EditionId, SourceId, ToolRelease, VersionId};
use fieldbook_core::manifest::{verify_support_catalogue_bytes, write_sums};
use fieldbook_core::schema::SourceRow;
use fieldbook_core::schema::table;
use fieldbook_core::schema::vocab::Lifecycle;
use serde::{Deserialize, Serialize};

use crate::allowlist::{AllowlistProfile, parse_sha256sums, shipped_original_filename};
use crate::compat::{Baselines, CompatibilityBase, ReleaseBase, parse_baselines, parse_published};
use crate::proc;
use crate::zipdet::read_and_verify_deterministic_zip;

/// Delivery kind (`raw` vs `site`) in `edition-release.toml` and `archive.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DeliveryKind {
    /// `"raw"` floor edition without a generated site or browser summary.
    #[serde(rename = "raw")]
    Raw,
    /// `"site"` rendered edition or tool package.
    #[serde(rename = "site")]
    Site,
}

impl DeliveryKind {
    /// Wire word (`"raw"` or `"site"`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Site => "site",
        }
    }

    /// Parse `"raw"` or `"site"`.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "raw" => Ok(Self::Raw),
            "site" => Ok(Self::Site),
            other => Err(format!(
                "invalid delivery kind `{other}` (expected `raw` or `site`)"
            )),
        }
    }
}

impl fmt::Display for DeliveryKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Release stream (`tool` vs `edition`) in `archive.toml` and `release-baseline.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ReleaseStream {
    /// `"tool"` (`vX.Y.Z` executable stream).
    #[serde(rename = "tool")]
    Tool,
    /// `"edition"` (`edition-NNN` Corpus stream).
    #[serde(rename = "edition")]
    Edition,
}

impl ReleaseStream {
    /// Wire word (`"tool"` or `"edition"`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tool => "tool",
            Self::Edition => "edition",
        }
    }
}

impl fmt::Display for ReleaseStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Validated `edition-release.toml` mode descriptor (`release.md#raw-edition-floor`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditionReleaseMode {
    /// Three-digit edition number (`"001"`..`"999"`).
    pub edition: String,
    /// Selected delivery kind (`Raw` or `Site`).
    pub kind: DeliveryKind,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEditionReleaseMode {
    release_schema: u32,
    edition: String,
    kind: DeliveryKind,
}

/// Validated edition tag facts returned by [`validate_edition_tag`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedEditionTag {
    /// Parsed `EditionId` (`edition-NNN`).
    pub tag: EditionId,
    /// Three-digit edition number string (`NNN`).
    pub edition_number: String,
    /// Full 40-character lowercase hex commit SHA that the tag resolves to.
    pub commit: String,
    /// Delivery kind resolved from `edition-release.toml` (or `Site` when absent).
    pub kind: DeliveryKind,
}

/// Arguments for [`archive_release`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveArgs {
    /// Optional explicit delivery kind (`--kind raw|site`); required for edition stream, defaults to `Site` for tool stream.
    pub kind: Option<DeliveryKind>,
    /// Optional tool package directory (`--package DIR`); present only for `ReleaseStream::Tool`.
    pub package: Option<PathBuf>,
    /// Edition directory (`--edition DIR`) containing the paired ZIPs, `SHA256SUMS`, `corpus-support.toml`, and `corpus-support.sha256`.
    pub edition: PathBuf,
    /// Optional intake companion directory (`--intake DIR`).
    pub intake: Option<PathBuf>,
    /// Optional downloaded compatibility evidence directory (`--evidence DIR`).
    pub evidence: Option<PathBuf>,
    /// Optional `release-baseline.toml` path (`--base FILE`).
    pub base: Option<PathBuf>,
    /// Approved durable archive parent or target directory (`--to DIR`).
    pub to: PathBuf,
}

/// One validated `[[file]]` entry in `archive.toml`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveFileEntry {
    /// Non-empty slash-separated relative path beneath the archive folder.
    pub path: String,
    /// Unsigned byte size of the file.
    pub size: u64,
    /// Full 64-character lowercase hexadecimal SHA-256 digest.
    pub sha256: String,
}

/// Validated `archive.toml` manifest (`release.md#raw-edition-floor`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveManifest {
    /// Schema version (`1`).
    pub archive_schema: u32,
    /// Delivery kind (`raw` or `site`).
    pub kind: DeliveryKind,
    /// Release stream (`tool` or `edition`).
    pub stream: ReleaseStream,
    /// Release tag (`vX.Y.Z` or `edition-NNN`).
    pub tag: String,
    /// Full 40-character lowercase hex commit SHA.
    pub commit: String,
    /// Full 64-character lowercase hex SHA-256 of the baseline descriptor (`release-baseline.toml`).
    pub baseline_sha256: String,
    /// Sorted file rows (excluding `archive.toml` itself).
    #[serde(rename = "file", default)]
    pub files: Vec<ArchiveFileEntry>,
}

/// Result of [`verify_archive`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyArchiveReport {
    /// Verified manifest of the current archive.
    pub manifest: ArchiveManifest,
    /// Full 64-character lowercase hex SHA-256 of `archive.toml`.
    pub archive_manifest_sha256: String,
    /// Tag of the verified predecessor archive, or `None` when the stream's baseline is explicitly `empty`.
    pub verified_predecessor_tag: Option<String>,
    /// True when a tool archive with an explicitly `empty` tool baseline rehearsed its current package and synthetic restore fixture.
    pub used_first_tool_rehearsal: bool,
}

/// Impact classification for one `edition-NNN` tag in [`edition_impact`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ImpactStatus {
    /// The edition includes payload (original, extracted text, `s/` page, or search unit) for the target version.
    #[serde(rename = "containing")]
    Containing,
    /// The edition was verified and does not include any payload for the target version.
    #[serde(rename = "absent")]
    Absent,
    /// The archive is missing or failed verification, so payload inclusion cannot be ruled out.
    #[serde(rename = "unresolved")]
    Unresolved,
}

impl ImpactStatus {
    /// Wire word (`"containing"`, `"absent"`, or `"unresolved"`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Containing => "containing",
            Self::Absent => "absent",
            Self::Unresolved => "unresolved",
        }
    }
}

impl fmt::Display for ImpactStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One row of [`edition_impact`] output for a single `edition-NNN` tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditionImpactRow {
    /// Edition tag (`edition-NNN`).
    pub tag: String,
    /// Classification (`containing`, `absent`, or `unresolved`).
    pub status: ImpactStatus,
    /// Reason explaining the classification.
    pub reason: String,
}

/// Signature of a registered archive payload verifier and restore inspector.
///
/// Receives `(archive_dir, restore_to_dir, manifest)` and verifies the stream/kind-specific
/// payload and restores it into `restore_to_dir` for inspection.
pub type ArchiveProviderHook =
    fn(archive_dir: &Path, restore_dir: &Path, manifest: &ArchiveManifest) -> Result<(), String>;

#[derive(Default, Clone, Copy)]
struct ProviderRegistry {
    raw_edition: Option<ArchiveProviderHook>,
    site_edition: Option<ArchiveProviderHook>,
    tool_package: Option<ArchiveProviderHook>,
}

fn provider_registry() -> &'static Mutex<ProviderRegistry> {
    static REGISTRY: OnceLock<Mutex<ProviderRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(ProviderRegistry::default()))
}

/// Register the payload verifier/restore hook for `(stream, kind)`.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "provider registration hook for build-v0 and windows-exe"
    )
)]
pub fn register_archive_provider(
    stream: ReleaseStream,
    kind: DeliveryKind,
    hook: ArchiveProviderHook,
) -> Result<(), String> {
    let mut guard = provider_registry()
        .lock()
        .map_err(|_| "provider registry mutex poisoned".to_owned())?;
    match (stream, kind) {
        (ReleaseStream::Edition, DeliveryKind::Raw) => guard.raw_edition = Some(hook),
        (ReleaseStream::Edition, DeliveryKind::Site) => guard.site_edition = Some(hook),
        (ReleaseStream::Tool, DeliveryKind::Site) => guard.tool_package = Some(hook),
        (ReleaseStream::Tool, DeliveryKind::Raw) => {
            return Err("raw delivery kind is invalid for tool stream".to_owned());
        }
    }
    Ok(())
}

fn get_archive_provider(
    stream: ReleaseStream,
    kind: DeliveryKind,
) -> Result<ArchiveProviderHook, String> {
    let guard = provider_registry()
        .lock()
        .map_err(|_| "provider registry mutex poisoned".to_owned())?;
    match (stream, kind) {
        (ReleaseStream::Edition, DeliveryKind::Raw) => guard.raw_edition.ok_or_else(|| {
            "missing capability: archive provider for `edition` stream (`raw` kind) is not registered (provided by `raw-edition`)"
                .to_owned()
        }),
        (ReleaseStream::Edition, DeliveryKind::Site) => guard.site_edition.ok_or_else(|| {
            "missing capability: archive provider for `edition` stream (`site` kind) is not registered (provided by `release-workflow`)"
                .to_owned()
        }),
        (ReleaseStream::Tool, DeliveryKind::Site) => guard.tool_package.ok_or_else(|| {
            "missing capability: archive provider for `tool` stream (`site` kind) is not registered (provided by `release-workflow`)"
                .to_owned()
        }),
        (ReleaseStream::Tool, DeliveryKind::Raw) => {
            Err("invalid archive combination: `raw` kind requires `edition` stream".to_owned())
        }
    }
}

fn is_lower_hex(s: &str, len: usize) -> bool {
    s.len() == len
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn is_three_digit_edition_num(s: &str) -> bool {
    s.len() == 3 && s.bytes().all(|b| b.is_ascii_digit()) && s != "000"
}

/// Validate that `path` is a safe, non-empty, slash-separated relative path beneath an archive
/// or evidence root (`release.md#raw-edition-floor`).
pub fn validate_archive_rel_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("archive relative path must not be empty".to_owned());
    }
    if path.starts_with('/')
        || path.ends_with('/')
        || path.contains('\\')
        || path.contains(':')
        || path.bytes().any(|b| b < 0x20 || b == 0x7f)
    {
        return Err(format!(
            "unsafe archive relative path `{path}`: must be a clean slash-separated relative path"
        ));
    }
    for seg in path.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." {
            return Err(format!(
                "unsafe archive relative path `{path}`: invalid segment `{seg}`"
            ));
        }
    }
    Ok(())
}

/// Parse and validate `edition-release.toml` (`release.md#raw-edition-floor`).
///
/// When `toml_opt` is `None` (the file is absent in the commit), the delivery kind defaults to
/// [`DeliveryKind::Site`] for `expected_edition_number`. When present, `release_schema` must be
/// `1`, `edition` must be a 3-digit number matching `expected_edition_number` (when supplied),
/// and `kind` must be `"raw"` or `"site"`. Unknown or duplicate keys refuse.
pub fn parse_edition_release_mode(
    toml_opt: Option<&str>,
    expected_edition_number: Option<&str>,
) -> Result<EditionReleaseMode, String> {
    let Some(toml_str) = toml_opt else {
        let num = expected_edition_number.unwrap_or("001").to_owned();
        if !is_three_digit_edition_num(&num) {
            return Err(format!(
                "invalid edition number `{num}` (expected three digits `001`..`999`)"
            ));
        }
        return Ok(EditionReleaseMode {
            edition: num,
            kind: DeliveryKind::Site,
        });
    };

    let raw: RawEditionReleaseMode =
        toml::from_str(toml_str).map_err(|e| format!("edition-release.toml parse error: {e}"))?;
    if raw.release_schema != 1 {
        return Err(format!(
            "edition-release.toml: unsupported release_schema {} (expected 1)",
            raw.release_schema
        ));
    }
    if !is_three_digit_edition_num(&raw.edition) {
        return Err(format!(
            "edition-release.toml: `edition = \"{}\"` must be a three-digit edition number (`001`..`999`)",
            raw.edition
        ));
    }
    if let Some(expected) = expected_edition_number
        && raw.edition != expected
    {
        return Err(format!(
            "edition-release.toml: `edition = \"{}\"` does not match expected edition `{expected}`",
            raw.edition
        ));
    }
    Ok(EditionReleaseMode {
        edition: raw.edition,
        kind: raw.kind,
    })
}

/// Validate an `edition-NNN` tag in `repo_root` against:
/// 1. `EditionId` grammar (`edition-NNN`),
/// 2. commit resolution and ancestry on `origin/main` (or `main` when `origin/main` is absent in a local test repository),
/// 3. the newest `## edition-NNN` heading in `corpus/changes.md` at that exact commit,
/// 4. the committed `edition-release.toml` mode descriptor at that exact commit (`expected_kind` when supplied).
pub fn validate_edition_tag(
    repo_root: &Path,
    tag_name: &str,
    expected_kind: Option<DeliveryKind>,
) -> Result<ValidatedEditionTag, String> {
    let tag_id =
        EditionId::parse(tag_name).map_err(|e| format!("invalid edition tag `{tag_name}`: {e}"))?;
    let edition_number = format!("{:03}", tag_id.number());

    let commit_out = proc::git(
        repo_root,
        &[
            "rev-parse",
            "--verify",
            &format!("refs/tags/{tag_name}^{{commit}}"),
        ],
    )
    .map_err(|e| format!("tag `{tag_name}` does not exist or does not resolve to a commit: {e}"))?;
    let commit = commit_out.trim().to_owned();
    if !is_lower_hex(&commit, 40) {
        return Err(format!(
            "tag `{tag_name}` resolved to invalid commit SHA `{commit}`"
        ));
    }

    // Verify main ancestry: prefer `refs/remotes/origin/main`, fall back to `refs/heads/main` only if `origin/main` is absent.
    let main_ref = if proc::git(
        repo_root,
        &["rev-parse", "--verify", "refs/remotes/origin/main"],
    )
    .is_ok()
    {
        "refs/remotes/origin/main"
    } else if proc::git(repo_root, &["rev-parse", "--verify", "refs/heads/main"]).is_ok() {
        "refs/heads/main"
    } else {
        return Err("neither `origin/main` nor `main` exists in repository".to_owned());
    };

    let is_ancestor = proc::output(
        "git",
        &["merge-base", "--is-ancestor", &commit, main_ref],
        repo_root,
    )?;
    if !is_ancestor.status.success() {
        return Err(format!(
            "tag `{tag_name}` commit `{commit}` is not an ancestor of `{main_ref}`"
        ));
    }

    // Read `corpus/changes.md` at `commit` and verify newest heading.
    let changes_md = proc::git(repo_root, &["show", &format!("{commit}:corpus/changes.md")])
        .map_err(|e| format!("cannot read `corpus/changes.md` at `{commit}`: {e}"))?;
    let newest_heading = latest_edition_heading(&changes_md).ok_or_else(|| {
        format!("`corpus/changes.md` at `{commit}` has no `## edition-NNN` heading")
    })?;
    if newest_heading != tag_name {
        return Err(format!(
            "tag `{tag_name}` does not match newest `corpus/changes.md` heading `{newest_heading}` at `{commit}`"
        ));
    }

    // Read optional `edition-release.toml` at `commit`.
    let mode_toml_opt = match proc::output(
        "git",
        &["show", &format!("{commit}:edition-release.toml")],
        repo_root,
    )? {
        out if out.status.success() => Some(
            String::from_utf8(out.stdout)
                .map_err(|e| format!("`edition-release.toml` at `{commit}` is not UTF-8: {e}"))?,
        ),
        _ => None,
    };
    let mode = parse_edition_release_mode(mode_toml_opt.as_deref(), Some(&edition_number))?;
    if let Some(exp_kind) = expected_kind
        && mode.kind != exp_kind
    {
        return Err(format!(
            "tag `{tag_name}` committed delivery kind is `{}`, expected `{exp_kind}`",
            mode.kind
        ));
    }

    Ok(ValidatedEditionTag {
        tag: tag_id,
        edition_number,
        commit,
        kind: mode.kind,
    })
}

impl ArchiveManifest {
    /// Validate all structural and semantic invariants of `archive.toml`.
    pub fn validate(&self) -> Result<(), String> {
        if self.archive_schema != 1 {
            return Err(format!(
                "archive.toml: unsupported archive_schema {} (expected 1)",
                self.archive_schema
            ));
        }
        if self.kind == DeliveryKind::Raw && self.stream != ReleaseStream::Edition {
            return Err(
                "archive.toml: `kind = \"raw\"` is only permitted with `stream = \"edition\"`"
                    .to_owned(),
            );
        }
        match self.stream {
            ReleaseStream::Tool => {
                if ToolRelease::parse(&self.tag).is_err() {
                    return Err(format!(
                        "archive.toml: tool stream requires `vX.Y.Z` tag, found `{}`",
                        self.tag
                    ));
                }
            }
            ReleaseStream::Edition => {
                if EditionId::parse(&self.tag).is_err() {
                    return Err(format!(
                        "archive.toml: edition stream requires `edition-NNN` tag, found `{}`",
                        self.tag
                    ));
                }
            }
        }
        if !is_lower_hex(&self.commit, 40) {
            return Err(format!(
                "archive.toml: `commit = \"{}\"` must be 40 lowercase hex characters",
                self.commit
            ));
        }
        if !is_lower_hex(&self.baseline_sha256, 64) {
            return Err(format!(
                "archive.toml: `baseline_sha256 = \"{}\"` must be 64 lowercase hex characters",
                self.baseline_sha256
            ));
        }
        if self.files.is_empty() {
            return Err("archive.toml: `[[file]]` list must not be empty".to_owned());
        }

        let mut prev_path: Option<&str> = None;
        for entry in &self.files {
            validate_archive_rel_path(&entry.path)?;
            if entry.path == "archive.toml" {
                return Err("archive.toml must not list `archive.toml` itself".to_owned());
            }
            if !is_lower_hex(&entry.sha256, 64) {
                return Err(format!(
                    "archive.toml: file `{}` has invalid sha256 `{}`",
                    entry.path, entry.sha256
                ));
            }
            if let Some(prev) = prev_path
                && entry.path.as_bytes() <= prev.as_bytes()
            {
                return Err(format!(
                    "archive.toml: `[[file]]` rows are not strictly sorted by path (`{prev}` followed by `{}`)",
                    entry.path
                ));
            }
            prev_path = Some(&entry.path);
        }
        Ok(())
    }

    /// Parse and validate `archive.toml` from UTF-8 text.
    pub fn parse(toml_str: &str) -> Result<Self, String> {
        let manifest: Self =
            toml::from_str(toml_str).map_err(|e| format!("archive.toml parse error: {e}"))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Serialize `archive.toml` deterministically in canonical field and row order.
    #[must_use]
    pub fn to_toml(&self) -> String {
        let mut out = format!(
            "archive_schema = {}\nkind = \"{}\"\nstream = \"{}\"\ntag = \"{}\"\ncommit = \"{}\"\nbaseline_sha256 = \"{}\"\n",
            self.archive_schema,
            self.kind.as_str(),
            self.stream.as_str(),
            self.tag,
            self.commit,
            self.baseline_sha256,
        );
        for file in &self.files {
            out.push_str(&format!(
                "\n[[file]]\npath = \"{}\"\nsize = {}\nsha256 = \"{}\"\n",
                file.path, file.size, file.sha256
            ));
        }
        out
    }
}

fn read_non_symlink_file(root: &Path, rel_path: &str) -> Result<Vec<u8>, String> {
    validate_archive_rel_path(rel_path)?;
    let mut cur = root.to_path_buf();
    for seg in rel_path.split('/') {
        cur.push(seg);
        let meta = fs::symlink_metadata(&cur)
            .map_err(|e| format!("missing or unreadable file `{rel_path}`: {e}"))?;
        if meta.file_type().is_symlink() {
            return Err(format!(
                "path `{rel_path}` must not be or traverse a symlink"
            ));
        }
    }
    fs::read(&cur).map_err(|e| format!("cannot read `{rel_path}`: {e}"))
}

fn write_file_creating_parents(root: &Path, rel_path: &str, bytes: &[u8]) -> Result<(), String> {
    validate_archive_rel_path(rel_path)?;
    let dst = root.join(rel_path);
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create parent directory for `{rel_path}`: {e}"))?;
    }
    fs::write(&dst, bytes).map_err(|e| format!("cannot write `{rel_path}`: {e}"))
}

fn build_evidence_map(
    source_root: &Path,
    published_toml_bytes: &[u8],
    bound_entries: &[(String, String)],
    baseline_toml_bytes: &[u8],
    mode_toml_bytes: Option<&[u8]>,
) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let baseline_str = std::str::from_utf8(baseline_toml_bytes)
        .map_err(|e| format!("release-baseline.toml is not UTF-8: {e}"))?;
    let _ = parse_baselines(baseline_str)?;
    if let Some(mode_bytes) = mode_toml_bytes {
        let mode_str = std::str::from_utf8(mode_bytes)
            .map_err(|e| format!("edition-release.toml is not UTF-8: {e}"))?;
        let _ = parse_edition_release_mode(Some(mode_str), None)?;
    }

    let mut staged: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    staged.insert(
        "compat/published.toml".to_owned(),
        published_toml_bytes.to_vec(),
    );
    staged.insert(
        "release-baseline.toml".to_owned(),
        baseline_toml_bytes.to_vec(),
    );
    if let Some(mode_bytes) = mode_toml_bytes {
        staged.insert("edition-release.toml".to_owned(), mode_bytes.to_vec());
    }

    let mut seen_bound = BTreeSet::new();
    for (rel_path, expected_sha) in bound_entries {
        validate_archive_rel_path(rel_path)?;
        if !is_lower_hex(expected_sha, 64) {
            return Err(format!(
                "bound compatibility file `{rel_path}` has invalid sha256 `{expected_sha}`"
            ));
        }
        if !seen_bound.insert(rel_path.clone()) {
            return Err(format!(
                "duplicate bound compatibility file path `{rel_path}`"
            ));
        }
        let blob = read_non_symlink_file(source_root, rel_path)?;
        let actual_sha = sha256_hex(&blob);
        if &actual_sha != expected_sha {
            return Err(format!(
                "bound compatibility file `{rel_path}` sha256 `{actual_sha}` differs from declared `{expected_sha}`"
            ));
        }
        staged.insert(rel_path.clone(), blob);
    }

    Ok(staged)
}

/// Generic helper (`edition-manifest` step 7) that takes the already-validated `compat/published.toml`
/// bytes, its parsed `(path, sha256)` entries from `Published::bound_files()`, `release-baseline.toml`
/// bytes, and optional `edition-release.toml` bytes, verifies every bound blob in `source_root`
/// against its declared SHA-256 without symlinks, and copies those exact public files into
/// `evidence_out_dir` under their repository-relative paths (`compat/published.toml`,
/// `release-baseline.toml`, optional `edition-release.toml`, and every bound file).
///
/// Returns the sorted `(rel_path, sha256)` list written to `evidence_out_dir`.
pub fn collect_release_evidence(
    source_root: &Path,
    published_toml_bytes: &[u8],
    bound_entries: &[(String, String)],
    baseline_toml_bytes: &[u8],
    mode_toml_bytes: Option<&[u8]>,
    evidence_out_dir: &Path,
) -> Result<Vec<(String, String)>, String> {
    let staged = build_evidence_map(
        source_root,
        published_toml_bytes,
        bound_entries,
        baseline_toml_bytes,
        mode_toml_bytes,
    )?;

    fs::create_dir_all(evidence_out_dir).map_err(|e| {
        format!(
            "cannot create evidence output directory `{}`: {e}",
            evidence_out_dir.display()
        )
    })?;

    let mut result = Vec::with_capacity(staged.len());
    for (rel_path, bytes) in &staged {
        write_file_creating_parents(evidence_out_dir, rel_path, bytes)?;
        result.push((rel_path.clone(), sha256_hex(bytes)));
    }
    Ok(result)
}

struct VerifiedEditionArtifacts {
    ed_tag: String,
    ed_info: EditionInfo,
    ed_zip_name: String,
    orig_zip_name: String,
    support_bytes: Vec<u8>,
    support_sha_bytes: Vec<u8>,
}

/// Inspect and verify a downloaded or local edition artifact directory (`--edition DIR`).
fn verify_edition_artifact_set(
    edition_dir: &Path,
    expected_kind: DeliveryKind,
) -> Result<VerifiedEditionArtifacts, String> {
    let sums_bytes = read_non_symlink_file(edition_dir, "SHA256SUMS")?;
    let sums_str =
        std::str::from_utf8(&sums_bytes).map_err(|e| format!("SHA256SUMS is not UTF-8: {e}"))?;
    let sums_map = parse_sha256sums(sums_str, "SHA256SUMS").map_err(|e| e.to_string())?;
    if sums_map.len() != 2 {
        return Err(format!(
            "edition `SHA256SUMS` must list exactly 2 zip files, found {}",
            sums_map.len()
        ));
    }

    let mut edition_zip_name: Option<String> = None;
    let mut originals_zip_name: Option<String> = None;
    for (name, expected_sha) in &sums_map {
        let zip_bytes = read_non_symlink_file(edition_dir, name)?;
        let actual_sha = sha256_hex(&zip_bytes);
        if &actual_sha != expected_sha {
            return Err(format!(
                "zip `{name}` sha256 `{actual_sha}` differs from `SHA256SUMS` (`{expected_sha}`)"
            ));
        }
        if name.starts_with("fieldbook-edition-") && name.ends_with(".zip") {
            edition_zip_name = Some(name.clone());
        } else if name.starts_with("fieldbook-originals-") && name.ends_with(".zip") {
            originals_zip_name = Some(name.clone());
        } else {
            return Err(format!("unexpected file `{name}` in edition `SHA256SUMS`"));
        }
    }

    let ed_zip = edition_zip_name
        .ok_or_else(|| "missing `fieldbook-edition-NNN.zip` in `SHA256SUMS`".to_owned())?;
    let orig_zip = originals_zip_name
        .ok_or_else(|| "missing `fieldbook-originals-NNN.zip` in `SHA256SUMS`".to_owned())?;

    let ed_num = ed_zip
        .strip_prefix("fieldbook-edition-")
        .and_then(|s| s.strip_suffix(".zip"))
        .ok_or_else(|| format!("invalid edition zip name `{ed_zip}`"))?;
    let orig_num = orig_zip
        .strip_prefix("fieldbook-originals-")
        .and_then(|s| s.strip_suffix(".zip"))
        .ok_or_else(|| format!("invalid originals zip name `{orig_zip}`"))?;
    if ed_num != orig_num || !is_three_digit_edition_num(ed_num) {
        return Err(format!(
            "mismatched or invalid edition numbers between `{ed_zip}` and `{orig_zip}`"
        ));
    }
    let expected_tag = format!("edition-{ed_num}");

    let ed_zip_bytes = read_non_symlink_file(edition_dir, &ed_zip)?;
    let orig_zip_bytes = read_non_symlink_file(edition_dir, &orig_zip)?;
    let ed_entries = read_and_verify_deterministic_zip(&ed_zip_bytes, "edition", None)?;
    let orig_entries = read_and_verify_deterministic_zip(&orig_zip_bytes, "originals", None)?;

    let ed_map: BTreeMap<String, Vec<u8>> = ed_entries.into_iter().collect();
    let orig_map: BTreeMap<String, Vec<u8>> = orig_entries.into_iter().collect();

    let profile = match expected_kind {
        DeliveryKind::Raw => AllowlistProfile::Raw,
        DeliveryKind::Site => AllowlistProfile::Site,
    };
    crate::allowlist::verify_staged_maps(profile, &ed_map, &orig_map, &[])
        .map_err(|e| format!("edition zip allowlist/integrity failure: {e}"))?;

    let ed_toml_bytes = ed_map
        .get("edition.toml")
        .ok_or_else(|| "missing `edition.toml` inside edition zip".to_owned())?;
    let ed_toml_str = std::str::from_utf8(ed_toml_bytes)
        .map_err(|e| format!("`edition.toml` is not UTF-8: {e}"))?;
    let ed_info =
        EditionInfo::parse(ed_toml_str).map_err(|e| format!("invalid `edition.toml`: {e}"))?;
    if ed_info.edition != expected_tag {
        return Err(format!(
            "`edition.toml` edition `{}` does not match zip tag `{expected_tag}`",
            ed_info.edition
        ));
    }
    if !is_lower_hex(&ed_info.commit, 40) {
        return Err(format!(
            "`edition.toml` commit `{}` must be a released 40-hex commit SHA",
            ed_info.commit
        ));
    }

    // Verify sidecar `corpus-support.toml` and `corpus-support.sha256` against staged `evidence/corpus-support.toml` and `ed_info.commit`.
    let staged_support = ed_map
        .get("evidence/corpus-support.toml")
        .ok_or_else(|| "missing `evidence/corpus-support.toml` in edition zip".to_owned())?;
    let sidecar_support = read_non_symlink_file(edition_dir, "corpus-support.toml")?;
    let sidecar_sha_file = read_non_symlink_file(edition_dir, "corpus-support.sha256")?;
    if staged_support != &sidecar_support {
        return Err(
            "sidecar `corpus-support.toml` differs from `edition/evidence/corpus-support.toml`"
                .to_owned(),
        );
    }
    let expected_sha_hex = sha256_hex(&sidecar_support);
    let expected_sha_line =
        write_sums(&[("corpus-support.toml".to_owned(), expected_sha_hex.clone())]);
    if sidecar_sha_file != expected_sha_line.as_bytes() {
        return Err(
            "`corpus-support.sha256` does not match `<sha256>  corpus-support.toml\\n` of `corpus-support.toml`"
                .to_owned(),
        );
    }
    let parsed_cat = verify_support_catalogue_bytes(&sidecar_support, &expected_sha_hex)
        .map_err(|e| format!("invalid `corpus-support.toml`: {e}"))?;
    if parsed_cat.commit != ed_info.commit {
        return Err(format!(
            "`corpus-support.toml` commit `{}` does not match `edition.toml` commit `{}`",
            parsed_cat.commit, ed_info.commit
        ));
    }

    Ok(VerifiedEditionArtifacts {
        ed_tag: expected_tag,
        ed_info,
        ed_zip_name: ed_zip,
        orig_zip_name: orig_zip,
        support_bytes: sidecar_support,
        support_sha_bytes: sidecar_sha_file,
    })
}

fn collect_all_rel_files(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let mut out = BTreeMap::new();
    collect_all_rel_files_inner(root, "", &mut out)?;
    Ok(out)
}

fn collect_all_rel_files_inner(
    cur: &Path,
    prefix: &str,
    out: &mut BTreeMap<String, Vec<u8>>,
) -> Result<(), String> {
    let meta =
        fs::symlink_metadata(cur).map_err(|e| format!("cannot stat `{}`: {e}", cur.display()))?;
    if meta.file_type().is_symlink() {
        return Err(format!("symlink `{}` is prohibited", cur.display()));
    }
    if !meta.is_dir() {
        return Err(format!("`{}` is not a directory", cur.display()));
    }
    let mut entries: Vec<fs::DirEntry> = fs::read_dir(cur)
        .map_err(|e| format!("cannot read `{}`: {e}", cur.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("dir entry error in `{}`: {e}", cur.display()))?;
    entries.sort_by_key(fs::DirEntry::file_name);

    for entry in entries {
        let name_os = entry.file_name();
        let Some(name) = name_os.to_str() else {
            return Err(format!("non-UTF-8 filename in `{}`", cur.display()));
        };
        let rel = if prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{prefix}/{name}")
        };
        validate_archive_rel_path(&rel)?;
        let path = entry.path();
        let emeta = fs::symlink_metadata(&path).map_err(|e| format!("cannot stat `{rel}`: {e}"))?;
        let ft = emeta.file_type();
        if ft.is_symlink() {
            return Err(format!("symlink `{rel}` is prohibited"));
        }
        if ft.is_dir() {
            collect_all_rel_files_inner(&path, &rel, out)?;
        } else if ft.is_file() {
            let bytes = fs::read(&path).map_err(|e| format!("cannot read `{rel}`: {e}"))?;
            out.insert(rel, bytes);
        } else {
            return Err(format!("special file `{rel}` is prohibited"));
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolPackageDescriptor {
    tag: String,
    commit: String,
}

/// Copy a verified release set into an immutable durable archive directory, reopen every file to
/// verify byte-for-byte equality, and publish `archive.toml` last (`release.md#durable-release-archive`).
///
/// Returns `(archive_folder_path, ArchiveManifest, archive_manifest_sha256)`.
pub fn archive_release(args: &ArchiveArgs) -> Result<(PathBuf, ArchiveManifest, String), String> {
    let stream = if args.package.is_some() {
        ReleaseStream::Tool
    } else {
        ReleaseStream::Edition
    };
    let kind = match (stream, args.kind) {
        (ReleaseStream::Tool, None | Some(DeliveryKind::Site)) => DeliveryKind::Site,
        (ReleaseStream::Tool, Some(DeliveryKind::Raw)) => {
            return Err("tool stream cannot use `kind = \"raw\"`".to_owned());
        }
        (ReleaseStream::Edition, Some(k)) => k,
        (ReleaseStream::Edition, None) => {
            return Err("edition archive-release requires `--kind raw|site`".to_owned());
        }
    };

    let VerifiedEditionArtifacts {
        ed_tag,
        ed_info,
        ed_zip_name,
        orig_zip_name,
        support_bytes,
        support_sha_bytes,
    } = verify_edition_artifact_set(&args.edition, kind)?;

    // Determine release tag and commit.
    let (release_tag, release_commit) = if let Some(pkg_dir) = &args.package {
        let pkg_files = collect_all_rel_files(pkg_dir)?;
        if pkg_files.is_empty() {
            return Err("tool `--package` directory must not be empty".to_owned());
        }
        if let Some(pkg_toml_bytes) = pkg_files.get("package.toml") {
            let pkg_str = std::str::from_utf8(pkg_toml_bytes)
                .map_err(|e| format!("package.toml is not UTF-8: {e}"))?;
            let desc: ToolPackageDescriptor =
                toml::from_str(pkg_str).map_err(|e| format!("invalid package.toml: {e}"))?;
            let _ = ToolRelease::parse(&desc.tag)
                .map_err(|e| format!("invalid tool tag `{}` in package.toml: {e}", desc.tag))?;
            if !is_lower_hex(&desc.commit, 40) {
                return Err(format!("invalid commit `{}` in package.toml", desc.commit));
            }
            if desc.commit != ed_info.commit {
                return Err(format!(
                    "tool package commit `{}` differs from paired edition commit `{}`",
                    desc.commit, ed_info.commit
                ));
            }
            (desc.tag, desc.commit)
        } else {
            return Err(
                "tool `--package` directory requires `package.toml` with `tag` and `commit`"
                    .to_owned(),
            );
        }
    } else {
        (ed_tag.clone(), ed_info.commit.clone())
    };

    // Locate and validate `release-baseline.toml` and compatibility evidence.
    let evidence_dir = args
        .evidence
        .as_deref()
        .ok_or_else(|| "archive-release requires `--evidence <dir>`".to_owned())?;
    let base_bytes = if let Some(base_path) = &args.base {
        fs::read(base_path)
            .map_err(|e| format!("cannot read `--base {}`: {e}", base_path.display()))?
    } else {
        read_non_symlink_file(evidence_dir, "release-baseline.toml")?
    };
    let base_str = std::str::from_utf8(&base_bytes)
        .map_err(|e| format!("release-baseline.toml is not UTF-8: {e}"))?;
    let _ = parse_baselines(base_str)?;
    let baseline_sha256 = sha256_hex(&base_bytes);

    // Verify evidence directory contains `compat/published.toml` and all bound files with matching hashes.
    let pub_toml_bytes = read_non_symlink_file(evidence_dir, "compat/published.toml")?;
    let pub_toml_str = std::str::from_utf8(&pub_toml_bytes)
        .map_err(|e| format!("evidence `compat/published.toml` is not UTF-8: {e}"))?;
    let published = parse_published(pub_toml_str)?;
    let bound_entries = published.bound_files()?;

    // Verify optional `edition-release.toml` in evidence matches `ed_tag` and `kind` for edition stream.
    let mode_bytes_opt = if evidence_dir.join("edition-release.toml").exists() {
        Some(read_non_symlink_file(evidence_dir, "edition-release.toml")?)
    } else {
        None
    };
    if stream == ReleaseStream::Edition {
        let ed_num = ed_tag
            .strip_prefix("edition-")
            .ok_or_else(|| format!("invalid edition tag `{ed_tag}`"))?;
        let mode_str_opt = match &mode_bytes_opt {
            Some(b) => Some(
                std::str::from_utf8(b)
                    .map_err(|e| format!("edition-release.toml is not UTF-8: {e}"))?,
            ),
            None => None,
        };
        let mode = parse_edition_release_mode(mode_str_opt, Some(ed_num))?;
        if mode.kind != kind {
            return Err(format!(
                "evidence mode descriptor kind `{}` does not match `--kind {kind}`",
                mode.kind
            ));
        }
    }

    // Build the complete map of relative paths -> bytes for the new archive folder.
    let mut staged_archive: BTreeMap<String, Vec<u8>> = BTreeMap::new();

    // 1. Edition paired zips, external SHA256SUMS, and support sidecars.
    for file_name in [
        ed_zip_name.as_str(),
        orig_zip_name.as_str(),
        "SHA256SUMS",
        "corpus-support.toml",
        "corpus-support.sha256",
    ] {
        let bytes = read_non_symlink_file(&args.edition, file_name)?;
        staged_archive.insert(file_name.to_owned(), bytes);
    }

    // 2. Collected compatibility evidence under `evidence/<repo-relative-path>`.
    let ev_map = build_evidence_map(
        evidence_dir,
        &pub_toml_bytes,
        &bound_entries,
        &base_bytes,
        mode_bytes_opt.as_deref(),
    )?;
    for (rel, bytes) in ev_map {
        staged_archive.insert(format!("evidence/{rel}"), bytes);
    }

    // 3. Optional `--package` files under `package/<rel>`.
    if let Some(pkg_dir) = &args.package {
        for (rel, bytes) in collect_all_rel_files(pkg_dir)? {
            staged_archive.insert(format!("package/{rel}"), bytes);
        }
    }

    // 4. Optional `--intake` files under `intake/<rel>`.
    if let Some(intake_dir) = &args.intake {
        for (rel, bytes) in collect_all_rel_files(intake_dir)? {
            staged_archive.insert(format!("intake/{rel}"), bytes);
        }
    }

    // Build `ArchiveManifest`.
    let files: Vec<ArchiveFileEntry> = staged_archive
        .iter()
        .map(|(path, bytes)| ArchiveFileEntry {
            path: path.clone(),
            size: bytes.len() as u64,
            sha256: sha256_hex(bytes),
        })
        .collect();
    let manifest = ArchiveManifest {
        archive_schema: 1,
        kind,
        stream,
        tag: release_tag.clone(),
        commit: release_commit.clone(),
        baseline_sha256,
        files,
    };
    manifest.validate()?;
    let manifest_toml_bytes = manifest.to_toml().into_bytes();
    let manifest_sha256 = sha256_hex(&manifest_toml_bytes);

    // Determine target archive folder: `<to>/<tag>-<commit[..12]>`.
    let folder_name = format!("{release_tag}-{}", &release_commit[..12]);
    let target_dir = args.to.join(&folder_name);

    if target_dir.exists() {
        // An existing folder is accepted ONLY if every byte (including archive.toml) matches; never overwritten.
        let existing_files = collect_all_rel_files(&target_dir)?;
        let mut expected_all = staged_archive.clone();
        expected_all.insert("archive.toml".to_owned(), manifest_toml_bytes.clone());
        if existing_files != expected_all {
            return Err(format!(
                "immutable archive folder `{}` already exists with different bytes; refusing to overwrite",
                target_dir.display()
            ));
        }
    } else {
        fs::create_dir_all(&args.to)
            .map_err(|e| format!("cannot create archive root `{}`: {e}", args.to.display()))?;
        let staging_dir = args
            .to
            .join(format!(".staging-{folder_name}-{}", std::process::id()));
        if staging_dir.exists() {
            let _ = fs::remove_dir_all(&staging_dir);
        }
        fs::create_dir_all(&staging_dir)
            .map_err(|e| format!("cannot create staging dir `{}`: {e}", staging_dir.display()))?;

        // Write all payload files first, reopen and verify, then write `archive.toml` last and rename into place.
        for (rel, bytes) in &staged_archive {
            write_file_creating_parents(&staging_dir, rel, bytes)?;
        }
        write_file_creating_parents(&staging_dir, "archive.toml", &manifest_toml_bytes)?;
        fs::rename(&staging_dir, &target_dir).map_err(|e| {
            format!(
                "cannot publish staging archive to `{}`: {e}",
                target_dir.display()
            )
        })?;
    }

    // Reopen every file in `target_dir` to verify exact byte equality, including `corpus-support.toml` and `corpus-support.sha256`.
    verify_archive_directory_bytes(&target_dir, &support_bytes, &support_sha_bytes)?;

    Ok((target_dir, manifest, manifest_sha256))
}

fn verify_archive_directory_bytes(
    archive_dir: &Path,
    expected_support_bytes: &[u8],
    expected_support_sha_bytes: &[u8],
) -> Result<(ArchiveManifest, String), String> {
    let actual_files = collect_all_rel_files(archive_dir)?;
    let Some(manifest_bytes) = actual_files.get("archive.toml") else {
        return Err(format!(
            "archive `{}` is missing `archive.toml`",
            archive_dir.display()
        ));
    };
    let manifest_str = std::str::from_utf8(manifest_bytes)
        .map_err(|e| format!("`archive.toml` is not UTF-8: {e}"))?;
    let manifest = ArchiveManifest::parse(manifest_str)?;
    let manifest_sha256 = sha256_hex(manifest_bytes);

    let listed_map: BTreeMap<&str, &ArchiveFileEntry> = manifest
        .files
        .iter()
        .map(|f| (f.path.as_str(), f))
        .collect();

    for (path, bytes) in &actual_files {
        if path == "archive.toml" {
            continue;
        }
        let Some(entry) = listed_map.get(path.as_str()) else {
            return Err(format!(
                "unlisted file `{path}` present in archive `{}`",
                archive_dir.display()
            ));
        };
        if bytes.len() as u64 != entry.size {
            return Err(format!(
                "archived file `{path}` size {} differs from `archive.toml` ({})",
                bytes.len(),
                entry.size
            ));
        }
        let sha = sha256_hex(bytes);
        if sha != entry.sha256 {
            return Err(format!(
                "archived file `{path}` sha256 `{sha}` differs from `archive.toml` (`{}`)",
                entry.sha256
            ));
        }
    }

    for entry in &manifest.files {
        if !actual_files.contains_key(&entry.path) {
            return Err(format!(
                "file `{}` listed in `archive.toml` is missing from `{}`",
                entry.path,
                archive_dir.display()
            ));
        }
    }

    // Verify reopened `corpus-support.toml` and `corpus-support.sha256` sidecars and their equality with the zip copy.
    let VerifiedEditionArtifacts {
        ed_info,
        support_bytes: zip_support,
        support_sha_bytes: zip_support_sha,
        ..
    } = verify_edition_artifact_set(archive_dir, manifest.kind)?;
    if ed_info.commit != manifest.commit {
        return Err(format!(
            "archived `edition.toml` commit `{}` differs from `archive.toml` commit `{}`",
            ed_info.commit, manifest.commit
        ));
    }
    if !expected_support_bytes.is_empty() && zip_support != expected_support_bytes {
        return Err("reopened `corpus-support.toml` differs from staged bytes".to_owned());
    }
    if !expected_support_sha_bytes.is_empty() && zip_support_sha != expected_support_sha_bytes {
        return Err("reopened `corpus-support.sha256` differs from staged bytes".to_owned());
    }

    // Verify `evidence/release-baseline.toml` hash equals `manifest.baseline_sha256`.
    let base_bytes = read_non_symlink_file(archive_dir, "evidence/release-baseline.toml")?;
    if sha256_hex(&base_bytes) != manifest.baseline_sha256 {
        return Err(
            "archived `evidence/release-baseline.toml` sha256 differs from `archive.toml` `baseline_sha256`"
                .to_owned(),
        );
    }

    // Verify `evidence/compat/published.toml` and all bound compatibility blobs inside `evidence/`.
    let pub_bytes = read_non_symlink_file(archive_dir, "evidence/compat/published.toml")?;
    let pub_str = std::str::from_utf8(&pub_bytes)
        .map_err(|e| format!("archived `compat/published.toml` is not UTF-8: {e}"))?;
    let published = parse_published(pub_str)?;
    for (rel_path, expected_sha) in published.bound_files()? {
        let ev_rel = format!("evidence/{rel_path}");
        let blob = read_non_symlink_file(archive_dir, &ev_rel)?;
        let actual_sha = sha256_hex(&blob);
        if actual_sha != expected_sha {
            return Err(format!(
                "archived compatibility vector `{ev_rel}` sha256 `{actual_sha}` differs from `published.toml` (`{expected_sha}`)"
            ));
        }
    }

    Ok((manifest, manifest_sha256))
}

fn ensure_empty_directory(dir: &Path) -> Result<(), String> {
    if dir.exists() {
        let mut rd =
            fs::read_dir(dir).map_err(|e| format!("cannot read `{}`: {e}", dir.display()))?;
        if rd.next().is_some() {
            return Err(format!(
                "restore directory `{}` must be empty",
                dir.display()
            ));
        }
    } else {
        fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create restore directory `{}`: {e}", dir.display()))?;
    }
    Ok(())
}

/// Verify a durable archive folder and its stream-specific predecessor (`release.md#durable-release-archive`).
///
/// - Always verifies the current archive's `archive.toml`, all file hashes, `edition.toml`,
///   `corpus-support.toml` / `corpus-support.sha256`, and `evidence/` compatibility blobs,
///   then dispatches to the registered provider for `(manifest.stream, manifest.kind)`.
/// - Reads `evidence/release-baseline.toml` from the current archive and selects ONLY the
///   predecessor for `manifest.stream` (`baselines.tool_predecessor()` for `Tool`,
///   `baselines.edition_predecessor()` for `Edition`).
/// - If the stream's baseline is `Some(pred)`, `--previous` MUST be supplied and its verified
///   `archive.toml` must match `(pred.tag, pred.commit, pred.archive_manifest_sha256)` and the
///   same stream, and its provider is invoked to restore/verify the predecessor.
/// - If the stream's baseline is `None` (`kind = "empty"`), `--previous` must be `None` and the
///   current archive's rehearsal/inspection satisfies first-delivery verification.
pub fn verify_archive(
    archive_dir: &Path,
    restore_to: &Path,
    previous_archive: Option<&Path>,
) -> Result<VerifyArchiveReport, String> {
    ensure_empty_directory(restore_to)?;
    let (manifest, archive_manifest_sha256) =
        verify_archive_directory_bytes(archive_dir, &[], &[])?;

    let base_bytes = read_non_symlink_file(archive_dir, "evidence/release-baseline.toml")?;
    let base_str = std::str::from_utf8(&base_bytes)
        .map_err(|e| format!("archived release-baseline.toml is not UTF-8: {e}"))?;
    let baselines = parse_baselines(base_str)?;

    // Select predecessor strictly from `manifest.stream` (never cross-substituting `compatibility` or the other stream).
    let expected_pred: Option<&ReleaseBase> = match manifest.stream {
        ReleaseStream::Tool => baselines.tool_predecessor(),
        ReleaseStream::Edition => baselines.edition_predecessor(),
    };

    let current_restore_dir = restore_to.join("current");
    fs::create_dir_all(&current_restore_dir)
        .map_err(|e| format!("cannot create `{}`: {e}", current_restore_dir.display()))?;
    let current_hook = get_archive_provider(manifest.stream, manifest.kind)?;
    current_hook(archive_dir, &current_restore_dir, &manifest)?;

    match (expected_pred, previous_archive) {
        (Some(pred), None) => Err(format!(
            "stream `{}` declares predecessor `{}` (`{}`), but `--previous` archive was omitted; missing predecessor never substitutes an empty baseline",
            manifest.stream, pred.tag, pred.commit
        )),
        (None, Some(prev_dir)) => Err(format!(
            "stream `{}` baseline is explicitly `empty`, but `--previous {}` was supplied",
            manifest.stream,
            prev_dir.display()
        )),
        (None, None) => Ok(VerifyArchiveReport {
            used_first_tool_rehearsal: manifest.stream == ReleaseStream::Tool,
            manifest,
            archive_manifest_sha256,
            verified_predecessor_tag: None,
        }),
        (Some(pred), Some(prev_dir)) => {
            let (prev_manifest, prev_sha) = verify_archive_directory_bytes(prev_dir, &[], &[])?;
            if prev_manifest.stream != manifest.stream {
                return Err(format!(
                    "previous archive stream `{}` does not match current stream `{}`",
                    prev_manifest.stream, manifest.stream
                ));
            }
            if prev_manifest.tag != pred.tag
                || prev_manifest.commit != pred.commit
                || prev_sha != pred.archive_manifest_sha256
            {
                return Err(format!(
                    "previous archive (`{}`, `{}`, `{prev_sha}`) does not match declared `{}` baseline (`{}`, `{}`, `{}`)",
                    prev_manifest.tag,
                    prev_manifest.commit,
                    manifest.stream,
                    pred.tag,
                    pred.commit,
                    pred.archive_manifest_sha256
                ));
            }
            let prev_restore_dir = restore_to.join("previous");
            fs::create_dir_all(&prev_restore_dir)
                .map_err(|e| format!("cannot create `{}`: {e}", prev_restore_dir.display()))?;
            let prev_hook = get_archive_provider(prev_manifest.stream, prev_manifest.kind)?;
            prev_hook(prev_dir, &prev_restore_dir, &prev_manifest)?;

            Ok(VerifyArchiveReport {
                used_first_tool_rehearsal: false,
                manifest,
                archive_manifest_sha256,
                verified_predecessor_tag: Some(prev_manifest.tag),
            })
        }
    }
}

/// Advance `release-baseline.toml` after accepting a verified archive (`release.md#release-baseline`,
/// `edition-manifest` step 8):
/// - Advances `[compatibility]` to the accepted archive's tag, commit, `archive_manifest_sha256`,
///   and the exact `compat/published.toml` + bound vector hashes retained in `archive_dir/evidence/`.
/// - Advances `[tool]` when `manifest.stream == ReleaseStream::Tool`, preserving `[edition]`.
/// - Advances `[edition]` when `manifest.stream == ReleaseStream::Edition`, preserving `[tool]`.
pub fn advance_baselines_from_archive(
    prior: &Baselines,
    archive_dir: &Path,
) -> Result<Baselines, String> {
    let (manifest, archive_manifest_sha256) =
        verify_archive_directory_bytes(archive_dir, &[], &[])?;
    let pub_bytes = read_non_symlink_file(archive_dir, "evidence/compat/published.toml")?;
    let pub_str = std::str::from_utf8(&pub_bytes)
        .map_err(|e| format!("archived compat/published.toml is not UTF-8: {e}"))?;
    let published = parse_published(pub_str)?;
    let mut compat_files = published.bound_files()?;
    compat_files.push(("compat/published.toml".to_owned(), sha256_hex(&pub_bytes)));
    compat_files.sort_by(|a, b| a.0.cmp(&b.0));

    let new_compat = CompatibilityBase {
        tag: manifest.tag.clone(),
        commit: manifest.commit.clone(),
        archive_manifest_sha256: archive_manifest_sha256.clone(),
        files: compat_files,
    };
    let new_stream_base = ReleaseBase {
        tag: manifest.tag,
        commit: manifest.commit,
        archive_manifest_sha256,
    };

    let (tool, edition) = match manifest.stream {
        ReleaseStream::Tool => (Some(new_stream_base), prior.edition.clone()),
        ReleaseStream::Edition => (prior.tool.clone(), Some(new_stream_base)),
    };

    Ok(Baselines {
        compatibility: Some(new_compat),
        tool,
        edition,
    })
}

/// Serialize [`Baselines`] into canonical `release-baseline.toml` text.
#[must_use]
pub fn serialize_baselines(baselines: &Baselines) -> String {
    let mut out = String::from("baseline_schema = 1\n\n[compatibility]\n");
    if let Some(compat) = &baselines.compatibility {
        out.push_str(&format!(
            "kind = \"release\"\ntag = \"{}\"\ncommit = \"{}\"\narchive_manifest_sha256 = \"{}\"\n",
            compat.tag, compat.commit, compat.archive_manifest_sha256
        ));
        for (path, sha) in &compat.files {
            out.push_str(&format!(
                "\n[[compatibility.file]]\npath = \"{path}\"\nsha256 = \"{sha}\"\n"
            ));
        }
    } else {
        out.push_str("kind = \"empty\"\n");
    }

    out.push_str("\n[tool]\n");
    if let Some(tool) = &baselines.tool {
        out.push_str(&format!(
            "kind = \"release\"\ntag = \"{}\"\ncommit = \"{}\"\narchive_manifest_sha256 = \"{}\"\n",
            tool.tag, tool.commit, tool.archive_manifest_sha256
        ));
    } else {
        out.push_str("kind = \"empty\"\n");
    }

    out.push_str("\n[edition]\n");
    if let Some(ed) = &baselines.edition {
        out.push_str(&format!(
            "kind = \"release\"\ntag = \"{}\"\ncommit = \"{}\"\narchive_manifest_sha256 = \"{}\"\n",
            ed.tag, ed.commit, ed.archive_manifest_sha256
        ));
    } else {
        out.push_str("kind = \"empty\"\n");
    }

    out
}

/// Inspect all `edition-NNN` tags in `repo_root` and their verified durable archives under
/// `archives_root` to determine which editions contain payload for `(source_id_str, version_num)`
/// (`release.md#put-a-source-version-on-hold-after-issue`, `edition-manifest` step 6).
///
/// - Uses structured `source_id` and `version` columns in `sources.csv` (never git pickaxe,
///   tag ancestry, or `supersedes` references) and inspects the verified archived edition/originals
///   payload paths and hashes.
/// - Missing or unverified archives are ALWAYS classified as [`ImpactStatus::Unresolved`], never
///   [`ImpactStatus::Absent`].
/// - A quarantined version retaining only its `sources.csv` row, hash, and note with no original,
///   extraction text, `s/<vid>.html` page, or search/context payload is classified as
///   [`ImpactStatus::Absent`].
pub fn edition_impact(
    repo_root: &Path,
    source_id_str: &str,
    version_num: u32,
    archives_root: &Path,
) -> Result<Vec<EditionImpactRow>, String> {
    let target_source_id = SourceId::parse(source_id_str)
        .map_err(|e| format!("invalid `--source {source_id_str}`: {e}"))?;
    if version_num == 0 {
        return Err("`--version` must be a positive integer (>= 1)".to_owned());
    }
    let target_vid_str = format!("{target_source_id}-v{version_num}");
    let target_vid = VersionId::parse(&target_vid_str)
        .map_err(|e| format!("invalid version id `{target_vid_str}`: {e}"))?;

    // Enumerate `edition-*` tags in `repo_root`.
    let tags_out = proc::git(repo_root, &["tag", "--list", "edition-*"])?;
    let mut edition_tags: Vec<EditionId> = Vec::new();
    for line in tags_out.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let id = EditionId::parse(trimmed)
            .map_err(|e| format!("invalid edition tag `{trimmed}` in repository: {e}"))?;
        edition_tags.push(id);
    }
    edition_tags.sort();

    // Index available archive folders under `archives_root` by tag.
    let archive_by_tag = index_archives_by_tag(archives_root)?;

    let mut rows = Vec::with_capacity(edition_tags.len());
    for tag_id in edition_tags {
        let tag = tag_id.as_str().to_owned();
        let row = evaluate_tag_impact(
            repo_root,
            &tag,
            &target_source_id,
            version_num,
            &target_vid,
            archive_by_tag.get(&tag).map(PathBuf::as_path),
        );
        rows.push(row);
    }

    Ok(rows)
}

fn index_archives_by_tag(archives_root: &Path) -> Result<BTreeMap<String, PathBuf>, String> {
    let mut map = BTreeMap::new();
    if !archives_root.exists() {
        return Ok(map);
    }
    let entries = fs::read_dir(archives_root).map_err(|e| {
        format!(
            "cannot read archives root `{}`: {e}",
            archives_root.display()
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("dir entry error: {e}"))?;
        let path = entry.path();
        if path.is_dir() && path.join("archive.toml").is_file() {
            if let Ok(text) = fs::read_to_string(path.join("archive.toml"))
                && let Ok(manifest) = ArchiveManifest::parse(&text)
                && manifest.stream == ReleaseStream::Edition
            {
                map.insert(manifest.tag.clone(), path);
            } else if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                // Even if archive.toml is corrupted, map by folder prefix `edition-NNN` so we report `unresolved`.
                if let Some(prefix) = name.get(..11)
                    && EditionId::parse(prefix).is_ok()
                {
                    map.entry(prefix.to_owned()).or_insert(path);
                }
            }
        }
    }
    Ok(map)
}

fn evaluate_tag_impact(
    repo_root: &Path,
    tag: &str,
    target_source_id: &SourceId,
    target_version: u32,
    target_vid: &VersionId,
    archive_dir_opt: Option<&Path>,
) -> EditionImpactRow {
    // 1. Resolve commit and parse structured `corpus/registers/sources.csv` at the tag.
    let commit = match proc::git(
        repo_root,
        &[
            "rev-parse",
            "--verify",
            &format!("refs/tags/{tag}^{{commit}}"),
        ],
    ) {
        Ok(c) => c.trim().to_owned(),
        Err(e) => {
            return EditionImpactRow {
                tag: tag.to_owned(),
                status: ImpactStatus::Unresolved,
                reason: format!("cannot resolve tag commit: {e}"),
            };
        }
    };

    let tagged_sources_bytes = match proc::git(
        repo_root,
        &["show", &format!("{commit}:corpus/registers/sources.csv")],
    ) {
        Ok(s) => s.into_bytes(),
        Err(e) => {
            return EditionImpactRow {
                tag: tag.to_owned(),
                status: ImpactStatus::Unresolved,
                reason: format!("cannot read tagged `corpus/registers/sources.csv`: {e}"),
            };
        }
    };

    let tagged_row_opt =
        match find_structured_source_row(&tagged_sources_bytes, target_source_id, target_version) {
            Ok(r) => r,
            Err(e) => {
                return EditionImpactRow {
                    tag: tag.to_owned(),
                    status: ImpactStatus::Unresolved,
                    reason: e,
                };
            }
        };

    // 2. Require and verify the corresponding durable archive pair (`missing archive never means absent`).
    let Some(archive_dir) = archive_dir_opt else {
        return EditionImpactRow {
            tag: tag.to_owned(),
            status: ImpactStatus::Unresolved,
            reason: format!("missing durable archive for `{tag}`"),
        };
    };

    let (manifest, _) = match verify_archive_directory_bytes(archive_dir, &[], &[]) {
        Ok(res) => res,
        Err(e) => {
            return EditionImpactRow {
                tag: tag.to_owned(),
                status: ImpactStatus::Unresolved,
                reason: format!("archive verification failed for `{tag}`: {e}"),
            };
        }
    };
    if manifest.tag != tag || manifest.commit != commit {
        return EditionImpactRow {
            tag: tag.to_owned(),
            status: ImpactStatus::Unresolved,
            reason: format!(
                "archive identity (`{}`, `{}`) does not match git tag `{tag}` (`{commit}`)",
                manifest.tag, manifest.commit
            ),
        };
    }

    // 3. Open and verify the archived edition and originals zips.
    let ed_num = &tag["edition-".len()..];
    let ed_zip_name = format!("fieldbook-edition-{ed_num}.zip");
    let orig_zip_name = format!("fieldbook-originals-{ed_num}.zip");
    let ed_zip_bytes = match read_non_symlink_file(archive_dir, &ed_zip_name) {
        Ok(b) => b,
        Err(e) => {
            return EditionImpactRow {
                tag: tag.to_owned(),
                status: ImpactStatus::Unresolved,
                reason: e,
            };
        }
    };
    let orig_zip_bytes = match read_non_symlink_file(archive_dir, &orig_zip_name) {
        Ok(b) => b,
        Err(e) => {
            return EditionImpactRow {
                tag: tag.to_owned(),
                status: ImpactStatus::Unresolved,
                reason: e,
            };
        }
    };
    let ed_entries = match read_and_verify_deterministic_zip(&ed_zip_bytes, "edition", None) {
        Ok(e) => e,
        Err(e) => {
            return EditionImpactRow {
                tag: tag.to_owned(),
                status: ImpactStatus::Unresolved,
                reason: e,
            };
        }
    };
    let orig_entries = match read_and_verify_deterministic_zip(&orig_zip_bytes, "originals", None) {
        Ok(e) => e,
        Err(e) => {
            return EditionImpactRow {
                tag: tag.to_owned(),
                status: ImpactStatus::Unresolved,
                reason: e,
            };
        }
    };

    let ed_map: BTreeMap<String, Vec<u8>> = ed_entries.into_iter().collect();
    let orig_map: BTreeMap<String, Vec<u8>> = orig_entries.into_iter().collect();

    // Ensure archived `sources.csv` matches the tagged `sources.csv` row for `(source_id, version)`.
    let Some(archived_sources_bytes) = ed_map.get("corpus/registers/sources.csv") else {
        return EditionImpactRow {
            tag: tag.to_owned(),
            status: ImpactStatus::Unresolved,
            reason: "archived edition zip is missing `corpus/registers/sources.csv`".to_owned(),
        };
    };
    let archived_row_opt = match find_structured_source_row(
        archived_sources_bytes,
        target_source_id,
        target_version,
    ) {
        Ok(r) => r,
        Err(e) => {
            return EditionImpactRow {
                tag: tag.to_owned(),
                status: ImpactStatus::Unresolved,
                reason: e,
            };
        }
    };
    if tagged_row_opt != archived_row_opt {
        return EditionImpactRow {
            tag: tag.to_owned(),
            status: ImpactStatus::Unresolved,
            reason: "tagged `sources.csv` row differs from archived `sources.csv` row".to_owned(),
        };
    }

    // 4. Inspect actual payload paths and hashes in `ed_map` and `orig_map`.
    let mut payload_hits: Vec<String> = Vec::new();
    let text_prefix = format!("corpus/sources/text/{target_vid}/");
    let site_source_page = format!("s/{target_vid}.html");

    for path in ed_map.keys() {
        if path.starts_with(&text_prefix) || path == &site_source_page {
            payload_hits.push(format!("edition/{path}"));
        }
    }
    for search_asset in ["assets/search-index.js", "assets/assist-data.js"] {
        if let Some(bytes) = ed_map.get(search_asset)
            && let Ok(text) = std::str::from_utf8(bytes)
            && text.contains(target_vid.as_str())
        {
            payload_hits.push(format!("edition/{search_asset}"));
        }
    }

    let orig_prefix = format!("{target_vid}.");
    for (orig_path, orig_bytes) in &orig_map {
        if orig_path == "SHA256SUMS" {
            continue;
        }
        if orig_path.starts_with(&orig_prefix) {
            payload_hits.push(format!("originals/{orig_path}"));
        } else if let Some(row) = &tagged_row_opt {
            if let Ok(expected_name) = shipped_original_filename(target_vid, &row.location)
                && orig_path == &expected_name
            {
                payload_hits.push(format!("originals/{orig_path}"));
            } else if sha256_hex(orig_bytes) == row.sha256 {
                payload_hits.push(format!("originals/{orig_path} (matching sha256)"));
            }
        }
    }

    if !payload_hits.is_empty() {
        return EditionImpactRow {
            tag: tag.to_owned(),
            status: ImpactStatus::Containing,
            reason: format!("payload present in {}", payload_hits.join(", ")),
        };
    }

    match tagged_row_opt {
        None => EditionImpactRow {
            tag: tag.to_owned(),
            status: ImpactStatus::Absent,
            reason: format!(
                "`{target_vid}` is not in `sources.csv` and no payload exists in the verified archive"
            ),
        },
        Some(row) if row.lifecycle == Lifecycle::Quarantined => EditionImpactRow {
            tag: tag.to_owned(),
            status: ImpactStatus::Absent,
            reason: format!(
                "`{target_vid}` is `quarantined` in `sources.csv` (retaining only its register row/hash/note) and no payload exists in the verified archive"
            ),
        },
        Some(_) => EditionImpactRow {
            tag: tag.to_owned(),
            status: ImpactStatus::Absent,
            reason: format!(
                "`{target_vid}` has no original, extraction, source page, or search payload in the verified archive"
            ),
        },
    }
}

/// Parse `sources.csv` using structured columns (`source_id` and `version`), ignoring
/// `supersedes` and other free-text columns.
fn find_structured_source_row(
    sources_csv_bytes: &[u8],
    target_source_id: &SourceId,
    target_version: u32,
) -> Result<Option<SourceRow>, String> {
    let tbl = table::read(sources_csv_bytes).map_err(|e| format!("invalid `sources.csv`: {e}"))?;
    for row in &tbl.rows {
        let sr = SourceRow::from_row(row).map_err(|errs| {
            format!("invalid `sources.csv` row at line {}: {:?}", row.line, errs)
        })?;
        // Match strictly on structured `source_id` and `version` columns (never `supersedes`).
        if &sr.source_id == target_source_id && sr.version == target_version {
            return Ok(Some(sr));
        }
    }
    Ok(None)
}

/// CLI entry point for `cargo xtask archive-release ...`.
pub fn command_archive_release(args: &[&str]) -> Result<bool, String> {
    let parsed = parse_archive_release_cli(args)?;
    let (folder, manifest, sha) = archive_release(&parsed)?;
    println!(
        "archived {} ({}) at {} (archive.toml sha256: {sha})",
        manifest.tag,
        manifest.stream,
        folder.display()
    );
    Ok(true)
}

/// CLI entry point for `cargo xtask verify-archive <folder> --restore-to <empty-folder> [--previous <previous-archive>]`.
pub fn command_verify_archive(args: &[&str]) -> Result<bool, String> {
    let (archive_dir, restore_to, previous) = parse_verify_archive_cli(args)?;
    let report = verify_archive(&archive_dir, &restore_to, previous.as_deref())?;
    if let Some(pred) = &report.verified_predecessor_tag {
        println!(
            "verified {} ({}) with predecessor {pred}",
            report.manifest.tag, report.manifest.stream
        );
    } else if report.used_first_tool_rehearsal {
        println!(
            "verified {} (tool, explicit empty baseline: current package + synthetic restore fixture)",
            report.manifest.tag
        );
    } else {
        println!(
            "verified {} ({}, explicit empty baseline)",
            report.manifest.tag, report.manifest.stream
        );
    }
    Ok(true)
}

/// CLI entry point for `cargo xtask edition-impact --source <source-id> --version <number> --archives <verified-archive-root>`.
pub fn command_edition_impact(default_root: &Path, args: &[&str]) -> Result<bool, String> {
    let (source_id, version, archives) = parse_edition_impact_cli(args)?;
    let repo_root = if let Ok(cwd) = std::env::current_dir()
        && cwd.join("corpus/corpus.toml").exists()
    {
        cwd
    } else {
        default_root.to_path_buf()
    };
    let rows = edition_impact(&repo_root, &source_id, version, &archives)?;
    for row in &rows {
        println!("{}\t{}\t{}", row.tag, row.status, row.reason);
    }
    Ok(true)
}

pub fn parse_archive_release_cli(args: &[&str]) -> Result<ArchiveArgs, String> {
    let mut kind: Option<DeliveryKind> = None;
    let mut package: Option<PathBuf> = None;
    let mut edition: Option<PathBuf> = None;
    let mut intake: Option<PathBuf> = None;
    let mut evidence: Option<PathBuf> = None;
    let mut base: Option<PathBuf> = None;
    let mut to: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "--kind" => {
                let Some(val) = args.get(i + 1) else {
                    return Err("missing value for `--kind`".to_owned());
                };
                kind = Some(DeliveryKind::parse(val)?);
                i += 2;
            }
            "--package" => {
                let Some(val) = args.get(i + 1) else {
                    return Err("missing value for `--package`".to_owned());
                };
                package = Some(PathBuf::from(val));
                i += 2;
            }
            "--edition" => {
                let Some(val) = args.get(i + 1) else {
                    return Err("missing value for `--edition`".to_owned());
                };
                edition = Some(PathBuf::from(val));
                i += 2;
            }
            "--intake" => {
                let Some(val) = args.get(i + 1) else {
                    return Err("missing value for `--intake`".to_owned());
                };
                intake = Some(PathBuf::from(val));
                i += 2;
            }
            "--evidence" => {
                let Some(val) = args.get(i + 1) else {
                    return Err("missing value for `--evidence`".to_owned());
                };
                evidence = Some(PathBuf::from(val));
                i += 2;
            }
            "--base" => {
                let Some(val) = args.get(i + 1) else {
                    return Err("missing value for `--base`".to_owned());
                };
                base = Some(PathBuf::from(val));
                i += 2;
            }
            "--to" => {
                let Some(val) = args.get(i + 1) else {
                    return Err("missing value for `--to`".to_owned());
                };
                to = Some(PathBuf::from(val));
                i += 2;
            }
            other => {
                return Err(format!("unknown `archive-release` argument `{other}`"));
            }
        }
    }

    let edition =
        edition.ok_or_else(|| "`archive-release` requires `--edition <dir>`".to_owned())?;
    let to = to.ok_or_else(|| "`archive-release` requires `--to <dir>`".to_owned())?;

    Ok(ArchiveArgs {
        kind,
        package,
        edition,
        intake,
        evidence,
        base,
        to,
    })
}

pub fn parse_verify_archive_cli(
    args: &[&str],
) -> Result<(PathBuf, PathBuf, Option<PathBuf>), String> {
    let Some(&first) = args.first() else {
        return Err("`verify-archive` requires `<folder>`".to_owned());
    };
    if first.starts_with("--") {
        return Err("`verify-archive` requires `<folder>` as first argument".to_owned());
    }
    let archive_dir = PathBuf::from(first);
    let mut restore_to: Option<PathBuf> = None;
    let mut previous: Option<PathBuf> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i] {
            "--restore-to" => {
                let Some(val) = args.get(i + 1) else {
                    return Err("missing value for `--restore-to`".to_owned());
                };
                restore_to = Some(PathBuf::from(val));
                i += 2;
            }
            "--previous" => {
                let Some(val) = args.get(i + 1) else {
                    return Err("missing value for `--previous`".to_owned());
                };
                previous = Some(PathBuf::from(val));
                i += 2;
            }
            other => {
                return Err(format!("unknown `verify-archive` argument `{other}`"));
            }
        }
    }

    let restore_to =
        restore_to.ok_or_else(|| "`verify-archive` requires `--restore-to <dir>`".to_owned())?;
    Ok((archive_dir, restore_to, previous))
}

pub fn parse_edition_impact_cli(args: &[&str]) -> Result<(String, u32, PathBuf), String> {
    let mut source: Option<String> = None;
    let mut version: Option<u32> = None;
    let mut archives: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "--source" => {
                let Some(val) = args.get(i + 1) else {
                    return Err("missing value for `--source`".to_owned());
                };
                source = Some((*val).to_owned());
                i += 2;
            }
            "--version" => {
                let Some(val) = args.get(i + 1) else {
                    return Err("missing value for `--version`".to_owned());
                };
                let parsed = val
                    .parse::<u32>()
                    .map_err(|_| format!("invalid `--version {val}`: expected positive integer"))?;
                version = Some(parsed);
                i += 2;
            }
            "--archives" => {
                let Some(val) = args.get(i + 1) else {
                    return Err("missing value for `--archives`".to_owned());
                };
                archives = Some(PathBuf::from(val));
                i += 2;
            }
            other => {
                return Err(format!("unknown `edition-impact` argument `{other}`"));
            }
        }
    }

    let source =
        source.ok_or_else(|| "`edition-impact` requires `--source <source-id>`".to_owned())?;
    let version =
        version.ok_or_else(|| "`edition-impact` requires `--version <number>`".to_owned())?;
    let archives =
        archives.ok_or_else(|| "`edition-impact` requires `--archives <dir>`".to_owned())?;
    Ok((source, version, archives))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::path::{Path, PathBuf};

    use fieldbook_core::audit::Now;
    use fieldbook_core::authz::ReadableStores;
    use fieldbook_core::check::{Entry, Host, Options, Tree, load};
    use fieldbook_core::date::CivilDate;
    use fieldbook_core::edition::{BuildInfo, BuildProfile, EditionInfo};
    use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check as egress_check};
    use fieldbook_core::hash::sha256_hex;
    use fieldbook_core::ids::VersionId;
    use fieldbook_core::manifest::{SupportCatalogue, shipped_corpus, write_sums};
    use fieldbook_core::policy::EffectivePolicy;
    use fieldbook_core::schema::Register;
    use tempfile::TempDir;

    use crate::compat::{Baselines, parse_baselines, parse_published};
    use crate::proc;
    use crate::zipdet::write_deterministic_zip_bytes;

    use super::{
        ArchiveArgs, ArchiveFileEntry, ArchiveManifest, DeliveryKind, ImpactStatus, ReleaseStream,
        advance_baselines_from_archive, archive_release, collect_release_evidence, edition_impact,
        parse_edition_release_mode, register_archive_provider, serialize_baselines,
        validate_archive_rel_path, validate_edition_tag, verify_archive,
    };

    const MINI_CORPUS_TOML: &str =
        include_str!("../../tests/fixtures/corpus-mini/corpus/corpus.toml");

    fn synthetic_provider_hook(
        archive_dir: &Path,
        restore_dir: &Path,
        manifest: &ArchiveManifest,
    ) -> Result<(), String> {
        fs::write(
            restore_dir.join("restored-tag.txt"),
            format!("{}:{}:{}\n", manifest.stream, manifest.kind, manifest.tag),
        )
        .map_err(|e| e.to_string())?;
        assert!(archive_dir.join("archive.toml").is_file());
        Ok(())
    }

    fn install_synthetic_providers() {
        register_archive_provider(
            ReleaseStream::Edition,
            DeliveryKind::Raw,
            synthetic_provider_hook,
        )
        .expect("register raw edition");
        register_archive_provider(
            ReleaseStream::Edition,
            DeliveryKind::Site,
            synthetic_provider_hook,
        )
        .expect("register site edition");
        register_archive_provider(
            ReleaseStream::Tool,
            DeliveryKind::Site,
            synthetic_provider_hook,
        )
        .expect("register tool");
    }

    fn build_synthetic_evidence_dir(
        dir: &Path,
        baselines: &Baselines,
        mode_toml: Option<&str>,
        vector_payload: &[u8],
    ) -> PathBuf {
        let src_root = dir.join("compat-src");
        fs::create_dir_all(src_root.join("compat/calc")).expect("mkdir");
        let contract_bytes = b"# synthetic contract\n";
        let impl_bytes = b"// synthetic impl\n";
        let input_bytes = vector_payload;
        let expected_bytes = b"expected = 1\n";

        fs::write(src_root.join("compat/calc/contract.md"), contract_bytes).expect("write");
        fs::write(src_root.join("compat/calc/impl.rs"), impl_bytes).expect("write");
        fs::write(src_root.join("compat/calc/in.toml"), input_bytes).expect("write");
        fs::write(src_root.join("compat/calc/out.toml"), expected_bytes).expect("write");

        let published_toml = format!(
            "compat_schema = 1\ncalculation_epoch = 1\ncase_set_sha256 = \"{}\"\n\n[[binding]]\nkind = \"calculation\"\nid = \"build_up@1\"\nrunner = \"calculation\"\nfiles = [\n  {{ role = \"contract\", path = \"compat/calc/contract.md\", sha256 = \"{}\" }},\n  {{ role = \"expected\", case = \"calc-01\", path = \"compat/calc/out.toml\", sha256 = \"{}\" }},\n  {{ role = \"implementation\", path = \"compat/calc/impl.rs\", sha256 = \"{}\" }},\n  {{ role = \"input\", case = \"calc-01\", path = \"compat/calc/in.toml\", sha256 = \"{}\" }},\n]\n",
            "11".repeat(32),
            sha256_hex(contract_bytes),
            sha256_hex(expected_bytes),
            sha256_hex(impl_bytes),
            sha256_hex(input_bytes),
        );
        let published = parse_published(&published_toml).expect("valid published.toml");
        let bound = published.bound_files().expect("bound_files");
        let base_toml = serialize_baselines(baselines);

        let ev_out = dir.join("evidence-out");
        collect_release_evidence(
            &src_root,
            published_toml.as_bytes(),
            &bound,
            base_toml.as_bytes(),
            mode_toml.map(str::as_bytes),
            &ev_out,
        )
        .expect("collect_release_evidence");
        ev_out
    }

    #[derive(Clone)]
    struct SyntheticSourceSpec {
        source_id: &'static str,
        version: u32,
        lifecycle: &'static str,
        supersedes: &'static str,
        note: &'static str,
        ship_payload: bool,
    }

    fn build_synthetic_edition_set(
        out_dir: &Path,
        edition_num: &str,
        commit_sha: &str,
        kind: DeliveryKind,
        sources: &[SyntheticSourceSpec],
    ) {
        fs::create_dir_all(out_dir).expect("mkdir edition set");
        let epoch = 1_773_532_800_i64;
        let today = CivilDate::from_ymd(2026, 3, 15).expect("date");
        let assistant_name = fieldbook_core::strings::paths()[6];

        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes());
        tree.insert_bytes(format!("corpus/{assistant_name}"), b"rules_version = 1\n");
        tree.insert_bytes(
            "corpus/changes.md",
            format!(
                "# Corpus changes\n\nSynthetic changes log.\n\n## edition-{edition_num}\n\n- Added: synthetic.\n"
            )
            .into_bytes(),
        );
        tree.insert_dir("corpus/registers");
        for reg in Register::ALL {
            let mut header = String::new();
            for (idx, col) in reg.columns().iter().enumerate() {
                if idx > 0 {
                    header.push(',');
                }
                header.push_str(col.name);
            }
            header.push('\n');
            tree.insert_bytes(
                format!("corpus/registers/{}", reg.file_name()),
                header.into_bytes(),
            );
        }

        let mut sources_csv = String::from(
            "source_id,version,tag,title,publisher,source_kind,source_role,authority_tier,licence,source_family,family_basis,location,sha256,published,effective_from,as_of,retrieved,lifecycle,supersedes,volatility,handling,note,purpose,origin_system,origin_stream,origin_key,doc_kind,series\n",
        );
        let mut quarantined = BTreeSet::new();
        let mut orig_files: BTreeMap<String, Vec<u8>> = BTreeMap::new();

        for (idx, spec) in sources.iter().enumerate() {
            let vid_str = format!("{}-v{}", spec.source_id, spec.version);
            let vid = VersionId::parse(&vid_str).expect("valid vid");
            let orig_bytes = format!("original payload for {vid_str}\n").into_bytes();
            let orig_sha = sha256_hex(&orig_bytes);
            let tag_slug = format!("syn-{}", idx + 1);
            let family = format!("syn_{}", idx + 1);
            sources_csv.push_str(&format!(
                "{},{},{tag_slug},Synthetic Source {vid_str},Synthetic Board,primary_law,primary_evidence,A,Synthetic,{family},reviewed,{vid_str}.txt,{orig_sha},2026-01-10,2026-01-15,2026-02-01,2026-02-01,{},{},V2,public,{},Baseline,,,,,\n",
                spec.source_id, spec.version, spec.lifecycle, spec.supersedes, spec.note
            ));
            if spec.lifecycle == "quarantined" {
                quarantined.insert(vid.clone());
            }
            if spec.ship_payload {
                tree.insert(
                    format!("corpus/sources/public/{vid_str}.txt"),
                    Entry::Original {
                        sha256: orig_sha.clone(),
                        len: orig_bytes.len() as u64,
                    },
                );
                let page_txt = format!("extracted text for {vid_str}\n").into_bytes();
                let page_sha = sha256_hex(&page_txt);
                let ext_toml = format!(
                    "tool = \"plain\"\ntool_version = \"1\"\nmethod = \"plain\"\npage_count = 1\nthin_pages = []\noriginal_sha256 = \"{orig_sha}\"\n\n[pages]\np0001 = \"{page_sha}\"\n"
                );
                tree.insert_bytes(
                    format!("corpus/sources/text/{vid_str}/extraction.toml"),
                    ext_toml.into_bytes(),
                );
                tree.insert_bytes(format!("corpus/sources/text/{vid_str}/p0001.txt"), page_txt);
                orig_files.insert(format!("{vid_str}.txt"), orig_bytes);
            }
        }

        tree.insert_bytes("corpus/registers/sources.csv", sources_csv.into_bytes());
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-sample.md",
            b"+++\nid = \"cmp-sample\"\ntitle = \"Synthetic Title\"\ntype = \"concept\"\narea = \"cmp\"\nsubarea = \"business-systems\"\nstage = 3\nstatus = \"draft\"\nreadiness = \"SYNTHETIC-NOW\"\nas_of = 2026-02-01\nhandling = [\"public\"]\nvolatility = \"V2\"\n+++\n\n## Purpose\n\nSynthetic.\n\n## The rule\n\nSynthetic.\n\n## How it works\n\nSynthetic.\n\n## Worked example\n\nSynthetic example. The numbers are made up.\n\n## Never confuse\n\nSynthetic.\n\n## Company calibration\n\nSynthetic.\n\n## Sources\n\n## Change note\n\n- NEW_KNOWLEDGE, 2026-02-01: Synthetic.\n",
        );
        tree.insert_bytes("corpus/eval/regression.toml", b"case = []\n");

        let (model, _) = load(&Host::new(tree, today), &Options::default());
        let eff = EffectivePolicy::floor(false);
        let now = Now::new(today, 0).expect("now");
        let Decision::Allowed(allowed) =
            egress_check(&eff, &Dest::Edition, None, None, now, Shipped::Hours(0))
        else {
            panic!("edition allowed");
        };
        let view = LabelledView::new(&model, &allowed, &ReadableStores::corpus());
        let shipped = shipped_corpus(&view, &quarantined).expect("shipped_corpus");
        let tag_str = format!("edition-{edition_num}");
        let build_info = BuildInfo::new(
            Some(today),
            Some(commit_sha.to_owned()),
            BuildProfile::Silent,
            Some(epoch),
        );
        let ed_info = EditionInfo::from(&view, &shipped, Some(&tag_str), &build_info, "0.1.0", 1)
            .expect("EditionInfo");
        let (support_bytes, support_sha) = SupportCatalogue::from_public_source(&view, commit_sha)
            .expect("SupportCatalogue")
            .to_toml_bytes_and_sha256();

        let mut ed_files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for (p, _) in &shipped {
            ed_files.insert(p.clone(), view.tree().get_bytes(p).expect("bytes").to_vec());
        }
        ed_files.insert(assistant_name.to_owned(), b"rules_version = 1\n".to_vec());
        ed_files.insert("README.txt".to_owned(), b"Synthetic README\n".to_vec());
        ed_files.insert(
            "TRANSFER-STATEMENT.txt".to_owned(),
            b"Synthetic transfer statement\n".to_vec(),
        );
        ed_files.insert("edition.toml".to_owned(), ed_info.to_toml().into_bytes());
        ed_files.insert(
            "evidence/corpus-support.toml".to_owned(),
            support_bytes.clone(),
        );
        ed_files.insert(
            "intake/ai-development-statement.md".to_owned(),
            b"# AI\n".to_vec(),
        );
        ed_files.insert("intake/data-handling.md".to_owned(), b"# Data\n".to_vec());
        ed_files.insert(
            "intake/integrity-guide.md".to_owned(),
            b"# Integrity\n".to_vec(),
        );
        ed_files.insert(
            "intake/network-silence.md".to_owned(),
            b"# Network\n".to_vec(),
        );
        match kind {
            DeliveryKind::Raw => {
                ed_files.insert(
                    "corpus/eval/regression.toml".to_owned(),
                    b"case = []\n".to_vec(),
                );
                ed_files.insert("evidence/check.json".to_owned(), b"{}\n".to_vec());
                ed_files.insert("evidence/regression.json".to_owned(), b"{}\n".to_vec());
            }
            DeliveryKind::Site => {
                ed_files.insert(
                    "intake/network-silence-summary.txt".to_owned(),
                    b"status: pass\n".to_vec(),
                );
                ed_files.insert("index.html".to_owned(), b"<!doctype html>\n".to_vec());
                ed_files.insert("about.html".to_owned(), b"<!doctype html>\n".to_vec());
            }
        }
        let ed_sums = write_sums(
            &ed_files
                .iter()
                .map(|(p, b)| (p.clone(), sha256_hex(b)))
                .collect::<Vec<_>>(),
        );
        ed_files.insert("SHA256SUMS".to_owned(), ed_sums.into_bytes());

        if orig_files.is_empty() {
            // Even when all sources are quarantined, originals zip has a minimal valid SHA256SUMS or active source.
        }
        let orig_sums = write_sums(
            &orig_files
                .iter()
                .map(|(p, b)| (p.clone(), sha256_hex(b)))
                .collect::<Vec<_>>(),
        );
        orig_files.insert("SHA256SUMS".to_owned(), orig_sums.into_bytes());

        let ed_zip_name = format!("fieldbook-edition-{edition_num}.zip");
        let orig_zip_name = format!("fieldbook-originals-{edition_num}.zip");
        let ed_zip_bytes =
            write_deterministic_zip_bytes("edition", epoch, ed_files).expect("ed zip");
        let orig_zip_bytes =
            write_deterministic_zip_bytes("originals", epoch, orig_files).expect("orig zip");

        fs::write(out_dir.join(&ed_zip_name), &ed_zip_bytes).expect("write ed zip");
        fs::write(out_dir.join(&orig_zip_name), &orig_zip_bytes).expect("write orig zip");

        let outer_sums = write_sums(&[
            (ed_zip_name, sha256_hex(&ed_zip_bytes)),
            (orig_zip_name, sha256_hex(&orig_zip_bytes)),
        ]);
        fs::write(out_dir.join("SHA256SUMS"), outer_sums).expect("write outer sums");
        fs::write(out_dir.join("corpus-support.toml"), &support_bytes).expect("write support");
        let support_sha_line = write_sums(&[("corpus-support.toml".to_owned(), support_sha)]);
        fs::write(out_dir.join("corpus-support.sha256"), support_sha_line)
            .expect("write support.sha256");
    }

    fn init_git_repo(dir: &Path) {
        proc::git(dir, &["init", "-b", "main"]).expect("git init");
        proc::git(dir, &["config", "user.name", "Test Owner"]).expect("git config name");
        proc::git(dir, &["config", "user.email", "owner@example.invalid"])
            .expect("git config email");
    }

    #[test]
    fn mode_descriptor_and_tag_validation_enforce_heading_ancestry_and_kind() {
        // 1. Mode descriptor parsing rules.
        let default_site =
            parse_edition_release_mode(None, Some("001")).expect("absent means site");
        assert_eq!(default_site.edition, "001");
        assert_eq!(default_site.kind, DeliveryKind::Site);

        let raw_mode = parse_edition_release_mode(
            Some("release_schema = 1\nedition = \"001\"\nkind = \"raw\"\n"),
            Some("001"),
        )
        .expect("valid raw mode");
        assert_eq!(raw_mode.kind, DeliveryKind::Raw);

        // Wrong edition number, unknown key, or bad schema must refuse.
        assert!(
            parse_edition_release_mode(
                Some("release_schema = 1\nedition = \"001\"\nkind = \"raw\"\n"),
                Some("002"),
            )
            .is_err()
        );
        assert!(
            parse_edition_release_mode(
                Some("release_schema = 1\nedition = \"001\"\nkind = \"raw\"\nextra = true\n"),
                Some("001"),
            )
            .is_err()
        );

        // 2. Git tag validation on temporary repo.
        let tmp = TempDir::new().expect("tempdir");
        let repo = tmp.path();
        init_git_repo(repo);
        fs::create_dir_all(repo.join("corpus")).expect("mkdir corpus");
        fs::write(
            repo.join("corpus/changes.md"),
            "# Corpus changes\n\nSynthetic changes.\n\n## edition-001\n\n- Added: initial.\n",
        )
        .expect("write changes.md");
        fs::write(
            repo.join("edition-release.toml"),
            "release_schema = 1\nedition = \"001\"\nkind = \"raw\"\n",
        )
        .expect("write mode");
        proc::git(repo, &["add", "."]).expect("git add");
        proc::git(repo, &["commit", "-m", "edition 001"]).expect("git commit");
        proc::git(repo, &["tag", "edition-001"]).expect("git tag");

        let v = validate_edition_tag(repo, "edition-001", Some(DeliveryKind::Raw))
            .expect("valid edition-001 tag");
        assert_eq!(v.edition_number, "001");
        assert_eq!(v.kind, DeliveryKind::Raw);

        // Mismatched expected kind (`Site` when committed is `Raw`) must fail.
        assert!(validate_edition_tag(repo, "edition-001", Some(DeliveryKind::Site)).is_err());

        // Mismatched tag number (`edition-002` when newest heading is `edition-001`) must fail.
        proc::git(repo, &["tag", "edition-002"]).expect("git tag 002");
        assert!(validate_edition_tag(repo, "edition-002", None).is_err());

        // Tag on a branch not in `main` ancestry must fail.
        proc::git(repo, &["checkout", "-b", "side"]).expect("checkout side");
        fs::write(
            repo.join("corpus/changes.md"),
            "# Corpus changes\n\nSynthetic changes.\n\n## edition-003\n\n- Added: side.\n",
        )
        .expect("write changes");
        fs::write(
            repo.join("edition-release.toml"),
            "release_schema = 1\nedition = \"003\"\nkind = \"raw\"\n",
        )
        .expect("write mode 003");
        proc::git(repo, &["add", "."]).expect("add");
        proc::git(repo, &["commit", "-m", "side 003"]).expect("commit");
        proc::git(repo, &["tag", "edition-003"]).expect("tag 003");
        let err = validate_edition_tag(repo, "edition-003", None)
            .expect_err("non-main tag must fail ancestry check");
        assert!(err.contains("not an ancestor"), "{err}");
    }

    #[test]
    fn archive_manifest_and_dispatcher_reject_unsafe_paths_tampered_bytes_and_missing_predecessors()
    {
        install_synthetic_providers();

        // 1. Unsafe relative paths refuse.
        for bad in [
            "",
            "/abs/path",
            "../up",
            "a//b",
            "a/./b",
            "back\\slash",
            "C:drive",
        ] {
            assert!(
                validate_archive_rel_path(bad).is_err(),
                "unsafe path `{bad}` must fail"
            );
        }

        // 2. Raw with tool stream refuses in ArchiveManifest::validate.
        let bad_manifest = ArchiveManifest {
            archive_schema: 1,
            kind: DeliveryKind::Raw,
            stream: ReleaseStream::Tool,
            tag: "v0.1.0".to_owned(),
            commit: "11".repeat(20),
            baseline_sha256: "22".repeat(32),
            files: vec![ArchiveFileEntry {
                path: "a.txt".to_owned(),
                size: 1,
                sha256: "33".repeat(32),
            }],
        };
        assert!(bad_manifest.validate().is_err());

        // 3. Archive a clean Raw edition with an explicit empty baseline and verify it.
        let tmp = TempDir::new().expect("tempdir");
        let empty_baselines = parse_baselines(
            "baseline_schema = 1\n\n[compatibility]\nkind = \"empty\"\n\n[tool]\nkind = \"empty\"\n\n[edition]\nkind = \"empty\"\n",
        )
        .expect("empty baselines");
        let commit_1 = "1111111111111111111111111111111111111111";
        let mode_1 = "release_schema = 1\nedition = \"001\"\nkind = \"raw\"\n";
        let ev_1 = build_synthetic_evidence_dir(
            &tmp.path().join("ev1"),
            &empty_baselines,
            Some(mode_1),
            b"vector = 1\n",
        );
        let ed_set_1 = tmp.path().join("ed1");
        build_synthetic_edition_set(
            &ed_set_1,
            "001",
            commit_1,
            DeliveryKind::Raw,
            &[SyntheticSourceSpec {
                source_id: "S-0001",
                version: 1,
                lifecycle: "active",
                supersedes: "",
                note: "Active.",
                ship_payload: true,
            }],
        );

        let archives_root = tmp.path().join("archives");
        let (arch_1_dir, _, _) = archive_release(&ArchiveArgs {
            kind: Some(DeliveryKind::Raw),
            package: None,
            edition: ed_set_1,
            intake: None,
            evidence: Some(ev_1),
            base: None,
            to: archives_root.clone(),
        })
        .expect("archive_release 001");

        let restore_1 = tmp.path().join("restore1");
        let rep_1 =
            verify_archive(&arch_1_dir, &restore_1, None).expect("verify first raw archive");
        assert_eq!(rep_1.verified_predecessor_tag, None);
        assert!(restore_1.join("current/restored-tag.txt").is_file());

        // 4. Tampering with a copied byte inside `arch_1_dir` MUST fail reopened verification.
        let readme_in_ev = arch_1_dir.join("evidence/compat/calc/contract.md");
        let orig_contract = fs::read(&readme_in_ev).expect("read contract");
        fs::write(&readme_in_ev, b"tampered contract\n").expect("tamper");
        let restore_tampered = tmp.path().join("restore-tampered");
        assert!(
            verify_archive(&arch_1_dir, &restore_tampered, None).is_err(),
            "tampered archived byte must fail verify_archive"
        );
        fs::write(&readme_in_ev, orig_contract).expect("restore contract");
    }

    #[test]
    fn mixed_stream_lifecycle_preserves_independent_tool_and_edition_predecessors() {
        install_synthetic_providers();
        let tmp = TempDir::new().expect("tempdir");
        let archives_root = tmp.path().join("archives");

        // Initial empty baseline.
        let b0 = parse_baselines(
            "baseline_schema = 1\n\n[compatibility]\nkind = \"empty\"\n\n[tool]\nkind = \"empty\"\n\n[edition]\nkind = \"empty\"\n",
        )
        .expect("b0");

        // Step A: Raw edition `edition-001` (compatibility = empty, tool = empty, edition = empty).
        let commit_e1 = "1111111111111111111111111111111111111111";
        let ev_e1 = build_synthetic_evidence_dir(
            &tmp.path().join("ev-e1"),
            &b0,
            Some("release_schema = 1\nedition = \"001\"\nkind = \"raw\"\n"),
            b"vector = 1\n",
        );
        let ed_e1 = tmp.path().join("ed-e1");
        build_synthetic_edition_set(
            &ed_e1,
            "001",
            commit_e1,
            DeliveryKind::Raw,
            &[SyntheticSourceSpec {
                source_id: "S-0001",
                version: 1,
                lifecycle: "active",
                supersedes: "",
                note: "Active.",
                ship_payload: true,
            }],
        );
        let (arch_e1, _, _) = archive_release(&ArchiveArgs {
            kind: Some(DeliveryKind::Raw),
            package: None,
            edition: ed_e1,
            intake: None,
            evidence: Some(ev_e1),
            base: None,
            to: archives_root.clone(),
        })
        .expect("archive edition-001");

        // Advance baseline after `edition-001`: compatibility -> edition-001, edition -> edition-001, tool stays empty!
        let b1 = advance_baselines_from_archive(&b0, &arch_e1).expect("advance b1");
        assert_eq!(
            b1.compatibility.as_ref().map(|c| c.tag.as_str()),
            Some("edition-001")
        );
        assert_eq!(
            b1.edition.as_ref().map(|e| e.tag.as_str()),
            Some("edition-001")
        );
        assert!(
            b1.tool.is_none(),
            "tool baseline must remain empty after edition-001"
        );

        // Step B: First tool release `v0.1.0` after raw `edition-001`.
        // Because `b1.tool` is still `None`, `verify_archive` for `v0.1.0` succeeds WITHOUT `--previous`
        // and runs the first-tool rehearsal, even though `compatibility` and `edition` are non-empty.
        let commit_t1 = "2222222222222222222222222222222222222222";
        let ev_t1 =
            build_synthetic_evidence_dir(&tmp.path().join("ev-t1"), &b1, None, b"vector = 2\n");
        let ed_t1 = tmp.path().join("ed-t1");
        build_synthetic_edition_set(
            &ed_t1,
            "001",
            commit_t1,
            DeliveryKind::Site,
            &[SyntheticSourceSpec {
                source_id: "S-0001",
                version: 1,
                lifecycle: "active",
                supersedes: "",
                note: "Active.",
                ship_payload: true,
            }],
        );
        let pkg_t1 = tmp.path().join("pkg-t1");
        fs::create_dir_all(&pkg_t1).expect("mkdir pkg-t1");
        fs::write(
            pkg_t1.join("package.toml"),
            format!("tag = \"v0.1.0\"\ncommit = \"{commit_t1}\"\n"),
        )
        .expect("write package.toml");
        let (arch_t1, _, _) = archive_release(&ArchiveArgs {
            kind: Some(DeliveryKind::Site),
            package: Some(pkg_t1),
            edition: ed_t1,
            intake: None,
            evidence: Some(ev_t1),
            base: None,
            to: archives_root.clone(),
        })
        .expect("archive v0.1.0");

        let rep_t1 = verify_archive(&arch_t1, &tmp.path().join("restore-t1"), None)
            .expect("first tool release verifies with empty tool baseline");
        assert!(rep_t1.used_first_tool_rehearsal);
        assert_eq!(rep_t1.verified_predecessor_tag, None);

        // Advance baseline after `v0.1.0`: compatibility -> v0.1.0, tool -> v0.1.0, edition preserved as `edition-001`!
        let b2 = advance_baselines_from_archive(&b1, &arch_t1).expect("advance b2");
        assert_eq!(
            b2.compatibility.as_ref().map(|c| c.tag.as_str()),
            Some("v0.1.0")
        );
        assert_eq!(b2.tool.as_ref().map(|t| t.tag.as_str()), Some("v0.1.0"));
        assert_eq!(
            b2.edition.as_ref().map(|e| e.tag.as_str()),
            Some("edition-001")
        );

        // Step C: Edition update `edition-002` after `v0.1.0`.
        // Its predecessor is `edition-001` (from `b2.edition`), NOT `v0.1.0`, and `b3.tool` stays `v0.1.0`!
        let commit_e2 = "3333333333333333333333333333333333333333";
        let ev_e2 = build_synthetic_evidence_dir(
            &tmp.path().join("ev-e2"),
            &b2,
            Some("release_schema = 1\nedition = \"002\"\nkind = \"site\"\n"),
            b"vector = 3\n",
        );
        let ed_e2 = tmp.path().join("ed-e2");
        build_synthetic_edition_set(
            &ed_e2,
            "002",
            commit_e2,
            DeliveryKind::Site,
            &[SyntheticSourceSpec {
                source_id: "S-0001",
                version: 1,
                lifecycle: "active",
                supersedes: "",
                note: "Active.",
                ship_payload: true,
            }],
        );
        let (arch_e2, _, _) = archive_release(&ArchiveArgs {
            kind: Some(DeliveryKind::Site),
            package: None,
            edition: ed_e2,
            intake: None,
            evidence: Some(ev_e2),
            base: None,
            to: archives_root.clone(),
        })
        .expect("archive edition-002");

        // Omitting `--previous` when `b2.edition` is declared (`edition-001`) MUST fail!
        assert!(
            verify_archive(&arch_e2, &tmp.path().join("restore-e2-missing"), None).is_err(),
            "missing declared edition predecessor must fail"
        );
        let rep_e2 = verify_archive(
            &arch_e2,
            &tmp.path().join("restore-e2"),
            Some(arch_e1.as_path()),
        )
        .expect("edition-002 verifies with edition-001 predecessor");
        assert_eq!(
            rep_e2.verified_predecessor_tag.as_deref(),
            Some("edition-001")
        );

        // Advance baseline after `edition-002`: compatibility -> edition-002, edition -> edition-002, tool preserved as `v0.1.0`!
        let b3 = advance_baselines_from_archive(&b2, &arch_e2).expect("advance b3");
        assert_eq!(
            b3.compatibility.as_ref().map(|c| c.tag.as_str()),
            Some("edition-002")
        );
        assert_eq!(
            b3.edition.as_ref().map(|e| e.tag.as_str()),
            Some("edition-002")
        );
        assert_eq!(
            b3.tool.as_ref().map(|t| t.tag.as_str()),
            Some("v0.1.0"),
            "intervening edition release must preserve tool binding `v0.1.0`"
        );

        // Step D: Next tool release `v0.2.0` after `edition-002`.
        // Must select `v0.1.0` (`arch_t1`) as predecessor, and reject `edition-002` (`arch_e2`) or `None`.
        let commit_t2 = "4444444444444444444444444444444444444444";
        let ev_t2 =
            build_synthetic_evidence_dir(&tmp.path().join("ev-t2"), &b3, None, b"vector = 4\n");
        let ed_t2 = tmp.path().join("ed-t2");
        build_synthetic_edition_set(
            &ed_t2,
            "002",
            commit_t2,
            DeliveryKind::Site,
            &[SyntheticSourceSpec {
                source_id: "S-0001",
                version: 1,
                lifecycle: "active",
                supersedes: "",
                note: "Active.",
                ship_payload: true,
            }],
        );
        let pkg_t2 = tmp.path().join("pkg-t2");
        fs::create_dir_all(&pkg_t2).expect("mkdir pkg-t2");
        fs::write(
            pkg_t2.join("package.toml"),
            format!("tag = \"v0.2.0\"\ncommit = \"{commit_t2}\"\n"),
        )
        .expect("write package.toml");
        let (arch_t2, _, _) = archive_release(&ArchiveArgs {
            kind: Some(DeliveryKind::Site),
            package: Some(pkg_t2),
            edition: ed_t2,
            intake: None,
            evidence: Some(ev_t2),
            base: None,
            to: archives_root,
        })
        .expect("archive v0.2.0");

        // Passing `arch_e2` or `None` as predecessor for `v0.2.0` MUST fail.
        assert!(verify_archive(&arch_t2, &tmp.path().join("restore-t2-none"), None).is_err());
        assert!(
            verify_archive(
                &arch_t2,
                &tmp.path().join("restore-t2-wrong"),
                Some(arch_e2.as_path())
            )
            .is_err()
        );

        let rep_t2 = verify_archive(
            &arch_t2,
            &tmp.path().join("restore-t2"),
            Some(arch_t1.as_path()),
        )
        .expect("v0.2.0 verifies against retained tool predecessor v0.1.0");
        assert_eq!(rep_t2.verified_predecessor_tag.as_deref(), Some("v0.1.0"));
    }

    #[test]
    fn edition_impact_classifies_before_admission_payload_quarantined_supersedes_and_missing_archive()
     {
        install_synthetic_providers();
        let tmp = TempDir::new().expect("tempdir");
        let repo = tmp.path().join("repo");
        fs::create_dir_all(repo.join("corpus/registers")).expect("mkdir");
        init_git_repo(&repo);

        let archives_root = tmp.path().join("archives");
        let empty_baselines = parse_baselines(
            "baseline_schema = 1\n\n[compatibility]\nkind = \"empty\"\n\n[tool]\nkind = \"empty\"\n\n[edition]\nkind = \"empty\"\n",
        )
        .expect("empty baselines");

        // Helper to commit `sources.csv` + `changes.md` and tag `edition-NNN`.
        let commit_and_tag = |edition_num: &str, specs: &[SyntheticSourceSpec]| -> String {
            let mut csv = String::from(
                "source_id,version,tag,title,publisher,source_kind,source_role,authority_tier,licence,source_family,family_basis,location,sha256,published,effective_from,as_of,retrieved,lifecycle,supersedes,volatility,handling,note,purpose,origin_system,origin_stream,origin_key,doc_kind,series\n",
            );
            for (i, s) in specs.iter().enumerate() {
                let vid_str = format!("{}-v{}", s.source_id, s.version);
                let sha = sha256_hex(format!("original payload for {vid_str}\n").as_bytes());
                csv.push_str(&format!(
                        "{},{},syn-{},Synthetic Source {vid_str},Synthetic Board,primary_law,primary_evidence,A,Synthetic,syn_{},reviewed,{vid_str}.txt,{sha},2026-01-10,2026-01-15,2026-02-01,2026-02-01,{},{},V2,public,{},Baseline,,,,,\n",
                        s.source_id,
                        s.version,
                        i + 1,
                        i + 1,
                        s.lifecycle,
                        s.supersedes,
                        s.note
                    ));
            }
            fs::write(repo.join("corpus/registers/sources.csv"), csv).expect("write csv");
            fs::write(
                    repo.join("corpus/changes.md"),
                    format!(
                        "# Corpus changes\n\nSynthetic changes.\n\n## edition-{edition_num}\n\n- Added: synthetic.\n"
                    ),
                )
                .expect("write changes.md");
            proc::git(&repo, &["add", "."]).expect("git add");
            proc::git(&repo, &["commit", "-m", &format!("edition {edition_num}")])
                .expect("git commit");
            let tag = format!("edition-{edition_num}");
            proc::git(&repo, &["tag", &tag]).expect("git tag");
            proc::git(&repo, &["rev-parse", "HEAD"])
                .expect("rev-parse")
                .trim()
                .to_owned()
        };

        let archive_tag = |edition_num: &str, commit: &str, specs: &[SyntheticSourceSpec]| {
            let ev = build_synthetic_evidence_dir(
                &tmp.path().join(format!("ev-{edition_num}")),
                &empty_baselines,
                Some(&format!(
                    "release_schema = 1\nedition = \"{edition_num}\"\nkind = \"raw\"\n"
                )),
                b"vector = 1\n",
            );
            let ed_dir = tmp.path().join(format!("ed-{edition_num}"));
            build_synthetic_edition_set(&ed_dir, edition_num, commit, DeliveryKind::Raw, specs);
            archive_release(&ArchiveArgs {
                kind: Some(DeliveryKind::Raw),
                package: None,
                edition: ed_dir,
                intake: None,
                evidence: Some(ev),
                base: None,
                to: archives_root.clone(),
            })
            .expect("archive_release");
        };

        // 1. `edition-001`: before S-0002-v1 admission; S-0001-v2 mentions `S-0002-v1` in `supersedes`!
        //    Must classify `edition-001` as `absent` (a combined version id appearing in `supersedes` is NOT evidence of payload).
        let specs_1 = vec![
            SyntheticSourceSpec {
                source_id: "S-0001",
                version: 1,
                lifecycle: "active",
                supersedes: "",
                note: "Active.",
                ship_payload: true,
            },
            SyntheticSourceSpec {
                source_id: "S-0001",
                version: 2,
                lifecycle: "active",
                supersedes: "S-0002-v1",
                note: "Mentions S-0002-v1 in supersedes.",
                ship_payload: true,
            },
        ];
        let c1 = commit_and_tag("001", &specs_1);
        archive_tag("001", &c1, &specs_1);

        // 2. `edition-002`: S-0002-v1 admitted and active with payload. Must classify as `containing`.
        let specs_2 = vec![
            specs_1[0].clone(),
            SyntheticSourceSpec {
                source_id: "S-0002",
                version: 1,
                lifecycle: "active",
                supersedes: "",
                note: "Admitted with payload.",
                ship_payload: true,
            },
        ];
        let c2 = commit_and_tag("002", &specs_2);
        archive_tag("002", &c2, &specs_2);

        // 3. `edition-003`: S-0002-v1 set to `quarantined`, retaining its row/hash/reason in `sources.csv`
        //    with NO original or text payload shipped. Must classify as `absent`.
        let specs_3 = vec![
            specs_1[0].clone(),
            SyntheticSourceSpec {
                source_id: "S-0002",
                version: 1,
                lifecycle: "quarantined",
                supersedes: "",
                note: "Held for licence review.",
                ship_payload: false,
            },
        ];
        let c3 = commit_and_tag("003", &specs_3);
        archive_tag("003", &c3, &specs_3);

        // 4. `edition-004`: tagged in git, but its archive is unavailable/missing. Must classify as `unresolved` (never `absent`).
        let _c4 = commit_and_tag("004", &specs_3);

        let rows = edition_impact(&repo, "S-0002", 1, &archives_root).expect("edition_impact runs");
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].tag, "edition-001");
        assert_eq!(rows[0].status, ImpactStatus::Absent, "{:?}", rows[0]);

        assert_eq!(rows[1].tag, "edition-002");
        assert_eq!(rows[1].status, ImpactStatus::Containing, "{:?}", rows[1]);

        assert_eq!(rows[2].tag, "edition-003");
        assert_eq!(rows[2].status, ImpactStatus::Absent, "{:?}", rows[2]);

        assert_eq!(rows[3].tag, "edition-004");
        assert_eq!(rows[3].status, ImpactStatus::Unresolved, "{:?}", rows[3]);
    }
}
