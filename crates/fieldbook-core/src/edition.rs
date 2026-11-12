//! Pure edition metadata (`BuildInfo`, `BuildProfile`, `EditionInfo`, `RenderContext`)
//! and deterministic `edition.toml` serializer (`docs/reference/corpus-format.md#scope-and-versioning`).
//!
//! **Rules.** Pure: no I/O, no clock, no environment, no renderer dependency.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::date::CivilDate;
use crate::egress::LabelledView;
use crate::hash::sha256_hex;
use crate::ids::{EditionId, ToolRelease, VersionId};
use crate::manifest::{CorpusSource, ManifestError, fingerprint, shipped_corpus};
use crate::schema::vocab::Lifecycle;
use crate::version::min_tool_version;

/// Where a source version's stored original sits, as `<version-id>.<ext>`.
const PUBLIC_ORIGINALS_PREFIX: &str = "corpus/sources/public/";

/// The `location` of a version that has no stored file.
const METADATA_ONLY_PREFIX: &str = "metadata-only:";

/// The versions with a stored original in the view's tree. An original goes by its version id, as
/// the permitted view keeps it and the build copies it; a row's `location` says where it came from.
fn stored_original_versions(view: &LabelledView) -> BTreeSet<VersionId> {
    view.tree()
        .iter()
        .filter_map(|(path, _)| {
            let name = path.strip_prefix(PUBLIC_ORIGINALS_PREFIX)?;
            if name.contains('/') {
                return None;
            }
            let (stem, ext) = name.split_once('.')?;
            if ext.is_empty() {
                return None;
            }
            VersionId::parse(stem).ok()
        })
        .collect()
}

/// Fixed synthetic fallback epoch (`2000-01-01T00:00:00Z`) used for workbooks when
/// `SOURCE_DATE_EPOCH` is absent (`edition-manifest` step 4).
pub const DEFAULT_WORKBOOK_EPOCH: i64 = 946_684_800;

/// Compiled build profile (`silent` vs `ai`).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum BuildProfile {
    /// Default silent build (`connect-src 'none'`, no network/model imports).
    #[default]
    #[serde(rename = "silent")]
    Silent,
    /// AI-enabled build (`--features net`).
    #[serde(rename = "ai")]
    Ai,
}

impl BuildProfile {
    /// Wire word for the profile (`"silent"` or `"ai"`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Silent => "silent",
            Self::Ai => "ai",
        }
    }
}

impl fmt::Display for BuildProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Build stamp and profile supplied by the binary boundary (`SOURCE_DATE_EPOCH` and `FIELDBOOK_GIT_SHA`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildInfo {
    /// UTC calendar date derived from `SOURCE_DATE_EPOCH`, or `None` when absent.
    pub built: Option<CivilDate>,
    /// Compile-time commit SHA (`FIELDBOOK_GIT_SHA`), or `None` when absent.
    pub commit: Option<String>,
    /// Compiled build profile (`Silent` or `Ai`).
    pub profile: BuildProfile,
    /// Full validated `SOURCE_DATE_EPOCH` in seconds, or [`DEFAULT_WORKBOOK_EPOCH`] when absent.
    /// Not serialized into `edition.toml` and independent of [`Self::is_released`].
    pub workbook_epoch: i64,
}

impl Default for BuildInfo {
    fn default() -> Self {
        Self::unreleased(BuildProfile::Silent)
    }
}

impl BuildInfo {
    /// Construct an unreleased [`BuildInfo`] with [`DEFAULT_WORKBOOK_EPOCH`].
    #[must_use]
    pub const fn unreleased(profile: BuildProfile) -> Self {
        Self {
            built: None,
            commit: None,
            profile,
            workbook_epoch: DEFAULT_WORKBOOK_EPOCH,
        }
    }

    /// Construct a [`BuildInfo`] from optional `built`, optional `commit`, `profile`,
    /// and optional `source_date_epoch` (falling back to [`DEFAULT_WORKBOOK_EPOCH`]).
    #[must_use]
    pub const fn new(
        built: Option<CivilDate>,
        commit: Option<String>,
        profile: BuildProfile,
        source_date_epoch: Option<i64>,
    ) -> Self {
        Self {
            built,
            commit,
            profile,
            workbook_epoch: match source_date_epoch {
                Some(epoch) => epoch,
                None => DEFAULT_WORKBOOK_EPOCH,
            },
        }
    }

    /// Returns `true` iff `self.built` is `Some(_)` and `self.commit` is `Some(c)` where
    /// `c` is exactly 40 lowercase hexadecimal characters.
    #[must_use]
    pub fn is_released(&self) -> bool {
        self.built.is_some() && self.commit.as_deref().is_some_and(|c| is_lower_hex(c, 40))
    }
}

/// Record counts in `[counts]` of `edition.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditionCounts {
    /// Number of pages in the edition.
    pub pages: usize,
    /// Number of source version rows in `sources.csv`.
    pub source_versions: usize,
    /// Number of claims in `claims.csv`.
    pub claims: usize,
    /// Number of questions in `questions.csv`.
    pub questions: usize,
    /// Number of conflicts in `conflicts.csv`.
    pub conflicts: usize,
    /// Number of glossary terms in `glossary.csv`.
    pub terms: usize,
    /// Number of shipped non-quarantined originals in `originals/`.
    pub originals: usize,
}

