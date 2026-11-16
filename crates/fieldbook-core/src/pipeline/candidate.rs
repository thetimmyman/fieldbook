//! Candidate preparation, field vocabularies, label hints, and verified extractions.

use std::collections::BTreeMap;
use std::fmt;

use crate::date::{CivilDate, PartialDate, Volatility};
use crate::extraction::{
    ExtractError, Extraction, ExtractionMethod, ExtractionToml, LocatorPages, Method, PageText,
    thin_page_numbers,
};
use crate::extraction::{page_stem, recognise_locators};
use crate::hash::sha256_hex;
use crate::ids::VersionId;
use crate::labels::{LabelContext, LabelId, LabelSet, MarkingRules, for_text};
use crate::schema::rows::{MAX_TAG_CHARS, MAX_TEXT_CHARS};
use crate::schema::{AuthorityTier, DocKind, SourceKind, vocab};

use super::suggest::{By, Support};

/// Stream provenance recorded by `sync` for a pulled candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    /// Configured system identifier from `systems.toml`.
    pub system: String,
    /// Configured stream identifier within `system`.
    pub stream: String,
    /// Stream-local item key.
    pub key: String,
    /// Source `as_of` partial date declared by the stream.
    pub as_of: PartialDate,
    /// Civil date when the item was retrieved.
    pub retrieved: CivilDate,
    /// Existing source version replaced by a `new_version` stream item, if any.
    pub base: Option<VersionId>,
    /// Vintage stream identifier (`vocab::stream_id_form`), when part of a vintage series.
    pub series: Option<String>,
    /// Whether the item was saved from a workbook (`false` omits the key in TOML).
    pub workbook: bool,
}

/// Candidate input prepared for classification, correlation, and proposal creation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// Name or slash path relative to the inbox.
    pub file: String,
    /// 64-character lowercase hexadecimal SHA-256 of the candidate bytes.
    pub sha256: String,
    /// Byte length of the candidate file.
    pub bytes: u64,
    /// Lowercased filename extension without a leading dot.
    pub ext: String,
    /// Sensitivity labels assigned solely by [`for_text`].
    pub labels: LabelSet,
    /// Stream provenance when produced by `sync`.
    pub origin: Option<Origin>,
    /// Extracted text pages when text extraction succeeded or was staged.
    pub extraction: Option<Extraction>,
}

impl Candidate {
    /// Derives the handling [`LabelSet`] of `extraction` in `context` using `rules`.
    #[must_use]
    pub fn labels_for_extraction(
        extraction: Option<&Extraction>,
        rules: &MarkingRules,
        context: &LabelContext,
    ) -> LabelSet {
        let mut combined = String::new();
        if let Some(ext) = extraction {
            for (idx, page) in ext.pages.iter().enumerate() {
                if idx > 0 {
                    combined.push('\n');
                }
                combined.push_str(&page.text);
            }
        }
        for_text(&combined, rules, context)
    }

    /// Constructs a [`Candidate`], computing `labels` via [`for_text`].
    #[expect(
        clippy::too_many_arguments,
        reason = "candidate constructor binds file metadata, optional origin/extraction, and labelling context"
    )]
    #[must_use]
    pub fn from_extraction(
        file: impl Into<String>,
        sha256: impl Into<String>,
        bytes: u64,
        ext: impl Into<String>,
        origin: Option<Origin>,
        extraction: Option<Extraction>,
        rules: &MarkingRules,
        context: &LabelContext,
    ) -> Self {
        let labels = Self::labels_for_extraction(extraction.as_ref(), rules, context);
        Self {
            file: file.into(),
            sha256: sha256.into(),
            bytes,
            ext: ext.into(),
            labels,
            origin,
            extraction,
        }
    }

    /// Returns the first non-empty trimmed line of page 1 along with its 1-based page number.
    #[must_use]
    pub fn first_line(&self) -> Option<(u32, &str)> {
        let ext = self.extraction.as_ref()?;
        let page = ext
            .pages
            .iter()
            .find(|p| p.number == 1)
            .or_else(|| ext.pages.first())?;
        for line in page.text.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                return Some((page.number, trimmed));
            }
        }
        None
    }

    /// Returns the first `n` whitespace-delimited words of page 1 joined by single spaces, or `None`.
    #[must_use]
    pub fn first_words(&self, n: usize) -> Option<String> {
        if n == 0 {
            return None;
        }
        let ext = self.extraction.as_ref()?;
        let page = ext
            .pages
            .iter()
            .find(|p| p.number == 1)
            .or_else(|| ext.pages.first())?;
        let words: Vec<&str> = page.text.split_whitespace().take(n).collect();
        if words.is_empty() {
            None
        } else {
            Some(words.join(" "))
        }
    }
}

