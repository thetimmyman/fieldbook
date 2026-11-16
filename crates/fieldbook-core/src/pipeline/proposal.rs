//! Queue proposal schema, strict TOML parsing, and canonical rendering (`queue/P-nnnn.toml`).
//!
//! # Proposal Keys (`proposal_version = 1`)
//!
//! Top-level keys in canonical render order:
//! - `proposal_version = 1`
//! - `state`: `"proposed" | "accepted" | "rejected" | "stale"`
//! - `kind`: `"new_source" | "new_version" | "regression_case" | "gap_question" | "note" | "decision" | "reclassify"`
//! - `labels`: non-empty list of valid label ids assigned by `labels::for_text`
//! - `writer`: integer writer digits (`0` at home)
//! - `[origin]` (optional; written only by `sync`): `system`, `stream`, `key`, `as_of`,
//!   `retrieved`, optional `base` (`new_version` only), optional `series` (`stream_id_form`),
//!   optional `workbook = true` (`workbook = false` is refused)
//! - `[candidate]` (required for `new_source` and `new_version`, absent for others): `file`,
//!   `sha256`, `bytes`
//! - `[extraction]` (optional for `new_source` and `new_version`, absent when unavailable):
//!   `adapter`, `method`, `pages`, `thin_pages`
//! - `[source_choice]` (optional for `new_source` and `new_version`, forbidden for others):
//!   `kind = "unresolved" | "new" | "existing"`, `id = "S-nnnn"` (`existing` only)
//! - Body table: `[row.<field>]` (`value`, `by`, `because`, optional `rule = { id, kind }`) for
//!   source kinds, `[reclassify]` (`source_version`, non-empty `doc_kind` and/or `series`) for
//!   `reclassify`, or `[<kind>]` opaque table kept verbatim for the remaining kinds
//! - `[links]` (optional): `version_of`, `related`, `answers`
//! - `[[hints]]` (optional): `{ label, by, because }`
//! - `[ai]` (optional; required with non-empty `calls` when any `by = "model:..."` appears):
//!   `request_labels`, `inputs`, `calls`, `dropped`

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::date::{CivilDate, PartialDate};
use crate::extraction::ExtractionMethod;
use crate::ids::{SourceId, VersionId};
use crate::labels::dependencies::ItemRef;
use crate::labels::{LabelId, LabelSet, StoreId};
use crate::schema::rows::MAX_TEXT_CHARS;
use crate::schema::{DocKind, vocab};

use super::candidate::{Origin, SuggestField, is_hex64};
use super::suggest::{By, Link, LinkKind, RuleKind, RuleRef};

/// Supported proposal file format version.
pub const PROPOSAL_VERSION: i64 = 1;

/// Kind of queued intake proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProposalKind {
    /// Adding a new source to the store.
    NewSource,
    /// Adding a new version of an existing source.
    NewVersion,
    /// Promoting an asked question/answer to an eval regression case.
    RegressionCase,
    /// Promoting an unanswered ask to `questions.csv`.
    GapQuestion,
    /// AI-drafted note proposal.
    Note,
    /// Decision proposal from the Decide port.
    Decision,
    /// Moving a source version to another `doc_kind` or `series`.
    Reclassify,
}

impl ProposalKind {
    /// Every [`ProposalKind`] variant in canonical order.
    pub const ALL: [Self; 7] = [
        Self::NewSource,
        Self::NewVersion,
        Self::RegressionCase,
        Self::GapQuestion,
        Self::Note,
        Self::Decision,
        Self::Reclassify,
    ];

    /// Lowercase TOML keyword for this proposal kind.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NewSource => "new_source",
            Self::NewVersion => "new_version",
            Self::RegressionCase => "regression_case",
            Self::GapQuestion => "gap_question",
            Self::Note => "note",
            Self::Decision => "decision",
            Self::Reclassify => "reclassify",
        }
    }

    /// Parses one of the closed proposal kind keywords.
    ///
    /// # Errors
    ///
    /// Returns [`ProposalParseError`] if `input` is not a recognized kind.
    pub fn parse(input: &str) -> Result<Self, ProposalParseError> {
        Self::ALL
            .into_iter()
            .find(|k| k.as_str() == input)
            .ok_or_else(|| ProposalParseError::InvalidValue {
                key: "kind".to_owned(),
                reason: format!("unknown proposal kind `{input}`"),
            })
    }

    /// Returns the row id in [`TARGETS`](super::TARGETS) that this kind writes, or `None`.
    #[must_use]
    pub const fn target(&self) -> Option<&'static str> {
        match self {
            Self::NewSource | Self::NewVersion | Self::Reclassify => Some("sources"),
            Self::RegressionCase | Self::GapQuestion | Self::Note | Self::Decision => None,
        }
    }

    /// Returns `true` for the source-bearing kinds (`new_source` and `new_version`).
    #[must_use]
    pub const fn is_source_bearing(self) -> bool {
        matches!(self, Self::NewSource | Self::NewVersion)
    }
}

impl fmt::Display for ProposalKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Lifecycle state of a proposal in the review queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProposalState {
    /// Waiting for review and decision.
    Proposed,
    /// Accepted and written to the store.
    Accepted,
    /// Explicitly rejected by a reviewer.
    Rejected,
    /// Source file or Corpus changed after proposal creation.
    Stale,
}

impl ProposalState {
    /// Every [`ProposalState`] variant.
    pub const ALL: [Self; 4] = [Self::Proposed, Self::Accepted, Self::Rejected, Self::Stale];

    /// Lowercase serialized state keyword.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Stale => "stale",
        }
    }

    /// Parses one of `"proposed"`, `"accepted"`, `"rejected"`, or `"stale"`.
    #[must_use]
    pub fn parse(input: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_str() == input)
    }
}

impl fmt::Display for ProposalState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One judged metadata cell entry stored in a source proposal `[row]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Judged {
    /// Proposed or human-edited cell value.
    pub value: String,
    /// Attribution naming the producer (`rules@1`, `person`, `origin`, `model:...`).
    pub by: By,
    /// Evidence summary backing `value` (empty string when written by `person`).
    pub because: String,
    /// Optional rule provenance when `by` was a matching rule.
    pub rule: Option<RuleRef>,
}

/// Suggested and human-judged cells keyed by [`SuggestField`].
pub type RowBody = BTreeMap<SuggestField, Judged>;

/// Persisted source-target selection for a source-bearing proposal (`[source_choice]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceChoice {
    /// Multiple `VersionOf` correlations matched; awaiting human choice before admission.
    Unresolved,
    /// Add as a brand-new `SourceId`.
    New,
    /// Add as a new version under an existing [`SourceId`].
    Existing(SourceId),
}

