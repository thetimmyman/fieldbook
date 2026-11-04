//! Closed vocabularies for Corpus pages (and, in later cards, registers and claims).
//!
//! **Responsibility.** Define each page vocabulary once as a typed enum with
//! `ALL`, `as_str` and `parse`, re-export [`Volatility`](crate::date::Volatility),
//! provide [`label_form`] for handling label syntax, and verify in tests that
//! every enum's `ALL` matches its table row in `docs/reference/corpus-format.md`.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub use crate::date::Volatility;

/// Error returned when a string is outside a closed vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VocabError {
    vocabulary: &'static str,
    value: String,
}

impl VocabError {
    /// Creates a new vocabulary error naming the vocabulary and offending value.
    #[must_use]
    pub fn new(vocabulary: &'static str, value: impl Into<String>) -> Self {
        Self {
            vocabulary,
            value: value.into(),
        }
    }

    /// Returns the name of the vocabulary that rejected the value.
    #[must_use]
    pub const fn vocabulary(&self) -> &'static str {
        self.vocabulary
    }

    /// Returns the rejected string value.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

impl fmt::Display for VocabError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "value {:?} is not in vocabulary {}",
            self.value, self.vocabulary
        )
    }
}

impl std::error::Error for VocabError {}

macro_rules! define_vocab_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident ($vocab_str:literal, $count:expr) {
            $(
                $(#[$vmeta:meta])*
                $variant:ident => $str_val:literal
            ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        $vis enum $name {
            $(
                $(#[$vmeta])*
                $variant,
            )+
        }

        impl $name {
            /// Every variant of this vocabulary, in reference table order.
            pub const ALL: [Self; $count] = [
                $(Self::$variant),+
            ];

            /// Returns the canonical string spelling in the Corpus format.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $str_val),+
                }
            }

            /// Parses a value from its exact Corpus string spelling.
            pub fn parse(input: &str) -> Result<Self, VocabError> {
                match input {
                    $($str_val => Ok(Self::$variant),)+
                    _ => Err(VocabError::new($vocab_str, input)),
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = VocabError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::parse(s)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let s = String::deserialize(deserializer)?;
                Self::parse(&s).map_err(serde::de::Error::custom)
            }
        }
    };
}

define_vocab_enum! {
    /// The nine v1 page types (`type` in page front matter; `note` arrives at Day 30 with schema 2).
    pub enum PageType("type", 9) {
        /// Define an idea and its boundaries.
        Concept => "concept",
        /// A public method done in steps.
        Procedure => "procedure",
        /// A lookup of values, rules or lists.
        Reference => "reference",
        /// A public case, read for what it shows.
        Case => "case",
        /// One clause or provision of a regulation.
        ClauseCard => "clause-card",
        /// A calculation `fieldbook-calc` can check.
        Calc => "calc",
        /// Tasks to run through.
        Checklist => "checklist",
        /// A fill-in form shown as text.
        Template => "template",
        /// A one-page decision paper.
        Brief => "brief",
    }
}

define_vocab_enum! {
    /// Page readiness (`readiness` in page front matter).
    pub enum Readiness("readiness", 5) {
        /// Written from public sources and usable before any company answer.
        PublicNow => "PUBLIC-NOW",
        /// Needs made-up, labelled example data to be useful; uses no company data.
        SyntheticNow => "SYNTHETIC-NOW",
        /// Maps what the owner will learn at onboarding; depends on company answers.
        MapOnboarding => "MAP-ONBOARDING",
        /// Depends on a company policy that is not public.
        PolicyGated => "POLICY-GATED",
        /// Depends on the terms of a contract the company holds.
        ContractSpecific => "CONTRACT-SPECIFIC",
    }
}

define_vocab_enum! {
    /// Maturity status of a page (`status` in page front matter).
    pub enum PageStatus("status", 3) {
        /// Being written; may hold open items.
        Draft => "draft",
        /// A person other than the author read it against its claims.
        Reviewed => "reviewed",
        /// The owner accepted it.
        Stable => "stable",
    }
}

define_vocab_enum! {
    /// Change-note entry class (`Change class` in the Corpus format).
    pub enum ChangeClass("change_class", 5) {
        /// A source or fact with no earlier row.
        NewKnowledge => "NEW_KNOWLEDGE",
        /// The same source in a new version.
        SourceRefresh => "SOURCE_REFRESH",
        /// Two sources or rows disagree.
        ClaimConflict => "CLAIM_CONFLICT",
        /// Detail is added and no existing claim changes.
        CompatibleExtension => "COMPATIBLE_EXTENSION",
        /// Company store only: a company contract term or clause changes a calculation.
        MaterialContractChange => "MATERIAL_CONTRACT_CHANGE",
    }
}

/// The four built-in handling label identifiers.
pub const BUILTIN_LABELS: [&str; 4] = ["public", "unlabelled", "marked", "owner-supplied"];

/// Returns `true` when `input` is one of the four built-in handling labels
/// (`public`, `unlabelled`, `marked`, `owner-supplied`) or matches the opaque
/// company label pattern `^[a-z]{2,4}-[0-9]{4}$`.
#[must_use]
pub fn label_form(input: &str) -> bool {
    if BUILTIN_LABELS.contains(&input) {
        return true;
    }
    let Some((prefix, digits)) = input.split_once('-') else {
        return false;
    };
    (2..=4).contains(&prefix.len())
        && prefix.bytes().all(|b| b.is_ascii_lowercase())
        && digits.len() == 4
        && digits.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CORPUS_FORMAT_MD: &str = include_str!("../../../../docs/reference/corpus-format.md");

    fn extract_backticked(cell: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = cell;
        while let Some(start) = rest.find('`') {
            let after_start = &rest[start + 1..];
            let Some(end) = after_start.find('`') else {
                break;
            };
            let token = &after_start[..end];
            if !token.is_empty() {
                out.push(token.to_owned());
            }
            rest = &after_start[end + 1..];
        }
        out
    }

    fn strip_backticks(s: &str) -> String {
        s.chars().filter(|&c| c != '`').collect()
    }

    fn reference_vocab_values(doc: &str, vocab_key: &str) -> Vec<String> {
        if vocab_key == "type" {
            // Read the `type` row in the Front matter fields table of `## Page files`,
            // taking the v1 values before `. \`note\` joins`.
            let mut in_page_files = false;
            for line in doc.lines() {
                if line.starts_with("## Page files") {
                    in_page_files = true;
                    continue;
                }
                if in_page_files && line.starts_with("## ") {
                    break;
                }
                if in_page_files && line.starts_with('|') {
                    let cells: Vec<&str> = line
                        .trim()
                        .trim_matches('|')
                        .split('|')
                        .map(str::trim)
                        .collect();
                    if cells.len() == 4 && cells[0] == "`type`" {
                        let allowed_v1 = cells[3].split('.').next().unwrap_or(cells[3]);
                        return extract_backticked(allowed_v1);
                    }
                }
            }
            return Vec::new();
        }

        // Look in `## Status vocabularies and promotion rules`.
        let Some(start_idx) = doc.find("## Status vocabularies and promotion rules") else {
            return Vec::new();
        };
        let section = &doc[start_idx..];
        let mut lines = section.lines().skip(1).peekable();

        while let Some(line) = lines.next() {
            if line.starts_with("## ") {
                break;
            }
            if !line.trim_start().starts_with('|') {
                continue;
            }
            let header_cells: Vec<&str> = line
                .trim()
                .trim_matches('|')
                .split('|')
                .map(str::trim)
                .collect();
            // Skip separator line `|---|...|`
            if lines
                .peek()
                .is_some_and(|next| next.trim_start().starts_with("|---"))
            {
                let _ = lines.next();
            }

            let first_header = strip_backticks(header_cells.first().copied().unwrap_or(""));
            if first_header == vocab_key {
                // Table where column 0 of each row holds the vocabulary value(s),
                // such as `| Change class | Used when |` or `| source_kind | ... |`.
                let mut values = Vec::new();
                while let Some(&row_line) = lines.peek() {
                    if !row_line.trim_start().starts_with('|') {
                        break;
                    }
                    let _ = lines.next();
                    let cells: Vec<&str> = row_line
                        .trim()
                        .trim_matches('|')
                        .split('|')
                        .map(str::trim)
                        .collect();
                    if let Some(&first_cell) = cells.first() {
                        values.extend(extract_backticked(first_cell));
                    }
                }
                return values;
            }

            if first_header == "Field" && header_cells.get(1) == Some(&"Value") {
                let mut active = false;
                let mut values = Vec::new();
                while let Some(&row_line) = lines.peek() {
                    if !row_line.trim_start().starts_with('|') {
                        break;
                    }
                    let _ = lines.next();
                    let cells: Vec<&str> = row_line
                        .trim()
                        .trim_matches('|')
                        .split('|')
                        .map(str::trim)
                        .collect();
                    if cells.len() < 2 {
                        continue;
                    }
                    let field_cell = strip_backticks(cells[0]);
                    let field_trimmed = field_cell.trim();
                    if !field_trimmed.is_empty() {
                        if active {
                            return values;
                        }
                        if field_trimmed == vocab_key
                            || field_trimmed
                                .strip_prefix(vocab_key)
                                .is_some_and(|rest| rest.starts_with(' '))
                        {
                            active = true;
                        }
                    }
                    if active {
                        values.extend(extract_backticked(cells[1]));
                    }
                }
                if active {
                    return values;
                }
            }
        }
        Vec::new()
    }

    fn assert_vocab_matches(vocab_name: &str, doc_key: &str, code_values: &[&str]) {
        let doc_values = reference_vocab_values(CORPUS_FORMAT_MD, doc_key);
        assert!(
            !doc_values.is_empty(),
            "vocabulary {vocab_name} ({doc_key}) was not found in corpus-format.md"
        );
        let doc_strs: Vec<&str> = doc_values.iter().map(String::as_str).collect();
        assert_eq!(
            code_values,
            doc_strs.as_slice(),
            "vocabulary {vocab_name} does not match corpus-format.md row {doc_key:?}"
        );
    }

    #[test]
    fn every_enum_matches_the_reference() {
        assert_vocab_matches("type", "type", &PageType::ALL.map(PageType::as_str));
        assert_vocab_matches(
            "readiness",
            "readiness",
            &Readiness::ALL.map(Readiness::as_str),
        );
        assert_vocab_matches(
            "page status",
            "page status",
            &PageStatus::ALL.map(PageStatus::as_str),
        );
        assert_vocab_matches(
            "volatility",
            "volatility",
            &Volatility::ALL.map(Volatility::as_str),
        );
        assert_vocab_matches(
            "change_class",
            "Change class",
            &ChangeClass::ALL.map(ChangeClass::as_str),
        );
    }

    #[test]
    fn label_form_accepts_builtins_and_opaque_company_labels() {
        for builtin in BUILTIN_LABELS {
            assert!(
                label_form(builtin),
                "expected builtin {builtin} to be valid"
            );
        }
        assert!(label_form("np-0007"));
        assert!(label_form("acme-1234"));
        assert!(!label_form(""));
        assert!(!label_form("PUBLIC"));
        assert!(!label_form("a-0001"));
        assert!(!label_form("abcde-0001"));
        assert!(!label_form("np-001"));
        assert!(!label_form("np-00001"));
        assert!(!label_form("NP-0007"));
    }
}
