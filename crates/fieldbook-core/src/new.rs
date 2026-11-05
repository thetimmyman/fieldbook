//! Pure planners for `fieldbook new` (`new-verbs`): [`plan_page`] and [`plan_row`].
//!
//! **Responsibility.** Given a loaded [`Model`], validate the area, subarea, page type and
//! source version a `new` command names, allocate the next free identifier in its series, and
//! build either a template-instantiated page ([`PagePlan`]) or one register row in its entry
//! state ([`RowPlan`]). [`replan_page`] and [`replan_row`] rebuild the same plan from the
//! identifier and planning date an interrupted run recorded, so recovery never allocates again.
//! [`render_register`] returns the register bytes with the planned row appended (or replaced
//! when its id is already there), in normal form.
//!
//! **Rules.** Pure: no I/O, clock or random state. `new` does not run `check`: a cell that no
//! flag fills keeps its required entry default, or is empty when optional.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use crate::check::Model;
use crate::date::{CivilDate, PartialDate, Volatility};
use crate::ids::{
    AreaCode, ClaimId, ConflictId, IdError, PageId, QuestionId, TermId, VersionId, next_claim_id,
    next_conflict_id, next_question_id,
};
use crate::labels::{LabelId, LabelSet};
use crate::schema::claims::{ClaimDraft, ClaimRow};
use crate::schema::locator::{Locator, LocatorError};
use crate::schema::table::{Row, write as write_csv};
use crate::schema::vocab::{
    ClaimKind, ConflictStatus, DefinitionStatus, DraftedBy, PageType, QuestionStatus,
};
use crate::schema::{Register, page};

/// A planned new page: the file path under the repository root and its full text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PagePlan {
    /// Repository-relative path, `corpus/pages/<area-slug>/<subarea>/<id>.md`.
    pub path: PathBuf,
    /// The template with `id`, `area`, `subarea` and `as_of` filled and `<today>` replaced.
    pub text: String,
}

/// A planned new register row in its entry state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowPlan {
    /// Repository-relative path of the register, `corpus/registers/<name>.csv`.
    pub path: PathBuf,
    /// The row's cells in column order.
    pub cells: Vec<String>,
    /// The row's primary key: allocated, or supplied for a term.
    pub id: String,
}

/// The register a `new` row command appends to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RowKind {
    /// A row of `claims.csv`.
    Claim,
    /// A row of `questions.csv`.
    Question,
    /// A row of `conflicts.csv`.
    Conflict,
    /// A row of `glossary.csv`.
    Term,
}

impl RowKind {
    /// The [`Register`] this kind appends to.
    #[must_use]
    pub const fn register(self) -> Register {
        match self {
            Self::Claim => Register::Claims,
            Self::Question => Register::Questions,
            Self::Conflict => Register::Conflicts,
            Self::Term => Register::Glossary,
        }
    }

    /// The lowercase word for this kind (`claim`, `question`, `conflict`, `term`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Claim => "claim",
            Self::Question => "question",
            Self::Conflict => "conflict",
            Self::Term => "term",
        }
    }
}

/// The value of each flag a row command took, keyed by flag name.
///
/// Keys read by [`plan_row`]: `area` (every kind), `source`, `at`, `quote` and `drafted_by`
/// (claim), `text` (question) and `id` (term, optional).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fields(BTreeMap<String, String>);

impl Fields {
    /// An empty map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set `key` to `value`.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.0.insert(key.into(), value.into());
    }

    /// Builder form of [`Fields::insert`].
    #[must_use]
    pub fn with(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.insert(key, value);
        self
    }

    /// The value of `key`, if set.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    /// Every `(key, value)` pair in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

