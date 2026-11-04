//! [`Model`]: the immutable parsed Corpus state shared across check passes and `fieldbook-render`.

use crate::date::CivilDate;
use crate::schema::table::{Row, Table};
use crate::schema::{CorpusToml, Register};

use super::registers;
use super::tree::Tree;

const fn register_index(register: Register) -> usize {
    match register {
        Register::Sources => 0,
        Register::Claims => 1,
        Register::Questions => 2,
        Register::Conflicts => 3,
        Register::Glossary => 4,
    }
}

/// Parsed state built once by [`super::load`] and shared across passes.
///
/// All fields are private; callers read them through [`Self::tree`], [`Self::today`],
/// [`Self::corpus`], [`Self::register`] and [`Self::register_rows`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    tree: Tree,
    today: CivilDate,
    corpus: Option<CorpusToml>,
    registers: [Option<Table>; 5],
}

impl Model {
    pub(crate) fn new(tree: Tree, today: CivilDate, corpus: Option<CorpusToml>) -> Self {
        let registers = if corpus.is_some() {
            registers::load_tables(&tree)
        } else {
            [None, None, None, None, None]
        };
        Self {
            tree,
            today,
            corpus,
            registers,
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

    /// The parsed [`Table`] for `register`, or `None` when `register` failed `FB0200`, `FB0201`
    /// or `FB0202` (or when `corpus/corpus.toml` failed `FB0100`).
    #[must_use]
    pub const fn register(&self, register: Register) -> Option<&Table> {
        self.registers[register_index(register)].as_ref()
    }

    /// The data rows of `register` with their 1-based line numbers and per-cell 1-based columns,
    /// or `&[]` when `register` failed `FB0200`, `FB0201` or `FB0202`.
    #[must_use]
    pub fn register_rows(&self, register: Register) -> &[Row] {
        match &self.registers[register_index(register)] {
            Some(table) => &table.rows,
            None => &[],
        }
    }
}
