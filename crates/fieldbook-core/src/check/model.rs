//! [`Model`]: the immutable parsed Corpus state shared across check passes and `fieldbook-render`.

use crate::date::CivilDate;
use crate::schema::rows::CellProblem;
use crate::schema::table::{Row, Table};
use crate::schema::{
    ClaimRow, ConflictRow, CorpusToml, GlossaryRow, QuestionRow, Register, SourceRow,
};

use super::index::Index;
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

/// A parsed register row paired with its 1-based CSV line number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located<T> {
    /// 1-based line number of the row in its register file.
    pub line: u32,
    /// The typed row value.
    pub value: T,
}

pub(crate) type RowProblems = (u32, Vec<CellProblem>);

fn parse_rows<T, F>(table: Option<&Table>, parse: F) -> (Vec<Located<T>>, Vec<RowProblems>)
where
    F: Fn(&Row) -> Result<T, Vec<CellProblem>>,
{
    let Some(table) = table else {
        return (Vec::new(), Vec::new());
    };
    let mut typed = Vec::new();
    let mut problems = Vec::new();
    for row in &table.rows {
        match parse(row) {
            Ok(value) => typed.push(Located {
                line: row.line,
                value,
            }),
            Err(errs) => problems.push((row.line, errs)),
        }
    }
    (typed, problems)
}

/// Parsed state built once by [`super::load`] and shared across passes.
///
/// All fields are private; callers read them through [`Self::tree`], [`Self::today`],
/// [`Self::corpus`], [`Self::register`], [`Self::register_rows`] and the typed row accessors
/// ([`Self::sources`], [`Self::claims`], [`Self::questions`], [`Self::conflicts`],
/// [`Self::glossary`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    tree: Tree,
    today: CivilDate,
    corpus: Option<CorpusToml>,
    registers: [Option<Table>; 5],
    sources: Vec<Located<SourceRow>>,
    claims: Vec<Located<ClaimRow>>,
    questions: Vec<Located<QuestionRow>>,
    conflicts: Vec<Located<ConflictRow>>,
    glossary: Vec<Located<GlossaryRow>>,
    register_problems: [Vec<RowProblems>; 5],
    index: Index,
}

impl Model {
    pub(crate) fn new(tree: Tree, today: CivilDate, corpus: Option<CorpusToml>) -> Self {
        let registers = if corpus.is_some() {
            registers::load_tables(&tree)
        } else {
            [None, None, None, None, None]
        };

        let (sources, p0) = parse_rows(registers[0].as_ref(), SourceRow::from_row);
        let (claims, p1) = parse_rows(registers[1].as_ref(), ClaimRow::from_row);
        let (questions, p2) = parse_rows(registers[2].as_ref(), QuestionRow::from_row);
        let (conflicts, p3) = parse_rows(registers[3].as_ref(), ConflictRow::from_row);
        let (glossary, p4) = parse_rows(registers[4].as_ref(), GlossaryRow::from_row);

        let index = Index::build(
            corpus.as_ref(),
            &sources,
            &claims,
            &questions,
            &conflicts,
            &glossary,
        );

        Self {
            tree,
            today,
            corpus,
            registers,
            sources,
            claims,
            questions,
            conflicts,
            glossary,
            register_problems: [p0, p1, p2, p3, p4],
            index,
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

    /// Typed rows of `sources.csv` in register order, skipping any row with a [`CellProblem`].
    #[must_use]
    pub fn sources(&self) -> &[Located<SourceRow>] {
        &self.sources
    }

    /// Typed rows of `claims.csv` in register order, skipping any row with a [`CellProblem`].
    #[must_use]
    pub fn claims(&self) -> &[Located<ClaimRow>] {
        &self.claims
    }

    /// Typed rows of `questions.csv` in register order, skipping any row with a [`CellProblem`].
    #[must_use]
    pub fn questions(&self) -> &[Located<QuestionRow>] {
        &self.questions
    }

    /// Typed rows of `conflicts.csv` in register order, skipping any row with a [`CellProblem`].
    #[must_use]
    pub fn conflicts(&self) -> &[Located<ConflictRow>] {
        &self.conflicts
    }

    /// Typed rows of `glossary.csv` in register order, skipping any row with a [`CellProblem`].
    #[must_use]
    pub fn glossary(&self) -> &[Located<GlossaryRow>] {
        &self.glossary
    }

    /// The resolved identifier [`Index`] built from the typed register rows and `corpus.toml`.
    #[must_use]
    pub const fn index(&self) -> &Index {
        &self.index
    }

    /// Single-cell [`CellProblem`]s collected while building typed rows for `register`.
    #[must_use]
    pub(crate) fn register_problems(&self, register: Register) -> &[RowProblems] {
        &self.register_problems[register_index(register)]
    }
}
