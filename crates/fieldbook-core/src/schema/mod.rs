//! Corpus schema types, vocabularies, `corpus.toml`, page front-matter and register parsers.
//!
//! **Responsibility.** Hold the typed schema representations that are the source
//! of truth for `corpus/corpus.toml`, page front matter, closed vocabularies, and
//! the CSV registers.

pub mod claims;
pub mod conflicts;
pub mod corpus_toml;
pub mod glossary;
pub mod locator;
pub mod page;
pub mod page_types;
pub mod questions;
pub mod register;
pub mod rows;
pub mod sources;
pub mod table;
pub mod vocab;

pub use claims::{
    AppliesTo, AppliesToEvent, AppliesToProblem, ClaimDraft, ClaimRow, ClaimUnit,
    StructuredAppliesTo, is_raise,
};
pub use conflicts::ConflictRow;
pub use corpus_toml::{
    AreaDef, AssistantConfig, CorpusProblem, CorpusToml, CorpusTomlError, ViewDef, ViewFilter,
};
pub use glossary::GlossaryRow;
pub use locator::{
    EXTENSION_REGISTRY, ExtensionKindEntry, ExtensionResolution, Locator, LocatorError,
    LocatorKind, decode_extension_payload, encode_extension_payload, is_valid_extension_kind,
    lookup_extension_kind,
};
pub use page::{
    MetaError, MetaErrorKind, PageFormatError, PageMeta, Split, SplitError, SplitErrorKind,
    parse_meta, split,
};
pub use page_types::template;
pub use questions::QuestionRow;
pub use register::{Column, ColumnKind, ColumnNeed, Register};
pub use rows::{CellProblem, CellProblemKind, RefId};
pub use sources::SourceRow;
pub use table::{Row, Table, TableError, TableErrorKind};
pub use vocab::{
    Adoption, Applicability, AuthorityTier, BuiltinLocatorKind, ChangeClass, ClaimKind,
    ConflictStatus, DOC_KINDS, DataOrigin, DefinitionStatus, DocKind, DraftedBy, EvidenceGrade,
    FamilyBasis, Instrument, Lifecycle, PageStatus, PageType, QuestionStatus, Readiness,
    SourceKind, SourceRole, TextStatus, Verification, VerifiedByRole, VocabError, Volatility,
    WorkCanContinue, label_form, stream_id_form,
};
