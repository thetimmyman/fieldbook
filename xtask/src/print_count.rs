//! Page count inspector for brief PDFs (`cargo xtask print-count <pdf>`).
//!
//! Scans a PDF without external dependencies to find the page count from its page-tree
//! root dictionary. Reports FB0607 when a printed brief exceeds one page.

use std::path::Path;

/// Maximum reasonable page count before flagging as absurd/unreadable.
const MAX_PAGES: u64 = 1_000_000;

/// Returns true if byte is a standard PDF whitespace character.
fn is_pdf_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b'\x0C' | 0)
}

/// Returns true if byte is a standard PDF delimiter or whitespace.
fn is_pdf_delimiter_or_whitespace(b: u8) -> bool {
    is_pdf_whitespace(b)
        || matches!(
            b,
            b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
        )
}

/// Advances `pos` through whitespace and PDF comments.
fn skip_whitespace_and_comments(bytes: &[u8], mut pos: usize) -> usize {
    while pos < bytes.len() {
        let Some(&b) = bytes.get(pos) else { break };
        if is_pdf_whitespace(b) {
            pos = pos.saturating_add(1);
        } else if b == b'%' {
            pos = pos.saturating_add(1);
            while pos < bytes.len() {
                let Some(&cb) = bytes.get(pos) else { break };
                pos = pos.saturating_add(1);
                if cb == b'\r' || cb == b'\n' {
                    break;
                }
            }
        } else {
            break;
        }
    }
    pos
}

/// Parses an unsigned decimal integer starting at `pos`.
fn parse_u64(bytes: &[u8], mut pos: usize) -> Option<(u64, usize)> {
    let mut val: u64 = 0;
    let mut digits: usize = 0;
    while pos < bytes.len() {
        let Some(&b) = bytes.get(pos) else { break };
        if b.is_ascii_digit() {
            let digit = u64::from(b - b'0');
            val = val.checked_mul(10)?.checked_add(digit)?;
            digits = digits.saturating_add(1);
            pos = pos.saturating_add(1);
        } else {
            break;
        }
    }
    if digits == 0 { None } else { Some((val, pos)) }
}

/// Parses a slice of ASCII digits into a `u64`.
fn parse_u64_exact(slice: &[u8]) -> Option<u64> {
    if slice.is_empty() {
        return None;
    }
    let mut val: u64 = 0;
    for &b in slice {
        if !b.is_ascii_digit() {
            return None;
        }
        val = val.checked_mul(10)?.checked_add(u64::from(b - b'0'))?;
    }
    Some(val)
}

/// Parses an indirect object reference `<n> <g> R` starting at `pos`.
fn parse_indirect_ref(bytes: &[u8], pos: usize) -> Option<((u64, u64), usize)> {
    let pos = skip_whitespace_and_comments(bytes, pos);
    let (n, pos) = parse_u64(bytes, pos)?;
    let pos_after_space = skip_whitespace_and_comments(bytes, pos);
    if pos_after_space == pos {
        return None;
    }
    let (g, pos) = parse_u64(bytes, pos_after_space)?;
    let pos_after_space2 = skip_whitespace_and_comments(bytes, pos);
    if pos_after_space2 == pos {
        return None;
    }
    let r_byte = bytes.get(pos_after_space2)?;
    if *r_byte != b'R' {
        return None;
    }
    let pos = pos_after_space2.saturating_add(1);
    if bytes
        .get(pos)
        .is_some_and(|&next_b| !is_pdf_delimiter_or_whitespace(next_b))
    {
        return None;
    }
    Some(((n, g), pos))
}

/// Finds the last occurrence of `/Root <n> <g> R` in `bytes`.
fn find_last_root(bytes: &[u8]) -> Option<(u64, u64)> {
    let needle = b"/Root";
    let mut last_ref = None;
    let mut search_from = 0;
    while search_from < bytes.len() {
        let Some(pos) = bytes
            .get(search_from..)
            .and_then(|slice| slice.windows(needle.len()).position(|w| w == needle))
        else {
            break;
        };
        let root_pos = search_from.saturating_add(pos);
        let after_root = root_pos.saturating_add(needle.len());
        if bytes
            .get(after_root)
            .is_some_and(|&next_b| is_pdf_delimiter_or_whitespace(next_b))
            && let Some((target, _)) = parse_indirect_ref(bytes, after_root)
        {
            last_ref = Some(target);
        }
        search_from = root_pos.saturating_add(1);
    }
    last_ref
}

