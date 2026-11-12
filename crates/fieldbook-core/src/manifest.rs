//! Shared public Corpus file selection, fingerprint, `SHA256SUMS` writer, and
//! data-only public support catalogue (`docs/reference/corpus-format.md#scope-and-versioning`,
//! `docs/reference/audit-contract.md#public-support-verification`).
//!
//! **Rules.** Pure: no I/O, no clock, no environment, no embedded Corpus bytes or
//! edition-specific hashes.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::Deserialize;
use unicode_normalization::UnicodeNormalization;

use crate::check::{Entry, Tree};
use crate::egress::LabelledView;
use crate::hash::sha256_hex;
use crate::ids::VersionId;
use crate::labels::{LabelAllow, PUBLIC};
use crate::scan::marking;
use crate::schema::page::{parse_meta, split};
use crate::schema::table;
use crate::schema::vocab::{Lifecycle, PageType};
use crate::schema::{CorpusToml, Register, SourceRow};
use crate::strings;
use crate::version::min_tool_version;

/// Maximum byte size of a `corpus-support.toml` input (`1 MiB`).
pub const MAX_SUPPORT_CATALOGUE_BYTES: usize = 1_048_576;

/// Maximum number of `[[file]]` entries in a `corpus-support.toml` input (`4096`).
pub const MAX_SUPPORT_CATALOGUE_ENTRIES: usize = 4096;

/// Error returned by [`shipped_corpus`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    /// The input view or tree is not a coherent public-only Corpus view.
    NonPublicView(String),
    /// A required Corpus file is absent or not byte-backed.
    MissingRequired(String),
    /// An included or candidate file is not `public` (or fails front-matter/register/marking checks).
    NonPublicFile {
        /// Repository-relative path of the offending file.
        path: String,
        /// Explanation of why the file is refused.
        reason: String,
    },
    /// A quarantined source version is missing its retained row, hash, or reason in `sources.csv`.
    QuarantinedRowInvalid {
        /// Version identifier (`S-NNNN-vN`).
        version: VersionId,
        /// Explanation of what is missing.
        reason: String,
    },
    /// An unexpected or malformed path was encountered under `corpus/`.
    UnexpectedPath {
        /// Offending path.
        path: String,
    },
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonPublicView(msg) => write!(f, "non-public view refused: {msg}"),
            Self::MissingRequired(path) => write!(f, "missing required corpus file `{path}`"),
            Self::NonPublicFile { path, reason } => {
                write!(f, "non-public file `{path}` refused: {reason}")
            }
            Self::QuarantinedRowInvalid { version, reason } => {
                write!(f, "quarantined version `{version}` invalid: {reason}")
            }
            Self::UnexpectedPath { path } => {
                write!(f, "unexpected corpus path `{path}`")
            }
        }
    }
}

impl std::error::Error for ManifestError {}

/// Source input accepted by [`shipped_corpus`] and [`SupportCatalogue::from_public_source`].
pub trait CorpusSource {
    /// Validate that the source represents a public-only Corpus view and return its [`Tree`].
    fn public_tree(&self) -> Result<&Tree, ManifestError>;
}

impl CorpusSource for Tree {
    fn public_tree(&self) -> Result<&Tree, ManifestError> {
        validate_tree_is_public_corpus_only(self)?;
        Ok(self)
    }
}

impl CorpusSource for LabelledView {
    fn public_tree(&self) -> Result<&Tree, ManifestError> {
        let public_only = LabelAllow::from_ids([PUBLIC]);
        if self.permitted() != &public_only {
            return Err(ManifestError::NonPublicView(
                "permitted labels are not strictly [\"public\"]".to_owned(),
            ));
        }
        if self.corpus().is_none() {
            return Err(ManifestError::NonPublicView(
                "corpus store is not readable or corpus.toml is missing".to_owned(),
            ));
        }
        for page in self.pages() {
            if !page.path.starts_with("corpus/pages/") {
                return Err(ManifestError::NonPublicFile {
                    path: page.path.clone(),
                    reason: "page does not belong to corpus/pages/".to_owned(),
                });
            }
            if page.meta.handling != vec!["public".to_owned()] {
                return Err(ManifestError::NonPublicFile {
                    path: page.path.clone(),
                    reason: "page handling is not [\"public\"]".to_owned(),
                });
            }
        }
        for source in self.sources() {
            if source.value.handling != vec!["public".to_owned()] {
                return Err(ManifestError::NonPublicFile {
                    path: "corpus/registers/sources.csv".to_owned(),
                    reason: format!(
                        "source {}-v{} handling is {:?}",
                        source.value.source_id, source.value.version, source.value.handling
                    ),
                });
            }
        }
        validate_tree_is_public_corpus_only(self.tree())?;
        Ok(self.tree())
    }
}

fn assistant_md_name() -> &'static str {
    strings::paths()[6]
}

fn validate_tree_is_public_corpus_only(tree: &Tree) -> Result<(), ManifestError> {
    for dir in ["overlay", "working", "state", ".fieldbook", "inbox"] {
        if tree.contains_dir(dir) {
            return Err(ManifestError::NonPublicView(format!(
                "tree contains non-corpus directory `{dir}`"
            )));
        }
    }
    for (path, _) in tree.iter() {
        if !path.starts_with("corpus/") {
            return Err(ManifestError::NonPublicFile {
                path: path.to_owned(),
                reason: "path lies outside `corpus/`".to_owned(),
            });
        }
        if path.ends_with("/labels.csv") {
            return Err(ManifestError::NonPublicFile {
                path: path.to_owned(),
                reason: "label ledger is not permitted in public corpus selection".to_owned(),
            });
        }
    }
    Ok(())
}

fn has_non_public_inline_handling(text: &str) -> bool {
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("handling") {
            let rest = rest.trim_start();
            if let Some(val) = rest.strip_prefix('=') {
                let val = val.trim();
                if val != "[\"public\"]" && val != "\"public\"" {
                    return true;
                }
            }
        }
    }
    false
}

fn is_known_non_shipped_corpus_path(rel_under_corpus: &str) -> bool {
    if rel_under_corpus == "AGENTS.md"
        || rel_under_corpus == assistant_md_name()
        || rel_under_corpus == "README.md"
        || rel_under_corpus == "changes.md"
        || rel_under_corpus == "scan-allow.toml"
        || rel_under_corpus == "eval/regression.toml"
        || rel_under_corpus == "pipeline/rules.toml"
    {
        return true;
    }
    rel_under_corpus.starts_with("sources/public/")
        || rel_under_corpus.starts_with("templates/")
        || rel_under_corpus.starts_with("cases/")
}

fn is_shipped_register_path(rel_under_corpus: &str) -> bool {
    let Some(name) = rel_under_corpus.strip_prefix("registers/") else {
        return false;
    };
    !name.contains('/') && Register::ALL.iter().any(|r| r.file_name() == name)
}

