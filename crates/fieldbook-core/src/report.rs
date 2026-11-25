//! JSON report shapes (`--json`, `AD-17`).

use serde::{Deserialize, Serialize};

use crate::assist::ContextPack;
use crate::audit::chain::RawEventObject;
use crate::check::{Diagnostic, Report};
use crate::date::CivilDate;
use crate::labels::{LabelId, LabelSet, StoreId};
use crate::ports::AdapterStatus;
use crate::search::{DocEntry, Hit, Index, PageRow, Results, ShownRows, query_tokens};

/// Schema version of every `--json` report (`AD-17`).
pub const REPORT_VERSION: u32 = 1;

/// One report a desk page reads, with its absence and its read failure kept apart.
///
/// The binary maps a missing file to [`Self::Missing`], a successful read and parse to
/// [`Self::Ready`], and any read or parse failure to [`Self::Unavailable`] with a safe
/// diagnostic. A page renders each state differently; it never turns an unreadable source
/// into an empty success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportSource<T> {
    /// The source does not exist yet.
    Missing,
    /// The source was read and parsed.
    Ready(T),
    /// The source exists but could not be read or parsed; the text is a safe diagnostic.
    Unavailable {
        /// Why the source is unavailable, escaped by the page before it is shown.
        diagnostic: String,
    },
}

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

/// One ranked result in [`SearchReport`] (`docs/reference/search.md#the-command-line`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchResult {
    /// 1-based rank in the result list.
    pub rank: usize,
    /// Unit identifier as written.
    pub id: String,
    /// Unit kind (`page`, `claim`, `source`, ...).
    pub kind: String,
    /// Display title.
    pub title: String,
    /// Display parent (the page title of a section; empty otherwise).
    pub parent: String,
    /// Display status.
    pub status: String,
    /// `as_of` date as written (possibly partial; empty when none).
    pub as_of: String,
    /// `effective_from` date as written (empty when none).
    pub effective_from: String,
    /// `verified_on` date as written (empty when none).
    pub verified_on: String,
    /// Label ids of the unit (`["public"]` for a Corpus unit).
    pub labels: Vec<String>,
    /// Store the unit came from (`corpus` or `overlay`).
    pub store: &'static str,
    /// Number of query tokens the unit matched.
    pub matched: usize,
    /// Summed integer impact across the matched query tokens.
    pub score: u32,
    /// Display snippet (of the best unit, on a page row).
    pub snippet: String,
    /// On a page row, the id of the page's best-ranked unit; absent on every other row.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_unit: Option<String>,
    /// On a page row, how many of the page's units matched; absent on every other row.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_units: Option<usize>,
}

impl SearchResult {
    /// The row of one unit, as the unit list and the exact and other rows of the page view show it.
    fn of_unit(rank: usize, doc: &DocEntry, hit: &Hit) -> Self {
        Self {
            rank,
            id: doc.id.clone(),
            kind: doc.kind.clone(),
            title: doc.title.clone(),
            parent: doc.parent.clone(),
            status: doc.status.clone(),
            as_of: doc.as_of.clone(),
            effective_from: doc.effective_from.clone(),
            verified_on: doc.verified_on.clone(),
            labels: vec!["public".to_owned()],
            store: "corpus",
            matched: hit.matched,
            score: hit.score,
            snippet: doc.snippet.clone(),
            best_unit: None,
            matched_units: None,
        }
    }

    /// The row of one page: the lead's display values, the best unit's ranking and snippet.
    fn of_page(rank: usize, row: &PageRow<'_>) -> Self {
        Self {
            id: row.page.to_owned(),
            snippet: row.best_doc.snippet.clone(),
            matched: row.best.matched,
            score: row.best.score,
            best_unit: Some(row.best.id.clone()),
            matched_units: Some(row.matched_units),
            ..Self::of_unit(rank, row.display, row.best)
        }
    }
}

/// The `--json` report produced by `fieldbook search`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchReport {
    /// Report schema version (`1`).
    pub report_version: u32,
    /// The query text as typed (words joined with single spaces).
    pub query: String,
    /// The query tokens the index was asked for.
    pub tokens: Vec<String>,
    /// The list these results are: `pages` (the page view) or `units` (the flat unit list).
    pub view: &'static str,
    /// Total number of matching units before truncation.
    pub total: usize,
    /// The displayed results in rank order.
    pub results: Vec<SearchResult>,
}

