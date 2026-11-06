//! In-memory search index builder (`docs/reference/search.md#the-site-index`).
//!
//! Builds [`Index`] (`docs`, `terms`, `post`) from Corpus [`Unit`]s ([`Index::build`]) or ranked
//! golden units ([`Index::from_golden`]). Units are ordered by [`natural_id_cmp`], terms are
//! ordered by UTF-16 code units ([`utf16_cmp`]), and each posting list is a flat `[unit, impact,
//! ...]` sequence with `unit` numbers ascending.

use std::collections::BTreeMap;

use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use unicode_normalization::UnicodeNormalization;

use super::bm25f::{average_field_lengths, idf, impact, tf_prime_from_counts};
use super::lexer::index_tokens;
use super::synonyms::{Glossary, apply_synonym_sets};
use super::units::{Unit, UnitDisplay, UnitFields, natural_id_cmp, utf16_cmp};

/// The five raw string fields of a [`GoldenUnit`] in a ranked golden file
/// (`tests/golden/README.md#ranked`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoldenFields {
    /// Raw text for the `id` field (weight 6).
    pub id: String,
    /// Raw text for the `title` field (weight 4).
    pub title: String,
    /// Raw text for the `headings` field (weight 2).
    pub headings: String,
    /// Raw text for the `claim` field (weight 2).
    pub claim: String,
    /// Raw text for the `body` field (weight 1).
    pub body: String,
}

/// One unit entry in a ranked golden file (`tests/golden/README.md#ranked`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoldenUnit {
    /// Unit identifier as written (`CL-CMP-0011`, `cmp-withholding#the-rule`).
    pub id: String,
    /// Unit kind (`page`, `clause`, `source`, `claim`, `question`, `conflict`, `term`, `record`,
    /// `text`).
    pub kind: String,
    /// The five raw field strings to be lexed with [`index_tokens`].
    pub fields: GoldenFields,
    /// The seven non-searched display values.
    pub display: UnitDisplay,
    /// Optional unmodified NFC origin key (defaults to `""`).
    #[serde(default)]
    pub exact_key: String,
}

/// One document row in [`Index::docs`], holding the 10 string slots of
/// `docs/reference/search.md#the-site-index`:
/// `[id, kind, title, parent, snippet, status, as_of, effective_from, verified_on, exact_key]`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DocEntry {
    /// Unit identifier as written (`0`).
    pub id: String,
    /// Unit kind (`1`).
    pub kind: String,
    /// Display `title` (`2`).
    pub title: String,
    /// Display `parent` (`3`).
    pub parent: String,
    /// 160-code-point `snippet` (`4`).
    pub snippet: String,
    /// Display `status` (`5`).
    pub status: String,
    /// Display `as_of` (`6`).
    pub as_of: String,
    /// Display `effective_from` (`7`).
    pub effective_from: String,
    /// Display `verified_on` (`8`).
    pub verified_on: String,
    /// Unmodified NFC `origin_key` for company source/record units, or `""` otherwise (`9`).
    pub exact_key: String,
}

impl DocEntry {
    /// Returns the 10 string fields in the fixed site-index array order:
    /// `[id, kind, title, parent, snippet, status, as_of, effective_from, verified_on, exact_key]`.
    #[must_use]
    pub fn as_array(&self) -> [&str; 10] {
        [
            &self.id,
            &self.kind,
            &self.title,
            &self.parent,
            &self.snippet,
            &self.status,
            &self.as_of,
            &self.effective_from,
            &self.verified_on,
            &self.exact_key,
        ]
    }
}

impl Serialize for DocEntry {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(10))?;
        for field in self.as_array() {
            seq.serialize_element(field)?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for DocEntry {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let arr: [String; 10] = <[String; 10]>::deserialize(deserializer)?;
        let [
            id,
            kind,
            title,
            parent,
            snippet,
            status,
            as_of,
            effective_from,
            verified_on,
            exact_key,
        ] = arr;
        Ok(Self {
            id,
            kind,
            title,
            parent,
            snippet,
            status,
            as_of,
            effective_from,
            verified_on,
            exact_key,
        })
    }
}

