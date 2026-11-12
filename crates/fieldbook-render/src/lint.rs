//! Site lints (`FB0603`, `FB0604`, `FB0605`) over rendered [`Site`] files (`build-site-lints`).

use std::collections::{BTreeMap, BTreeSet};

use fieldbook_core::check::{Code, Diagnostic};

use crate::csp::CSP_STATIC;
use crate::site::Site;

/// Budget for `assets/app.js` in decimal bytes (`FB0605`).
pub const APP_JS: u64 = 32_000;
/// Budget for `assets/search.js` in decimal bytes (`FB0605`).
pub const SEARCH_JS: u64 = 10_000;
/// Budget for `assets/assist.js` in decimal bytes (`FB0605`).
pub const ASSIST_JS: u64 = 20_000;
/// Total budget across all script files in decimal bytes (`FB0605`).
pub const SCRIPTS_TOTAL: u64 = 64_000;
/// Budget for each data file other than the search index (`assets/assist-data.js`, `data/*`) in decimal bytes (`FB0605`).
pub const DATA_FILE: u64 = 3_000_000;
/// Budget for each register copy (`data/registers.xlsx` and `data/<register>.csv`) in bytes
/// (`FB0605`). The copies are downloads that no page loads, so they take the hosted site's
/// per-file limit (25 MiB) rather than the script-loaded data budget.
pub const REGISTER_COPY: u64 = 26_214_400;
/// Budget for `assets/search-index.js` alone in decimal bytes (`FB0605`). The edition index holds
/// page and section units only (`docs/reference/search.md#the-site-index`).
pub const SEARCH_INDEX: u64 = 15_000_000;

/// Shipped filenames that `originals/` will contain (an HTML or SVG original is `<version-id>.<ext>.txt`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OriginalNames(pub BTreeSet<String>);

/// Parsed HTML attribute inside a tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlAttr {
    /// Lowercase attribute name.
    pub name: String,
    /// Attribute value (empty if valueless).
    pub value: String,
    /// 1-based line of the attribute name.
    pub line: usize,
    /// 1-based code-point column of the attribute name.
    pub col: usize,
}

/// Parsed HTML start or end tag from [`scan_html`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlTag {
    /// Lowercase tag name (`a`, `script`, `meta`, etc.).
    pub name: String,
    /// `true` if the tag starts with `</`.
    pub is_closing: bool,
    /// `true` if the tag ends with `/>`.
    pub self_closing: bool,
    /// Attributes on this tag.
    pub attrs: Vec<HtmlAttr>,
    /// 1-based line where `<` starts.
    pub line: usize,
    /// 1-based code-point column where `<` starts.
    pub col: usize,
    /// Byte offset where `<` starts in the decoded text.
    pub start_byte: usize,
    /// Byte offset immediately after `>` in the decoded text.
    pub end_byte: usize,
}

/// Typed error from [`scan_html`] when a tag or comment is malformed or unterminated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanError {
    /// Human-readable description of the syntax error.
    pub message: String,
    /// Byte offset within the input slice (`0..=input.len()`).
    pub byte_offset: usize,
    /// 1-based line number (`>= 1`).
    pub line: usize,
    /// 1-based code-point column number (`>= 1`).
    pub col: usize,
}

fn compute_line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (idx, b) in text.bytes().enumerate() {
        if b == b'\n' {
            starts.push(idx + 1);
        }
    }
    starts
}

fn line_col_at(text: &str, line_starts: &[usize], byte_offset: usize) -> (usize, usize) {
    let clamped = byte_offset.min(text.len());
    let mut end = clamped;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let line_idx = line_starts.partition_point(|&s| s <= end).saturating_sub(1);
    let line = line_idx + 1;
    let last_nl = line_starts[line_idx];
    let line_slice = &text[last_nl..end];
    let col = line_slice.chars().count() + 1;
    (line, col)
}

/// Scans `input` (decoded lossily as UTF-8) into a sequence of [`HtmlTag`]s, or returns a
/// [`ScanError`] with positions bounded within `input`. Never panics on arbitrary byte sequences.
///
/// # Errors
///
/// Returns [`ScanError`] if an HTML comment, tag, or quoted attribute is unterminated.
pub fn scan_html(input: &[u8]) -> Result<Vec<HtmlTag>, ScanError> {
    let text = String::from_utf8_lossy(input);
    scan_html_str(&text, input.len())
}