impl SourceChoice {
    /// Derives [`SourceChoice`] from correlation links (`VersionOf`):
    /// zero matches -> `New`, one match -> `Existing`, multiple distinct matches -> `Unresolved`.
    #[must_use]
    pub fn from_links(links: &[Link]) -> Self {
        let mut distinct = BTreeSet::new();
        for link in links {
            if link.kind == LinkKind::VersionOf
                && let Ok(src) = SourceId::parse(&link.target)
            {
                distinct.insert(src);
            }
        }
        match distinct.len() {
            0 => Self::New,
            1 => distinct
                .into_iter()
                .next()
                .map_or(Self::New, Self::Existing),
            _ => Self::Unresolved,
        }
    }

    /// Returns the effective source proposal kind (`NewVersion` for `Existing`, `NewSource` otherwise).
    #[must_use]
    pub const fn effective_kind(&self) -> ProposalKind {
        match self {
            Self::Existing(_) => ProposalKind::NewVersion,
            Self::New | Self::Unresolved => ProposalKind::NewSource,
        }
    }

    /// Returns the serialized `kind` keyword (`"unresolved"`, `"new"`, `"existing"`).
    #[must_use]
    pub const fn as_kind_str(&self) -> &'static str {
        match self {
            Self::Unresolved => "unresolved",
            Self::New => "new",
            Self::Existing(_) => "existing",
        }
    }
}

/// `[candidate]` metadata recorded for a file-backed proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateMeta {
    /// Relative name under the inbox.
    pub file: String,
    /// Lowercase 64-hex SHA-256 of the candidate file.
    pub sha256: String,
    /// File length in bytes.
    pub bytes: u64,
}

/// `[extraction]` summary recorded when text extraction ran for the candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractionMeta {
    /// Adapter id that produced or replayed the extraction (`"plain@1"`, `"staged@1"`, ...).
    pub adapter: String,
    /// Canonical extraction method name (`"plain"`, ...).
    pub method: String,
    /// Total extracted page count.
    pub pages: u32,
    /// 1-based thin page numbers in strictly ascending order.
    pub thin_pages: Vec<u32>,
}

/// Body payload discriminated by [`ProposalKind`].
#[derive(Debug, Clone, PartialEq)]
pub enum ProposalBody {
    /// `[row]` body for `new_source` and `new_version`.
    Row(RowBody),
    /// `[reclassify]` body for `reclassify`.
    Reclassify {
        /// Existing source version whose classification changes.
        source_version: VersionId,
        /// Target document kind (`Some` when non-empty).
        doc_kind: Option<DocKind>,
        /// Target stream/series identifier (`vocab::stream_id_form`, `Some` when non-empty).
        series: Option<String>,
    },
    /// Opaque TOML table kept verbatim for `regression_case`, `gap_question`, `note`, or `decision`.
    Opaque(toml::Table),
}

/// One related item reference persisted in `[links].related`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatedLink {
    /// Target identifier in the Corpus.
    pub target: String,
    /// Optional search score recorded alongside the link.
    pub score: Option<u32>,
}

/// Correlated links persisted in `[links]`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Links {
    /// Candidate `VersionOf` source alternatives (`S-nnnn`).
    pub version_of: Vec<SourceId>,
    /// Related unit or row targets (`S-nnnn-vN`, `CL-...`, etc.).
    pub related: Vec<RelatedLink>,
    /// Matched open question identifiers (`Q-nnn`).
    pub answers: Vec<String>,
}

impl Links {
    /// Returns `true` when every link list is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.version_of.is_empty() && self.related.is_empty() && self.answers.is_empty()
    }
}

/// One handling label hint persisted on a proposal (`[[hints]]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposalHint {
    /// Non-builtin label identifier proposed for confirmation.
    pub label: LabelId,
    /// Adapter or origin attribution.
    pub by: By,
    /// Supporting reason or excerpt text.
    pub because: String,
}

/// One recorded AI call provenance entry under `[ai].calls`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiCall {
    /// Journal sequence number of the `ai.call` or `ai.paste` event.
    pub seq: u64,
    /// Model adapter identifier.
    pub adapter: String,
    /// Prompt template identifier (`ask@1`, `classify@1`, ...).
    pub template: String,
    /// Lowercase 64-hex SHA-256 of the model output recorded in the journal.
    pub output_sha256: String,
}

/// Model provenance block persisted under `[ai]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiRecord {
    /// Handling labels of the outbound model request.
    pub request_labels: LabelSet,
    /// Store item references included in the outbound request context.
    pub inputs: Vec<ItemRef>,
    /// Recorded model invocations backing `model:...` attributions.
    pub calls: Vec<AiCall>,
    /// Count of dropped model suggestions (`validate` failures).
    pub dropped: usize,
}

/// Complete typed representation of one `queue/P-nnnn.toml` proposal.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    /// Proposal identifier (for example `P-0001`).
    pub id: String,
    /// Queue lifecycle state.
    pub state: ProposalState,
    /// Proposal kind.
    pub kind: ProposalKind,
    /// Labels assigned at proposal creation (never mutated by hints).
    pub labels: LabelSet,
    /// Store writer digits (`0` at home).
    pub writer: u32,
    /// Optional stream provenance set only by `sync`.
    pub origin: Option<Origin>,
    /// Candidate file metadata (`Some` for source kinds, `None` for others).
    pub candidate: Option<CandidateMeta>,
    /// Optional extraction summary (`None` when extraction was unavailable or non-file kind).
    pub extraction: Option<ExtractionMeta>,
    /// Optional source target selection (`new_source` / `new_version` only).
    pub source_choice: Option<SourceChoice>,
    /// Kind-specific body payload.
    pub body: ProposalBody,
    /// Optional label hints proposed for human confirmation.
    pub hints: Vec<ProposalHint>,
    /// Correlator links preserved as alternatives and context.
    pub links: Links,
    /// Optional model provenance block.
    pub ai: Option<AiRecord>,
}

/// Typed parse failure returned by [`Proposal::parse`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposalParseError {
    /// Invalid proposal id argument.
    InvalidId(String),
    /// TOML syntax error.
    Toml(String),
    /// Legacy proposal format (`zone` without `labels`) refused as obsolete.
    OldFormat,
    /// Unknown `proposal_version`.
    UnsupportedVersion(i64),
    /// Missing required key or table.
    MissingKey(&'static str),
    /// Unrecognized key or table.
    UnknownKey(String),
    /// Key has invalid type or out-of-contract value.
    InvalidValue {
        /// Key path or name.
        key: String,
        /// Explanation.
        reason: String,
    },
    /// At least one field carries `by = "model:..."` with no `[ai].calls` entry.
    ModelWithoutCalls,
    /// `reclassify` has neither a non-empty `doc_kind` nor a non-empty `series`.
    ReclassifyChangesNothing,
}

