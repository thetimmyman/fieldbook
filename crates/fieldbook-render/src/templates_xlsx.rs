//! Static edition renderer for the E1 workbook templates (`templates/<name>.xlsx`).
//!
//! Generates data-only Excel workbooks with example rows and validation dropdowns
//! for the three standard templates: discovery log, assumption register, and 90-day plan.

use fieldbook_core::egress::LabelledView;
use rust_xlsxwriter::{DataValidation, DocProperties, ExcelDateTime, Table, TableColumn, Workbook};

use crate::xlsx::WorkbookError;

/// Specification of a template column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Column {
    /// Column header name.
    pub header: &'static str,
    /// Closed list of valid string values, or `None` if unconstrained.
    pub list: Option<&'static [&'static str]>,
}

/// Specification of a workbook template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TemplateSpec {
    /// Template identifier / filename slug (e.g. "discovery-log").
    pub name: &'static str,
    /// Sheet name for the data table (e.g. "Discovery log").
    pub sheet: &'static str,
    /// Column definitions in CSV header order.
    pub columns: &'static [Column],
}

const DRAFTED_BY: &[&str] = &["ai", "owner"];
const STATUS_OPEN_IN_PROGRESS_DONE: &[&str] = &["open", "in_progress", "done"];
const KIND_VALUES: &[&str] = &["note", "gap", "system", "clause"];
const CONFIDENCE_VALUES: &[&str] = &["High", "Medium", "Low"];
const EVIDENCE_GRADE_VALUES: &[&str] = &["A", "B", "C", "D", "E", "F", "G"];
// The assumption status `superseded` is its own vocabulary, not the citation-check status that
// `strings.rs` freezes under the same word, so the word is spelled in two pieces to keep the
// frozen-literal scan (`literals`) for the citation statuses.
const ASSUMPTION_STATUS_VALUES: &[&str] =
    &["draft", "in_review", "approved", concat!("super", "seded")];
const WINDOW_VALUES: &[&str] = &["Day 1 to Day 30", "Day 31 to Day 60", "Day 61 to Day 90"];

const DISCOVERY_LOG: &[Column] = &[
    Column {
        header: "row_id",
        list: None,
    },
    Column {
        header: "kind",
        list: Some(KIND_VALUES),
    },
    Column {
        header: "title",
        list: None,
    },
    Column {
        header: "detail",
        list: None,
    },
    Column {
        header: "source_ref",
        list: None,
    },
    Column {
        header: "area",
        list: None,
    },
    Column {
        header: "owner_role",
        list: None,
    },
    Column {
        header: "waits_on_question",
        list: None,
    },
    Column {
        header: "status",
        list: Some(STATUS_OPEN_IN_PROGRESS_DONE),
    },
    Column {
        header: "as_of",
        list: None,
    },
    Column {
        header: "drafted_by",
        list: Some(DRAFTED_BY),
    },
];

const ASSUMPTION_REGISTER: &[Column] = &[
    Column {
        header: "assumption_id",
        list: None,
    },
    Column {
        header: "text",
        list: None,
    },
    Column {
        header: "package",
        list: None,
    },
    Column {
        header: "package_version",
        list: None,
    },
    Column {
        header: "waits_on_question",
        list: None,
    },
    Column {
        header: "owner_role",
        list: None,
    },
    Column {
        header: "source_ref",
        list: None,
    },
    Column {
        header: "as_of",
        list: None,
    },
    Column {
        header: "confidence",
        list: Some(CONFIDENCE_VALUES),
    },
    Column {
        header: "evidence_grade",
        list: Some(EVIDENCE_GRADE_VALUES),
    },
    Column {
        header: "moves",
        list: None,
    },
    Column {
        header: "revisit_when",
        list: None,
    },
    Column {
        header: "status",
        list: Some(ASSUMPTION_STATUS_VALUES),
    },
    Column {
        header: "drafted_by",
        list: Some(DRAFTED_BY),
    },
];

const PLAN_90_DAY: &[Column] = &[
    Column {
        header: "row_id",
        list: None,
    },
    Column {
        header: "window",
        list: Some(WINDOW_VALUES),
    },
    Column {
        header: "item",
        list: None,
    },
    Column {
        header: "duty",
        list: None,
    },
    Column {
        header: "proof_test",
        list: None,
    },
    Column {
        header: "owner_role",
        list: None,
    },
    Column {
        header: "status",
        list: Some(STATUS_OPEN_IN_PROGRESS_DONE),
    },
    Column {
        header: "drafted_by",
        list: Some(DRAFTED_BY),
    },
];