/// Closed set of `sources.csv` fields that a classifier or person may suggest during intake.
///
/// Reserved columns (`source_role`, `source_family`, `family_basis`, `as_of`, `lifecycle`,
/// `handling`, `series`, `source_id`, `version`, `location`, `sha256`, `retrieved`, `supersedes`,
/// `note`, and `origin_*`) intentionally have no variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SuggestField {
    /// Human-readable source title (`title`).
    Title,
    /// Issuing body or publisher (`publisher`).
    Publisher,
    /// Closed Corpus or store kind (`source_kind`).
    SourceKind,
    /// Optional reliability tier (`authority_tier`).
    AuthorityTier,
    /// Licence or copyright terms (`licence`).
    Licence,
    /// Publication partial date (`published`).
    Published,
    /// Effective date (`effective_from`).
    EffectiveFrom,
    /// Review cadence tier (`volatility`).
    Volatility,
    /// Short grouping tag (`tag`).
    Tag,
    /// Intended use or question link (`purpose`).
    Purpose,
    /// Company-store document class (`doc_kind`; never offered for Corpus).
    DocKind,
}

impl SuggestField {
    /// The ten fields valid for the public repository Corpus.
    pub const CORPUS_FIELDS: [Self; 10] = [
        Self::Title,
        Self::Publisher,
        Self::SourceKind,
        Self::AuthorityTier,
        Self::Licence,
        Self::Published,
        Self::EffectiveFrom,
        Self::Volatility,
        Self::Tag,
        Self::Purpose,
    ];

    /// Every variant of [`SuggestField`], in canonical order.
    pub const ALL: [Self; 11] = [
        Self::Title,
        Self::Publisher,
        Self::SourceKind,
        Self::AuthorityTier,
        Self::Licence,
        Self::Published,
        Self::EffectiveFrom,
        Self::Volatility,
        Self::Tag,
        Self::Purpose,
        Self::DocKind,
    ];

    /// The exact lowercase `sources.csv` column name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Publisher => "publisher",
            Self::SourceKind => "source_kind",
            Self::AuthorityTier => "authority_tier",
            Self::Licence => "licence",
            Self::Published => "published",
            Self::EffectiveFrom => "effective_from",
            Self::Volatility => "volatility",
            Self::Tag => "tag",
            Self::Purpose => "purpose",
            Self::DocKind => "doc_kind",
        }
    }

    /// Parses an exact `sources.csv` column name into [`SuggestField`].
    #[must_use]
    pub fn parse(input: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.as_str() == input)
    }
}

impl fmt::Display for SuggestField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Reason a candidate suggestion value fails schema/cell validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueProblem {
    /// Value is empty.
    Empty,
    /// Value contains CR/LF or `|`.
    ForbiddenChars,
    /// Value exceeds the column character limit.
    TooLong,
    /// Value is not in the allowed enum vocabulary.
    NotInVocab,
    /// Value fails partial-date parsing.
    InvalidDate,
}

/// Allowed fields and closed enum values offered by a target store schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vocab {
    fields: Vec<SuggestField>,
    source_kinds: Vec<&'static str>,
    authority_tiers: Vec<&'static str>,
    volatilities: Vec<&'static str>,
    doc_kinds: Vec<&'static str>,
}

impl Vocab {
    /// Builds the Corpus intake vocabulary (ten fields; never `doc_kind`, never overlay kinds).
    #[must_use]
    pub fn from_schema() -> Self {
        let source_kinds = SourceKind::ALL
            .iter()
            .filter(|k| !k.is_overlay())
            .map(|k| k.as_str())
            .collect();
        let authority_tiers = AuthorityTier::ALL.iter().map(|t| t.as_str()).collect();
        let volatilities = Volatility::ALL.iter().map(|v| v.as_str()).collect();
        let doc_kinds = DocKind::ALL.iter().map(|d| d.as_str()).collect();
        Self {
            fields: SuggestField::CORPUS_FIELDS.to_vec(),
            source_kinds,
            authority_tiers,
            volatilities,
            doc_kinds,
        }
    }

    /// Lists the fields offered by this store vocabulary.
    #[must_use]
    pub fn fields(&self) -> &[SuggestField] {
        &self.fields
    }

    /// Returns `true` when `field` is offered by this store vocabulary.
    #[must_use]
    pub fn offers(&self, field: SuggestField) -> bool {
        self.fields.contains(&field)
    }

    /// Returns the closed set of allowed string values for an enum field, or `None` for free text/dates.
    #[must_use]
    pub fn allowed_values(&self, field: SuggestField) -> Option<&[&'static str]> {
        match field {
            SuggestField::SourceKind => Some(&self.source_kinds),
            SuggestField::AuthorityTier => Some(&self.authority_tiers),
            SuggestField::Volatility => Some(&self.volatilities),
            SuggestField::DocKind => Some(&self.doc_kinds),
            SuggestField::Title
            | SuggestField::Publisher
            | SuggestField::Licence
            | SuggestField::Published
            | SuggestField::EffectiveFrom
            | SuggestField::Tag
            | SuggestField::Purpose => None,
        }
    }

