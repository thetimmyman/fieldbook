//! [`Model`]: the immutable parsed Corpus state shared across check passes and `fieldbook-render`.

use std::collections::BTreeMap;

use crate::cite::{self, Citation, KeyKind, LexFinding, Mode};
use crate::date::CivilDate;
use crate::ids::{ClaimId, ConflictId, PageId, QuestionId, SourceId, VersionId};
use crate::schema::page::{self, MetaError, PageMeta, SplitError};
use crate::schema::rows::CellProblem;
use crate::schema::table::{Row, Table};
use crate::schema::vocab::Lifecycle;
use crate::schema::{
    ClaimRow, ConflictRow, CorpusToml, GlossaryRow, QuestionRow, Register, SourceRow,
};

use super::Code;
use super::files;
use super::index::{Index, SourceVersionEntry};
use super::page_parts::{self, Parts};
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

/// A parsed page in [`Model`], sorted by [`PageId`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageEntry {
    /// The page's identifier (`meta.id`).
    pub id: PageId,
    /// Relative path in `corpus/` (`corpus/pages/<area-slug>/<subarea>/<page-id>.md`).
    pub path: String,
    /// Parsed TOML front matter.
    pub meta: PageMeta,
    /// Markdown body following the blank line after the closing `+++` fence.
    pub body: String,
    /// 1-based line number in the page file where `body` starts.
    pub body_line: u32,
    parts: Parts,
}