impl fmt::Display for ProposalParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId(id) => write!(f, "invalid proposal id `{id}`"),
            Self::Toml(msg) => write!(f, "invalid proposal TOML: {msg}"),
            Self::OldFormat => {
                write!(
                    f,
                    "old proposal format (`zone` without `labels`) is refused"
                )
            }
            Self::UnsupportedVersion(v) => write!(f, "unsupported proposal_version `{v}`"),
            Self::MissingKey(k) => write!(f, "missing required key `{k}`"),
            Self::UnknownKey(k) => write!(f, "unknown key `{k}` in proposal"),
            Self::InvalidValue { key, reason } => write!(f, "invalid `{key}`: {reason}"),
            Self::ModelWithoutCalls => {
                write!(
                    f,
                    "proposal has `by = \"model:...\"` without an `ai.calls` entry"
                )
            }
            Self::ReclassifyChangesNothing => {
                write!(
                    f,
                    "`reclassify` must set at least one non-empty `doc_kind` or `series`"
                )
            }
        }
    }
}

impl std::error::Error for ProposalParseError {}

impl Proposal {
    /// Parses `text` as a `proposal_version = 1` TOML proposal bound to `id`.
    ///
    /// Denies unknown keys, obsolete `zone` proposals without `labels`, invalid `workbook = false`,
    /// malformed `origin.series`, empty `reclassify` bodies, and `by = "model:..."` without an
    /// `ai.calls` entry.
    ///
    /// # Errors
    ///
    /// Returns [`ProposalParseError`] on any schema or value violation.
    pub fn parse(id: &str, text: &str) -> Result<Self, ProposalParseError> {
        if id.is_empty() || id.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(ProposalParseError::InvalidId(id.to_owned()));
        }

        let root: toml::Table =
            toml::from_str(text).map_err(|e| ProposalParseError::Toml(e.to_string()))?;

        if root.contains_key("zone") && !root.contains_key("labels") {
            return Err(ProposalParseError::OldFormat);
        }

        let version = req_i64(&root, "proposal_version")?;
        if version != PROPOSAL_VERSION {
            return Err(ProposalParseError::UnsupportedVersion(version));
        }

        let state_raw = req_str(&root, "state")?;
        let state =
            ProposalState::parse(state_raw).ok_or_else(|| ProposalParseError::InvalidValue {
                key: "state".to_owned(),
                reason: format!("unknown state `{state_raw}`"),
            })?;

        let kind_raw = req_str(&root, "kind")?;
        let kind = ProposalKind::parse(kind_raw)?;

        let labels = parse_label_array(&root, "labels", true)?;
        let writer_raw = req_i64(&root, "writer")?;
        let writer = u32::try_from(writer_raw).map_err(|_| ProposalParseError::InvalidValue {
            key: "writer".to_owned(),
            reason: "writer must be a non-negative 32-bit integer".to_owned(),
        })?;

        check_top_level_keys(&root, kind)?;

        let origin = if let Some(val) = root.get("origin") {
            Some(parse_origin(val, kind)?)
        } else {
            None
        };

        let candidate = if kind.is_source_bearing() {
            let val = root
                .get("candidate")
                .ok_or(ProposalParseError::MissingKey("candidate"))?;
            Some(parse_candidate(val)?)
        } else {
            if root.contains_key("candidate") {
                return Err(ProposalParseError::UnknownKey("candidate".to_owned()));
            }
            None
        };

        let extraction = if let Some(val) = root.get("extraction") {
            if !kind.is_source_bearing() {
                return Err(ProposalParseError::UnknownKey("extraction".to_owned()));
            }
            Some(parse_extraction(val)?)
        } else {
            None
        };

        let source_choice = if let Some(val) = root.get("source_choice") {
            if !kind.is_source_bearing() {
                return Err(ProposalParseError::UnknownKey("source_choice".to_owned()));
            }
            Some(parse_source_choice(val)?)
        } else {
            None
        };

        let body = match kind {
            ProposalKind::NewSource | ProposalKind::NewVersion => {
                if let Some(val) = root.get("row") {
                    ProposalBody::Row(parse_row_body(val)?)
                } else {
                    ProposalBody::Row(BTreeMap::new())
                }
            }
            ProposalKind::Reclassify => {
                let val = root
                    .get("reclassify")
                    .ok_or(ProposalParseError::MissingKey("reclassify"))?;
                parse_reclassify(val)?
            }
            ProposalKind::RegressionCase
            | ProposalKind::GapQuestion
            | ProposalKind::Note
            | ProposalKind::Decision => {
                let key = kind.as_str();
                let val = root.get(key).ok_or(ProposalParseError::MissingKey(key))?;
                let tbl = val
                    .as_table()
                    .ok_or_else(|| ProposalParseError::InvalidValue {
                        key: key.to_owned(),
                        reason: "must be a table".to_owned(),
                    })?;
                validate_opaque_table(tbl, key)?;
                ProposalBody::Opaque(tbl.clone())
            }
        };

        let links = if let Some(val) = root.get("links") {
            parse_links(val)?
        } else {
            Links::default()
        };

        let hints = if let Some(val) = root.get("hints") {
            parse_hints(val)?
        } else {
            Vec::new()
        };

        let ai = if let Some(val) = root.get("ai") {
            Some(parse_ai(val)?)
        } else {
            None
        };

        let has_model_by = match &body {
            ProposalBody::Row(row) => row.values().any(|j| matches!(j.by, By::Model { .. })),
            ProposalBody::Reclassify { .. } | ProposalBody::Opaque(_) => false,
        } || hints.iter().any(|h| matches!(h.by, By::Model { .. }));

        if has_model_by && ai.as_ref().is_none_or(|a| a.calls.is_empty()) {
            return Err(ProposalParseError::ModelWithoutCalls);
        }

