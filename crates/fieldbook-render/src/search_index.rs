//! The edition's site search index, `assets/search-index.js` (`search-site-index`,
//! `docs/reference/search.md#the-site-index`).
//!
//! **Responsibility.** Build the public units of a [`LabelledView`], keep the page and section
//! units (the kinds `page` and `clause`) and the glossary term units (the kind `term`) the
//! edition's index holds, build the [`Index`] from that unit set alone, and serialise it as one
//! classic script that assigns `window.FB_INDEX`.
//!
//! **Rules.** Pure: no I/O. The units come from the permitted view only and every kept unit is
//! `public`. Section snippets hold at most [`SECTION_SNIPPET_MAX`] code points. N, document
//! frequencies, average field lengths and impacts are those of the kept unit set: a full index is
//! never built and then filtered. Equal views give equal bytes.

use std::collections::{BTreeMap, BTreeSet};

use fieldbook_core::egress::LabelledView;
use fieldbook_core::ids::VersionId;
use fieldbook_core::labels::LabelSet;
use fieldbook_core::schema::vocab::Lifecycle;
use fieldbook_core::schema::{ClaimRow, ConflictRow, GlossaryRow, QuestionRow, SourceRow};
use fieldbook_core::search::units::kept_glossary;
use fieldbook_core::search::{Glossary, Index, Unit, build_corpus_units_with, labels_of};

/// Site-relative output path of the search index.
pub const OUTPUT_PATH: &str = "assets/search-index.js";

/// The site index format, the `v` key.
pub const FORMAT_VERSION: u32 = 1;

/// The most code points a section unit's snippet holds in the site index (`search-section-snippets`).
pub const SECTION_SNIPPET_MAX: usize = 100;

/// The unit kinds the edition's index holds: page leads and page sections, and glossary terms.
pub const EDITION_KINDS: [&str; 3] = ["page", "clause", "term"];

/// Every public unit of `view`, every kind, in natural id order, with the glossary of the kept
/// terms: the unit list before the edition's kind filter. A held (`quarantined`) source version
/// has no unit.
#[must_use]
pub fn view_units(view: &LabelledView) -> (Vec<Unit>, Glossary) {
    let sources: Vec<SourceRow> = view.sources().iter().map(|s| s.value.clone()).collect();
    let claims: Vec<ClaimRow> = view.claims().iter().map(|c| c.value.clone()).collect();
    let questions: Vec<QuestionRow> = view.questions().iter().map(|q| q.value.clone()).collect();
    let conflicts: Vec<ConflictRow> = view.conflicts().iter().map(|x| x.value.clone()).collect();
    let terms: Vec<GlossaryRow> = view.glossary().iter().map(|t| t.value.clone()).collect();

    let mut quarantined = BTreeSet::new();
    for row in &sources {
        if row.lifecycle == Lifecycle::Quarantined
            && let Ok(vid) = VersionId::parse(&format!("{}-v{}", row.source_id, row.version))
        {
            quarantined.insert(vid);
        }
    }

    let glossary = kept_glossary(&terms, &sources, &claims, LabelSet::is_public);
    let units = build_corpus_units_with(
        view.pages(),
        &sources,
        &claims,
        &questions,
        &conflicts,
        &terms,
        &quarantined,
        &glossary,
        LabelSet::is_public,
    );
    (units, glossary)
}

/// The units the edition's index holds: the `public` units of kind `page`, `clause` or `term`.
#[must_use]
pub fn edition_units(units: Vec<Unit>) -> Vec<Unit> {
    units
        .into_iter()
        .filter(|u| EDITION_KINDS.contains(&u.kind.as_str()) && labels_of(u).is_public())
        .collect()
}

/// Cuts a section unit's snippet to at most [`SECTION_SNIPPET_MAX`] code points
/// (`search-section-snippets`).
///
/// Returns `snippet.to_owned()` unchanged unless `id` contains `'#'` and
/// `snippet.chars().count() > SECTION_SNIPPET_MAX`. Otherwise takes the first
/// [`SECTION_SNIPPET_MAX`] - 1 code points, cuts back to the last space if there is one, trims,
/// and appends `…` (`U+2026`).
#[must_use]
pub fn cut_section_snippet(id: &str, snippet: &str) -> String {
    if !id.contains('#') || snippet.chars().count() <= SECTION_SNIPPET_MAX {
        return snippet.to_owned();
    }
    let first: String = snippet.chars().take(SECTION_SNIPPET_MAX - 1).collect();
    let prefix = match first.rfind(' ') {
        Some(space_idx) => first[..space_idx].trim(),
        None => first.trim(),
    };
    format!("{prefix}\u{2026}")
}