fn scan_html_str(text: &str, raw_len: usize) -> Result<Vec<HtmlTag>, ScanError> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let line_starts = compute_line_starts(text);
    let mut tags = Vec::new();
    let mut i = 0usize;

    while i < len {
        let Some(rel_lt) = text[i..].find('<') else {
            break;
        };
        let tag_start = i + rel_lt;
        let (tag_line, tag_col) = line_col_at(text, &line_starts, tag_start);

        if tag_start + 1 >= len {
            return Err(ScanError {
                message: "unterminated `<` at end of input".to_owned(),
                byte_offset: tag_start.min(raw_len),
                line: tag_line,
                col: tag_col,
            });
        }

        // Comment `<!-- ... -->`
        if text[tag_start..].starts_with("<!--") {
            let after_open = tag_start + 4;
            let Some(rel_close) = text[after_open..].find("-->") else {
                return Err(ScanError {
                    message: "unterminated HTML comment".to_owned(),
                    byte_offset: tag_start.min(raw_len),
                    line: tag_line,
                    col: tag_col,
                });
            };
            i = after_open + rel_close + 3;
            continue;
        }

        // Declaration `<!DOCTYPE ...>`
        if text[tag_start..].starts_with("<!") || text[tag_start..].starts_with("<?") {
            let after_open = tag_start + 2;
            let Some(rel_gt) = text[after_open..].find('>') else {
                return Err(ScanError {
                    message: "unterminated declaration".to_owned(),
                    byte_offset: tag_start.min(raw_len),
                    line: tag_line,
                    col: tag_col,
                });
            };
            i = after_open + rel_gt + 1;
            continue;
        }

        let mut p = tag_start + 1;
        let is_closing = if p < len && bytes[p] == b'/' {
            p += 1;
            true
        } else {
            false
        };

        while p < len && bytes[p].is_ascii_whitespace() {
            p += 1;
        }

        let name_start = p;
        while p < len
            && (bytes[p].is_ascii_alphanumeric() || matches!(bytes[p], b'-' | b'_' | b':'))
        {
            p += 1;
        }

        if p == name_start {
            // Not a valid tag name after `<`; advance past `<` and continue or return ScanError.
            return Err(ScanError {
                message: "empty tag name after `<`".to_owned(),
                byte_offset: tag_start.min(raw_len),
                line: tag_line,
                col: tag_col,
            });
        }

        let name = text[name_start..p].to_ascii_lowercase();
        let mut attrs = Vec::new();
        let mut self_closing = false;

        loop {
            while p < len && bytes[p].is_ascii_whitespace() {
                p += 1;
            }
            if p >= len {
                return Err(ScanError {
                    message: format!("unterminated `<{name}` tag"),
                    byte_offset: tag_start.min(raw_len),
                    line: tag_line,
                    col: tag_col,
                });
            }
            if bytes[p] == b'>' {
                p += 1;
                break;
            }
            if p + 1 < len && bytes[p] == b'/' && bytes[p + 1] == b'>' {
                self_closing = true;
                p += 2;
                break;
            }
            if bytes[p] == b'/' {
                p += 1;
                continue;
            }

            let attr_start = p;
            let (attr_line, attr_col) = line_col_at(text, &line_starts, attr_start);
            while p < len
                && !bytes[p].is_ascii_whitespace()
                && !matches!(bytes[p], b'=' | b'>' | b'/' | b'"' | b'\'')
            {
                p += 1;
            }
            if p == attr_start {
                return Err(ScanError {
                    message: format!("unexpected character in `<{name}` tag"),
                    byte_offset: attr_start.min(raw_len),
                    line: attr_line,
                    col: attr_col,
                });
            }
            let attr_name = text[attr_start..p].to_ascii_lowercase();

            while p < len && bytes[p].is_ascii_whitespace() {
                p += 1;
            }

            let mut attr_value = String::new();
            if p < len && bytes[p] == b'=' {
                p += 1;
                while p < len && bytes[p].is_ascii_whitespace() {
                    p += 1;
                }
                if p >= len {
                    return Err(ScanError {
                        message: format!("unterminated attribute `{attr_name}=`"),
                        byte_offset: attr_start.min(raw_len),
                        line: attr_line,
                        col: attr_col,
                    });
                }
                if bytes[p] == b'"' || bytes[p] == b'\'' {
                    let quote = bytes[p];
                    p += 1;
                    let val_start = p;
                    while p < len && bytes[p] != quote {
                        p += 1;
                    }
                    if p >= len {
                        return Err(ScanError {
                            message: format!(
                                "unterminated quoted attribute `{attr_name}` in `<{name}`"
                            ),
                            byte_offset: attr_start.min(raw_len),
                            line: attr_line,
                            col: attr_col,
                        });
                    }
                    attr_value = text[val_start..p].to_owned();
                    p += 1;
                } else {
                    let val_start = p;
                    while p < len && !bytes[p].is_ascii_whitespace() && bytes[p] != b'>' {
                        p += 1;
                    }
                    attr_value = text[val_start..p].to_owned();
                }
            }

            attrs.push(HtmlAttr {
                name: attr_name,
                value: attr_value,
                line: attr_line,
                col: attr_col,
            });
        }

        let end_byte = p;
        tags.push(HtmlTag {
            name,
            is_closing,
            self_closing,
            attrs,
            line: tag_line,
            col: tag_col,
            start_byte: tag_start,
            end_byte,
        });
        i = end_byte;
    }

    Ok(tags)
}

