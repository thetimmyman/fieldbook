//! Text and JSON report formatting and persistence for `fieldbook add` (`add-folder-mode`, AD-17).

use std::fmt::Write as _;

use fieldbook_core::labels::{self, LabelSet, MarkingRules, StoreId, UNLABELLED};
use fieldbook_core::policy::Policy;
use fieldbook_core::report::{
    AddCounts, AddFileReport, AddFileResult, AddInputRef, AddReport, REPORT_VERSION,
};

use crate::World;
use crate::io::{Journal, StoreLease};

/// Observed per-file classification input for [`classify_add_report`].
#[derive(Debug, Clone)]
pub struct FileReportEntry {
    /// Per-file outcome entry for [`AddReport::files`].
    pub file: AddFileReport,
    /// Observed [`LabelSet`] of the input file if classification completed, or `None` when the
    /// file was refused before classification (`FB0821` or `FB0822`).
    pub observed_labels: Option<LabelSet>,
}

/// Computes the effective [`LabelSet`] of an [`AddReport`] (`add-folder-mode` step 9):
/// - Starts from [`LabelSet::owner_supplied`] because the report carries owner-supplied filenames
///   and folder names.
/// - Unions any markings found in `dir` and in each file's `name` via [`labels::markings_in`].
/// - Unions `policy.store_rule(&StoreId::State).default` when a loaded policy is active
///   (`policy.revision() > 0`).
/// - Unions every input's observed [`LabelSet`] (including refused inputs), or
///   [`LabelSet::unlabelled`] when any input was refused before classification completed.
#[must_use]
pub fn classify_add_report(
    dir: &str,
    entries: &[FileReportEntry],
    policy: &Policy,
    rules: &MarkingRules,
) -> LabelSet {
    let mut labels = LabelSet::owner_supplied();
    if let Some(found) = labels::markings_in(dir, rules) {
        labels = labels.union(&found);
    }
    if policy.revision() > 0 {
        let def = &policy.store_rule(&StoreId::State).default;
        if !def.contains(&UNLABELLED) {
            labels = labels.union(def);
        }
    }
    for entry in entries {
        if let Some(found) = labels::markings_in(&entry.file.name, rules) {
            labels = labels.union(&found);
        }
        match &entry.observed_labels {
            Some(file_labels) => {
                labels = labels.union(file_labels);
            }
            None => {
                labels = labels.union(&LabelSet::unlabelled());
            }
        }
    }
    labels
}

/// Assembles an [`AddReport`] from `dir`, `entries`, `store`, `policy`, and `exit`.
#[must_use]
pub fn build_add_report(
    dir: String,
    entries: &[FileReportEntry],
    store: StoreId,
    policy: &Policy,
    exit: u8,
) -> AddReport {
    let rules = policy.marking_rules();
    let labels = classify_add_report(&dir, entries, policy, &rules);

    let mut inputs: Vec<AddInputRef> = entries
        .iter()
        .filter(|e| matches!(e.file.result, AddFileResult::Added | AddFileResult::Present))
        .filter_map(|e| {
            e.file.id.as_ref().map(|id| AddInputRef {
                store,
                id: id.clone(),
            })
        })
        .collect();
    inputs.sort();
    inputs.dedup();

    let mut counts = AddCounts::default();
    let mut files = Vec::with_capacity(entries.len());
    for entry in entries {
        match entry.file.result {
            AddFileResult::Added => counts.added += 1,
            AddFileResult::Present => counts.present += 1,
            AddFileResult::Refused => counts.refused += 1,
        }
        if entry.file.needs_extraction {
            counts.needs_extraction += 1;
        }
        files.push(entry.file.clone());
    }

    AddReport {
        report_version: REPORT_VERSION,
        verb: "add".to_owned(),
        dir,
        labels,
        inputs,
        files,
        counts,
        exit,
    }
}

/// Formats an [`AddReport`] in folder-mode text form (`add-folder-mode` step 4).
#[must_use]
pub fn render_add_text(report: &AddReport) -> String {
    let mut out = String::new();
    for file in &report.files {
        let mid = match file.result {
            AddFileResult::Added | AddFileResult::Present => file.id.as_deref().unwrap_or(""),
            AddFileResult::Refused => file.code.as_deref().unwrap_or(""),
        };
        let _ = writeln!(out, "{:<10}{mid}  {}", file.result.as_str(), file.name);
    }

    let mut parts = Vec::new();
    if report.counts.added > 0 {
        parts.push(format!("{} added", report.counts.added));
    }
    if report.counts.present > 0 {
        parts.push(format!("{} present", report.counts.present));
    }
    if report.counts.refused > 0 {
        parts.push(format!("{} refused", report.counts.refused));
    }
    if parts.is_empty() {
        out.push_str("done: 0 added\n");
    } else {
        let _ = writeln!(out, "done: {}", parts.join(", "));
    }
    out
}

/// Formats an [`AddReport`] as pretty-printed JSON with a trailing newline (`AD-17`).
#[must_use]
pub fn render_add_json(report: &AddReport) -> String {
    let mut json = serde_json::to_string_pretty(report).unwrap_or_default();
    json.push('\n');
    json
}

/// Parses and validates a saved `reports/last-add.json` payload (`add-folder-mode` step 12).
///
/// # Errors
///
/// Returns an error message when `json` is malformed or violates the saved-report classification
/// contract (missing or empty `labels`, missing `inputs`, or an `added`/`present` file absent from
/// `inputs`).
pub fn parse_saved_add_report(json: &str) -> Result<AddReport, String> {
    serde_json::from_str(json).map_err(|err| err.to_string())
}

/// Best-effort persistence of `report` to `<state>/reports/last-add.json` under `lease`
/// (`add-folder-mode` step 9).
///
/// Checks `labels::admit` against `policy.store_rule(StoreId::State)` and `labels::git_guard`
/// against the state directory's git context before writing through `journal.state_scope(lease)`.
/// On any failure, leaves any existing `reports/last-add.json` untouched and returns `false` so
/// the caller can append `"report not saved\n"` to `stderr` without changing the command exit code.
pub fn save_last_add_report(
    world: &dyn World,
    journal: &Journal<'_>,
    lease: &StoreLease,
    policy: &Policy,
    report: &AddReport,
) -> bool {
    let state_rule = policy.store_rule(&StoreId::State);
    if labels::admit(&report.labels, &state_rule).is_err() {
        return false;
    }
    let report_path = world.state_dir().join("reports").join("last-add.json");
    let Ok(git_ctx) = crate::io::root::git_context(world, &report_path) else {
        return false;
    };
    if labels::git_guard(&report.labels, git_ctx).is_err() {
        return false;
    }
    let Ok(state_audited) = journal.state_scope(lease) else {
        return false;
    };
    let json = render_add_json(report);
    world
        .write_file(&state_audited, &report_path, json.as_bytes())
        .is_ok()
}