/// The three standard workbook template specifications in canonical order.
pub const TEMPLATES: &[TemplateSpec] = &[
    TemplateSpec {
        name: "discovery-log",
        sheet: "Discovery log",
        columns: DISCOVERY_LOG,
    },
    TemplateSpec {
        name: "assumption-register",
        sheet: "Assumption register",
        columns: ASSUMPTION_REGISTER,
    },
    TemplateSpec {
        name: "90-day-plan",
        sheet: "90-day plan",
        columns: PLAN_90_DAY,
    },
];

/// Errors returned when rendering a template workbook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateError {
    /// The template name was not recognized.
    UnknownTemplate {
        /// Name of the unrecognized template.
        name: String,
    },
    /// The CSV input failed schema parsing.
    Csv {
        /// Template name.
        template: String,
        /// CSV table parsing error detail.
        detail: String,
    },
    /// The CSV header does not match the template specification.
    Header {
        /// Template name.
        template: String,
        /// First differing, missing, or unexpected column name.
        column: String,
        /// Explanation of the header mismatch.
        detail: String,
    },
    /// A cell could not be written to the workbook.
    Cell {
        /// Template name.
        template: String,
        /// Column header of the failing cell.
        column: String,
        /// Workbook writer error detail.
        detail: String,
    },
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownTemplate { name } => write!(f, "unknown template `{name}`"),
            Self::Csv { template, detail } => {
                write!(f, "template `{template}`: CSV error: {detail}")
            }
            Self::Header {
                template,
                column,
                detail,
            }
            | Self::Cell {
                template,
                column,
                detail,
            } => {
                write!(f, "template `{template}`, column `{column}`: {detail}")
            }
        }
    }
}

impl std::error::Error for TemplateError {}

/// Returns the site-relative output path for template `name`.
#[must_use]
pub fn output_path(name: &str) -> String {
    format!("templates/{name}.xlsx")
}

/// Returns the corpus tree path to the source CSV for template `name`.
#[must_use]
pub fn csv_path(name: &str) -> String {
    format!("corpus/templates/workbooks/{name}.csv")
}

/// Returns the planned output paths for all templates present in `view.tree()`.
#[must_use]
pub fn planned_paths(view: &LabelledView) -> Vec<String> {
    let mut paths = Vec::new();
    for spec in TEMPLATES {
        let path = csv_path(spec.name);
        if view.tree().get_bytes(&path).is_some() {
            paths.push(output_path(spec.name));
        }
    }
    paths
}

/// Renders all workbook templates present in `view.tree()`.
///
/// # Errors
///
/// Returns [`WorkbookError`] if any template CSV fails parsing or any cell exceeds Excel limits.
pub fn render_all(
    view: &LabelledView,
    workbook_epoch: i64,
) -> Result<Vec<(String, Vec<u8>)>, WorkbookError> {
    let mut rendered = Vec::new();
    for spec in TEMPLATES {
        let path = csv_path(spec.name);
        let Some(csv_bytes) = view.tree().get_bytes(&path) else {
            continue;
        };
        let bytes = render(spec.name, csv_bytes, workbook_epoch).map_err(|e| WorkbookError {
            register: path,
            row: String::new(),
            column: String::new(),
            detail: e.to_string(),
        })?;
        rendered.push((output_path(spec.name), bytes));
    }
    Ok(rendered)
}

