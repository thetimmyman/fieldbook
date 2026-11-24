//! Deterministic intake rule classifier (`rules@1`), rule-file parsing, and the shared
//! source-kind/tier table (`docs/reference/corpus-format.md#08-audit-backup-and-pipeline`).
//!
//! **Responsibility.** Read `corpus/pipeline/rules.toml` into a [`RulesFile`], match each rule's
//! `when` against a [`Candidate`]'s extracted pages, turn the first matching rule per field into a
//! [`Suggestion`] that carries the rule's provenance ([`RuleRef`]) and the page's own text as its
//! quote, fall back to the first line for `title`, and drop an `authority_tier` the suggested
//! `source_kind` does not allow.
//!
//! **Rules.** Pure: no I/O, clock, environment, or model access. Never panics on arbitrary inputs.

use std::collections::BTreeSet;
use std::ops::RangeInclusive;

use crate::check::register_cells::source_kind_allows_tier;
use crate::extraction::PageText;
use crate::pipeline::candidate::{Candidate, SuggestField, Vocab};
use crate::pipeline::suggest::{
    By, Classifier, RuleKind, RuleRef, StageError, Suggestion, Suggestions, Support, validate,
};
use crate::quote;
use crate::schema::corpus_toml::byte_offset_to_line_col;
use crate::schema::{AuthorityTier, SourceKind};
use crate::search::lexer;

/// The pages a rule's `when` may scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pages {
    /// The first page only (the default).
    First,
    /// Every page, in order.
    Any,
}

impl Pages {
    /// Parses the optional `pages` key value (`"any"`); the default is [`Pages::First`].
    #[must_use]
    pub fn parse(input: &str) -> Option<Self> {
        match input {
            "any" => Some(Self::Any),
            _ => None,
        }
    }
}

/// The lexer shape number an `id_shape` rule matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdShape {
    /// Clause number (`[0-9]{1,4}\.[0-9]{3}(-[0-9]+)?(\([a-z0-9]{1,4}\))*`), lexer shape 9.
    ClauseNumber,
}

impl IdShape {
    /// The canonical lowercase shape name accepted in `id_shape`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ClauseNumber => "clause_number",
        }
    }

    /// The 1-based [`lexer::LEXER_SHAPES`] number this shape matches.
    #[must_use]
    pub const fn shape_number(self) -> u8 {
        match self {
            Self::ClauseNumber => 9,
        }
    }

    /// Parses a shape name; only `clause_number` is useful for an intake rule.
    #[must_use]
    pub fn parse(input: &str) -> Option<Self> {
        match input {
            "clause_number" => Some(Self::ClauseNumber),
            _ => None,
        }
    }
}

/// The `when` condition of one rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum When {
    /// A phrase found verbatim (or through quote normalisation) on the page.
    Phrase {
        /// The phrase to find.
        text: String,
        /// Which pages to scan.
        pages: Pages,
    },
    /// A publisher name found on the page; matches like [`When::Phrase`], only `queue show`
    /// prints a different reason.
    Publisher {
        /// The publisher name to find.
        text: String,
        /// Which pages to scan.
        pages: Pages,
    },
    /// A lexer shape token found by the lexer.
    IdShape {
        /// The lexer shape to match.
        shape: IdShape,
        /// Which pages to scan.
        pages: Pages,
    },
}

impl When {
    /// The pages this `when` scans.
    #[must_use]
    pub const fn pages(&self) -> Pages {
        match self {
            Self::Phrase { pages, .. }
            | Self::Publisher { pages, .. }
            | Self::IdShape { pages, .. } => *pages,
        }
    }

    /// The [`RuleKind`] of this `when` for the suggestion's [`RuleRef`].
    #[must_use]
    pub const fn kind(&self) -> RuleKind {
        match self {
            Self::Phrase { .. } => RuleKind::Phrase,
            Self::Publisher { .. } => RuleKind::Publisher,
            Self::IdShape { .. } => RuleKind::IdShape,
        }
    }
}

