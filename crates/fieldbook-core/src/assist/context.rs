//! Pure context selection types, argument parsers, and Unicode-scalar request budgets
//! (`handoff-context` steps 1, 12, 14).
//!
//! **Responsibility.** Defines the serializable data structures shared by `fieldbook context`,
//! the Assistant page (`assist.js`), and the Bridge (`POST /api/context`): [`Scope`],
//! [`Include`], [`Origin`], [`Why`], [`ContextItem`], [`NamedPack`], [`ContextIds`],
//! [`RequestBudget`], [`Budget`], and [`UsageError`].
//!
//! **Rules.** Pure: no I/O, no clock, no environment, no `serde_json`. Every character budget
//! counts Unicode scalar values (`str::chars().count()`), never UTF-8 byte lengths.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use super::pack::ContextPack;

/// Typed error returned when parsing `--scope`, `--include`, or `--budget` arguments fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageError {
    message: String,
}

impl UsageError {
    /// Constructs a new [`UsageError`] with a plain diagnostic message.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Returns the error message text.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for UsageError {}

/// Selection scope for a context pack (`all`, `area:<code>`, or `page:<id>`).
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Scope {
    /// Search across every permitted unit in the view.
    #[default]
    All,
    /// Restrict hits to units belonging to one area code (`area:<code>`).
    Area(String),
    /// Lead with one page and its cited claims, then hits outside that page (`page:<id>`).
    Page(String),
}

impl Scope {
    /// Parses `all`, `area:<code>`, or `page:<id>`.
    ///
    /// Accepts arbitrary text without panicking and returns [`UsageError`] for any other form.
    /// Checking that `<code>` or `<id>` exists in the view is left to the caller.
    ///
    /// # Errors
    ///
    /// Returns [`UsageError`] when `input` is empty, has an unknown prefix, or carries an empty
    /// or whitespace/control-bearing target identifier.
    pub fn parse(input: &str) -> Result<Self, UsageError> {
        if input == "all" {
            return Ok(Self::All);
        }
        if let Some(code) = input.strip_prefix("area:") {
            if code.is_empty() || code.chars().any(|c| c.is_whitespace() || c.is_control()) {
                return Err(UsageError::new(format!(
                    "invalid --scope value `{input}`: expected `area:<code>` or `page:<id>`"
                )));
            }
            return Ok(Self::Area(code.to_owned()));
        }
        if let Some(id) = input.strip_prefix("page:") {
            if id.is_empty() || id.chars().any(|c| c.is_whitespace() || c.is_control()) {
                return Err(UsageError::new(format!(
                    "invalid --scope value `{input}`: expected `area:<code>` or `page:<id>`"
                )));
            }
            return Ok(Self::Page(id.to_owned()));
        }
        Err(UsageError::new(format!(
            "invalid --scope value `{input}`: expected `area:<code>` or `page:<id>`"
        )))
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::All => f.write_str("all"),
            Self::Area(code) => write!(f, "area:{code}"),
            Self::Page(id) => write!(f, "page:{id}"),
        }
    }
}

impl Serialize for Scope {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Scope {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(de::Error::custom)
    }
}

/// Optional exclusion overrides for `fieldbook context --include` (`HG-02`).
///
/// All flags default to `false`, leaving Tier-D sources, superseded versions/claims, and
/// contradicted claims out unless named in the question or brought as a conflict dependency.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Include {
    /// Include Tier-D sources (`tier-d`).
    #[serde(default)]
    pub tier_d: bool,
    /// Include superseded source versions and claims (`superseded`).
    #[serde(default)]
    pub superseded: bool,
    /// Include contradicted claims (`contradicted`).
    #[serde(default)]
    pub contradicted: bool,
}

impl Include {
    /// Parses a comma-separated list of inclusion classes (`tier-d`, `superseded`, `contradicted`).
    ///
    /// Accepts arbitrary text without panicking and returns [`UsageError`] for an empty list,
    /// an empty segment, or any unknown word.
    ///
    /// # Errors
    ///
    /// Returns [`UsageError`] if `input` is empty or contains any word outside
    /// `tier-d`, `superseded`, `contradicted`.
    pub fn parse(input: &str) -> Result<Self, UsageError> {
        if input.is_empty() {
            return Err(UsageError::new(
                "invalid --include value: expected one or more of `tier-d`, `superseded`, `contradicted`",
            ));
        }
        let mut out = Self::default();
        for part in input.split(',') {
            match part {
                "tier-d" => out.tier_d = true,
                s if s == crate::strings::CITE_STATUSES[3] => out.superseded = true,
                "contradicted" => out.contradicted = true,
                other => {
                    return Err(UsageError::new(format!(
                        "unknown --include class `{other}`: expected `tier-d`, `superseded`, or `contradicted`"
                    )));
                }
            }
        }
        Ok(out)
    }
}

