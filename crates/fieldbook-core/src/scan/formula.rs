//! Spreadsheet formula-trigger scan (`FB0403`, `docs/reference/corpus-format.md#scans`, `#registers`).
//!
//! **Responsibility.** Hold [`TRIGGERS`] in the order of the "Formula triggers" row
//! in `docs/reference/corpus-format.md#scans` and check whether a register cell is
//! a text cell that starts with a formula trigger character (`=`, `+`, `-`, `@`,
//! TAB or CR). A cell that parses as a decimal (`fieldbook_calc::Dec::parse`)
//! is not a text cell (for example, `-5` is not a formula).

use fieldbook_calc::Dec;

use super::Finding;

/// Formula trigger characters in the order of `docs/reference/corpus-format.md#scans`.
pub const TRIGGERS: &[char] = &['=', '+', '-', '@', '\t', '\r'];

/// Display names of [`TRIGGERS`] as written in `docs/reference/corpus-format.md#scans`.
pub const TRIGGER_NAMES: &[&str] = &["=", "+", "-", "@", "TAB", "CR"];

/// Check a single register cell for a leading spreadsheet formula trigger (`FB0403`).
///
/// Returns `None` if `cell` is empty, parses as a valid [`Dec`], or does not
/// start with a character in [`TRIGGERS`].
#[must_use]
pub fn scan_cell(cell: &str) -> Option<Finding> {
    let first = cell.chars().next()?;
    if !TRIGGERS.contains(&first) {
        return None;
    }
    if Dec::parse(cell).is_ok() {
        return None;
    }
    let len = first.len_utf8();
    Some(Finding::new(&cell[..len], 0..len))
}
