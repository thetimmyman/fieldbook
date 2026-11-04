//! Typed row representation and cell parser for `corpus/registers/sources.csv`.

use crate::date::{CivilDate, PartialDate, Volatility};
use crate::ids::{SourceId, VersionId};
use crate::schema::rows::{
    CellProblem, MAX_TAG_CHARS, MAX_TEXT_CHARS, cell_at, join_pipe, opt_id_list, opt_pdate,
    opt_snake_case_text, opt_text, opt_vocab, req_date, req_id, req_pdate, req_positive_u32,
    req_sha256, req_text, req_vocab,
};
use crate::schema::table::Row;
use crate::schema::vocab::{AuthorityTier, FamilyBasis, Lifecycle, SourceKind, SourceRole};

/// One typed row of `sources.csv` (28 columns).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRow {
    /// Column 1: `source_id` (`S-nnnn`).
    pub source_id: SourceId,
    /// Column 2: `version` (`1, 2, ...`).
    pub version: u32,
    /// Column 3: `tag` (at most 40 characters).
    pub tag: Option<String>,
    /// Column 4: `title`.
    pub title: String,
    /// Column 5: `publisher`.
    pub publisher: String,
    /// Column 6: `source_kind`.
    pub source_kind: SourceKind,
    /// Column 7: `source_role`.
    pub source_role: SourceRole,
    /// Column 8: `authority_tier` (`A`..`D`, or `None` for fixtures and company kinds).
    pub authority_tier: Option<AuthorityTier>,
    /// Column 9: `licence`.
    pub licence: String,
    /// Column 10: `source_family` (optional snake_case key).
    pub source_family: Option<String>,
    /// Column 11: `family_basis`.
    pub family_basis: Option<FamilyBasis>,
    /// Column 12: `location`.
    pub location: String,
    /// Column 13: `sha256` (64 lowercase hex characters).
    pub sha256: String,
    /// Column 14: `published`.
    pub published: Option<PartialDate>,
    /// Column 15: `effective_from`.
    pub effective_from: Option<PartialDate>,
    /// Column 16: `as_of`.
    pub as_of: PartialDate,
    /// Column 17: `retrieved`.
    pub retrieved: CivilDate,
    /// Column 18: `lifecycle`.
    pub lifecycle: Lifecycle,
    /// Column 19: `supersedes`.
    pub supersedes: Vec<VersionId>,
    /// Column 20: `volatility`.
    pub volatility: Volatility,
    /// Column 21: `handling` (split on `|` and retained as written for FB0410).
    pub handling: Vec<String>,
    /// Column 22: `note`.
    pub note: Option<String>,
    /// Column 23: `purpose`.
    pub purpose: Option<String>,
    /// Column 24: `origin_system`.
    pub origin_system: Option<String>,
    /// Column 25: `origin_stream`.
    pub origin_stream: Option<String>,
    /// Column 26: `origin_key`.
    pub origin_key: Option<String>,
    /// Column 27: `doc_kind` (read as text here; FB0215 validates against `DOC_KINDS`).
    pub doc_kind: Option<String>,
    /// Column 28: `series` (read as text here; FB0215 validates against `stream_id_form`).
    pub series: Option<String>,
}

fn parse_volatility(raw: &str) -> Result<Volatility, crate::schema::vocab::VocabError> {
    Volatility::parse(raw).ok_or_else(|| crate::schema::vocab::VocabError::new("volatility", raw))
}