/// How an item entered a [`ContextPack`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    /// Selected automatically by `context_pack`.
    Auto,
    /// Suggested by an assistant or caller edit.
    Suggested,
    /// Pinned by the owner (mandatory; never trimmed for budget).
    Pinned,
    /// Explicitly added by the owner.
    Added,
}

impl Origin {
    /// Returns the canonical lowercase word (`auto`, `suggested`, `pinned`, `added`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Suggested => "suggested",
            Self::Pinned => "pinned",
            Self::Added => "added",
        }
    }
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why an item was considered for a [`ContextPack`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Why {
    /// Matched the question in search.
    Hit,
    /// Pulled in by AD-18 closure or conflict dependency of another selected item.
    Closure,
    /// Brought in directly by the active [`Scope`] (e.g. `Scope::Page`).
    Scope,
    /// Named explicitly by id in the question, an edit, or a saved [`NamedPack`].
    Named,
}

impl Why {
    /// Returns the canonical lowercase word (`hit`, `closure`, `scope`, `named`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hit => "hit",
            Self::Closure => "closure",
            Self::Scope => "scope",
            Self::Named => "named",
        }
    }
}

impl fmt::Display for Why {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One candidate or included unit in a [`ContextPack`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextItem {
    /// Unit identifier (`cmp-system-review`, `CL-CMP-0001`, `S-0001-v1`, ...).
    pub unit: String,
    /// How this item entered the pack.
    pub origin: Origin,
    /// Why this item was selected.
    pub why: Why,
    /// Identifier of the first selected item that pulled in this `Closure` item, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
    /// Whether this item's block is included in the rendered pack text.
    pub included: bool,
    /// Whether this item is unavailable in the current view or resolved against a changed view fingerprint.
    pub stale: bool,
}

/// A saved named context pack storing references only (never unit text or labels).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedPack {
    /// Pack name.
    pub name: String,
    /// Ordered list of unit identifiers.
    pub ids: Vec<String>,
    /// `view_fingerprint` when the pack was saved.
    pub view_fingerprint: String,
}

/// Sorted, deduplicated set of unit identifiers included in a context pack.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContextIds {
    ids: BTreeSet<String>,
}

impl ContextIds {
    /// Collects the unit identifiers of every `included` item in `pack`.
    #[must_use]
    pub fn from_pack(pack: &ContextPack) -> Self {
        let ids = pack
            .items()
            .iter()
            .filter(|item| item.included)
            .map(|item| item.unit.clone())
            .collect();
        Self { ids }
    }

    /// Builds a [`ContextIds`] set from any iterator of identifiers.
    #[must_use]
    pub fn from_ids<I, S>(iter: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            ids: iter.into_iter().map(Into::into).collect(),
        }
    }

    /// Returns `true` when `id` is in the set.
    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.ids.contains(id)
    }

    /// Returns the number of identifiers in the set.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// Returns `true` when the set holds no identifiers.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Iterates over the sorted identifiers.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.ids.iter().map(String::as_str)
    }

    /// Returns the underlying sorted set.
    #[must_use]
    pub const fn as_set(&self) -> &BTreeSet<String> {
        &self.ids
    }
}

/// Exact Unicode-scalar accounting of a serialized context request (`handoff-context` step 12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestBudget {
    /// Maximum permitted Unicode scalar count for the whole request.
    pub limit: usize,
    /// Scalars consumed by rules (`ASSISTANT.md`) and fixed pack framing (labels, banners, footer, separators).
    pub rules: usize,
    /// Scalars consumed by the question block and its separator.
    pub question: usize,
    /// Scalars consumed by retained conversation history turns and separators (`0` for single-question CLI).
    pub history: usize,
    /// Scalars consumed by included context blocks, their separators, and any `Left out (budget)` list.
    pub context: usize,
    /// Total Unicode scalars of the exact serialized request (including separators and repair text).
    pub total: usize,
    /// `true` iff `total <= limit` and no pinned item is unavailable/inaccessible.
    pub sendable: bool,
    /// `total.saturating_sub(limit)` (`0` when within budget).
    pub over_by: usize,
}