fn has_scheme(url: &str) -> bool {
    let Some(colon_idx) = url.find(':') else {
        return false;
    };
    if colon_idx == 0 {
        return false;
    }
    let prefix = &url[..colon_idx];
    if prefix.contains('/') || prefix.contains('#') || prefix.contains('?') {
        return false;
    }
    let mut chars = prefix.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

fn resolve_relative_path(page_path: &str, target_rel: &str) -> Option<String> {
    if target_rel.starts_with('/') || target_rel.contains('\\') {
        return None;
    }
    let mut segments: Vec<&str> = match page_path.rfind('/') {
        Some(idx) => page_path[..idx]
            .split('/')
            .filter(|s| !s.is_empty())
            .collect(),
        None => Vec::new(),
    };
    for part in target_rel.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if segments.is_empty() || segments.last() == Some(&"..") {
                    segments.push("..");
                } else {
                    segments.pop();
                }
            }
            other => segments.push(other),
        }
    }
    Some(segments.join("/"))
}

fn is_valid_pdf_page_fragment(frag: &str) -> bool {
    let Some(num_str) = frag.strip_prefix("page=") else {
        return false;
    };
    if num_str.is_empty() || !num_str.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    num_str.parse::<u64>().is_ok_and(|n| n >= 1)
}

fn is_valid_search_query_fragment(frag: &str) -> bool {
    let Some(query) = frag.strip_prefix("q=") else {
        return false;
    };
    let bytes = query.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len()
                || !bytes[i + 1].is_ascii_hexdigit()
                || !bytes[i + 2].is_ascii_hexdigit()
            {
                return false;
            }
            i += 3;
        } else if b.is_ascii_graphic() && b != b'#' && b != b' ' {
            i += 1;
        } else {
            return false;
        }
    }
    true
}