    /// Validates `value` against the schema vocabulary or FB0204 cell rule for `field`.
    ///
    /// # Errors
    ///
    /// Returns [`ValueProblem`] if `value` violates empty, character, length, enum, or date rules.
    pub fn accepts_value(&self, field: SuggestField, value: &str) -> Result<(), ValueProblem> {
        if value.is_empty() {
            return Err(ValueProblem::Empty);
        }
        if value.contains(['\n', '\r', '|']) {
            return Err(ValueProblem::ForbiddenChars);
        }
        if let Some(allowed) = self.allowed_values(field) {
            return if allowed.contains(&value) {
                Ok(())
            } else {
                Err(ValueProblem::NotInVocab)
            };
        }
        match field {
            SuggestField::Tag => {
                if value.chars().count() > MAX_TAG_CHARS {
                    return Err(ValueProblem::TooLong);
                }
                Ok(())
            }
            SuggestField::Published | SuggestField::EffectiveFrom => PartialDate::parse(value)
                .map(|_| ())
                .map_err(|_| ValueProblem::InvalidDate),
            SuggestField::Title
            | SuggestField::Publisher
            | SuggestField::Licence
            | SuggestField::Purpose => {
                if value.chars().count() > MAX_TEXT_CHARS {
                    return Err(ValueProblem::TooLong);
                }
                Ok(())
            }
            SuggestField::SourceKind
            | SuggestField::AuthorityTier
            | SuggestField::Volatility
            | SuggestField::DocKind => Err(ValueProblem::NotInVocab),
        }
    }
}

/// Optional handling-label hint proposed for human confirmation during company review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelHint {
    /// Non-builtin label identifier.
    pub label: LabelId,
    /// Evidence supporting the label hint.
    pub support: Support,
    /// Adapter or origin that produced the hint.
    pub by: By,
}

/// Validated metadata state recorded inside [`VerifiedExtraction`].
#[derive(Debug, Clone, PartialEq)]
pub enum VerifiedMethodMetadata {
    /// Extraction method is registered in `METHODS` and its metadata passed schema validation.
    Known {
        /// Registered method specification.
        method: &'static Method,
        /// Optional method-specific table validated against `method.keys`.
        method_data: Option<toml::Table>,
        /// Extra root metadata validated against `method.keys`.
        extra: BTreeMap<String, toml::Value>,
    },
    /// Extraction method is unknown to this binary; raw metadata is preserved as unavailable.
    Unavailable {
        /// Unrecognized method name from `extraction.toml`.
        method: String,
        /// Fixed reason describing why schema metadata cannot be verified.
        reason: &'static str,
    },
}

/// Complete extracted text and verified metadata bound to one original SHA-256.
#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedExtraction {
    /// Complete page-text extraction.
    pub extraction: Extraction,
    /// Verified `extraction.toml` record bound to the original and page hashes.
    pub record: ExtractionToml,
    /// Exact UTF-8 bytes per page stem (`"p0001"`, ...).
    pub page_bytes: BTreeMap<String, Vec<u8>>,
    /// Verified SHA-256 hex digest per page stem (`"p0001"`, ...).
    pub page_hashes: BTreeMap<String, String>,
    /// Recognised and retained section locators.
    pub locators: BTreeMap<String, LocatorPages>,
    /// Extraction tool name recorded in `extraction.toml`.
    pub tool: String,
    /// Extraction tool version recorded in `extraction.toml`.
    pub tool_version: String,
    /// Original file SHA-256 digest this extraction is bound to.
    pub original_sha256: String,
    /// Method metadata validation outcome.
    pub method_metadata: VerifiedMethodMetadata,
}