/// Counts Unicode scalar values (`char`s) in `text` — never UTF-8 byte length.
#[must_use]
pub fn scalar_count(text: &str) -> usize {
    text.chars().count()
}

/// Exact scalar reservation of a normalized rules preamble (`ASSISTANT.md`) plus its `\n\n` separator.
#[must_use]
pub(crate) fn rules_preamble_scalars(preamble: &str) -> usize {
    if preamble.is_empty() {
        return 0;
    }
    let normalized = preamble.lines().collect::<Vec<_>>().join("\n");
    scalar_count(&normalized) + 2
}

/// Exact scalar reservation of the `Question: <question>` section plus its blank-line separators.
#[must_use]
pub(crate) fn question_section_scalars(question: &str) -> usize {
    if question.is_empty() {
        return 0;
    }
    // "\nQuestion: " (11) + question + "\n" (1)
    12 + scalar_count(question)
}

/// Exact scalar reservation of an optional history or repair block plus its `\n\n` separator.
#[must_use]
fn optional_block_scalars(text: &str) -> usize {
    if text.is_empty() {
        0
    } else {
        scalar_count(text) + 2
    }
}

/// Request budget input carrying the Unicode-scalar limit and fixed reservations for rules,
/// current question, history, and optional repair text (`handoff-context` step 14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// Maximum total Unicode scalar values permitted for the complete serialized request.
    pub limit: usize,
    /// Reserved Unicode scalars for rules (`ASSISTANT.md`) and separator, if explicitly pre-computed.
    pub rules: usize,
    /// Reserved Unicode scalars for the question and separator, if explicitly pre-computed.
    pub question: usize,
    /// Reserved Unicode scalars for retained conversation history turns and separators.
    pub history: usize,
    /// Reserved Unicode scalars for schema/citation repair text and separator.
    pub repair: usize,
}

impl Budget {
    /// Constructs a [`Budget`] with `limit` and zero explicit pre-reservations.
    ///
    /// When passed to `context_pack`, the builder computes exact `rules` and `question` counts
    /// directly from the view's `ASSISTANT.md` and the question text.
    #[must_use]
    pub const fn from_limit(limit: usize) -> Self {
        Self {
            limit,
            rules: 0,
            question: 0,
            history: 0,
            repair: 0,
        }
    }

    /// Constructs a [`Budget`] for a single-question CLI invocation with empty history.
    #[must_use]
    pub fn for_question(limit: usize, rules: &str, question: &str) -> Self {
        Self {
            limit,
            rules: rules_preamble_scalars(rules),
            question: question_section_scalars(question),
            history: 0,
            repair: 0,
        }
    }

    /// Constructs a [`Budget`] for a multi-turn Bridge chat request after selecting complete turns.
    #[must_use]
    pub fn with_history(limit: usize, rules: &str, question: &str, history: &str) -> Self {
        Self {
            limit,
            rules: rules_preamble_scalars(rules),
            question: question_section_scalars(question),
            history: optional_block_scalars(history),
            repair: 0,
        }
    }

    /// Adds a repair-text reservation (including its separator) to this budget.
    #[must_use]
    pub fn with_repair(mut self, repair: &str) -> Self {
        self.repair = optional_block_scalars(repair);
        self
    }

    /// Returns the total pre-reserved Unicode scalars (`rules + question + history + repair`).
    #[must_use]
    pub const fn reserved(&self) -> usize {
        self.rules
            .saturating_add(self.question)
            .saturating_add(self.history)
            .saturating_add(self.repair)
    }

    /// Returns the remaining Unicode scalar budget after fixed reservations.
    #[must_use]
    pub const fn remaining(&self) -> usize {
        self.limit.saturating_sub(self.reserved())
    }
}

impl From<usize> for Budget {
    fn from(limit: usize) -> Self {
        Self::from_limit(limit)
    }
}

impl From<&Self> for Budget {
    fn from(budget: &Self) -> Self {
        *budget
    }
}
