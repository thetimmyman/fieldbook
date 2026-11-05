//! Internal link and chip target resolution against [`PlannedPaths`].
//!
//! **Responsibility.** Provide one gate function ([`href_if_planned`]) that turns an internal
//! site target (`p/<page-id>.html`, `s/<version-id>.html`, `a/<area>.html`,
//! `<register>.html#<id>`) into a relative `href` when the target file path is in
//! [`PlannedPaths`], and render plain text (`<span>`) otherwise so no emitted page can
//! fail `FB0603` while sibling cards are not yet built.

use fieldbook_core::ids::{AreaCode, ClaimId, ConflictId, QuestionId, TermId};
use fieldbook_core::schema::{GlossaryRow, Lifecycle, SourceRow};

use crate::frame::root_prefix_for_path;
use crate::site::PlannedPaths;

/// Escape plain text for safe inclusion in HTML text or double-quoted attribute values.
#[must_use]
pub fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

fn parent_dir(path: &str) -> &str {
    let trimmed = path.trim_start_matches('/');
    match trimmed.rfind('/') {
        Some(idx) => &trimmed[..idx],
        None => "",
    }
}

fn relative_target(from_path: &str, target: &str) -> String {
    let (file_part, frag_opt) = match target.split_once('#') {
        Some((f, frag)) => (f, Some(frag)),
        None => (target, None),
    };
    let from_dir = parent_dir(from_path);
    let target_dir = parent_dir(file_part);
    if !from_dir.is_empty() && from_dir == target_dir {
        let base_name = match file_part.rfind('/') {
            Some(idx) => &file_part[idx + 1..],
            None => file_part,
        };
        return match frag_opt {
            Some(frag) => format!("{base_name}#{frag}"),
            None => base_name.to_owned(),
        };
    }
    format!("{}{target}", root_prefix_for_path(from_path))
}

/// The single gate function that turns a site-relative `target` (optionally carrying a
/// `#fragment`) into a relative `href` from `from_path` when the target's file path is
/// present in `planned`, or `None` when the target is not built yet.
#[must_use]
pub fn href_if_planned(from_path: &str, target: &str, planned: &PlannedPaths) -> Option<String> {
    let file_part = target.split('#').next().unwrap_or("");
    if file_part.is_empty() || !planned.contains(file_part) {
        return None;
    }
    Some(relative_target(from_path, target))
}

/// Resolve an area link target (`a/<area>.html`) if planned.
#[must_use]
pub fn area_href(from_path: &str, area: &AreaCode, planned: &PlannedPaths) -> Option<String> {
    href_if_planned(from_path, &format!("a/{area}.html"), planned)
}

/// Resolve a page link target (`p/<page-id>.html` with optional `#fragment`) if planned.
#[must_use]
pub fn page_href(
    from_path: &str,
    page_id: &str,
    fragment: Option<&str>,
    planned: &PlannedPaths,
) -> Option<String> {
    let target = match fragment {
        Some(frag) if !frag.is_empty() => format!("p/{page_id}.html#{frag}"),
        _ => format!("p/{page_id}.html"),
    };
    href_if_planned(from_path, &target, planned)
}

/// Resolve a claim register row target (`claims.html#<id>`) if planned.
#[must_use]
pub fn claim_href(from_path: &str, claim_id: &ClaimId, planned: &PlannedPaths) -> Option<String> {
    href_if_planned(from_path, &format!("claims.html#{claim_id}"), planned)
}

/// Resolve a question register row target (`questions.html#<id>`) if planned.
#[must_use]
pub fn question_href(
    from_path: &str,
    question_id: &QuestionId,
    planned: &PlannedPaths,
) -> Option<String> {
    href_if_planned(from_path, &format!("questions.html#{question_id}"), planned)
}

/// Resolve a conflict register row target (`conflicts.html#<id>`) if planned.
#[must_use]
pub fn conflict_href(
    from_path: &str,
    conflict_id: &ConflictId,
    planned: &PlannedPaths,
) -> Option<String> {
    href_if_planned(from_path, &format!("conflicts.html#{conflict_id}"), planned)
}

/// Resolve a glossary term target (`glossary.html#<term_id>`) if planned.
#[must_use]
pub fn term_href(from_path: &str, term_id: &TermId, planned: &PlannedPaths) -> Option<String> {
    href_if_planned(from_path, &format!("glossary.html#{term_id}"), planned)
}

/// Resolve a source version target: returns `None` when `is_quarantined` is `true`;
/// otherwise prefers `s/<version_id>.html` when planned and falls back to
/// `sources.html#<version_id>` when planned.
#[must_use]
pub fn version_href(
    from_path: &str,
    version_id: &str,
    is_quarantined: bool,
    planned: &PlannedPaths,
) -> Option<String> {
    if is_quarantined {
        return None;
    }
    href_if_planned(from_path, &format!("s/{version_id}.html"), planned)
        .or_else(|| href_if_planned(from_path, &format!("sources.html#{version_id}"), planned))
}