/// Builds the edition's [`Index`] over the page, section and term units of `view` alone. Section
/// snippets are cut to [`SECTION_SNIPPET_MAX`] code points ([`cut_section_snippet`]). A term's
/// entry carries its aliases as written, joined with `|`, in the `parent` position, which is empty
/// for a term otherwise. The URL schemes are removed last from every display string
/// ([`strip_url_schemes`]).
#[must_use]
pub fn build(view: &LabelledView) -> Index {
    let (units, glossary) = view_units(view);
    let mut index = Index::build(edition_units(units), &glossary);
    let aliases: BTreeMap<&str, String> = view
        .glossary()
        .iter()
        .map(|t| (t.value.term_id.as_str(), t.value.aliases.join("|")))
        .collect();
    for doc in &mut index.docs {
        doc.snippet = cut_section_snippet(&doc.id, &doc.snippet);
        if doc.kind == "term"
            && let Some(joined) = aliases.get(doc.id.as_str())
        {
            doc.parent.clone_from(joined);
        }
        for field in [
            &mut doc.id,
            &mut doc.kind,
            &mut doc.title,
            &mut doc.parent,
            &mut doc.snippet,
            &mut doc.status,
            &mut doc.as_of,
            &mut doc.effective_from,
            &mut doc.verified_on,
            &mut doc.exact_key,
        ] {
            *field = strip_url_schemes(field);
        }
    }
    index
}

/// Removes every `http://` and `https://` (ASCII case-insensitive) from `s`, so
/// `see https://sam.gov/x` reads `see sam.gov/x`. Repeats until none is left, so a removal cannot
/// join two halves into a new scheme.
#[must_use]
pub fn strip_url_schemes(s: &str) -> String {
    let mut out = s.to_owned();
    loop {
        let lower = out.to_ascii_lowercase();
        let Some(at) = lower.find("http://").or_else(|| lower.find("https://")) else {
            return out;
        };
        let len = if lower[at..].starts_with("https://") {
            8
        } else {
            7
        };
        out.replace_range(at..at + len, "");
    }
}

/// Renders the bytes of `assets/search-index.js` for `view`.
#[must_use]
pub fn render(view: &LabelledView) -> String {
    write(&build(view))
}

/// Serialises `index` as `window.FB_INDEX = {"v":1,"docs":[...],"terms":[...],"post":[...]};`:
/// compact JSON, keys in that order, no spaces.
#[must_use]
pub fn write(index: &Index) -> String {
    let mut out = String::from("window.FB_INDEX = {\"v\":");
    out.push_str(&FORMAT_VERSION.to_string());
    out.push_str(",\"docs\":[");
    for (i, doc) in index.docs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('[');
        for (j, field) in doc.as_array().into_iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            push_json_str(&mut out, field);
        }
        out.push(']');
    }
    out.push_str("],\"terms\":[");
    for (i, term) in index.terms.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_json_str(&mut out, term);
    }
    out.push_str("],\"post\":[");
    for (i, list) in index.post.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('[');
        for (j, n) in list.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            out.push_str(&n.to_string());
        }
        out.push(']');
    }
    out.push_str("]};");
    out
}

