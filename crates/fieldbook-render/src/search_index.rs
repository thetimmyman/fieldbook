//! The edition's site search index, `assets/search-index.js` (`search-site-index`,
//! `docs/reference/search.md#the-site-index`).
//!
//! **Responsibility.** Build the public units of a [`LabelledView`], keep the page and section
//! units (the kinds `page` and `clause`) the edition's index holds, build the [`Index`] from that
//! unit set alone, and serialise it as one classic script that assigns `window.FB_INDEX`.
//!
//! **Rules.** Pure: no I/O. The units come from the permitted view only and every kept unit is
//! `public`. N, document frequencies, average field lengths and impacts are those of the kept unit
//! set: a full index is never built and then filtered. Equal views give equal bytes.

use std::collections::BTreeSet;

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

/// The unit kinds the edition's index holds: page leads and page sections.
pub const EDITION_KINDS: [&str; 2] = ["page", "clause"];

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

/// The units the edition's index holds: the `public` units of kind `page` or `clause`.
#[must_use]
pub fn edition_units(units: Vec<Unit>) -> Vec<Unit> {
    units
        .into_iter()
        .filter(|u| EDITION_KINDS.contains(&u.kind.as_str()) && labels_of(u).is_public())
        .collect()
}

/// Builds the edition's [`Index`] over the page and section units of `view` alone, with the URL
/// schemes removed from its display strings ([`strip_url_schemes`]).
#[must_use]
pub fn build(view: &LabelledView) -> Index {
    let (units, glossary) = view_units(view);
    let mut index = Index::build(edition_units(units), &glossary);
    for doc in &mut index.docs {
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

    use super::{strip_url_schemes, write};

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