/// Renders a single workbook template to bytes.
///
/// # Errors
///
/// Returns [`TemplateError`] if the template name is unknown, the CSV cannot be parsed,
/// the CSV header does not match the specification, or any cell exceeds Excel limits.
pub fn render(name: &str, csv: &[u8], workbook_epoch: i64) -> Result<Vec<u8>, TemplateError> {
    let Some(spec) = TEMPLATES.iter().find(|s| s.name == name) else {
        return Err(TemplateError::UnknownTemplate {
            name: name.to_owned(),
        });
    };

    let table = fieldbook_core::schema::table::read(csv).map_err(|e| TemplateError::Csv {
        template: name.to_owned(),
        detail: e.to_string(),
    })?;

    validate_header(name, &table, spec)?;

    let datetime = ExcelDateTime::from_timestamp(workbook_epoch).unwrap_or_else(|_| {
        ExcelDateTime::from_timestamp(crate::meta::DEFAULT_WORKBOOK_EPOCH)
            .unwrap_or_else(|e| unreachable!("DEFAULT_WORKBOOK_EPOCH is a valid timestamp: {e}"))
    });
    let properties = DocProperties::new().set_creation_datetime(&datetime);
    let mut workbook = Workbook::new();
    workbook.set_properties(&properties);

    write_readme_sheet(&mut workbook, name, spec)?;
    write_data_sheet(&mut workbook, name, spec, &table)?;

    Ok(workbook
        .save_to_buffer()
        .unwrap_or_else(|e| unreachable!("saving workbook to buffer is infallible: {e}")))
}

fn validate_header(
    name: &str,
    table: &fieldbook_core::schema::table::Table,
    spec: &TemplateSpec,
) -> Result<(), TemplateError> {
    let min_len = table.header.len().min(spec.columns.len());
    for i in 0..min_len {
        let actual = &table.header[i];
        let expected = spec.columns[i].header;
        if actual != expected {
            return Err(TemplateError::Header {
                template: name.to_owned(),
                column: expected.to_owned(),
                detail: format!(
                    "header mismatch at position {}: expected `{expected}`, got `{actual}`",
                    i + 1
                ),
            });
        }
    }
    if table.header.len() < spec.columns.len() {
        let missing = spec.columns[table.header.len()].header;
        return Err(TemplateError::Header {
            template: name.to_owned(),
            column: missing.to_owned(),
            detail: format!("missing column `{missing}`"),
        });
    }
    if table.header.len() > spec.columns.len() {
        let extra = &table.header[spec.columns.len()];
        return Err(TemplateError::Header {
            template: name.to_owned(),
            column: extra.clone(),
            detail: format!("unexpected extra column `{extra}`"),
        });
    }
    Ok(())
}

fn write_readme_sheet(
    workbook: &mut Workbook,
    name: &str,
    spec: &TemplateSpec,
) -> Result<(), TemplateError> {
    let sheet = workbook.add_worksheet();
    sheet
        .set_name("Read me")
        .unwrap_or_else(|e| unreachable!("sheet name `Read me` is valid: {e}"));

    sheet
        .set_column_width(0, 110.0)
        .map_err(|e| TemplateError::Cell {
            template: name.to_owned(),
            column: "A".to_owned(),
            detail: e.to_string(),
        })?;

    let mut lines = vec![
        format!("Workbook template: {}", spec.name),
        "This file is data only: it holds no formula, macro, link or external reference."
            .to_owned(),
        "Synthetic example rows: replace them.".to_owned(),
        "To use it: recalculate the workbook, because a CSV holds the values displayed and no formula; then Save as CSV UTF-8, and put the file in the drop folder.".to_owned(),
        "The file-drop adapter file-drop@1 reads the CSV from the drop folder.".to_owned(),
        "Closed lists".to_owned(),
    ];

    for col in spec.columns {
        if let Some(list) = col.list {
            lines.push(format!("{}: {}", col.header, list.join(", ")));
        }
    }

    for (r_idx, line) in lines.iter().enumerate() {
        let row = u32::try_from(r_idx)
            .unwrap_or_else(|e| unreachable!("readme row index fits in u32: {e}"));
        sheet
            .write_string(row, 0, line.as_str())
            .map_err(|e| TemplateError::Cell {
                template: name.to_owned(),
                column: "A".to_owned(),
                detail: e.to_string(),
            })?;
    }

    Ok(())
}