        Ok(Self {
            id: id.to_owned(),
            state,
            kind,
            labels,
            writer,
            origin,
            candidate,
            extraction,
            source_choice,
            body,
            hints,
            links,
            ai,
        })
    }

    /// Renders `self` into canonical TOML bytes in one fixed key order so `render(parse(x)) == x`.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        push_kv_int(&mut out, "proposal_version", PROPOSAL_VERSION);
        push_kv_str(&mut out, "state", self.state.as_str());
        push_kv_str(&mut out, "kind", self.kind.as_str());
        let labels: Vec<&str> = self.labels.ids().iter().map(LabelId::as_str).collect();
        push_kv_str_array(&mut out, "labels", &labels);
        push_kv_int(&mut out, "writer", i64::from(self.writer));

        if let Some(origin) = &self.origin {
            out.push_str("\n[origin]\n");
            push_kv_str(&mut out, "system", &origin.system);
            push_kv_str(&mut out, "stream", &origin.stream);
            push_kv_str(&mut out, "key", &origin.key);
            push_kv_str(&mut out, "as_of", &origin.as_of.to_string());
            push_kv_str(&mut out, "retrieved", &origin.retrieved.to_string());
            if let Some(base) = &origin.base {
                push_kv_str(&mut out, "base", base.as_str());
            }
            if let Some(series) = &origin.series {
                push_kv_str(&mut out, "series", series);
            }
            if origin.workbook {
                out.push_str("workbook = true\n");
            }
        }

        if let Some(cand) = &self.candidate {
            out.push_str("\n[candidate]\n");
            push_kv_str(&mut out, "file", &cand.file);
            push_kv_str(&mut out, "sha256", &cand.sha256);
            push_kv_u64(&mut out, "bytes", cand.bytes);
        }

        if let Some(ext) = &self.extraction {
            out.push_str("\n[extraction]\n");
            push_kv_str(&mut out, "adapter", &ext.adapter);
            push_kv_str(&mut out, "method", &ext.method);
            push_kv_int(&mut out, "pages", i64::from(ext.pages));
            push_kv_u32_array(&mut out, "thin_pages", &ext.thin_pages);
        }

        if let Some(choice) = &self.source_choice {
            out.push_str("\n[source_choice]\n");
            push_kv_str(&mut out, "kind", choice.as_kind_str());
            if let SourceChoice::Existing(id) = choice {
                push_kv_str(&mut out, "id", id.as_str());
            }
        }

        match &self.body {
            ProposalBody::Row(row) => {
                for (field, judged) in row {
                    out.push_str(&format!("\n[row.{}]\n", field.as_str()));
                    push_kv_str(&mut out, "value", &judged.value);
                    push_kv_str(&mut out, "by", &judged.by.format());
                    push_kv_str(&mut out, "because", &judged.because);
                    if let Some(rule) = &judged.rule {
                        out.push_str(&format!(
                            "rule = {{ id = {}, kind = {} }}\n",
                            toml_str(&rule.id),
                            toml_str(rule.kind.as_str())
                        ));
                    }
                }
            }
            ProposalBody::Reclassify {
                source_version,
                doc_kind,
                series,
            } => {
                out.push_str("\n[reclassify]\n");
                push_kv_str(&mut out, "source_version", source_version.as_str());
                if let Some(dk) = doc_kind {
                    push_kv_str(&mut out, "doc_kind", dk.as_str());
                }
                if let Some(ser) = series {
                    push_kv_str(&mut out, "series", ser);
                }
            }
            ProposalBody::Opaque(table) => {
                out.push_str(&format!("\n[{}]\n", self.kind.as_str()));
                render_table_entries(&mut out, table);
            }
        }

        if !self.links.is_empty() {
            out.push_str("\n[links]\n");
            if !self.links.version_of.is_empty() {
                let ids: Vec<&str> = self.links.version_of.iter().map(SourceId::as_str).collect();
                push_kv_str_array(&mut out, "version_of", &ids);
            }
            if !self.links.related.is_empty() {
                let mut items = Vec::with_capacity(self.links.related.len());
                for rel in &self.links.related {
                    if let Some(score) = rel.score {
                        items.push(format!(
                            "{{ target = {}, score = {score} }}",
                            toml_str(&rel.target)
                        ));
                    } else {
                        items.push(toml_str(&rel.target));
                    }
                }
                out.push_str(&format!("related = [{}]\n", items.join(", ")));
            }
            if !self.links.answers.is_empty() {
                let ans: Vec<&str> = self.links.answers.iter().map(String::as_str).collect();
                push_kv_str_array(&mut out, "answers", &ans);
            }
        }

        for hint in &self.hints {
            out.push_str("\n[[hints]]\n");
            push_kv_str(&mut out, "label", hint.label.as_str());
            push_kv_str(&mut out, "by", &hint.by.format());
            push_kv_str(&mut out, "because", &hint.because);
        }

        if let Some(ai) = &self.ai {
            out.push_str("\n[ai]\n");
            let req_labels: Vec<&str> = ai
                .request_labels
                .ids()
                .iter()
                .map(LabelId::as_str)
                .collect();
            push_kv_str_array(&mut out, "request_labels", &req_labels);
            if !ai.inputs.is_empty() {
                let inputs: Vec<String> = ai
                    .inputs
                    .iter()
                    .map(|item| {
                        format!(
                            "{{ store = {}, id = {} }}",
                            toml_str(item.store.as_str()),
                            toml_str(&item.id)
                        )
                    })
                    .collect();
                out.push_str(&format!("inputs = [{}]\n", inputs.join(", ")));
            }
            let calls: Vec<String> = ai
                .calls
                .iter()
                .map(|c| {
                    format!(
                        "{{ seq = {}, adapter = {}, template = {}, output_sha256 = {} }}",
                        c.seq,
                        toml_str(&c.adapter),
                        toml_str(&c.template),
                        toml_str(&c.output_sha256)
                    )
                })
                .collect();
            out.push_str(&format!("calls = [{}]\n", calls.join(", ")));
            push_kv_int(
                &mut out,
                "dropped",
                i64::try_from(ai.dropped).unwrap_or(i64::MAX),
            );
        }

        out
    }
}

fn check_top_level_keys(root: &toml::Table, kind: ProposalKind) -> Result<(), ProposalParseError> {
    let body_key = match kind {
        ProposalKind::NewSource | ProposalKind::NewVersion => "row",
        ProposalKind::Reclassify => "reclassify",
        ProposalKind::RegressionCase
        | ProposalKind::GapQuestion
        | ProposalKind::Note
        | ProposalKind::Decision => kind.as_str(),
    };

    for key in root.keys() {
        let allowed = matches!(
            key.as_str(),
            "proposal_version"
                | "state"
                | "kind"
                | "labels"
                | "writer"
                | "origin"
                | "candidate"
                | "extraction"
                | "source_choice"
                | "links"
                | "hints"
                | "ai"
        ) || key == body_key;
        if !allowed {
            return Err(ProposalParseError::UnknownKey(key.clone()));
        }
    }
    Ok(())
}

fn validate_opaque_table(table: &toml::Table, prefix: &str) -> Result<(), ProposalParseError> {
    for (k, v) in table {
        validate_opaque_value(v, &format!("{prefix}.{k}"))?;
    }
    Ok(())
}

fn validate_opaque_value(val: &toml::Value, key_path: &str) -> Result<(), ProposalParseError> {
    match val {
        toml::Value::Float(f) if !f.is_finite() => Err(ProposalParseError::InvalidValue {
            key: key_path.to_owned(),
            reason: "non-finite floats (`nan`, `inf`) are forbidden".to_owned(),
        }),
        toml::Value::Array(arr) => {
            for item in arr {
                validate_opaque_value(item, key_path)?;
            }
            Ok(())
        }
        toml::Value::Table(tbl) => validate_opaque_table(tbl, key_path),
        toml::Value::String(_)
        | toml::Value::Integer(_)
        | toml::Value::Float(_)
        | toml::Value::Boolean(_)
        | toml::Value::Datetime(_) => Ok(()),
    }
}

fn check_allowed_keys(
    table: &toml::Table,
    prefix: &str,
    allowed: &[&str],
) -> Result<(), ProposalParseError> {
    for key in table.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(ProposalParseError::UnknownKey(format!("{prefix}.{key}")));
        }
    }
    Ok(())
}