impl SearchReport {
    /// Build a [`SearchReport`] for `raw_query` from the `results` that `query` returned over
    /// `index`. Every unit of a Corpus index is public and comes from the `corpus` store.
    #[must_use]
    pub fn from_results(index: &Index, raw_query: &str, results: &Results) -> Self {
        let rows = results
            .hits
            .iter()
            .enumerate()
            .filter_map(|(pos, hit)| {
                let doc = index.docs.get(hit.unit)?;
                Some(SearchResult::of_unit(pos + 1, doc, hit))
            })
            .collect();
        Self {
            report_version: REPORT_VERSION,
            query: raw_query.to_owned(),
            tokens: query_tokens(raw_query),
            view: "units",
            total: results.total,
            results: rows,
        }
    }

    /// Build a [`SearchReport`] for `raw_query` from the rows of the page view that a surface
    /// shows, so that the report and the plain output list the same rows. Ranks run across the
    /// exact, page and other rows.
    #[must_use]
    pub fn from_page_view(
        index: &Index,
        raw_query: &str,
        total: usize,
        shown: &ShownRows<'_, '_>,
    ) -> Self {
        let of_units = |hits: &[&Hit], first: usize| -> Vec<SearchResult> {
            hits.iter()
                .enumerate()
                .filter_map(|(pos, hit)| {
                    let doc = index.docs.get(hit.unit)?;
                    Some(SearchResult::of_unit(first + pos, doc, hit))
                })
                .collect()
        };
        let mut rows = of_units(shown.exact, 1);
        let first_page = rows.len() + 1;
        rows.extend(
            shown
                .pages
                .iter()
                .enumerate()
                .map(|(pos, row)| SearchResult::of_page(first_page + pos, row)),
        );
        let first_other = rows.len() + 1;
        rows.extend(of_units(shown.other, first_other));
        Self {
            report_version: REPORT_VERSION,
            query: raw_query.to_owned(),
            tokens: query_tokens(raw_query),
            view: "pages",
            total,
            results: rows,
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

/// The `--json` report produced by `fieldbook audit export` (`audit-export`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditExport {
    /// Report schema version (`1`).
    pub report_version: u32,
    /// Always `"audit export"`.
    pub verb: &'static str,
    /// `false` when the journal held nothing at or after `--since`.
    pub written: bool,
    /// Number of exported journal lines.
    pub events: usize,
    /// First exported sequence number.
    pub first_seq: u64,
    /// Last exported sequence number.
    pub last_seq: u64,
    /// Chain hash of the last exported line.
    pub head: String,
    /// File name of the exported journal segment.
    pub file: String,
    /// File name of the manifest.
    pub manifest: String,
    /// Process exit code (`0`).
    pub exit: u8,
}

/// The `--json` report produced by `fieldbook audit verify --export MANIFEST` (`audit-export`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditVerifyExport {
    /// Report schema version (`1`).
    pub report_version: u32,
    /// Always `"audit verify"`.
    pub verb: &'static str,
    /// File name of the verified manifest.
    pub manifest: String,
    /// Verification state (`ok` or `broken`).
    pub state: &'static str,
    /// Number of journal lines in the exported segment.
    pub events: usize,
    /// First sequence number in the exported segment.
    pub first_seq: u64,
    /// Last sequence number in the exported segment.
    pub last_seq: u64,
    /// Chain hash of the last exported line.
    pub head: String,
    /// Findings in the order raised.
    pub findings: Vec<AuditVerifyFinding>,
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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

/// Target configuration in [`BackupList`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupListTarget {
    /// Target identifier from `settings.toml`.
    pub id: String,
    /// Kind of target (`"folder"`, `"it-location"`, `"synced"`).
    pub kind: String,
    /// Destination path as configured.
    pub path: String,
    /// Whether target is marked company storage.
    pub company_storage: bool,
}

/// Latest verification outcome and timestamp in [`BackupListSnapshot`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupLatestVerification {
    /// Verification outcome (`"ok"` or `"failed"`).
    pub outcome: String,
    /// Event timestamp in ISO 8601 UTC.
    pub utc: String,
}