/// Checks whether `obj` at `idx` is preceded by `<target_n> <target_g>` at a line start or after whitespace.
/// Returns the offset immediately after `obj` on success.
fn match_obj_header(bytes: &[u8], idx: usize, target_n: u64, target_g: u64) -> Option<usize> {
    if bytes
        .get(idx.saturating_add(3))
        .is_some_and(|&after_b| !is_pdf_delimiter_or_whitespace(after_b))
    {
        return None;
    }

    let mut p = idx;
    let mut skipped_ws = false;
    while p > 0 {
        let prev_b = *bytes.get(p.saturating_sub(1))?;
        if is_pdf_whitespace(prev_b) {
            skipped_ws = true;
            p = p.saturating_sub(1);
        } else {
            break;
        }
    }
    if !skipped_ws {
        return None;
    }

    let end_g = p;
    while p > 0 {
        let prev_b = *bytes.get(p.saturating_sub(1))?;
        if prev_b.is_ascii_digit() {
            p = p.saturating_sub(1);
        } else {
            break;
        }
    }
    let start_g = p;
    if start_g == end_g {
        return None;
    }
    let g = parse_u64_exact(bytes.get(start_g..end_g)?)?;
    if g != target_g {
        return None;
    }

    let mut skipped_ws2 = false;
    while p > 0 {
        let prev_b = *bytes.get(p.saturating_sub(1))?;
        if is_pdf_whitespace(prev_b) {
            skipped_ws2 = true;
            p = p.saturating_sub(1);
        } else {
            break;
        }
    }
    if !skipped_ws2 {
        return None;
    }

    let end_n = p;
    while p > 0 {
        let prev_b = *bytes.get(p.saturating_sub(1))?;
        if prev_b.is_ascii_digit() {
            p = p.saturating_sub(1);
        } else {
            break;
        }
    }
    let start_n = p;
    if start_n == end_n {
        return None;
    }
    let n = parse_u64_exact(bytes.get(start_n..end_n)?)?;
    if n != target_n {
        return None;
    }

    if start_n > 0 {
        let prev_b = *bytes.get(start_n.saturating_sub(1))?;
        if !is_pdf_whitespace(prev_b) {
            return None;
        }
    }

    Some(idx.saturating_add(3))
}

/// Finds the last occurrence of `<target_n> <target_g> obj` and returns slice range of its dictionary content.
fn find_last_object(bytes: &[u8], target_n: u64, target_g: u64) -> Option<(usize, usize)> {
    let needle = b"obj";
    let mut last = None;
    let mut search_from = 0;
    while search_from < bytes.len() {
        let Some(pos) = bytes
            .get(search_from..)
            .and_then(|slice| slice.windows(needle.len()).position(|w| w == needle))
        else {
            break;
        };
        let obj_pos = search_from.saturating_add(pos);
        if let Some(content_start) = match_obj_header(bytes, obj_pos, target_n, target_g) {
            let end_needle = b"endobj";
            let content_end = bytes
                .get(content_start..)
                .and_then(|slice| {
                    slice
                        .windows(end_needle.len())
                        .position(|w| w == end_needle)
                })
                .map_or(bytes.len(), |p| content_start.saturating_add(p));
            let stream_needle = b"stream";
            let dict_end = bytes
                .get(content_start..content_end)
                .and_then(|slice| {
                    slice
                        .windows(stream_needle.len())
                        .position(|w| w == stream_needle)
                })
                .map_or(content_end, |p| content_start.saturating_add(p));
            last = Some((content_start, dict_end));
        }
        search_from = obj_pos.saturating_add(1);
    }
    last
}

/// Returns true if the PDF references compressed object streams (`/ObjStm`).
fn pdf_has_objstm(bytes: &[u8]) -> bool {
    bytes.windows(7).any(|w| w == b"/ObjStm")
}

/// Reads `/Pages <m> <g> R` from a catalog dictionary.
fn find_pages_ref_in_dict(dict_bytes: &[u8]) -> Option<(u64, u64)> {
    let needle = b"/Pages";
    let mut last = None;
    let mut search_from = 0;
    while search_from < dict_bytes.len() {
        let Some(pos) = dict_bytes
            .get(search_from..)
            .and_then(|slice| slice.windows(needle.len()).position(|w| w == needle))
        else {
            break;
        };
        let pages_pos = search_from.saturating_add(pos);
        let after_pages = pages_pos.saturating_add(needle.len());
        if dict_bytes
            .get(after_pages)
            .is_some_and(|&next_b| is_pdf_delimiter_or_whitespace(next_b))
            && let Some((target, _)) = parse_indirect_ref(dict_bytes, after_pages)
        {
            last = Some(target);
        }
        search_from = pages_pos.saturating_add(1);
    }
    last
}