/// Return sorted `(path, sha256)` pairs for the shipped public Corpus files:
/// `corpus/corpus.toml`, `corpus/pages/**`, `corpus/registers/*.csv`, and
/// `corpus/sources/text/**` (excluding quarantined version folders), while
/// excluding originals (`corpus/sources/public/**`) and agent/support files.
///
/// Refuses any non-public view or non-public included file, and requires every
/// quarantined version in `sources.csv` (or `quarantined`) to keep its register
/// row, hash, and reason (`note`).
pub fn shipped_corpus<S: CorpusSource + ?Sized>(
    source: &S,
    quarantined: &BTreeSet<VersionId>,
) -> Result<Vec<(String, String)>, ManifestError> {
    let tree = source.public_tree()?;

    let Some(corpus_toml_bytes) = tree.get_bytes("corpus/corpus.toml") else {
        return Err(ManifestError::MissingRequired(
            "corpus/corpus.toml".to_owned(),
        ));
    };
    let corpus_toml_text =
        std::str::from_utf8(corpus_toml_bytes).map_err(|_| ManifestError::NonPublicFile {
            path: "corpus/corpus.toml".to_owned(),
            reason: "invalid UTF-8".to_owned(),
        })?;
    if CorpusToml::parse(corpus_toml_text).is_err() {
        return Err(ManifestError::NonPublicFile {
            path: "corpus/corpus.toml".to_owned(),
            reason: "invalid corpus.toml".to_owned(),
        });
    }

    // Inspect `corpus/registers/sources.csv` if present to validate public handling
    // and quarantined version rows (retaining row, hash, and reason in `note`).
    let mut effective_quarantined: BTreeSet<VersionId> = quarantined.clone();
    let mut seen_quarantined_rows: BTreeSet<VersionId> = BTreeSet::new();
    let mut known_public_versions: BTreeSet<VersionId> = BTreeSet::new();
    let mut has_sources_csv = false;

    if let Some(sources_bytes) = tree.get_bytes("corpus/registers/sources.csv") {
        has_sources_csv = true;
        let tbl = table::read(sources_bytes).map_err(|e| ManifestError::NonPublicFile {
            path: "corpus/registers/sources.csv".to_owned(),
            reason: format!("failed to parse sources.csv: {e}"),
        })?;
        for row in &tbl.rows {
            let sr = SourceRow::from_row(row).map_err(|_| ManifestError::NonPublicFile {
                path: "corpus/registers/sources.csv".to_owned(),
                reason: format!("invalid source row at line {}", row.line),
            })?;
            if sr.handling != vec!["public".to_owned()] {
                return Err(ManifestError::NonPublicFile {
                    path: "corpus/registers/sources.csv".to_owned(),
                    reason: format!(
                        "source {}-v{} has non-public handling {:?}",
                        sr.source_id, sr.version, sr.handling
                    ),
                });
            }
            let vid =
                VersionId::parse(&format!("{}-v{}", sr.source_id, sr.version)).map_err(|_| {
                    ManifestError::NonPublicFile {
                        path: "corpus/registers/sources.csv".to_owned(),
                        reason: format!("invalid version id for {}-v{}", sr.source_id, sr.version),
                    }
                })?;
            known_public_versions.insert(vid.clone());
            if sr.lifecycle == Lifecycle::Quarantined || effective_quarantined.contains(&vid) {
                if sr.sha256.trim().is_empty() {
                    return Err(ManifestError::QuarantinedRowInvalid {
                        version: vid,
                        reason: "missing sha256 in sources.csv".to_owned(),
                    });
                }
                if sr.note.as_deref().unwrap_or("").trim().is_empty() {
                    return Err(ManifestError::QuarantinedRowInvalid {
                        version: vid,
                        reason: "missing quarantine reason in `note`".to_owned(),
                    });
                }
                effective_quarantined.insert(vid.clone());
                seen_quarantined_rows.insert(vid);
            }
        }
    }

    if has_sources_csv {
        for q_vid in quarantined {
            if !seen_quarantined_rows.contains(q_vid) {
                return Err(ManifestError::QuarantinedRowInvalid {
                    version: q_vid.clone(),
                    reason: "quarantined version row missing from corpus/registers/sources.csv"
                        .to_owned(),
                });
            }
        }
    }

    let mut out: Vec<(String, String)> = Vec::new();

    for (path, entry) in tree.iter() {
        let Some(rel) = path.strip_prefix("corpus/") else {
            return Err(ManifestError::NonPublicFile {
                path: path.to_owned(),
                reason: "path lies outside `corpus/`".to_owned(),
            });
        };

        if is_known_non_shipped_corpus_path(rel) {
            // Even for non-shipped originals, ensure no non-Bytes entry sneaks into shipped paths.
            continue;
        }

        let is_corpus_toml = rel == "corpus.toml";
        let is_page = rel.starts_with("pages/");
        let is_register = is_shipped_register_path(rel);
        let is_text = rel.starts_with("sources/text/");

        if !(is_corpus_toml || is_page || is_register || is_text) {
            return Err(ManifestError::UnexpectedPath {
                path: path.to_owned(),
            });
        }

        if let Some(after_text) = rel.strip_prefix("sources/text/") {
            let Some((vid_seg, rest_file)) = after_text.split_once('/') else {
                return Err(ManifestError::UnexpectedPath {
                    path: path.to_owned(),
                });
            };
            if rest_file.is_empty() {
                return Err(ManifestError::UnexpectedPath {
                    path: path.to_owned(),
                });
            }
            let vid = VersionId::parse(vid_seg).map_err(|_| ManifestError::UnexpectedPath {
                path: path.to_owned(),
            })?;
            if effective_quarantined.contains(&vid) {
                continue;
            }
            if has_sources_csv
                && !known_public_versions.is_empty()
                && !known_public_versions.contains(&vid)
            {
                return Err(ManifestError::NonPublicFile {
                    path: path.to_owned(),
                    reason: format!("source version `{vid}` is not in public sources.csv"),
                });
            }
        }

        let Entry::Bytes(bytes) = entry else {
            return Err(ManifestError::NonPublicFile {
                path: path.to_owned(),
                reason: "shipped corpus entry must be byte-backed".to_owned(),
            });
        };

        let text = std::str::from_utf8(bytes).map_err(|_| ManifestError::NonPublicFile {
            path: path.to_owned(),
            reason: "shipped corpus file is not valid UTF-8".to_owned(),
        })?;

        if is_page {
            if !path.ends_with(".md") {
                return Err(ManifestError::UnexpectedPath {
                    path: path.to_owned(),
                });
            }
            let parts = split(text).map_err(|e| ManifestError::NonPublicFile {
                path: path.to_owned(),
                reason: format!("invalid page front matter split: {e}"),
            })?;
            let meta =
                parse_meta(&parts.front_matter).map_err(|e| ManifestError::NonPublicFile {
                    path: path.to_owned(),
                    reason: format!("invalid page front matter: {e}"),
                })?;
            if meta.handling != vec!["public".to_owned()] {
                return Err(ManifestError::NonPublicFile {
                    path: path.to_owned(),
                    reason: format!(
                        "page handling is {:?}, expected [\"public\"]",
                        meta.handling
                    ),
                });
            }
        } else if !is_register && has_non_public_inline_handling(text) {
            return Err(ManifestError::NonPublicFile {
                path: path.to_owned(),
                reason: "file contains non-public inline handling".to_owned(),
            });
        }

        if !marking::scan(text).is_empty() {
            return Err(ManifestError::NonPublicFile {
                path: path.to_owned(),
                reason: "file contains restricted marking".to_owned(),
            });
        }

        out.push((path.to_owned(), sha256_hex(bytes)));
    }

    out.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    Ok(out)
}

/// Format `(path, sha256)` entries as canonical `SHA256SUMS` text:
/// one `<sha256>  <path>\n` line per entry, sorted by `path` as UTF-8 bytes,
/// with LF line endings and a final newline.
#[must_use]
pub fn write_sums(entries: &[(String, String)]) -> String {
    let mut sorted: Vec<(&str, &str)> = entries
        .iter()
        .map(|(path, sha)| (path.as_str(), sha.as_str()))
        .collect();
    sorted.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));

    let mut out = String::new();
    for (path, sha) in sorted {
        out.push_str(sha);
        out.push_str("  ");
        out.push_str(path);
        out.push('\n');
    }
    out
}

/// Compute the 64-character lowercase hex SHA-256 fingerprint over `entries`
/// formatted as canonical `<sha256>  <path>\n` lines sorted by path as bytes.
#[must_use]
pub fn fingerprint(entries: &[(String, String)]) -> String {
    let sums = write_sums(entries);
    sha256_hex(sums.as_bytes())
}

// ============================================================================
// Public support catalogue (`docs/reference/audit-contract.md#public-support-verification`)
// ============================================================================

/// Recognized public Corpus support file roles (`audit-contract.md#public-support-verification`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SupportRole {
    /// Root file (`AGENTS.md`, `ASSISTANT.md`, `README.md`, `corpus.toml`, `changes.md`, `scan-allow.toml`).
    RootFile,
    /// Registered page template (`templates/<type>.md` for a [`PageType`] in `PageType::ALL`).
    PageTemplate(PageType),
    /// Registered workbook CSV template (`templates/workbooks/<name>.csv`).
    WorkbookCsvTemplate,
    /// Evaluation regression set (`eval/regression.toml`).
    Regression,
    /// Optional pipeline rules (`pipeline/rules.toml`).
    PipelineRules,
}

