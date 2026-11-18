//! Suggestions, correlations, attribution, support validation, and classifier/correlator traits.

use std::fmt;

use crate::check::Code;
use crate::labels::LabelTable;
use crate::ports::HandoffTicket;
use crate::ports::index::IndexPort;
use crate::quote;

use super::candidate::{Candidate, LabelHint, SuggestField, Vocab};
use super::view::CorpusView;

/// Kind of automated matching rule that emitted a suggestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RuleKind {
    /// Phrase match rule (`"phrase"`).
    Phrase,
    /// Publisher match rule (`"publisher"`).
    Publisher,
    /// Document/source identifier shape rule (`"id_shape"`).
    IdShape,
}

impl RuleKind {
    /// Every [`RuleKind`] variant.
    pub const ALL: [Self; 3] = [Self::Phrase, Self::Publisher, Self::IdShape];

    /// Lowercase serialized name (`"phrase"`, `"publisher"`, `"id_shape"`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Phrase => "phrase",
            Self::Publisher => "publisher",
            Self::IdShape => "id_shape",
        }
    }

    /// Parses one of `"phrase"`, `"publisher"`, or `"id_shape"`.
    #[must_use]
    pub fn parse(input: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == input)
    }
}

impl fmt::Display for RuleKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Provenance identifying which automated rule produced a suggestion.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuleRef {
    /// Rule identifier string from `rules.toml`.
    pub id: String,
    /// Closed kind of rule.
    pub kind: RuleKind,
}

/// Error returned when [`By::parse`] rejects a string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByParseError(pub String);

impl fmt::Display for ByParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid `by` attribution: {}", self.0)
    }
}

impl std::error::Error for ByParseError {}

/// Attribution naming who or what produced a field value, link, or label hint.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum By {
    /// Deterministic rule classifier (`"rules@1"`).
    Rules,
    /// Deterministic correlator (`"exact-ids+bm25@1"`).
    Correlator,
    /// Model adapter (`"model:<adapter>:<model id>"`).
    Model {
        /// Model adapter identifier (non-empty).
        adapter: String,
        /// Model identifier returned or configured for the call (non-empty).
        model: String,
    },
    /// Human reviewer or editor (`"person"`).
    Person,
    /// Stream origin declaration copied by `sync` (`"origin"`).
    Origin,
}

impl By {
    /// Formats `self` into its canonical string spelling.
    #[must_use]
    pub fn format(&self) -> String {
        match self {
            Self::Rules => "rules@1".to_owned(),
            Self::Correlator => "exact-ids+bm25@1".to_owned(),
            Self::Model { adapter, model } => format!("model:{adapter}:{model}"),
            Self::Person => "person".to_owned(),
            Self::Origin => "origin".to_owned(),
        }
    }

    /// Parses one of `rules@1`, `exact-ids+bm25@1`, `person`, `origin`, or `model:<adapter>:<model id>`.
    ///
    /// # Errors
    ///
    /// Returns [`ByParseError`] if `input` does not match any valid spelling.
    pub fn parse(input: &str) -> Result<Self, ByParseError> {
        match input {
            "rules@1" => return Ok(Self::Rules),
            "exact-ids+bm25@1" => return Ok(Self::Correlator),
            "person" => return Ok(Self::Person),
            "origin" => return Ok(Self::Origin),
            _ => {}
        }
        let Some(rest) = input.strip_prefix("model:") else {
            return Err(ByParseError(input.to_owned()));
        };
        let Some((adapter, model)) = rest.split_once(':') else {
            return Err(ByParseError(input.to_owned()));
        };
        if !is_valid_token(adapter) || !is_valid_token(model) {
            return Err(ByParseError(input.to_owned()));
        }
        Ok(Self::Model {
            adapter: adapter.to_owned(),
            model: model.to_owned(),
        })
    }
}

impl fmt::Display for By {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format())
    }
}

fn is_valid_token(s: &str) -> bool {
    !s.is_empty() && !s.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Evidence backing a [`Suggestion`], [`Link`], or [`LabelHint`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Support {
    /// Quoted text verified against a page of the candidate extraction.
    Quote {
        /// 1-based page number.
        page: u32,
        /// Verbatim or normalised-matching excerpt from `page`.
        quote: String,
    },
    /// Reference to an existing unit/row identifier and search score (`0` for exact matches).
    Id {
        /// Identifier of the matching row or unit.
        id: String,
        /// Search score (`0` for an exact hash/ID match).
        score: u32,
    },
    /// Stream origin declaration copied from `systems.toml`.
    Origin {
        /// Configured system id matching [`Candidate::origin`].
        system: String,
        /// Configured stream id matching [`Candidate::origin`].
        stream: String,
    },
}

impl Support {
    /// Renders `self` into a human-readable support string for proposal summaries.
    #[must_use]
    pub fn as_text(&self) -> String {
        match self {
            Self::Quote { page, quote } => format!("p.{page}: \"{quote}\""),
            Self::Id { id, score } => {
                if *score == 0 {
                    id.clone()
                } else {
                    format!("{id} (score {score})")
                }
            }
            Self::Origin { system, stream } => format!("origin:{system}/{stream}"),
        }
    }
}

impl fmt::Display for Support {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_text())
    }
}