/// Lints `site` and `originals` in memory for `FB0603`, `FB0604` and `FB0605`.
#[must_use]
pub fn lint(site: &Site, originals: &OriginalNames) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let mut html_ids: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut scanned_pages: Vec<(String, std::borrow::Cow<'_, str>, Vec<HtmlTag>)> = Vec::new();

    for (page_path, bytes) in site.files() {
        if !(std::path::Path::new(page_path)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("html")))
        {
            continue;
        }
        let text = String::from_utf8_lossy(bytes);
        match scan_html_str(&text, bytes.len()) {
            Ok(tags) => {
                let mut ids = BTreeSet::new();
                for tag in &tags {
                    if tag.is_closing {
                        continue;
                    }
                    for attr in &tag.attrs {
                        if attr.name == "id" && !attr.value.is_empty() {
                            ids.insert(attr.value.clone());
                        }
                    }
                }
                html_ids.insert(page_path.clone(), ids);
                scanned_pages.push((page_path.clone(), text, tags));
            }
            Err(err) => {
                html_ids.insert(page_path.clone(), BTreeSet::new());
                diags.push(Diagnostic::new(
                    Code::Fb0604,
                    false,
                    page_path.clone(),
                    err.line,
                    err.col,
                    format!("malformed HTML in `{page_path}`: {}", err.message),
                ));
            }
        }
    }

    // 1. Per-HTML-file checks (FB0603 & FB0604)
    for (page_path, text, tags) in &scanned_pages {
        let mut csp_metas: Vec<(usize, usize, String)> = Vec::new();
        let mut open_script: Option<(usize, usize, Option<String>, usize)> = None;

        for tag in tags {
            if tag.is_closing {
                if tag.name == "script"
                    && let Some((s_line, s_col, src_opt, content_start)) = open_script.take()
                {
                    let inner = text.get(content_start..tag.start_byte).unwrap_or("").trim();
                    if let Some(src_val) = src_opt
                        && !inner.is_empty()
                    {
                        diags.push(Diagnostic::new(
                            Code::Fb0604,
                            false,
                            page_path.clone(),
                            s_line,
                            s_col,
                            format!(
                                "`<script src=\"{src_val}\">` must not have inline content in `{page_path}`"
                            ),
                        ));
                    }
                }
                continue;
            }

            if tag.name == "meta" {
                let is_csp = tag.attrs.iter().any(|a| {
                    a.name == "http-equiv"
                        && a.value.eq_ignore_ascii_case("Content-Security-Policy")
                });
                if is_csp {
                    let content = tag
                        .attrs
                        .iter()
                        .find(|a| a.name == "content")
                        .map_or_else(String::new, |a| a.value.clone());
                    csp_metas.push((tag.line, tag.col, content));
                }
            }

            if tag.name == "script" {
                if let Some((prev_line, prev_col, _, _)) = open_script.take() {
                    diags.push(Diagnostic::new(
                        Code::Fb0604,
                        false,
                        page_path.clone(),
                        prev_line,
                        prev_col,
                        format!("unclosed `<script>` tag in `{page_path}`"),
                    ));
                }
                let src_attr = tag
                    .attrs
                    .iter()
                    .find(|a| a.name == "src")
                    .map(|a| a.value.clone());
                match src_attr.as_deref() {
                    None => {
                        diags.push(Diagnostic::new(
                            Code::Fb0604,
                            false,
                            page_path.clone(),
                            tag.line,
                            tag.col,
                            format!("inline `<script>` is forbidden in `{page_path}`"),
                        ));
                    }
                    Some(src_val) => {
                        check_script_src(page_path, tag.line, tag.col, src_val, &mut diags);
                    }
                }
                if !tag.self_closing {
                    open_script = Some((tag.line, tag.col, src_attr, tag.end_byte));
                }
            }

            for attr in &tag.attrs {
                if attr.name == "style" {
                    diags.push(Diagnostic::new(
                        Code::Fb0604,
                        false,
                        page_path.clone(),
                        attr.line,
                        attr.col,
                        format!("inline `style=\"\"` attribute is forbidden in `{page_path}`"),
                    ));
                } else if attr.name.starts_with("on") && attr.name.len() > 2 {
                    diags.push(Diagnostic::new(
                        Code::Fb0604,
                        false,
                        page_path.clone(),
                        attr.line,
                        attr.col,
                        format!(
                            "inline event handler `{}=` is forbidden in `{page_path}`",
                            attr.name
                        ),
                    ));
                }

                if attr.name == "href" || attr.name == "src" {
                    let raw_val = attr.value.as_str();
                    let raw_lower = raw_val.to_ascii_lowercase();
                    if attr.name == "href"
                        && (raw_lower.starts_with("http:")
                            || raw_lower.starts_with("https:")
                            || raw_lower.starts_with("http"))
                        && has_scheme(raw_val)
                    {
                        diags.push(Diagnostic::new(
                            Code::Fb0604,
                            false,
                            page_path.clone(),
                            attr.line,
                            attr.col,
                            format!(
                                "external link `href=\"{raw_val}\"` is forbidden in `{page_path}`"
                            ),
                        ));
                        continue;
                    }

                    if !has_scheme(raw_val) && !raw_val.starts_with("//") {
                        check_relative_link(
                            site, originals, &html_ids, page_path, attr, &mut diags,
                        );
                    }
                }
            }
        }

        if let Some((s_line, s_col, _, _)) = open_script.take() {
            diags.push(Diagnostic::new(
                Code::Fb0604,
                false,
                page_path.clone(),
                s_line,
                s_col,
                format!("unclosed `<script>` tag in `{page_path}`"),
            ));
        }

        if csp_metas.len() != 1 {
            diags.push(Diagnostic::new(
                Code::Fb0604,
                false,
                page_path.clone(),
                1,
                1,
                format!(
                    "page `{page_path}` must carry exactly 1 CSP meta tag, found {}",
                    csp_metas.len()
                ),
            ));
        } else if csp_metas[0].2 != CSP_STATIC {
            diags.push(Diagnostic::new(
                Code::Fb0604,
                false,
                page_path.clone(),
                csp_metas[0].0,
                csp_metas[0].1,
                format!(
                    "page `{page_path}` CSP meta does not equal CSP_STATIC: `{}`",
                    csp_metas[0].2
                ),
            ));
        }
    }

    // 2. Script and data file budgets (FB0605)
    check_budgets(site, &mut diags);

    diags.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.col.cmp(&b.col))
            .then_with(|| a.code.id().cmp(b.code.id()))
            .then_with(|| a.message.cmp(&b.message))
    });
    diags
}