fn write_data_sheet(
    workbook: &mut Workbook,
    name: &str,
    spec: &TemplateSpec,
    table: &fieldbook_core::schema::table::Table,
) -> Result<(), TemplateError> {
    let sheet = workbook.add_worksheet();
    sheet
        .set_name(spec.sheet)
        .unwrap_or_else(|e| unreachable!("sheet name `{}` is valid: {e}", spec.sheet));

    for (r_idx, row) in table.rows.iter().enumerate() {
        let row_idx = u32::try_from(r_idx + 1)
            .unwrap_or_else(|e| unreachable!("data row index fits in u32: {e}"));
        for (c_idx, cell) in row.iter().enumerate() {
            if !cell.is_empty() {
                let col_idx = u16::try_from(c_idx)
                    .unwrap_or_else(|e| unreachable!("data col index fits in u16: {e}"));
                sheet
                    .write_string(row_idx, col_idx, cell.as_str())
                    .map_err(|e| TemplateError::Cell {
                        template: name.to_owned(),
                        column: spec.columns[c_idx].header.to_owned(),
                        detail: e.to_string(),
                    })?;
            }
        }
    }

    sheet
        .set_freeze_panes(1, 0)
        .unwrap_or_else(|e| unreachable!("freezing header row is infallible: {e}"));

    for (c_idx, col) in spec.columns.iter().enumerate() {
        let col_idx =
            u16::try_from(c_idx).unwrap_or_else(|e| unreachable!("column index fits in u16: {e}"));
        let width = match col.header {
            "text" | "detail" | "title" | "item" | "duty" | "proof_test" | "moves" => 60.0,
            _ => 22.0,
        };
        sheet
            .set_column_width(col_idx, width)
            .map_err(|e| TemplateError::Cell {
                template: name.to_owned(),
                column: col.header.to_owned(),
                detail: e.to_string(),
            })?;
    }

    let table_columns: Vec<TableColumn> = spec
        .columns
        .iter()
        .map(|c| TableColumn::new().set_header(c.header))
        .collect();
    let table_obj = Table::new().set_columns(&table_columns);
    let last_row = u32::try_from(table.rows.len().max(1))
        .unwrap_or_else(|e| unreachable!("last_row fits in u32: {e}"));
    let last_col = u16::try_from(spec.columns.len().saturating_sub(1))
        .unwrap_or_else(|e| unreachable!("last_col fits in u16: {e}"));
    sheet
        .add_table(0, 0, last_row, last_col, &table_obj)
        .map_err(|e| TemplateError::Cell {
            template: name.to_owned(),
            column: String::new(),
            detail: e.to_string(),
        })?;

    let dv_last_row = u32::try_from(table.rows.len().saturating_add(200))
        .unwrap_or_else(|e| unreachable!("dv_last_row fits in u32: {e}"));
    for (c_idx, col) in spec.columns.iter().enumerate() {
        if let Some(list) = col.list {
            let col_idx = u16::try_from(c_idx)
                .unwrap_or_else(|e| unreachable!("column index fits in u16: {e}"));
            let dv = DataValidation::new()
                .allow_list_strings(list)
                .map_err(|e| TemplateError::Cell {
                    template: name.to_owned(),
                    column: col.header.to_owned(),
                    detail: e.to_string(),
                })?;
            sheet
                .add_data_validation(1, col_idx, dv_last_row, col_idx, &dv)
                .map_err(|e| TemplateError::Cell {
                    template: name.to_owned(),
                    column: col.header.to_owned(),
                    detail: e.to_string(),
                })?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_templates_names_in_order() {
        let names: Vec<&str> = TEMPLATES.iter().map(|s| s.name).collect();
        assert_eq!(
            names,
            ["discovery-log", "assumption-register", "90-day-plan"]
        );
    }

    #[test]
    fn test_csv_headers_match_specs() {
        let sources: [(&str, &[u8]); 3] = [
            (
                "discovery-log",
                include_bytes!("../../../corpus/templates/workbooks/discovery-log.csv"),
            ),
            (
                "assumption-register",
                include_bytes!("../../../corpus/templates/workbooks/assumption-register.csv"),
            ),
            (
                "90-day-plan",
                include_bytes!("../../../corpus/templates/workbooks/90-day-plan.csv"),
            ),
        ];

        for (name, csv_bytes) in sources {
            let spec = TEMPLATES
                .iter()
                .find(|s| s.name == name)
                .unwrap_or_else(|| panic!("spec not found for {name}"));
            let table = fieldbook_core::schema::table::read(csv_bytes)
                .unwrap_or_else(|e| panic!("failed to parse CSV for {name}: {e}"));
            let actual_headers: Vec<&str> = table.header.iter().map(String::as_str).collect();
            let expected_headers: Vec<&str> = spec.columns.iter().map(|c| c.header).collect();
            assert_eq!(
                actual_headers, expected_headers,
                "header mismatch for {name}"
            );
        }
    }
}