/// One proposed field value returned by a [`Classifier`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// Target suggestible field.
    pub field: SuggestField,
    /// Suggested value string.
    pub value: String,
    /// Attribution naming the producer.
    pub by: By,
    /// Non-empty list of supporting evidence items.
    pub support: Vec<Support>,
    /// Optional rule reference (omitted for title fallback and human edits).
    pub rule: Option<RuleRef>,
}

/// Relationship kind between a candidate and an existing Corpus item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LinkKind {
    /// Candidate appears to be a new version of an existing source.
    VersionOf,
    /// Candidate references or relates to an existing item or unit.
    Related,
    /// Candidate appears to answer an open Corpus question.
    Answers,
    /// Candidate matches the exact SHA-256 of an existing source version.
    Present,
}

/// One correlation link produced by a [`Correlator`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// Relationship kind.
    pub kind: LinkKind,
    /// Target identifier in the view or Corpus.
    pub target: String,
    /// Correlator attribution.
    pub by: By,
    /// Non-empty supporting evidence (for [`LinkKind::Present`], `Support::Id { id, score: 0 }`).
    pub support: Vec<Support>,
}

/// Reason [`validate`] or [`validate_hint`] rejected a suggestion or label hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    /// Field is not offered by the store [`Vocab`].
    FieldNotOffered,
    /// Suggested value violates the vocabulary or column cell rule.
    InvalidValue,
    /// Suggestion or hint provided no supporting evidence.
    MissingSupport,
    /// A `Quote` support did not match text on the named candidate page.
    QuoteNotFound,
    /// `by = origin` or `Origin` support was supplied without matching candidate origin.
    OriginNotAllowed,
    /// `by = origin` lacked an `Origin` support matching the candidate's system and stream.
    MissingOriginSupport,
    /// Label hint named a built-in label (`public`, `unlabelled`, `marked`, `owner-supplied`).
    BuiltinLabelHint,
    /// Label hint named a label not present in the [`LabelTable`].
    UnknownLabelHint,
}

/// Typed stage failure returned by [`Classifier`] and [`Correlator`] adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageError {
    /// Stage deferred execution to a Handoff pack.
    Deferred(HandoffTicket),
    /// Stage failed with an FB diagnostic code and adapter-prefixed context.
    Failed(Code),
}

impl fmt::Display for StageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Deferred(ticket) => {
                write!(f, "deferred to handoff {}", ticket.id)
            }
            Self::Failed(code) => write!(f, "{code}"),
        }
    }
}

impl std::error::Error for StageError {}

/// Checks that one [`Support`] evidence item holds against `candidate`.
fn check_support(support: &Support, candidate: &Candidate, by: &By) -> Result<(), DropReason> {
    match support {
        Support::Quote { page, quote: text } => {
            if *page == 0 {
                return Err(DropReason::QuoteNotFound);
            }
            let Some(ext) = candidate.extraction.as_ref() else {
                return Err(DropReason::QuoteNotFound);
            };
            let Some(page_entry) = ext.pages.iter().find(|p| p.number == *page) else {
                return Err(DropReason::QuoteNotFound);
            };
            if quote::find(text, &[page_entry.text.as_str()], 1..=1).is_none() {
                return Err(DropReason::QuoteNotFound);
            }
            Ok(())
        }
        Support::Id { id, .. } => {
            if id.is_empty() {
                Err(DropReason::MissingSupport)
            } else {
                Ok(())
            }
        }
        Support::Origin { system, stream } => {
            if *by != By::Origin {
                return Err(DropReason::OriginNotAllowed);
            }
            let Some(origin) = candidate.origin.as_ref() else {
                return Err(DropReason::OriginNotAllowed);
            };
            if origin.system == *system && origin.stream == *stream {
                Ok(())
            } else {
                Err(DropReason::OriginNotAllowed)
            }
        }
    }
}