/// Sorted record identifier lists in `[ids]` of `edition.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditionIds {
    /// Sorted page identifiers.
    pub pages: Vec<String>,
    /// Sorted source version identifiers (`S-NNNN-vN`).
    pub source_versions: Vec<String>,
    /// Sorted claim identifiers.
    pub claims: Vec<String>,
    /// Sorted question identifiers.
    pub questions: Vec<String>,
    /// Sorted conflict identifiers.
    pub conflicts: Vec<String>,
    /// Sorted glossary term identifiers.
    pub terms: Vec<String>,
}

/// Validated `edition.toml` metadata (`corpus-format.md#scope-and-versioning`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditionInfo {
    /// `"edition-NNN"` when released, or `"unreleased"` when unreleased.
    pub edition: String,
    /// Positive edition number when released, or `0` when unreleased.
    pub number: u32,
    /// UTC build date `"YYYY-MM-DD"` when released, or `""` when unreleased.
    pub built: String,
    /// Version of `fieldbook` that built the edition (`"X.Y.Z"`).
    pub tool_version: String,
    /// Oldest tool version that supports `schema_version` (`"0.1.0"` for schema 1).
    pub min_tool_version: String,
    /// Schema version copied from `corpus/corpus.toml`.
    pub schema_version: u32,
    /// 40 lowercase hex commit SHA when released, or `""` when unreleased.
    pub commit: String,
    /// 64 lowercase hex SHA-256 fingerprint over the shipped Corpus files.
    pub fingerprint: String,
    /// `[counts]` table.
    pub counts: EditionCounts,
    /// `[ids]` table.
    pub ids: EditionIds,
}

/// Error returned when constructing or parsing [`EditionInfo`] or [`RenderContext`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetadataError {
    /// The input view is not a valid public-only Corpus view.
    Manifest(ManifestError),
    /// `schema_version` is unsupported or disagrees with `corpus/corpus.toml`.
    InvalidSchemaVersion {
        /// Supplied schema version.
        supplied: u32,
        /// Reason for rejection.
        reason: String,
    },
    /// `tool_version` is not a valid `X.Y.Z` semantic version.
    InvalidToolVersion(String),
    /// A released build (`info.is_released() == true`) is missing a valid `edition-NNN` heading,
    /// or a supplied heading is malformed.
    InvalidEditionHeading(String),
    /// The supplied `files` slice does not match the complete shipped Corpus selection.
    IncompleteOrMismatchedSelection(String),
    /// Failed to parse or validate `edition.toml`.
    ParseError(String),
}

impl fmt::Display for MetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Manifest(e) => write!(f, "{e}"),
            Self::InvalidSchemaVersion { supplied, reason } => {
                write!(f, "invalid schema_version {supplied}: {reason}")
            }
            Self::InvalidToolVersion(v) => write!(f, "invalid tool_version `{v}`"),
            Self::InvalidEditionHeading(msg) => write!(f, "invalid edition heading: {msg}"),
            Self::IncompleteOrMismatchedSelection(msg) => {
                write!(
                    f,
                    "incomplete or mismatched shipped corpus selection: {msg}"
                )
            }
            Self::ParseError(msg) => write!(f, "edition.toml parse error: {msg}"),
        }
    }
}