fn req_i64(table: &toml::Table, key: &'static str) -> Result<i64, ProposalParseError> {
    let val = table.get(key).ok_or(ProposalParseError::MissingKey(key))?;
    val.as_integer()
        .ok_or_else(|| ProposalParseError::InvalidValue {
            key: key.to_owned(),
            reason: "must be an integer".to_owned(),
        })
}

fn req_str<'a>(table: &'a toml::Table, key: &'static str) -> Result<&'a str, ProposalParseError> {
    let val = table.get(key).ok_or(ProposalParseError::MissingKey(key))?;
    val.as_str()
        .ok_or_else(|| ProposalParseError::InvalidValue {
            key: key.to_owned(),
            reason: "must be a string".to_owned(),
        })
}

fn req_nonempty_line<'a>(
    table: &'a toml::Table,
    prefix: &str,
    key: &'static str,
) -> Result<&'a str, ProposalParseError> {
    let val = table.get(key).ok_or(ProposalParseError::MissingKey(key))?;
    let s = val
        .as_str()
        .ok_or_else(|| ProposalParseError::InvalidValue {
            key: format!("{prefix}.{key}"),
            reason: "must be a string".to_owned(),
        })?;
    if s.is_empty() || s.contains(['\n', '\r']) {
        return Err(ProposalParseError::InvalidValue {
            key: format!("{prefix}.{key}"),
            reason: "must be a non-empty single-line string".to_owned(),
        });
    }
    Ok(s)
}

fn parse_label_array(
    table: &toml::Table,
    key: &'static str,
    require_non_empty: bool,
) -> Result<LabelSet, ProposalParseError> {
    let val = table.get(key).ok_or(ProposalParseError::MissingKey(key))?;
    let arr = val
        .as_array()
        .ok_or_else(|| ProposalParseError::InvalidValue {
            key: key.to_owned(),
            reason: "must be an array of label strings".to_owned(),
        })?;
    if require_non_empty && arr.is_empty() {
        return Err(ProposalParseError::InvalidValue {
            key: key.to_owned(),
            reason: "must contain at least one label".to_owned(),
        });
    }
    let mut ids = Vec::with_capacity(arr.len());
    for item in arr {
        let raw = item
            .as_str()
            .ok_or_else(|| ProposalParseError::InvalidValue {
                key: key.to_owned(),
                reason: "every label must be a string".to_owned(),
            })?;
        let id = LabelId::parse(raw).map_err(|_| ProposalParseError::InvalidValue {
            key: key.to_owned(),
            reason: format!("invalid label identifier `{raw}`"),
        })?;
        ids.push(id);
    }
    Ok(LabelSet::from_ids(ids))
}

fn parse_origin(val: &toml::Value, kind: ProposalKind) -> Result<Origin, ProposalParseError> {
    let tbl = val
        .as_table()
        .ok_or_else(|| ProposalParseError::InvalidValue {
            key: "origin".to_owned(),
            reason: "must be a table".to_owned(),
        })?;
    check_allowed_keys(
        tbl,
        "origin",
        &[
            "system",
            "stream",
            "key",
            "as_of",
            "retrieved",
            "base",
            "series",
            "workbook",
        ],
    )?;

    let system = req_nonempty_line(tbl, "origin", "system")?.to_owned();
    let stream = req_nonempty_line(tbl, "origin", "stream")?.to_owned();
    let key = req_nonempty_line(tbl, "origin", "key")?.to_owned();
    let as_of_raw = req_nonempty_line(tbl, "origin", "as_of")?;
    let as_of = PartialDate::parse(as_of_raw).map_err(|_| ProposalParseError::InvalidValue {
        key: "origin.as_of".to_owned(),
        reason: format!("invalid partial date `{as_of_raw}`"),
    })?;
    let retrieved_raw = req_nonempty_line(tbl, "origin", "retrieved")?;
    let retrieved =
        CivilDate::parse(retrieved_raw).map_err(|_| ProposalParseError::InvalidValue {
            key: "origin.retrieved".to_owned(),
            reason: format!("invalid civil date `{retrieved_raw}`"),
        })?;

    let base = if let Some(bval) = tbl.get("base") {
        if kind != ProposalKind::NewVersion {
            return Err(ProposalParseError::InvalidValue {
                key: "origin.base".to_owned(),
                reason: "`origin.base` is valid only on `new_version` proposals".to_owned(),
            });
        }
        let braw = bval
            .as_str()
            .ok_or_else(|| ProposalParseError::InvalidValue {
                key: "origin.base".to_owned(),
                reason: "must be a version id string".to_owned(),
            })?;
        Some(
            VersionId::parse(braw).map_err(|_| ProposalParseError::InvalidValue {
                key: "origin.base".to_owned(),
                reason: format!("invalid version id `{braw}`"),
            })?,
        )
    } else {
        None
    };

    let series = if let Some(sval) = tbl.get("series") {
        let sraw = sval
            .as_str()
            .ok_or_else(|| ProposalParseError::InvalidValue {
                key: "origin.series".to_owned(),
                reason: "must be a string".to_owned(),
            })?;
        if !vocab::stream_id_form(sraw) {
            return Err(ProposalParseError::InvalidValue {
                key: "origin.series".to_owned(),
                reason: format!("`{sraw}` does not match stream_id_form"),
            });
        }
        Some(sraw.to_owned())
    } else {
        None
    };

    let workbook = if let Some(wval) = tbl.get("workbook") {
        let flag = wval
            .as_bool()
            .ok_or_else(|| ProposalParseError::InvalidValue {
                key: "origin.workbook".to_owned(),
                reason: "must be a boolean".to_owned(),
            })?;
        if !flag {
            return Err(ProposalParseError::InvalidValue {
                key: "origin.workbook".to_owned(),
                reason: "`workbook = false` is forbidden; omit the key instead".to_owned(),
            });
        }
        true
    } else {
        false
    };

    Ok(Origin {
        system,
        stream,
        key,
        as_of,
        retrieved,
        base,
        series,
        workbook,
    })
}

fn parse_candidate(val: &toml::Value) -> Result<CandidateMeta, ProposalParseError> {
    let tbl = val
        .as_table()
        .ok_or_else(|| ProposalParseError::InvalidValue {
            key: "candidate".to_owned(),
            reason: "must be a table".to_owned(),
        })?;
    check_allowed_keys(tbl, "candidate", &["file", "sha256", "bytes"])?;

    let file = req_nonempty_line(tbl, "candidate", "file")?;
    if file.starts_with('/')
        || file.contains('\0')
        || file.split(['/', '\\']).any(|seg| seg == "..")
    {
        return Err(ProposalParseError::InvalidValue {
            key: "candidate.file".to_owned(),
            reason: "must be a safe relative inbox path".to_owned(),
        });
    }

    let sha256 = req_nonempty_line(tbl, "candidate", "sha256")?;
    if !is_hex64(sha256) {
        return Err(ProposalParseError::InvalidValue {
            key: "candidate.sha256".to_owned(),
            reason: "must be 64 lowercase hexadecimal characters".to_owned(),
        });
    }

    let bytes_raw = req_i64(tbl, "bytes")?;
    let bytes = u64::try_from(bytes_raw).map_err(|_| ProposalParseError::InvalidValue {
        key: "candidate.bytes".to_owned(),
        reason: "must be a non-negative integer".to_owned(),
    })?;

    Ok(CandidateMeta {
        file: file.to_owned(),
        sha256: sha256.to_owned(),
        bytes,
    })
}

