//! The Import report desk page: `reports/last-add.json`, as `fieldbook add` wrote it.

use std::fmt::Write as _;

use fieldbook_core::report::{AddFileResult, AddReport, ReportSource};

use crate::desk::{chip_row, source_line, source_state_line};
use crate::links::escape_html;

/// The page title and desk-header link text.
pub const TITLE: &str = "Import report";

/// The page path relative to the state folder.
pub const PATH: &str = "pages/import.html";

/// The report file the page reads.
pub const SOURCE: &str = "reports/last-add.json";

/// Renders the page body for `source`.
#[must_use]
pub fn render(source: &ReportSource<AddReport>) -> String {
    let mut out = source_line(SOURCE);
    out.push_str(&chip_row(&["Add a source file"]));
    out.push('\n');
    match source {
        ReportSource::Missing => {
            out.push_str(&source_state_line(source, "No add has run", ""));
            out.push_str(
                "<p>Refresh after running <code>fieldbook add</code> to record the next report.</p>\n",
            );
        }
        ReportSource::Unavailable { .. } => {
            out.push_str(&source_state_line(source, "", "Import report unavailable"));
            out.push_str("<p>No counts are inferred from an unreadable report.</p>\n");
        }
        ReportSource::Ready(report) => out.push_str(&ready(report)),
    }
    out
}

fn ready(report: &AddReport) -> String {
    let mut out = String::new();
    out.push_str("<section class=\"panel\"><div class=\"panel-head\"><h2 class=\"panel-title\">Summary</h2></div><div class=\"panel-b\">\n");
    let _ = writeln!(
        out,
        "<p>{} added · {} present · {} refused · {} need extraction</p>",
        report.counts.added,
        report.counts.present,
        report.counts.refused,
        report.counts.needs_extraction
    );
    let _ = writeln!(
        out,
        "<p class=\"meta-row\"><span class=\"mono\">Folder: {}</span></p>",
        escape_html(if report.dir.is_empty() {
            "(single file)"
        } else {
            &report.dir
        })
    );
    out.push_str("</div></section>\n");

    if report.files.is_empty() {
        out.push_str("<p class=\"empty\">No files in this report</p>\n");
        return out;
    }

    out.push_str(
        "<div class=\"table-wrap\"><table>\n<caption>Files in this report</caption>\n<thead><tr>",
    );
    for head in ["Path", "Result", "ID", "Bytes", "Diagnostic"] {
        let _ = write!(out, "<th scope=\"col\">{head}</th>");
    }
    out.push_str("</tr></thead>\n<tbody>\n");
    for file in &report.files {
        let (result, diagnostic) = match file.result {
            AddFileResult::Added => ("added", String::new()),
            AddFileResult::Present => ("present", String::new()),
            AddFileResult::Refused => ("refused", file.code.clone().unwrap_or_default()),
        };
        let _ = writeln!(
            out,
            "<tr><td class=\"mono\">{}</td><td>{}</td><td class=\"mono\">{}</td><td>{}</td><td>{}</td></tr>",
            escape_html(&file.name),
            escape_html(result),
            escape_html(file.id.as_deref().unwrap_or("")),
            file.bytes,
            escape_html(&diagnostic)
        );
    }
    out.push_str("</tbody></table></div>\n");
    out
}
