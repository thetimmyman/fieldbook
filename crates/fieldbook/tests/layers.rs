//! The crate chain links in the order the architecture fixes.

#![expect(clippy::disallowed_methods, reason = "the test reads the source tree")]

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn the_chain_is_calc_core_render() {
    assert_eq!(
        [
            fieldbook_calc::LAYER,
            fieldbook_core::LAYER,
            fieldbook_render::LAYER
        ],
        ["calc", "core", "render"]
    );
}

fn strip_comments_and_strings(src: &str) -> String {
    let bytes = src.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len);
    let mut i = 0;

    while i < len {
        let b = bytes[i];
        if b == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
            out.push_str("  ");
            i += 2;
            while i < len && bytes[i] != b'\n' {
                out.push(' ');
                i += 1;
            }
            continue;
        }
        if b == b'/' && i + 1 < len && bytes[i + 1] == b'*' {
            out.push_str("  ");
            i += 2;
            let mut depth = 1usize;
            while i < len && depth > 0 {
                if bytes[i] == b'/' && i + 1 < len && bytes[i + 1] == b'*' {
                    depth += 1;
                    out.push_str("  ");
                    i += 2;
                } else if bytes[i] == b'*' && i + 1 < len && bytes[i + 1] == b'/' {
                    depth -= 1;
                    out.push_str("  ");
                    i += 2;
                } else {
                    out.push(if bytes[i] == b'\n' { '\n' } else { ' ' });
                    i += 1;
                }
            }
            continue;
        }
        let raw_start = if b == b'r' {
            Some(i + 1)
        } else if (b == b'b' || b == b'c') && i + 1 < len && bytes[i + 1] == b'r' {
            Some(i + 2)
        } else {
            None
        };
        if let Some(mut j) = raw_start {
            let mut hashes = 0usize;
            while j < len && bytes[j] == b'#' {
                hashes += 1;
                j += 1;
            }
            if j < len && bytes[j] == b'"' {
                while i <= j {
                    out.push(' ');
                    i += 1;
                }
                while i < len {
                    if bytes[i] == b'"' {
                        out.push(' ');
                        i += 1;
                        let mut matched = 0usize;
                        while matched < hashes && i + matched < len && bytes[i + matched] == b'#' {
                            matched += 1;
                        }
                        if matched == hashes {
                            for _ in 0..hashes {
                                out.push(' ');
                                i += 1;
                            }
                            break;
                        }
                    } else {
                        out.push(if bytes[i] == b'\n' { '\n' } else { ' ' });
                        i += 1;
                    }
                }
                continue;
            }
        }
        if b == b'"' {
            out.push(' ');
            i += 1;
            while i < len {
                if bytes[i] == b'\\' {
                    out.push(' ');
                    i += 1;
                    if i < len {
                        out.push(if bytes[i] == b'\n' { '\n' } else { ' ' });
                        i += 1;
                    }
                } else if bytes[i] == b'"' {
                    out.push(' ');
                    i += 1;
                    break;
                } else {
                    out.push(if bytes[i] == b'\n' { '\n' } else { ' ' });
                    i += 1;
                }
            }
            continue;
        }

        out.push(char::from(b));
        i += 1;
    }

    out
}

fn contains_ident(stripped: &str, word: &str) -> bool {
    let is_ident_char = |c: char| c.is_ascii_alphanumeric() || c == '_';
    for (idx, _) in stripped.match_indices(word) {
        let before = stripped[..idx].chars().next_back();
        let after = stripped[idx + word.len()..].chars().next();
        if !before.is_some_and(is_ident_char) && !after.is_some_and(is_ident_char) {
            return true;
        }
    }
    false
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|entry| entry.path()))
        .collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn fieldbook_win_is_named_only_in_io_win() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let allowed = src_dir.join("io/win.rs");
    let mut files = Vec::new();
    collect_rs_files(&src_dir, &mut files);

    let mut named_in = Vec::new();
    for file in files {
        let src = fs::read_to_string(&file).expect("read .rs file");
        let stripped = strip_comments_and_strings(&src);
        if contains_ident(&stripped, "fieldbook_win") {
            named_in.push(file);
        }
    }

    assert_eq!(
        named_in,
        vec![allowed],
        "only crates/fieldbook/src/io/win.rs may name `fieldbook_win`"
    );
}