fn parse_extraction(val: &toml::Value) -> Result<ExtractionMeta, ProposalParseError> {
    let tbl = val
        .as_table()
        .ok_or_else(|| ProposalParseError::InvalidValue {
            key: "extraction".to_owned(),
            reason: "must be a table".to_owned(),
        })?;
    check_allowed_keys(
        tbl,
        "extraction",
        &["adapter", "method", "pages", "thin_pages"],
    )?;

    let adapter = req_nonempty_line(tbl, "extraction", "adapter")?.to_owned();
    let method_raw = req_nonempty_line(tbl, "extraction", "method")?;
    let _parsed_method = ExtractionMethod::parse(method_raw);

    let pages_raw = req_i64(tbl, "pages")?;
    let pages = u32::try_from(pages_raw).map_err(|_| ProposalParseError::InvalidValue {
        key: "extraction.pages".to_owned(),
        reason: "must be a non-negative 32-bit integer".to_owned(),
    })?;

    let thin_val = tbl
        .get("thin_pages")
        .ok_or(ProposalParseError::MissingKey("thin_pages"))?;
    let thin_arr = thin_val
        .as_array()
        .ok_or_else(|| ProposalParseError::InvalidValue {
            key: "extraction.thin_pages".to_owned(),
            reason: "must be an array of 1-based page numbers".to_owned(),
        })?;

    let mut thin_pages = Vec::with_capacity(thin_arr.len());
    let mut prev = 0u32;
    for elem in thin_arr {
        let n_i64 = elem
            .as_integer()
            .ok_or_else(|| ProposalParseError::InvalidValue {
                key: "extraction.thin_pages".to_owned(),
                reason: "elements must be integers".to_owned(),
            })?;
        let n = u32::try_from(n_i64).map_err(|_| ProposalParseError::InvalidValue {
            key: "extraction.thin_pages".to_owned(),
            reason: "page number out of range".to_owned(),
        })?;
        if n == 0 || n > pages || n <= prev {
            return Err(ProposalParseError::InvalidValue {
                key: "extraction.thin_pages".to_owned(),
                reason: "thin_pages must be strictly ascending within 1..=pages".to_owned(),
            });
        }
        prev = n;
        thin_pages.push(n);
    }

    Ok(ExtractionMeta {
        adapter,
        method: method_raw.to_owned(),
        pages,
        thin_pages,
    })
}

fn parse_source_choice(val: &toml::Value) -> Result<SourceChoice, ProposalParseError> {
    let tbl = val
        .as_table()
        .ok_or_else(|| ProposalParseError::InvalidValue {
            key: "source_choice".to_owned(),
            reason: "must be a table".to_owned(),
        })?;
    check_allowed_keys(tbl, "source_choice", &["kind", "id"])?;
    let kind_raw = req_nonempty_line(tbl, "source_choice", "kind")?;
    match kind_raw {
        "unresolved" => {
            if tbl.contains_key("id") {
                return Err(ProposalParseError::InvalidValue {
                    key: "source_choice.id".to_owned(),
                    reason: "`id` is forbidden when `kind = \"unresolved\"`".to_owned(),
                });
            }
            Ok(SourceChoice::Unresolved)
        }
        "new" => {
            if tbl.contains_key("id") {
                return Err(ProposalParseError::InvalidValue {
                    key: "source_choice.id".to_owned(),
                    reason: "`id` is forbidden when `kind = \"new\"`".to_owned(),
                });
            }
            Ok(SourceChoice::New)
        }
        "existing" => {
            let id_raw = req_nonempty_line(tbl, "source_choice", "id")?;
            let id = SourceId::parse(id_raw).map_err(|_| ProposalParseError::InvalidValue {
                key: "source_choice.id".to_owned(),
                reason: format!("invalid source id `{id_raw}`"),
            })?;
            Ok(SourceChoice::Existing(id))
        }
        other => Err(ProposalParseError::InvalidValue {
            key: "source_choice.kind".to_owned(),
            reason: format!("unknown source_choice kind `{other}`"),
        }),
    }
}

fn parse_row_body(val: &toml::Value) -> Result<RowBody, ProposalParseError> {
    let tbl = val
        .as_table()
        .ok_or_else(|| ProposalParseError::InvalidValue {
            key: "row".to_owned(),
            reason: "must be a table".to_owned(),
        })?;
    let mut row = BTreeMap::new();
    for (field_name, entry_val) in tbl {
        let Some(field) = SuggestField::parse(field_name) else {
            return Err(ProposalParseError::UnknownKey(format!("row.{field_name}")));
        };
        let entry_tbl = entry_val
            .as_table()
            .ok_or_else(|| ProposalParseError::InvalidValue {
                key: format!("row.{field_name}"),
                reason: "each row field must be a table `{ value, by, because }`".to_owned(),
            })?;
        let prefix = format!("row.{field_name}");
        check_allowed_keys(entry_tbl, &prefix, &["value", "by", "because", "rule"])?;

        let value = req_str(entry_tbl, "value")?;
        if value.contains(['\n', '\r']) || value.chars().count() > MAX_TEXT_CHARS {
            return Err(ProposalParseError::InvalidValue {
                key: format!("{prefix}.value"),
                reason: "invalid single-line value or exceeds limit".to_owned(),
            });
        }

        let by_raw = req_str(entry_tbl, "by")?;
        let by = By::parse(by_raw).map_err(|_| ProposalParseError::InvalidValue {
            key: format!("{prefix}.by"),
            reason: format!("invalid attribution `{by_raw}`"),
        })?;

        let because = req_str(entry_tbl, "because")?;
        if because.contains(['\n', '\r']) {
            return Err(ProposalParseError::InvalidValue {
                key: format!("{prefix}.because"),
                reason: "line break in `because`".to_owned(),
            });
        }

        let rule = if let Some(rval) = entry_tbl.get("rule") {
            Some(parse_rule_ref(rval, &format!("{prefix}.rule"))?)
        } else {
            None
        };

        row.insert(
            field,
            Judged {
                value: value.to_owned(),
                by,
                because: because.to_owned(),
                rule,
            },
        );
    }
    Ok(row)
}