/// The `set` of a rule: the field to fill and its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Set {
    /// Target suggestible field.
    pub field: SuggestField,
    /// Suggested value string.
    pub value: String,
}

/// The `expect` of a rule: what its own `example` must reproduce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expect {
    /// The field the example must reproduce.
    pub field: SuggestField,
    /// The value the example must reproduce.
    pub value: String,
    /// Optional quote the reproduced suggestion must carry.
    pub quote: Option<String>,
}

/// One parsed rule, in file order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// Unique rule identifier (`[a-z0-9-]{1,40}`).
    pub id: String,
    /// 1-based line where this `[[rule]]` starts.
    pub line: u32,
    /// The `when` condition.
    pub when: When,
    /// The `set` target.
    pub set: Set,
    /// The single-page example text.
    pub example: String,
    /// The expected reproduction.
    pub expect: Expect,
}

/// One rule that could not be read, with its line and reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleError {
    /// 1-based line of the fault (at least 1).
    pub line: u32,
    /// What is wrong.
    pub message: String,
}

/// The parsed rules of `corpus/pipeline/rules.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RulesFile {
    /// Successfully-parsed rules, in file order.
    pub rules: Vec<Rule>,
}

fn line_of(text: &str, byte_offset: usize) -> u32 {
    byte_offset_to_line_col(text, byte_offset).0.max(1)
}

fn is_rule_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn rule_header_lines(text: &str) -> Vec<u32> {
    let mut lines = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("[[rule]]") {
            let rest = rest.trim_start();
            if rest.is_empty() || rest.starts_with('#') {
                lines.push(u32::try_from(idx + 1).unwrap_or(u32::MAX));
            }
        }
    }
    lines
}

impl RulesFile {
    /// Parses `corpus/pipeline/rules.toml` text, denying unknown keys and validating each rule's
    /// `set` against `vocab`.
    ///
    /// # Errors
    ///
    /// Returns every [`RuleError`] (each with a 1-based line) when any rule cannot be read, a rule
    /// id repeats, a rule lacks its `example` or `expect`, or a `set` value is outside `vocab`.
    pub fn parse(text: &str, vocab: &Vocab) -> Result<Self, Vec<RuleError>> {
        let value: toml::Value = match toml::from_str(text) {
            Ok(value) => value,
            Err(err) => {
                let offset = err.span().map_or(0, |r| r.start);
                return Err(vec![RuleError {
                    line: line_of(text, offset),
                    message: err.message().to_owned(),
                }]);
            }
        };

        let Some(table) = value.as_table() else {
            return Err(vec![RuleError {
                line: 1,
                message: "rules file must be a table".to_owned(),
            }]);
        };

        let mut errors = Vec::new();
        for key in table.keys() {
            if key != "rule" {
                errors.push(RuleError {
                    line: 1,
                    message: format!("unknown key `{key}` in rules file"),
                });
            }
        }

        let Some(rule_value) = table.get("rule") else {
            if errors.is_empty() {
                return Ok(Self { rules: Vec::new() });
            }
            return Err(errors);
        };

        let Some(array) = rule_value.as_array() else {
            errors.push(RuleError {
                line: 1,
                message: "`rule` must be an array of tables".to_owned(),
            });
            return Err(errors);
        };

        let header_lines = rule_header_lines(text);
        let mut rules = Vec::new();
        let mut seen_ids = BTreeSet::new();

        for (idx, item) in array.iter().enumerate() {
            let line = header_lines.get(idx).copied().unwrap_or(1);
            let Some(rule_table) = item.as_table() else {
                errors.push(RuleError {
                    line,
                    message: "each `[[rule]]` must be a table".to_owned(),
                });
                continue;
            };
            match parse_rule(rule_table, line, vocab, &mut seen_ids) {
                Ok(rule) => rules.push(rule),
                Err(mut errs) => errors.append(&mut errs),
            }
        }

        if !errors.is_empty() {
            return Err(errors);
        }
        Ok(Self { rules })
    }
}

