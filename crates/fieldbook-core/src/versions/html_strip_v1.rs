//! Retained implementation of the `html-strip@1` extractor (published compatibility binding).
//!
//! **Responsibility.** The frozen byte-to-text behavior of `html-strip@1`: tag stripping, block
//! and cell separation, entity decoding and whitespace cleaning. A change of these bytes is a new
//! `html-strip@2` in its own file; this file and its archived vectors stay executable.
//!
//! **Rules.** Pure: no I/O. Input original size is capped at [`MAX_ORIGINAL_BYTES`].

use crate::extraction::{ExtractError, Extraction, PageText};

/// The v1 size cap, frozen; a change is a new version.
pub const MAX_ORIGINAL_BYTES: u64 = 33_554_432;

/// Canonical extraction method name (`html-strip`).
pub const METHOD: &str = "html-strip";

/// Extract the single page of text from HTML original bytes (`html-strip@1`).
///
/// # Errors
///
/// [`ExtractError::TooLarge`] over the size cap and [`ExtractError::NotText`] for bytes that are
/// not UTF-8.
pub fn extract(bytes: &[u8]) -> Result<Extraction, ExtractError> {
    let len = u64::try_from(bytes.len()).map_err(|_| ExtractError::TooLarge { bytes: u64::MAX })?;
    if len > MAX_ORIGINAL_BYTES {
        return Err(ExtractError::TooLarge { bytes: len });
    }
    let text = std::str::from_utf8(bytes).map_err(|_| ExtractError::NotText)?;
    let stripped = strip_html(text);
    Ok(Extraction {
        method: METHOD,
        pages: vec![PageText {
            number: 1,
            text: stripped,
        }],
    })
}

fn is_block_tag(name: &str) -> bool {
    matches!(
        name,
        "p" | "div"
            | "br"
            | "li"
            | "ul"
            | "ol"
            | "tr"
            | "table"
            | "section"
            | "article"
            | "header"
            | "footer"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "title"
            | "blockquote"
            | "pre"
            | "hr"
            | "dt"
            | "dd"
            | "body"
            | "head"
            | "html"
            | "thead"
            | "tbody"
            | "tfoot"
            | "form"
            | "nav"
            | "main"
            | "aside"
            | "figure"
            | "figcaption"
    )
}

fn is_cell_tag(name: &str) -> bool {
    matches!(name, "td" | "th")
}