fn check_script_src(
    page_path: &str,
    line: usize,
    col: usize,
    src_val: &str,
    diags: &mut Vec<Diagnostic>,
) {
    if has_scheme(src_val) || src_val.starts_with("//") {
        diags.push(Diagnostic::new(
            Code::Fb0604,
            false,
            page_path.to_owned(),
            line,
            col,
            format!("remote `<script src=\"{src_val}\">` is forbidden in `{page_path}`"),
        ));
        return;
    }
    let path_part = src_val.split('#').next().unwrap_or(src_val);
    let Some(resolved) = resolve_relative_path(page_path, path_part) else {
        diags.push(Diagnostic::new(
            Code::Fb0604,
            false,
            page_path.to_owned(),
            line,
            col,
            format!("invalid `<script src=\"{src_val}\">` path in `{page_path}`"),
        ));
        return;
    };
    if !resolved.starts_with("assets/") || resolved.len() <= "assets/".len() {
        diags.push(Diagnostic::new(
            Code::Fb0604,
            false,
            page_path.to_owned(),
            line,
            col,
            format!(
                "`<script src=\"{src_val}\">` in `{page_path}` must resolve under `assets/`, got `{resolved}`"
            ),
        ));
    }
}

fn check_relative_link(
    site: &Site,
    originals: &OriginalNames,
    html_ids: &BTreeMap<String, BTreeSet<String>>,
    page_path: &str,
    attr: &HtmlAttr,
    diags: &mut Vec<Diagnostic>,
) {
    let raw = attr.value.as_str();
    let (path_part, frag_opt) = match raw.split_once('#') {
        Some((p, f)) => (p, Some(f)),
        None => (raw, None),
    };

    // Directory link rule: a link that is empty (without fragment), ends with `/`, or ends with `.` / `..` must name `index.html`.
    if (path_part.is_empty() && frag_opt.is_none())
        || path_part.ends_with('/')
        || path_part == "."
        || path_part == ".."
        || path_part.ends_with("/.")
        || path_part.ends_with("/..")
    {
        diags.push(Diagnostic::new(
            Code::Fb0603,
            false,
            page_path.to_owned(),
            attr.line,
            attr.col,
            format!("directory link `{raw}` in `{page_path}` must name `index.html`"),
        ));
        return;
    }

    let resolved = if path_part.is_empty() {
        page_path.to_owned()
    } else {
        let Some(r) = resolve_relative_path(page_path, path_part) else {
            diags.push(Diagnostic::new(
                Code::Fb0603,
                false,
                page_path.to_owned(),
                attr.line,
                attr.col,
                format!("unresolved link `{raw}` in `{page_path}`"),
            ));
            return;
        };
        r
    };

    // Sibling originals/ link (`../originals/<name>` or `originals/<name>` depending on how it's referenced)
    let orig_name_opt = resolved
        .strip_prefix("../originals/")
        .or_else(|| resolved.strip_prefix("originals/"));

    if let Some(orig_name) = orig_name_opt {
        if orig_name.is_empty() || orig_name.contains('/') || !originals.0.contains(orig_name) {
            diags.push(Diagnostic::new(
                Code::Fb0603,
                false,
                page_path.to_owned(),
                attr.line,
                attr.col,
                format!(
                    "link `{raw}` in `{page_path}` does not resolve to a shipped original (`{orig_name}`)"
                ),
            ));
            return;
        }
        if let Some(frag) = frag_opt {
            let is_pdf = std::path::Path::new(orig_name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"));
            if !(is_pdf && is_valid_pdf_page_fragment(frag)) {
                diags.push(Diagnostic::new(
                    Code::Fb0603,
                    false,
                    page_path.to_owned(),
                    attr.line,
                    attr.col,
                    format!("invalid fragment `#{frag}` on original link `{raw}` in `{page_path}`"),
                ));
            }
        }
        return;
    }

    if !site.files().contains_key(&resolved) {
        diags.push(Diagnostic::new(
            Code::Fb0603,
            false,
            page_path.to_owned(),
            attr.line,
            attr.col,
            format!(
                "link `{raw}` in `{page_path}` does not resolve to a built site file (`{resolved}`)"
            ),
        ));
        return;
    }

    if let Some(frag) = frag_opt {
        if resolved == "search.html" && is_valid_search_query_fragment(frag) {
            return;
        }
        let has_id = html_ids
            .get(&resolved)
            .is_some_and(|ids| ids.contains(frag));
        if !has_id {
            diags.push(Diagnostic::new(
                Code::Fb0603,
                false,
                page_path.to_owned(),
                attr.line,
                attr.col,
                format!(
                    "link `{raw}` in `{page_path}` targets missing anchor `#{frag}` in `{resolved}`"
                ),
            ));
        }
    }
}

/// The budget of a `data/` file that is not a script: a register copy, or any other data file.
fn data_budget(path: &str) -> u64 {
    let is_copy = path == fieldbook_core::strings::PATHS[0]
        || (path.starts_with("data/")
            && path.matches('/').count() == 1
            && std::path::Path::new(path)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("csv")));
    if is_copy { REGISTER_COPY } else { DATA_FILE }
}