fn push_pv_attrs(out: &mut String, pv_key: &str, pv_title: Option<&str>, pv_body: Option<&str>) {
    out.push_str(" data-pv=\"");
    out.push_str(&escape_html(pv_key));
    out.push('"');
    if let Some(t) = pv_title
        && !t.is_empty()
    {
        out.push_str(" data-pv-title=\"");
        out.push_str(&escape_html(t));
        out.push('"');
    }
    if let Some(b) = pv_body
        && !b.is_empty()
    {
        out.push_str(" data-pv-body=\"");
        out.push_str(&escape_html(b));
        out.push('"');
    }
}

/// Render an area header chip (`CMP · Compliance`), linked to `a/<area>.html` if planned.
#[must_use]
pub fn render_area_chip(
    from_path: &str,
    area: &AreaCode,
    area_title: Option<&str>,
    planned: &PlannedPaths,
) -> String {
    let code_upper = area.as_str().to_ascii_uppercase();
    let label = match area_title {
        Some(t) if !t.is_empty() => format!("{code_upper} · {t}"),
        _ => code_upper,
    };
    let escaped = escape_html(&label);
    match area_href(from_path, area, planned) {
        Some(href) => format!(
            "<a class=\"chip accent\" href=\"{}\">{escaped}</a>",
            escape_html(&href)
        ),
        None => format!("<span class=\"chip accent\">{escaped}</span>"),
    }
}

/// Render an internal `PageLink` (`class="plink"`), linked if `p/<page_id>.html` is planned
/// and plain text `<span class="plink">` otherwise.
#[must_use]
pub fn render_page_link(
    from_path: &str,
    page_id: &str,
    fragment: Option<&str>,
    label_html: &str,
    pv_title: Option<&str>,
    pv_body: Option<&str>,
    planned: &PlannedPaths,
) -> String {
    let pv_key = format!("page:{page_id}");
    match page_href(from_path, page_id, fragment, planned) {
        Some(href) => {
            let mut out = format!("<a class=\"plink\" href=\"{}\"", escape_html(&href));
            push_pv_attrs(&mut out, &pv_key, pv_title, pv_body);
            out.push('>');
            out.push_str(label_html);
            out.push_str("</a>");
            out
        }
        None => {
            let mut out = String::from("<span class=\"plink\"");
            push_pv_attrs(&mut out, &pv_key, pv_title, pv_body);
            out.push('>');
            out.push_str(label_html);
            out.push_str("</span>");
            out
        }
    }
}

/// Render a `TermLink` (`class="term"`), linked to `glossary.html#<term_id>` if planned
/// and plain text `<span class="term">` otherwise.
#[must_use]
pub fn render_term_link(
    from_path: &str,
    term_id: &TermId,
    matched_text: &str,
    glossary_row: Option<&GlossaryRow>,
    planned: &PlannedPaths,
) -> String {
    let pv_key = format!("term:{term_id}");
    let pv_title = glossary_row.map(|g| g.label.as_str());
    let pv_body = glossary_row.map(|g| g.definition.as_str());
    let escaped_text = escape_html(matched_text);
    match term_href(from_path, term_id, planned) {
        Some(href) => {
            let mut out = format!("<a class=\"term\" href=\"{}\"", escape_html(&href));
            push_pv_attrs(&mut out, &pv_key, pv_title, pv_body);
            out.push('>');
            out.push_str(&escaped_text);
            out.push_str("</a>");
            out
        }
        None => {
            let mut out = String::from("<span class=\"term\"");
            push_pv_attrs(&mut out, &pv_key, pv_title, pv_body);
            out.push('>');
            out.push_str(&escaped_text);
            out.push_str("</span>");
            out
        }
    }
}

/// Render a `ClaimMark` (`class="claim-mark {variant}"`), linked to `claims.html#<claim_id>`
/// when `claims.html` is planned, and plain text `<span class="claim-mark ...">` otherwise.
#[must_use]
pub fn render_claim_mark(
    from_path: &str,
    claim_id: &ClaimId,
    variant: &str,
    ver_label: &str,
    icon_svg: &str,
    planned: &PlannedPaths,
) -> String {
    let cid = escape_html(claim_id.as_str());
    let var = escape_html(variant);
    let vlabel = escape_html(ver_label);
    let attrs = format!(
        "class=\"claim-mark {var}\" data-claim=\"{cid}\" aria-label=\"Claim {cid}: {vlabel}\" title=\"{cid} · {vlabel}\""
    );
    match claim_href(from_path, claim_id, planned) {
        Some(href) => format!(
            "<a href=\"{}\" {attrs}>{icon_svg}<span>{cid}</span></a>",
            escape_html(&href)
        ),
        None => format!("<span {attrs}>{icon_svg}<span>{cid}</span></span>"),
    }
}