fn parse_rule_ref(val: &toml::Value, prefix: &str) -> Result<RuleRef, ProposalParseError> {
    let tbl = val
        .as_table()
        .ok_or_else(|| ProposalParseError::InvalidValue {
            key: prefix.to_owned(),
            reason: "must be a table `{ id, kind }`".to_owned(),
        })?;
    check_allowed_keys(tbl, prefix, &["id", "kind"])?;
    let id = req_nonempty_line(tbl, prefix, "id")?.to_owned();
    let kind_raw = req_nonempty_line(tbl, prefix, "kind")?;
    let kind = RuleKind::parse(kind_raw).ok_or_else(|| ProposalParseError::InvalidValue {
        key: format!("{prefix}.kind"),
        reason: format!("unknown rule kind `{kind_raw}`"),
    })?;
    Ok(RuleRef { id, kind })
}

fn parse_reclassify(val: &toml::Value) -> Result<ProposalBody, ProposalParseError> {
    let tbl = val
        .as_table()
        .ok_or_else(|| ProposalParseError::InvalidValue {
            key: "reclassify".to_owned(),
            reason: "must be a table".to_owned(),
        })?;
    check_allowed_keys(tbl, "reclassify", &["source_version", "doc_kind", "series"])?;

    let ver_raw = req_nonempty_line(tbl, "reclassify", "source_version")?;
    let source_version =
        VersionId::parse(ver_raw).map_err(|_| ProposalParseError::InvalidValue {
            key: "reclassify.source_version".to_owned(),
            reason: format!("invalid version id `{ver_raw}`"),
        })?;

    let doc_kind = if let Some(dval) = tbl.get("doc_kind") {
        let raw = dval
            .as_str()
            .ok_or_else(|| ProposalParseError::InvalidValue {
                key: "reclassify.doc_kind".to_owned(),
                reason: "must be a string".to_owned(),
            })?;
        if raw.is_empty() {
            None
        } else {
            Some(
                DocKind::parse(raw).map_err(|_| ProposalParseError::InvalidValue {
                    key: "reclassify.doc_kind".to_owned(),
                    reason: format!("unknown doc_kind `{raw}`"),
                })?,
            )
        }
    } else {
        None
    };

    let series = if let Some(sval) = tbl.get("series") {
        let raw = sval
            .as_str()
            .ok_or_else(|| ProposalParseError::InvalidValue {
                key: "reclassify.series".to_owned(),
                reason: "must be a string".to_owned(),
            })?;
        if raw.is_empty() {
            None
        } else if !vocab::stream_id_form(raw) {
            return Err(ProposalParseError::InvalidValue {
                key: "reclassify.series".to_owned(),
                reason: format!("`{raw}` does not match stream_id_form"),
            });
        } else {
            Some(raw.to_owned())
        }
    } else {
        None
    };

    if doc_kind.is_none() && series.is_none() {
        return Err(ProposalParseError::ReclassifyChangesNothing);
    }

    Ok(ProposalBody::Reclassify {
        source_version,
        doc_kind,
        series,
    })
}

fn parse_links(val: &toml::Value) -> Result<Links, ProposalParseError> {
    let tbl = val
        .as_table()
        .ok_or_else(|| ProposalParseError::InvalidValue {
            key: "links".to_owned(),
            reason: "must be a table".to_owned(),
        })?;
    check_allowed_keys(tbl, "links", &["version_of", "related", "answers"])?;

    let mut links = Links::default();
    if let Some(vo) = tbl.get("version_of") {
        let arr = vo
            .as_array()
            .ok_or_else(|| ProposalParseError::InvalidValue {
                key: "links.version_of".to_owned(),
                reason: "must be an array of source ids".to_owned(),
            })?;
        for item in arr {
            let s = item
                .as_str()
                .ok_or_else(|| ProposalParseError::InvalidValue {
                    key: "links.version_of".to_owned(),
                    reason: "entries must be source id strings".to_owned(),
                })?;
            let id = SourceId::parse(s).map_err(|_| ProposalParseError::InvalidValue {
                key: "links.version_of".to_owned(),
                reason: format!("invalid source id `{s}`"),
            })?;
            links.version_of.push(id);
        }
    }

    if let Some(rel) = tbl.get("related") {
        let arr = rel
            .as_array()
            .ok_or_else(|| ProposalParseError::InvalidValue {
                key: "links.related".to_owned(),
                reason: "must be an array".to_owned(),
            })?;
        for item in arr {
            if let Some(s) = item.as_str() {
                if s.is_empty() {
                    return Err(ProposalParseError::InvalidValue {
                        key: "links.related".to_owned(),
                        reason: "target must be non-empty".to_owned(),
                    });
                }
                links.related.push(RelatedLink {
                    target: s.to_owned(),
                    score: None,
                });
            } else if let Some(rt) = item.as_table() {
                check_allowed_keys(rt, "links.related", &["target", "score"])?;
                let target = req_nonempty_line(rt, "links.related", "target")?.to_owned();
                let score = if let Some(sv) = rt.get("score") {
                    let s_i64 =
                        sv.as_integer()
                            .ok_or_else(|| ProposalParseError::InvalidValue {
                                key: "links.related.score".to_owned(),
                                reason: "score must be a non-negative integer".to_owned(),
                            })?;
                    Some(
                        u32::try_from(s_i64).map_err(|_| ProposalParseError::InvalidValue {
                            key: "links.related.score".to_owned(),
                            reason: "score out of u32 range".to_owned(),
                        })?,
                    )
                } else {
                    None
                };
                links.related.push(RelatedLink { target, score });
            } else {
                return Err(ProposalParseError::InvalidValue {
                    key: "links.related".to_owned(),
                    reason: "entries must be string or `{ target, score }`".to_owned(),
                });
            }
        }
    }

    if let Some(ans) = tbl.get("answers") {
        let arr = ans
            .as_array()
            .ok_or_else(|| ProposalParseError::InvalidValue {
                key: "links.answers".to_owned(),
                reason: "must be an array of strings".to_owned(),
            })?;
        for item in arr {
            let s = item
                .as_str()
                .ok_or_else(|| ProposalParseError::InvalidValue {
                    key: "links.answers".to_owned(),
                    reason: "entries must be strings".to_owned(),
                })?;
            if s.is_empty() {
                return Err(ProposalParseError::InvalidValue {
                    key: "links.answers".to_owned(),
                    reason: "entries must be non-empty".to_owned(),
                });
            }
            links.answers.push(s.to_owned());
        }
    }

    Ok(links)
}

