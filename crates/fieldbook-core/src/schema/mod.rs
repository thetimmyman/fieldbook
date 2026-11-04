//! Corpus schema types, vocabularies, `corpus.toml`, page front-matter and register parsers.
//!
//! **Responsibility.** Hold the typed schema representations that are the source
//! of truth for `corpus/corpus.toml`, page front matter, closed vocabularies, and
//! the CSV registers.

pub mod conflicts;
pub mod corpus_toml;
pub mod glossary;
pub mod page;
pub mod page_types;
pub mod questions;
pub mod register;
pub mod rows;
pub mod sources;
pub mod table;
pub mod vocab;

pub use conflicts::ConflictRow;
pub use corpus_toml::{
    AreaDef, AssistantConfig, CorpusProblem, CorpusToml, CorpusTomlError, ViewDef, ViewFilter,
};
pub use glossary::GlossaryRow;
pub use page::{
    MetaError, MetaErrorKind, PageMeta, Split, SplitError, SplitErrorKind, parse_meta, split,
};
pub use page_types::template;
pub use questions::QuestionRow;
pub use register::{Column, ColumnKind, ColumnNeed, Register};
pub use rows::{CellProblem, CellProblemKind, RefId};
pub use sources::SourceRow;
pub use table::{Row, Table, TableError, TableErrorKind};
pub use vocab::{
    Adoption, AuthorityTier, ChangeClass, ConflictStatus, DOC_KINDS, DefinitionStatus, DocKind,
    FamilyBasis, Lifecycle, PageStatus, PageType, QuestionStatus, Readiness, SourceKind,
    SourceRole, VocabError, Volatility, WorkCanContinue, label_form, stream_id_form,
};