/// Render a mono claim reference chip (`CL-...`), linked to `claims.html#<claim_id>` if planned.
#[must_use]
pub fn render_claim_chip(from_path: &str, claim_id: &ClaimId, planned: &PlannedPaths) -> String {
    let cid = escape_html(claim_id.as_str());
    match claim_href(from_path, claim_id, planned) {
        Some(href) => format!(
            "<a class=\"cite neutral\" href=\"{}\" data-claim=\"{cid}\">{cid}</a>",
            escape_html(&href)
        ),
        None => format!("<span class=\"cite neutral\" data-claim=\"{cid}\">{cid}</span>"),
    }
}

/// Render a neutral `RefChip` for an open question (`Q-nnn`).
#[must_use]
pub fn render_question_chip(
    from_path: &str,
    question_id: &QuestionId,
    pv_body: Option<&str>,
    planned: &PlannedPaths,
) -> String {
    let qid = escape_html(question_id.as_str());
    let pv_key = format!("q:{question_id}");
    match question_href(from_path, question_id, planned) {
        Some(href) => {
            let mut out = format!("<a class=\"cite neutral\" href=\"{}\"", escape_html(&href));
            push_pv_attrs(&mut out, &pv_key, Some(question_id.as_str()), pv_body);
            out.push('>');
            out.push_str(&qid);
            out.push_str("</a>");
            out
        }
        None => {
            let mut out = String::from("<span class=\"cite neutral\"");
            push_pv_attrs(&mut out, &pv_key, Some(question_id.as_str()), pv_body);
            out.push('>');
            out.push_str(&qid);
            out.push_str("</span>");
            out
        }
    }
}

/// Render a neutral `RefChip` for a conflict (`XC-nn`).
#[must_use]
pub fn render_conflict_chip(
    from_path: &str,
    conflict_id: &ConflictId,
    pv_body: Option<&str>,
    planned: &PlannedPaths,
) -> String {
    let cid = escape_html(conflict_id.as_str());
    let pv_key = format!("xc:{conflict_id}");
    match conflict_href(from_path, conflict_id, planned) {
        Some(href) => {
            let mut out = format!("<a class=\"cite neutral\" href=\"{}\"", escape_html(&href));
            push_pv_attrs(&mut out, &pv_key, Some(conflict_id.as_str()), pv_body);
            out.push('>');
            out.push_str(&cid);
            out.push_str("</a>");
            out
        }
        None => {
            let mut out = String::from("<span class=\"cite neutral\"");
            push_pv_attrs(&mut out, &pv_key, Some(conflict_id.as_str()), pv_body);
            out.push('>');
            out.push_str(&cid);
            out.push_str("</span>");
            out
        }
    }
}

/// Render a `CitationChip` for a source version and optional locator.
///
/// - A held (`Lifecycle::Quarantined`) source version is never linked; it renders as plain text
///   alongside its escaped hold reason (`note`).
/// - A `metadata-only:` source version carries the `meta-only` class and note.
/// - Otherwise links via [`version_href`] when `s/<version_id>.html` or `sources.html` is in
///   `planned`, or renders as `<span class="cite">` when neither is built yet.
#[must_use]
pub fn render_citation_chip(
    from_path: &str,
    version_id: &str,
    locator: Option<&str>,
    source: Option<&SourceRow>,
    planned: &PlannedPaths,
) -> String {
    let is_quarantined = source.is_some_and(|s| s.lifecycle == Lifecycle::Quarantined);
    let is_meta_only = source.is_some_and(|s| s.location.starts_with("metadata-only:"));
    let vid_esc = escape_html(version_id);
    let loc_html = match locator.filter(|l| !l.is_empty()) {
        Some(loc) => format!(" <span class=\"loc\">{}</span>", escape_html(loc)),
        None => String::new(),
    };

    if is_quarantined {
        let reason = source
            .and_then(|s| s.note.as_deref())
            .filter(|n| !n.is_empty())
            .unwrap_or("held");
        return format!(
            "<span class=\"cite held\"><span>{vid_esc}</span>{loc_html} <span class=\"hold-reason\">(Held: {})</span></span>",
            escape_html(reason)
        );
    }

    let class_name = if is_meta_only {
        "cite meta-only"
    } else {
        "cite"
    };
    let meta_badge = if is_meta_only {
        " <span class=\"meta-only-tag\">Metadata only</span>"
    } else {
        ""
    };
    let pv_key = format!("src:{version_id}");
    let pv_title = source.map(|s| s.title.as_str());
    let pv_body = source.map(|s| s.publisher.as_str());

    match version_href(from_path, version_id, false, planned) {
        Some(href) => {
            let mut out = format!("<a class=\"{class_name}\" href=\"{}\"", escape_html(&href));
            push_pv_attrs(&mut out, &pv_key, pv_title, pv_body);
            out.push_str(&format!(
                "><span>{vid_esc}</span>{loc_html}{meta_badge}</a>"
            ));
            out
        }
        None => {
            let mut out = format!("<span class=\"{class_name}\"");
            push_pv_attrs(&mut out, &pv_key, pv_title, pv_body);
            out.push_str(&format!(
                "><span>{vid_esc}</span>{loc_html}{meta_badge}</span>"
            ));
            out
        }
    }
}