/// One validated `[[file]]` entry in a [`SupportCatalogue`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SupportEntry {
    /// Canonical Corpus-relative path (no `corpus/` prefix).
    pub path: String,
    /// Full 64-character lowercase hexadecimal SHA-256 digest of the file's bytes.
    pub sha256: String,
    /// Recognized role of this support file.
    pub role: SupportRole,
}

/// Validated `corpus-support.toml` schema 1 document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportCatalogue {
    /// Support catalogue schema version (always `1`).
    pub support_schema: u32,
    /// Supported positive Corpus schema version (`1..=SCHEMA_VERSION_MAX`).
    pub corpus_schema: u32,
    /// Source commit SHA (40 lowercase hexadecimal characters).
    pub commit: String,
    /// Entries sorted strictly by `path` as UTF-8 bytes.
    pub files: Vec<SupportEntry>,
}

/// Error returned when building, parsing, or verifying a [`SupportCatalogue`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupportCatalogueError {
    /// Input exceeds [`MAX_SUPPORT_CATALOGUE_BYTES`] (`1 MiB`).
    InputTooLarge {
        /// Actual byte length.
        len: usize,
    },
    /// Input exceeds [`MAX_SUPPORT_CATALOGUE_ENTRIES`] (`4096`).
    TooManyEntries {
        /// Actual entry count.
        count: usize,
    },
    /// Input starts with a UTF-8 BOM or is not valid UTF-8.
    InvalidUtf8OrBom,
    /// TOML syntax error, unknown key, duplicate key, or wrong field type.
    TomlParse(String),
    /// `support_schema` is not `1`.
    UnsupportedSupportSchema(u32),
    /// `corpus_schema` is `0` or not supported by this tool.
    UnsupportedCorpusSchema(u32),
    /// `commit` is not 40 lowercase hex characters.
    InvalidCommit(String),
    /// `sha256` or expected digest is not 64 lowercase hex characters.
    InvalidSha256(String),
    /// `path` is not a canonical relative path (traversal, backslash, drive/UNC, control char, etc.).
    NonCanonicalPath {
        /// Offending path string.
        path: String,
        /// Reason for rejection.
        reason: String,
    },
    /// `path` does not match any recognized [`SupportRole`].
    UnrecognizedRole {
        /// Offending path string.
        path: String,
    },
    /// Entries are empty, unsorted, duplicate, or collide under case-folding/NFC.
    InvalidEntryOrderOrCollision {
        /// Offending path string.
        path: String,
        /// Explanation.
        reason: String,
    },
    /// Exact catalogue bytes do not match the expected SHA-256 digest.
    DigestMismatch {
        /// Expected 64-hex digest.
        expected: String,
        /// Actual 64-hex digest of the supplied bytes.
        actual: String,
    },
    /// Source generation is not a valid public Corpus view or contains non-public/ambiguous support files.
    InvalidSourceGeneration(String),
    /// A present recognized support file in the tree is missing from the catalogue or has a different hash.
    FileBindingMismatch {
        /// Corpus-relative support path.
        path: String,
        /// Explanation of mismatch.
        reason: String,
    },
}

impl fmt::Display for SupportCatalogueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputTooLarge { len } => write!(
                f,
                "support catalogue length {len} exceeds maximum {MAX_SUPPORT_CATALOGUE_BYTES}"
            ),
            Self::TooManyEntries { count } => write!(
                f,
                "support catalogue entry count {count} exceeds maximum {MAX_SUPPORT_CATALOGUE_ENTRIES}"
            ),
            Self::InvalidUtf8OrBom => {
                write!(
                    f,
                    "support catalogue must be UTF-8 without a byte order mark"
                )
            }
            Self::TomlParse(msg) => write!(f, "support catalogue parse error: {msg}"),
            Self::UnsupportedSupportSchema(v) => {
                write!(f, "unsupported support_schema {v} (expected 1)")
            }
            Self::UnsupportedCorpusSchema(v) => {
                write!(f, "unsupported corpus_schema {v}")
            }
            Self::InvalidCommit(c) => {
                write!(f, "invalid commit `{c}` (expected 40 lowercase hex chars)")
            }
            Self::InvalidSha256(h) => {
                write!(f, "invalid sha256 `{h}` (expected 64 lowercase hex chars)")
            }
            Self::NonCanonicalPath { path, reason } => {
                write!(f, "non-canonical support path `{path}`: {reason}")
            }
            Self::UnrecognizedRole { path } => {
                write!(f, "unrecognized support role or path `{path}`")
            }
            Self::InvalidEntryOrderOrCollision { path, reason } => {
                write!(f, "invalid support entry `{path}`: {reason}")
            }
            Self::DigestMismatch { expected, actual } => write!(
                f,
                "support catalogue digest mismatch: expected {expected}, got {actual}"
            ),
            Self::InvalidSourceGeneration(msg) => {
                write!(f, "invalid public corpus generation: {msg}")
            }
            Self::FileBindingMismatch { path, reason } => {
                write!(f, "support file `{path}` binding mismatch: {reason}")
            }
        }
    }
}