impl SourceRow {
    /// Parses a [`Row`] of `sources.csv` one cell at a time, collecting all single-cell
    /// [`CellProblem`]s.
    pub fn from_row(row: &Row) -> Result<Self, Vec<CellProblem>> {
        let mut problems = Vec::new();

        let source_id = req_id(row, 0, "source_id", SourceId::parse, &mut problems);
        let version = req_positive_u32(row, 1, "version", &mut problems);
        let tag = opt_text(row, 2, "tag", MAX_TAG_CHARS, &mut problems);
        let title = req_text(row, 3, "title", MAX_TEXT_CHARS, &mut problems);
        let publisher = req_text(row, 4, "publisher", MAX_TEXT_CHARS, &mut problems);
        let source_kind = req_vocab(row, 5, "source_kind", SourceKind::parse, &mut problems);
        let source_role = req_vocab(row, 6, "source_role", SourceRole::parse, &mut problems);
        let authority_tier = opt_vocab(
            row,
            7,
            "authority_tier",
            AuthorityTier::parse,
            &mut problems,
        );
        let licence = req_text(row, 8, "licence", MAX_TEXT_CHARS, &mut problems);
        let source_family = opt_snake_case_text(row, 9, "source_family", &mut problems);
        let family_basis = opt_vocab(row, 10, "family_basis", FamilyBasis::parse, &mut problems);
        let location = req_text(row, 11, "location", MAX_TEXT_CHARS, &mut problems);
        let sha256 = req_sha256(row, 12, "sha256", &mut problems);
        let published = opt_pdate(row, 13, "published", &mut problems);
        let effective_from = opt_pdate(row, 14, "effective_from", &mut problems);
        let as_of = req_pdate(row, 15, "as_of", &mut problems);
        let retrieved = req_date(row, 16, "retrieved", &mut problems);
        let lifecycle = req_vocab(row, 17, "lifecycle", Lifecycle::parse, &mut problems);
        let supersedes = opt_id_list(row, 18, "supersedes", VersionId::parse, &mut problems);
        let volatility = req_vocab(row, 19, "volatility", parse_volatility, &mut problems);

        let raw_handling = cell_at(row, 20);
        let handling = if raw_handling.is_empty() {
            Vec::new()
        } else {
            raw_handling.split('|').map(str::to_owned).collect()
        };

        let note = opt_text(row, 21, "note", MAX_TEXT_CHARS, &mut problems);
        let purpose = opt_text(row, 22, "purpose", MAX_TEXT_CHARS, &mut problems);
        let origin_system = opt_text(row, 23, "origin_system", MAX_TEXT_CHARS, &mut problems);
        let origin_stream = opt_text(row, 24, "origin_stream", MAX_TEXT_CHARS, &mut problems);
        let origin_key = opt_text(row, 25, "origin_key", MAX_TEXT_CHARS, &mut problems);
        let doc_kind = opt_text(row, 26, "doc_kind", MAX_TEXT_CHARS, &mut problems);
        let series = opt_text(row, 27, "series", MAX_TEXT_CHARS, &mut problems);

        let (
            Some(source_id),
            Some(version),
            Some(tag),
            Some(title),
            Some(publisher),
            Some(source_kind),
            Some(source_role),
            Some(authority_tier),
            Some(licence),
            Some(source_family),
            Some(family_basis),
            Some(location),
            Some(sha256),
            Some(published),
            Some(effective_from),
            Some(as_of),
            Some(retrieved),
            Some(lifecycle),
            Some(supersedes),
            Some(volatility),
            Some(note),
            Some(purpose),
            Some(origin_system),
            Some(origin_stream),
            Some(origin_key),
            Some(doc_kind),
            Some(series),
        ) = (
            source_id,
            version,
            tag,
            title,
            publisher,
            source_kind,
            source_role,
            authority_tier,
            licence,
            source_family,
            family_basis,
            location,
            sha256,
            published,
            effective_from,
            as_of,
            retrieved,
            lifecycle,
            supersedes,
            volatility,
            note,
            purpose,
            origin_system,
            origin_stream,
            origin_key,
            doc_kind,
            series,
        )
        else {
            return Err(problems);
        };

        Ok(Self {
            source_id,
            version,
            tag,
            title,
            publisher,
            source_kind,
            source_role,
            authority_tier,
            licence,
            source_family,
            family_basis,
            location,
            sha256,
            published,
            effective_from,
            as_of,
            retrieved,
            lifecycle,
            supersedes,
            volatility,
            handling,
            note,
            purpose,
            origin_system,
            origin_stream,
            origin_key,
            doc_kind,
            series,
        })
    }