/// Reads the integer after `/Count` in a pages dictionary.
fn find_count_in_dict(dict_bytes: &[u8]) -> Result<u64, String> {
    let needle = b"/Count";
    let mut last_count = None;
    let mut search_from = 0;
    while search_from < dict_bytes.len() {
        let Some(pos) = dict_bytes
            .get(search_from..)
            .and_then(|slice| slice.windows(needle.len()).position(|w| w == needle))
        else {
            break;
        };
        let count_pos = search_from.saturating_add(pos);
        let after_count = count_pos.saturating_add(needle.len());
        if dict_bytes
            .get(after_count)
            .is_some_and(|&next_b| is_pdf_delimiter_or_whitespace(next_b))
        {
            let val_pos = skip_whitespace_and_comments(dict_bytes, after_count);
            if val_pos > after_count {
                if dict_bytes.get(val_pos) == Some(&b'-') {
                    return Err("negative /Count value".to_owned());
                }
                if let Some((n, end_pos)) = parse_u64(dict_bytes, val_pos) {
                    let valid_end = match dict_bytes.get(end_pos) {
                        None => true,
                        Some(&b) => is_pdf_delimiter_or_whitespace(b),
                    };
                    if valid_end {
                        last_count = Some(n);
                    } else {
                        return Err("invalid /Count value".to_owned());
                    }
                } else {
                    return Err("non-integer /Count value".to_owned());
                }
            }
        }
        search_from = count_pos.saturating_add(1);
    }
    match last_count {
        Some(c) => Ok(c),
        None => Err("missing /Count in pages dictionary".to_owned()),
    }
}

/// Scans a PDF file to read its page count and enforce the one-page printed brief limit.
pub fn command(path: &Path) -> Result<bool, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;

    if !bytes.starts_with(b"%PDF-") {
        return Err(format!("{}: not a PDF file", path.display()));
    }

    let (root_n, root_g) = find_last_root(&bytes)
        .ok_or_else(|| format!("{}: missing /Root in trailer", path.display()))?;

    let (root_start, root_end) = find_last_object(&bytes, root_n, root_g).ok_or_else(|| {
        if pdf_has_objstm(&bytes) {
            format!(
                "{}: object {root_n} {root_g} lives in a compressed object stream; compressed object streams are not supported",
                path.display()
            )
        } else {
            format!("{}: cannot find object {root_n} {root_g} obj", path.display())
        }
    })?;

    let root_dict = bytes
        .get(root_start..root_end)
        .ok_or_else(|| format!("{}: unreadable root object dictionary", path.display()))?;

    let (pages_n, pages_g) = find_pages_ref_in_dict(root_dict)
        .ok_or_else(|| format!("{}: missing /Pages in catalog dictionary", path.display()))?;

    let (pages_start, pages_end) = find_last_object(&bytes, pages_n, pages_g).ok_or_else(|| {
        if pdf_has_objstm(&bytes) {
            format!(
                "{}: object {pages_n} {pages_g} lives in a compressed object stream; compressed object streams are not supported",
                path.display()
            )
        } else {
            format!("{}: cannot find object {pages_n} {pages_g} obj", path.display())
        }
    })?;

    let pages_dict = bytes
        .get(pages_start..pages_end)
        .ok_or_else(|| format!("{}: unreadable pages object dictionary", path.display()))?;

    let count = find_count_in_dict(pages_dict).map_err(|e| format!("{}: {e}", path.display()))?;

    if count == 0 {
        return Err(format!("{}: PDF has 0 pages", path.display()));
    }

    if count > MAX_PAGES {
        return Err(format!(
            "{}: page count exceeds limit: {count}",
            path.display()
        ));
    }

    println!("pages: {count}");

    if count > 1 {
        let sev = if cfg!(windows) { "error" } else { "warning" };
        println!(
            "{}: {sev}[FB0607]: a brief prints on {count} pages; it must print on one",
            path.display()
        );
        return Ok(false);
    }

    Ok(true)
}
