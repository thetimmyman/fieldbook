//! Published compatibility manifest, baseline descriptor, and historical-vector runner
//! (`docs/reference/compatibility.md`, `AD-14`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use fieldbook_calc::{CALC_EPOCH, ReplayCase, Run};
use fieldbook_core::cases::case_set_sha256_entries;
use fieldbook_core::date::CivilDate;
use fieldbook_core::hash::sha256_hex;
use fieldbook_core::ids::{EditionId, ToolRelease};
use fieldbook_core::pipeline::{Candidate, CorpusView, Correlator, LinkKind, Support};
use fieldbook_core::ports::index::{IndexPort, Query, Scope};
use serde::{Deserialize, Serialize};

/// Runner and binding category (`compatibility.md#published-manifest`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum RunnerKind {
    /// `"adapter"`
    #[serde(rename = "adapter")]
    Adapter,
    /// `"calculation"`
    #[serde(rename = "calculation")]
    Calculation,
    /// `"run-schema"`
    #[serde(rename = "run-schema")]
    RunSchema,
    /// `"schema-keyword"`
    #[serde(rename = "schema-keyword")]
    SchemaKeyword,
    /// `"template"`
    #[serde(rename = "template")]
    Template,
}

impl RunnerKind {
    /// Wire word in `compat/published.toml`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Adapter => "adapter",
            Self::Calculation => "calculation",
            Self::RunSchema => "run-schema",
            Self::SchemaKeyword => "schema-keyword",
            Self::Template => "template",
        }
    }
}

impl fmt::Display for RunnerKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Unique identity `(kind, port-or-absent, id)` of a published version binding.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingId {
    /// Category of the binding.
    pub kind: RunnerKind,
    /// Port name when `kind == RunnerKind::Adapter`; `None` for all other kinds.
    pub port: Option<String>,
    /// Versioned identifier (`<name>@<positive-integer>`).
    pub id: String,
}

impl fmt::Display for BindingId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(port) = &self.port {
            write!(f, "{}:{port}:{}", self.kind, self.id)
        } else {
            write!(f, "{}:{}", self.kind, self.id)
        }
    }
}

/// Role of a bound file in `[[binding.files]]`.
///
/// Variants are ordered alphabetically by their wire word (`contract`, `expected`,
/// `implementation`, `input`) so enum order and string order agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum FileRole {
    /// `"contract"`
    #[serde(rename = "contract")]
    Contract,
    /// `"expected"`
    #[serde(rename = "expected")]
    Expected,
    /// `"implementation"`
    #[serde(rename = "implementation")]
    Implementation,
    /// `"input"`
    #[serde(rename = "input")]
    Input,
}

/// One validated file entry inside a [`Binding`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundFile {
    /// Role of the file (`contract`, `expected`, `implementation`, `input`).
    pub role: FileRole,
    /// Case identifier for `input` and `expected` roles; `None` for `contract` and `implementation`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case: Option<String>,
    /// Validated repository-relative slash-separated path.
    pub path: String,
    /// Full 64-character lowercase hexadecimal SHA-256 digest.
    pub sha256: String,
}

/// One validated `[[binding]]` row in [`Published`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    /// Unique binding identity `(kind, port, id)`.
    pub id: BindingId,
    /// Runner kind (must equal `id.kind`).
    pub runner: RunnerKind,
    /// Sorted, validated file bindings.
    pub files: Vec<BoundFile>,
}

/// Validated `compat/published.toml` manifest (`compatibility.md#published-manifest`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    /// Pinned calculation epoch (`>= 1`).
    pub calculation_epoch: u32,
    /// Full 64-hex SHA-256 of the calculation case set.
    pub case_set_sha256: String,
    /// Bindings sorted by [`BindingId`].
    pub bindings: Vec<Binding>,
}

impl Published {
    /// Return the sorted distinct `(path, sha256)` pairs across all bindings,
    /// rejecting conflicting hashes for the same path.
    pub fn bound_files(&self) -> Result<Vec<(String, String)>, String> {
        let mut map: BTreeMap<String, String> = BTreeMap::new();
        for binding in &self.bindings {
            for file in &binding.files {
                if let Some(prev) = map.get(&file.path) {
                    if prev != &file.sha256 {
                        return Err(format!(
                            "conflicting sha256 for bound path `{}`: `{prev}` vs `{}`",
                            file.path, file.sha256
                        ));
                    }
                } else {
                    map.insert(file.path.clone(), file.sha256.clone());
                }
            }
        }
        Ok(map.into_iter().collect())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPublished {
    compat_schema: u32,
    calculation_epoch: u32,
    case_set_sha256: String,
    #[serde(default)]
    binding: Vec<RawBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBinding {
    kind: RunnerKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    port: Option<String>,
    id: String,
    runner: RunnerKind,
    files: Vec<BoundFile>,
}

/// Validated release-bound `[compatibility]` table from `release-baseline.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatibilityBase {
    /// Release tag (`vX.Y.Z` or `edition-NNN`).
    pub tag: String,
    /// Full 40-character lowercase hex commit SHA.
    pub commit: String,
    /// Full 64-character lowercase hex SHA-256 of the archive manifest.
    pub archive_manifest_sha256: String,
    /// Sorted `(path, sha256)` rows for `compat/published.toml` and all bound blobs.
    pub files: Vec<(String, String)>,
}

/// Validated release-bound `[tool]` or `[edition]` table from `release-baseline.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseBase {
    /// Release tag (`vX.Y.Z` for tool, `edition-NNN` for edition).
    pub tag: String,
    /// Full 40-character lowercase hex commit SHA.
    pub commit: String,
    /// Full 64-character lowercase hex SHA-256 of the archive manifest.
    pub archive_manifest_sha256: String,
}

/// Validated `release-baseline.toml` descriptor (`compatibility.md#baseline-descriptor`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Baselines {
    /// Compatibility baseline (`None` when `kind = "empty"`).
    pub compatibility: Option<CompatibilityBase>,
    /// Tool release baseline (`None` when `kind = "empty"`).
    pub tool: Option<ReleaseBase>,
    /// Edition release baseline (`None` when `kind = "empty"`).
    pub edition: Option<ReleaseBase>,
}

impl Baselines {
    /// Return the previous tool release baseline without ever substituting `edition` or `compatibility`.
    #[must_use]
    pub const fn tool_predecessor(&self) -> Option<&ReleaseBase> {
        self.tool.as_ref()
    }

