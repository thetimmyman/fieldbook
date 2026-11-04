//! JSON report shapes (`--json`, `AD-17`).

use serde::Serialize;

use crate::check::{Diagnostic, Report};
use crate::date::CivilDate;

/// Schema version of every `--json` report (`AD-17`).
pub const REPORT_VERSION: u32 = 1;

/// One diagnostic entry in [`CheckReport`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckDiagnostic {
    /// Four-digit `FBxxxx` catalogue code.
    pub code: &'static str,
    /// Effective severity (`"error"` or `"warning"`).
    pub severity: &'static str,
    /// Repository-relative `/`-separated path.
    pub path: String,
    /// 1-based line number.
    pub line: usize,
    /// 1-based code-point column number.
    pub col: usize,
    /// Finding message.
    pub message: String,
    /// Catalogue remediation help text.
    pub help: &'static str,
}

impl From<&Diagnostic> for CheckDiagnostic {
    fn from(diag: &Diagnostic) -> Self {
        Self {
            code: diag.code.id(),
            severity: diag.severity.as_str(),
            path: diag.path.clone(),
            line: diag.line,
            col: diag.col,
            message: diag.message.clone(),
            help: diag.help,
        }
    }
}

/// The `--json` report produced by `fieldbook check`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckReport {
    /// Report schema version (`1`).
    pub report_version: u32,
    /// Whether `--strict` was enabled.
    pub strict: bool,
    /// Calendar date (`YYYY-MM-DD`) used by the check run.
    pub today: String,
    /// Number of error diagnostics after path filtering.
    pub errors: usize,
    /// Number of warning diagnostics after path filtering.
    pub warnings: usize,
    /// Informational note lines emitted by check passes.
    pub notes: Vec<String>,
    /// Filtered diagnostics in sorted `(path, line, col, code)` order.
    pub diagnostics: Vec<CheckDiagnostic>,
}

impl CheckReport {
    /// Build a [`CheckReport`] from a filtered [`Report`], `strict` flag and `today` date.
    #[must_use]
    pub fn from_report(report: &Report, strict: bool, today: CivilDate) -> Self {
        Self {
            report_version: REPORT_VERSION,
            strict,
            today: today.to_string(),
            errors: report.errors(),
            warnings: report.warnings(),
            notes: report.notes.clone(),
            diagnostics: report
                .diagnostics
                .iter()
                .map(CheckDiagnostic::from)
                .collect(),
        }
    }
}
