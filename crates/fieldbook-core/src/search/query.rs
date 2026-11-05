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
    let norm_q = normalise(raw_query);
    let s = norm_q.trim();
    let tokens = query_tokens(s);
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
