//! The Backup status desk page: the backup list report, verification first, then snapshots.

use std::fmt::Write as _;

use fieldbook_core::report::{
    BackupLatestVerification, BackupList, BackupListSnapshot, ReportSource,
};

use crate::desk::{chip_row, source_line, source_state_line};
use crate::links::escape_html;

/// The page title and desk-header link text.
pub const TITLE: &str = "Backup status";

/// The page path relative to the state folder.
pub const PATH: &str = "pages/backup.html";

/// The source of the page.
pub const SOURCE: &str = "settings.toml [[backup.target]] · reports/last-backup.json";

/// Renders the page body for `source`.
#[must_use]
pub fn render(source: &ReportSource<BackupList>) -> String {
    let mut out = source_line(SOURCE);
    out.push_str(&chip_row(&[
        "Back up the working data",
        "List backups",
        "Check a backup",
    ]));
    out.push('\n');
    match source {
        ReportSource::Missing => {
            out.push_str(&source_state_line(source, "No backups recorded", ""));
        }
        ReportSource::Unavailable { .. } => {
            out.push_str(&source_state_line(source, "", "Backup status unavailable"));
            out.push_str("<p>Read failed. No backup is reported as present or verified.</p>\n");
        }
        ReportSource::Ready(list) => {
            if list.target.is_none() && list.snapshots.is_empty() {
                out.push_str("<p class=\"empty\">No backups recorded</p>\n");
                return out;
            }
            out.push_str(&verification(list));
            out.push_str(&snapshots(list));
        }
    }
    out
}

fn latest_recorded(list: &BackupList) -> Option<&BackupLatestVerification> {
    list.snapshots
        .iter()
        .filter_map(|snapshot| snapshot.latest_verification.as_ref())
        .max_by(|a, b| a.utc.cmp(&b.utc))
}

fn verification(list: &BackupList) -> String {
    let mut out = String::from(
        "<section class=\"panel\"><div class=\"panel-head\"><h2 class=\"panel-title\">Verification</h2></div><div class=\"panel-b\">\n",
    );
    let target = list.target.as_ref().map_or_else(
        || "No target configured".to_owned(),
        |target| format!("{} ({})", target.id, target.kind),
    );
    let _ = writeln!(out, "<p>Target: {}</p>", escape_html(&target));
    match latest_recorded(list) {
        Some(latest) => {
            let class = if latest.outcome == "ok" { "ok" } else { "bad" };
            let _ = writeln!(
                out,
                "<p>Latest recorded verification: <span class=\"chip {class}\">{}</span> {}</p>",
                escape_html(&latest.outcome),
                escape_html(&latest.utc)
            );
        }
        None => out.push_str("<p>Latest recorded verification: none recorded</p>\n"),
    }
    match &list.verified_utc {
        Some(utc) => {
            let _ = writeln!(
                out,
                "<p>Last successful verification: {}</p>",
                escape_html(utc)
            );
        }
        None => out.push_str("<p>Last successful verification: none yet</p>\n"),
    }
    if !list.findings.is_empty() {
        out.push_str("<ul>\n");
        for finding in &list.findings {
            let _ = writeln!(
                out,
                "<li><span class=\"chip warn\">{}</span> {}</li>",
                escape_html(&finding.severity),
                escape_html(&finding.code)
            );
        }
        out.push_str("</ul>\n");
    }
    out.push_str("</div></section>\n");
    out
}

fn snapshot_result(snapshot: &BackupListSnapshot) -> &'static str {
    match &snapshot.latest_verification {
        Some(latest) if latest.outcome != "ok" => "failed",
        Some(_) | None if snapshot.verified => "checked",
        Some(_) => "recorded",
        None => "not checked",
    }
}

fn snapshots(list: &BackupList) -> String {
    if list.snapshots.is_empty() {
        return "<p class=\"empty\">No backups recorded</p>\n".to_owned();
    }
    let target_kind = list
        .target
        .as_ref()
        .map_or("Unavailable", |target| target.kind.as_str());
    let mut out = String::from(
        "<div class=\"table-wrap\"><table>\n<caption>Snapshots in the target</caption>\n<thead><tr>",
    );
    for head in [
        "Snapshot",
        "Recorded (UTC)",
        "Target kind",
        "Verification",
        "Files",
    ] {
        let _ = write!(out, "<th scope=\"col\">{head}</th>");
    }
    out.push_str("</tr></thead>\n<tbody>\n");
    for snapshot in &list.snapshots {
        let _ = writeln!(
            out,
            "<tr><td>{:04}</td><td class=\"mono\">{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            snapshot.snapshot,
            escape_html(&snapshot.created_utc),
            escape_html(target_kind),
            escape_html(snapshot_result(snapshot)),
            snapshot.files
        );
    }
    out.push_str("</tbody></table></div>\n");
    out
}
