//! Typed row representation and cell parser for `corpus/registers/questions.csv`.

use crate::date::CivilDate;
use crate::ids::{AreaCode, QuestionId};
use crate::schema::rows::{
    CellProblem, MAX_TEXT_CHARS, opt_date, opt_text, opt_vocab, req_bool, req_id, req_text,
    req_vocab,
};
use crate::schema::table::Row;
use crate::schema::vocab::{Adoption, QuestionStatus, WorkCanContinue};

/// One typed row of `questions.csv` (13 columns).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionRow {
    /// Column 1: `question_id` (`Q-nnn`).
    pub question_id: QuestionId,
    /// Column 2: `area`.
    pub area: AreaCode,
    /// Column 3: `question`.
    pub question: String,
    /// Column 4: `owner_role` (required when `day_one` is true under strict mode, FB0230).
    pub owner_role: Option<String>,
    /// Column 5: `blocks`.
    pub blocks: Option<String>,
    /// Column 6: `why_it_matters`.
    pub why_it_matters: Option<String>,
    /// Column 7: `default_assumption`.
    pub default_assumption: Option<String>,
    /// Column 8: `work_can_continue`.
    pub work_can_continue: Option<WorkCanContinue>,
    /// Column 9: `day_one`.
    pub day_one: bool,
    /// Column 10: `status`.
    pub status: QuestionStatus,
    /// Column 11: `answer` (empty in the repository Corpus).
    pub answer: Option<String>,
    /// Column 12: `answered_on` (empty in the repository Corpus).
    pub answered_on: Option<CivilDate>,
    /// Column 13: `adoption` (empty in the repository Corpus).
    pub adoption: Option<Adoption>,
}

impl QuestionRow {
    /// Parses a [`Row`] of `questions.csv` one cell at a time, collecting all single-cell
    /// [`CellProblem`]s.
    pub fn from_row(row: &Row) -> Result<Self, Vec<CellProblem>> {
        let mut problems = Vec::new();

        let question_id = req_id(row, 0, "question_id", QuestionId::parse, &mut problems);
        let area = req_id(row, 1, "area", AreaCode::parse, &mut problems);
        let question = req_text(row, 2, "question", MAX_TEXT_CHARS, &mut problems);
        let owner_role = opt_text(row, 3, "owner_role", MAX_TEXT_CHARS, &mut problems);
        let blocks = opt_text(row, 4, "blocks", MAX_TEXT_CHARS, &mut problems);
        let why_it_matters = opt_text(row, 5, "why_it_matters", MAX_TEXT_CHARS, &mut problems);
        let default_assumption =
            opt_text(row, 6, "default_assumption", MAX_TEXT_CHARS, &mut problems);
        let work_can_continue = opt_vocab(
            row,
            7,
            "work_can_continue",
            WorkCanContinue::parse,
            &mut problems,
        );
        let day_one = req_bool(row, 8, "day_one", &mut problems);
        let status = req_vocab(row, 9, "status", QuestionStatus::parse, &mut problems);
        let answer = opt_text(row, 10, "answer", MAX_TEXT_CHARS, &mut problems);
        let answered_on = opt_date(row, 11, "answered_on", &mut problems);
        let adoption = opt_vocab(row, 12, "adoption", Adoption::parse, &mut problems);

        let (
            Some(question_id),
            Some(area),
            Some(question),
            Some(owner_role),
            Some(blocks),
            Some(why_it_matters),
            Some(default_assumption),
            Some(work_can_continue),
            Some(day_one),
            Some(status),
            Some(answer),
            Some(answered_on),
            Some(adoption),
        ) = (
            question_id,
            area,
            question,
            owner_role,
            blocks,
            why_it_matters,
            default_assumption,
            work_can_continue,
            day_one,
            status,
            answer,
            answered_on,
            adoption,
        )
        else {
            return Err(problems);
        };

        Ok(Self {
            question_id,
            area,
            question,
            owner_role,
            blocks,
            why_it_matters,
            default_assumption,
            work_can_continue,
            day_one,
            status,
            answer,
            answered_on,
            adoption,
        })
    }

    /// Formats this [`QuestionRow`] back into its 13 CSV cell strings in column order.
    #[must_use]
    pub fn to_cells(&self) -> Vec<String> {
        vec![
            self.question_id.to_string(),
            self.area.to_string(),
            self.question.clone(),
            self.owner_role.clone().unwrap_or_default(),
            self.blocks.clone().unwrap_or_default(),
            self.why_it_matters.clone().unwrap_or_default(),
            self.default_assumption.clone().unwrap_or_default(),
            self.work_can_continue
                .map_or_else(String::new, |w| w.as_str().to_owned()),
            self.day_one.to_string(),
            self.status.as_str().to_owned(),
            self.answer.clone().unwrap_or_default(),
            self.answered_on.map_or_else(String::new, |d| d.to_string()),
            self.adoption
                .map_or_else(String::new, |a| a.as_str().to_owned()),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::rows::CellProblemKind;

    fn valid_cells() -> Vec<String> {
        vec![
            "Q-001".to_owned(),
            "opm".to_owned(),
            "Which outcomes take priority?".to_owned(),
            "Executive sponsor".to_owned(),
            "First-month priority plan".to_owned(),
            "Unclear priorities can divert effort.".to_owned(),
            "Retain the public method pending confirmation.".to_owned(),
            "partly".to_owned(),
            "true".to_owned(),
            "open".to_owned(),
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
    fn valid_question_row_round_trips_and_problem_kinds_report() {
        let cells = valid_cells();
        let parsed = QuestionRow::from_row(&row_from(cells.clone())).expect("valid question row");
        assert_eq!(parsed.to_cells(), cells);

        // Empty question -> Empty
        let mut empty_q = valid_cells();
        empty_q[2] = String::new();
        assert_eq!(
            QuestionRow::from_row(&row_from(empty_q)).expect_err("empty question"),
            vec![CellProblem::new("question", CellProblemKind::Empty)]
        );

        // Form -> bad bool or pipe in text
        let mut bad_bool = valid_cells();
        bad_bool[8] = "yes".to_owned();
        assert_eq!(
            QuestionRow::from_row(&row_from(bad_bool)).expect_err("bad bool"),
            vec![CellProblem::new("day_one", CellProblemKind::Form)]
        );

        // CalendarDate -> answered_on = "2025-02-30"
        let mut bad_date = valid_cells();
        bad_date[11] = "2025-02-30".to_owned();
        assert_eq!(
            QuestionRow::from_row(&row_from(bad_date)).expect_err("impossible answered_on"),
            vec![CellProblem::new(
                "answered_on",
                CellProblemKind::CalendarDate
            )]
        );

        // Vocabulary -> status = "closed"
        let mut bad_status = valid_cells();
        bad_status[9] = "closed".to_owned();
        assert_eq!(
            QuestionRow::from_row(&row_from(bad_status)).expect_err("bad status"),
            vec![CellProblem::new("status", CellProblemKind::Vocabulary)]
        );

        // TooLong -> question > 1,000 chars
        let mut long_q = valid_cells();
        long_q[2] = "q".repeat(1_001);
        assert_eq!(
            QuestionRow::from_row(&row_from(long_q)).expect_err("long question"),
            vec![CellProblem::new("question", CellProblemKind::TooLong)]
        );
    }
}
