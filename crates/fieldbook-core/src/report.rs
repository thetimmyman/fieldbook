//! JSON report shapes (`--json`, `AD-17`).

use serde::{Deserialize, Serialize};

use crate::audit::chain::RawEventObject;
use crate::check::{Diagnostic, Report};
use crate::date::CivilDate;
use crate::labels::{LabelId, LabelSet, StoreId};

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

/// Store-qualified input reference `{ "store", "id" }` in [`AddReport`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct AddInputRef {
    /// Store holding the added or present source (`"corpus"` or `"overlay"`).
    pub store: StoreId,
    /// Allocated or existing version identifier (`S-nnnn-vN` or `SO-nnnnnn-vN`).
    pub id: String,
}

/// Per-file outcome in [`AddFileReport`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AddFileResult {
    /// Added as a new source or version.
    Added,
    /// Content hash was already present in `sources.csv`.
    Present,
    /// Refused by type check, label context, store admission, or git guard.
    Refused,
}

impl AddFileResult {
    /// Returns the lowercase word (`"added"`, `"present"`, or `"refused"`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Present => "present",
            Self::Refused => "refused",
        }
    }
}

/// One per-file entry in [`AddReport`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddFileReport {
    /// Relative `/`-separated file name (or basename in single-file mode).
    pub name: String,
    /// Per-file outcome (`added`, `present`, or `refused`).
    pub result: AddFileResult,
    /// Allocated or matched version ID when `result` is `added` or `present`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 64-character lowercase hexadecimal SHA-256 digest of the input bytes.
    pub sha256: String,
    /// Byte length of the input file.
    pub bytes: u64,
    /// `true` when `result` is `added` or `present` and the file type needs text extraction.
    pub needs_extraction: bool,
    /// Four-digit `FBxxxx` refusal code when `result` is `refused`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// Aggregate file counts in [`AddReport`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AddCounts {
    /// Number of files with `result == added`.
    pub added: usize,
    /// Number of files with `result == present`.
    pub present: usize,
    /// Number of files with `result == refused`.
    pub refused: usize,
    /// Number of `added` or `present` files with `needs_extraction == true`.
    pub needs_extraction: usize,
}

/// The `--json` and `reports/last-add.json` report produced by `fieldbook add` (`add-folder-mode`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawAddReport")]
pub struct AddReport {
    /// Report schema version (`1`).
    pub report_version: u32,
    /// Always `"add"`.
    pub verb: String,
    /// Folder argument as passed on the command line (normalized to `/`), or `""` in single-file mode.
    pub dir: String,
    /// Effective classification of this report (never empty).
    pub labels: LabelSet,
    /// Sorted, deduplicated `{ "store", "id" }` pairs for every `added` or `present` entry in `files`.
    pub inputs: Vec<AddInputRef>,
    /// Per-file outcomes in byte order of relative path.
    pub files: Vec<AddFileReport>,
    /// Summary counts matching `files`.
    pub counts: AddCounts,
    /// Process exit code (`0`, `1`, or `3` on partial I/O failure).
    pub exit: u8,
}

#[derive(Deserialize)]
struct RawAddReport {
    report_version: u32,
    verb: String,
    dir: String,
    labels: Vec<LabelId>,
    inputs: Vec<AddInputRef>,
    files: Vec<AddFileReport>,
    counts: AddCounts,
    exit: u8,
}

impl TryFrom<RawAddReport> for AddReport {
    type Error = String;

    fn try_from(raw: RawAddReport) -> Result<Self, Self::Error> {
        if raw.report_version != REPORT_VERSION {
            return Err(format!(
                "unsupported AddReport report_version `{}`",
                raw.report_version
            ));
        }
        if raw.verb != "add" {
            return Err(format!("invalid AddReport verb `{}`", raw.verb));
        }
        if raw.labels.is_empty() {
            return Err("saved AddReport has an empty `labels` array".to_owned());
        }
        for file in &raw.files {
            if matches!(file.result, AddFileResult::Added | AddFileResult::Present) {
                let Some(ref file_id) = file.id else {
                    return Err(format!(
                        "file `{}` is `{}` but has no `id`",
                        file.name,
                        file.result.as_str()
                    ));
                };
                if !raw.inputs.iter().any(|input| &input.id == file_id) {
                    return Err(format!(
                        "file `{}` id `{file_id}` is missing from `inputs`",
                        file.name
                    ));
                }
            }
        }
        let labels = LabelSet::from_ids(raw.labels);
        Ok(Self {
            report_version: raw.report_version,
            verb: raw.verb,
            dir: raw.dir,
            labels,
            inputs: raw.inputs,
            files: raw.files,
            counts: raw.counts,
            exit: raw.exit,
        })
    }
}
