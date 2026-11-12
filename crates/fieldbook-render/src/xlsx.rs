//! The workbook copy of the registers (`data/registers.xlsx`): data only, every cell a string.

use fieldbook_core::egress::LabelledView;
use fieldbook_core::schema::Register;
use rust_xlsxwriter::{DocProperties, ExcelDateTime, Workbook};

/// Canonical site-relative path to the workbook copy of the registers.
pub const OUTPUT_PATH: &str = fieldbook_core::strings::PATHS[0];

/// Error returned when a cell in a register sheet cannot be written to the workbook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkbookError {
    /// Register sheet name.
    pub register: String,
    /// First cell of the failing row, or empty if the row is empty.
    pub row: String,
    /// Column header name.
    pub column: String,
    /// Error detail from the workbook writer.
    pub detail: String,
}

impl std::fmt::Display for WorkbookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "workbook cell cannot be written: register {}, row {}, column {}: {}",
            self.register, self.row, self.column, self.detail
        )
    }
}

impl std::error::Error for WorkbookError {}

/// Returns the worksheet name for `register` ("sources", "claims", "questions", "conflicts", "glossary").
#[must_use]
pub const fn sheet_name(register: Register) -> &'static str {
    match register {
        Register::Sources => "sources",
        Register::Claims => "claims",
        Register::Questions => "questions",
        Register::Conflicts => "conflicts",
        Register::Glossary => "glossary",
    }
}

/// Writes a single worksheet with header, data rows, freeze pane, and autofilter.
fn write_sheet(
    workbook: &mut Workbook,
    name: &str,
    headers: &[&str],
    rows: &[Vec<String>],
) -> Result<(), WorkbookError> {
    let worksheet = workbook.add_worksheet();
    worksheet
        .set_name(name)
        .unwrap_or_else(|e| unreachable!("sheet name {name} is valid: {e}"));

    for (c_idx, &header) in headers.iter().enumerate() {
        let col = u16::try_from(c_idx)
            .unwrap_or_else(|e| unreachable!("header column index fits in u16: {e}"));
        worksheet
            .write_string(0, col, header)
            .unwrap_or_else(|e| unreachable!("writing header string is infallible: {e}"));
    }

    for (r_idx, row) in rows.iter().enumerate() {
        let row_idx = u32::try_from(r_idx + 1)
            .unwrap_or_else(|e| unreachable!("data row index fits in u32: {e}"));
        for (c_idx, cell) in row.iter().enumerate() {
            if !cell.is_empty() {
                let col_idx = u16::try_from(c_idx)
                    .unwrap_or_else(|e| unreachable!("data col index fits in u16: {e}"));
                worksheet
                    .write_string(row_idx, col_idx, cell)
                    .map_err(|e| WorkbookError {
                        register: name.to_owned(),
                        row: row.first().cloned().unwrap_or_default(),
                        column: headers[c_idx].to_owned(),
                        detail: e.to_string(),
                    })?;
            }
        }
    }

    worksheet
        .set_freeze_panes(1, 0)
        .unwrap_or_else(|e| unreachable!("freezing header row is infallible: {e}"));

    let last_row = u32::try_from(rows.len().max(1))
        .unwrap_or_else(|e| unreachable!("last_row fits in u32: {e}"));
    let last_col = u16::try_from(headers.len().saturating_sub(1))
        .unwrap_or_else(|e| unreachable!("last_col fits in u16: {e}"));

    worksheet
        .autofilter(0, 0, last_row, last_col)
        .unwrap_or_else(|e| unreachable!("setting autofilter is infallible: {e}"));

    Ok(())
}

/// Renders the complete workbook containing all five registers in canonical order.
///
/// # Errors
///
/// Returns [`WorkbookError`] if any register cell exceeds the Excel cell character limit.
pub fn render(view: &LabelledView, workbook_epoch: i64) -> Result<Vec<u8>, WorkbookError> {
    let datetime = ExcelDateTime::from_timestamp(workbook_epoch).unwrap_or_else(|_| {
        ExcelDateTime::from_timestamp(crate::meta::DEFAULT_WORKBOOK_EPOCH)
            .unwrap_or_else(|e| unreachable!("DEFAULT_WORKBOOK_EPOCH is a valid timestamp: {e}"))
    });
    let properties = DocProperties::new().set_creation_datetime(&datetime);
    let mut workbook = Workbook::new();
    workbook.set_properties(&properties);

    for &register in &Register::ALL {
        let headers: Vec<&'static str> = register.columns().iter().map(|col| col.name).collect();
        let data_rows = crate::csv_copy::rows(view, register);
        write_sheet(&mut workbook, sheet_name(register), &headers, &data_rows)?;
    }

    Ok(workbook
        .save_to_buffer()
        .unwrap_or_else(|e| unreachable!("saving workbook to buffer is infallible: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sheet_names() {
        assert_eq!(sheet_name(Register::Sources), "sources");
        assert_eq!(sheet_name(Register::Claims), "claims");
        assert_eq!(sheet_name(Register::Questions), "questions");
        assert_eq!(sheet_name(Register::Conflicts), "conflicts");
        assert_eq!(sheet_name(Register::Glossary), "glossary");
    }

    #[test]
    fn test_write_sheet_and_determinism() {
        let dt = ExcelDateTime::from_timestamp(crate::meta::DEFAULT_WORKBOOK_EPOCH).unwrap();
        let props = DocProperties::new().set_creation_datetime(&dt);

        let headers = ["id", "name", "note"];
        let rows = vec![
            vec!["1".to_string(), "first".to_string(), String::new()],
            vec![
                "2".to_string(),
                "second".to_string(),
                "sample note".to_string(),
            ],
        ];

        let mut wb1 = Workbook::new();
        wb1.set_properties(&props);
        write_sheet(&mut wb1, "test_sheet", &headers, &rows).unwrap();
        let bytes1 = wb1.save_to_buffer().unwrap();

        let mut wb2 = Workbook::new();
        wb2.set_properties(&props);
        write_sheet(&mut wb2, "test_sheet", &headers, &rows).unwrap();
        let bytes2 = wb2.save_to_buffer().unwrap();

        assert_eq!(bytes1, bytes2);
    }

    #[test]
    fn test_write_sheet_empty_data_rows() {
        let mut wb = Workbook::new();
        let headers = ["col1", "col2"];
        let rows: Vec<Vec<String>> = Vec::new();
        write_sheet(&mut wb, "empty", &headers, &rows).unwrap();
        let bytes = wb.save_to_buffer().unwrap();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn test_write_sheet_over_limit_cell() {
        let mut wb = Workbook::new();
        let headers = ["col1", "col2", "col3"];
        let rows = vec![vec![
            "row-id-1".to_string(),
            "second".to_string(),
            "a".repeat(32_768),
        ]];
        let err = write_sheet(&mut wb, "t", &headers, &rows).unwrap_err();
        assert_eq!(err.register, "t");
        assert_eq!(err.row, "row-id-1");
        assert_eq!(err.column, "col3");
    }
}
