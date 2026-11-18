//! Read-only Corpus view passed to correlators and store admission adapters.

use crate::add::AddError;
use crate::check::{Located, Model, PageEntry};
use crate::date::CivilDate;
use crate::ids::{IdError, QuestionId, SourceId, VersionId, next_source_id};
use crate::schema::{ClaimRow, ConflictRow, QuestionRow, Register, SourceRow};

/// Read-only projection of a checked Corpus [`Model`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorpusView {
    model: Model,
}

impl CorpusView {
    /// Builds a read-only [`CorpusView`] from a loaded [`Model`].
    #[must_use]
    pub fn from_model(model: &Model) -> Self {
        Self {
            model: model.clone(),
        }
    }

    /// Returns the calendar date held by the underlying model.
    #[must_use]
    pub const fn today(&self) -> CivilDate {
        self.model.today()
    }

    /// Returns the existing [`VersionId`] if `sources.csv` already holds a row with `sha256`.
    #[must_use]
    pub fn has_hash(&self, sha256: &str) -> Option<VersionId> {
        let table = self.model.register(Register::Sources)?;
        for row in &table.rows {
            let row_sha = row.cells.get(12).map_or("", String::as_str);
            if row_sha == sha256 {
                let src = row.cells.first().map_or("", String::as_str);
                let ver = row.cells.get(1).map_or("", String::as_str);
                if let Ok(vid) = VersionId::parse(&format!("{src}-v{ver}")) {
                    return Some(vid);
                }
            }
        }
        None
    }

    /// Allocates the next unused [`SourceId`] above every row in `sources.csv`.
    ///
    /// # Errors
    ///
    /// Returns [`IdError`] if existing identifiers fail parsing or the `S-9999` limit is reached.
    pub fn next_source_id(&self) -> Result<SourceId, IdError> {
        let Some(table) = self.model.register(Register::Sources) else {
            return SourceId::parse("S-0001");
        };
        let existing = table
            .rows
            .iter()
            .filter_map(|r| r.cells.first().map(String::as_str))
            .filter(|s| !s.is_empty());
        next_source_id(existing)
    }

    /// Returns the next [`VersionId`] for `source`, or `Ok(None)` if `source` does not exist.
    ///
    /// # Errors
    ///
    /// Returns [`AddError`] if `sources.csv` is missing or version numbers overflow.
    pub fn next_version(&self, source: &SourceId) -> Result<Option<VersionId>, AddError> {
        let table = self
            .model
            .register(Register::Sources)
            .ok_or(AddError::MissingSourcesRegister)?;
        let mut max_ver = 0u32;
        for row in &table.rows {
            if row.cells.first().map(String::as_str) == Some(source.as_str())
                && let Some(ver_raw) = row.cells.get(1)
                && let Ok(ver) = ver_raw.parse::<u32>()
                && ver > max_ver
            {
                max_ver = ver;
            }
        }
        if max_ver == 0 {
            return Ok(None);
        }
        let next_ver = max_ver
            .checked_add(1)
            .ok_or_else(|| AddError::InvalidCell {
                column: "version",
                value: max_ver.to_string(),
                reason: "source version overflow",
            })?;
        let vid = VersionId::parse(&format!("{source}-v{next_ver}")).map_err(|_| {
            AddError::InvalidCell {
                column: "version",
                value: next_ver.to_string(),
                reason: "invalid next version identifier",
            }
        })?;
        Ok(Some(vid))
    }

    /// Returns the highest-version [`SourceRow`] for `id`, if present.
    #[must_use]
    pub fn source(&self, id: &SourceId) -> Option<&SourceRow> {
        self.model
            .sources()
            .iter()
            .map(|loc| &loc.value)
            .filter(|row| row.source_id == *id)
            .max_by_key(|row| row.version)
    }

    /// Returns every parsed [`SourceRow`] in `sources.csv`.
    #[must_use]
    pub fn sources(&self) -> &[Located<SourceRow>] {
        self.model.sources()
    }

    /// Returns every parsed [`QuestionRow`] in `questions.csv`.
    #[must_use]
    pub fn questions(&self) -> &[Located<QuestionRow>] {
        self.model.questions()
    }

    /// Looks up one [`QuestionRow`] by [`QuestionId`].
    #[must_use]
    pub fn question(&self, id: &QuestionId) -> Option<&QuestionRow> {
        self.model
            .questions()
            .iter()
            .map(|loc| &loc.value)
            .find(|row| row.question_id == *id)
    }

    /// Returns every parsed [`ClaimRow`] in `claims.csv`.
    #[must_use]
    pub fn claims(&self) -> &[Located<ClaimRow>] {
        self.model.claims()
    }

    /// Returns every parsed [`ConflictRow`] in `conflicts.csv`.
    #[must_use]
    pub fn conflicts(&self) -> &[Located<ConflictRow>] {
        self.model.conflicts()
    }

    /// Returns every indexed [`PageEntry`] in the Corpus.
    #[must_use]
    pub fn pages(&self) -> &[PageEntry] {
        self.model.pages()
    }

    /// Returns `true` when `target` identifies an existing source, source version, claim,
    /// question, conflict, or page in this Corpus view.
    #[must_use]
    pub fn has_target(&self, target: &str) -> bool {
        if self
            .model
            .sources()
            .iter()
            .any(|s| s.value.source_id.as_str() == target || s.value.version_id_str() == target)
        {
            return true;
        }
        if self
            .model
            .claims()
            .iter()
            .any(|c| c.value.claim_id.as_str() == target)
        {
            return true;
        }
        if self
            .model
            .questions()
            .iter()
            .any(|q| q.value.question_id.as_str() == target)
        {
            return true;
        }
        if self
            .model
            .conflicts()
            .iter()
            .any(|c| c.value.conflict_id.as_str() == target)
        {
            return true;
        }
        self.model.pages().iter().any(|p| p.id.as_str() == target)
    }

    pub(crate) const fn model(&self) -> &Model {
        &self.model
    }
}

trait SourceRowExt {
    fn version_id_str(&self) -> String;
}

impl SourceRowExt for SourceRow {
    fn version_id_str(&self) -> String {
        format!("{}-v{}", self.source_id, self.version)
    }
}