fn check_budgets(site: &Site, diags: &mut Vec<Diagnostic>) {
    let mut scripts_total = 0u64;
    let mut has_any_script = false;

    for (path, bytes) in site.files() {
        let size = bytes.len() as u64;
        if path == "assets/search-index.js" || path == "assets/assist-data.js" {
            let limit = if path == "assets/search-index.js" {
                SEARCH_INDEX
            } else {
                DATA_FILE
            };
            if size > limit {
                diags.push(Diagnostic::new(
                    Code::Fb0605,
                    false,
                    path.clone(),
                    1,
                    1,
                    format!(
                        "data file `{path}` is {size} bytes, exceeding budget of {limit} bytes"
                    ),
                ));
            }
            continue;
        }

        let is_js = std::path::Path::new(path)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("js"));
        if is_js {
            has_any_script = true;
            scripts_total = scripts_total.saturating_add(size);

            let individual_limit = match path.as_str() {
                "assets/app.js" => Some(APP_JS),
                "assets/search.js" => Some(SEARCH_JS),
                "assets/assist.js" => Some(ASSIST_JS),
                _ => None,
            };
            if let Some(limit) = individual_limit
                && size > limit
            {
                diags.push(Diagnostic::new(
                    Code::Fb0605,
                    false,
                    path.clone(),
                    1,
                    1,
                    format!(
                        "script `{path}` is {size} bytes, exceeding individual budget of {limit} bytes"
                    ),
                ));
            }
            if path.starts_with("data/") && size > DATA_FILE {
                diags.push(Diagnostic::new(
                    Code::Fb0605,
                    false,
                    path.clone(),
                    1,
                    1,
                    format!(
                        "data file `{path}` is {size} bytes, exceeding budget of {DATA_FILE} bytes"
                    ),
                ));
            }
            continue;
        }

        if path.starts_with("data/") {
            let limit = data_budget(path);
            if size > limit {
                diags.push(Diagnostic::new(
                    Code::Fb0605,
                    false,
                    path.clone(),
                    1,
                    1,
                    format!(
                        "data file `{path}` is {size} bytes, exceeding budget of {limit} bytes"
                    ),
                ));
            }
        }
    }

    if has_any_script && scripts_total > SCRIPTS_TOTAL {
        diags.push(Diagnostic::new(
            Code::Fb0605,
            false,
            "assets/".to_owned(),
            1,
            1,
            format!(
                "total script size is {scripts_total} bytes, exceeding total budget of {SCRIPTS_TOTAL} bytes"
            ),
        ));
    }
}