impl std::error::Error for MetadataError {}

impl From<ManifestError> for MetadataError {
    fn from(value: ManifestError) -> Self {
        Self::Manifest(value)
    }
}

fn is_lower_hex(s: &str, len: usize) -> bool {
    s.len() == len
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn normalize_tool_version(tool_version: &str) -> Result<String, MetadataError> {
    let trimmed = tool_version.strip_prefix('v').unwrap_or(tool_version);
    if trimmed.is_empty() || ToolRelease::parse(&format!("v{trimmed}")).is_err() {
        return Err(MetadataError::InvalidToolVersion(tool_version.to_owned()));
    }
    Ok(trimmed.to_owned())
}

fn parse_heading_str(raw: &str) -> Result<EditionId, MetadataError> {
    let trimmed = raw.trim();
    let candidate = trimmed.strip_prefix("##").map(str::trim).unwrap_or(trimmed);
    let normalized = if candidate.starts_with("edition-") {
        candidate.to_owned()
    } else {
        format!("edition-{candidate}")
    };
    EditionId::parse(&normalized).map_err(|_| {
        MetadataError::InvalidEditionHeading(format!("expected `edition-NNN` heading, got `{raw}`"))
    })
}

/// Extract the newest `## edition-NNN` heading token (`"edition-NNN"`) from `changes.md`,
/// or `None` if no level-2 edition heading is present.
#[must_use]
pub fn latest_edition_heading(changes_md: &str) -> Option<&str> {
    for line in changes_md.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("## ") {
            let token = rest.trim();
            if EditionId::parse(token).is_ok() {
                return Some(token);
            }
        }
    }
    None
}

fn escape_toml_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn format_toml_string_array(items: &[String]) -> String {
    let mut out = String::from("[");
    for (idx, item) in items.iter().enumerate() {
        if idx > 0 {
            out.push_str(", ");
        }
        out.push_str(&escape_toml_string(item));
    }
    out.push(']');
    out
}