/// Appends `s` as a JSON string literal. Control characters, U+2028 and U+2029 are escaped, so the
/// literal is also a valid string in any JavaScript engine.
fn push_json_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 || c == '\u{2028}' || c == '\u{2029}' => {
                out.push_str(&format!("\\u{:04x}", u32::from(c)));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use fieldbook_core::search::{DocEntry, Index};

    use super::{cut_section_snippet, strip_url_schemes, write};

    #[test]
    fn cut_leaves_page_lead_whole() {
        let snippet = "a".repeat(160);
        assert_eq!(cut_section_snippet("p-1", &snippet), snippet);
    }

    #[test]
    fn cut_keeps_section_snippet_of_exactly_100() {
        let snippet = "a".repeat(100);
        assert_eq!(cut_section_snippet("p-1#a", &snippet), snippet);
    }

    #[test]
    fn cut_shortens_101_code_point_section_snippet() {
        let snippet = format!("{}ab", "ab ".repeat(33));
        assert_eq!(snippet.chars().count(), 101);
        let res = cut_section_snippet("p-1#a", &snippet);
        assert!(res.chars().count() <= 100);
        assert!(res.ends_with('\u{2026}'));
    }

    #[test]
    fn cut_without_space_takes_99_and_stays_at_100() {
        let snippet = "a".repeat(150);
        let expected = format!("{}\u{2026}", "a".repeat(99));
        let res = cut_section_snippet("p-1#a", &snippet);
        assert_eq!(res, expected);
        assert_eq!(res.chars().count(), 100);
    }

    #[test]
    fn cut_counts_code_points_not_bytes() {
        let snippet_two_byte = "\u{e9}".repeat(120);
        let expected_two_byte = format!("{}\u{2026}", "\u{e9}".repeat(99));
        let res_two_byte = cut_section_snippet("p-1#a", &snippet_two_byte);
        assert_eq!(res_two_byte, expected_two_byte);
        assert_eq!(res_two_byte.chars().count(), 100);

        let snippet_astral = "\u{10000}".repeat(120);
        let expected_astral = format!("{}\u{2026}", "\u{10000}".repeat(99));
        let res_astral = cut_section_snippet("p-1#a", &snippet_astral);
        assert_eq!(res_astral, expected_astral);
        assert_eq!(res_astral.chars().count(), 100);
    }

    #[test]
    fn cut_goes_back_to_last_space() {
        let snippet = "aaaa ".repeat(30);
        assert_eq!(snippet.chars().count(), 150);
        let res = cut_section_snippet("p-1#a", &snippet);
        assert_eq!(res, format!("{}\u{2026}", ["aaaa"; 19].join(" ")));
        assert_eq!(res.chars().count(), 95);
        assert!(!res.ends_with(" \u{2026}"));
    }

    #[test]
    fn cut_threshold_counts_code_points_not_bytes() {
        let snippet = "\u{e9}".repeat(100);
        assert_eq!(snippet.len(), 200);
        assert_eq!(cut_section_snippet("p-1#a", &snippet), snippet);
    }

    #[test]
    fn strip_url_schemes_removes_every_scheme() {
        assert_eq!(
            strip_url_schemes("see https://sam.gov/x and HTTP://a.b"),
            "see sam.gov/x and a.b"
        );
        assert_eq!(strip_url_schemes("hthttp://tp://x"), "x");
        assert_eq!(strip_url_schemes("no scheme: a/b"), "no scheme: a/b");
        assert_eq!(strip_url_schemes("caf\u{e9} https://x"), "caf\u{e9} x");
    }

    #[test]
    fn write_is_compact_json_with_fixed_key_order() {
        let index = Index {
            docs: vec![DocEntry {
                id: "p-1".to_owned(),
                kind: "page".to_owned(),
                title: "A \"quoted\" \\ title\u{2028}".to_owned(),
                ..DocEntry::default()
            }],
            terms: vec!["a".to_owned(), "b".to_owned()],
            post: vec![vec![0, 12], vec![0, 3]],
        };
        assert_eq!(
            write(&index),
            "window.FB_INDEX = {\"v\":1,\"docs\":[[\"p-1\",\"page\",\"A \\\"quoted\\\" \\\\ title\\u2028\",\"\",\"\",\"\",\"\",\"\",\"\",\"\"]],\"terms\":[\"a\",\"b\"],\"post\":[[0,12],[0,3]]};"
        );
        let json = write(&index)
            .strip_prefix("window.FB_INDEX = ")
            .and_then(|s| s.strip_suffix(';'))
            .map(str::to_owned)
            .unwrap_or_default();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
        assert_eq!(parsed["docs"][0][2], "A \"quoted\" \\ title\u{2028}");
    }
}