/// Validates a [`Suggestion`] against `candidate` and `vocab`.
///
/// A suggestion is kept only when:
/// - `vocab.offers(suggestion.field)` is `true`,
/// - `vocab.accepts_value(suggestion.field, &suggestion.value)` passes,
/// - `suggestion.support` is non-empty and every `Quote` support matches its page via
///   [`quote::find`], and
/// - `by = origin` carries a `Support::Origin` matching `candidate.origin` (`system` and `stream`).
///
/// # Errors
///
/// Returns [`DropReason`] describing the first failed check.
pub fn validate(
    suggestion: &Suggestion,
    candidate: &Candidate,
    vocab: &Vocab,
) -> Result<(), DropReason> {
    if !vocab.offers(suggestion.field) {
        return Err(DropReason::FieldNotOffered);
    }
    if vocab
        .accepts_value(suggestion.field, &suggestion.value)
        .is_err()
    {
        return Err(DropReason::InvalidValue);
    }
    if suggestion.support.is_empty() {
        return Err(DropReason::MissingSupport);
    }

    if suggestion.by == By::Origin {
        let Some(origin) = candidate.origin.as_ref() else {
            return Err(DropReason::OriginNotAllowed);
        };
        let has_matching_origin = suggestion.support.iter().any(|s| {
            matches!(
                s,
                Support::Origin { system, stream }
                    if system == &origin.system && stream == &origin.stream
            )
        });
        if !has_matching_origin {
            return Err(DropReason::MissingOriginSupport);
        }
    }

    for support in &suggestion.support {
        check_support(support, candidate, &suggestion.by)?;
    }
    Ok(())
}

/// Validates a [`LabelHint`] against `candidate` and `table`.
///
/// Built-in labels (`public`, `unlabelled`, `marked`, `owner-supplied`) and unknown labels
/// outside `table` are rejected, and the supporting evidence must hold on `candidate`.
///
/// # Errors
///
/// Returns [`DropReason`] if the label is invalid or its support does not hold.
pub fn validate_hint(
    hint: &LabelHint,
    candidate: &Candidate,
    table: &LabelTable,
) -> Result<(), DropReason> {
    if hint.label.is_builtin() {
        return Err(DropReason::BuiltinLabelHint);
    }
    if !table.is_known(&hint.label) {
        return Err(DropReason::UnknownLabelHint);
    }
    check_support(&hint.support, candidate, &hint.by)
}

/// Complete output of one [`Classifier`] run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Suggestions {
    /// Proposed field values.
    pub fields: Vec<Suggestion>,
    /// Optional label hints for human confirmation.
    pub hints: Vec<LabelHint>,
}

/// Architectural port for candidate field classifiers (`AD-01`).
pub trait Classifier {
    /// Registered adapter identifier (`"rules@1"`, ...).
    fn id(&self) -> &'static str;

    /// Suggests metadata fields and optional label hints for `candidate` using `vocab`.
    ///
    /// # Errors
    ///
    /// Returns [`StageError`] on deferral or failure.
    fn classify(&self, candidate: &Candidate, vocab: &Vocab) -> Result<Suggestions, StageError>;
}

/// Architectural port for candidate correlators (`AD-01`).
pub trait Correlator {
    /// Registered adapter identifier (`"exact-ids+bm25@1"`, ...).
    fn id(&self) -> &'static str;

    /// Correlates `candidate` against `view` and `index`.
    ///
    /// # Errors
    ///
    /// Returns [`StageError`] on deferral or failure.
    fn correlate(
        &self,
        candidate: &Candidate,
        view: &CorpusView,
        index: &dyn IndexPort,
    ) -> Result<Vec<Link>, StageError>;
}

/// Checks whether a classifier-produced suggestion illegally claims [`By::Origin`].
#[must_use]
pub const fn is_classifier_origin_violation(suggestion: &Suggestion) -> bool {
    matches!(suggestion.by, By::Origin)
}

/// Builds `Vec::new` or validates link invariants cleanly.
#[must_use]
pub fn is_valid_present_link(link: &Link) -> bool {
    link.kind == LinkKind::Present
        && link.support.len() == 1
        && matches!(&link.support[0], Support::Id { id, score: 0 } if id == &link.target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn by_parses_and_formats_all_forms() {
        for raw in [
            "rules@1",
            "exact-ids+bm25@1",
            "person",
            "origin",
            "model:local@1:test-model",
        ] {
            let parsed = By::parse(raw).expect("valid by");
            assert_eq!(parsed.format(), raw);
        }
        assert!(By::parse("model::bad").is_err());
        assert!(By::parse("model:bad:").is_err());
        assert!(By::parse("").is_err());
    }
}