impl EditionInfo {
    /// Validate the public [`LabelledView`], the complete shipped Corpus file selection
    /// (including `corpus/eval/regression.toml` when shipped in raw mode), the edition heading,
    /// and the [`BuildInfo`] stamp, returning a deterministic [`EditionInfo`].
    pub fn from(
        view: &LabelledView,
        files: &[(String, String)],
        heading: Option<&str>,
        info: &BuildInfo,
        tool_version: &str,
        schema_version: u32,
    ) -> Result<Self, MetadataError> {
        let _tree = view.public_tree()?;

        let Some(corpus_toml) = view.corpus() else {
            return Err(MetadataError::Manifest(ManifestError::MissingRequired(
                "corpus/corpus.toml".to_owned(),
            )));
        };
        if corpus_toml.schema_version != schema_version {
            return Err(MetadataError::InvalidSchemaVersion {
                supplied: schema_version,
                reason: format!(
                    "does not match corpus.toml schema_version {}",
                    corpus_toml.schema_version
                ),
            });
        }
        let Some(min_tool) = min_tool_version(schema_version) else {
            return Err(MetadataError::InvalidSchemaVersion {
                supplied: schema_version,
                reason: "unsupported schema_version".to_owned(),
            });
        };
        let norm_tool_version = normalize_tool_version(tool_version)?;

        // Collect quarantined versions from the view's sources.
        let mut quarantined: BTreeSet<VersionId> = BTreeSet::new();
        for s in view.sources() {
            if s.value.lifecycle == Lifecycle::Quarantined
                && let Ok(vid) =
                    VersionId::parse(&format!("{}-v{}", s.value.source_id, s.value.version))
            {
                quarantined.insert(vid);
            }
        }

        // Validate that `files` is sorted and equals the complete `shipped_corpus` selection
        // (or `shipped_corpus` + `corpus/eval/regression.toml` when present in `view.tree()`).
        let expected_base = shipped_corpus(view, &quarantined)?;
        let mut expected_with_regression = expected_base.clone();
        if let Some(reg_bytes) = view.tree().get_bytes("corpus/eval/regression.toml") {
            let reg_pair = (
                "corpus/eval/regression.toml".to_owned(),
                sha256_hex(reg_bytes),
            );
            if !expected_with_regression
                .iter()
                .any(|(p, _)| p == &reg_pair.0)
            {
                expected_with_regression.push(reg_pair);
                expected_with_regression.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
            }
        }

        if files != expected_base.as_slice() && files != expected_with_regression.as_slice() {
            return Err(MetadataError::IncompleteOrMismatchedSelection(format!(
                "supplied {} files do not match expected shipped corpus selection ({} base files)",
                files.len(),
                expected_base.len()
            )));
        }

        let (edition, number, built, commit) = match (info.built, info.commit.as_deref()) {
            (Some(built_date), Some(commit_sha)) if is_lower_hex(commit_sha, 40) => {
                let Some(raw_heading) = heading else {
                    return Err(MetadataError::InvalidEditionHeading(
                        "released build requires an `edition-NNN` heading".to_owned(),
                    ));
                };
                let eid = parse_heading_str(raw_heading)?;
                (
                    eid.as_str().to_owned(),
                    eid.number(),
                    built_date.to_string(),
                    commit_sha.to_owned(),
                )
            }
            _ => {
                if let Some(raw_heading) = heading
                    && !raw_heading.trim().is_empty()
                {
                    let _ = parse_heading_str(raw_heading)?;
                }
                ("unreleased".to_owned(), 0, String::new(), String::new())
            }
        };

        let mut page_ids: Vec<String> = view
            .pages()
            .iter()
            .map(|p| p.id.as_str().to_owned())
            .collect();
        page_ids.sort();

        let mut source_version_ids: Vec<String> = Vec::with_capacity(view.sources().len());
        let stored_originals = stored_original_versions(view);
        let mut originals = 0;
        for s in view.sources() {
            let vid_str = format!("{}-v{}", s.value.source_id, s.value.version);
            if s.value.lifecycle != Lifecycle::Quarantined
                && !s.value.location.starts_with(METADATA_ONLY_PREFIX)
                && VersionId::parse(&vid_str).is_ok_and(|vid| stored_originals.contains(&vid))
            {
                originals += 1;
            }
            source_version_ids.push(vid_str);
        }
        source_version_ids.sort();

        let mut claim_ids: Vec<String> = view
            .claims()
            .iter()
            .map(|c| c.value.claim_id.as_str().to_owned())
            .collect();
        claim_ids.sort();

        let mut question_ids: Vec<String> = view
            .questions()
            .iter()
            .map(|q| q.value.question_id.as_str().to_owned())
            .collect();
        question_ids.sort();

        let mut conflict_ids: Vec<String> = view
            .conflicts()
            .iter()
            .map(|x| x.value.conflict_id.as_str().to_owned())
            .collect();
        conflict_ids.sort();

        let mut term_ids: Vec<String> = view
            .glossary()
            .iter()
            .map(|g| g.value.term_id.as_str().to_owned())
            .collect();
        term_ids.sort();

        let counts = EditionCounts {
            pages: page_ids.len(),
            source_versions: source_version_ids.len(),
            claims: claim_ids.len(),
            questions: question_ids.len(),
            conflicts: conflict_ids.len(),
            terms: term_ids.len(),
            originals,
        };

        let ids = EditionIds {
            pages: page_ids,
            source_versions: source_version_ids,
            claims: claim_ids,
            questions: question_ids,
            conflicts: conflict_ids,
            terms: term_ids,
        };

        Ok(Self {
            edition,
            number,
            built,
            tool_version: norm_tool_version,
            min_tool_version: min_tool.to_owned(),
            schema_version,
            commit,
            fingerprint: fingerprint(files),
            counts,
            ids,
        })
    }

