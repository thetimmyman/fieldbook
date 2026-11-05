//! JSON report shapes (`--json`, `AD-17`).

use serde::Serialize;

use crate::audit::chain::RawEventObject;
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

/// Journal head `{ "seq", "hash" }` in [`AuditShowReport`] and [`AuditVerifyReport`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditHead {
    /// Sequence number of the last line (`0` when the journal is empty).
    pub seq: u64,
    /// 64-hex hash of the last line (64 zeros when the journal is empty).
    pub hash: String,
}

/// The `--json` report produced by `fieldbook audit show`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditShowReport {
    /// Report schema version (`1`).
    pub report_version: u32,
    /// Always `"audit show"`.
    pub verb: &'static str,
    /// Sequence number and hash of the journal head.
    pub head: AuditHead,
    /// Total number of events in the journal.
    pub count: usize,
    /// Number of events in `events`.
    pub shown: usize,
    /// Selected journal events as objects preserving exact stored key order, oldest first.
    pub events: Vec<RawEventObject>,
    /// Process exit code (`0`).
    pub exit: u8,
}

/// One diagnostic finding in [`AuditVerifyReport`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditVerifyFinding {
    /// Four-digit `FB08xx` catalogue code.
    pub code: &'static str,
    /// Effective severity (`"error"` or `"warning"`).
    pub severity: &'static str,
    /// Sequence number of the offending line (`0` for whole-journal findings such as FB0802).
    pub seq: u64,
}

/// The `--json` report produced by `fieldbook audit verify`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditVerifyReport {
    /// Report schema version (`1`).
    pub report_version: u32,
    /// Always `"audit verify"`.
    pub verb: &'static str,
    /// Verification state (`ok`, `broken`, `torn`, or `unfinished`).
    pub state: &'static str,
    /// Number of events in the journal.
    pub events: usize,
    /// Sequence number and hash of the journal head.
    pub head: AuditHead,
    /// Findings in the order raised.
    pub findings: Vec<AuditVerifyFinding>,
    /// Sequence numbers of `write.begin` events with no matching `write.end`.
    pub unfinished: Vec<u64>,
    /// Process exit code (`0` or `1`).
    pub exit: u8,
}
