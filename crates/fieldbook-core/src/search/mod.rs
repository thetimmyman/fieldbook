//! Fieldbook search tokenizer, synonym expansion, document units, BM25F ranking, index builder,
//! and pure integer query execution (`docs/reference/search.md`).
//!
//! **Responsibility.** Normalise search text, lex identifier shapes and words on both the index
//! side and the query side, expand glossary synonyms into unit body fields, construct the
//! five token fields and display metadata of each document unit kind, compute deterministic BM25F
//! integer impacts, build the in-memory [`Index`], and answer queries with integer math only.
//!
//! **Rules.** Pure: no I/O, no clock, no environment, no `f64::ln`/`powf`/`powi`/`exp`.

pub mod bm25f;
pub mod index;
pub mod lexer;
pub mod normalise;
pub mod query;
pub mod synonyms;
pub mod units;

pub use bm25f::{B, FIELD_WEIGHTS, K1, average_field_lengths, idf, impact, ln, score, tf_prime};
pub use index::{DocEntry, GoldenFields, GoldenUnit, Index};
pub use lexer::{
    LEXER_SHAPES, LexedToken, LexerShape, MAX_QUERY_TOKENS, index_tokens, is_word_char, lex,
    lex_normalised, query_tokens,
};
pub use normalise::normalise;
pub use query::{Hit, MAX_PREFIX_TERMS, MIN_PREFIX_UTF16_LEN, Results, is_exact_class, query};
pub use synonyms::{Glossary, GlossaryEntry, SynonymSet, apply_synonym_sets};
pub use units::{
    UNIT_KINDS, Unit, UnitDisplay, UnitFields, UnitKind, build_corpus_units,
    build_corpus_units_with, build_page_units, build_shipped_corpus_units,
    extract_first_paragraph_text, extract_headings_and_body_text, labels_of, lookup_unit_kind,
    make_snippet, natural_id_cmp, unit_href, utf16_cmp,
};

#[cfg(test)]
#[test]
fn quarantined_source_rows_have_no_units() {
    use std::collections::BTreeSet;

    use crate::check::Tree;
    use crate::ids::VersionId;
    use crate::labels::LabelSet;
    use crate::manifest::shipped_corpus;

    const MINI_CORPUS_TOML: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/corpus.toml");

    // Three versions with distinct canaries:
    // - S-0001-v1: active and not named, so it stays searchable;
    // - S-0001-v2: `lifecycle=quarantined` but not named in `quarantined` (the row guard alone);
    // - S-0002-v1: `lifecycle=active` but named in `quarantined` (the set guard alone).
    let header = "source_id,version,tag,title,publisher,source_kind,source_role,authority_tier,licence,source_family,family_basis,location,sha256,published,effective_from,as_of,retrieved,lifecycle,supersedes,volatility,handling,note,purpose,origin_system,origin_stream,origin_key,doc_kind,series\n";
    let row = |id: &str, ver: u32, sha: char, lifecycle: &str, title: &str, note: &str| {
        format!(
            "{id},{ver},synthetic-cost-rule,{title},Synthetic Standards Board,primary_law,primary_evidence,A,\"Synthetic, no restrictions\",synthetic_cost_rule,reviewed,{id}-v{ver}.txt,{},2026-01-10,2026-01-15,2026-02-01,2026-02-01,{lifecycle},,V2,public,{note},Synthetic fixture row,,,,,\n",
            sha.to_string().repeat(64),
        )
    };
    let sources_csv = [
        header.to_owned(),
        row(
            "S-0001",
            1,
            '1',
            "active",
            "Active regulation alphatitlecanary",
            "Active note alphanotecanary",
        ),
        row(
            "S-0001",
            2,
            '2',
            "quarantined",
            "Held regulation betatitlecanary",
            "Held note betanotecanary",
        ),
        row(
            "S-0002",
            1,
            '3',
            "active",
            "Named regulation gammatitlecanary",
            "Named note gammanotecanary",
        ),
    ]
    .concat();

    let mut tree = Tree::new();
    tree.insert_dir("corpus");
    tree.insert_dir("corpus/registers");
    tree.insert_dir("corpus/sources");
    tree.insert_dir("corpus/sources/text");
    tree.insert_dir("corpus/sources/text/S-0001-v1");
    tree.insert_dir("corpus/sources/text/S-0002-v1");
    tree.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes().to_vec());
    tree.insert_bytes(
        "corpus/registers/sources.csv",
        sources_csv.as_bytes().to_vec(),
    );
    tree.insert_bytes(
        "corpus/sources/text/S-0001-v1/p0001.txt",
        b"Active text".to_vec(),
    );
    tree.insert_bytes(
        "corpus/sources/text/S-0002-v1/p0001.txt",
        b"Named text delta_text_canary".to_vec(),
    );

    let mut quarantined = BTreeSet::new();
    quarantined.insert(VersionId::parse("S-0002-v1").expect("valid version id"));

    // `shipped_corpus` still selects `corpus/registers/sources.csv` and retains every row.
    let shipped = shipped_corpus(&tree, &quarantined).expect("shipped_corpus succeeds");
    assert!(
        shipped
            .iter()
            .any(|(p, _)| p == "corpus/registers/sources.csv"),
        "shipped_corpus must still select corpus/registers/sources.csv"
    );
    assert!(
        !shipped.iter().any(|(p, _)| p.contains("S-0002-v1")),
        "shipped_corpus must exclude the named version's source text files"
    );

    let (units, glossary) = build_shipped_corpus_units(&tree, &quarantined, LabelSet::is_public)
        .expect("build_shipped_corpus_units succeeds");
    assert_eq!(units.len(), 1, "units: {units:?}");
    assert_eq!(units[0].id, "S-0001-v1");

    let index = Index::build(units, &glossary);

    // Active content remains searchable by title canary, note canary, and unversioned source ID.
    let res_title = query(&index, "alphatitlecanary", 10);
    assert_eq!(res_title.hits.len(), 1);
    assert_eq!(res_title.hits[0].id, "S-0001-v1");

    let res_note = query(&index, "alphanotecanary", 10);
    assert_eq!(res_note.hits.len(), 1);
    assert_eq!(res_note.hits[0].id, "S-0001-v1");

    let res_sid = query(&index, "S-0001", 10);
    assert_eq!(res_sid.hits.len(), 1);
    assert_eq!(res_sid.hits[0].id, "S-0001-v1");

    // Neither held version has a unit, a snippet or a posting, whichever rule held it.
    let held = ["S-0001-v2", "S-0002-v1"];
    let canaries = [
        "betatitlecanary",
        "betanotecanary",
        "gammatitlecanary",
        "gammanotecanary",
    ];
    for id in held {
        assert!(
            !index.docs.iter().any(|d| d.id == id),
            "held unit {id} must not appear in index.docs"
        );
        assert!(
            query(&index, id, 10).hits.is_empty(),
            "held id {id} must find nothing"
        );
    }
    for doc in &index.docs {
        for canary in canaries {
            assert!(
                !doc.title.contains(canary) && !doc.snippet.contains(canary),
                "held canary {canary} leaked into doc entry: {doc:?}"
            );
        }
    }
    for canary in canaries {
        assert!(
            !index.terms.iter().any(|t| t == canary),
            "held canary {canary} must not appear in index.terms: {:?}",
            index.terms
        );
        assert!(query(&index, canary, 10).hits.is_empty());
    }
    for term in held.map(str::to_lowercase) {
        assert!(!index.terms.contains(&term));
    }
}
