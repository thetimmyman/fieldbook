//! Lint-table parity and `unsafe` confinement checks for `fieldbook-win` and the workspace.

#![expect(clippy::disallowed_methods, reason = "the test reads the source tree")]

use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn extract_section_lines<'a>(manifest: &'a str, header: &str) -> Vec<&'a str> {
    let mut in_section = false;
    let mut lines = Vec::new();
    for raw in manifest.lines() {
        let trimmed = raw.trim();
        if trimmed.starts_with('[') {
            if trimmed == header {
                in_section = true;
                continue;
            } else if in_section {
                break;
            }
        }
        if in_section && !trimmed.is_empty() && !trimmed.starts_with('#') {
            lines.push(trimmed);
        }
    }
    lines
}

#[test]
fn lints_match_workspace_except_unsafe() {
    let root = workspace_root();
    let root_toml = fs::read_to_string(root.join("Cargo.toml")).expect("read root Cargo.toml");
    let win_toml = fs::read_to_string(root.join("crates/fieldbook-win/Cargo.toml"))
        .expect("read fieldbook-win Cargo.toml");

    let ws_rust = extract_section_lines(&root_toml, "[workspace.lints.rust]");
    let win_rust = extract_section_lines(&win_toml, "[lints.rust]");
    assert!(!ws_rust.is_empty(), "workspace.lints.rust is non-empty");
    let expected_win_rust: Vec<String> = ws_rust
        .iter()
        .map(|line| {
            if *line == r#"unsafe_code = "forbid""# {
                r#"unsafe_code = "deny""#.to_owned()
            } else {
                (*line).to_owned()
            }
        })
        .collect();
    assert_eq!(
        win_rust, expected_win_rust,
        "[lints.rust] in fieldbook-win must match [workspace.lints.rust] except for unsafe_code = \"deny\""
    );

    let ws_clippy = extract_section_lines(&root_toml, "[workspace.lints.clippy]");
    let win_clippy = extract_section_lines(&win_toml, "[lints.clippy]");
    assert!(!ws_clippy.is_empty(), "workspace.lints.clippy is non-empty");
    assert_eq!(
        win_clippy, ws_clippy,
        "[lints.clippy] in fieldbook-win must match [workspace.lints.clippy] line by line"
    );
}

/// Strip comments (`//` and nested `/* ... */`), normal string literals (`"..."`),
/// raw string literals (`r#"..."#` / `br#"..."#` / `cr#"..."#`) and character literals,
/// replacing stripped regions with spaces (preserving newlines).
fn strip_comments_and_strings(src: &str) -> String {
    let bytes = src.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len);
    let mut i = 0;

    while i < len {
        let b = bytes[i];
        // Line comment
        if b == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
            out.push_str("  ");
            i += 2;
            while i < len && bytes[i] != b'\n' {
                out.push(' ');
                i += 1;
            }
            continue;
        }
        // Nested block comment
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
        // Raw string literal: `r"..."`, `r#"..."#`, `br"..."`, `cr"..."`
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
        // Normal string literal
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
        // Character literal (distinguished from lifetime `'a`)
        if b == b'\'' {
            if i + 2 < len && bytes[i + 1] != b'\\' && bytes[i + 2] == b'\'' {
                out.push_str("   ");
                i += 3;
                continue;
            }
            if i + 3 < len && bytes[i + 1] == b'\\' {
                out.push_str("  ");
                i += 2;
                while i < len && bytes[i] != b'\'' && bytes[i] != b'\n' {
                    out.push(' ');
                    i += 1;
                }
                if i < len && bytes[i] == b'\'' {
                    out.push(' ');
                    i += 1;
                }
                continue;
            }
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

fn has_unsafe_token(src: &str) -> bool {
    let stripped = strip_comments_and_strings(src);
    contains_ident(&stripped, "unsafe")
}

#[test]
fn unsafe_scan_ignores_comments_and_strings_and_finds_a_block() {
    let harmless = r##"
        // unsafe in a line comment
        /* unsafe in a /* nested unsafe */ block comment */
        let a = "unsafe in a string literal";
        let b = r#"unsafe in a raw string"#;
        let unsafe_variable_name = 1;
    "##;
    assert!(
        !has_unsafe_token(harmless),
        "comments, strings, raw strings and longer identifiers must not count as unsafe"
    );

    let real_block = r"
        fn demo() {
            // SAFETY: demo
            unsafe { core::hint::unreachable_unchecked() }
        }
    ";
    assert!(
        has_unsafe_token(real_block),
        "a real unsafe block must be detected"
    );
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
            if path.file_name().and_then(|n| n.to_str()) == Some("fixtures") {
                continue;
            }
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

// control: CD-01
#[test]
fn unsafe_only_in_sys_rs() {
    let root = workspace_root();
    let allowed = root.join("crates/fieldbook-win/src/sys.rs");
    let mut files = Vec::new();

    let crates_dir = root.join("crates");
    let mut crate_entries: Vec<PathBuf> = fs::read_dir(&crates_dir)
        .expect("read crates/")
        .filter_map(|e| e.ok().map(|entry| entry.path()))
        .collect();
    crate_entries.sort();
    for krate in crate_entries {
        if krate.is_dir() {
            collect_rs_files(&krate.join("src"), &mut files);
            collect_rs_files(&krate.join("tests"), &mut files);
        }
    }
    collect_rs_files(&root.join("xtask/src"), &mut files);
    collect_rs_files(&root.join("xtask/tests"), &mut files);

    let mut unsafe_files = Vec::new();
    let mut expect_unsafe_files = Vec::new();
    for file in &files {
        let src = fs::read_to_string(file).expect("read .rs file");
        let stripped = strip_comments_and_strings(&src);
        if contains_ident(&stripped, "unsafe") {
            unsafe_files.push(file.clone());
        }
        if stripped.contains("#![expect(unsafe_code") {
            expect_unsafe_files.push(file.clone());
        }
    }

    assert_eq!(
        unsafe_files,
        vec![allowed.clone()],
        "the word `unsafe` may appear only in crates/fieldbook-win/src/sys.rs"
    );
    assert_eq!(
        expect_unsafe_files,
        vec![allowed],
        "`#![expect(unsafe_code` may appear only in crates/fieldbook-win/src/sys.rs"
    );
}

#[test]
fn every_unsafe_block_has_a_safety_comment() {
    let root = workspace_root();
    let sys_path = root.join("crates/fieldbook-win/src/sys.rs");
    let src = fs::read_to_string(&sys_path).expect("read crates/fieldbook-win/src/sys.rs");
    let stripped = strip_comments_and_strings(&src);

    let raw_lines: Vec<&str> = src.lines().collect();
    let stripped_lines: Vec<&str> = stripped.lines().collect();
    assert_eq!(raw_lines.len(), stripped_lines.len());

    let mut unsafe_count = 0usize;
    for (idx, sline) in stripped_lines.iter().enumerate() {
        if !contains_ident(sline, "unsafe") {
            continue;
        }
        unsafe_count += 1;
        // Look backwards up to 4 lines for a `// SAFETY:` comment above this statement.
        let start = idx.saturating_sub(4);
        let has_safety = raw_lines[start..=idx]
            .iter()
            .any(|line| line.trim_start().starts_with("// SAFETY:"));
        assert!(
            has_safety,
            "crates/fieldbook-win/src/sys.rs:{} has an `unsafe` block without a preceding `// SAFETY:` comment",
            idx + 1
        );
    }
    assert!(
        unsafe_count > 0,
        "sys.rs must contain at least one unsafe block"
    );
}