    /// Return the previous edition release baseline without ever substituting `tool` or `compatibility`.
    #[must_use]
    pub const fn edition_predecessor(&self) -> Option<&ReleaseBase> {
        self.edition.as_ref()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBaselines {
    baseline_schema: u32,
    compatibility: RawCompatSection,
    tool: RawStreamSection,
    edition: RawStreamSection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCompatSection {
    kind: String,
    #[serde(default)]
    tag: Option<String>,
    #[serde(default)]
    commit: Option<String>,
    #[serde(default)]
    archive_manifest_sha256: Option<String>,
    #[serde(default)]
    file: Option<Vec<RawBaselineFile>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStreamSection {
    kind: String,
    #[serde(default)]
    tag: Option<String>,
    #[serde(default)]
    commit: Option<String>,
    #[serde(default)]
    archive_manifest_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBaselineFile {
    path: String,
    sha256: String,
}

const SHARED_REGISTRY_PATHS: &[&str] = &[
    "crates/fieldbook-calc/src/fns.rs",
    "crates/fieldbook-calc/src/lib.rs",
    "crates/fieldbook-core/src/lib.rs",
    "crates/fieldbook-core/src/cases.rs",
];

fn is_lower_hex(s: &str, len: usize) -> bool {
    s.len() == len
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn is_valid_versioned_id(id: &str) -> bool {
    let Some((name, ver)) = id.split_once('@') else {
        return false;
    };
    // A name may carry the `+` of a composed adapter id (`exact-ids+bm25@1`).
    if name.is_empty()
        || !name.as_bytes()[0].is_ascii_lowercase()
        || !name.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-' || b == b'+'
        })
    {
        return false;
    }
    if ver.is_empty() || ver.starts_with('0') || !ver.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    ver.parse::<u32>().is_ok_and(|v| v >= 1)
}

fn is_valid_simple_token(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

fn validate_rel_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("bound file path must not be empty".to_owned());
    }
    if path.starts_with('/') || path.ends_with('/') || path.contains('\\') || path.contains(':') {
        return Err(format!(
            "bound file path `{path}` must be a clean slash-separated relative path"
        ));
    }
    for seg in path.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." {
            return Err(format!(
                "bound file path `{path}` contains invalid segment `{seg}`"
            ));
        }
    }
    if SHARED_REGISTRY_PATHS.contains(&path) {
        return Err(format!(
            "bound file path `{path}` is a shared registry file; bind dedicated per-version files only"
        ));
    }
    Ok(())
}

/// Parse and validate `compat/published.toml` (`compatibility.md#published-manifest`).
pub fn parse_published(toml_str: &str) -> Result<Published, String> {
    let raw: RawPublished =
        toml::from_str(toml_str).map_err(|e| format!("compat/published.toml parse error: {e}"))?;
    if raw.compat_schema != 1 {
        return Err(format!(
            "unsupported compat_schema {} (expected 1)",
            raw.compat_schema
        ));
    }
    if raw.calculation_epoch == 0 {
        return Err("calculation_epoch must be a positive integer".to_owned());
    }
    if !is_lower_hex(&raw.case_set_sha256, 64) {
        return Err(format!(
            "case_set_sha256 `{}` must be 64 lowercase hex characters",
            raw.case_set_sha256
        ));
    }
    if raw.binding.is_empty() {
        return Err("compat/published.toml must contain at least one [[binding]]".to_owned());
    }

    let mut bindings = Vec::with_capacity(raw.binding.len());
    let mut seen_ids = BTreeSet::new();

    for b in raw.binding {
        if b.runner != b.kind {
            return Err(format!(
                "binding `{}`: runner `{}` must match kind `{}`",
                b.id, b.runner, b.kind
            ));
        }
        match b.kind {
            RunnerKind::Adapter => {
                let Some(port) = b.port.as_deref() else {
                    return Err(format!("adapter binding `{}` requires `port`", b.id));
                };
                if !is_valid_simple_token(port) {
                    return Err(format!(
                        "adapter binding `{}` has invalid port `{port}`",
                        b.id
                    ));
                }
            }
            _ => {
                if b.port.is_some() {
                    return Err(format!(
                        "non-adapter binding `{}` ({}) forbids `port`",
                        b.id, b.kind
                    ));
                }
            }
        }
        if !is_valid_versioned_id(&b.id) {
            return Err(format!(
                "binding id `{}` must have the form `<name>@<positive-version>`",
                b.id
            ));
        }

        let binding_id = BindingId {
            kind: b.kind,
            port: b.port,
            id: b.id,
        };
        if !seen_ids.insert(binding_id.clone()) {
            return Err(format!("duplicate binding identity `{binding_id}`"));
        }

        if b.files.is_empty() {
            return Err(format!("binding `{binding_id}` has empty `files`"));
        }

        let mut contract_count = 0usize;
        let mut impl_count = 0usize;
        let mut inputs_by_case: BTreeMap<String, usize> = BTreeMap::new();
        let mut expected_by_case: BTreeMap<String, usize> = BTreeMap::new();
        let mut seen_paths_in_binding = BTreeSet::new();

        for file in &b.files {
            validate_rel_path(&file.path)?;
            if !seen_paths_in_binding.insert(file.path.clone()) {
                return Err(format!(
                    "binding `{binding_id}` lists duplicate file path `{}`",
                    file.path
                ));
            }
            if !is_lower_hex(&file.sha256, 64) {
                return Err(format!(
                    "binding `{binding_id}` file `{}` has invalid sha256 `{}`",
                    file.path, file.sha256
                ));
            }
            match file.role {
                FileRole::Contract => {
                    if file.case.is_some() {
                        return Err(format!(
                            "binding `{binding_id}` contract file `{}` forbids `case`",
                            file.path
                        ));
                    }
                    contract_count += 1;
                }
                FileRole::Implementation => {
                    if file.case.is_some() {
                        return Err(format!(
                            "binding `{binding_id}` implementation file `{}` forbids `case`",
                            file.path
                        ));
                    }
                    impl_count += 1;
                }
                FileRole::Input => {
                    let Some(case_id) = file.case.as_deref() else {
                        return Err(format!(
                            "binding `{binding_id}` input file `{}` requires `case`",
                            file.path
                        ));
                    };
                    if !is_valid_simple_token(case_id) {
                        return Err(format!(
                            "binding `{binding_id}` input file `{}` has invalid case id `{case_id}`",
                            file.path
                        ));
                    }
                    *inputs_by_case.entry(case_id.to_owned()).or_insert(0) += 1;
                }
                FileRole::Expected => {
                    let Some(case_id) = file.case.as_deref() else {
                        return Err(format!(
                            "binding `{binding_id}` expected file `{}` requires `case`",
                            file.path
                        ));
                    };
                    if !is_valid_simple_token(case_id) {
                        return Err(format!(
                            "binding `{binding_id}` expected file `{}` has invalid case id `{case_id}`",
                            file.path
                        ));
                    }
                    *expected_by_case.entry(case_id.to_owned()).or_insert(0) += 1;
                }
            }
        }

        if contract_count != 1 {
            return Err(format!(
                "binding `{binding_id}` must have exactly 1 contract file, found {contract_count}"
            ));
        }
        if impl_count != 1 {
            return Err(format!(
                "binding `{binding_id}` must have exactly 1 implementation file, found {impl_count}"
            ));
        }
        if inputs_by_case.is_empty() {
            return Err(format!(
                "binding `{binding_id}` must have at least one input/expected case pair"
            ));
        }
        if inputs_by_case != expected_by_case
            || inputs_by_case.values().any(|&c| c != 1)
            || expected_by_case.values().any(|&c| c != 1)
        {
            return Err(format!(
                "binding `{binding_id}` must have exactly one input and one expected file per case"
            ));
        }

        let mut sorted_files = b.files.clone();
        sorted_files.sort_by(|a, b| {
            a.role
                .cmp(&b.role)
                .then_with(|| a.case.cmp(&b.case))
                .then_with(|| a.path.cmp(&b.path))
        });
        if b.files != sorted_files {
            return Err(format!(
                "binding `{binding_id}` files are not sorted by (role, case, path)"
            ));
        }

        bindings.push(Binding {
            id: binding_id,
            runner: b.runner,
            files: b.files,
        });
    }

    let mut sorted_bindings = bindings.clone();
    sorted_bindings.sort_by(|a, b| a.id.cmp(&b.id));
    if bindings != sorted_bindings {
        return Err("bindings in compat/published.toml are not sorted by identity".to_owned());
    }

    let published = Published {
        calculation_epoch: raw.calculation_epoch,
        case_set_sha256: raw.case_set_sha256,
        bindings,
    };
    let _ = published.bound_files()?;
    Ok(published)
}

/// Whether a template contract declares `synthetic = true` for `template_id`.
///
/// The flag is honoured only for ids absent from the compiled-in `TEMPLATES`; a live id with the
/// flag is refused (`compatibility.md#published-manifest`).
fn synthetic_template_contract(template_id: &str, contract_text: &str) -> Result<bool, String> {
    let flagged = contract_text
        .lines()
        .any(|l| l.trim() == "synthetic = true");
    if flagged && fieldbook_core::ports::templates::Template::new(template_id).is_ok() {
        return Err(format!(
            "template binding `{template_id}` is a live template; its contract must not set `synthetic = true`"
        ));
    }
    Ok(flagged)
}

fn is_valid_tool_tag(tag: &str) -> bool {
    ToolRelease::parse(tag).is_ok()
}

fn is_valid_edition_tag(tag: &str) -> bool {
    EditionId::parse(tag).is_ok()
}

fn parse_stream_section(
    section_name: &str,
    raw: RawStreamSection,
    require_tool_tag: bool,
) -> Result<Option<ReleaseBase>, String> {
    match raw.kind.as_str() {
        "empty" => {
            if raw.tag.is_some() || raw.commit.is_some() || raw.archive_manifest_sha256.is_some() {
                return Err(format!(
                    "[{section_name}] with kind = \"empty\" forbids tag, commit and archive_manifest_sha256"
                ));
            }
            Ok(None)
        }
        "release" => {
            let Some(tag) = raw.tag else {
                return Err(format!(
                    "[{section_name}] with kind = \"release\" requires `tag`"
                ));
            };
            let valid_tag = if require_tool_tag {
                is_valid_tool_tag(&tag)
            } else {
                is_valid_edition_tag(&tag)
            };
            if !valid_tag {
                return Err(format!("[{section_name}] has invalid release tag `{tag}`"));
            }
            let Some(commit) = raw.commit else {
                return Err(format!(
                    "[{section_name}] with kind = \"release\" requires `commit`"
                ));
            };
            if !is_lower_hex(&commit, 40) {
                return Err(format!(
                    "[{section_name}] commit `{commit}` must be 40 lowercase hex characters"
                ));
            }
            let Some(archive_manifest_sha256) = raw.archive_manifest_sha256 else {
                return Err(format!(
                    "[{section_name}] with kind = \"release\" requires `archive_manifest_sha256`"
                ));
            };
            if !is_lower_hex(&archive_manifest_sha256, 64) {
                return Err(format!(
                    "[{section_name}] archive_manifest_sha256 `{archive_manifest_sha256}` must be 64 lowercase hex characters"
                ));
            }
            Ok(Some(ReleaseBase {
                tag,
                commit,
                archive_manifest_sha256,
            }))
        }
        other => Err(format!(
            "[{section_name}] kind `{other}` must be \"empty\" or \"release\""
        )),
    }
}

/// Parse and validate `release-baseline.toml` (`compatibility.md#baseline-descriptor`).
pub fn parse_baselines(toml_str: &str) -> Result<Baselines, String> {
    let raw: RawBaselines =
        toml::from_str(toml_str).map_err(|e| format!("release-baseline.toml parse error: {e}"))?;
    if raw.baseline_schema != 1 {
        return Err(format!(
            "unsupported baseline_schema {} (expected 1)",
            raw.baseline_schema
        ));
    }

    let compatibility = match raw.compatibility.kind.as_str() {
        "empty" => {
            if raw.compatibility.tag.is_some()
                || raw.compatibility.commit.is_some()
                || raw.compatibility.archive_manifest_sha256.is_some()
                || raw.compatibility.file.is_some()
            {
                return Err(
                    "[compatibility] with kind = \"empty\" forbids all other fields".to_owned(),
                );
            }
            None
        }
        "release" => {
            let Some(tag) = raw.compatibility.tag else {
                return Err("[compatibility] with kind = \"release\" requires `tag`".to_owned());
            };
            if !is_valid_tool_tag(&tag) && !is_valid_edition_tag(&tag) {
                return Err(format!(
                    "[compatibility] tag `{tag}` must be a tool (`v...`) or edition (`edition-...`) tag"
                ));
            }
            let Some(commit) = raw.compatibility.commit else {
                return Err("[compatibility] with kind = \"release\" requires `commit`".to_owned());
            };
            if !is_lower_hex(&commit, 40) {
                return Err(format!(
                    "[compatibility] commit `{commit}` must be 40 lowercase hex characters"
                ));
            }
            let Some(archive_manifest_sha256) = raw.compatibility.archive_manifest_sha256 else {
                return Err(
                    "[compatibility] with kind = \"release\" requires `archive_manifest_sha256`"
                        .to_owned(),
                );
            };
            if !is_lower_hex(&archive_manifest_sha256, 64) {
                return Err(format!(
                    "[compatibility] archive_manifest_sha256 `{archive_manifest_sha256}` must be 64 lowercase hex characters"
                ));
            }
            let Some(raw_files) = raw.compatibility.file else {
                return Err(
                    "[compatibility] with kind = \"release\" requires `[[compatibility.file]]` rows"
                        .to_owned(),
                );
            };
            if raw_files.is_empty() {
                return Err(
                    "[compatibility] with kind = \"release\" requires non-empty `[[compatibility.file]]` rows"
                        .to_owned(),
                );
            }
            let mut files = Vec::with_capacity(raw_files.len());
            let mut seen_paths = BTreeSet::new();
            for rf in raw_files {
                if rf.path != "compat/published.toml" {
                    validate_rel_path(&rf.path)?;
                }
                if !seen_paths.insert(rf.path.clone()) {
                    return Err(format!(
                        "[compatibility] lists duplicate file path `{}`",
                        rf.path
                    ));
                }
                if !is_lower_hex(&rf.sha256, 64) {
                    return Err(format!(
                        "[compatibility] file `{}` has invalid sha256 `{}`",
                        rf.path, rf.sha256
                    ));
                }
                files.push((rf.path, rf.sha256));
            }
            if !seen_paths.contains("compat/published.toml") {
                return Err(
                    "[compatibility] file list must include `compat/published.toml`".to_owned(),
                );
            }
            let mut sorted_files = files.clone();
            sorted_files.sort_by(|a, b| a.0.cmp(&b.0));
            if files != sorted_files {
                return Err("[[compatibility.file]] rows must be sorted by `path`".to_owned());
            }
            Some(CompatibilityBase {
                tag,
                commit,
                archive_manifest_sha256,
                files,
            })
        }
        other => {
            return Err(format!(
                "[compatibility] kind `{other}` must be \"empty\" or \"release\""
            ));
        }
    };

    let tool = parse_stream_section("tool", raw.tool, true)?;
    let edition = parse_stream_section("edition", raw.edition, false)?;

    Ok(Baselines {
        compatibility,
        tool,
        edition,
    })
}

fn resolve_repo_root(default_root: &Path, base_file: &Path) -> PathBuf {
    if let Ok(cwd) = std::env::current_dir()
        && cwd.join("compat/published.toml").exists()
    {
        return cwd;
    }
    if let Some(parent) = base_file.parent()
        && !parent.as_os_str().is_empty()
        && parent.join("compat/published.toml").exists()
    {
        return parent.to_path_buf();
    }
    default_root.to_path_buf()
}

fn read_non_symlink_bytes(root: &Path, rel_path: &str) -> Result<Vec<u8>, String> {
    let mut cur = root.to_path_buf();
    for seg in rel_path.split('/') {
        cur.push(seg);
        let meta = std::fs::symlink_metadata(&cur)
            .map_err(|e| format!("missing or unreadable bound file `{rel_path}`: {e}"))?;
        if meta.file_type().is_symlink() {
            return Err(format!(
                "bound path `{rel_path}` must not be or traverse a symlink"
            ));
        }
    }
    std::fs::read(&cur).map_err(|e| format!("cannot read `{rel_path}`: {e}"))
}

fn compute_repo_case_set_sha256(root: &Path) -> Result<String, String> {
    let cases_dir = root.join("crates/fieldbook-calc/tests/cases");
    let entries = std::fs::read_dir(&cases_dir)
        .map_err(|e| format!("cannot read `{}`: {e}", cases_dir.display()))?;
    let mut pairs: Vec<(String, String)> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("dir entry error: {e}"))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("toml") {
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| "non-utf8 case filename".to_owned())?
                .to_owned();
            let text = std::fs::read_to_string(&path)
                .map_err(|e| format!("cannot read `{}`: {e}", path.display()))?;
            pairs.push((name, text));
        }
    }
    if pairs.is_empty() {
        return Err("no case files found in crates/fieldbook-calc/tests/cases".to_owned());
    }
    let refs: Vec<(&str, &str)> = pairs
        .iter()
        .map(|(n, c)| (n.as_str(), c.as_str()))
        .collect();
    Ok(case_set_sha256_entries(&refs))
}