fn parse_hints(val: &toml::Value) -> Result<Vec<ProposalHint>, ProposalParseError> {
    let arr = val
        .as_array()
        .ok_or_else(|| ProposalParseError::InvalidValue {
            key: "hints".to_owned(),
            reason: "must be an array of tables".to_owned(),
        })?;
    let mut hints = Vec::with_capacity(arr.len());
    for item in arr {
        let tbl = item
            .as_table()
            .ok_or_else(|| ProposalParseError::InvalidValue {
                key: "hints".to_owned(),
                reason: "each hint must be a table".to_owned(),
            })?;
        check_allowed_keys(tbl, "hints", &["label", "by", "because"])?;
        let raw_label = req_nonempty_line(tbl, "hints", "label")?;
        let label = LabelId::parse(raw_label).map_err(|_| ProposalParseError::InvalidValue {
            key: "hints.label".to_owned(),
            reason: format!("invalid label `{raw_label}`"),
        })?;
        if label.is_builtin() {
            return Err(ProposalParseError::InvalidValue {
                key: "hints.label".to_owned(),
                reason: "built-in labels cannot be used as hints".to_owned(),
            });
        }
        let by_raw = req_nonempty_line(tbl, "hints", "by")?;
        let by = By::parse(by_raw).map_err(|_| ProposalParseError::InvalidValue {
            key: "hints.by".to_owned(),
            reason: format!("invalid attribution `{by_raw}`"),
        })?;
        let because = req_str(tbl, "because")?.to_owned();
        hints.push(ProposalHint { label, by, because });
    }
    Ok(hints)
}

fn parse_ai(val: &toml::Value) -> Result<AiRecord, ProposalParseError> {
    let tbl = val
        .as_table()
        .ok_or_else(|| ProposalParseError::InvalidValue {
            key: "ai".to_owned(),
            reason: "must be a table".to_owned(),
        })?;
    check_allowed_keys(tbl, "ai", &["request_labels", "inputs", "calls", "dropped"])?;

    let request_labels = if tbl.contains_key("request_labels") {
        parse_label_array(tbl, "request_labels", false)?
    } else {
        LabelSet::unlabelled()
    };

    let mut inputs = Vec::new();
    if let Some(ivals) = tbl.get("inputs") {
        let arr = ivals
            .as_array()
            .ok_or_else(|| ProposalParseError::InvalidValue {
                key: "ai.inputs".to_owned(),
                reason: "must be an array".to_owned(),
            })?;
        for item in arr {
            let itbl = item
                .as_table()
                .ok_or_else(|| ProposalParseError::InvalidValue {
                    key: "ai.inputs".to_owned(),
                    reason: "each input must be `{ store, id }`".to_owned(),
                })?;
            check_allowed_keys(itbl, "ai.inputs", &["store", "id"])?;
            let store_raw = req_nonempty_line(itbl, "ai.inputs", "store")?;
            let store = StoreId::from_root_name(store_raw).ok_or_else(|| {
                ProposalParseError::InvalidValue {
                    key: "ai.inputs.store".to_owned(),
                    reason: format!("unknown store `{store_raw}`"),
                }
            })?;
            let id = req_nonempty_line(itbl, "ai.inputs", "id")?.to_owned();
            inputs.push(ItemRef::new(store, id));
        }
    }

    let calls_val = tbl
        .get("calls")
        .ok_or(ProposalParseError::MissingKey("calls"))?;
    let calls_arr = calls_val
        .as_array()
        .ok_or_else(|| ProposalParseError::InvalidValue {
            key: "ai.calls".to_owned(),
            reason: "must be an array".to_owned(),
        })?;
    let mut calls = Vec::with_capacity(calls_arr.len());
    for item in calls_arr {
        let ctbl = item
            .as_table()
            .ok_or_else(|| ProposalParseError::InvalidValue {
                key: "ai.calls".to_owned(),
                reason: "each call must be a table".to_owned(),
            })?;
        check_allowed_keys(
            ctbl,
            "ai.calls",
            &["seq", "adapter", "template", "output_sha256"],
        )?;
        let seq_raw = req_i64(ctbl, "seq")?;
        let seq = u64::try_from(seq_raw).map_err(|_| ProposalParseError::InvalidValue {
            key: "ai.calls.seq".to_owned(),
            reason: "seq must be a non-negative integer".to_owned(),
        })?;
        let adapter = req_nonempty_line(ctbl, "ai.calls", "adapter")?.to_owned();
        let template = req_nonempty_line(ctbl, "ai.calls", "template")?.to_owned();
        let output_sha256 = req_nonempty_line(ctbl, "ai.calls", "output_sha256")?;
        if !is_hex64(output_sha256) {
            return Err(ProposalParseError::InvalidValue {
                key: "ai.calls.output_sha256".to_owned(),
                reason: "must be 64 lowercase hexadecimal characters".to_owned(),
            });
        }
        calls.push(AiCall {
            seq,
            adapter,
            template,
            output_sha256: output_sha256.to_owned(),
        });
    }

    let dropped_raw = req_i64(tbl, "dropped")?;
    let dropped = usize::try_from(dropped_raw).map_err(|_| ProposalParseError::InvalidValue {
        key: "ai.dropped".to_owned(),
        reason: "must be a non-negative integer".to_owned(),
    })?;

    Ok(AiRecord {
        request_labels,
        inputs,
        calls,
        dropped,
    })
}

fn toml_str(s: &str) -> String {
    toml::Value::String(s.to_owned()).to_string()
}

fn push_kv_str(out: &mut String, key: &str, val: &str) {
    out.push_str(key);
    out.push_str(" = ");
    out.push_str(&toml_str(val));
    out.push('\n');
}

fn push_kv_int(out: &mut String, key: &str, val: i64) {
    out.push_str(&format!("{key} = {val}\n"));
}

fn push_kv_u64(out: &mut String, key: &str, val: u64) {
    out.push_str(&format!("{key} = {val}\n"));
}

fn push_kv_str_array(out: &mut String, key: &str, vals: &[&str]) {
    let rendered: Vec<String> = vals.iter().map(|v| toml_str(v)).collect();
    out.push_str(&format!("{key} = [{}]\n", rendered.join(", ")));
}

fn push_kv_u32_array(out: &mut String, key: &str, vals: &[u32]) {
    let rendered: Vec<String> = vals.iter().map(ToString::to_string).collect();
    out.push_str(&format!("{key} = [{}]\n", rendered.join(", ")));
}

fn render_table_entries(out: &mut String, table: &toml::Table) {
    for (k, v) in table {
        let key_repr = if is_bare_toml_key(k) {
            k.clone()
        } else {
            toml_str(k)
        };
        out.push_str(&key_repr);
        out.push_str(" = ");
        out.push_str(&render_toml_value(v));
        out.push('\n');
    }
}

fn render_toml_value(val: &toml::Value) -> String {
    match val {
        toml::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(render_toml_value).collect();
            format!("[{}]", items.join(", "))
        }
        toml::Value::Table(tbl) => {
            let items: Vec<String> = tbl
                .iter()
                .map(|(k, v)| {
                    let key_repr = if is_bare_toml_key(k) {
                        k.clone()
                    } else {
                        toml_str(k)
                    };
                    format!("{key_repr} = {}", render_toml_value(v))
                })
                .collect();
            format!("{{ {} }}", items.join(", "))
        }
        scalar => scalar.to_string(),
    }
}

fn is_bare_toml_key(k: &str) -> bool {
    !k.is_empty()
        && k.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