    /// Formats this [`SourceRow`] back into its 28 CSV cell strings in column order.
    #[must_use]
    pub fn to_cells(&self) -> Vec<String> {
        vec![
            self.source_id.to_string(),
            self.version.to_string(),
            self.tag.clone().unwrap_or_default(),
            self.title.clone(),
            self.publisher.clone(),
            self.source_kind.as_str().to_owned(),
            self.source_role.as_str().to_owned(),
            self.authority_tier
                .map_or_else(String::new, |t| t.as_str().to_owned()),
            self.licence.clone(),
            self.source_family.clone().unwrap_or_default(),
            self.family_basis
                .map_or_else(String::new, |b| b.as_str().to_owned()),
            self.location.clone(),
            self.sha256.clone(),
            self.published.map_or_else(String::new, |d| d.to_string()),
            self.effective_from
                .map_or_else(String::new, |d| d.to_string()),
            self.as_of.to_string(),
            self.retrieved.to_string(),
            self.lifecycle.as_str().to_owned(),
            join_pipe(&self.supersedes),
            self.volatility.as_str().to_owned(),
            self.handling.join("|"),
            self.note.clone().unwrap_or_default(),
            self.purpose.clone().unwrap_or_default(),
            self.origin_system.clone().unwrap_or_default(),
            self.origin_stream.clone().unwrap_or_default(),
            self.origin_key.clone().unwrap_or_default(),
            self.doc_kind.clone().unwrap_or_default(),
            self.series.clone().unwrap_or_default(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::rows::CellProblemKind;

    fn valid_cells() -> Vec<String> {
        vec![
            "S-0001".to_owned(),
            "1".to_owned(),
            "FAR 15".to_owned(),
            "48 CFR Part 15".to_owned(),
            "Office of the Federal Register".to_owned(),
            "primary_law".to_owned(),
            "primary_evidence".to_owned(),
            "A".to_owned(),
            "US Government work, public domain".to_owned(),
            "far_15".to_owned(),
            "reviewed".to_owned(),
            "S-0001-v1.pdf".to_owned(),
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_owned(),
            "2025-10".to_owned(),
            "2025-10-01".to_owned(),
            "2025-11-01".to_owned(),
            "2025-11-02".to_owned(),
            "active".to_owned(),
            String::new(),
            "V1".to_owned(),
            "public".to_owned(),
            "Sample note".to_owned(),
            "Sample purpose".to_owned(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ]
    }

    fn row_from(cells: Vec<String>) -> Row {
        let cols = (1..=u32::try_from(cells.len()).unwrap_or(1)).collect();
        Row {
            line: 2,
            cols,
            cells,
        }
    }

    #[test]
    fn valid_source_row_round_trips_and_problem_kinds_report() {
        let cells = valid_cells();
        let parsed = SourceRow::from_row(&row_from(cells.clone())).expect("valid row");
        assert_eq!(parsed.to_cells(), cells);

        // Empty title -> Empty
        let mut empty_title = valid_cells();
        empty_title[3] = String::new();
        assert_eq!(
            SourceRow::from_row(&row_from(empty_title)).expect_err("empty title"),
            vec![CellProblem::new("title", CellProblemKind::Empty)]
        );

        // retrieved = "2025-02-30" and "2025-02" -> CalendarDate
        let mut imp_date = valid_cells();
        imp_date[16] = "2025-02-30".to_owned();
        assert_eq!(
            SourceRow::from_row(&row_from(imp_date)).expect_err("impossible date"),
            vec![CellProblem::new("retrieved", CellProblemKind::CalendarDate)]
        );

        let mut partial_date = valid_cells();
        partial_date[16] = "2025-02".to_owned();
        assert_eq!(
            SourceRow::from_row(&row_from(partial_date)).expect_err("partial date in full date"),
            vec![CellProblem::new("retrieved", CellProblemKind::CalendarDate)]
        );

        // version = "x" -> Form
        let mut bad_ver = valid_cells();
        bad_ver[1] = "x".to_owned();
        assert_eq!(
            SourceRow::from_row(&row_from(bad_ver)).expect_err("version = x"),
            vec![CellProblem::new("version", CellProblemKind::Form)]
        );

        // source_kind = "blog" -> Vocabulary
        let mut bad_kind = valid_cells();
        bad_kind[5] = "blog".to_owned();
        assert_eq!(
            SourceRow::from_row(&row_from(bad_kind)).expect_err("source_kind = blog"),
            vec![CellProblem::new("source_kind", CellProblemKind::Vocabulary)]
        );

        // tag > 40 chars -> TooLong
        let mut long_tag = valid_cells();
        long_tag[2] = "t".repeat(41);
        assert_eq!(
            SourceRow::from_row(&row_from(long_tag)).expect_err("41-char tag"),
            vec![CellProblem::new("tag", CellProblemKind::TooLong)]
        );

        // pipe in text cell -> Form
        let mut pipe_in_title = valid_cells();
        pipe_in_title[3] = "Bad | title".to_owned();
        assert_eq!(
            SourceRow::from_row(&row_from(pipe_in_title)).expect_err("pipe in title"),
            vec![CellProblem::new("title", CellProblemKind::Form)]
        );

        // space around bar in supersedes -> Form
        let mut space_supersedes = valid_cells();
        space_supersedes[18] = "S-0001-v1 | S-0001-v2".to_owned();
        assert_eq!(
            SourceRow::from_row(&row_from(space_supersedes)).expect_err("space around bar"),
            vec![CellProblem::new("supersedes", CellProblemKind::Form)]
        );

        // doc_kind = "memo" and series = "Bad Name" still read (FB0215 judges them)
        let mut unjudged_tail = valid_cells();
        unjudged_tail[26] = "memo".to_owned();
        unjudged_tail[27] = "Bad Name".to_owned();
        let tail_parsed = SourceRow::from_row(&row_from(unjudged_tail.clone()))
            .expect("doc_kind and series are read as text in from_row");
        assert_eq!(tail_parsed.doc_kind.as_deref(), Some("memo"));
        assert_eq!(tail_parsed.series.as_deref(), Some("Bad Name"));
        assert_eq!(tail_parsed.to_cells(), unjudged_tail);

        // source_family > 64 chars (up to 1,000 chars) is accepted when valid snake_case
        let mut long_family = valid_cells();
        let family_70 = format!("family_{}", "a".repeat(65));
        long_family[9] = family_70.clone();
        let fam_parsed =
            SourceRow::from_row(&row_from(long_family)).expect("72-char snake_case source_family");
        assert_eq!(
            fam_parsed.source_family.as_deref(),
            Some(family_70.as_str())
        );
    }
}