impl VerifiedExtraction {
    /// Verifies `extraction_toml_bytes` and `page_files` (keyed by stem, e.g. `"p0001"`)
    /// against `expected_original_sha256`, constructing a [`VerifiedExtraction`].
    ///
    /// # Errors
    ///
    /// Returns [`ExtractError::Missing`] if any hash, count, thin-page set, or method schema
    /// constraint fails.
    pub fn verify(
        expected_original_sha256: &str,
        extraction_toml_bytes: &[u8],
        mut page_files: BTreeMap<String, Vec<u8>>,
    ) -> Result<Self, ExtractError> {
        if !is_hex64(expected_original_sha256) {
            return Err(ExtractError::Missing("matching staging folder"));
        }
        let toml_text = std::str::from_utf8(extraction_toml_bytes)
            .map_err(|_| ExtractError::Missing("matching staging folder"))?;
        let record = ExtractionToml::parse(toml_text)
            .map_err(|_| ExtractError::Missing("matching staging folder"))?;
        if record.original_sha256 != expected_original_sha256 {
            return Err(ExtractError::Missing("matching staging folder"));
        }

        let expected_pages = usize::try_from(record.page_count)
            .map_err(|_| ExtractError::Missing("matching staging folder"))?;
        if record.pages.len() != expected_pages || page_files.len() != expected_pages {
            return Err(ExtractError::Missing("matching staging folder"));
        }

        let mut pages = Vec::with_capacity(expected_pages);
        let mut verified_bytes = BTreeMap::new();
        let mut verified_hashes = BTreeMap::new();
        for num in 1..=record.page_count {
            let stem = page_stem(num);
            let expected_hash = record
                .pages
                .get(&stem)
                .ok_or(ExtractError::Missing("matching staging folder"))?;
            let bytes = page_files
                .remove(&stem)
                .ok_or(ExtractError::Missing("matching staging folder"))?;
            let actual_hash = sha256_hex(&bytes);
            if actual_hash != *expected_hash {
                return Err(ExtractError::Missing("matching staging folder"));
            }
            let text = std::str::from_utf8(&bytes)
                .map_err(|_| ExtractError::Missing("matching staging folder"))?
                .to_owned();
            pages.push(PageText { number: num, text });
            verified_hashes.insert(stem.clone(), actual_hash);
            verified_bytes.insert(stem, bytes);
        }

        if thin_page_numbers(&pages) != record.thin_pages {
            return Err(ExtractError::Missing("matching staging folder"));
        }

        let (method_name, method_metadata) = match &record.method {
            ExtractionMethod::Known(method) => {
                if record.method_data.is_some() && !method.keys.contains(&"method_data") {
                    return Err(ExtractError::Missing("matching staging folder"));
                }
                if record
                    .extra
                    .keys()
                    .any(|k| !method.keys.contains(&k.as_str()))
                {
                    return Err(ExtractError::Missing("matching staging folder"));
                }
                (
                    method.name,
                    VerifiedMethodMetadata::Known {
                        method,
                        method_data: record.method_data.clone(),
                        extra: record.extra.clone(),
                    },
                )
            }
            ExtractionMethod::Unsupported(name) => (
                "unsupported",
                VerifiedMethodMetadata::Unavailable {
                    method: name.clone(),
                    reason: "unsupported extraction method",
                },
            ),
        };

        let extraction = Extraction {
            method: method_name,
            pages,
        };
        let tool = record.tool.clone();
        let tool_version = record.tool_version.clone();
        let original_sha256 = record.original_sha256.clone();
        let locators = record.locators.clone();

        Ok(Self {
            extraction,
            record,
            page_bytes: verified_bytes,
            page_hashes: verified_hashes,
            locators,
            tool,
            tool_version,
            original_sha256,
            method_metadata,
        })
    }

    /// Builds a [`VerifiedExtraction`] directly from an in-memory [`Extraction`] for tests or callers
    /// that generated the extraction in-process.
    #[must_use]
    pub fn from_extraction(
        extraction: Extraction,
        tool: &str,
        tool_version: &str,
        original_sha256: &str,
    ) -> Self {
        let mut locators = recognise_locators(&extraction.pages);
        let record = crate::extraction::build_record(
            &extraction,
            tool,
            tool_version,
            original_sha256,
            &locators,
        );
        locators = record.locators.clone();
        let mut page_bytes = BTreeMap::new();
        for page in &extraction.pages {
            page_bytes.insert(page_stem(page.number), page.text.as_bytes().to_vec());
        }
        let page_hashes = record.pages.clone();
        let method_metadata = match &record.method {
            ExtractionMethod::Known(method) => VerifiedMethodMetadata::Known {
                method,
                method_data: record.method_data.clone(),
                extra: record.extra.clone(),
            },
            ExtractionMethod::Unsupported(name) => VerifiedMethodMetadata::Unavailable {
                method: name.clone(),
                reason: "unsupported extraction method",
            },
        };
        Self {
            extraction,
            record,
            page_bytes,
            page_hashes,
            locators,
            tool: tool.to_owned(),
            tool_version: tool_version.to_owned(),
            original_sha256: original_sha256.to_owned(),
            method_metadata,
        }
    }

    /// Returns the canonical rendered `extraction.toml` bytes for this verified extraction.
    #[must_use]
    pub fn rendered_toml_bytes(&self) -> Vec<u8> {
        self.record.render().into_bytes()
    }
}

pub(crate) fn is_hex64(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[must_use]
#[expect(
    dead_code,
    reason = "used by vocab::stream_id_form callers in pipeline"
)]
pub(crate) fn is_valid_stream_id(s: &str) -> bool {
    vocab::stream_id_form(s)
}