/// Why a `new` command cannot be planned. Each is a usage error (exit 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewError {
    /// A page with this id is already in the Corpus.
    PageExists(PageId),
    /// A glossary term with this id is already in `glossary.csv`.
    TermExists(TermId),
    /// A supplied term id is not snake_case.
    InvalidTermId(String),
    /// The area code is not in `corpus/corpus.toml`.
    UnknownArea(String),
    /// The subarea is not one of the area's subareas.
    UnknownSubarea {
        /// The area.
        area: AreaCode,
        /// The subarea that was named.
        subarea: String,
    },
    /// The page type is not one of the v1 page types.
    UnknownPageType(String),
    /// The version id is well formed but is not in `sources.csv`.
    UnknownSourceVersion(VersionId),
    /// The text is not a version id (`S-nnnn-vN`).
    InvalidSourceVersion(String),
    /// The `--at` text is not a locator the Corpus accepts.
    InvalidLocator(String),
    /// There is no `corpus/templates/<type>.md` to copy.
    MissingTemplate(PageType),
    /// A register is missing or unreadable, so no row can be appended.
    MissingRegister(Register),
    /// An id series has no free number left (carries the typed exhausted family).
    Exhausted(IdError),
    /// A supplied value holds a line break or another control character, which a register cell
    /// cannot carry.
    ControlCharacter {
        /// The flag whose value held it.
        field: String,
    },
}

impl fmt::Display for NewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PageExists(id) => write!(f, "page `{id}` already exists"),
            Self::TermExists(id) => write!(f, "term `{id}` already exists"),
            Self::InvalidTermId(raw) => {
                write!(f, "`{raw}` is not a term id (lowercase snake_case)")
            }
            Self::UnknownArea(area) => write!(f, "unknown area `{area}`"),
            Self::UnknownSubarea { area, subarea } => {
                write!(f, "unknown subarea `{subarea}` in area `{area}`")
            }
            Self::UnknownPageType(ty) => write!(f, "unknown page type `{ty}`"),
            Self::UnknownSourceVersion(id) => {
                write!(f, "unknown source version `{id}` in sources.csv")
            }
            Self::InvalidSourceVersion(raw) => {
                write!(f, "`{raw}` is not a source version id (S-nnnn-vN)")
            }
            Self::InvalidLocator(raw) => write!(f, "`{raw}` is not a locator"),
            Self::MissingTemplate(ty) => {
                write!(f, "no template corpus/templates/{}.md", ty.as_str())
            }
            Self::MissingRegister(register) => {
                write!(
                    f,
                    "register corpus/registers/{register} is missing or unreadable"
                )
            }
            Self::Exhausted(err) => write!(f, "{} id space exhausted", err.form()),
            Self::ControlCharacter { field } => write!(
                f,
                "the value of `{field}` holds a line break or control character"
            ),
        }
    }
}

impl std::error::Error for NewError {}

/// The repository-relative path of `register`.
#[must_use]
pub fn register_path(register: Register) -> PathBuf {
    PathBuf::from(format!("corpus/registers/{}", register.file_name()))
}

/// The repository-relative path of the template for `ty`.
#[must_use]
pub fn template_path(ty: PageType) -> String {
    format!("corpus/templates/{}.md", ty.as_str())
}

/// Plan a new page from `template_text`.
///
/// # Errors
///
/// Returns [`NewError::UnknownArea`] or [`NewError::UnknownSubarea`] when the id's area or the
/// subarea is not in `corpus.toml`, and [`NewError::PageExists`] when a page with `id` is in the
/// Corpus.
pub fn plan_page(
    model: &Model,
    id: &PageId,
    ty: PageType,
    subarea: &str,
    template_text: &str,
) -> Result<PagePlan, NewError> {
    if page_exists(model, id) {
        return Err(NewError::PageExists(id.clone()));
    }
    replan_page(model, id, ty, subarea, template_text, model.today())
}