impl PageEntry {
    /// The parsed [`Parts`] of this page's body.
    #[must_use]
    pub const fn parts(&self) -> &Parts {
        &self.parts
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RawPageOutcome {
    SplitErr(SplitError),
    MetaErr(MetaError),
    Parsed(Box<PageMeta>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawPageRecord {
    pub path: String,
    pub area_folder: String,
    pub subarea_folder: String,
    pub stem: String,
    pub outcome: RawPageOutcome,
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

fn col_index(reg: Register, col_name: &str) -> usize {
    reg.columns()
        .iter()
        .position(|c| c.name == col_name)
        .unwrap_or(0)
}

fn cell_str<'a>(reg: Register, row: &'a Row, col_name: &str) -> &'a str {
    let idx = col_index(reg, col_name);
    row.cells.get(idx).map_or("", String::as_str)
}

fn build_citation_resolve_index(index: &Index, registers: &[Option<Table>; 5]) -> Index {
    let mut out = index.clone();

    if let Some(table) = &registers[register_index(Register::Sources)] {
        for row in &table.rows {
            if let (Ok(sid), Ok(ver)) = (
                SourceId::parse(cell_str(Register::Sources, row, "source_id")),
                cell_str(Register::Sources, row, "version").parse::<u32>(),
            ) && let Ok(vid) = VersionId::parse(&format!("{sid}-v{ver}"))
            {
                let lifecycle = Lifecycle::parse(cell_str(Register::Sources, row, "lifecycle"))
                    .unwrap_or(Lifecycle::Active);
                out.source_versions
                    .entry(vid)
                    .or_insert(SourceVersionEntry {
                        line: row.line,
                        lifecycle,
                        superseded: false,
                    });
            }
        }
    }

    if let Some(table) = &registers[register_index(Register::Claims)] {
        for row in &table.rows {
            if let Ok(cid) = ClaimId::parse(cell_str(Register::Claims, row, "claim_id")) {
                out.claims.entry(cid).or_insert(row.line);
            }
        }
    }

    if let Some(table) = &registers[register_index(Register::Questions)] {
        for row in &table.rows {
            if let Ok(qid) = QuestionId::parse(cell_str(Register::Questions, row, "question_id")) {
                out.questions.entry(qid).or_insert(row.line);
            }
        }
    }

    if let Some(table) = &registers[register_index(Register::Conflicts)] {
        for row in &table.rows {
            if let Ok(xcid) = ConflictId::parse(cell_str(Register::Conflicts, row, "conflict_id")) {
                out.conflicts.entry(xcid).or_insert(row.line);
            }
        }
    }

    out
}

fn split_page_path(path: &str) -> Option<(String, String, String)> {
    let rest = path.strip_prefix("corpus/pages/")?;
    let mut parts = rest.split('/');
    let (area_folder, subarea_folder, file, None) =
        (parts.next()?, parts.next()?, parts.next()?, parts.next())
    else {
        return None;
    };
    let stem = file.strip_suffix(".md")?;
    Some((
        area_folder.to_owned(),
        subarea_folder.to_owned(),
        stem.to_owned(),
    ))
}

fn parse_pages(tree: &Tree, corpus: Option<&CorpusToml>) -> (Vec<PageEntry>, Vec<RawPageRecord>) {
    let mut pages = Vec::new();
    let mut raw_pages = Vec::new();

    for path in files::page_paths_for(tree, corpus) {
        let Some(bytes) = tree.get_bytes(&path) else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(bytes) else {
            continue;
        };
        let Some((area_folder, subarea_folder, stem)) = split_page_path(&path) else {
            continue;
        };

        match page::split(text) {
            Err(err) => {
                raw_pages.push(RawPageRecord {
                    path,
                    area_folder,
                    subarea_folder,
                    stem,
                    outcome: RawPageOutcome::SplitErr(err),
                });
            }
            Ok(split) => match page::parse_meta(&split.front_matter) {
                Err(err) => {
                    raw_pages.push(RawPageRecord {
                        path,
                        area_folder,
                        subarea_folder,
                        stem,
                        outcome: RawPageOutcome::MetaErr(err),
                    });
                }
                Ok(meta) => {
                    raw_pages.push(RawPageRecord {
                        path: path.clone(),
                        area_folder,
                        subarea_folder,
                        stem,
                        outcome: RawPageOutcome::Parsed(Box::new(meta.clone())),
                    });
                    let parts = page_parts::split_parts(
                        &split.body,
                        usize::try_from(split.body_line).unwrap_or(1),
                    );
                    pages.push(PageEntry {
                        id: meta.id.clone(),
                        path,
                        meta,
                        body: split.body,
                        body_line: split.body_line,
                        parts,
                    });
                }
            },
        }
    }

    pages.sort_by(|a, b| (&a.id, &a.path).cmp(&(&b.id, &b.path)));
    (pages, raw_pages)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PageCitationFinding {
    pub path: String,
    pub finding: LexFinding,
}

/// Parsed state built once by [`super::load`] and shared across passes.
///
/// All fields are private; callers read them through [`Self::tree`], [`Self::today`],
/// [`Self::corpus`], [`Self::register`], [`Self::register_rows`], [`Self::pages`],
/// [`Self::citations`] and the typed row accessors ([`Self::sources`], [`Self::claims`],
/// [`Self::questions`], [`Self::conflicts`], [`Self::glossary`]).
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
    pages: Vec<PageEntry>,
    raw_pages: Vec<RawPageRecord>,
    index: Index,
    page_citations: BTreeMap<PageId, Vec<Citation>>,
    citation_findings: Vec<PageCitationFinding>,
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

        let (pages, raw_pages) = parse_pages(&tree, corpus.as_ref());

        let index = Index::build(
            corpus.as_ref(),
            &sources,
            &claims,
            &questions,
            &conflicts,
            &glossary,
            &pages,
        );

        let resolve_index = build_citation_resolve_index(&index, &registers);
        let sources_loaded = registers[register_index(Register::Sources)].is_some();
        let claims_loaded = registers[register_index(Register::Claims)].is_some();
        let questions_loaded = registers[register_index(Register::Questions)].is_some();
        let conflicts_loaded = registers[register_index(Register::Conflicts)].is_some();

        let mut page_citations = BTreeMap::new();
        let mut citation_findings = Vec::new();
        for page in &pages {
            let base_line = usize::try_from(page.body_line).unwrap_or(1).max(1);
            let lexed = cite::lex_from_line(&page.body, base_line, Mode::Strict);
            let missing_reg_keys: Vec<String> = lexed
                .citations
                .iter()
                .filter_map(|c| {
                    let loaded = match c.key_kind {
                        KeyKind::Version => sources_loaded,
                        KeyKind::Claim => claims_loaded,
                        KeyKind::Question => questions_loaded,
                        KeyKind::Conflict => conflicts_loaded,
                        KeyKind::Note => true,
                    };
                    (!loaded).then(|| format!("`@{}", c.key))
                })
                .collect();
            let (resolved_cites, _resolved_keys, findings) =
                cite::resolve_citations(lexed, &resolve_index);
            page_citations
                .entry(page.id.clone())
                .or_insert(resolved_cites);
            for finding in findings {
                if finding.code == Code::Fb0303
                    && missing_reg_keys
                        .iter()
                        .any(|prefix| finding.message.contains(prefix))
                {
                    continue;
                }
                citation_findings.push(PageCitationFinding {
                    path: page.path.clone(),
                    finding,
                });
            }
        }

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
            pages,
            raw_pages,
            index,
            page_citations,
            citation_findings,
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

    /// Parsed pages in `corpus/pages/`, sorted by [`PageId`], skipping any page whose fences or
    /// front matter did not parse.
    #[must_use]
    pub fn pages(&self) -> &[PageEntry] {
        &self.pages
    }

    /// The resolved identifier [`Index`] built from the typed register rows, parsed pages and
    /// `corpus.toml`.
    #[must_use]
    pub const fn index(&self) -> &Index {
        &self.index
    }

    /// Resolved [`Citation`]s for `page` in reading order, or `&[]` when `page` has none (or does
    /// not exist).
    #[must_use]
    pub fn citations(&self, page: &PageId) -> &[Citation] {
        match self.page_citations.get(page) {
            Some(cites) => cites.as_slice(),
            None => &[],
        }
    }

    /// Lexer and resolution findings collected across all parsed pages while building [`Model`].
    #[must_use]
    pub(crate) fn citation_findings(&self) -> &[PageCitationFinding] {
        &self.citation_findings
    }

    /// Single-cell [`CellProblem`]s collected while building typed rows for `register`.
    #[must_use]
    pub(crate) fn register_problems(&self, register: Register) -> &[RowProblems] {
        &self.register_problems[register_index(register)]
    }

    /// All UTF-8 page records from `files::page_paths`, in path order, including split and
    /// front-matter parse failures.
    #[must_use]
    pub(crate) fn raw_pages(&self) -> &[RawPageRecord] {
        &self.raw_pages
    }
}
