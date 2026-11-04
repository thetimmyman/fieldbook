//! Corpus schema types, vocabularies, `corpus.toml` and page front-matter parsers.
//!
//! **Responsibility.** Hold the typed schema representations that are the source
//! of truth for `corpus/corpus.toml`, page front matter, and closed vocabularies.

pub mod corpus_toml;
pub mod page;
pub mod page_types;
pub mod vocab;

pub use corpus_toml::{
    AreaDef, AssistantConfig, CorpusProblem, CorpusToml, CorpusTomlError, ViewDef, ViewFilter,
};
pub use page::{
    MetaError, MetaErrorKind, PageMeta, Split, SplitError, SplitErrorKind, parse_meta, split,
};
pub use page_types::template;
pub use vocab::{ChangeClass, PageStatus, PageType, Readiness, VocabError, Volatility, label_form};