/// Plan a page for `planned_on` without the existing-page check, to finish an interrupted run
/// whose page file may already be on disk.
///
/// # Errors
///
/// As [`plan_page`], except that an existing page is not an error.
pub fn replan_page(
    model: &Model,
    id: &PageId,
    ty: PageType,
    subarea: &str,
    template_text: &str,
    planned_on: CivilDate,
) -> Result<PagePlan, NewError> {
    let area = id.area();
    let slug = area_slug(model, &area, subarea)?;
    let date = planned_on.to_string();
    let mut in_front_matter = false;
    let mut fences = 0usize;
    let mut out = String::with_capacity(template_text.len() + 64);
    for (n, raw) in template_text.split('\n').enumerate() {
        if n > 0 {
            out.push('\n');
        }
        let (line, eol) = raw.strip_suffix('\r').map_or((raw, ""), |l| (l, "\r"));
        if line == "+++" {
            fences += 1;
            in_front_matter = fences == 1;
            out.push_str(raw);
            continue;
        }
        let filled = if in_front_matter {
            if line.starts_with("id = ") {
                Some(format!("id = \"{id}\""))
            } else if line.starts_with("area = ") {
                Some(format!("area = \"{area}\""))
            } else if line.starts_with("subarea = ") {
                Some(format!("subarea = \"{subarea}\""))
            } else if line.starts_with("as_of = ") {
                Some(format!("as_of = {date}"))
            } else {
                None
            }
        } else {
            None
        };
        match filled {
            Some(text) => {
                out.push_str(&text);
                out.push_str(eol);
            }
            None => out.push_str(&raw.replace("<today>", &date)),
        }
    }
    let _ = ty;
    Ok(PagePlan {
        path: PathBuf::from(format!("corpus/pages/{slug}/{subarea}/{id}.md")),
        text: out,
    })
}

/// Plan a new register row with the next free id in its series (claims per area).
///
/// A term takes its id from the `id` field; without one it takes `<area>_term_<n>`.
///
/// # Errors
///
/// Returns a [`NewError`] when the area, source version, locator or term id is not valid, the
/// register is missing, or the id series is exhausted.
pub fn plan_row(
    model: &Model,
    kind: RowKind,
    area: Option<&AreaCode>,
    fields: &Fields,
) -> Result<RowPlan, NewError> {
    let area = row_area(model, area, fields)?;
    let existing = existing_ids(model, kind);
    let id = match kind {
        RowKind::Claim => next_claim_id(area.as_str(), &existing)
            .map(|id| id.to_string())
            .map_err(NewError::Exhausted)?,
        RowKind::Question => next_question_id(&existing)
            .map(|id| id.to_string())
            .map_err(NewError::Exhausted)?,
        RowKind::Conflict => next_conflict_id(&existing)
            .map(|id| id.to_string())
            .map_err(NewError::Exhausted)?,
        RowKind::Term => term_id(&area, &existing, fields)?,
    };
    build_row(model, kind, &area, fields, id, model.today())
}

/// Plan a row with an id and planning date an interrupted run recorded. No id is allocated and
/// an id already in the register is not an error.
///
/// # Errors
///
/// As [`plan_row`], except for allocation and duplicate-id failures.
pub fn replan_row(
    model: &Model,
    kind: RowKind,
    area: Option<&AreaCode>,
    fields: &Fields,
    id: &str,
    planned_on: CivilDate,
) -> Result<RowPlan, NewError> {
    let area = row_area(model, area, fields)?;
    build_row(model, kind, &area, fields, id.to_owned(), planned_on)
}

/// The bytes of `plan`'s register with the planned row appended, or replacing the row that
/// already has its id, in normal form.
///
/// # Errors
///
/// Returns [`NewError::MissingRegister`] when the register did not load.
pub fn render_register(model: &Model, kind: RowKind, plan: &RowPlan) -> Result<Vec<u8>, NewError> {
    let register = kind.register();
    let mut table = model
        .register(register)
        .ok_or(NewError::MissingRegister(register))?
        .clone();
    if let Some(row) = table
        .rows
        .iter_mut()
        .find(|row| row.cells.first().is_some_and(|id| *id == plan.id))
    {
        row.cells.clone_from(&plan.cells);
    } else {
        let line = table
            .rows
            .last()
            .map_or(2, |row| row.line.saturating_add(1));
        table.rows.push(Row {
            line,
            cols: vec![1; plan.cells.len()],
            cells: plan.cells.clone(),
        });
    }
    Ok(write_csv(&table).into_bytes())
}

/// The complete `handling` label set a page template's front matter declares.
///
/// The front matter is read with the same fence split and TOML parser as every page, so a
/// multiline array is read whole. Returns `None` (the caller fails closed) when the fences are
/// malformed, the front matter is not TOML, `handling` is missing, is not an array of strings,
/// is empty, or names an item that is not a label id.
#[must_use]
pub fn template_handling(template_text: &str) -> Option<LabelSet> {
    let split = page::split(template_text).ok()?;
    let table: toml::Table = toml::from_str(split.front_matter()).ok()?;
    let items = table.get("handling")?.as_array()?;
    if items.is_empty() {
        return None;
    }
    let mut ids = Vec::with_capacity(items.len());
    for item in items {
        ids.push(LabelId::parse(item.as_str()?).ok()?);
    }
    Some(LabelSet::from_ids(ids))
}

