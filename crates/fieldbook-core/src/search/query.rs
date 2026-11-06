//! Pure integer query execution and ranking (`docs/reference/search.md#query-procedure`).
//!
//! No floating-point operations are performed at query time.

use super::index::{DocEntry, Index};
use super::lexer::query_tokens;
use super::normalise::normalise;

/// Maximum number of prefix-matching terms examined for the last query token (`docs/reference/search.md#budgets`).
pub const MAX_PREFIX_TERMS: usize = 64;

/// Minimum length in UTF-16 code units for the last query token to trigger prefix matching.
pub const MIN_PREFIX_UTF16_LEN: usize = 2;

/// One ranked search hit returned by [`query`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// Unit number (`0..N`) in [`Index::docs`] (assigned in natural ID order).
    pub unit: usize,
    /// Unit identifier as written (`doc.id`).
    pub id: String,
    /// Unit kind (`doc.kind`).
    pub kind: String,
    /// Number of query tokens this unit scored for (`matched`).
    pub matched: usize,
    /// Summed integer impact across matched query tokens (`sum`).
    pub score: u32,
    /// `true` when this unit is in the exact class for the query.
    pub exact: bool,
}

/// Ranked results returned by [`query`], truncated to `limit` while retaining `total`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Results {
    /// Ranked hits in query-procedure order, truncated to `limit`.
    pub hits: Vec<Hit>,
    /// Total number of matching units before truncation.
    pub total: usize,
}

/// Returns `true` when `doc` belongs to the exact class for the normalized, trimmed query `s`
/// (`docs/reference/search.md#query-procedure`):
/// - `s` equals `doc.id` folded (`normalise`), OR
/// - `doc.kind` is `"source"` or `"record"` and `s` equals the unversioned stem of `doc.id`
///   folded (before `-v<digits>`), OR
/// - `doc.exact_key` is non-empty and `s` equals `doc.exact_key` folded and trimmed.
#[must_use]
pub fn is_exact_class(doc: &DocEntry, s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let folded_id = normalise(&doc.id);
    let folded_id = folded_id.trim();
    if s == folded_id {
        return true;
    }
    if (doc.kind == "source" || doc.kind == "record")
        && let Some((unver, ver)) = folded_id.rsplit_once("-v")
        && !unver.is_empty()
        && !ver.is_empty()
        && ver.bytes().all(|b| b.is_ascii_digit())
        && s == unver
    {
        return true;
    }
    if !doc.exact_key.is_empty() {
        let folded_key = normalise(&doc.exact_key);
        let folded_key = folded_key.trim();
        if !folded_key.is_empty() && s == folded_key {
            return true;
        }
    }
    false
}