/// One unit that holds a term: its unit number and the term's count in each of the five fields.
type TermHit = (usize, [usize; 5]);

/// The in-memory search index (`docs`, `terms`, `post`) (`docs/reference/search.md#the-site-index`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Index {
    /// One entry per unit in natural ID order; position is the unit number (`0..N`).
    pub docs: Vec<DocEntry>,
    /// Every distinct indexed term, sorted in UTF-16 code-unit order.
    pub terms: Vec<String>,
    /// Posting lists parallel to `terms`; each entry is `[unit, impact, unit, impact, ...]` with
    /// `unit` numbers ascending.
    pub post: Vec<Vec<u32>>,
}

impl Index {
    /// Builds an [`Index`] from `units` and `glossary` (`docs/reference/search.md#ranking`).
    #[must_use]
    pub fn build(mut units: Vec<Unit>, glossary: &Glossary) -> Self {
        let sets = glossary.synonym_sets();
        if !sets.is_empty() {
            for unit in &mut units {
                if !unit.synonyms_applied {
                    apply_synonym_sets(&sets, &mut unit.fields);
                    unit.synonyms_applied = true;
                }
            }
        }

        units.sort_by(|a, b| natural_id_cmp(&a.id, &b.id));

        let docs: Vec<DocEntry> = units
            .iter()
            .map(|u| DocEntry {
                id: u.id.clone(),
                kind: u.kind.clone(),
                title: u.display.title.clone(),
                parent: u.display.parent.clone(),
                snippet: u.display.snippet.clone(),
                status: u.display.status.clone(),
                as_of: u.display.as_of.clone(),
                effective_from: u.display.effective_from.clone(),
                verified_on: u.display.verified_on.clone(),
                exact_key: u.exact_key.nfc().collect(),
            })
            .collect();

        let avgl = average_field_lengths(&units);
        let num_units = units.len();

        // One pass over the units: per unit, count each term in each field; then merge into
        // term -> ascending unit list with those counts and the unit's field lengths.
        let mut lens_of: Vec<[usize; 5]> = Vec::with_capacity(num_units);
        let mut by_term: BTreeMap<&str, Vec<TermHit>> = BTreeMap::new();
        for (u_idx, unit) in units.iter().enumerate() {
            let mut local: BTreeMap<&str, [usize; 5]> = BTreeMap::new();
            let mut lens = [0usize; 5];
            for (f_idx, field) in unit.fields.as_array().into_iter().enumerate() {
                lens[f_idx] = field.len();
                for tok in field {
                    local.entry(tok.as_str()).or_default()[f_idx] += 1;
                }
            }
            lens_of.push(lens);
            for (term, counts) in local {
                by_term.entry(term).or_default().push((u_idx, counts));
            }
        }

        let mut entries: Vec<(&str, Vec<TermHit>)> = by_term.into_iter().collect();
        entries.sort_by(|a, b| utf16_cmp(a.0, b.0));

        let mut terms: Vec<String> = Vec::with_capacity(entries.len());
        let mut post: Vec<Vec<u32>> = Vec::with_capacity(entries.len());
        for (term, units_with_term) in entries {
            let term_idf = idf(num_units, units_with_term.len());
            let mut postings = Vec::with_capacity(units_with_term.len() * 2);
            for (u_idx, counts) in units_with_term {
                let tfp = tf_prime_from_counts(&counts, &lens_of[u_idx], &avgl);
                if tfp > 0.0 {
                    postings.push(u32::try_from(u_idx).unwrap_or(u32::MAX));
                    postings.push(impact(term_idf, tfp));
                }
            }
            terms.push(term.to_owned());
            post.push(postings);
        }

        Self { docs, terms, post }
    }

    /// Builds an [`Index`] from the `units` and `glossary` of a ranked golden file
    /// (`docs/reference/search.md#the-site-index`), preserving `exact_key` unmodified in NFC.
    #[must_use]
    pub fn from_golden(units: &[GoldenUnit], glossary: &Glossary) -> Self {
        let built: Vec<Unit> = units
            .iter()
            .map(|g| {
                let fields = UnitFields {
                    id: index_tokens(&g.fields.id),
                    title: index_tokens(&g.fields.title),
                    headings: index_tokens(&g.fields.headings),
                    claim: index_tokens(&g.fields.claim),
                    body: index_tokens(&g.fields.body),
                };
                let nfc_key: String = g.exact_key.nfc().collect();
                Unit::new(g.id.clone(), g.kind.clone(), fields)
                    .with_display(g.display.clone())
                    .with_exact_key(nfc_key)
            })
            .collect();
        Self::build(built, glossary)
    }

