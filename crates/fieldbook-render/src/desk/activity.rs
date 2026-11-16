//! The Activity log desk page: the audit journal's newest events and its verification result.

use std::fmt::Write as _;

use fieldbook_core::audit::chain::{RawEventObject, RawJsonValue};
use fieldbook_core::report::{AuditShowReport, AuditVerifyReport, ReportSource};

use crate::desk::{PAGE_EVENTS, chip_row, source_line, source_state_line};
use crate::links::escape_html;

/// The page title and desk-header link text.
pub const TITLE: &str = "Activity log";

/// The page path relative to the state folder.
pub const PATH: &str = "pages/activity.html";

/// The source of the event rows.
pub const SOURCE: &str = "audit/journal.jsonl";

fn text_field<'a>(event: &'a RawEventObject, key: &str) -> Option<&'a str> {
    event.get(key).and_then(RawJsonValue::as_str)
}

fn number_field(event: &RawEventObject, key: &str) -> Option<u64> {
    match event.get(key) {
        Some(RawJsonValue::Uint(n)) => Some(*n),
        _ => None,
    }
}

fn list_field<'a>(event: &'a RawEventObject, key: &str) -> Vec<&'a str> {
    match event.get(key) {
        Some(RawJsonValue::Array(items)) => items.iter().filter_map(RawJsonValue::as_str).collect(),
        Some(RawJsonValue::Str(one)) => vec![one.as_str()],
        _ => Vec::new(),
    }
}

/// Renders the page body for its two independent sources.
#[must_use]
pub fn render(
    show: &ReportSource<AuditShowReport>,
    verify: &ReportSource<AuditVerifyReport>,
) -> String {
    let mut out = source_line(SOURCE);
    out.push_str(&chip_row(&[
        "Show recent activity",
        "Verify the audit journal",
    ]));
    out.push('\n');
    out.push_str(&verification(verify));
    out.push_str(&events(show, verify));
    out
}

fn verification(source: &ReportSource<AuditVerifyReport>) -> String {
    match source {
        ReportSource::Unavailable { .. } => {
            let mut out = source_state_line(source, "", "Activity unavailable");
            out.push_str("<p>Read failed. No successful verification is inferred.</p>\n");
            out
        }
        ReportSource::Missing => source_state_line(source, "None recorded", ""),
        ReportSource::Ready(report) => {
            let ok = report.state == "ok";
            let class = if ok { "ok" } else { "warn" };
            let mut out = format!(
                "<section class=\"panel\"><div class=\"panel-head\"><h2 class=\"panel-title\">Verification</h2></div><div class=\"panel-b\">\n<p><span class=\"chip {class}\">{}</span> {} events, head {} {}</p>\n",
                escape_html(report.state),
                report.events,
                report.head.seq,
                escape_html(&report.head.hash)
            );
            if !ok {
                out.push_str("<p>The journal did not verify. The events below are readable and remain unverified.</p>\n");
                out.push_str("<ul>\n");
                for finding in &report.findings {
                    let _ = writeln!(
                        out,
                        "<li>{}[{}] at line {}</li>",
                        escape_html(finding.severity),
                        escape_html(finding.code),
                        finding.seq
                    );
                }
                out.push_str("</ul>\n");
            }
            out.push_str("</div></section>\n");
            out
        }
    }
}

fn events(
    show: &ReportSource<AuditShowReport>,
    verify: &ReportSource<AuditVerifyReport>,
) -> String {
    match show {
        ReportSource::Missing => source_state_line(show, "None recorded", ""),
        ReportSource::Unavailable { .. } => {
            let mut out = source_state_line(show, "", "Activity unavailable");
            out.push_str("<p>Read failed. No events are shown.</p>\n");
            out
        }
        ReportSource::Ready(report) => {
            let mut out = String::new();
            let verified = matches!(verify, ReportSource::Ready(v) if v.state == "ok");
            let _ = writeln!(
                out,
                "<p class=\"meta-row\"><span>Showing {} of {} recorded events</span></p>",
                report.shown, report.count
            );
            if !verified {
                out.push_str("<p class=\"unver\">These events are unverified.</p>\n");
            }
            out.push_str("<div class=\"table-wrap\"><table>\n<caption>Newest recorded events</caption>\n<thead><tr>");
            for head in [
                "Sequence",
                "UTC time",
                "Role",
                "Kind",
                "Affected ids",
                "Outcome",
            ] {
                let _ = write!(out, "<th scope=\"col\">{head}</th>");
            }
            out.push_str("</tr></thead>\n<tbody>\n");
            for event in report.events.iter().rev().take(PAGE_EVENTS) {
                let seq = number_field(event, "seq").map_or_else(String::new, |n| n.to_string());
                let time = text_field(event, "time").unwrap_or("");
                let role = text_field(event, "role").unwrap_or("");
                let kind = text_field(event, "kind").unwrap_or("");
                let affected = list_field(event, "affected").join(", ");
                let outcome = text_field(event, "outcome").unwrap_or("");
                let _ = writeln!(
                    out,
                    "<tr><td>{}</td><td class=\"mono\">{}</td><td>{}</td><td>{}</td><td class=\"mono\">{}</td><td>{}</td></tr>",
                    escape_html(&seq),
                    escape_html(time),
                    escape_html(role),
                    escape_html(kind),
                    escape_html(&affected),
                    escape_html(outcome)
                );
            }
            out.push_str("</tbody></table></div>\n");
            out
        }
    }
}