/// The `handling` labels (`|`-separated cell) of the source version row, for admission.
#[must_use]
pub fn source_handling(model: &Model, version: &VersionId) -> Option<String> {
    let row = source_row(model, version)?;
    let at = column_index(Register::Sources, "handling")?;
    row.cells.get(at).cloned()
}

fn page_exists(model: &Model, id: &PageId) -> bool {
    let suffix = format!("/{id}.md");
    model.index().contains_page(id)
        || model
            .tree()
            .paths()
            .any(|path| path.starts_with("corpus/pages/") && path.ends_with(&suffix))
}

fn area_slug(model: &Model, area: &AreaCode, subarea: &str) -> Result<String, NewError> {
    let Some(def) = model
        .corpus()
        .and_then(|corpus| corpus.areas().iter().find(|a| a.code == area.as_str()))
    else {
        return Err(NewError::UnknownArea(area.to_string()));
    };
    if !def.subareas.iter().any(|s| s == subarea) {
        return Err(NewError::UnknownSubarea {
            area: area.clone(),
            subarea: subarea.to_owned(),
        });
    }
    Ok(def.slug.clone())
}

fn row_area(model: &Model, area: Option<&AreaCode>, fields: &Fields) -> Result<AreaCode, NewError> {
    let code = match area {
        Some(code) => code.clone(),
        None => {
            let raw = fields.get("area").unwrap_or_default();
            AreaCode::parse(raw).map_err(|_| NewError::UnknownArea(raw.to_owned()))?
        }
    };
    let known = model
        .corpus()
        .is_some_and(|corpus| corpus.areas().iter().any(|a| a.code == code.as_str()));
    if known {
        Ok(code)
    } else {
        Err(NewError::UnknownArea(code.to_string()))
    }
}

/// Every id in the register's first column that parses in the kind's family, retired rows
/// included. A cell that is not an id is left to `check`.
fn existing_ids(model: &Model, kind: RowKind) -> Vec<String> {
    model
        .register_rows(kind.register())
        .iter()
        .filter_map(|row| row.cells.first())
        .filter(|cell| match kind {
            RowKind::Claim => ClaimId::parse(cell).is_ok(),
            RowKind::Question => QuestionId::parse(cell).is_ok(),
            RowKind::Conflict => ConflictId::parse(cell).is_ok(),
            RowKind::Term => TermId::parse(cell).is_ok(),
        })
        .cloned()
        .collect()
}

fn term_id(area: &AreaCode, existing: &[String], fields: &Fields) -> Result<String, NewError> {
    if let Some(raw) = fields.get("id").filter(|raw| !raw.is_empty()) {
        let id = TermId::parse(raw).map_err(|_| NewError::InvalidTermId(raw.to_owned()))?;
        if existing.iter().any(|e| e == id.as_str()) {
            return Err(NewError::TermExists(id));
        }
        return Ok(id.to_string());
    }
    let prefix = format!("{area}_term_");
    let next = existing
        .iter()
        .filter_map(|e| e.strip_prefix(&prefix)?.parse::<u32>().ok())
        .max()
        .map_or(1, |n| n.saturating_add(1));
    Ok(format!("{prefix}{next}"))
}