impl std::error::Error for SupportCatalogueError {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSupportCatalogue {
    support_schema: i64,
    corpus_schema: i64,
    commit: String,
    #[serde(default)]
    file: Vec<RawSupportFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSupportFile {
    path: String,
    sha256: String,
}

/// Check if a string slice consists of exactly `expected_len` lowercase hexadecimal characters.
#[must_use]
pub fn is_lower_hex(s: &str, expected_len: usize) -> bool {
    s.len() == expected_len
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn is_windows_reserved_stem(stem: &str) -> bool {
    let upper = stem.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

/// Validate a Corpus-relative path under the canonical relative-path rules and
/// return its recognized [`SupportRole`].
pub fn classify_support_path(path: &str) -> Result<SupportRole, SupportCatalogueError> {
    if path.is_empty() {
        return Err(SupportCatalogueError::NonCanonicalPath {
            path: path.to_owned(),
            reason: "path must not be empty".to_owned(),
        });
    }
    if path.chars().any(char::is_control) {
        return Err(SupportCatalogueError::NonCanonicalPath {
            path: path.to_owned(),
            reason: "path contains control character".to_owned(),
        });
    }
    if path.starts_with('/') || path.ends_with('/') || path.contains('\\') || path.contains(':') {
        return Err(SupportCatalogueError::NonCanonicalPath {
            path: path.to_owned(),
            reason: "path must be a clean slash-separated relative path".to_owned(),
        });
    }
    let nfc: String = path.nfc().collect();
    if nfc != path {
        return Err(SupportCatalogueError::NonCanonicalPath {
            path: path.to_owned(),
            reason: "path is not in Unicode NFC".to_owned(),
        });
    }
    for seg in path.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." {
            return Err(SupportCatalogueError::NonCanonicalPath {
                path: path.to_owned(),
                reason: format!("invalid path segment `{seg}`"),
            });
        }
        if seg.ends_with('.') || seg.ends_with(' ') {
            return Err(SupportCatalogueError::NonCanonicalPath {
                path: path.to_owned(),
                reason: format!("segment `{seg}` ends with a dot or space"),
            });
        }
        let stem = seg.split('.').next().unwrap_or(seg);
        if is_windows_reserved_stem(stem) {
            return Err(SupportCatalogueError::NonCanonicalPath {
                path: path.to_owned(),
                reason: format!("segment `{seg}` uses a Windows reserved name"),
            });
        }
    }

    if path == "AGENTS.md"
        || path == assistant_md_name()
        || path == "README.md"
        || path == "corpus.toml"
        || path == "changes.md"
        || path == "scan-allow.toml"
    {
        return Ok(SupportRole::RootFile);
    }

    if path == "eval/regression.toml" {
        return Ok(SupportRole::Regression);
    }

    if path == "pipeline/rules.toml" {
        return Ok(SupportRole::PipelineRules);
    }

    if let Some(rest) = path.strip_prefix("templates/workbooks/") {
        if !rest.contains('/')
            && let Some(stem) = rest.strip_suffix(".csv")
            && !stem.is_empty()
            && stem
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        {
            return Ok(SupportRole::WorkbookCsvTemplate);
        }
        return Err(SupportCatalogueError::UnrecognizedRole {
            path: path.to_owned(),
        });
    }

    if let Some(rest) = path.strip_prefix("templates/") {
        if !rest.contains('/')
            && let Some(type_name) = rest.strip_suffix(".md")
            && let Some(&page_type) = PageType::ALL.iter().find(|ty| ty.as_str() == type_name)
        {
            return Ok(SupportRole::PageTemplate(page_type));
        }
        return Err(SupportCatalogueError::UnrecognizedRole {
            path: path.to_owned(),
        });
    }

    Err(SupportCatalogueError::UnrecognizedRole {
        path: path.to_owned(),
    })
}

fn escape_toml_basic_string(s: &str) -> String {
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

impl SupportCatalogue {
    /// Construct and validate a [`SupportCatalogue`] from `(path, sha256)` pairs,
    /// sorting entries by `path` as UTF-8 bytes and rejecting any unrecognized role,
    /// duplicate/colliding path, malformed hash, or out-of-bounds count.
    pub fn new(
        corpus_schema: u32,
        commit: &str,
        raw_entries: Vec<(String, String)>,
    ) -> Result<Self, SupportCatalogueError> {
        if min_tool_version(corpus_schema).is_none() {
            return Err(SupportCatalogueError::UnsupportedCorpusSchema(
                corpus_schema,
            ));
        }
        if !is_lower_hex(commit, 40) {
            return Err(SupportCatalogueError::InvalidCommit(commit.to_owned()));
        }
        if raw_entries.is_empty() {
            return Err(SupportCatalogueError::InvalidEntryOrderOrCollision {
                path: String::new(),
                reason: "support catalogue must contain at least one [[file]] entry".to_owned(),
            });
        }
        if raw_entries.len() > MAX_SUPPORT_CATALOGUE_ENTRIES {
            return Err(SupportCatalogueError::TooManyEntries {
                count: raw_entries.len(),
            });
        }

        let mut entries = Vec::with_capacity(raw_entries.len());
        for (path, sha256) in raw_entries {
            let role = classify_support_path(&path)?;
            if !is_lower_hex(&sha256, 64) {
                return Err(SupportCatalogueError::InvalidSha256(sha256));
            }
            entries.push(SupportEntry { path, sha256, role });
        }

        entries.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
        validate_sorted_no_collisions(&entries)?;

        let cat = Self {
            support_schema: 1,
            corpus_schema,
            commit: commit.to_owned(),
            files: entries,
        };
        if cat.to_toml().len() > MAX_SUPPORT_CATALOGUE_BYTES {
            return Err(SupportCatalogueError::InputTooLarge {
                len: cat.to_toml().len(),
            });
        }
        Ok(cat)
    }

    /// Build a [`SupportCatalogue`] from one validated public Corpus generation (`Tree` or `LabelledView`).
    ///
    /// Selects every present recognized support file under `corpus/` and refuses any
    /// unrecognized or ambiguous file in a support directory (`templates/`, `eval/`, `pipeline/`).
    pub fn from_public_source<S: CorpusSource + ?Sized>(
        source: &S,
        commit: &str,
    ) -> Result<Self, SupportCatalogueError> {
        let tree = source
            .public_tree()
            .map_err(|e| SupportCatalogueError::InvalidSourceGeneration(e.to_string()))?;

        let Some(corpus_toml_bytes) = tree.get_bytes("corpus/corpus.toml") else {
            return Err(SupportCatalogueError::InvalidSourceGeneration(
                "missing corpus/corpus.toml".to_owned(),
            ));
        };
        let corpus_toml_text = std::str::from_utf8(corpus_toml_bytes).map_err(|_| {
            SupportCatalogueError::InvalidSourceGeneration(
                "corpus/corpus.toml is not valid UTF-8".to_owned(),
            )
        })?;
        let parsed_corpus = CorpusToml::parse(corpus_toml_text).map_err(|e| {
            SupportCatalogueError::InvalidSourceGeneration(format!("invalid corpus.toml: {e}"))
        })?;
        let corpus_schema = parsed_corpus.schema_version;

        let mut pairs: Vec<(String, String)> = Vec::new();

        for (full_path, entry) in tree.iter() {
            let Some(rel) = full_path.strip_prefix("corpus/") else {
                return Err(SupportCatalogueError::InvalidSourceGeneration(format!(
                    "path `{full_path}` lies outside `corpus/`"
                )));
            };

            // Skip pages, registers, originals, extracted text, and cases; those have their own kinds.
            if rel.starts_with("pages/")
                || rel.starts_with("registers/")
                || rel.starts_with("sources/")
                || rel.starts_with("cases/")
            {
                continue;
            }

            // Everything else under `corpus/` must be a recognized support role with byte content.
            let _role = classify_support_path(rel)?;
            let Entry::Bytes(bytes) = entry else {
                return Err(SupportCatalogueError::InvalidSourceGeneration(format!(
                    "support file `{full_path}` must be byte-backed"
                )));
            };
            let text = std::str::from_utf8(bytes).map_err(|_| {
                SupportCatalogueError::InvalidSourceGeneration(format!(
                    "support file `{full_path}` is not valid UTF-8"
                ))
            })?;
            if !marking::scan(text).is_empty() || has_non_public_inline_handling(text) {
                return Err(SupportCatalogueError::InvalidSourceGeneration(format!(
                    "support file `{full_path}` contains non-public marking or handling"
                )));
            }
            pairs.push((rel.to_owned(), sha256_hex(bytes)));
        }

        Self::new(corpus_schema, commit, pairs)
    }

    /// Strictly parse `corpus-support.toml` from exact UTF-8 bytes.
    pub fn parse_bytes(bytes: &[u8]) -> Result<Self, SupportCatalogueError> {
        if bytes.len() > MAX_SUPPORT_CATALOGUE_BYTES {
            return Err(SupportCatalogueError::InputTooLarge { len: bytes.len() });
        }
        if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            return Err(SupportCatalogueError::InvalidUtf8OrBom);
        }
        let text =
            std::str::from_utf8(bytes).map_err(|_| SupportCatalogueError::InvalidUtf8OrBom)?;
        Self::parse_str(text)
    }

    /// Strictly parse `corpus-support.toml` from a UTF-8 string slice.
    pub fn parse_str(text: &str) -> Result<Self, SupportCatalogueError> {
        if text.len() > MAX_SUPPORT_CATALOGUE_BYTES {
            return Err(SupportCatalogueError::InputTooLarge { len: text.len() });
        }
        if text.as_bytes().starts_with(&[0xEF, 0xBB, 0xBF]) {
            return Err(SupportCatalogueError::InvalidUtf8OrBom);
        }

        let raw: RawSupportCatalogue =
            toml::from_str(text).map_err(|e| SupportCatalogueError::TomlParse(e.to_string()))?;

        let support_schema = u32::try_from(raw.support_schema)
            .map_err(|_| SupportCatalogueError::UnsupportedSupportSchema(0))?;
        if support_schema != 1 {
            return Err(SupportCatalogueError::UnsupportedSupportSchema(
                support_schema,
            ));
        }

        let corpus_schema = u32::try_from(raw.corpus_schema)
            .map_err(|_| SupportCatalogueError::UnsupportedCorpusSchema(0))?;
        if min_tool_version(corpus_schema).is_none() {
            return Err(SupportCatalogueError::UnsupportedCorpusSchema(
                corpus_schema,
            ));
        }

        if !is_lower_hex(&raw.commit, 40) {
            return Err(SupportCatalogueError::InvalidCommit(raw.commit));
        }

        if raw.file.is_empty() {
            return Err(SupportCatalogueError::InvalidEntryOrderOrCollision {
                path: String::new(),
                reason: "support catalogue must contain at least one [[file]] entry".to_owned(),
            });
        }
        if raw.file.len() > MAX_SUPPORT_CATALOGUE_ENTRIES {
            return Err(SupportCatalogueError::TooManyEntries {
                count: raw.file.len(),
            });
        }

        let mut files = Vec::with_capacity(raw.file.len());
        for rf in raw.file {
            let role = classify_support_path(&rf.path)?;
            if !is_lower_hex(&rf.sha256, 64) {
                return Err(SupportCatalogueError::InvalidSha256(rf.sha256));
            }
            files.push(SupportEntry {
                path: rf.path,
                sha256: rf.sha256,
                role,
            });
        }

        validate_sorted_no_collisions(&files)?;

        Ok(Self {
            support_schema,
            corpus_schema,
            commit: raw.commit,
            files,
        })
    }

    /// Deterministic schema 1 TOML serialization (`audit-contract.md#public-support-verification`):
    /// UTF-8 without BOM, LF endings, fixed header key order, a blank line before each `[[file]]`,
    /// entries sorted by `path` as UTF-8 bytes, and one final LF.
    #[must_use]
    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("support_schema = {}\n", self.support_schema));
        out.push_str(&format!("corpus_schema = {}\n", self.corpus_schema));
        out.push_str(&format!(
            "commit = {}\n",
            escape_toml_basic_string(&self.commit)
        ));
        for entry in &self.files {
            out.push_str("\n[[file]]\n");
            out.push_str(&format!(
                "path = {}\n",
                escape_toml_basic_string(&entry.path)
            ));
            out.push_str(&format!(
                "sha256 = {}\n",
                escape_toml_basic_string(&entry.sha256)
            ));
        }
        out
    }

