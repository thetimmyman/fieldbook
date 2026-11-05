//! Glossary-driven synonym expansion (`docs/reference/search.md#synonyms`).
//!
//! For each glossary row, its `label` and `aliases` form one synonym set. Each member is lexed to
//! tokens (query-side lexing without deduplication or length cap). On the index side, for each
//! field of each unit except `id`: if the field's token sequence contains a member as a contiguous
//! run, the tokens of every other member of the set are appended to the unit's `body` field, once
//! per set per unit, in glossary order (`term_id`) and member order (`label`, then `aliases`).

use serde::{Deserialize, Serialize};

use crate::schema::GlossaryRow;

use super::lexer::lex;
use super::units::UnitFields;

/// One row of a [`Glossary`] (the form of `glossary` entries in ranked golden files).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlossaryEntry {
    /// The canonical term label (`label` column of `glossary.csv`).
    pub label: String,
    /// The term's synonyms and acronyms (`aliases` column of `glossary.csv`).
    #[serde(default)]
    pub aliases: Vec<String>,
}

/// Ordered glossary rows used to build [`SynonymSet`]s for index-side expansion.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Glossary {
    /// Glossary rows in `term_id` order.
    pub entries: Vec<GlossaryEntry>,
}

/// One pre-lexed synonym set built from a [`GlossaryEntry`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SynonymSet {
    /// Token sequences for `label` followed by each entry in `aliases`.
    pub members: Vec<Vec<String>>,
}

impl Glossary {
    /// Creates a [`Glossary`] from an ordered list of [`GlossaryEntry`] rows.
    #[must_use]
    pub const fn new(entries: Vec<GlossaryEntry>) -> Self {
        Self { entries }
    }

    /// Builds a [`Glossary`] from typed `glossary.csv` rows, sorted by `term_id`.
    #[must_use]
    pub fn from_rows(rows: &[GlossaryRow]) -> Self {
        let mut sorted: Vec<&GlossaryRow> = rows.iter().collect();
        sorted.sort_by(|a, b| a.term_id.cmp(&b.term_id));
        let entries = sorted
            .into_iter()
            .map(|row| GlossaryEntry {
                label: row.label.clone(),
                aliases: row.aliases.clone(),
            })
            .collect();
        Self { entries }
    }

    /// Returns the glossary entries.
    #[must_use]
    pub fn entries(&self) -> &[GlossaryEntry] {
        &self.entries
    }

    /// Compiles this glossary's rows into [`SynonymSet`]s.
    #[must_use]
    pub fn synonym_sets(&self) -> Vec<SynonymSet> {
        self.entries.iter().map(SynonymSet::from_entry).collect()
    }

    /// Applies synonym expansion to `fields` in glossary order, once per set per unit.
    pub fn apply(&self, fields: &mut UnitFields) {
        let sets = self.synonym_sets();
        apply_synonym_sets(&sets, fields);
    }
}

impl From<Vec<GlossaryEntry>> for Glossary {
    fn from(entries: Vec<GlossaryEntry>) -> Self {
        Self { entries }
    }
}

impl SynonymSet {
    /// Builds a [`SynonymSet`] from a [`GlossaryEntry`] by lexing `label` and each alias without
    /// deduplication or index-side extras.
    #[must_use]
    pub fn from_entry(entry: &GlossaryEntry) -> Self {
        let mut members = Vec::with_capacity(1 + entry.aliases.len());
        let label_tokens = lex_member(&entry.label);
        if !label_tokens.is_empty() {
            members.push(label_tokens);
        }
        for alias in &entry.aliases {
            let alias_tokens = lex_member(alias);
            if !alias_tokens.is_empty() {
                members.push(alias_tokens);
            }
        }
        Self { members }
    }
}

fn lex_member(text: &str) -> Vec<String> {
    lex(text).into_iter().map(|t| t.text).collect()
}

fn contains_contiguous_run(haystack: &[String], needle: &[String]) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Applies pre-built `sets` to `fields` on the index side: for each set in glossary order, checks
/// the unit's non-`id` fields (`title`, `headings`, `claim`, `body`) as built before synonym
/// expansion, and on the first matching member appends the tokens of every other member of that
/// set to `fields.body`, once per set per unit.
pub fn apply_synonym_sets(sets: &[SynonymSet], fields: &mut UnitFields) {
    if sets.is_empty() {
        return;
    }
    let original_body_len = fields.body.len();
    for set in sets {
        if set.members.len() < 2 {
            continue;
        }
        let non_id_fields: [&[String]; 4] = [
            &fields.title,
            &fields.headings,
            &fields.claim,
            &fields.body[..original_body_len],
        ];
        let mut matched_member: Option<usize> = None;
        'search: for field in non_id_fields {
            for (idx, member) in set.members.iter().enumerate() {
                if contains_contiguous_run(field, member) {
                    matched_member = Some(idx);
                    break 'search;
                }
            }
        }
        if let Some(matched_idx) = matched_member {
            for (idx, member) in set.members.iter().enumerate() {
                if idx != matched_idx {
                    fields.body.extend(member.iter().cloned());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::lexer::index_tokens;

    #[test]
    fn expands_synonyms_into_body_once_per_set_per_unit_in_glossary_order() {
        let glossary = Glossary::new(vec![
            GlossaryEntry {
                label: "independent cost estimate".to_owned(),
                aliases: vec!["ice".to_owned()],
            },
            GlossaryEntry {
                label: "9904.401".to_owned(),
                aliases: vec!["cas 401".to_owned()],
            },
        ]);

        // 1. Match in body expands alias into body; CAS alias expands clause number into body.
        let mut fields = UnitFields {
            id: index_tokens("cmp-review"),
            title: index_tokens("Prepare an independent cost estimate"),
            headings: Vec::new(),
            claim: Vec::new(),
            body: index_tokens("Follows CAS 401 and independent cost estimate rules."),
        };
        glossary.apply(&mut fields);
        assert_eq!(
            fields.body,
            vec![
                "follows",
                "cas",
                "401",
                "and",
                "independent",
                "cost",
                "estimate",
                "rules",
                "ice",
                "9904.401",
            ]
        );

        // 2. Match in `id` field alone does NOT trigger synonym expansion.
        let mut id_only = UnitFields {
            id: index_tokens("ice"),
            title: index_tokens("unrelated title"),
            headings: Vec::new(),
            claim: Vec::new(),
            body: Vec::new(),
        };
        glossary.apply(&mut id_only);
        assert!(id_only.body.is_empty());
    }
}
