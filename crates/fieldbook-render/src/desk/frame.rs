//! The compact desk frame shared by the Import report, Activity log and Backup status pages.

use fieldbook_core::strings;

use crate::chip;
use crate::csp::CSP_STATIC;
use crate::desk::{PAGES, activity, backup, import};
use crate::links::escape_html;

/// The page title shared by the desk header.
pub const DESK_TITLE: &str = "Desk";

/// Renders one desk page: the header links, the title, the written time, the refresh chip and
/// `body`.
///
/// `written_utc` is the UTC time captured once by the binary handler from `World::now()`; the
/// page states it and never implies a fresher observation than it is.
#[must_use]
pub fn frame(title: &str, written_utc: &str, body: &str) -> String {
    let escaped_title = escape_html(title);
    let links = PAGES
        .iter()
        .map(|page| {
            let current = if page.name == title {
                " aria-current=\"page\""
            } else {
                ""
            };
            format!(
                "<a href=\"{}\"{}>{}</a>",
                escape_html(page_file(page.path)),
                current,
                escape_html(page.name)
            )
        })
        .collect::<Vec<_>>()
        .join(" · ");
    let refresh = chip::quote("Write the desk pages", &[])
        .map_or_else(|_| String::new(), |c| chip::render(&c));
    format!(
        concat!(
            "<!DOCTYPE html>\n",
            "<html lang=\"en\">\n",
            "<head>\n",
            "<meta charset=\"utf-8\">\n",
            "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n",
            "<meta http-equiv=\"Content-Security-Policy\" content=\"{csp}\">\n",
            "<title>{title} · {product}</title>\n",
            "<link rel=\"stylesheet\" href=\"{css}\">\n",
            "<script src=\"{js}\" defer></script>\n",
            "</head>\n",
            "<body>\n",
            "<a class=\"skip\" href=\"#fb:main\">Skip to content</a>\n",
            "<div class=\"app\"><div class=\"shell\">\n",
            "<header class=\"topbar\"><nav class=\"sections\" aria-label=\"{desk}\">{links}</nav></header>\n",
            "<main id=\"fb:main\" class=\"page\">\n",
            "<div class=\"page-head\"><h1>{title}</h1>",
            "<p class=\"meta-row\"><span>Written {written}</span><span class=\"sep\">·</span>",
            "<span>Refresh copies the command; it does not run it.</span></p>",
            "<div class=\"cmd-row\">{refresh}</div></div>\n",
            "{body}\n",
            "</main>\n",
            "</div></div>\n",
            "<div class=\"toast\" id=\"fb:toast\" role=\"status\" aria-live=\"polite\" hidden></div>\n",
            "<div class=\"sr-only\" id=\"fb:sr\" aria-live=\"polite\"></div>\n",
            "</body>\n",
            "</html>\n",
        ),
        csp = CSP_STATIC,
        title = escaped_title,
        product = strings::binary(),
        css = "assets/app.css",
        js = "assets/app.js",
        desk = DESK_TITLE,
        links = links,
        written = escape_html(written_utc),
        refresh = refresh,
        body = body,
    )
}

/// Returns the file name a desk-header link uses from another page in `pages/`.
fn page_file(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Returns the header link of the page a reader is on, for [`PAGES`]-driven tests.
#[must_use]
pub fn route_name(path: &str) -> Option<&'static str> {
    PAGES.iter().find(|p| p.path == path).map(|p| p.name)
}

/// The three page titles, so a caller can name a page without importing each module.
#[must_use]
pub const fn titles() -> [&'static str; 3] {
    [import::TITLE, activity::TITLE, backup::TITLE]
}
