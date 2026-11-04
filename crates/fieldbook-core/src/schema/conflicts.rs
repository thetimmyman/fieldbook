//! Typed row representation and cell parser for `corpus/registers/conflicts.csv`.

use crate::date::CivilDate;
use crate::ids::{AreaCode, ClaimId, ConflictId, QuestionId};
use crate::schema::rows::{
    CellProblem, MAX_TEXT_CHARS, RefId, join_pipe, opt_date, opt_id, opt_id_list, opt_text, req_id,
    req_id_list, req_text, req_text_list, req_vocab,
};
use crate::schema::table::Row;
use crate::schema::vocab::ConflictStatus;

/// One typed row of `conflicts.csv` (11 columns).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictRow {
    /// Column 1: `conflict_id` (`XC-nn`).
    pub conflict_id: ConflictId,
    /// Column 2: `area`.
    pub area: AreaCode,
    /// Column 3: `topic`.
    pub topic: String,
    /// Column 4: `positions` (two or more competing statements, separated by `|`).
    pub positions: Vec<String>,
    /// Column 5: `resolution` (when resolved).
    pub resolution: Option<String>,
    /// Column 6: `rationale` (when resolved).
    pub rationale: Option<String>,
    /// Column 7: `evidence` (claim ids and source version ids supporting the resolution).
    pub evidence: Vec<RefId>,
    /// Column 8: `resolved_on` (when resolved).
    pub resolved_on: Option<CivilDate>,
    /// Column 9: `status`.
    pub status: ConflictStatus,
    /// Column 10: `claim_ids` (the claims in dispute).
    pub claim_ids: Vec<ClaimId>,
    /// Column 11: `question_id` (when unresolved).
    pub question_id: Option<QuestionId>,
}

impl ConflictRow {
    /// Parses a [`Row`] of `conflicts.csv` one cell at a time, collecting all single-cell
    /// [`CellProblem`]s.
    pub fn from_row(row: &Row) -> Result<Self, Vec<CellProblem>> {
        let mut problems = Vec::new();

        let conflict_id = req_id(row, 0, "conflict_id", ConflictId::parse, &mut problems);
        let area = req_id(row, 1, "area", AreaCode::parse, &mut problems);
        let topic = req_text(row, 2, "topic", MAX_TEXT_CHARS, &mut problems);
        let positions = req_text_list(row, 3, "positions", 2, &mut problems);
        let resolution = opt_text(row, 4, "resolution", MAX_TEXT_CHARS, &mut problems);
        let rationale = opt_text(row, 5, "rationale", MAX_TEXT_CHARS, &mut problems);
        let evidence = opt_id_list(row, 6, "evidence", RefId::parse, &mut problems);
        let resolved_on = opt_date(row, 7, "resolved_on", &mut problems);
        let status = req_vocab(row, 8, "status", ConflictStatus::parse, &mut problems);
        let claim_ids = req_id_list(row, 9, "claim_ids", ClaimId::parse, &mut problems);
        let question_id = opt_id(row, 10, "question_id", QuestionId::parse, &mut problems);

        let (
            Some(conflict_id),
            Some(area),
            Some(topic),
            Some(positions),
            Some(resolution),
            Some(rationale),
            Some(evidence),
            Some(resolved_on),
            Some(status),
            Some(claim_ids),
            Some(question_id),
        ) = (
            conflict_id,
            area,
            topic,
            positions,
            resolution,
            rationale,
            evidence,
            resolved_on,
            status,
            claim_ids,
            question_id,
        )
        else {
            return Err(problems);
        };

        Ok(Self {
            conflict_id,
            area,
            topic,
            positions,
            resolution,
            rationale,
            evidence,
            resolved_on,
            status,
            claim_ids,
            question_id,
        })
    }

    /// Formats this [`ConflictRow`] back into its 11 CSV cell strings in column order.
    #[must_use]
    pub fn to_cells(&self) -> Vec<String> {
        vec![
            self.conflict_id.to_string(),
            self.area.to_string(),
            self.topic.clone(),
            self.positions.join("|"),
            self.resolution.clone().unwrap_or_default(),
            self.rationale.clone().unwrap_or_default(),
            join_pipe(&self.evidence),
            self.resolved_on.map_or_else(String::new, |d| d.to_string()),
            self.status.as_str().to_owned(),
            join_pipe(&self.claim_ids),
            self.question_id
                .as_ref()
                .map_or_else(String::new, ToString::to_string),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::rows::CellProblemKind;

    fn valid_cells() -> Vec<String> {
        vec![
            "XC-14".to_owned(),
            "cmp".to_owned(),
            "Withholding cap".to_owned(),
            "Five percent|Ten percent".to_owned(),
            "Use five percent.".to_owned(),
            "Primary text wins.".to_owned(),
            "CL-CMP-0011|S-0042-v2".to_owned(),
            "2025-11-02".to_owned(),
            "resolved".to_owned(),
            "CL-CMP-0011|CL-CMP-0012".to_owned(),
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
    fn valid_conflict_row_round_trips_and_problem_kinds_report() {
        let cells = valid_cells();
        let parsed = ConflictRow::from_row(&row_from(cells.clone())).expect("valid conflict row");
        assert_eq!(parsed.to_cells(), cells);

        // Empty topic -> Empty
        let mut empty_topic = valid_cells();
        empty_topic[2] = String::new();
        assert_eq!(
            ConflictRow::from_row(&row_from(empty_topic)).expect_err("empty topic"),
            vec![CellProblem::new("topic", CellProblemKind::Empty)]
        );

        // Form -> single position or space around bar
        let mut one_pos = valid_cells();
        one_pos[3] = "Only one position".to_owned();
        assert_eq!(
            ConflictRow::from_row(&row_from(one_pos)).expect_err("single position"),
            vec![CellProblem::new("positions", CellProblemKind::Form)]
        );

        // CalendarDate -> resolved_on = "2025-02-30"
        let mut bad_date = valid_cells();
        bad_date[7] = "2025-02-30".to_owned();
        assert_eq!(
            ConflictRow::from_row(&row_from(bad_date)).expect_err("bad resolved_on"),
            vec![CellProblem::new(
                "resolved_on",
                CellProblemKind::CalendarDate
            )]
        );

        // Vocabulary -> status = "pending"
        let mut bad_status = valid_cells();
        bad_status[8] = "pending".to_owned();
        assert_eq!(
            ConflictRow::from_row(&row_from(bad_status)).expect_err("bad status"),
            vec![CellProblem::new("status", CellProblemKind::Vocabulary)]
        );

        // TooLong -> topic > 1,000 chars
        let mut long_topic = valid_cells();
        long_topic[2] = "t".repeat(1_001);
        assert_eq!(
            ConflictRow::from_row(&row_from(long_topic)).expect_err("long topic"),
            vec![CellProblem::new("topic", CellProblemKind::TooLong)]
        );
    }
}
