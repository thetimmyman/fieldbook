//! Published compatibility manifest, baseline descriptor, and historical-vector runner
//! (`docs/reference/compatibility.md`, `AD-14`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use fieldbook_calc::{CALC_EPOCH, ReplayCase, Run};
use fieldbook_core::cases::case_set_sha256_entries;
use fieldbook_core::hash::sha256_hex;
use fieldbook_core::ids::{EditionId, ToolRelease};
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
    if name.is_empty()
        || !name.as_bytes()[0].is_ascii_lowercase()
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
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

fn execute_vector(
    root: &Path,
    binding: &Binding,
    case_id: &str,
    input_bytes: &[u8],
    expected_bytes: &[u8],
) -> Result<(), String> {
    let input_str = std::str::from_utf8(input_bytes).map_err(|e| {
        format!(
            "binding `{}` case `{case_id}` input is not UTF-8: {e}",
            binding.id
        )
    })?;
    let expected_str = std::str::from_utf8(expected_bytes).map_err(|e| {
        format!(
            "binding `{}` case `{case_id}` expected is not UTF-8: {e}",
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
            // Verify the implementation file on disk was not tampered with to alter calculation constants.
            let current_impl_sha = sha256_hex(&impl_bytes);
            if current_impl_sha != impl_file.sha256 {
                return Err(format!(
                    "binding `{}` implementation `{}` sha256 mismatch on disk",
                    binding.id, impl_file.path
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
        RunnerKind::Template => {
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
        }
        RunnerKind::Adapter | RunnerKind::SchemaKeyword => {
            return Err(format!(
                "runner `{}` for `{}` is not registered in this build",
                binding.runner, binding.id
            ));
        }
    };

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
        if ex_trimmed.starts_with("FB05") && ex_trimmed.len() == 6 {
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
mod rng;

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use super::{parse_baselines, parse_published, rng};

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
}