    /// Serialize [`EditionInfo`] to `edition.toml` in the fixed key order of
    /// `docs/reference/corpus-format.md#scope-and-versioning`.
    #[must_use]
    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "edition = {}\n",
            escape_toml_string(&self.edition)
        ));
        out.push_str(&format!("number = {}\n", self.number));
        out.push_str(&format!("built = {}\n", escape_toml_string(&self.built)));
        out.push_str(&format!(
            "tool_version = {}\n",
            escape_toml_string(&self.tool_version)
        ));
        out.push_str(&format!(
            "min_tool_version = {}\n",
            escape_toml_string(&self.min_tool_version)
        ));
        out.push_str(&format!("schema_version = {}\n", self.schema_version));
        out.push_str(&format!("commit = {}\n", escape_toml_string(&self.commit)));
        out.push_str(&format!(
            "fingerprint = {}\n",
            escape_toml_string(&self.fingerprint)
        ));

        out.push_str("\n[counts]\n");
        out.push_str(&format!("pages = {}\n", self.counts.pages));
        out.push_str(&format!(
            "source_versions = {}\n",
            self.counts.source_versions
        ));
        out.push_str(&format!("claims = {}\n", self.counts.claims));
        out.push_str(&format!("questions = {}\n", self.counts.questions));
        out.push_str(&format!("conflicts = {}\n", self.counts.conflicts));
        out.push_str(&format!("terms = {}\n", self.counts.terms));
        out.push_str(&format!("originals = {}\n", self.counts.originals));

        out.push_str("\n[ids]\n");
        out.push_str(&format!(
            "pages = {}\n",
            format_toml_string_array(&self.ids.pages)
        ));
        out.push_str(&format!(
            "source_versions = {}\n",
            format_toml_string_array(&self.ids.source_versions)
        ));
        out.push_str(&format!(
            "claims = {}\n",
            format_toml_string_array(&self.ids.claims)
        ));
        out.push_str(&format!(
            "questions = {}\n",
            format_toml_string_array(&self.ids.questions)
        ));
        out.push_str(&format!(
            "conflicts = {}\n",
            format_toml_string_array(&self.ids.conflicts)
        ));
        out.push_str(&format!(
            "terms = {}\n",
            format_toml_string_array(&self.ids.terms)
        ));
        out
    }

    /// Parse and validate `edition.toml` text.
    pub fn parse(toml_str: &str) -> Result<Self, MetadataError> {
        let parsed: Self =
            toml::from_str(toml_str).map_err(|e| MetadataError::ParseError(e.to_string()))?;
        let Some(expected_min_tool) = min_tool_version(parsed.schema_version) else {
            return Err(MetadataError::InvalidSchemaVersion {
                supplied: parsed.schema_version,
                reason: "unsupported schema_version".to_owned(),
            });
        };
        if parsed.min_tool_version != expected_min_tool {
            return Err(MetadataError::ParseError(format!(
                "min_tool_version `{}` does not match `{expected_min_tool}`",
                parsed.min_tool_version
            )));
        }
        let _ = normalize_tool_version(&parsed.tool_version)?;
        if !is_lower_hex(&parsed.fingerprint, 64) {
            return Err(MetadataError::ParseError(
                "fingerprint must be 64 lowercase hex characters".to_owned(),
            ));
        }
        if parsed.edition == "unreleased" {
            if parsed.number != 0 || !parsed.built.is_empty() || !parsed.commit.is_empty() {
                return Err(MetadataError::ParseError(
                    "unreleased edition.toml must have number = 0 and empty built/commit"
                        .to_owned(),
                ));
            }
        } else {
            let eid = EditionId::parse(&parsed.edition).map_err(|_| {
                MetadataError::ParseError(format!("invalid edition id `{}`", parsed.edition))
            })?;
            if parsed.number != eid.number() {
                return Err(MetadataError::ParseError(format!(
                    "edition number {} does not match `{}`",
                    parsed.number, parsed.edition
                )));
            }
            if CivilDate::parse(&parsed.built).is_err() {
                return Err(MetadataError::ParseError(format!(
                    "invalid built date `{}`",
                    parsed.built
                )));
            }
            if !is_lower_hex(&parsed.commit, 40) {
                return Err(MetadataError::ParseError(format!(
                    "invalid commit `{}`",
                    parsed.commit
                )));
            }
        }
        Ok(parsed)
    }
}

/// Rendering metadata context carried by site renderers and output wrappers (`edition-manifest` step 9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderContext {
    /// Public edition rendering context carrying validated [`EditionInfo`].
    Public(EditionInfo),
}