/// Executes the search query procedure over `index` using integer arithmetic only
/// (`docs/reference/search.md#query-procedure`), returning at most `limit` hits and the `total`
/// match count.
#[must_use]
pub fn query(index: &Index, raw_query: &str, limit: usize) -> Results {
    // Normalisation runs once. `normalise` is not idempotent (deleting U+00AD can leave a base
    // letter and a combining mark adjacent, which a second NFKC would compose), and the index
    // side normalises once, so the tokens come from the raw query and only `s` is derived here.
    let norm_q = normalise(raw_query);
    let s = norm_q.trim();
    let tokens = query_tokens(raw_query);
    if tokens.is_empty() || index.docs.is_empty() {
        return Results::default();
    }

    let num_units = index.docs.len();
    let mut matched = vec![0usize; num_units];
    let mut sum = vec![0u32; num_units];
    let mut unit_tok_score = vec![0u32; num_units];

    for (tok_idx, token) in tokens.iter().enumerate() {
        unit_tok_score.fill(0);

        if let Some(t_idx) = index.term_index(token)
            && let Some(plist) = index.post.get(t_idx)
        {
            for pair in plist.as_chunks::<2>().0 {
                let u = pair[0] as usize;
                let imp = pair[1];
                if u < num_units && imp > unit_tok_score[u] {
                    unit_tok_score[u] = imp;
                }
            }
        }

        let is_last = tok_idx + 1 == tokens.len();
        if is_last && token.encode_utf16().count() >= MIN_PREFIX_UTF16_LEN {
            let mut prefix_count = 0usize;
            for (t_idx, term) in index.terms.iter().enumerate() {
                if term != token && term.starts_with(token.as_str()) {
                    if let Some(plist) = index.post.get(t_idx) {
                        for pair in plist.as_chunks::<2>().0 {
                            let u = pair[0] as usize;
                            let half = pair[1] / 2;
                            if u < num_units && half > unit_tok_score[u] {
                                unit_tok_score[u] = half;
                            }
                        }
                    }
                    prefix_count += 1;
                    if prefix_count >= MAX_PREFIX_TERMS {
                        break;
                    }
                }
            }
        }

        for u in 0..num_units {
            let tok_score = unit_tok_score[u];
            if tok_score > 0 {
                matched[u] += 1;
                sum[u] = sum[u].saturating_add(tok_score);
            }
        }
    }

    let mut hits = Vec::new();
    for (u, doc) in index.docs.iter().enumerate() {
        let exact = is_exact_class(doc, s);
        if exact || matched[u] > 0 {
            hits.push(Hit {
                unit: u,
                id: doc.id.clone(),
                kind: doc.kind.clone(),
                matched: matched[u],
                score: sum[u],
                exact,
            });
        }
    }

    hits.sort_by(|a, b| {
        b.exact.cmp(&a.exact).then_with(|| {
            if a.exact {
                a.unit.cmp(&b.unit)
            } else {
                b.matched
                    .cmp(&a.matched)
                    .then_with(|| b.score.cmp(&a.score))
                    .then_with(|| a.unit.cmp(&b.unit))
            }
        })
    });

    let total = hits.len();
    hits.truncate(limit);
    Results { hits, total }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::lexer::index_tokens;
    use crate::search::synonyms::Glossary;
    use crate::search::units::{Unit, UnitFields};

    fn text_unit(id: &str, body: &str) -> Unit {
        Unit::new(
            id,
            "text",
            UnitFields {
                id: Vec::new(),
                title: Vec::new(),
                headings: Vec::new(),
                claim: Vec::new(),
                body: index_tokens(body),
            },
        )
    }

    fn doc(id: &str) -> DocEntry {
        DocEntry {
            id: id.to_owned(),
            kind: "text".to_owned(),
            ..DocEntry::default()
        }
    }

    fn ids(results: &Results) -> Vec<&str> {
        results.hits.iter().map(|h| h.id.as_str()).collect()
    }

    #[test]
    fn the_query_is_normalised_once_because_normalise_is_not_idempotent() {
        // Deleting the soft hyphen leaves `e` beside a combining acute. A second NFKC would
        // compose them, so a query that normalised twice would look for the composed word.
        let raw = "cafe\u{ad}\u{301}";
        assert_ne!(normalise(&normalise(raw)), normalise(raw));

        let index = Index::build(
            vec![
                text_unit("plain", "cafe"),
                text_unit("composed", "caf\u{e9}"),
            ],
            &Glossary::default(),
        );
        assert_eq!(ids(&query(&index, raw, 10)), ["plain"]);
        assert_eq!(ids(&query(&index, "caf\u{e9}", 10)), ["composed"]);
    }

    #[test]
    fn total_counts_every_match_while_limit_cuts_the_list() {
        let units = (1..=12)
            .map(|n| text_unit(&format!("n{n:02}"), "ledger"))
            .collect();
        let index = Index::build(units, &Glossary::default());

        let ten = query(&index, "ledger", 10);
        assert_eq!(ten.hits.len(), 10);
        assert_eq!(ten.total, 12);
        assert_eq!(ten.hits[0].id, "n01");
        assert_eq!(ten.hits[9].id, "n10");

        let three = query(&index, "ledger", 3);
        assert_eq!((three.hits.len(), three.total), (3, 12));

        for limit in [12, 13, 50] {
            let all = query(&index, "ledger", limit);
            assert_eq!((all.hits.len(), all.total), (12, 12), "limit {limit}");
        }

        let none = query(&index, "absent", 10);
        assert_eq!((none.hits.len(), none.total), (0, 0));
    }

    #[test]
    fn total_counts_an_exact_class_unit_that_has_no_score() {
        // A section unit has an empty `id` field, so its exact id scores nothing yet is a result.
        let index = Index::build(
            vec![
                text_unit("page#rule", "withholding"),
                text_unit("other", "ledger"),
            ],
            &Glossary::default(),
        );
        let found = query(&index, "page#rule", 10);
        assert_eq!(ids(&found), ["page#rule"]);
        assert_eq!(found.total, 1);
        assert_eq!(found.hits[0].score, 0);
        assert_eq!(found.hits[0].matched, 0);
        assert!(found.hits[0].exact);
    }

    #[test]
    fn a_prefix_only_hit_whose_halved_impact_is_zero_is_dropped() {
        // `docs/reference/search.md#query-procedure` says a prefix term scores half its impact by
        // integer division and is silent on an impact of 1, which halves to 0. This pins the
        // current behaviour (such a unit does not score for the token and is not a result);
        // it is not a decision. The exact term with impact 1 still counts, and 2 halves to 1.
        let index = Index {
            docs: vec![doc("u0"), doc("u1"), doc("u2")],
            terms: vec!["abcd".to_owned(), "abce".to_owned(), "other".to_owned()],
            post: vec![vec![0, 1], vec![1, 2], vec![2, 9]],
        };

        let prefix = query(&index, "abc", 10);
        assert_eq!(ids(&prefix), ["u1"]);
        assert_eq!(prefix.total, 1);
        assert_eq!((prefix.hits[0].matched, prefix.hits[0].score), (1, 1));

        let exact = query(&index, "abcd", 10);
        assert_eq!(ids(&exact), ["u0"]);
        assert_eq!((exact.hits[0].matched, exact.hits[0].score), (1, 1));
    }
}
