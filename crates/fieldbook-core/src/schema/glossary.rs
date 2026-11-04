//! Typed row representation and cell parser for `corpus/registers/glossary.csv`.

use crate::ids::{AreaCode, TermId};
use crate::schema::rows::{
    CellProblem, MAX_BOUNDARY_RULE_CHARS, MAX_DEFINITION_CHARS, MAX_TEXT_CHARS, RefId, join_pipe,
    opt_id, opt_id_list, opt_text_list, req_id, req_text, req_vocab,
};
use crate::schema::table::Row;
use crate::schema::vocab::DefinitionStatus;

/// One typed row of `glossary.csv` (9 columns).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlossaryRow {
    /// Column 1: `term_id` (snake_case).
    pub term_id: TermId,
    /// Column 2: `label` (at most 1,000 characters).
    pub label: String,
    /// Column 3: `definition` (at most 400 characters).
    pub definition: String,
    /// Column 4: `boundary_rule` (at most 300 characters).
    pub boundary_rule: String,
    /// Column 5: `owner_area`.
    pub owner_area: AreaCode,
    /// Column 6: `source_ref` (claim ids and source version ids behind the definition).
    pub source_ref: Vec<RefId>,
    /// Column 7: `definition_status`.
    pub definition_status: DefinitionStatus,
    /// Column 8: `aliases` (synonyms and acronyms separated by `|`).
    pub aliases: Vec<String>,
    /// Column 9: `superseded_by`.
    pub superseded_by: Option<TermId>,
}

impl GlossaryRow {
    /// Parses a [`Row`] of `glossary.csv` one cell at a time, collecting all single-cell
    /// [`CellProblem`]s.
    pub fn from_row(row: &Row) -> Result<Self, Vec<CellProblem>> {
        let mut problems = Vec::new();

        let term_id = req_id(row, 0, "term_id", TermId::parse, &mut problems);
        let label = req_text(row, 1, "label", MAX_TEXT_CHARS, &mut problems);
        let definition = req_text(row, 2, "definition", MAX_DEFINITION_CHARS, &mut problems);
        let boundary_rule = req_text(
            row,
            3,
            "boundary_rule",
            MAX_BOUNDARY_RULE_CHARS,
            &mut problems,
        );
        let owner_area = req_id(row, 4, "owner_area", AreaCode::parse, &mut problems);
        let source_ref = opt_id_list(row, 5, "source_ref", RefId::parse, &mut problems);
        let definition_status = req_vocab(
            row,
            6,
            "definition_status",
            DefinitionStatus::parse,
            &mut problems,
        );
        let aliases = opt_text_list(row, 7, "aliases", &mut problems);
        let superseded_by = opt_id(row, 8, "superseded_by", TermId::parse, &mut problems);

        let (
            Some(term_id),
            Some(label),
            Some(definition),
            Some(boundary_rule),
            Some(owner_area),
            Some(source_ref),
            Some(definition_status),
            Some(aliases),
            Some(superseded_by),
        ) = (
            term_id,
            label,
            definition,
            boundary_rule,
            owner_area,
            source_ref,
            definition_status,
            aliases,
            superseded_by,
        )
        else {
            return Err(problems);
        };

        Ok(Self {
            term_id,
            label,
            definition,
            boundary_rule,
            owner_area,
            source_ref,
            definition_status,
            aliases,
            superseded_by,
        })
    }

    /// Formats this [`GlossaryRow`] back into its 9 CSV cell strings in column order.
    #[must_use]
    pub fn to_cells(&self) -> Vec<String> {
        vec![
            self.term_id.to_string(),
            self.label.clone(),
            self.definition.clone(),
            self.boundary_rule.clone(),
            self.owner_area.to_string(),
            join_pipe(&self.source_ref),
            self.definition_status.as_str().to_owned(),
            self.aliases.join("|"),
            self.superseded_by
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
            "material_weakness".to_owned(),
            "material weakness".to_owned(),
            "A deficiency in internal control.".to_owned(),
            "Distinct from a significant deficiency.".to_owned(),
            "cmp".to_owned(),
            "S-0001-v1".to_owned(),
            "public_reference".to_owned(),
            "MW|material control weakness".to_owned(),
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
    fn valid_glossary_row_round_trips_and_problem_kinds_report() {
        let cells = valid_cells();
        let parsed = GlossaryRow::from_row(&row_from(cells.clone())).expect("valid glossary row");
        assert_eq!(parsed.to_cells(), cells);

        // Empty label -> Empty
        let mut empty_label = valid_cells();
        empty_label[1] = String::new();
        assert_eq!(
            GlossaryRow::from_row(&row_from(empty_label)).expect_err("empty label"),
            vec![CellProblem::new("label", CellProblemKind::Empty)]
        );

        // Form -> bad term_id or space around bar
        let mut bad_id = valid_cells();
        bad_id[0] = "Bad-Id".to_owned();
        assert_eq!(
            GlossaryRow::from_row(&row_from(bad_id)).expect_err("bad term_id"),
            vec![CellProblem::new("term_id", CellProblemKind::Form)]
        );

        // Vocabulary -> definition_status = "draft"
        let mut bad_status = valid_cells();
        bad_status[6] = "draft".to_owned();
        assert_eq!(
            GlossaryRow::from_row(&row_from(bad_status)).expect_err("bad definition_status"),
            vec![CellProblem::new(
                "definition_status",
                CellProblemKind::Vocabulary
            )]
        );

        // TooLong -> definition > 400 chars, boundary_rule > 300 chars
        let mut long_def = valid_cells();
        long_def[2] = "d".repeat(401);
        assert_eq!(
            GlossaryRow::from_row(&row_from(long_def)).expect_err("401-char definition"),
            vec![CellProblem::new("definition", CellProblemKind::TooLong)]
        );

        let mut long_rule = valid_cells();
        long_rule[3] = "b".repeat(301);
        assert_eq!(
            GlossaryRow::from_row(&row_from(long_rule)).expect_err("301-char boundary_rule"),
            vec![CellProblem::new("boundary_rule", CellProblemKind::TooLong)]
        );
    }
}
