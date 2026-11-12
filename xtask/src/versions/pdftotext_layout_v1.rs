//! Retained implementation of the `pdftotext-layout@1` layout parser (published compatibility binding).
//!
//! **Responsibility.** The frozen decoding, argument vector, and page-splitting behavior of
//! `pdftotext-layout@1`: UTF-8 lossy decode of pdftotext output, splitting on form-feed (`\u{000c}`),
//! and dropping at most one trailing empty slice when output terminates with a trailing form-feed.
//! A change of these rules is a new `pdftotext-layout@2` in its own file; this file and its archived
//! vectors stay executable.
//!
//! **Rules.** Pure: no I/O.

/// Pinned command-line arguments for pdftotext layout extraction (`pdftotext-layout@1`).
pub const ARGS: [&str; 5] = ["-layout", "-enc", "UTF-8", "-", "-"];

/// Decodes pdftotext standard output bytes into individual page strings (`pdftotext-layout@1`).
///
/// Reproduces lossy UTF-8 decoding, splitting on form-feed (`\u{000c}`), and dropping at most
/// one trailing empty slice.
#[must_use]
pub fn pages_from_stdout(stdout: &[u8]) -> Vec<String> {
    let decoded = String::from_utf8_lossy(stdout);
    let mut parts: Vec<&str> = decoded.split('\u{c}').collect();
    if parts.last().is_some_and(|p| p.is_empty()) {
        parts.pop();
    }
    parts.into_iter().map(str::to_owned).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_match_contract() {
        assert_eq!(ARGS, ["-layout", "-enc", "UTF-8", "-", "-"]);
    }

    #[test]
    fn pages_from_empty_stdout() {
        let pages = pages_from_stdout(b"");
        assert!(pages.is_empty());
    }

    #[test]
    fn pages_from_single_page_no_trailing_formfeed() {
        let pages = pages_from_stdout(b"hello world");
        assert_eq!(pages, vec!["hello world"]);
    }

    #[test]
    fn pages_from_single_page_trailing_formfeed() {
        let pages = pages_from_stdout(b"hello world\x0c");
        assert_eq!(pages, vec!["hello world"]);
    }

    #[test]
    fn pages_from_multi_page() {
        let pages = pages_from_stdout(b"page 1\x0cpage 2\x0c");
        assert_eq!(pages, vec!["page 1", "page 2"]);
    }

    #[test]
    fn pages_from_multi_page_no_trailing_formfeed() {
        let pages = pages_from_stdout(b"page 1\x0cpage 2");
        assert_eq!(pages, vec!["page 1", "page 2"]);
    }

    #[test]
    fn pages_drops_only_one_trailing_empty() {
        let pages = pages_from_stdout(b"page 1\x0c\x0c");
        assert_eq!(pages, vec!["page 1", ""]);
    }

    #[test]
    fn pages_lossy_utf8_replacement() {
        let pages = pages_from_stdout(b"page \xff\xfe 1\x0c");
        assert_eq!(pages, vec!["page \u{fffd}\u{fffd} 1"]);
    }
}