fn parse_rule(
    table: &toml::Table,
    line: u32,
    vocab: &Vocab,
    seen_ids: &mut BTreeSet<String>,
) -> Result<Rule, Vec<RuleError>> {
    let mut errors = Vec::new();

    for key in table.keys() {
        if !matches!(key.as_str(), "id" | "when" | "set" | "example" | "expect") {
            errors.push(RuleError {
                line,
                message: format!("unknown key `{key}` in rule"),
            });
        }
    }

    let id = match table.get("id").and_then(toml::Value::as_str) {
        Some(id) => {
            if !is_rule_id(id) {
                errors.push(RuleError {
                    line,
                    message: format!("rule id `{id}` must match `[a-z0-9-]{{1,40}}`"),
                });
            }
            if !seen_ids.insert(id.to_owned()) {
                errors.push(RuleError {
                    line,
                    message: format!("duplicate rule id `{id}`"),
                });
            }
            id.to_owned()
        }
        None => {
            errors.push(RuleError {
                line,
                message: "rule is missing its `id`".to_owned(),
            });
            String::new()
        }
    };

    let when = parse_when(table.get("when"), line, &mut errors);
    if table.get("when").is_none() {
        errors.push(RuleError {
            line,
            message: format!("rule `{id}` is missing its `when`"),
        });
    }

    let set = parse_set(table.get("set"), line, vocab, &mut errors);
    if table.get("set").is_none() {
        errors.push(RuleError {
            line,
            message: format!("rule `{id}` is missing its `set`"),
        });
    }

    let example = match table.get("example").and_then(toml::Value::as_str) {
        Some(example) => example.to_owned(),
        None => {
            errors.push(RuleError {
                line,
                message: format!("rule `{id}` is missing its `example`"),
            });
            String::new()
        }
    };

    let expect = parse_expect(table.get("expect"), line, &mut errors);
    if table.get("expect").is_none() {
        errors.push(RuleError {
            line,
            message: format!("rule `{id}` is missing its `expect`"),
        });
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(Rule {
        id,
        line,
        when,
        set,
        example,
        expect,
    })
}

fn parse_when(value: Option<&toml::Value>, line: u32, errors: &mut Vec<RuleError>) -> When {
    let Some(table) = value.and_then(toml::Value::as_table) else {
        errors.push(RuleError {
            line,
            message: "`when` must be a table".to_owned(),
        });
        return When::Phrase {
            text: String::new(),
            pages: Pages::First,
        };
    };

    for key in table.keys() {
        if !matches!(key.as_str(), "phrase" | "id_shape" | "publisher" | "pages") {
            errors.push(RuleError {
                line,
                message: format!("unknown key `{key}` in `when`"),
            });
        }
    }

    let pages = match table.get("pages") {
        Some(value) => match value.as_str().and_then(Pages::parse) {
            Some(pages) => pages,
            None => {
                errors.push(RuleError {
                    line,
                    message: "`pages` must be `\"any\"` when present".to_owned(),
                });
                Pages::First
            }
        },
        None => Pages::First,
    };

    let phrase = table.get("phrase").and_then(toml::Value::as_str);
    let id_shape = table.get("id_shape").and_then(toml::Value::as_str);
    let publisher = table.get("publisher").and_then(toml::Value::as_str);
    let count = usize::from(phrase.is_some())
        + usize::from(id_shape.is_some())
        + usize::from(publisher.is_some());
    let fallback = When::Phrase {
        text: String::new(),
        pages: Pages::First,
    };
    if count != 1 {
        errors.push(RuleError {
            line,
            message: "`when` must have exactly one of `phrase`, `id_shape` or `publisher`"
                .to_owned(),
        });
        return fallback;
    }

    if let Some(text) = phrase {
        When::Phrase {
            text: text.to_owned(),
            pages,
        }
    } else if let Some(text) = publisher {
        When::Publisher {
            text: text.to_owned(),
            pages,
        }
    } else if let Some(name) = id_shape {
        match IdShape::parse(name) {
            Some(shape) => When::IdShape { shape, pages },
            None => {
                errors.push(RuleError {
                    line,
                    message: format!("unknown `id_shape` name `{name}`"),
                });
                fallback
            }
        }
    } else {
        unreachable!("count == 1 guarantees one arm")
    }
}

fn parse_set(
    value: Option<&toml::Value>,
    line: u32,
    vocab: &Vocab,
    errors: &mut Vec<RuleError>,
) -> Set {
    let Some(table) = value.and_then(toml::Value::as_table) else {
        errors.push(RuleError {
            line,
            message: "`set` must be a table".to_owned(),
        });
        return Set {
            field: SuggestField::Title,
            value: String::new(),
        };
    };

    for key in table.keys() {
        if !matches!(key.as_str(), "field" | "value") {
            errors.push(RuleError {
                line,
                message: format!("unknown key `{key}` in `set`"),
            });
        }
    }

    let field = match table.get("field").and_then(toml::Value::as_str) {
        Some(name) => match SuggestField::parse(name) {
            Some(field) => {
                if !vocab.offers(field) {
                    errors.push(RuleError {
                        line,
                        message: format!(
                            "`set` names field `{name}` the vocabulary does not offer"
                        ),
                    });
                }
                field
            }
            None => {
                errors.push(RuleError {
                    line,
                    message: format!("`set` names field `{name}` that is reserved or unknown"),
                });
                SuggestField::Title
            }
        },
        None => {
            errors.push(RuleError {
                line,
                message: "`set` is missing its `field`".to_owned(),
            });
            SuggestField::Title
        }
    };

    let value = match table.get("value").and_then(toml::Value::as_str) {
        Some(value) => {
            if vocab.accepts_value(field, value).is_err() {
                errors.push(RuleError {
                    line,
                    message: format!("`set` value `{value}` is outside the `{field}` vocabulary"),
                });
            }
            value.to_owned()
        }
        None => {
            errors.push(RuleError {
                line,
                message: "`set` is missing its `value`".to_owned(),
            });
            String::new()
        }
    };

    Set { field, value }
}

fn parse_expect(value: Option<&toml::Value>, line: u32, errors: &mut Vec<RuleError>) -> Expect {
    let Some(table) = value.and_then(toml::Value::as_table) else {
        errors.push(RuleError {
            line,
            message: "`expect` must be a table".to_owned(),
        });
        return Expect {
            field: SuggestField::Title,
            value: String::new(),
            quote: None,
        };
    };

    for key in table.keys() {
        if !matches!(key.as_str(), "field" | "value" | "quote") {
            errors.push(RuleError {
                line,
                message: format!("unknown key `{key}` in `expect`"),
            });
        }
    }

    let field = match table.get("field").and_then(toml::Value::as_str) {
        Some(name) => match SuggestField::parse(name) {
            Some(field) => field,
            None => {
                errors.push(RuleError {
                    line,
                    message: format!("`expect` names field `{name}` that is reserved or unknown"),
                });
                SuggestField::Title
            }
        },
        None => {
            errors.push(RuleError {
                line,
                message: "`expect` is missing its `field`".to_owned(),
            });
            SuggestField::Title
        }
    };

    let value = match table.get("value").and_then(toml::Value::as_str) {
        Some(value) => value.to_owned(),
        None => {
            errors.push(RuleError {
                line,
                message: "`expect` is missing its `value`".to_owned(),
            });
            String::new()
        }
    };

    let quote = table
        .get("quote")
        .and_then(toml::Value::as_str)
        .map(str::to_owned);

    Expect {
        field,
        value,
        quote,
    }
}

/// Whether a suggested `authority_tier` is permitted for a suggested `source_kind`, using the same
/// table as register validation (`FB0204`): every eligible public kind permits discovery-only `D`,
/// while `synthetic_fixture` and the company kinds stay untiered.
#[must_use]
pub fn tier_allowed(kind: SourceKind, tier: AuthorityTier) -> bool {
    source_kind_allows_tier(kind, tier)
}

/// Finds the first page in `within` where `when` matches, returning the page's own text as the
/// quote support, or `None` when nothing matches.
fn match_when(when: &When, candidate: &Candidate) -> Option<Support> {
    let ext = candidate.extraction.as_ref()?;
    let page_count = u32::try_from(ext.pages.len()).unwrap_or(0);
    let within: RangeInclusive<u32> = match when.pages() {
        Pages::First => 1..=1,
        Pages::Any => 1..=page_count,
    };
    let page_texts: Vec<&str> = ext.pages.iter().map(|p| p.text.as_str()).collect();

    match when {
        When::Phrase { text, .. } | When::Publisher { text, .. } => {
            let span = quote::find(text, &page_texts, within)?;
            slice_quote(&ext.pages, span)
        }
        When::IdShape { shape, .. } => {
            for page in &ext.pages {
                if !within.contains(&page.number) {
                    continue;
                }
                for token in lexer::lex(&page.text) {
                    if token.shape == shape.shape_number() {
                        let span =
                            quote::find(&token.text, &page_texts, page.number..=page.number)?;
                        return slice_quote(&ext.pages, span);
                    }
                }
            }
            None
        }
    }
}

fn slice_quote(pages: &[PageText], span: quote::Span) -> Option<Support> {
    let page = pages.get(span.page.saturating_sub(1) as usize)?;
    let quote = page.text.get(span.start..span.end)?;
    Some(Support::Quote {
        page: span.page,
        quote: quote.to_owned(),
    })
}

/// The `rules@1` deterministic classifier.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rules {
    rules: RulesFile,
}