    /// Binary-searches `self.terms` in UTF-16 code-unit order and returns the term's index.
    #[must_use]
    pub fn term_index(&self, token: &str) -> Option<usize> {
        self.terms
            .binary_search_by(|cand| utf16_cmp(cand.as_str(), token))
            .ok()
    }

    /// Looks up the unit number (`0..N`) for `unit_id` in `self.docs`.
    #[must_use]
    pub fn unit_index(&self, unit_id: &str) -> Option<usize> {
        self.docs.iter().position(|d| d.id == unit_id)
    }

    /// Returns the stored integer impact for `(unit_id, token)`, or `None` if no posting exists.
    #[must_use]
    pub fn posting_impact(&self, unit_id: &str, token: &str) -> Option<u32> {
        let u_idx = u32::try_from(self.unit_index(unit_id)?).ok()?;
        let t_idx = self.term_index(token)?;
        let plist = self.post.get(t_idx)?;
        for pair in plist.as_chunks::<2>().0 {
            if pair[0] == u_idx {
                return Some(pair[1]);
            }
            if pair[0] > u_idx {
                break;
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::query::query;

    fn text_unit(id: &str, body: &[&str]) -> Unit {
        Unit::new(
            id,
            "text",
            UnitFields {
                body: body.iter().map(|t| (*t).to_owned()).collect(),
                ..UnitFields::default()
            },
        )
    }

    #[test]
    fn a_term_in_nearly_every_unit_of_a_large_index_still_has_an_impact_of_one() {
        // idf = ln(1 + 0.5 / (N + 0.5)) is about 0.0004 for N = 1200, so 1000 * score rounds to
        // 0 and only the minimum of 1 keeps the postings.
        let units: Vec<Unit> = (0..1200)
            .map(|i| text_unit(&format!("U-{i:04}"), &["common"]))
            .collect();
        let index = Index::build(units, &Glossary::default());
        assert_eq!(index.posting_impact("U-0000", "common"), Some(1));
        assert_eq!(index.posting_impact("U-1199", "common"), Some(1));
        let results = query(&index, "common", 5);
        assert_eq!(results.total, 1200);
        assert_eq!(results.hits.len(), 5);
        assert!(results.hits.iter().all(|h| h.score == 1 && h.matched == 1));
    }

    #[test]
    fn units_are_numbered_in_natural_id_order_and_terms_in_utf16_order() {
        let units = vec![
            text_unit("S-0042-v10", &["zeta"]),
            text_unit("Q-027", &["alpha"]),
            text_unit("S-0042-v2", &["\u{10000}"]),
            text_unit("q-3", &["\u{fa0e}"]),
            text_unit("Q-4", &["beta"]),
        ];
        let index = Index::build(units, &Glossary::default());
        let ids: Vec<&str> = index.docs.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(ids, ["q-3", "Q-4", "Q-027", "S-0042-v2", "S-0042-v10"]);
        // U+10000 is two UTF-16 units starting 0xD800, so it sorts before U+FA0E, although it is
        // after it by code point.
        assert_eq!(
            index.terms,
            ["alpha", "beta", "zeta", "\u{10000}", "\u{fa0e}"]
        );
        // Posting lists are parallel to the terms and name unit numbers in the order above.
        assert_eq!(index.unit_index("S-0042-v2"), Some(3));
        for (term, unit) in [
            ("alpha", "Q-027"),
            ("zeta", "S-0042-v10"),
            ("\u{fa0e}", "q-3"),
        ] {
            let t = index.term_index(term).expect("term");
            let u = index.unit_index(unit).expect("unit");
            assert_eq!(index.post[t].len(), 2, "{term}");
            assert_eq!(index.post[t][0] as usize, u, "{term}");
        }
    }
}
