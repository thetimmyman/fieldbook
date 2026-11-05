//! Fieldbook search tokenizer, synonym expansion and document units
//! (`docs/reference/search.md`).
//!
//! **Responsibility.** Normalise search text, lex identifier shapes and words on both the index
//! side and the query side, expand glossary synonyms into unit body fields, and construct the
//! five token fields of each document unit kind.
//!
//! **Rules.** Pure: no I/O, no clock, no environment, no `f64::ln`/`powf`/`powi`/`exp`.

pub mod lexer;
pub mod normalise;
pub mod synonyms;
pub mod units;

pub use lexer::{
    LEXER_SHAPES, LexedToken, LexerShape, MAX_QUERY_TOKENS, index_tokens, is_word_char, lex,
    lex_normalised, query_tokens,
};
pub use normalise::normalise;
pub use synonyms::{Glossary, GlossaryEntry, SynonymSet, apply_synonym_sets};
pub use units::{
    UNIT_KINDS, Unit, UnitFields, UnitKind, build_corpus_units, build_page_units,
    extract_headings_and_body_text, lookup_unit_kind, unit_href,
};