impl Rules {
    /// Builds the classifier from a parsed [`RulesFile`].
    #[must_use]
    pub const fn new(rules: RulesFile) -> Self {
        Self { rules }
    }

    /// The parsed rules, in file order.
    #[must_use]
    pub fn rules(&self) -> &[Rule] {
        &self.rules.rules
    }
}

fn drop_tier_the_kind_does_not_allow(fields: &mut Vec<Suggestion>) {
    let kind = fields
        .iter()
        .find(|s| s.field == SuggestField::SourceKind)
        .and_then(|s| SourceKind::parse(&s.value).ok());
    let Some(kind) = kind else {
        return;
    };
    let tier = fields
        .iter()
        .find(|s| s.field == SuggestField::AuthorityTier)
        .and_then(|s| AuthorityTier::parse(&s.value).ok());
    let Some(tier) = tier else {
        return;
    };
    if !tier_allowed(kind, tier) {
        fields.retain(|s| s.field != SuggestField::AuthorityTier);
    }
}

impl Classifier for Rules {
    fn id(&self) -> &'static str {
        "rules@1"
    }

    fn classify(&self, candidate: &Candidate, vocab: &Vocab) -> Result<Suggestions, StageError> {
        let mut fields = Vec::new();
        let mut taken: BTreeSet<SuggestField> = BTreeSet::new();

        for rule in &self.rules.rules {
            let field = rule.set.field;
            if taken.contains(&field) || !vocab.offers(field) {
                continue;
            }
            let Some(support) = match_when(&rule.when, candidate) else {
                continue;
            };
            let suggestion = Suggestion {
                field,
                value: rule.set.value.clone(),
                by: By::Rules,
                support: vec![support],
                rule: Some(RuleRef {
                    id: rule.id.clone(),
                    kind: rule.when.kind(),
                }),
            };
            if validate(&suggestion, candidate, vocab).is_ok() {
                taken.insert(field);
                fields.push(suggestion);
            }
        }

        if !taken.contains(&SuggestField::Title)
            && let Some((page, line)) = candidate.first_line()
            && line.chars().count() <= 160
        {
            let suggestion = Suggestion {
                field: SuggestField::Title,
                value: line.to_owned(),
                by: By::Rules,
                support: vec![Support::Quote {
                    page,
                    quote: line.to_owned(),
                }],
                rule: None,
            };
            if validate(&suggestion, candidate, vocab).is_ok() {
                fields.push(suggestion);
            }
        }

        drop_tier_the_kind_does_not_allow(&mut fields);

        Ok(Suggestions {
            fields,
            hints: Vec::new(),
        })
    }
}