    /// Return the deterministic schema 1 TOML bytes and their 64-hex SHA-256 digest.
    #[must_use]
    pub fn to_toml_bytes_and_sha256(&self) -> (Vec<u8>, String) {
        let bytes = self.to_toml().into_bytes();
        let sha = sha256_hex(&bytes);
        (bytes, sha)
    }

    /// Format the sidecar digest file (`corpus-support.sha256`) using [`write_sums`]:
    /// `<full lowercase SHA-256>  corpus-support.toml\n`.
    #[must_use]
    pub fn sidecar_sha256_line(catalogue_bytes: &[u8]) -> String {
        let digest = sha256_hex(catalogue_bytes);
        write_sums(&[("corpus-support.toml".to_owned(), digest)])
    }

    /// Verify every present recognized support file under `corpus/` in `tree` against
    /// this catalogue's entries.
    ///
    /// - Any present recognized support file in `tree` that is missing from `self.files`
    ///   or whose SHA-256 differs refuses with [`SupportCatalogueError::FileBindingMismatch`].
    /// - An entry in `self.files` for a non-shipped support file that is absent in `tree`
    ///   (such as `AGENTS.md` or a template in an extracted edition) is permitted.
    pub fn verify_present_support_files(&self, tree: &Tree) -> Result<(), SupportCatalogueError> {
        let by_path: BTreeMap<&str, &str> = self
            .files
            .iter()
            .map(|e| (e.path.as_str(), e.sha256.as_str()))
            .collect();

        let mut checked = 0usize;
        for (full_path, entry) in tree.iter() {
            let Some(rel) = full_path.strip_prefix("corpus/") else {
                continue;
            };
            if rel.starts_with("pages/")
                || rel.starts_with("registers/")
                || rel.starts_with("sources/")
                || rel.starts_with("cases/")
            {
                continue;
            }
            let _role = classify_support_path(rel)?;
            let Entry::Bytes(bytes) = entry else {
                return Err(SupportCatalogueError::FileBindingMismatch {
                    path: rel.to_owned(),
                    reason: "present support entry is not byte-backed".to_owned(),
                });
            };
            let Some(&expected_sha) = by_path.get(rel) else {
                return Err(SupportCatalogueError::FileBindingMismatch {
                    path: rel.to_owned(),
                    reason: "present support file is missing from support catalogue".to_owned(),
                });
            };
            let actual_sha = sha256_hex(bytes);
            if actual_sha != expected_sha {
                return Err(SupportCatalogueError::FileBindingMismatch {
                    path: rel.to_owned(),
                    reason: format!("sha256 mismatch: expected {expected_sha}, got {actual_sha}"),
                });
            }
            checked += 1;
        }

        if checked == 0 {
            return Err(SupportCatalogueError::InvalidSourceGeneration(
                "no present support files found under `corpus/`".to_owned(),
            ));
        }

        Ok(())
    }
}

/// Verified support facts used for resolving public provenance of Corpus support files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SupportFacts {
    /// Retained catalogues that have a verified binding: `(SupportCatalogue, full_catalogue_sha256)`.
    pub catalogues: Vec<(SupportCatalogue, String)>,
    /// Completed public writes per path: `(path, full_sha256_of_last_completed_write)`.
    pub public_writes: BTreeMap<String, String>,
}

impl SupportFacts {
    /// Construct empty support facts.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            catalogues: Vec::new(),
            public_writes: BTreeMap::new(),
        }
    }

    /// Construct `SupportFacts` with parsed retained catalogues and completed public writes.
    #[must_use]
    pub fn new(
        catalogues: Vec<(SupportCatalogue, String)>,
        public_writes: BTreeMap<String, String>,
    ) -> Self {
        Self {
            catalogues,
            public_writes,
        }
    }

    /// Check if `rel_path` with `current_sha256` has public provenance.
    #[must_use]
    pub fn has_provenance(&self, rel_path: &str, current_sha256: &str) -> bool {
        let clean = rel_path.replace('\\', "/");
        let stripped = clean.trim_start_matches('/');
        let corpus_rel = stripped.strip_prefix("corpus/").unwrap_or(stripped);

        // (a) last completed public write for that path in admitted verified journal history
        if self
            .public_writes
            .get(corpus_rel)
            .is_some_and(|sha| sha.eq_ignore_ascii_case(current_sha256))
            || self
                .public_writes
                .get(stripped)
                .is_some_and(|sha| sha.eq_ignore_ascii_case(current_sha256))
        {
            return true;
        }

        // Also check by canonical path digest
        if let Ok(canon) = crate::audit::CorpusPath::parse(corpus_rel) {
            let h = crate::audit::Sha256::digest(canon.as_str().as_bytes()).to_string();
            if self
                .public_writes
                .get(&h)
                .is_some_and(|sha| sha.eq_ignore_ascii_case(current_sha256))
            {
                return true;
            }
        }
        let prefixed = format!("corpus/{corpus_rel}");
        if let Ok(canon) = crate::audit::CorpusPath::parse(&prefixed) {
            let h = crate::audit::Sha256::digest(canon.as_str().as_bytes()).to_string();
            if self
                .public_writes
                .get(&h)
                .is_some_and(|sha| sha.eq_ignore_ascii_case(current_sha256))
            {
                return true;
            }
        }

        // (b) the same path's entry in a retained, verified catalogue
        for (cat, _cat_sha) in &self.catalogues {
            if cat.files.iter().any(|entry| {
                entry.path == corpus_rel && entry.sha256.eq_ignore_ascii_case(current_sha256)
            }) {
                return true;
            }
        }

        false
    }
}

fn validate_sorted_no_collisions(entries: &[SupportEntry]) -> Result<(), SupportCatalogueError> {
    let mut prev_path: Option<&str> = None;
    let mut seen_folded: BTreeSet<String> = BTreeSet::new();

    for entry in entries {
        if let Some(prev) = prev_path {
            match prev.as_bytes().cmp(entry.path.as_bytes()) {
                std::cmp::Ordering::Equal => {
                    return Err(SupportCatalogueError::InvalidEntryOrderOrCollision {
                        path: entry.path.clone(),
                        reason: "duplicate support path".to_owned(),
                    });
                }
                std::cmp::Ordering::Greater => {
                    return Err(SupportCatalogueError::InvalidEntryOrderOrCollision {
                        path: entry.path.clone(),
                        reason: format!(
                            "entries are not sorted by UTF-8 path bytes (`{}` after `{prev}`)",
                            entry.path
                        ),
                    });
                }
                std::cmp::Ordering::Less => {}
            }
        }
        prev_path = Some(&entry.path);

        let folded = entry.path.nfc().collect::<String>().to_lowercase();
        if !seen_folded.insert(folded) {
            return Err(SupportCatalogueError::InvalidEntryOrderOrCollision {
                path: entry.path.clone(),
                reason: "case-insensitive or Unicode normalized path collision".to_owned(),
            });
        }
    }
    Ok(())
}