fn strip_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut pos = 0;

    while pos < input.len() {
        let Some(rest) = input.get(pos..) else {
            break;
        };

        if rest.starts_with('<') {
            if let Some(after_comment) = rest.strip_prefix("<!--") {
                if let Some(comment_end) = after_comment.find("-->") {
                    pos += 4 + comment_end + 3;
                } else {
                    // Unterminated comment drops everything to the end.
                    break;
                }
            } else if rest.starts_with("<!") || rest.starts_with("<?") {
                if let Some(tag_end) = rest.find('>') {
                    pos += tag_end + 1;
                } else {
                    // Unterminated construct drops everything to the end.
                    break;
                }
            } else {
                let second_byte = rest.as_bytes().get(1).copied();
                if matches!(second_byte, Some(b) if b.is_ascii_alphabetic() || b == b'/') {
                    let mut in_quote: Option<u8> = None;
                    let mut tag_end: Option<usize> = None;
                    for (idx, &byte) in rest.as_bytes().iter().enumerate().skip(1) {
                        if let Some(q) = in_quote {
                            if byte == q {
                                in_quote = None;
                            }
                        } else if byte == b'"' || byte == b'\'' {
                            in_quote = Some(byte);
                        } else if byte == b'>' {
                            tag_end = Some(idx);
                            break;
                        }
                    }

                    let Some(end_idx) = tag_end else {
                        // Unterminated tag drops everything to the end.
                        break;
                    };

                    let tag_inner = rest.get(1..end_idx).unwrap_or("");
                    let is_closing = tag_inner.starts_with('/');
                    let after_slash = if is_closing {
                        tag_inner.get(1..).unwrap_or("")
                    } else {
                        tag_inner
                    };
                    let trimmed_name = after_slash.trim_start();
                    let tag_name_len = trimmed_name
                        .find(|c: char| !c.is_ascii_alphanumeric() && c != '-')
                        .unwrap_or(trimmed_name.len());
                    let tag_name = trimmed_name
                        .get(..tag_name_len)
                        .unwrap_or("")
                        .to_ascii_lowercase();

                    if !is_closing && (tag_name == "script" || tag_name == "style") {
                        pos += end_idx + 1;
                        if tag_inner.trim_end().ends_with('/') {
                            // XML empty-element form: complete tag, opens no skip region.
                        } else {
                            let closing_name = if tag_name == "script" {
                                "script"
                            } else {
                                "style"
                            };
                            let mut found_close = false;
                            let mut search_offset = 0;
                            while let Some(current_rest) = input.get(pos + search_offset..) {
                                if let Some(rel_idx) = current_rest.find("</") {
                                    let candidate_start = search_offset + rel_idx;
                                    if let Some(after_open) = input.get(pos + candidate_start + 2..)
                                    {
                                        let name_len = closing_name.len();
                                        if after_open.len() >= name_len
                                            && after_open.get(..name_len).is_some_and(|s| {
                                                s.eq_ignore_ascii_case(closing_name)
                                            })
                                        {
                                            let rest_after =
                                                after_open.get(name_len..).unwrap_or("");
                                            let next_char = rest_after.chars().next();
                                            if matches!(
                                                next_char,
                                                None | Some('>' | '/' | ' ' | '\t' | '\r' | '\n')
                                            ) && let Some(close_gt) = rest_after.find('>')
                                            {
                                                pos +=
                                                    candidate_start + 2 + name_len + close_gt + 1;
                                                found_close = true;
                                                break;
                                            }
                                        }
                                    }
                                    search_offset = candidate_start + 2;
                                } else {
                                    break;
                                }
                            }
                            if !found_close {
                                break;
                            }
                        }
                    } else {
                        pos += end_idx + 1;
                        if is_block_tag(&tag_name) {
                            out.push('\n');
                        } else if is_cell_tag(&tag_name) {
                            out.push(' ');
                        }
                    }
                } else {
                    out.push('<');
                    pos += 1;
                }
            }
        } else if rest.starts_with('&') {
            let after_amp = rest.get(1..).unwrap_or("");
            let mut semi_pos: Option<usize> = None;
            for (idx, ch) in after_amp.char_indices() {
                if ch == ';' {
                    semi_pos = Some(idx);
                    break;
                }
                if ch.is_whitespace() || ch == '<' || ch == '&' || idx > 32 {
                    break;
                }
            }

            if let Some(semi_idx) = semi_pos {
                let entity = after_amp.get(..semi_idx).unwrap_or("");
                let full_entity_len = 1 + semi_idx + 1;
                match entity {
                    "amp" => {
                        out.push('&');
                        pos += full_entity_len;
                    }
                    "lt" => {
                        out.push('<');
                        pos += full_entity_len;
                    }
                    "gt" => {
                        out.push('>');
                        pos += full_entity_len;
                    }
                    "quot" => {
                        out.push('"');
                        pos += full_entity_len;
                    }
                    "apos" => {
                        out.push('\'');
                        pos += full_entity_len;
                    }
                    "nbsp" => {
                        out.push(' ');
                        pos += full_entity_len;
                    }
                    _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                        let hex_str = entity.get(2..).unwrap_or("");
                        if let Ok(val) = u32::from_str_radix(hex_str, 16) {
                            if let Some(ch) = char::from_u32(val) {
                                if ch != '\0' {
                                    out.push(ch);
                                } else {
                                    out.push_str(rest.get(..full_entity_len).unwrap_or(""));
                                }
                            } else {
                                out.push_str(rest.get(..full_entity_len).unwrap_or(""));
                            }
                        } else {
                            out.push_str(rest.get(..full_entity_len).unwrap_or(""));
                        }
                        pos += full_entity_len;
                    }
                    _ if entity.starts_with('#') => {
                        let dec_str = entity.get(1..).unwrap_or("");
                        if let Ok(val) = dec_str.parse::<u32>() {
                            if let Some(ch) = char::from_u32(val) {
                                if ch != '\0' {
                                    out.push(ch);
                                } else {
                                    out.push_str(rest.get(..full_entity_len).unwrap_or(""));
                                }
                            } else {
                                out.push_str(rest.get(..full_entity_len).unwrap_or(""));
                            }
                        } else {
                            out.push_str(rest.get(..full_entity_len).unwrap_or(""));
                        }
                        pos += full_entity_len;
                    }
                    _ => {
                        out.push_str(rest.get(..full_entity_len).unwrap_or(""));
                        pos += full_entity_len;
                    }
                }
            } else {
                out.push('&');
                pos += 1;
            }
        } else {
            let ch = rest.chars().next().unwrap_or(' ');
            out.push(ch);
            pos += ch.len_utf8();
        }
    }

    clean_html_text(&out)
}

fn clean_html_text(raw: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in raw.split('\n') {
        let mut cleaned_line = String::with_capacity(line.len());
        let mut in_ws = false;
        for ch in line.chars() {
            if ch == ' ' || ch == '\t' || ch == '\r' {
                if !in_ws {
                    cleaned_line.push(' ');
                    in_ws = true;
                }
            } else {
                cleaned_line.push(ch);
                in_ws = false;
            }
        }
        lines.push(cleaned_line.trim().to_owned());
    }

    let Some(start) = lines.iter().position(|l| !l.is_empty()) else {
        return String::new();
    };
    let end = lines.iter().rposition(|l| !l.is_empty()).unwrap_or(start);
    let slice = &lines[start..=end];

    let mut result_lines: Vec<&str> = Vec::new();
    let mut last_was_empty = false;
    for l in slice {
        if l.is_empty() {
            if !last_was_empty {
                result_lines.push("");
                last_was_empty = true;
            }
        } else {
            result_lines.push(l.as_str());
            last_was_empty = false;
        }
    }

    if result_lines.is_empty() {
        String::new()
    } else {
        let mut out = result_lines.join("\n");
        out.push('\n');
        out
    }
}