/// One snapshot entry in [`BackupList`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupListSnapshot {
    /// Snapshot number.
    pub snapshot: u32,
    /// Schema version (`1`).
    pub backup_version: u32,
    /// Tool version that created the snapshot.
    pub tool_version: String,
    /// Creation timestamp in ISO 8601 UTC.
    pub created_utc: String,
    /// Sorted list of label identifiers.
    pub labels: Vec<String>,
    /// Roots copied into the snapshot.
    pub roots: Vec<String>,
    /// Total number of data files.
    pub files: u64,
    /// Total bytes of data files.
    pub bytes: u64,
    /// SHA-256 fingerprint over corpus files.
    pub fingerprint: String,
    /// Sequence number of the snapshot's `backup.begin` journal event.
    pub journal_seq: u64,
    /// Hash of the snapshot's `backup.begin` journal event.
    pub journal_head: String,
    /// Journal state (`ok` or `broken`).
    pub journal_state: String,
    /// Manifest SHA-256 hash.
    pub manifest_sha256: String,
    /// Whether this snapshot has ever been successfully verified.
    pub verified: bool,
    /// Timestamp of newest successful verification (omitted when unverified).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified_utc: Option<String>,
    /// Outcome and timestamp of the latest verification (omitted when unverified).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_verification: Option<BackupLatestVerification>,
}

/// Restore drill outcome in [`BackupList`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupListDrill {
    /// Drill mode (`"drill"` or `"real"`).
    pub mode: String,
    /// Outcome (`"ok"` or failing code).
    pub outcome: String,
    /// Timestamp in ISO 8601 UTC.
    pub utc: String,
}

/// Diagnostic finding in [`BackupList`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupListFinding {
    /// Four-digit `FB08xx` catalogue code.
    pub code: String,
    /// Effective severity (`"error"` or `"warning"`).
    pub severity: String,
}

/// The `--json` report produced by `fieldbook backup list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupList {
    /// Report schema version (`1`).
    pub report_version: u32,
    /// Always `"backup list"`.
    pub verb: String,
    /// Target information (omitted when no target is configured).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<BackupListTarget>,
    /// Age in days after which unverified backups trigger a warning.
    pub warn_after_days: u32,
    /// Number of recent snapshots to retain.
    pub keep_last: usize,
    /// Number of weekly snapshots to retain.
    pub keep_weekly: usize,
    /// Snapshots present in the target (empty list when no target).
    pub snapshots: Vec<BackupListSnapshot>,
    /// Timestamp of newest successful verification across all snapshots (omitted when none).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified_utc: Option<String>,
    /// Outcome of newest restore drill (omitted when none).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drill: Option<BackupListDrill>,
    /// Findings raised by the listing (`FB0812`, `FB0813`, `FB0814`).
    pub findings: Vec<BackupListFinding>,
    /// Process exit code (`0` or `1`).
    pub exit: u8,
}

/// The persisted `reports/last-backup.json` verification cache (`backup-snapshot` step 21).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LastBackupReport {
    /// Verification mode (always `"snapshot"`).
    pub verification: String,
    /// Canonical path of the backup target.
    pub target_path: String,
    /// Effective classification labels of this cache record.
    pub labels: Vec<LabelId>,
    /// Lowercase SHA-256 digest of `target_path`.
    pub target_sha256: String,
    /// Verified snapshot number.
    pub snapshot: u32,
    /// Manifest SHA-256 digest of the verified snapshot.
    pub manifest_sha256: String,
    /// Verification timestamp in ISO 8601 UTC.
    pub verified_utc: String,
}

/// Recorded revision and 12-hex hash prefix in [`PolicyShow`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyShowRecorded {
    /// Highest recorded policy revision.
    pub revision: u64,
    /// First 12 lowercase hex digits of the recorded policy SHA-256 hash.
    pub hash: String,
}

/// One defined label entry in [`PolicyShow`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyShowLabel {
    /// Opaque label id.
    pub id: String,
    /// Human-readable label title.
    pub title: String,
    /// Optional banner text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
    /// Whether holding this label requires store `readers`.
    pub restricted: bool,
}