/// Hash the exact `catalogue_bytes` BEFORE parsing and compare against `expected_sha256`,
/// then strictly parse and return the [`SupportCatalogue`].
///
/// Never reserializes or hashes a parsed map; a comment or whitespace edit changes the digest
/// and is rejected.
pub fn verify_support_catalogue_bytes(
    catalogue_bytes: &[u8],
    expected_sha256: &str,
) -> Result<SupportCatalogue, SupportCatalogueError> {
    if catalogue_bytes.len() > MAX_SUPPORT_CATALOGUE_BYTES {
        return Err(SupportCatalogueError::InputTooLarge {
            len: catalogue_bytes.len(),
        });
    }
    if !is_lower_hex(expected_sha256, 64) {
        return Err(SupportCatalogueError::InvalidSha256(
            expected_sha256.to_owned(),
        ));
    }
    let actual_sha256 = sha256_hex(catalogue_bytes);
    if actual_sha256 != expected_sha256 {
        return Err(SupportCatalogueError::DigestMismatch {
            expected: expected_sha256.to_owned(),
            actual: actual_sha256,
        });
    }
    SupportCatalogue::parse_bytes(catalogue_bytes)
}

#[cfg(test)]
#[expect(
    clippy::duplicate_mod,
    reason = "shared seeded test RNG is included at file level per card spec"
)]
#[path = "../../../tests/support/rng.rs"]
mod rng;

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
    use crate::policy::EffectivePolicy;
    use crate::schema::Register;
    use crate::schema::vocab::PageType;

    use super::{
        MAX_SUPPORT_CATALOGUE_BYTES, MAX_SUPPORT_CATALOGUE_ENTRIES, ManifestError,
        SupportCatalogue, SupportCatalogueError, SupportRole, classify_support_path, fingerprint,
        rng::Rng, shipped_corpus, verify_support_catalogue_bytes, write_sums,
    };

    const MINI_CORPUS_TOML: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/corpus.toml");

    fn insert_empty_registers(tree: &mut Tree) {
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
    }

    fn valid_page_markdown(id: &str, handling: &str) -> String {
        format!(
            "+++\nid = \"{id}\"\ntitle = \"Synthetic Title\"\ntype = \"concept\"\narea = \"cmp\"\nsubarea = \"business-systems\"\nstage = 3\nstatus = \"draft\"\nreadiness = \"SYNTHETIC-NOW\"\nas_of = 2026-02-01\nhandling = [\"{handling}\"]\nvolatility = \"V2\"\n+++\n\n## Purpose\n\nSynthetic.\n\n## The rule\n\nSynthetic.\n\n## How it works\n\nSynthetic.\n\n## Worked example\n\nSynthetic example. The numbers are made up.\n\n## Never confuse\n\nSynthetic.\n\n## Company calibration\n\nSynthetic.\n\n## Sources\n\n## Change note\n\n- NEW_KNOWLEDGE, 2026-02-01: Synthetic.\n"
        )
    }

    fn build_synthetic_public_tree() -> Tree {
        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes());
        tree.insert_bytes("corpus/AGENTS.md", b"# Synthetic agents\n");
        tree.insert_bytes(
            format!("corpus/{}", super::assistant_md_name()),
            b"rules_version = 1\n",
        );
        tree.insert_bytes("corpus/README.md", b"# Synthetic readme\n");
        tree.insert_bytes(
            "corpus/changes.md",
            b"# Corpus changes\n\nSynthetic changes log.\n\n## edition-001\n\n- Added: synthetic.\n",
        );
        tree.insert_bytes("corpus/scan-allow.toml", b"allow = []\n");
        insert_empty_registers(&mut tree);

        let sources_csv = "source_id,version,tag,title,publisher,source_kind,source_role,authority_tier,licence,source_family,family_basis,location,sha256,published,effective_from,as_of,retrieved,lifecycle,supersedes,volatility,handling,note,purpose,origin_system,origin_stream,origin_key,doc_kind,series\nS-0001,1,syn-one,Synthetic Active Source,Synthetic Board,primary_law,primary_evidence,A,Synthetic,syn_one,reviewed,S-0001-v1.txt,e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855,2026-01-10,2026-01-15,2026-02-01,2026-02-01,active,,V2,public,Active fixture.,Baseline,,,,,\nS-0002,1,syn-two,Synthetic Held Source,Synthetic Board,primary_law,primary_evidence,A,Synthetic,syn_two,reviewed,S-0002-v1.txt,ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad,2026-01-10,2026-01-15,2026-02-01,2026-02-01,quarantined,,V2,public,Held for synthetic licence review.,Held test,,,,,\n";
        tree.insert_bytes("corpus/registers/sources.csv", sources_csv.as_bytes());
        tree.insert(
            "corpus/sources/public/S-0001-v1.txt",
            Entry::Original {
                sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                    .to_owned(),
                len: 0,
            },
        );
        tree.insert_bytes(
            "corpus/sources/text/S-0001-v1/p0001.txt",
            b"active extracted text\n",
        );
        tree.insert_bytes(
            "corpus/sources/text/S-0002-v1/p0001.txt",
            b"quarantined extracted text that must not ship\n",
        );
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-sample.md",
            valid_page_markdown("cmp-sample", "public").into_bytes(),
        );
        tree
    }

    #[test]
    fn shipped_corpus_selects_public_files_retains_quarantined_row_and_excludes_payload() {
        let tree = build_synthetic_public_tree();
        let mut quarantined = BTreeSet::new();
        quarantined.insert(VersionId::parse("S-0002-v1").expect("valid version id"));

        let entries = shipped_corpus(&tree, &quarantined).expect("shipped_corpus succeeds");
        let paths: Vec<&str> = entries.iter().map(|(p, _)| p.as_str()).collect();

        // Sorted bytewise and contains only corpus.toml, pages, registers, and non-quarantined text.
        assert_eq!(
            paths,
            vec![
                "corpus/corpus.toml",
                "corpus/pages/compliance/business-systems/cmp-sample.md",
                "corpus/registers/claims.csv",
                "corpus/registers/conflicts.csv",
                "corpus/registers/glossary.csv",
                "corpus/registers/questions.csv",
                "corpus/registers/sources.csv",
                "corpus/sources/text/S-0001-v1/p0001.txt",
            ]
        );

        // Quarantined row, hash, and reason remain in sources.csv, while its text folder is excluded.
        let sources_text = std::str::from_utf8(
            tree.get_bytes("corpus/registers/sources.csv")
                .expect("sources.csv"),
        )
        .expect("utf8");
        assert!(sources_text.contains("S-0002,1"));
        assert!(
            sources_text
                .contains("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
        assert!(sources_text.contains("Held for synthetic licence review."));
        assert!(
            !paths.iter().any(|p| p.contains("S-0002-v1")
                || p.contains("AGENTS.md")
                || p.contains("public/"))
        );

        // Hand-computed write_sums and fingerprint over two synthetic entries.
        let sample_entries = vec![
            ("corpus/b.txt".to_owned(), "22".repeat(32)),
            ("corpus/a.txt".to_owned(), "11".repeat(32)),
        ];
        let expected_sums = format!(
            "{}  corpus/a.txt\n{}  corpus/b.txt\n",
            "11".repeat(32),
            "22".repeat(32)
        );
        assert_eq!(write_sums(&sample_entries), expected_sums);
        assert_eq!(
            fingerprint(&sample_entries),
            sha256_hex(expected_sums.as_bytes())
        );
    }

    #[test]
    fn shipped_corpus_refuses_nonpublic_files_and_views() {
        let mut tree = build_synthetic_public_tree();
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-private.md",
            valid_page_markdown("cmp-private", "np-0001").into_bytes(),
        );
        let err = shipped_corpus(&tree, &BTreeSet::new()).expect_err("non-public page must fail");
        assert!(matches!(err, ManifestError::NonPublicFile { .. }), "{err}");

        // Also test via LabelledView with a clean tree.
        let clean_tree = build_synthetic_public_tree();
        let today = CivilDate::from_ymd(2026, 3, 1).expect("valid date");
        let (model, _) = load(&Host::new(clean_tree, today), &Options::default());
        let eff = EffectivePolicy::floor(false);
        let now = Now::new(today, 0).expect("valid now");
        let Decision::Allowed(allowed) =
            egress_check(&eff, &Dest::Edition, None, None, now, Shipped::Hours(0))
        else {
            panic!("edition preflight must allow");
        };
        let view = LabelledView::new(&model, &allowed, &ReadableStores::corpus());
        let from_view = shipped_corpus(&view, &BTreeSet::new()).expect("public view succeeds");
        assert!(!from_view.is_empty());
    }

    #[test]
    fn support_catalogue_literal_bytes_and_hash() {
        // Exact 194-byte synthetic vector from `docs/reference/audit-contract.md#public-support-verification`.
        // The expected literal is NOT calculated with the encoder under test.
        const EXPECTED_LITERAL: &str = concat!(
            "support_schema = 1\n",
            "corpus_schema = 1\n",
            "commit = \"1111111111111111111111111111111111111111\"\n",
            "\n",
            "[[file]]\n",
            "path = \"README.md\"\n",
            "sha256 = \"c9bda0b40940d8ac5e2a25f44febdb435b6f38fdcad65eaa4c1ab5e488523c06\"\n",
        );
        const EXPECTED_README_SHA256: &str =
            "c9bda0b40940d8ac5e2a25f44febdb435b6f38fdcad65eaa4c1ab5e488523c06";
        const EXPECTED_CATALOGUE_SHA256: &str =
            "088869d68222f925aa1e96f5d06a7bf5491188f04689209b8f3b28301349514c";

        assert_eq!(EXPECTED_LITERAL.len(), 194);
        assert_eq!(
            sha256_hex(b"synthetic public support\n"),
            EXPECTED_README_SHA256
        );
        assert_eq!(
            sha256_hex(EXPECTED_LITERAL.as_bytes()),
            EXPECTED_CATALOGUE_SHA256
        );

        let built = SupportCatalogue::new(
            1,
            "1111111111111111111111111111111111111111",
            vec![("README.md".to_owned(), EXPECTED_README_SHA256.to_owned())],
        )
        .expect("valid single-entry catalogue");
        assert_eq!(built.to_toml(), EXPECTED_LITERAL);
        let (bytes, digest) = built.to_toml_bytes_and_sha256();
        assert_eq!(bytes, EXPECTED_LITERAL.as_bytes());
        assert_eq!(digest, EXPECTED_CATALOGUE_SHA256);

        let verified =
            verify_support_catalogue_bytes(EXPECTED_LITERAL.as_bytes(), EXPECTED_CATALOGUE_SHA256)
                .expect("exact bytes verify");
        assert_eq!(verified, built);

        // Entries provided out of order are sorted by UTF-8 bytes of `path`.
        let multi = SupportCatalogue::new(
            1,
            "1111111111111111111111111111111111111111",
            vec![
                ("corpus.toml".to_owned(), "22".repeat(32)),
                ("AGENTS.md".to_owned(), "11".repeat(32)),
            ],
        )
        .expect("valid multi-entry catalogue");
        let multi_paths: Vec<&str> = multi.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(multi_paths, vec!["AGENTS.md", "corpus.toml"]);

        // Adding a comment or formatting change alters the raw bytes and MUST fail digest verification
        // against the original digest (never hashing a parsed map or reserialized TOML).
        let with_comment = format!("# synthetic comment\n{EXPECTED_LITERAL}");
        assert_ne!(
            sha256_hex(with_comment.as_bytes()),
            EXPECTED_CATALOGUE_SHA256
        );
        let comment_err =
            verify_support_catalogue_bytes(with_comment.as_bytes(), EXPECTED_CATALOGUE_SHA256)
                .expect_err("comment change must fail raw-byte hash verification");
        assert!(matches!(
            comment_err,
            SupportCatalogueError::DigestMismatch { .. }
        ));
    }

    #[test]
    fn support_catalogue_is_strict_and_complete() {
        // 1. Build a complete public tree with all landed support roles (with and without optional pipeline/rules.toml).
        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes());
        tree.insert_bytes("corpus/AGENTS.md", b"CANARY_AGENTS_BODY\n");
        tree.insert_bytes(
            format!("corpus/{}", super::assistant_md_name()),
            b"CANARY_ASSISTANT_BODY\n",
        );
        tree.insert_bytes("corpus/README.md", b"CANARY_README_BODY\n");
        tree.insert_bytes("corpus/changes.md", b"## edition-001\n\n- Added: syn.\n");
        tree.insert_bytes("corpus/scan-allow.toml", b"allow = []\n");
        for ty in PageType::ALL {
            tree.insert_bytes(
                format!("corpus/templates/{}.md", ty.as_str()),
                format!("template body for {}\n", ty.as_str()).into_bytes(),
            );
        }
        tree.insert_bytes("corpus/templates/workbooks/rate-book.csv", b"col_a,col_b\n");
        tree.insert_bytes("corpus/eval/regression.toml", b"case = []\n");

        let commit = "2222222222222222222222222222222222222222";
        let cat_without_rules = SupportCatalogue::from_public_source(&tree, commit)
            .expect("valid support catalogue without optional rules.toml");
        assert!(
            !cat_without_rules
                .files
                .iter()
                .any(|f| f.path == "pipeline/rules.toml")
        );

        // Now add optional pipeline/rules.toml and verify all roles are present.
        tree.insert_bytes("corpus/pipeline/rules.toml", b"rule = []\n");
        let cat_with_rules = SupportCatalogue::from_public_source(&tree, commit)
            .expect("valid support catalogue with optional rules.toml");
        assert!(
            cat_with_rules
                .files
                .iter()
                .any(|f| f.role == SupportRole::PipelineRules && f.path == "pipeline/rules.toml")
        );
        assert!(
            cat_with_rules
                .files
                .iter()
                .any(|f| f.role == SupportRole::Regression && f.path == "eval/regression.toml")
        );
        assert!(
            cat_with_rules
                .files
                .iter()
                .any(|f| f.role == SupportRole::WorkbookCsvTemplate
                    && f.path == "templates/workbooks/rate-book.csv")
        );
        for ty in PageType::ALL {
            assert!(
                cat_with_rules
                    .files
                    .iter()
                    .any(|f| f.role == SupportRole::PageTemplate(ty))
            );
        }

        // No source file bytes occur in the serialized catalogue output.
        let serialized = cat_with_rules.to_toml();
        assert!(!serialized.contains("CANARY_AGENTS_BODY"));
        assert!(!serialized.contains("CANARY_ASSISTANT_BODY"));
        assert!(!serialized.contains("CANARY_README_BODY"));

        // 2. Unknown roles / paths refuse.
        for bad_role in [
            "corpus/README.md",
            "pages/compliance/far/cmp-sample.md",
            "registers/sources.csv",
            "sources/public/S-0001-v1.txt",
            "templates/unknown-type.md",
            "templates/concept.txt",
            "templates/workbooks/nested/sheet.csv",
            "templates/workbooks/sheet.xlsx",
            "eval/other.toml",
            "pipeline/other.toml",
            "state/settings.toml",
        ] {
            assert!(
                classify_support_path(bad_role).is_err(),
                "expected `{bad_role}` to be refused"
            );
        }

        // An unrecognized file under `corpus/templates/` in the source tree refuses generation.
        let mut tree_with_unknown = tree.clone();
        tree_with_unknown.insert_bytes("corpus/templates/unknown.md", b"unknown\n");
        assert!(SupportCatalogue::from_public_source(&tree_with_unknown, commit).is_err());

        // Non-bytes (original/link) entry at a support path refuses generation.
        let mut tree_with_link = tree.clone();
        tree_with_link.insert(
            "corpus/README.md",
            Entry::Original {
                sha256: "11".repeat(32),
                len: 0,
            },
        );
        assert!(SupportCatalogue::from_public_source(&tree_with_link, commit).is_err());

        // 3. Traversal, backslash, absolute, drive, control-char, and non-NFC paths refuse.
        for bad_path in [
            "",
            "/README.md",
            "README.md/",
            "../README.md",
            "./README.md",
            "templates/../README.md",
            "templates\\concept.md",
            "C:README.md",
            "README.md ",
            "README.md.",
            "CON.md",
            "README\0.md",
            "e\u{0301}.md",
        ] {
            assert!(
                classify_support_path(bad_path).is_err(),
                "expected non-canonical path `{bad_path:?}` to be refused"
            );
        }

        // 4. Duplicate paths, unsorted paths, and case-colliding paths refuse in parser.
        let dup_toml = concat!(
            "support_schema = 1\n",
            "corpus_schema = 1\n",
            "commit = \"1111111111111111111111111111111111111111\"\n\n",
            "[[file]]\npath = \"README.md\"\nsha256 = \"",
            "1111111111111111111111111111111111111111111111111111111111111111\"\n\n",
            "[[file]]\npath = \"README.md\"\nsha256 = \"",
            "1111111111111111111111111111111111111111111111111111111111111111\"\n",
        );
        assert!(SupportCatalogue::parse_str(dup_toml).is_err());

        let unsorted_toml = concat!(
            "support_schema = 1\n",
            "corpus_schema = 1\n",
            "commit = \"1111111111111111111111111111111111111111\"\n\n",
            "[[file]]\npath = \"corpus.toml\"\nsha256 = \"",
            "1111111111111111111111111111111111111111111111111111111111111111\"\n\n",
            "[[file]]\npath = \"AGENTS.md\"\nsha256 = \"",
            "2222222222222222222222222222222222222222222222222222222222222222\"\n",
        );
        assert!(SupportCatalogue::parse_str(unsorted_toml).is_err());

        // 5. Malformed hashes, bad schemas, and unknown keys refuse.
        let bad_sha_toml = concat!(
            "support_schema = 1\n",
            "corpus_schema = 1\n",
            "commit = \"1111111111111111111111111111111111111111\"\n\n",
            "[[file]]\npath = \"README.md\"\nsha256 = \"short\"\n",
        );
        assert!(SupportCatalogue::parse_str(bad_sha_toml).is_err());

        let bad_schema_toml = concat!(
            "support_schema = 2\n",
            "corpus_schema = 1\n",
            "commit = \"1111111111111111111111111111111111111111\"\n\n",
            "[[file]]\npath = \"README.md\"\nsha256 = \"",
            "1111111111111111111111111111111111111111111111111111111111111111\"\n",
        );
        assert!(SupportCatalogue::parse_str(bad_schema_toml).is_err());

        let unknown_key_toml = concat!(
            "support_schema = 1\n",
            "corpus_schema = 1\n",
            "commit = \"1111111111111111111111111111111111111111\"\n",
            "extra = 1\n\n",
            "[[file]]\npath = \"README.md\"\nsha256 = \"",
            "1111111111111111111111111111111111111111111111111111111111111111\"\n",
        );
        assert!(SupportCatalogue::parse_str(unknown_key_toml).is_err());

        // 6. Both input bounds (1 MiB byte bound and 4096 file-count bound) refuse.
        let oversized = vec![b'a'; MAX_SUPPORT_CATALOGUE_BYTES + 1];
        assert!(matches!(
            SupportCatalogue::parse_bytes(&oversized),
            Err(SupportCatalogueError::InputTooLarge { .. })
        ));

        let too_many_entries: Vec<(String, String)> = (0..=MAX_SUPPORT_CATALOGUE_ENTRIES)
            .map(|i| (format!("templates/workbooks/w-{i:04}.csv"), "11".repeat(32)))
            .collect();
        assert!(matches!(
            SupportCatalogue::new(
                1,
                "1111111111111111111111111111111111111111",
                too_many_entries
            ),
            Err(SupportCatalogueError::TooManyEntries { .. })
        ));
    }

    #[test]
    fn support_catalogue_never_panics_on_200_seeded_inputs() {
        const VALID: &[u8] = concat!(
            "support_schema = 1\n",
            "corpus_schema = 1\n",
            "commit = \"1111111111111111111111111111111111111111\"\n",
            "\n",
            "[[file]]\n",
            "path = \"README.md\"\n",
            "sha256 = \"c9bda0b40940d8ac5e2a25f44febdb435b6f38fdcad65eaa4c1ab5e488523c06\"\n",
        )
        .as_bytes();
        const EXPECTED_SHA: &str =
            "088869d68222f925aa1e96f5d06a7bf5491188f04689209b8f3b28301349514c";

        let mut rng = Rng::new(1);
        for i in 0..200usize {
            let len = i % 257;
            let bytes = rng.bytes(len);
            let _ = SupportCatalogue::parse_bytes(&bytes);
            let _ = verify_support_catalogue_bytes(&bytes, EXPECTED_SHA);
        }

        for idx in 0..VALID.len() {
            let mut flipped = VALID.to_vec();
            flipped[idx] ^= 0x5a;
            let _ = SupportCatalogue::parse_bytes(&flipped);
            let _ = verify_support_catalogue_bytes(&flipped, EXPECTED_SHA);

            let mut deleted = VALID.to_vec();
            deleted.remove(idx);
            let _ = SupportCatalogue::parse_bytes(&deleted);
            let _ = verify_support_catalogue_bytes(&deleted, EXPECTED_SHA);
        }
    }

    #[test]
    fn one_executable_accepts_two_supported_editions() {
        // Production code in `manifest.rs` must not embed `corpus/` bytes or any 64-hex hash constant.
        let src = include_str!("manifest.rs");
        let prod_src = src.split("#[cfg(test)]").next().unwrap_or(src);
        assert!(
            !prod_src.contains("include_str!") && !prod_src.contains("include_bytes!"),
            "production manifest.rs must not embed corpus bytes with include_str!/include_bytes!"
        );
        for token in prod_src.split('"') {
            assert!(
                !super::is_lower_hex(token, 64),
                "production manifest.rs must not hardcode a 64-hex sha256 constant: `{token}`"
            );
        }

        // Build two distinct synthetic public editions with different support content and commits,
        // and verify that the same executable accepts both as data and cross-rejects mismatched trees.
        let mut tree_a = Tree::new();
        tree_a.insert_dir("corpus");
        tree_a.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes());
        tree_a.insert_bytes("corpus/README.md", b"Edition A support readme\n");
        tree_a.insert_bytes("corpus/changes.md", b"## edition-001\n\n- Added: A.\n");

        let mut tree_b = Tree::new();
        tree_b.insert_dir("corpus");
        tree_b.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes());
        tree_b.insert_bytes("corpus/README.md", b"Edition B updated support readme\n");
        tree_b.insert_bytes("corpus/changes.md", b"## edition-002\n\n- Changed: B.\n");

        let (bytes_a, sha_a) = SupportCatalogue::from_public_source(
            &tree_a,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .expect("catalogue A")
        .to_toml_bytes_and_sha256();
        let (bytes_b, sha_b) = SupportCatalogue::from_public_source(
            &tree_b,
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        )
        .expect("catalogue B")
        .to_toml_bytes_and_sha256();

        assert_ne!(sha_a, sha_b);

        let parsed_a =
            verify_support_catalogue_bytes(&bytes_a, &sha_a).expect("catalogue A verifies");
        let parsed_b =
            verify_support_catalogue_bytes(&bytes_b, &sha_b).expect("catalogue B verifies");

        parsed_a
            .verify_present_support_files(&tree_a)
            .expect("A matches tree A");
        parsed_b
            .verify_present_support_files(&tree_b)
            .expect("B matches tree B");

        assert!(parsed_a.verify_present_support_files(&tree_b).is_err());
        assert!(parsed_b.verify_present_support_files(&tree_a).is_err());
    }
}