fn build_row(
    model: &Model,
    kind: RowKind,
    area: &AreaCode,
    fields: &Fields,
    id: String,
    planned_on: CivilDate,
) -> Result<RowPlan, NewError> {
    if let Some((field, _)) = fields
        .iter()
        .find(|(_, value)| value.chars().any(char::is_control))
    {
        return Err(NewError::ControlCharacter {
            field: field.to_owned(),
        });
    }
    let register = kind.register();
    let cells = match kind {
        RowKind::Claim => claim_cells(model, area, fields, &id, planned_on)?,
        RowKind::Question => by_column(
            register,
            &[
                ("question_id", &id),
                ("area", area.as_str()),
                ("question", fields.get("text").unwrap_or_default()),
                ("day_one", "false"),
                ("status", QuestionStatus::Open.as_str()),
            ],
        ),
        RowKind::Conflict => by_column(
            register,
            &[
                ("conflict_id", &id),
                ("area", area.as_str()),
                ("status", ConflictStatus::Unresolved.as_str()),
            ],
        ),
        RowKind::Term => by_column(
            register,
            &[
                ("term_id", &id),
                ("owner_area", area.as_str()),
                (
                    "definition_status",
                    DefinitionStatus::GeneralPractice.as_str(),
                ),
            ],
        ),
    };
    Ok(RowPlan {
        path: register_path(register),
        cells,
        id,
    })
}

fn claim_cells(
    model: &Model,
    area: &AreaCode,
    fields: &Fields,
    id: &str,
    _planned_on: CivilDate,
) -> Result<Vec<String>, NewError> {
    let raw = fields.get("source").unwrap_or_default();
    let version =
        VersionId::parse(raw).map_err(|_| NewError::InvalidSourceVersion(raw.to_owned()))?;
    let Some(source) = source_row(model, &version) else {
        return Err(NewError::UnknownSourceVersion(version));
    };
    let at = fields.get("at").unwrap_or_default().trim();
    let locator =
        Locator::parse(at).map_err(|_: LocatorError| NewError::InvalidLocator(at.to_owned()))?;
    let locator_kind = locator.kind();
    if locator_kind.is_overlay() {
        return Err(NewError::InvalidLocator(at.to_owned()));
    }
    let claim_id = ClaimId::parse(id).map_err(NewError::Exhausted)?;
    let as_of = source_cell(source, "as_of").and_then(|text| PartialDate::parse(text).ok());
    let volatility = source_cell(source, "volatility")
        .and_then(Volatility::parse)
        .unwrap_or(Volatility::V2);
    let drafted_by = fields
        .get("drafted_by")
        .and_then(|text| DraftedBy::parse(text).ok())
        .unwrap_or(DraftedBy::Codex);
    let quote = fields
        .get("quote")
        .filter(|q| !q.is_empty())
        .map(str::to_owned);
    let row = ClaimRow::entry(ClaimDraft {
        claim_id,
        area: area.clone(),
        kind: ClaimKind::Fact,
        statement: String::new(),
        as_of,
        source_id: Some(version.source()),
        version: Some(version.version()),
        locator_kind: Some(locator_kind),
        locator: Some(locator.to_canonical_string()),
        quote,
        consequential: false,
        drafted_by,
        volatility,
    });
    Ok(row.to_cells())
}

fn source_row<'m>(model: &'m Model, version: &VersionId) -> Option<&'m Row> {
    let number = version.version().to_string();
    model.register_rows(Register::Sources).iter().find(|row| {
        row.cells
            .first()
            .is_some_and(|s| s == version.source().as_str())
            && row.cells.get(1).is_some_and(|v| *v == number)
    })
}

fn source_cell<'r>(row: &'r Row, column: &str) -> Option<&'r str> {
    let at = column_index(Register::Sources, column)?;
    row.cells
        .get(at)
        .map(String::as_str)
        .filter(|c| !c.is_empty())
}

fn column_index(register: Register, name: &str) -> Option<usize> {
    register.columns().iter().position(|c| c.name == name)
}