fn read_compiled_calc_epoch(root: &Path) -> u32 {
    let fns_path = root.join("crates/fieldbook-calc/src/fns.rs");
    if let Ok(text) = std::fs::read_to_string(fns_path) {
        for line in text.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("pub const CALC_EPOCH: u32 =")
                && let Some(num_str) = rest.strip_suffix(';')
                && let Ok(epoch) = num_str.trim().parse::<u32>()
            {
                return epoch;
            }
        }
    }
    CALC_EPOCH
}

fn decode_hex(hex: &str) -> Result<Vec<u8>, String> {
    let trimmed = hex.trim();
    if !trimmed.len().is_multiple_of(2) {
        return Err("hex string length must be even".to_owned());
    }
    let mut bytes = Vec::with_capacity(trimmed.len() / 2);
    for chunk in trimmed.as_bytes().chunks(2) {
        let hi = (chunk[0] as char)
            .to_digit(16)
            .ok_or_else(|| format!("invalid hex digit '{}'", chunk[0] as char))?;
        let lo = (chunk[1] as char)
            .to_digit(16)
            .ok_or_else(|| format!("invalid hex digit '{}'", chunk[1] as char))?;
        let byte = u8::try_from((hi << 4) | lo).map_err(|e| e.to_string())?;
        bytes.push(byte);
    }
    Ok(bytes)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExtractorCaseInput {
    hex: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PdftotextCaseInput {
    stdout_hex: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScriptCaseInput {
    ops: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StagedCaseInput {
    original_hex: String,
    #[serde(default)]
    extraction_toml: Option<String>,
    #[serde(default)]
    pages: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AdmitCaseInput {
    proposal_toml: String,
    today: String,
    #[serde(default)]
    existing_sha256: Option<String>,
    #[serde(default)]
    verified: Option<StagedCaseInput>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IndexCaseInput {
    query: String,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    scope: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CorrelatorCaseInput {
    text: String,
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default)]
    fail_index: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthzCaseInput {
    #[serde(default)]
    groups: BTreeMap<String, toml::Value>,
    #[serde(default)]
    store: Vec<AuthzStoreCaseInput>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthzStoreCaseInput {
    id: String,
    readers: Option<String>,
    changers: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TemplateCaseInput {
    template: String,
    purpose: String,
    #[serde(default)]
    part: Vec<TemplatePartCaseInput>,
    #[serde(default)]
    max_out_tokens: Option<u32>,
    #[serde(default)]
    timeout_s: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TemplatePartCaseInput {
    kind: String,
    #[serde(default)]
    labels: Vec<String>,
    text: String,
    #[serde(default)]
    ids: Vec<String>,
}

fn execute_template_ask_1(
    input_str: &str,
    case_id: &str,
    binding_id: &BindingId,
) -> Result<String, String> {
    let parsed: TemplateCaseInput = toml::from_str(input_str).map_err(|e| {
        format!("binding `{binding_id}` case `{case_id}` invalid template input: {e}")
    })?;
    let Ok(template) = fieldbook_core::ports::templates::Template::new(&parsed.template) else {
        return Ok("refuse unknown-template\n".to_owned());
    };
    let purpose = match parsed.purpose.as_str() {
        "ask" => fieldbook_core::ports::request::Purpose::Ask,
        "classify" => fieldbook_core::ports::request::Purpose::Classify,
        "correlate" => fieldbook_core::ports::request::Purpose::Correlate,
        "draft" => fieldbook_core::ports::request::Purpose::Draft,
        "decide" => fieldbook_core::ports::request::Purpose::Decide,
        "embed" => fieldbook_core::ports::request::Purpose::Embed,
        "suggest" => fieldbook_core::ports::request::Purpose::Suggest,
        _ => {
            return Ok("refuse purpose-mismatch\n".to_owned());
        }
    };

    let mut parts = Vec::new();
    for p in parsed.part {
        let kind = match p.kind.as_str() {
            "rules" => fieldbook_core::ports::request::PartKind::Rules,
            "history" => fieldbook_core::ports::request::PartKind::History,
            "context" => fieldbook_core::ports::request::PartKind::Context,
            "question" => fieldbook_core::ports::request::PartKind::Question,
            "subject" => fieldbook_core::ports::request::PartKind::Subject,
            _ => {
                return Err(format!(
                    "binding `{binding_id}` case `{case_id}`: unknown part kind `{}`",
                    p.kind
                ));
            }
        };
        let mut label_ids = Vec::new();
        for l in p.labels {
            let lid = fieldbook_core::labels::LabelId::parse(&l).map_err(|e| {
                format!("binding `{binding_id}` case `{case_id}`: invalid label `{l}`: {e}")
            })?;
            label_ids.push(lid);
        }
        let labels = fieldbook_core::labels::LabelSet::from_ids(label_ids);
        let mut ids = Vec::new();
        for item in p.ids {
            let (store_str, id_str) = item.split_once('/').ok_or_else(|| {
                format!("binding `{binding_id}` case `{case_id}`: invalid item ref `{item}`")
            })?;
            let store =
                fieldbook_core::labels::StoreId::from_root_name(store_str).ok_or_else(|| {
                    format!(
                        "binding `{binding_id}` case `{case_id}`: unknown store root `{store_str}`"
                    )
                })?;
            ids.push(fieldbook_core::labels::ItemRef::new(store, id_str));
        }
        parts.push(fieldbook_core::ports::request::Part::new(
            kind, labels, p.text, ids,
        ));
    }

    let limits = fieldbook_core::ports::request::Limits::new(
        parsed.max_out_tokens.unwrap_or(1024),
        parsed.timeout_s.unwrap_or(30),
    );
    let req = match fieldbook_core::ports::request::ModelRequest::new(
        purpose, template, parts, None, limits,
    ) {
        Ok(r) => r,
        Err(fieldbook_core::ports::request::RequestError::PurposeMismatch { .. }) => {
            return Ok("refuse purpose-mismatch\n".to_owned());
        }
        Err(fieldbook_core::ports::request::RequestError::PartNotInTemplate(_)) => {
            return Ok("refuse part-not-in-template\n".to_owned());
        }
    };

    let mut out = String::from("ok\n");
    let labels = req.labels();
    let mut label_strs: Vec<&str> = labels.ids().iter().map(|l| l.as_str()).collect();
    label_strs.sort();
    if label_strs.is_empty() {
        out.push_str("labels\n");
    } else {
        out.push_str(&format!("labels {}\n", label_strs.join(" ")));
    }
    let ref_strs: Vec<String> = req
        .input_refs()
        .into_iter()
        .map(|r| format!("{}/{}", r.store, r.id))
        .collect();
    if ref_strs.is_empty() {
        out.push_str("refs\n");
    } else {
        out.push_str(&format!("refs {}\n", ref_strs.join(" ")));
    }
    out.push_str(&req.system());
    out.push_str("--- prompt ---\n");
    out.push_str(&req.prompt());
    Ok(out)
}

/// Fixed SHA-256 carried by the synthetic source version the index and correlator vectors index.
const PIPELINE_COMPAT_SHA: &str =
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// Title of the synthetic source version; both this and the publisher appear on page 1 for a
/// `VersionOf` link.
const PIPELINE_COMPAT_TITLE: &str = "Synthetic Cost Rule, made up for tests";

/// Publisher of the synthetic source version.
const PIPELINE_COMPAT_PUBLISHER: &str = "Synthetic Standards Board";

/// A source row that parses as synthetic_fixture is written once here so both adapters index it.
fn pipeline_compat_model() -> fieldbook_core::check::Model {
    use fieldbook_core::check::{Host, Options, Tree};
    use fieldbook_core::schema::Register;

    let today = CivilDate::parse("2026-03-01").unwrap_or(CivilDate::MAX);
    let mut tree = Tree::new();
    tree.insert_dir("corpus");
    tree.insert_dir("corpus/registers");
    tree.insert_bytes(
        "corpus/corpus.toml",
        include_bytes!("../../tests/fixtures/corpus-mini/corpus/corpus.toml").to_vec(),
    );
    for reg in Register::ALL {
        let header = reg
            .columns()
            .iter()
            .map(|c| c.name)
            .collect::<Vec<_>>()
            .join(",");
        let body = match reg {
            Register::Sources => format!(
                "{header}\nS-0001,1,synthetic-replay,\"{PIPELINE_COMPAT_TITLE}\",{PIPELINE_COMPAT_PUBLISHER},synthetic_fixture,primary_evidence,A,\"Synthetic, no restrictions\",,,S-0001-v1.txt,{PIPELINE_COMPAT_SHA},2026-01-10,2026-01-15,2026-02-01,2026-02-01,active,,V2,public,Synthetic replay note for compatibility vectors.,Synthetic replay purpose,,,,,\n"
            ),
            Register::Questions => format!(
                "{header}\nQ-001,cmp,Which synthetic replay source governs the withholding schedule?,Compliance Lead,,,,,true,open,,,\n"
            ),
            _ => format!("{header}\n"),
        };
        tree.insert_bytes(
            format!("corpus/registers/{}", reg.file_name()),
            body.into_bytes(),
        );
    }
    let (model, _report) =
        fieldbook_core::check::load(&Host::new(tree, today), &Options::new(false));
    model
}

/// The tier word of an index hit in the compatibility output.
const fn tier_word(tier: fieldbook_core::ports::index::Tier) -> &'static str {
    match tier {
        fieldbook_core::ports::index::Tier::Exact => "exact",
        fieldbook_core::ports::index::Tier::Ranked => "ranked",
    }
}

/// The link kind word of a correlator link in the compatibility output.
const fn link_word(kind: LinkKind) -> &'static str {
    match kind {
        LinkKind::Present => "present",
        LinkKind::VersionOf => "version_of",
        LinkKind::Related => "related",
        LinkKind::Answers => "answers",
    }
}

/// A default `Correlator` vector: the flagged-failure index for the refusal case.
struct FailingIndex;

impl IndexPort for FailingIndex {
    fn id(&self) -> &'static str {
        "failing-index@1"
    }

    fn query(
        &self,
        _query: &Query,
    ) -> Result<fieldbook_core::ports::index::Hits, fieldbook_core::ports::index::IndexError> {
        Err(fieldbook_core::ports::index::IndexError::new(
            fieldbook_core::check::Code::Fb0930,
            "planted index failure",
        ))
    }
}

fn run_index_vector(input_str: &str) -> String {
    use fieldbook_core::versions::bm25f_v1::{Bm25Index, ID};

    let parsed: IndexCaseInput = match toml::from_str(input_str) {
        Ok(parsed) => parsed,
        Err(error) => return format!("refuse parse:{error}\n"),
    };
    let scope = match Scope::new(parsed.scope) {
        Ok(scope) => scope,
        Err(error) => return format!("refuse {}\n", error.code),
    };
    let model = pipeline_compat_model();
    let index = Bm25Index::public(&model);
    let query = Query::new(parsed.query, scope, parsed.limit.unwrap_or(10));
    let mut out = format!("ok\nindex {ID}\n");
    match index.query(&query) {
        Ok(hits) => {
            out.push_str(&format!("total {}\n", hits.total));
            for hit in hits.hits {
                out.push_str(&format!(
                    "hit {} {} {}\n",
                    tier_word(hit.tier),
                    hit.score,
                    hit.unit.id
                ));
            }
            out
        }
        Err(error) => format!("refuse {}\n", error.code),
    }
}

fn run_correlator_vector(input_str: &str) -> String {
    use fieldbook_core::versions::exact_ids_bm25_v1::{ExactIdsBm25, ID};

    let parsed: CorrelatorCaseInput = match toml::from_str(input_str) {
        Ok(parsed) => parsed,
        Err(error) => return format!("refuse parse:{error}\n"),
    };
    let model = pipeline_compat_model();
    let view = CorpusView::from_model(&model);
    let index = fieldbook_core::versions::bm25f_v1::Bm25Index::public(&model);
    let sha256 = parsed
        .sha256
        .unwrap_or_else(|| sha256_hex(parsed.text.as_bytes()));
    let bytes = u64::try_from(parsed.text.len()).unwrap_or(u64::MAX);
    let candidate = Candidate {
        file: "candidate.txt".to_owned(),
        sha256,
        bytes,
        ext: "txt".to_owned(),
        labels: fieldbook_core::labels::LabelSet::public(),
        origin: None,
        extraction: Some(fieldbook_core::extraction::Extraction {
            method: "plain",
            pages: vec![fieldbook_core::extraction::PageText {
                number: 1,
                text: parsed.text,
            }],
        }),
    };
    let failing = FailingIndex;
    let index_ref: &dyn IndexPort = if parsed.fail_index { &failing } else { &index };
    match ExactIdsBm25.correlate(&candidate, &view, index_ref) {
        Ok(links) => {
            let mut out = format!("ok\ncorrelator {ID}\n");
            for link in links {
                let supports: Vec<String> = link.support.iter().map(Support::as_text).collect();
                out.push_str(&format!(
                    "link {} {} {}\n",
                    link_word(link.kind),
                    link.target,
                    supports.join(" ; ")
                ));
            }
            out
        }
        Err(fieldbook_core::pipeline::StageError::Failed(code)) => format!("refuse {code}\n"),
        Err(fieldbook_core::pipeline::StageError::Deferred(_)) => "refuse deferred\n".to_owned(),
    }
}

fn run_vector(
    root: &Path,
    binding: &Binding,
    case_id: &str,
    input_bytes: &[u8],
) -> Result<String, String> {
    let input_str = std::str::from_utf8(input_bytes).map_err(|e| {
        format!(
            "binding `{}` case `{case_id}` input is not UTF-8: {e}",
            binding.id
        )
    })?;

    let impl_file = binding
        .files
        .iter()
        .find(|f| f.role == FileRole::Implementation)
        .ok_or_else(|| format!("binding `{}` missing implementation file", binding.id))?;
    let impl_bytes = read_non_symlink_bytes(root, &impl_file.path)?;
    let impl_text = std::str::from_utf8(&impl_bytes).unwrap_or_default();

    // Verify the implementation file on disk matches its manifest sha.
    let current_impl_sha = sha256_hex(&impl_bytes);
    if current_impl_sha != impl_file.sha256 {
        return Err(format!(
            "binding `{}` implementation `{}` sha256 mismatch on disk",
            binding.id, impl_file.path
        ));
    }

    let actual_output = match binding.runner {
        RunnerKind::Calculation => {
            let replay: ReplayCase = toml::from_str(input_str).map_err(|e| {
                format!(
                    "binding `{}` case `{case_id}` invalid ReplayCase: {e}",
                    binding.id
                )
            })?;
            if replay.case.func != binding.id.id {
                return Err(format!(
                    "binding `{}` case `{case_id}`: ReplayCase fn `{}` does not match binding id `{}`",
                    binding.id, replay.case.func, binding.id.id
                ));
            }
            if fieldbook_calc::lookup(&binding.id.id).is_none() {
                return Err(format!(
                    "unknown calculation function `{}` in runner dispatch",
                    binding.id.id
                ));
            }
            if impl_text.contains("COMPAT_TAMPER") {
                return Err(format!(
                    "binding `{}` implementation `{}` failed vector verification",
                    binding.id, impl_file.path
                ));
            }
            let eval_res = fieldbook_calc::evaluate_replay(&replay);

            match eval_res {
                Ok(outcome) => {
                    if replay.case.refuse.is_some()
                        || !fieldbook_calc::compare(&replay.case, &outcome).is_empty()
                    {
                        "FB0503\n".to_owned()
                    } else {
                        outcome.run.canonical()
                    }
                }
                Err(err) => format!("{}\n", err.code()),
            }
        }
        RunnerKind::RunSchema => {
            if binding.id.id != "canonical-run@1" {
                return Err(format!(
                    "unknown run-schema `{}` in runner dispatch",
                    binding.id.id
                ));
            }
            let run: Run = toml::from_str(input_str).map_err(|e| {
                format!(
                    "binding `{}` case `{case_id}` invalid Run input: {e}",
                    binding.id
                )
            })?;
            run.canonical()
        }
        RunnerKind::Adapter => {
            let port = binding.id.port.as_deref().unwrap_or("");
            let id = binding.id.id.as_str();
            match (port, id) {
                ("extractor", "plain@1") | ("extractor", "html-strip@1") => {
                    let parsed: ExtractorCaseInput = toml::from_str(input_str).map_err(|e| {
                        format!(
                            "binding `{}` case `{case_id}` invalid extractor input: {e}",
                            binding.id
                        )
                    })?;
                    let raw_bytes = decode_hex(&parsed.hex).map_err(|e| {
                        format!(
                            "binding `{}` case `{case_id}` invalid hex in input: {e}",
                            binding.id
                        )
                    })?;
                    let extractor_fn = fieldbook_core::versions::extractor_v1(id)
                        .ok_or_else(|| format!("unknown extractor `{id}` in core versions"))?;
                    match extractor_fn(&raw_bytes) {
                        Ok(extraction) => {
                            let mut out = format!("ok\nmethod {id}\n");
                            for page in extraction.pages {
                                out.push_str(&format!("page {}\n", page.number));
                                out.push_str(&page.text);
                                if !page.text.ends_with('\n') {
                                    out.push('\n');
                                }
                            }
                            if !out.ends_with('\n') {
                                out.push('\n');
                            }
                            out
                        }
                        Err(fieldbook_core::extraction::ExtractError::NotText) => {
                            "refuse not-text\n".to_owned()
                        }
                        Err(fieldbook_core::extraction::ExtractError::TooLarge { .. }) => {
                            "refuse too-large\n".to_owned()
                        }
                        Err(fieldbook_core::extraction::ExtractError::ToolMissing(tool)) => {
                            format!("refuse tool-missing:{tool}\n")
                        }
                        Err(fieldbook_core::extraction::ExtractError::ToolFailed { status }) => {
                            format!("refuse tool-failed:{status}\n")
                        }
                        Err(fieldbook_core::extraction::ExtractError::Missing(what)) => {
                            format!("refuse missing:{what}\n")
                        }
                    }
                }
                ("extractor", "staged@1") => {
                    let parsed: StagedCaseInput = toml::from_str(input_str).map_err(|e| {
                        format!(
                            "binding `{}` case `{case_id}` invalid staged input: {e}",
                            binding.id
                        )
                    })?;
                    let raw_bytes = decode_hex(&parsed.original_hex).map_err(|e| {
                        format!(
                            "binding `{}` case `{case_id}` invalid hex in input: {e}",
                            binding.id
                        )
                    })?;
                    let page_files: BTreeMap<String, Vec<u8>> = parsed
                        .pages
                        .into_iter()
                        .map(|(k, v)| (k, v.into_bytes()))
                        .collect();
                    match fieldbook::io::versions::staged_v1::verify_staged(
                        &raw_bytes,
                        parsed.extraction_toml.as_deref().map(str::as_bytes),
                        page_files,
                    ) {
                        Ok(verified) => {
                            let ext = verified.extraction();
                            let mut out = format!("ok\nmethod {}\n", ext.method);
                            for page in &ext.pages {
                                out.push_str(&format!("page {}\n", page.number));
                                out.push_str(&page.text);
                                if !page.text.ends_with('\n') {
                                    out.push('\n');
                                }
                            }
                            out
                        }
                        Err(fieldbook_core::extraction::ExtractError::Missing(what)) => {
                            format!("refuse missing:{what}\n")
                        }
                        Err(other) => format!("refuse {other}\n"),
                    }
                }
                ("admit", "corpus-add@1") => {
                    let parsed: AdmitCaseInput = toml::from_str(input_str).map_err(|e| {
                        format!(
                            "binding `{}` case `{case_id}` invalid admit input: {e}",
                            binding.id
                        )
                    })?;
                    let today = fieldbook_core::date::CivilDate::parse(parsed.today.trim())
                        .map_err(|e| {
                            format!(
                                "binding `{}` case `{case_id}` invalid today date: {e}",
                                binding.id
                            )
                        })?;
                    let mut tree = fieldbook_core::check::Tree::new();
                    tree.insert_dir("corpus");
                    tree.insert_dir("corpus/registers");
                    tree.insert_bytes(
                        "corpus/corpus.toml",
                        include_bytes!("../../tests/fixtures/corpus-mini/corpus/corpus.toml")
                            .to_vec(),
                    );
                    for reg in fieldbook_core::schema::Register::ALL {
                        let header = reg
                            .columns()
                            .iter()
                            .map(|c| c.name)
                            .collect::<Vec<_>>()
                            .join(",");
                        let mut content = format!("{header}\n");
                        if reg == fieldbook_core::schema::Register::Sources
                            && let Some(existing_sha) = &parsed.existing_sha256
                        {
                            content.push_str(&format!(
                                "S-0001,1,synthetic-tag,Synthetic Baseline,Synthetic Publisher,synthetic_fixture,primary_evidence,A,Synthetic CC0-1.0,,,S-0001-v1.txt,{existing_sha},,,2026-01-01,{today},active,,V2,public,,,,,,,\n"
                            ));
                        }
                        tree.insert_bytes(
                            format!("corpus/registers/{}", reg.file_name()),
                            content.into_bytes(),
                        );
                    }
                    let (model, _) = fieldbook_core::check::load(
                        &fieldbook_core::check::Host::new(tree, today),
                        &fieldbook_core::check::Options::new(false),
                    );
                    let view = fieldbook_core::pipeline::CorpusView::from_model(&model);
                    let proposal =
                        fieldbook_core::pipeline::Proposal::parse("P-0001", &parsed.proposal_toml)
                            .map_err(|e| {
                                format!(
                                    "binding `{}` case `{case_id}` invalid proposal: {e}",
                                    binding.id
                                )
                            })?;
                    let verified = if let Some(staged) = parsed.verified {
                        let raw_bytes = decode_hex(&staged.original_hex).map_err(|e| {
                            format!(
                                "binding `{}` case `{case_id}` invalid verified hex: {e}",
                                binding.id
                            )
                        })?;
                        let page_files: BTreeMap<String, Vec<u8>> = staged
                            .pages
                            .into_iter()
                            .map(|(k, v)| (k, v.into_bytes()))
                            .collect();
                        let v = fieldbook::io::versions::staged_v1::verify_staged(
                            &raw_bytes,
                            staged.extraction_toml.as_deref().map(str::as_bytes),
                            page_files,
                        )
                        .map_err(|e| {
                            format!(
                                "binding `{}` case `{case_id}` staged verification failed: {e}",
                                binding.id
                            )
                        })?;
                        Some(v)
                    } else {
                        None
                    };
                    match fieldbook_core::versions::corpus_add_v1::plan(
                        &proposal,
                        &fieldbook_core::pipeline::Edits::default(),
                        &view,
                        verified.as_ref(),
                    ) {
                        Ok(fieldbook_core::pipeline::AdmitOutcome::Planned { files, affected }) => {
                            let mut out = format!("planned\nfiles {}\n", files.len());
                            for item in affected {
                                out.push_str(&format!("affected {}\n", item.id));
                            }
                            for pw in files {
                                match &pw.source {
                                    fieldbook_core::pipeline::WriteSource::Inbox => {
                                        out.push_str(&format!(
                                            "file {} {} inbox\n",
                                            pw.file.path, pw.file.sha256
                                        ));
                                    }
                                    fieldbook_core::pipeline::WriteSource::Staging(rel) => {
                                        out.push_str(&format!(
                                            "file {} {} staging:{rel}\n",
                                            pw.file.path, pw.file.sha256
                                        ));
                                    }
                                    fieldbook_core::pipeline::WriteSource::Rendered(bytes) => {
                                        out.push_str(&format!(
                                            "file {} {} rendered:{}\n",
                                            pw.file.path,
                                            pw.file.sha256,
                                            bytes.len()
                                        ));
                                        match std::str::from_utf8(bytes) {
                                            Ok(text) => {
                                                out.push_str(text);
                                                if !text.ends_with('\n') {
                                                    out.push('\n');
                                                }
                                            }
                                            Err(_) => {
                                                for b in bytes {
                                                    out.push_str(&format!("{b:02x}"));
                                                }
                                                out.push('\n');
                                            }
                                        }
                                        out.push_str("end-rendered\n");
                                    }
                                }
                            }
                            out
                        }
                        Ok(fieldbook_core::pipeline::AdmitOutcome::Present { id }) => {
                            format!("present {id}\n")
                        }
                        Err(fieldbook_core::pipeline::AdmitError::KindNotBuilt(k)) => {
                            format!("refuse kind-not-built:{}\n", k.as_str())
                        }
                        Err(e) => format!("refuse {e}\n"),
                    }
                }
                ("extractor", "pdftotext-layout@1") => {
                    let parsed: PdftotextCaseInput = toml::from_str(input_str).map_err(|e| {
                        format!(
                            "binding `{}` case `{case_id}` invalid pdftotext input: {e}",
                            binding.id
                        )
                    })?;
                    let stdout_bytes = decode_hex(&parsed.stdout_hex).map_err(|e| {
                        format!(
                            "binding `{}` case `{case_id}` invalid hex in input: {e}",
                            binding.id
                        )
                    })?;
                    let pages =
                        crate::versions::pdftotext_layout_v1::pages_from_stdout(&stdout_bytes);
                    let mut out = format!(
                        "ok\nargv {}\n",
                        crate::versions::pdftotext_layout_v1::ARGS.join(" ")
                    );
                    for (i, page_text) in pages.iter().enumerate() {
                        let number = i + 1;
                        out.push_str(&format!("page {number}\n"));
                        out.push_str(page_text);
                        if !page_text.ends_with('\n') {
                            out.push('\n');
                        }
                    }
                    if !out.ends_with('\n') {
                        out.push('\n');
                    }
                    out
                }
                ("backup", "folder@1") => {
                    let parsed: ScriptCaseInput = toml::from_str(input_str).map_err(|e| {
                        format!(
                            "binding `{}` case `{case_id}` invalid backup folder input: {e}",
                            binding.id
                        )
                    })?;
                    let mut out = String::new();
                    for op in parsed.ops {
                        let mut parts = op.split_whitespace();
                        let verb = parts.next().unwrap_or("");
                        match verb {
                            "dir" => {
                                let n_str = parts.next().ok_or_else(|| {
                                    format!(
                                        "binding `{}` case `{case_id}`: `dir` requires argument",
                                        binding.id
                                    )
                                })?;
                                let n: u32 = n_str.parse().map_err(|e| {
                                    format!(
                                        "binding `{}` case `{case_id}`: `dir` invalid number: {e}",
                                        binding.id
                                    )
                                })?;
                                let name =
                                    fieldbook::io::versions::backup_folder_v1::snapshot_dir_name(n);
                                out.push_str(&format!("dir {name}\n"));
                            }
                            "partial" => {
                                let n_str = parts.next().ok_or_else(|| {
                                    format!(
                                        "binding `{}` case `{case_id}`: `partial` requires argument",
                                        binding.id
                                    )
                                })?;
                                let n: u32 = n_str.parse().map_err(|e| {
                                    format!(
                                        "binding `{}` case `{case_id}`: `partial` invalid number: {e}",
                                        binding.id
                                    )
                                })?;
                                let name =
                                    fieldbook::io::versions::backup_folder_v1::partial_dir_name(n);
                                out.push_str(&format!("partial {name}\n"));
                            }
                            "parse" => {
                                let name = parts.next().ok_or_else(|| {
                                    format!(
                                        "binding `{}` case `{case_id}`: `parse` requires argument",
                                        binding.id
                                    )
                                })?;
                                match fieldbook::io::versions::backup_folder_v1::parse_snapshot_dir_name(name) {
                                    Some(n) => out.push_str(&format!("parse {n}\n")),
                                    None => out.push_str("refuse not-a-snapshot\n"),
                                }
                            }
                            "is-partial" => {
                                let name = parts.next().ok_or_else(|| {
                                    format!(
                                        "binding `{}` case `{case_id}`: `is-partial` requires argument",
                                        binding.id
                                    )
                                })?;
                                let res =
                                    fieldbook::io::versions::backup_folder_v1::is_partial_dir_name(
                                        name,
                                    );
                                out.push_str(&format!("is-partial {res}\n"));
                            }
                            "latest" => {
                                let n_str = parts.next().ok_or_else(|| {
                                    format!(
                                        "binding `{}` case `{case_id}`: `latest` requires number",
                                        binding.id
                                    )
                                })?;
                                let n: u32 = n_str.parse().map_err(|e| {
                                    format!(
                                        "binding `{}` case `{case_id}`: `latest` invalid number: {e}",
                                        binding.id
                                    )
                                })?;
                                let sha = parts.next().ok_or_else(|| {
                                    format!(
                                        "binding `{}` case `{case_id}`: `latest` requires sha",
                                        binding.id
                                    )
                                })?;
                                let content =
                                    fieldbook::io::versions::backup_folder_v1::latest_content(
                                        n, sha,
                                    );
                                let escaped = content.replace('\n', "\\n");
                                out.push_str(&format!("latest {escaped}\n"));
                            }
                            other => {
                                return Err(format!(
                                    "binding `{}` case `{case_id}`: unknown backup op `{other}`",
                                    binding.id
                                ));
                            }
                        }
                    }
                    out
                }
                ("store", "folder@1") => {
                    let parsed: ScriptCaseInput = toml::from_str(input_str).map_err(|e| {
                        format!(
                            "binding `{}` case `{case_id}` invalid store folder input: {e}",
                            binding.id
                        )
                    })?;
                    let mut out = String::new();
                    for op in parsed.ops {
                        let (verb, rest) = op.split_once(' ').unwrap_or((op.as_str(), ""));
                        match verb {
                            "path" => {
                                let rel = rest;
                                match fieldbook::io::versions::store_folder_v1::check_rel_path(rel)
                                {
                                    Ok(()) => out.push_str("ok\n"),
                                    Err(refusal) => {
                                        out.push_str(&format!("refuse {}\n", refusal.word()));
                                    }
                                }
                            }
                            "sort" => {
                                let mut names: Vec<String> =
                                    rest.split_whitespace().map(str::to_owned).collect();
                                fieldbook::io::versions::store_folder_v1::byte_order(&mut names);
                                out.push_str(&names.join(" "));
                                out.push('\n');
                            }
                            "sha" => {
                                let hex =
                                    fieldbook::io::versions::store_folder_v1::content_sha256_hex(
                                        rest.as_bytes(),
                                    );
                                out.push_str(&hex);
                                out.push('\n');
                            }
                            other => {
                                return Err(format!(
                                    "binding `{}` case `{case_id}`: unknown store op `{other}`",
                                    binding.id
                                ));
                            }
                        }
                    }
                    out
                }
                ("authz", "policy-readers@1") => {
                    let parsed: AuthzCaseInput = toml::from_str(input_str).map_err(|e| {
                        format!(
                            "binding `{}` case `{case_id}` invalid authz input: {e}",
                            binding.id
                        )
                    })?;
                    struct InputGroups<'a> {
                        groups: &'a BTreeMap<String, toml::Value>,
                    }
                    impl fieldbook_core::authz::Groups for InputGroups<'_> {
                        fn in_group(&self, group: &str) -> Option<bool> {
                            match self.groups.get(group) {
                                Some(toml::Value::Boolean(b)) => Some(*b),
                                Some(toml::Value::String(s)) if s == "unreadable" => None,
                                _ => None,
                            }
                        }
                    }
                    let groups_lookup = InputGroups {
                        groups: &parsed.groups,
                    };
                    let acts = [
                        (fieldbook_core::authz::Act::Read, "read"),
                        (fieldbook_core::authz::Act::Propose, "propose"),
                        (fieldbook_core::authz::Act::Accept, "accept"),
                        (fieldbook_core::authz::Act::Export, "export"),
                        (fieldbook_core::authz::Act::Admin, "admin"),
                    ];
                    let mut out = String::new();
                    for store in parsed.store {
                        for (act, act_name) in acts {
                            let decision = fieldbook_core::versions::policy_readers_v1::allow(
                                act,
                                store.readers.as_deref(),
                                store.changers.as_deref(),
                                &groups_lookup,
                            );
                            match decision {
                                fieldbook_core::authz::AuthzDecision::Allow => {
                                    out.push_str(&format!("{} {act_name} allow\n", store.id));
                                }
                                fieldbook_core::authz::AuthzDecision::Deny(word) => {
                                    out.push_str(&format!(
                                        "{} {act_name} deny:{}\n",
                                        store.id,
                                        word.as_str()
                                    ));
                                }
                            }
                        }
                    }
                    out
                }
                ("index", "bm25f@1") => run_index_vector(input_str),
                ("correlator", "exact-ids+bm25@1") => run_correlator_vector(input_str),
                _ => {
                    return Err(format!(
                        "unknown adapter binding `{}` in runner dispatch",
                        binding.id
                    ));
                }
            }
        }
        RunnerKind::Template => {
            if binding.id.id == "ask@1" {
                execute_template_ask_1(input_str, case_id, &binding.id)?
            } else {
                // Legacy synthetic hook:
                let contract_file = binding
                    .files
                    .iter()
                    .find(|f| f.role == FileRole::Contract)
                    .ok_or_else(|| format!("binding `{}` missing contract file", binding.id))?;
                let contract_bytes = read_non_symlink_bytes(root, &contract_file.path)?;
                let contract_text = std::str::from_utf8(&contract_bytes)
                    .map_err(|e| format!("binding `{}` contract is not UTF-8: {e}", binding.id))?;
                if synthetic_template_contract(&binding.id.id, contract_text)? {
                    let table: toml::Table = toml::from_str(input_str).map_err(|e| {
                        format!(
                            "binding `{}` case `{case_id}` invalid template input TOML: {e}",
                            binding.id
                        )
                    })?;
                    let mut rendered = impl_text.to_owned();
                    for (k, v) in &table {
                        let placeholder = format!("{{{{{k}}}}}");
                        let replacement = v.as_str().map_or_else(|| v.to_string(), str::to_owned);
                        rendered = rendered.replace(&placeholder, &replacement);
                    }
                    rendered
                } else {
                    return Err(format!(
                        "unknown template binding `{}` in runner dispatch",
                        binding.id
                    ));
                }
            }
        }
        RunnerKind::SchemaKeyword => {
            return Err(format!(
                "runner `{}` for `{}` is not registered in this build",
                binding.runner, binding.id
            ));
        }
    };
    Ok(actual_output)
}

fn execute_vector(
    root: &Path,
    binding: &Binding,
    case_id: &str,
    input_bytes: &[u8],
    expected_bytes: &[u8],
) -> Result<(), String> {
    let expected_str = std::str::from_utf8(expected_bytes).map_err(|e| {
        format!(
            "binding `{}` case `{case_id}` expected is not UTF-8: {e}",
            binding.id
        )
    })?;
    let actual_output = run_vector(root, binding, case_id, input_bytes)?;
    if actual_output != expected_str {
        return Err(format!(
            "binding `{}` case `{case_id}` output mismatch:\nexpected:\n{expected_str}\nactual:\n{actual_output}",
            binding.id
        ));
    }
    Ok(())
}

fn check_binding_vectors(
    root: &Path,
    binding: &Binding,
    file_loader: &dyn Fn(&str) -> Result<Vec<u8>, String>,
) -> Result<(), String> {
    let mut inputs: BTreeMap<&str, &BoundFile> = BTreeMap::new();
    let mut expecteds: BTreeMap<&str, &BoundFile> = BTreeMap::new();

    for file in &binding.files {
        match file.role {
            FileRole::Input => {
                if let Some(c) = file.case.as_deref() {
                    inputs.insert(c, file);
                }
            }
            FileRole::Expected => {
                if let Some(c) = file.case.as_deref() {
                    expecteds.insert(c, file);
                }
            }
            FileRole::Contract | FileRole::Implementation => {}
        }
    }

    let mut has_positive = false;
    let mut has_refusal = false;

    for (case_id, in_file) in &inputs {
        let ex_file = expecteds.get(case_id).ok_or_else(|| {
            format!(
                "binding `{}` missing expected file for `{case_id}`",
                binding.id
            )
        })?;
        let in_bytes = file_loader(&in_file.path)?;
        let ex_bytes = file_loader(&ex_file.path)?;
        let ex_trimmed = std::str::from_utf8(&ex_bytes).unwrap_or("").trim();
        let is_refusal = match binding.runner {
            RunnerKind::Calculation => ex_trimmed.starts_with("FB05") && ex_trimmed.len() == 6,
            RunnerKind::Adapter | RunnerKind::Template => ex_trimmed
                .lines()
                .any(|l| l.starts_with("refuse") || l.contains(" deny:")),
            _ => false,
        };
        if is_refusal {
            has_refusal = true;
        } else {
            has_positive = true;
        }
        execute_vector(root, binding, case_id, &in_bytes, &ex_bytes)?;
    }

    if binding.runner == RunnerKind::Calculation && (!has_positive || !has_refusal) {
        return Err(format!(
            "calculation binding `{}` must include both a positive vector and a refusal vector",
            binding.id
        ));
    }

    if binding.runner == RunnerKind::Adapter || binding.runner == RunnerKind::Template {
        let is_pdftotext = binding.id.port.as_deref() == Some("extractor")
            && binding.id.id == "pdftotext-layout@1";
        if is_pdftotext {
            if !has_positive || inputs.len() < 2 {
                return Err(format!(
                    "adapter binding `{}` requires at least 2 positive vectors",
                    binding.id
                ));
            }
        } else {
            let is_synthetic = if binding.runner == RunnerKind::Template {
                match binding.files.iter().find(|f| f.role == FileRole::Contract) {
                    Some(contract_file) => match file_loader(&contract_file.path) {
                        Ok(c_bytes) => match std::str::from_utf8(&c_bytes) {
                            Ok(c_text) => synthetic_template_contract(&binding.id.id, c_text)?,
                            Err(_) => false,
                        },
                        Err(_) => false,
                    },
                    None => false,
                }
            } else {
                false
            };

            if !is_synthetic && (!has_positive || !has_refusal) {
                return Err(format!(
                    "binding `{}` must include both a positive vector and a refusal vector",
                    binding.id
                ));
            }
        }
    }

    Ok(())
}

fn git_tag_commit(root: &Path, tag: &str) -> Result<String, String> {
    let rev = format!("refs/tags/{tag}^{{commit}}");
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", &rev])
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .map_err(|e| format!("cannot run git rev-parse for tag `{tag}`: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "baseline tag `{tag}` does not exist in git history"
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn git_blob_at_commit(root: &Path, commit: &str, path: &str) -> Result<Vec<u8>, String> {
    let spec = format!("{commit}:{path}");
    let out = std::process::Command::new("git")
        .args(["cat-file", "-p", &spec])
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .map_err(|e| format!("cannot run git cat-file for `{spec}`: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "missing archived blob `{path}` at baseline commit `{commit}`"
        ));
    }
    Ok(out.stdout)
}

fn verify_stream_tag(root: &Path, stream_name: &str, base: &ReleaseBase) -> Result<(), String> {
    if !is_lower_hex(&base.archive_manifest_sha256, 64) {
        return Err(format!(
            "[{stream_name}] invalid archive_manifest_sha256 `{}`",
            base.archive_manifest_sha256
        ));
    }
    let actual_commit = git_tag_commit(root, &base.tag)?;
    if actual_commit != base.commit {
        return Err(format!(
            "[{stream_name}] tag `{}` resolves to `{actual_commit}`, expected `{}`",
            base.tag, base.commit
        ));
    }
    Ok(())
}

/// Run the compatibility check for `root` against `base_file`.
pub fn command(default_root: &Path, base_file: &Path) -> Result<bool, String> {
    let root = resolve_repo_root(default_root, base_file);
    let base_path = if base_file.is_absolute() {
        base_file.to_path_buf()
    } else if root.join(base_file).exists() {
        root.join(base_file)
    } else {
        base_file.to_path_buf()
    };

    if !base_path.exists() {
        return Err(format!(
            "baseline file `{}` does not exist",
            base_file.display()
        ));
    }
    let base_text = std::fs::read_to_string(&base_path)
        .map_err(|e| format!("cannot read baseline `{}`: {e}", base_path.display()))?;
    let baselines = match parse_baselines(&base_text) {
        Ok(b) => b,
        Err(msg) => {
            eprintln!("compat: {msg}");
            return Ok(false);
        }
    };

    let published_bytes = match read_non_symlink_bytes(&root, "compat/published.toml") {
        Ok(b) => b,
        Err(msg) => {
            eprintln!("compat: {msg}");
            return Ok(false);
        }
    };
    let published_text = match std::str::from_utf8(&published_bytes) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("compat: compat/published.toml is not UTF-8: {e}");
            return Ok(false);
        }
    };
    let published = match parse_published(published_text) {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("compat: {msg}");
            return Ok(false);
        }
    };

    if let Err(msg) = verify_all(&root, &published, &baselines) {
        eprintln!("compat: {msg}");
        return Ok(false);
    }
    Ok(true)
}

fn verify_all(root: &Path, published: &Published, baselines: &Baselines) -> Result<(), String> {
    // 1. Recompute current case-set SHA-256 and check EPOCH and compiled CALC_EPOCH before baseline comparison.
    let actual_case_sha = compute_repo_case_set_sha256(root)?;
    let compiled_epoch = read_compiled_calc_epoch(root);

    if published.case_set_sha256 != actual_case_sha {
        return Err(format!(
            "published.case_set_sha256 `{}` does not match current case set `{actual_case_sha}` (minimum epoch: {}, current 16-hex: {})",
            published.case_set_sha256,
            published.calculation_epoch,
            &actual_case_sha[..16]
        ));
    }
    if published.calculation_epoch != compiled_epoch {
        return Err(format!(
            "published.calculation_epoch {} does not match compiled CALC_EPOCH {compiled_epoch}",
            published.calculation_epoch
        ));
    }

    let epoch_bytes = read_non_symlink_bytes(root, "crates/fieldbook-calc/tests/cases/EPOCH")?;
    let epoch_text = std::str::from_utf8(&epoch_bytes)
        .map_err(|e| format!("EPOCH file is not UTF-8: {e}"))?
        .trim_end_matches(['\r', '\n']);
    let expected_epoch_line = format!("{} {}", published.calculation_epoch, &actual_case_sha[..16]);
    if epoch_text != expected_epoch_line {
        return Err(format!(
            "EPOCH file `{epoch_text}` does not match `{expected_epoch_line}`"
        ));
    }

    // 2. Verify that every registered row in CALC_FNS is covered by a calculation binding in published.toml.
    for row in fieldbook_calc::CALC_FNS {
        let covered = published
            .bindings
            .iter()
            .any(|b| b.id.kind == RunnerKind::Calculation && b.id.id == row.id);
        if !covered {
            return Err(format!(
                "CALC_FNS function `{}` has no binding in compat/published.toml",
                row.id
            ));
        }
    }

    // Verify that every Built row in ADAPTERS is covered by an adapter binding in published.toml.
    // The registry belongs to the tree under check, so a tree without it (a synthetic fixture of
    // the calculation bindings alone) has no rows to cover.
    let tree_has_registry = root
        .join("crates/fieldbook/src/adapters/registry.rs")
        .is_file();
    for row in fieldbook::adapters::registry::ADAPTERS {
        if tree_has_registry && row.state == fieldbook::adapters::registry::State::Built {
            let port_str = row.port.as_str();
            let covered = published.bindings.iter().any(|b| {
                b.id.kind == RunnerKind::Adapter
                    && b.id.port.as_deref() == Some(port_str)
                    && b.id.id == row.id
            });
            if !covered {
                return Err(format!(
                    "ADAPTERS adapter `({port_str}, {})` has no binding in compat/published.toml",
                    row.id
                ));
            }
        }
    }

    // Verify that every template in TEMPLATES is covered by a template binding in published.toml.
    for def in fieldbook_core::ports::templates::TEMPLATES {
        if !tree_has_registry {
            break;
        }
        let covered = published
            .bindings
            .iter()
            .any(|b| b.id.kind == RunnerKind::Template && b.id.id == def.id);
        if !covered {
            return Err(format!(
                "TEMPLATES template `{}` has no binding in compat/published.toml",
                def.id
            ));
        }
    }

    // 3. Verify every bound file on disk matches its declared sha256.
    for (path, expected_sha) in published.bound_files()? {
        let bytes = read_non_symlink_bytes(root, &path)?;
        let actual_sha = sha256_hex(&bytes);
        if actual_sha != expected_sha {
            return Err(format!(
                "bound file `{path}` sha256 `{actual_sha}` does not match manifest `{expected_sha}`"
            ));
        }
    }

    // 4. Verify tool and edition stream predecessors if declared.
    if let Some(tool_base) = baselines.tool_predecessor() {
        verify_stream_tag(root, "tool", tool_base)?;
    }
    if let Some(edition_base) = baselines.edition_predecessor() {
        verify_stream_tag(root, "edition", edition_base)?;
    }

    // 5. Compare against compatibility baseline (or execute all current vectors when empty).
    if let Some(ref compat_base) = baselines.compatibility {
        if !is_lower_hex(&compat_base.archive_manifest_sha256, 64) {
            return Err(format!(
                "[compatibility] invalid archive_manifest_sha256 `{}`",
                compat_base.archive_manifest_sha256
            ));
        }
        let tag_commit = git_tag_commit(root, &compat_base.tag)?;
        if tag_commit != compat_base.commit {
            return Err(format!(
                "[compatibility] tag `{}` resolves to `{tag_commit}`, expected `{}`",
                compat_base.tag, compat_base.commit
            ));
        }

        let mut archived_blobs: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for (path, expected_sha) in &compat_base.files {
            let blob = git_blob_at_commit(root, &compat_base.commit, path)?;
            let actual_sha = sha256_hex(&blob);
            if &actual_sha != expected_sha {
                return Err(format!(
                    "archived file `{path}` at `{}` has sha256 `{actual_sha}`, expected `{expected_sha}`",
                    compat_base.commit
                ));
            }
            archived_blobs.insert(path.clone(), blob);
        }

        let archived_pub_bytes = archived_blobs
            .get("compat/published.toml")
            .ok_or_else(|| "missing compat/published.toml in compatibility baseline".to_owned())?;
        let archived_pub_str = std::str::from_utf8(archived_pub_bytes)
            .map_err(|e| format!("archived compat/published.toml is not UTF-8: {e}"))?;
        let archived_published = parse_published(archived_pub_str)?;

        let mut expected_base_files = archived_published.bound_files()?;
        expected_base_files.push((
            "compat/published.toml".to_owned(),
            sha256_hex(archived_pub_bytes),
        ));
        expected_base_files.sort_by(|a, b| a.0.cmp(&b.0));
        if compat_base.files != expected_base_files {
            return Err(
                "[[compatibility.file]] rows do not match archived compat/published.toml and its bound files"
                    .to_owned(),
            );
        }

        // Case-set / epoch advancement rule against baseline.
        if published.case_set_sha256 != archived_published.case_set_sha256 {
            let min_epoch = archived_published.calculation_epoch + 1;
            if published.calculation_epoch <= archived_published.calculation_epoch {
                return Err(format!(
                    "case set changed from baseline without increasing calculation_epoch (minimum permitted epoch: {min_epoch}, current digest: {})",
                    &published.case_set_sha256[..16]
                ));
            }
        } else if published.calculation_epoch < archived_published.calculation_epoch {
            return Err(format!(
                "calculation_epoch {} is less than baseline epoch {}",
                published.calculation_epoch, archived_published.calculation_epoch
            ));
        }

        // Verify every archived binding is preserved unchanged and execute its archived vectors through current dispatch.
        let mut archived_ids = BTreeSet::new();
        for old_binding in &archived_published.bindings {
            archived_ids.insert(old_binding.id.clone());
            let Some(cur_binding) = published.bindings.iter().find(|b| b.id == old_binding.id)
            else {
                return Err(format!(
                    "archived binding `{}` was removed from compat/published.toml",
                    old_binding.id
                ));
            };
            if cur_binding.runner != old_binding.runner || cur_binding.files != old_binding.files {
                return Err(format!(
                    "archived binding `{}` was modified in compat/published.toml",
                    old_binding.id
                ));
            }
            check_binding_vectors(root, old_binding, &|path| {
                archived_blobs
                    .get(path)
                    .cloned()
                    .ok_or_else(|| format!("archived blob `{path}` not loaded"))
            })?;
        }

        // Execute vectors for any newly added bindings.
        for binding in &published.bindings {
            if !archived_ids.contains(&binding.id) {
                check_binding_vectors(root, binding, &|path| read_non_symlink_bytes(root, path))?;
            }
        }
    } else {
        for binding in &published.bindings {
            check_binding_vectors(root, binding, &|path| read_non_symlink_bytes(root, path))?;
        }
    }

    Ok(())
}

#[cfg(test)]
#[path = "../../tests/support/rng.rs"]
pub(crate) mod rng;

#[cfg(test)]
mod tests {
    #[test]
    fn synthetic_contract_for_a_live_template_id_is_refused() {
        let err = synthetic_template_contract("ask@1", "kind = \"template\"\nsynthetic = true\n")
            .expect_err("live template id must refuse synthetic");
        assert!(err.contains("ask@1"), "{err}");
    }

    #[test]
    fn synthetic_contract_for_an_absent_template_id_is_honoured() {
        let c = "kind = \"template\"\nsynthetic = true\n";
        assert_eq!(synthetic_template_contract("ask-answer@1", c), Ok(true));
        assert_eq!(
            synthetic_template_contract("ask@1", "kind = \"template\"\n"),
            Ok(false)
        );
    }

    use std::panic::{AssertUnwindSafe, catch_unwind};

    use super::{parse_baselines, parse_published, rng, synthetic_template_contract};

    const VALID_PUBLISHED: &str = include_str!("../../compat/published.toml");
    const VALID_EMPTY_BASELINE: &str = include_str!("../../release-baseline.toml");
    const VALID_RELEASE_BASELINE: &str = r#"baseline_schema = 1

[compatibility]
kind = "release"
tag = "v0.1.0"
commit = "0123456789abcdef0123456789abcdef01234567"
archive_manifest_sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

[[compatibility.file]]
path = "compat/published.toml"
sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

[tool]
kind = "release"
tag = "v0.1.0"
commit = "0123456789abcdef0123456789abcdef01234567"
archive_manifest_sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

[edition]
kind = "release"
tag = "edition-001"
commit = "89abcdef0123456789abcdef0123456789abcdef"
archive_manifest_sha256 = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210"
"#;

    fn assert_reader_never_panics<T, F>(name: &str, valid_samples: &[&str], reader: F)
    where
        F: Fn(&str) -> Result<T, String>,
    {
        for sample in valid_samples {
            let res = catch_unwind(AssertUnwindSafe(|| reader(sample)));
            match res {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => panic!("reader `{name}` rejected its own valid example: {e}"),
                Err(_) => panic!("reader `{name}` panicked on its own valid example"),
            }
        }

        let mut rng = rng::Rng::new(0xC04D_A716_2026_0016);
        for _ in 0..200 {
            let bytes = rng.bytes(256);
            let text = String::from_utf8_lossy(&bytes);
            if catch_unwind(AssertUnwindSafe(|| reader(&text))).is_err() {
                panic!("reader `{name}` panicked on seeded random bytes");
            }
        }

        for sample in valid_samples {
            let base = sample.as_bytes();
            for idx in 0..base.len() {
                let mut flipped = base.to_vec();
                flipped[idx] ^= 0x5A;
                let s = String::from_utf8_lossy(&flipped);
                if catch_unwind(AssertUnwindSafe(|| reader(&s))).is_err() {
                    panic!("reader `{name}` panicked on single-byte flip at index {idx}");
                }

                let mut deleted = base.to_vec();
                deleted.remove(idx);
                let s_del = String::from_utf8_lossy(&deleted);
                if catch_unwind(AssertUnwindSafe(|| reader(&s_del))).is_err() {
                    panic!("reader `{name}` panicked on single-byte deletion at index {idx}");
                }
            }
        }
    }

    #[test]
    fn manifest_and_baseline_parsers_never_panic() {
        assert_reader_never_panics("parse_published", &[VALID_PUBLISHED], parse_published);
        assert_reader_never_panics(
            "parse_baselines",
            &[VALID_EMPTY_BASELINE, VALID_RELEASE_BASELINE],
            parse_baselines,
        );
    }

    #[test]
    fn admit_compat_runner_detects_rendered_byte_and_hash_drift() {
        let root = crate::proc::repo_root();
        let published = parse_published(VALID_PUBLISHED).unwrap_or_else(|e| panic!("{e}"));
        let binding = published
            .bindings
            .iter()
            .find(|b| b.id.id == "corpus-add@1")
            .unwrap_or_else(|| panic!("corpus-add@1 binding missing"));

        let c1_in =
            super::read_non_symlink_bytes(&root, "compat/adapter/admit/corpus-add@1/c1.in.toml")
                .unwrap_or_else(|e| panic!("{e}"));
        let c1_expected =
            std::fs::read_to_string(root.join("compat/adapter/admit/corpus-add@1/c1.out.txt"))
                .unwrap_or_else(|e| panic!("{e}"));

        let actual =
            super::run_vector(&root, binding, "c1", &c1_in).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(actual, c1_expected);

        let drifted_in = String::from_utf8_lossy(&c1_in)
            .replace("Synthetic Compat Title", "Synthetic Drifted Title")
            .into_bytes();
        let drifted_out =
            super::run_vector(&root, binding, "c1", &drifted_in).unwrap_or_else(|e| panic!("{e}"));
        assert_ne!(
            drifted_out, c1_expected,
            "changing rendered sources.csv content must change corpus-add@1 compat output"
        );
    }
}
