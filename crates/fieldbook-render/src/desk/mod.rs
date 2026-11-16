//! The desk pages `fieldbook status --page` writes into `pages/` of the state folder
//! (`AD-17`, `status-command`).
//!
//! **Responsibility.** Render three static pages from the reports of `fieldbook-core`:
//! the Import report, the Activity log and Backup status. Every page is a snapshot of a
//! report, never a live dashboard: it names its source, the UTC time it was written and the
//! refresh command, and it runs nothing.
//!
//! **Rules.** No I/O, no clock, no journal read, and no invented result. A page takes
//! [`ReportSource`] values and an explicit written time. `PAGES` is the one route table.

pub mod activity;
pub mod backup;
pub mod frame;
pub mod import;

use fieldbook_core::report::ReportSource;

/// The number of newest journal events the Activity page shows.
pub const PAGE_EVENTS: usize = 200;

/// One row of [`PAGES`]: the one table of desk page names, paths and renderers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeskPage {
    /// The page name, which is also its heading and its desk-header link text.
    pub name: &'static str,
    /// The page path relative to the state folder.
    pub path: &'static str,
}

/// The desk pages, in the order the desk header lists them.
pub const PAGES: [DeskPage; 3] = [
    DeskPage {
        name: import::TITLE,
        path: import::PATH,
    },
    DeskPage {
        name: activity::TITLE,
        path: activity::PATH,
    },
    DeskPage {
        name: backup::TITLE,
        path: backup::PATH,
    },
];

/// The static assets a desk page needs, at their path relative to the state folder.
///
/// They sit beside the pages in `pages/assets/`, and the bytes are the render crate's embedded
/// copy of the site's stylesheet, so the pages use the same tokens and frame as an edition
/// without a second copy in the repository.
#[must_use]
pub fn assets() -> &'static [(&'static str, &'static str)] {
    &[
        ("pages/assets/app.css", crate::site::APP_CSS),
        ("pages/assets/app.js", crate::site::APP_JS),
    ]
}

/// Returns the empty state of a source the page cannot show, as its own paragraph.
#[must_use]
pub fn source_state_line<T>(source: &ReportSource<T>, missing: &str, unavailable: &str) -> String {
    match source {
        ReportSource::Missing => format!(
            "<p class=\"empty\">{}</p>",
            crate::links::escape_html(missing)
        ),
        ReportSource::Ready(_) => String::new(),
        ReportSource::Unavailable { diagnostic } => format!(
            "<p class=\"empty\">{}</p><p class=\"mono\">{}</p>",
            crate::links::escape_html(unavailable),
            crate::links::escape_html(diagnostic)
        ),
    }
}

/// Renders the command chips of the given verb-map intents, in order.
///
/// A chip is one row of `strings::chips` and nothing else; an intent with no row renders
/// nothing, so a page never shows a command no verb answers.
#[must_use]
pub fn chip_row(intents: &[&str]) -> String {
    let chips = intents
        .iter()
        .filter_map(|intent| crate::chip::quote(intent, &[]).ok())
        .map(|command| crate::chip::render(&command))
        .collect::<Vec<_>>()
        .join(" ");
    if chips.is_empty() {
        String::new()
    } else {
        format!("<div class=\"cmd-row\">{chips}</div>")
    }
}

/// Renders the source line a desk page shows above its first table.
#[must_use]
pub fn source_line(source: &str) -> String {
    format!(
        "<p class=\"meta-row\"><span class=\"mono\">Source: {}</span></p>\n",
        crate::links::escape_html(source)
    )
}