fn by_column(register: Register, values: &[(&str, &str)]) -> Vec<String> {
    register
        .columns()
        .iter()
        .map(|column| {
            values
                .iter()
                .find(|(name, _)| *name == column.name)
                .map(|(_, value)| (*value).to_owned())
                .unwrap_or_default()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::{self, Host, Options, Tree};

    const CORPUS_TOML: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/corpus.toml");
    const SOURCES: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/sources.csv");
    const CLAIMS: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/claims.csv");
    const QUESTIONS: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/questions.csv");
    const CONFLICTS: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/conflicts.csv");
    const GLOSSARY: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/glossary.csv");
    const CONCEPT: &str = include_str!("../../../corpus/templates/concept.md");
    const PAGE: &str = include_str!(
        "../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-system-review.md"
    );

    fn model_with(claims: &str, questions: &str) -> Model {
        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_bytes("corpus/corpus.toml", CORPUS_TOML);
        tree.insert_bytes("corpus/registers/sources.csv", SOURCES);
        tree.insert_bytes("corpus/registers/claims.csv", claims);
        tree.insert_bytes("corpus/registers/questions.csv", questions);
        tree.insert_bytes("corpus/registers/conflicts.csv", CONFLICTS);
        tree.insert_bytes("corpus/registers/glossary.csv", GLOSSARY);
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-system-review.md",
            PAGE,
        );
        let today = CivilDate::parse("2026-03-01").unwrap_or_else(|e| panic!("{e}"));
        check::load(&Host::new(tree, today), &Options::new(false)).0
    }

    fn model() -> Model {
        model_with(CLAIMS, QUESTIONS)
    }

    fn area(code: &str) -> AreaCode {
        AreaCode::parse(code).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn page_fills_front_matter_and_replaces_today() {
        let id = PageId::parse("cmp-new-topic").unwrap_or_else(|e| panic!("{e}"));
        let plan = plan_page(
            &model(),
            &id,
            PageType::Concept,
            "business-systems",
            CONCEPT,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            plan.path,
            PathBuf::from("corpus/pages/compliance/business-systems/cmp-new-topic.md")
        );
        assert!(plan.text.contains("id = \"cmp-new-topic\"\n"));
        assert!(plan.text.contains("area = \"cmp\"\n"));
        assert!(plan.text.contains("subarea = \"business-systems\"\n"));
        assert!(plan.text.contains("as_of = 2026-03-01\n"));
        assert!(
            plan.text
                .contains("- NEW_KNOWLEDGE, 2026-03-01: <One sentence")
        );
        assert!(plan.text.contains("title = \"<title>\""));
        assert!(!plan.text.contains("<today>"));
    }

    #[test]
    fn page_refuses_a_taken_id_and_unknown_names() {
        let m = model();
        let taken = PageId::parse("cmp-system-review").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            plan_page(&m, &taken, PageType::Concept, "business-systems", CONCEPT),
            Err(NewError::PageExists(taken.clone()))
        );
        assert!(
            replan_page(
                &m,
                &taken,
                PageType::Concept,
                "business-systems",
                CONCEPT,
                m.today()
            )
            .is_ok()
        );
        let free = PageId::parse("cmp-free").unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(
            plan_page(&m, &free, PageType::Concept, "nowhere", CONCEPT),
            Err(NewError::UnknownSubarea { .. })
        ));
        let other = PageId::parse("zzz-free").unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(
            plan_page(&m, &other, PageType::Concept, "business-systems", CONCEPT),
            Err(NewError::UnknownArea(_))
        ));
    }

    #[test]
    fn question_takes_the_next_id_with_both_defaults() {
        let fields = Fields::new().with("text", "Synthetic question?");
        let plan = plan_row(&model(), RowKind::Question, Some(&area("cmp")), &fields)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(plan.id, "Q-003");
        assert_eq!(plan.cells.len(), 13);
        assert_eq!(plan.cells[0], "Q-003");
        assert_eq!(plan.cells[2], "Synthetic question?");
        assert_eq!(plan.cells[8], "false");
        assert_eq!(plan.cells[9], "open");
        assert!(plan.cells[10..].iter().all(String::is_empty));
    }

    #[test]
    fn claim_takes_the_next_id_in_its_area_and_entry_values() {
        let fields = Fields::new()
            .with("source", "S-0002-v2")
            .with("at", "p. 3")
            .with("quote", "synthetic");
        let plan = plan_row(&model(), RowKind::Claim, Some(&area("est")), &fields)
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(plan.id.starts_with("CL-EST-"), "{}", plan.id);
        assert_eq!(plan.cells.len(), 35);
        assert_eq!(plan.cells[5], "S-0002");
        assert_eq!(plan.cells[6], "2");
        assert_eq!(plan.cells[7], "pdf_page_box");
        assert_eq!(plan.cells[8], "p. 3");
        assert_eq!(plan.cells[9], "synthetic");
        assert_eq!(plan.cells[12], "unchecked");
        assert_eq!(plan.cells[13], "pending_company_confirmation");
        assert_eq!(plan.cells[14], "proposed");
        assert_eq!(plan.cells[15], "unknown");
        assert_eq!(plan.cells[28], "");
        assert_eq!(plan.cells[29], "");
    }

    #[test]
    fn claim_refuses_an_unknown_version_and_a_bad_locator() {
        let m = model();
        let bad = Fields::new()
            .with("source", "S-9999-v1")
            .with("at", "p. 1")
            .with("quote", "q");
        assert!(matches!(
            plan_row(&m, RowKind::Claim, Some(&area("cmp")), &bad),
            Err(NewError::UnknownSourceVersion(_))
        ));
        let not_id = Fields::new().with("source", "nope").with("at", "p. 1");
        assert!(matches!(
            plan_row(&m, RowKind::Claim, Some(&area("cmp")), &not_id),
            Err(NewError::InvalidSourceVersion(_))
        ));
        let loc = Fields::new()
            .with("source", "S-0002-v2")
            .with("at", "page one");
        assert!(matches!(
            plan_row(&m, RowKind::Claim, Some(&area("cmp")), &loc),
            Err(NewError::InvalidLocator(_))
        ));
    }

    #[test]
    fn question_series_counts_retired_rows_and_exhausts() {
        let header = QUESTIONS.lines().next().unwrap_or_default();
        let retired = format!("{header}\nQ-998,cmp,Old?,,,,,,false,withdrawn,,,\n");
        let m = model_with(CLAIMS, &retired);
        let fields = Fields::new().with("text", "Next?");
        let plan = plan_row(&m, RowKind::Question, Some(&area("cmp")), &fields)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(plan.id, "Q-999");
        let full = format!("{header}\nQ-999,cmp,Last?,,,,,,false,open,,,\n");
        let m = model_with(CLAIMS, &full);
        let err = plan_row(&m, RowKind::Question, Some(&area("cmp")), &fields)
            .expect_err("Q-999 is the last question id");
        assert!(matches!(&err, NewError::Exhausted(e) if e.is_exhausted()));
        assert_eq!(err.to_string(), "question id space exhausted");
    }

    #[test]
    fn term_takes_a_supplied_or_a_default_id() {
        let m = model();
        let supplied = Fields::new().with("id", "synthetic_term");
        let plan = plan_row(&m, RowKind::Term, Some(&area("cmp")), &supplied)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(plan.id, "synthetic_term");
        assert_eq!(plan.cells[4], "cmp");
        assert_eq!(plan.cells[6], "general_practice");
        let default = plan_row(&m, RowKind::Term, Some(&area("cmp")), &Fields::new())
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(default.id, "cmp_term_1");
        let bad = Fields::new().with("id", "Not Snake");
        assert!(matches!(
            plan_row(&m, RowKind::Term, Some(&area("cmp")), &bad),
            Err(NewError::InvalidTermId(_))
        ));
    }

    #[test]
    fn conflict_enters_unresolved() {
        let plan = plan_row(
            &model(),
            RowKind::Conflict,
            Some(&area("cmp")),
            &Fields::new(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(plan.id, "XC-02");
        assert_eq!(plan.cells[1], "cmp");
        assert_eq!(plan.cells[8], "unresolved");
    }

    #[test]
    fn render_appends_then_replaces_the_same_id() {
        let m = model();
        let fields = Fields::new().with("text", "Synthetic question?");
        let plan = plan_row(&m, RowKind::Question, Some(&area("cmp")), &fields)
            .unwrap_or_else(|e| panic!("{e}"));
        let once = render_register(&m, RowKind::Question, &plan).unwrap_or_else(|e| panic!("{e}"));
        let m2 = model_with(
            CLAIMS,
            &String::from_utf8(once.clone()).unwrap_or_else(|e| panic!("{e}")),
        );
        let replan = replan_row(
            &m2,
            RowKind::Question,
            Some(&area("cmp")),
            &fields,
            "Q-003",
            m2.today(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let again =
            render_register(&m2, RowKind::Question, &replan).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(once, again);
        assert!(String::from_utf8_lossy(&once).contains("Q-003,cmp,Synthetic question?"));
    }
}
