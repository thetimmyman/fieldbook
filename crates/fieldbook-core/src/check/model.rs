//! [`Model`]: the immutable parsed Corpus state shared across check passes and `fieldbook-render`.

use crate::date::CivilDate;
use crate::schema::CorpusToml;

use super::tree::Tree;

/// Parsed state built once by [`super::load`] and shared across passes.
///
/// All fields are private; callers read them through [`Self::tree`], [`Self::today`] and [`Self::corpus`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    tree: Tree,
    today: CivilDate,
    corpus: Option<CorpusToml>,
}

impl Model {
    pub(crate) const fn new(tree: Tree, today: CivilDate, corpus: Option<CorpusToml>) -> Self {
        Self {
            tree,
            today,
            corpus,
        }
    }

    /// The in-memory Corpus [`Tree`].
    #[must_use]
    pub const fn tree(&self) -> &Tree {
        &self.tree
    }

    /// The calendar date supplied by the caller (`--today` or the executable's clock).
    #[must_use]
    pub const fn today(&self) -> CivilDate {
        self.today
    }

    /// The validated [`CorpusToml`], or `None` when `corpus/corpus.toml` raised `FB0100`.
    #[must_use]
    pub const fn corpus(&self) -> Option<&CorpusToml> {
        self.corpus.as_ref()
    }
}