impl RenderContext {
    /// Construct [`RenderContext::Public`] with [`EditionInfo::from`]'s full public-view,
    /// shipped-selection, and heading validation.
    pub fn public(
        view: &LabelledView,
        files: &[(String, String)],
        heading: Option<&str>,
        info: &BuildInfo,
        tool_version: &str,
        schema_version: u32,
    ) -> Result<Self, MetadataError> {
        let edition_info =
            EditionInfo::from(view, files, heading, info, tool_version, schema_version)?;
        Ok(Self::Public(edition_info))
    }

    /// Return the underlying [`EditionInfo`] for a public edition context.
    #[must_use]
    pub const fn edition_info(&self) -> &EditionInfo {
        match self {
            Self::Public(info) => info,
        }
    }

    /// Planned metadata output paths contributed by this context (`["edition.toml"]` for [`Self::Public`]).
    #[must_use]
    pub const fn planned_paths(&self) -> &'static [&'static str] {
        match self {
            Self::Public(_) => &["edition.toml"],
        }
    }

    /// Serialize `edition.toml` when this context emits one.
    #[must_use]
    pub fn to_edition_toml(&self) -> Option<String> {
        match self {
            Self::Public(info) => Some(info.to_toml()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::audit::Now;
    use crate::authz::ReadableStores;
    use crate::check::{Entry, Host, Options, Tree, load};
    use crate::date::CivilDate;
    use crate::egress::{Decision, Dest, LabelledView, Shipped, check as egress_check};
    use crate::hash::sha256_hex;
    use crate::ids::VersionId;
    use crate::manifest::{fingerprint, shipped_corpus};
    use crate::policy::EffectivePolicy;
    use crate::schema::Register;

    use super::{
        BuildInfo, BuildProfile, DEFAULT_WORKBOOK_EPOCH, EditionInfo, MetadataError, RenderContext,
        latest_edition_heading,
    };

    const MINI_CORPUS_TOML: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/corpus.toml");

    const SOURCES_HEADER: &str = "source_id,version,tag,title,publisher,source_kind,source_role,authority_tier,licence,source_family,family_basis,location,sha256,published,effective_from,as_of,retrieved,lifecycle,supersedes,volatility,handling,note,purpose,origin_system,origin_stream,origin_key,doc_kind,series\n";
    const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    fn build_public_view() -> LabelledView {
        public_view_with_sources(
            &format!(
                "{SOURCES_HEADER}S-0001,1,syn-one,Synthetic Active Source,Synthetic Board,primary_law,primary_evidence,A,Synthetic,syn_one,reviewed,S-0001-v1.txt,{EMPTY_SHA256},2026-01-10,2026-01-15,2026-02-01,2026-02-01,active,,V2,public,Active fixture.,Baseline,,,,,\n"
            ),
            &["S-0001-v1.txt"],
        )
    }

    fn public_view_with_sources(sources_csv: &str, originals: &[&str]) -> LabelledView {
        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes());
        tree.insert_bytes(
            format!("corpus/{}", crate::strings::paths()[6]),
            b"rules_version = 1\n",
        );
        tree.insert_bytes(
            "corpus/changes.md",
            b"# Corpus changes\n\nSynthetic changes log.\n\n## edition-007\n\n- Added: synthetic.\n\n## edition-006\n\n- Added: older.\n",
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
        tree.insert_bytes("corpus/registers/sources.csv", sources_csv.as_bytes());
        for name in originals {
            tree.insert(
                format!("corpus/sources/public/{name}"),
                Entry::Original {
                    sha256: EMPTY_SHA256.to_owned(),
                    len: 0,
                },
            );
        }
        let page_text: &[u8] =
            b"synthetic text, long enough that no page is thin in this fixture\n";
        tree.insert_bytes("corpus/sources/text/S-0001-v1/p0001.txt", page_text);
        tree.insert_bytes(
            "corpus/sources/text/S-0001-v1/extraction.toml",
            format!(
                "tool = \"plain\"\ntool_version = \"1\"\nmethod = \"plain\"\npage_count = 1\nthin_pages = []\noriginal_sha256 = \"{EMPTY_SHA256}\"\n\n[pages]\np0001 = \"{}\"\n",
                crate::hash::sha256_hex(page_text)
            )
            .into_bytes(),
        );
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-sample.md",
            b"+++\nid = \"cmp-sample\"\ntitle = \"Synthetic Title\"\ntype = \"concept\"\narea = \"cmp\"\nsubarea = \"business-systems\"\nstage = 3\nstatus = \"draft\"\nreadiness = \"SYNTHETIC-NOW\"\nas_of = 2026-02-01\nhandling = [\"public\"]\nvolatility = \"V2\"\n+++\n\n## Purpose\n\nSynthetic.\n\n## The rule\n\nSynthetic.\n\n## How it works\n\nSynthetic.\n\n## Worked example\n\nSynthetic example. The numbers are made up.\n\n## Never confuse\n\nSynthetic.\n\n## Company calibration\n\nSynthetic.\n\n## Sources\n\n## Change note\n\n- NEW_KNOWLEDGE, 2026-02-01: Synthetic.\n",
        );
        tree.insert_bytes("corpus/eval/regression.toml", b"case = []\n");

        let today = CivilDate::from_ymd(2026, 3, 1).expect("valid date");
        let (model, report) = load(&Host::new(tree, today), &Options::default());
        assert_eq!(report.errors(), 0, "{:?}", report.diagnostics);

        let eff = EffectivePolicy::floor(false);
        let now = Now::new(today, 0).expect("valid now");
        let Decision::Allowed(allowed) =
            egress_check(&eff, &Dest::Edition, None, None, now, Shipped::Hours(0))
        else {
            panic!("edition allowed");
        };
        LabelledView::new(&model, &allowed, &ReadableStores::corpus())
    }

    /// An original goes by its version id, and `location` says where it came from: a real row holds
    /// a web address. A held version and a metadata-only version count for nothing.
    #[test]
    fn originals_count_by_version_id_not_location() {
        let row = |id: &str, tag: &str, location: &str, lifecycle: &str, note: &str| {
            let family = tag.replace('-', "_");
            format!(
                "{id},1,{tag},Synthetic {tag},Synthetic Board,primary_law,primary_evidence,A,Synthetic,{family},reviewed,{location},{EMPTY_SHA256},2026-01-10,2026-01-15,2026-02-01,2026-02-01,{lifecycle},,V2,public,{note},Baseline,,,,,\n"
            )
        };
        let sources = [
            row(
                "S-0001",
                "syn-web",
                "https://example.invalid/synthetic-one",
                "active",
                "Web address.",
            ),
            row(
                "S-0002",
                "syn-held",
                "https://example.invalid/synthetic-two",
                "quarantined",
                "Held for a synthetic reason.",
            ),
            row(
                "S-0003",
                "syn-meta",
                "metadata-only: https://example.invalid/synthetic-three",
                "active",
                "Metadata only.",
            ),
        ]
        .concat();
        let view = public_view_with_sources(
            &format!("{SOURCES_HEADER}{sources}"),
            &["S-0001-v1.txt", "S-0002-v1.txt"],
        );
        let held = BTreeSet::from([VersionId::parse("S-0002-v1").expect("valid id")]);
        let files = shipped_corpus(&view, &held).expect("shipped_corpus");
        let info = EditionInfo::from(&view, &files, None, &BuildInfo::default(), "0.1.0", 1)
            .expect("unreleased EditionInfo");
        assert_eq!(info.counts.source_versions, 3);
        assert_eq!(info.counts.originals, 1);
    }

    #[test]
    fn edition_metadata_without_renderer() {
        let view = build_public_view();
        let files = shipped_corpus(&view, &BTreeSet::new()).expect("shipped_corpus");
        let expected_fp = fingerprint(&files);

        let changes_text = std::str::from_utf8(
            view.tree()
                .get_bytes("corpus/changes.md")
                .expect("changes.md"),
        )
        .expect("utf8");
        let heading = latest_edition_heading(changes_text);
        assert_eq!(heading, Some("edition-007"));

        let built_date = CivilDate::from_ymd(2026, 3, 15).expect("valid date");
        let commit_sha = "0123456789abcdef0123456789abcdef01234567";
        let released_info = BuildInfo::new(
            Some(built_date),
            Some(commit_sha.to_owned()),
            BuildProfile::Silent,
            Some(1_773_532_800),
        );
        assert!(released_info.is_released());
        assert_eq!(released_info.workbook_epoch, 1_773_532_800);

        let edition_info = EditionInfo::from(&view, &files, heading, &released_info, "0.1.0", 1)
            .expect("released EditionInfo");

        let expected_toml = format!(
            "edition = \"edition-007\"\nnumber = 7\nbuilt = \"2026-03-15\"\ntool_version = \"0.1.0\"\nmin_tool_version = \"0.1.0\"\nschema_version = 1\ncommit = \"0123456789abcdef0123456789abcdef01234567\"\nfingerprint = \"{expected_fp}\"\n\n[counts]\npages = 1\nsource_versions = 1\nclaims = 0\nquestions = 0\nconflicts = 0\nterms = 0\noriginals = 1\n\n[ids]\npages = [\"cmp-sample\"]\nsource_versions = [\"S-0001-v1\"]\nclaims = []\nquestions = []\nconflicts = []\nterms = []\n"
        );
        assert_eq!(edition_info.to_toml(), expected_toml);
        assert!(!edition_info.to_toml().contains("workbook_epoch"));
        assert_eq!(
            EditionInfo::parse(&edition_info.to_toml()).expect("round-trip parse"),
            edition_info
        );

        // RenderContext::Public serializes to the exact same metadata vector and includes edition.toml.
        let ctx = RenderContext::public(&view, &files, heading, &released_info, "0.1.0", 1)
            .expect("public RenderContext");
        assert_eq!(ctx.planned_paths(), &["edition.toml"]);
        assert_eq!(
            ctx.to_edition_toml().as_deref(),
            Some(expected_toml.as_str())
        );

        // Raw edition selection (including corpus/eval/regression.toml) is also accepted.
        let mut raw_files = files.clone();
        let reg_sha = sha256_hex(
            view.tree()
                .get_bytes("corpus/eval/regression.toml")
                .expect("regression.toml"),
        );
        raw_files.push(("corpus/eval/regression.toml".to_owned(), reg_sha));
        raw_files.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        let raw_edition_info =
            EditionInfo::from(&view, &raw_files, heading, &released_info, "0.1.0", 1)
                .expect("raw selection with regression.toml succeeds");
        assert_eq!(raw_edition_info.fingerprint, fingerprint(&raw_files));

        // Dropping a shipped file from `files` MUST fail.
        let mut dropped = files.clone();
        dropped.pop();
        assert!(matches!(
            EditionInfo::from(&view, &dropped, heading, &released_info, "0.1.0", 1),
            Err(MetadataError::IncompleteOrMismatchedSelection(_))
        ));

        // Released build without heading MUST fail.
        assert!(matches!(
            EditionInfo::from(&view, &files, None, &released_info, "0.1.0", 1),
            Err(MetadataError::InvalidEditionHeading(_))
        ));

        // Incomplete date/commit (missing date, missing commit, or non-40-hex commit) is NOT released
        // and produces edition = "unreleased", number = 0, built = "", commit = "".
        for incomplete in [
            BuildInfo::unreleased(BuildProfile::Silent),
            BuildInfo::new(Some(built_date), None, BuildProfile::Silent, None),
            BuildInfo::new(
                None,
                Some(commit_sha.to_owned()),
                BuildProfile::Silent,
                None,
            ),
            BuildInfo::new(
                Some(built_date),
                Some("short-sha".to_owned()),
                BuildProfile::Silent,
                None,
            ),
        ] {
            assert!(
                !incomplete.is_released(),
                "incomplete BuildInfo must not be marked released: {incomplete:?}"
            );
            assert_eq!(incomplete.workbook_epoch, DEFAULT_WORKBOOK_EPOCH);
            let unrel = EditionInfo::from(&view, &files, None, &incomplete, "0.1.0", 1)
                .expect("unreleased metadata");
            assert_eq!(unrel.edition, "unreleased");
            assert_eq!(unrel.number, 0);
            assert_eq!(unrel.built, "");
            assert_eq!(unrel.commit, "");
            assert_eq!(unrel.fingerprint, expected_fp);
        }
    }
}
