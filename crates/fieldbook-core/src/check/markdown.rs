//! Shared Markdown reader for page bodies (`check-page-links`, `check-page-parts`, `check-page-body`).
//!
//! Reads a page body with [`pulldown_cmark`] using the body extensions of
//! `docs/reference/corpus-format.md#page-files` (pipe tables and task lists) and nothing else,
//! mapping byte offsets to 1-based page-file line numbers and 1-based Unicode scalar (code-point)
//! columns.

pub use pulldown_cmark::{Event, HeadingLevel, LinkType, Tag, TagEnd};

use pulldown_cmark::{Options, Parser};

/// A Markdown event paired with its 1-based line and 1-based code-point column in the page file.
#[derive(Debug, Clone, PartialEq)]
pub struct Ev<'a> {
    /// The `pulldown-cmark` event.
    pub event: Event<'a>,
    /// 1-based line number in the page file.
    pub line: usize,
    /// 1-based Unicode code-point column on `line`.
    pub col: usize,
}

/// A link extracted from a page body, with its 1-based line and code-point column in the page file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyLink {
    /// The link target URL/path as written.
    pub target: String,
    /// 1-based line number in the page file.
    pub line: usize,
    /// 1-based Unicode code-point column on `line`.
    pub col: usize,
}

/// Parse `body` with `pulldown-cmark` (pipe tables and task lists enabled, and nothing else) and
/// return all events with 1-based page-file lines and 1-based code-point columns.
///
/// `body_start_line` is the 1-based page-file line of `body`'s first line.
#[must_use]
pub fn events(body: &str, body_start_line: usize) -> Vec<Ev<'_>> {
    let base_line = body_start_line.max(1);
    let line_starts = compute_line_starts(body);
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_TASKLISTS);

    let mut out = Vec::new();
    for (event, range) in Parser::new_ext(body, opts).into_offset_iter() {
        let offset = range.start.min(body.len());
        let line_idx = line_starts
            .partition_point(|&start| start <= offset)
            .saturating_sub(1);
        let line_start = line_starts[line_idx];
        let col = body[line_start..offset].chars().count() + 1;
        let line = base_line + line_idx;
        out.push(Ev { event, line, col });
    }
    out
}

/// Extract all links (`Tag::Link`) from `body`, returning their targets and 1-based page-file
/// positions.
///
/// Adjacent citations like `[@CL-CMP-0001][@CL-CMP-0002]` with no link definition are not emitted
/// as `Tag::Link` by `pulldown-cmark` and are therefore ignored here.
#[must_use]
pub fn body_links(body: &str, body_start_line: usize) -> Vec<BodyLink> {
    let mut out = Vec::new();
    for ev in events(body, body_start_line) {
        if let Event::Start(Tag::Link { dest_url, .. }) = ev.event {
            out.push(BodyLink {
                target: dest_url.into_string(),
                line: ev.line,
                col: ev.col,
            });
        }
    }
    out
}

fn compute_line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (idx, b) in text.bytes().enumerate() {
        if b == b'\n' {
            starts.push(idx + 1);
        }
    }
    starts
}

#[cfg(test)]
#[path = "../../../../tests/support/rng.rs"]
mod rng;

#[cfg(test)]
mod tests {
    use super::{Event, Tag, body_links, events, rng::Rng};

    #[test]
    fn lines_are_counted_in_the_page_file() {
        let body = "## Purpose\n\n§ 15.404-1 — café [review](cmp-system-review.md).\n";
        let evs = events(body, 7);
        assert!(!evs.is_empty());
        assert_eq!(evs[0].line, 7);
        assert_eq!(evs[0].col, 1);

        let links = body_links(body, 7);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "cmp-system-review.md");
        // Line 0 of body is 7, line 1 is 8 (blank), line 2 is 9.
        assert_eq!(links[0].line, 9);
        // "§ 15.404-1 — café " is 18 code points, so `[review]` starts at col 19.
        assert_eq!(links[0].col, 19);

        // Adjacent citations without a link definition are not treated as links.
        let cite_body = "Rule text [@CL-CMP-0001][@CL-CMP-0002] and [page](other.md).\n";
        let cite_links = body_links(cite_body, 10);
        assert_eq!(cite_links.len(), 1);
        assert_eq!(cite_links[0].target, "other.md");
        assert_eq!(cite_links[0].line, 10);

        // Confirm tables and tasklists are enabled and nothing else (like strikethrough).
        let ext_body = "- [x] done\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\n~~not strike~~\n";
        let ext_evs = events(ext_body, 1);
        assert!(
            ext_evs
                .iter()
                .any(|e| matches!(e.event, Event::TaskListMarker(true)))
        );
        assert!(
            ext_evs
                .iter()
                .any(|e| matches!(e.event, Event::Start(Tag::Table(_))))
        );
        assert!(
            !ext_evs
                .iter()
                .any(|e| matches!(e.event, Event::Start(Tag::Strikethrough)))
        );
    }

    #[test]
    fn events_never_panic_on_200_seeded_inputs() {
        let mut prng = Rng::new(1);
        for i in 0..200 {
            let len = usize::try_from(prng.next_u64() % 160).unwrap_or(0);
            let raw = prng.bytes(len);
            let text = String::from_utf8_lossy(&raw);
            let start_line = (i % 15) + 1;
            let max_line = start_line + text.bytes().filter(|&b| b == b'\n').count();
            for ev in events(&text, start_line) {
                assert!(ev.line >= start_line && ev.line <= max_line);
                assert!(ev.col >= 1);
            }
            for link in body_links(&text, start_line) {
                assert!(link.line >= start_line && link.line <= max_line);
                assert!(link.col >= 1);
            }
        }

        let seed = "## Purpose\n\n§ 15.404-1 — café — 日本語 [link](other.md#purpose) [@CL-CMP-0001]\n\n### Sub\n\n- [ ] task item\n\n| Col |\n|---|\n| Val |\n";
        let seed_bytes = seed.as_bytes();
        for idx in 0..seed_bytes.len() {
            // Single-byte flip.
            let mut flipped = seed_bytes.to_vec();
            flipped[idx] ^= 0x5a;
            let text_flip = String::from_utf8_lossy(&flipped);
            let max_line_flip = 5 + text_flip.bytes().filter(|&b| b == b'\n').count();
            for ev in events(&text_flip, 5) {
                assert!(ev.line >= 5 && ev.line <= max_line_flip);
                assert!(ev.col >= 1);
            }
            for link in body_links(&text_flip, 5) {
                assert!(link.line >= 5 && link.line <= max_line_flip);
                assert!(link.col >= 1);
            }

            // Single-byte deletion.
            let mut deleted = seed_bytes.to_vec();
            deleted.remove(idx);
            let text_del = String::from_utf8_lossy(&deleted);
            let max_line_del = 5 + text_del.bytes().filter(|&b| b == b'\n').count();
            for ev in events(&text_del, 5) {
                assert!(ev.line >= 5 && ev.line <= max_line_del);
                assert!(ev.col >= 1);
            }
            for link in body_links(&text_del, 5) {
                assert!(link.line >= 5 && link.line <= max_line_del);
                assert!(link.col >= 1);
            }
        }
    }
}