/// One store rule entry in [`PolicyShow`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyShowStore {
    /// Store identifier (`corpus`, `overlay`, `working`, `state`).
    pub id: String,
    /// Permitted label ids.
    pub holds: Vec<String>,
    /// Default label ids.
    pub default: Vec<String>,
    /// Optional OS reader group.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readers: Option<String>,
    /// Optional OS changer group.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changers: Option<String>,
    /// Whether the store is on hold.
    pub hold: bool,
}

/// One destination row in [`PolicyShow`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyShowDestination {
    /// Destination kind.
    pub kind: String,
    /// Endpoint name (`model` / `handoff`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Endpoint URL (`model`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Allowed label ids.
    pub allow: Vec<String>,
    /// Optional expiry date (`YYYY-MM-DD`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
    /// Whether `expires` has passed relative to today.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expired: Option<bool>,
    /// Maximum call retention days (`model`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_calls_days_max: Option<u32>,
    /// Change or approval reference (`ref`).
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    /// Approved MCP client hashes (`mcp`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clients: Option<Vec<String>>,
    /// Approved path prefixes (`export` / `backup`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paths: Option<Vec<String>>,
    /// True when this is a fixed built-in destination row (`audit`, `git`, `edition`).
    pub built_in: bool,
    /// True when this destination kind is advisory (`handoff`, `print`, `clipboard`).
    pub advisory: bool,
}

/// One label cut explanation entry in [`PolicyShowEffective`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyShowCut {
    /// Excluded label id.
    pub label: String,
    /// Side that cut the label (`"policy"` or `"owner"`).
    pub by: String,
}

/// Effective narrowed allow set and cut attribution for one destination row in [`PolicyShow`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyShowEffective {
    /// Destination kind.
    pub kind: String,
    /// Optional endpoint name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Effective allowed label ids after owner narrowing.
    pub allow: Vec<String>,
    /// Labels excluded from the effective allow set and which side excluded each.
    pub cuts: Vec<PolicyShowCut>,
}

/// The `--json` report produced by `fieldbook policy show` (`policy-load` step 7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyShow {
    /// Report schema version (`1`).
    pub report_version: u32,
    /// Always `"policy show"`.
    pub verb: String,
    /// Policy state (`"floor"`, `"loaded"`, or `"closed"`).
    pub state: String,
    /// Policy file path (`None` when no source is configured).
    pub path: Option<String>,
    /// 64-character lowercase hex SHA-256 policy or observation hash.
    pub hash: String,
    /// Writer check outcome (`"ok"` or failure reason).
    pub writer_check: String,
    /// Policy revision (`0` for `floor` and `closed`).
    pub revision: u64,
    /// Highest recorded revision and 12-hex hash prefix from `state/policy-seen.toml`.
    pub recorded: Option<PolicyShowRecorded>,
    /// Defined company labels.
    pub labels: Vec<PolicyShowLabel>,
    /// Store rules.
    pub stores: Vec<PolicyShowStore>,
    /// Policy and built-in destination rows.
    pub destinations: Vec<PolicyShowDestination>,
    /// Effective destination allow sets and cut explanations.
    pub effective: Vec<PolicyShowEffective>,
    /// Process exit code (`0` for `floor` and `loaded`, `1` for `closed`).
    pub exit: u8,
}

/// The `--json` report produced by `fieldbook ai status` (`ai-config-and-keys` step 6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiStatus {
    /// Report schema version (`1`).
    pub report_version: u32,
    /// Policy state (`"floor"`, `"loaded"`, or `"closed"`).
    pub policy: String,
    /// Status entries for model and other port adapters.
    pub adapters: Vec<AdapterStatus>,
    /// Process exit code (`0`).
    pub exit: u8,
}

/// The `--json` report produced by `fieldbook context` (`handoff-context` step 7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContextReport {
    /// Report schema version (`1`).
    pub report_version: u32,
    /// Always `"context"`.
    pub verb: &'static str,
    /// Built context pack with items, request budget, labels, and view fingerprint.
    pub pack: ContextPack,
    /// Exact plain-text rendering of `pack` (`ContextPack::to_text`).
    pub pack_text: String,
}

impl ContextReport {
    /// Builds a [`ContextReport`] from `pack`.
    #[must_use]
    pub fn from_pack(pack: ContextPack) -> Self {
        let pack_text = pack.to_text();
        Self {
            report_version: REPORT_VERSION,
            verb: "context",
            pack,
            pack_text,
        }
    }
}
