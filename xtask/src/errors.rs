//! `errors`: parse the Check catalogue of `docs/reference/corpus-format.md` and render
//! `docs/reference/errors.md` (`check-v0`, AD-22).

use std::fmt::Write as _;

use fieldbook_core::check::Code;

use crate::proc::repo_root;

/// One row parsed from a Check catalogue table (`### 01:` through `### 09:`) in `corpus-format.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogueRow {
    /// Two-digit range prefix (`"01"` through `"09"`).
    pub range: String,
    /// Full section heading title after `### `, such as `"01: pages, \`corpus.toml\` and files"`.
    pub range_title: String,
    /// The 6-character code (`"FB0100"`).
    pub code: String,
    /// The `Severity` cell text.
    pub severity: String,
    /// The `Check` (or `Raised when`) cell text.
    pub check: String,
    /// The `Known-bad example` cell text from the reference table.
    pub example: String,
}

/// Split a Markdown table row `| a | b | c | d |` into trimmed cells, preserving `\|` inside backticks.
fn split_table_cells(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    let inner = trimmed
        .strip_prefix('|')
        .unwrap_or(trimmed)
        .strip_suffix('|')
        .unwrap_or(trimmed);

    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut in_backticks = false;
    let mut prev_backslash = false;

    for ch in inner.chars() {
        if ch == '`' {
            in_backticks = !in_backticks;
            cur.push(ch);
            prev_backslash = false;
        } else if ch == '|' && !in_backticks && !prev_backslash {
            cells.push(cur.trim().to_owned());
            cur.clear();
            prev_backslash = false;
        } else {
            prev_backslash = ch == '\\' && !prev_backslash;
            cur.push(ch);
        }
    }
    cells.push(cur.trim().to_owned());
    cells
}

fn is_separator_row(cells: &[String]) -> bool {
    !cells.is_empty()
        && cells
            .iter()
            .all(|c| !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':' || ch == ' '))
}

fn is_fb_code(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.len() == 6
        && bytes[0] == b'F'
        && bytes[1] == b'B'
        && bytes[2].is_ascii_digit()
        && bytes[3].is_ascii_digit()
        && bytes[4].is_ascii_digit()
        && bytes[5].is_ascii_digit()
}

/// Expand a first-cell token such as `"FB0100"` or `"FB0800 to FB0805"` into one or more `"FBnnnn"` codes.
fn expand_first_cell_codes(cell: &str) -> Option<Vec<String>> {
    let clean = cell.trim().trim_matches('`');
    if is_fb_code(clean) {
        return Some(vec![clean.to_owned()]);
    }
    if let Some((left, right)) = clean.split_once(" to ") {
        let l = left.trim().trim_matches('`');
        let r = right.trim().trim_matches('`');
        if is_fb_code(l)
            && is_fb_code(r)
            && let (Ok(start), Ok(end)) = (l[2..].parse::<u16>(), r[2..].parse::<u16>())
            && start <= end
        {
            let mut out = Vec::new();
            for n in start..=end {
                let candidate = format!("FB{n:04}");
                if Code::parse(&candidate).is_ok() {
                    out.push(candidate);
                }
            }
            if !out.is_empty() {
                return Some(out);
            }
        }
    }
    None
}

/// Parse the Check catalogue tables (`### 01:` to `### 09:`) out of `corpus-format.md`.
///
/// # Errors
///
/// Returns an error if a table row under `01:`..`09:` is malformed or if a code appears outside
/// a range table or repeats.
pub fn parse_catalogue(markdown: &str) -> Result<Vec<CatalogueRow>, String> {
    let mut rows = Vec::new();
    let mut in_catalogue = false;
    let mut current_range: Option<(String, String)> = None;

    for (idx, raw_line) in markdown.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw_line.trim_end_matches('\r');

        if let Some(h2) = line.strip_prefix("## ") {
            in_catalogue = h2.trim() == "Check catalogue";
            current_range = None;
            continue;
        }
        if !in_catalogue {
            continue;
        }

        if let Some(h3) = line.strip_prefix("### ") {
            let title = h3.trim();
            if title.len() >= 3
                && title.as_bytes()[0] == b'0'
                && (b'1'..=b'9').contains(&title.as_bytes()[1])
                && title.as_bytes()[2] == b':'
            {
                let prefix = title[..2].to_owned();
                current_range = Some((prefix, title.to_owned()));
            } else {
                current_range = None;
            }
            continue;
        }

        if !line.starts_with('|') {
            continue;
        }

        let cells = split_table_cells(line);
        if is_separator_row(&cells) {
            continue;
        }
        if cells.first().is_some_and(|c| c == "Code") {
            continue;
        }

        let Some((range, range_title)) = &current_range else {
            continue;
        };

        let Some(first) = cells.first() else {
            continue;
        };
        let Some(codes) = expand_first_cell_codes(first) else {
            return Err(format!(
                "corpus-format.md:{line_no}: unexpected row in range {range}: `{line}`"
            ));
        };
        if cells.len() != 4 {
            return Err(format!(
                "corpus-format.md:{line_no}: expected 4 columns in range {range}, got {}",
                cells.len()
            ));
        }

        for code in codes {
            rows.push(CatalogueRow {
                range: range.clone(),
                range_title: range_title.clone(),
                code,
                severity: cells[1].clone(),
                check: cells[2].clone(),
                example: cells[3].clone(),
            });
        }
    }

    if rows.is_empty() {
        return Err("no Check catalogue rows found in docs/reference/corpus-format.md".to_owned());
    }
    Ok(rows)
}

/// Render the generated body between `<!-- generated:errors -->` and `<!-- /generated:errors -->`
/// in `docs/reference/errors.md`.
///
/// # Errors
///
/// Returns an error if `docs/reference/corpus-format.md` cannot be read or parsed, or if a catalogue
/// code is missing from [`Code`].
pub fn render_errors_section(_current_body: &str) -> Result<String, String> {
    let path = repo_root().join("docs/reference/corpus-format.md");
    let text = std::fs::read_to_string(&path)
        .map_err(|err| format!("cannot read {}: {err}", path.display()))?;
    render_from_catalogue(&text)
}

/// Render the `errors.md` body from the text of `corpus-format.md`.
///
/// # Errors
///
/// Returns an error if the catalogue tables cannot be parsed or do not match [`Code`].
pub fn render_from_catalogue(corpus_format_md: &str) -> Result<String, String> {
    let rows = parse_catalogue(corpus_format_md)?;

    let mut out = String::new();
    out.push_str(
        "Generated from the [Check catalogue](corpus-format.md#check-catalogue) and `fieldbook_core::check::Code` by `cargo xtask docs`. Do not edit by hand.\n\n",
    );
    out.push_str("## Reading a diagnostic\n\n");
    out.push_str(
        "Every diagnostic is printed on two lines with no indent ([AD-17](../decisions/0001-foundation.md#ad-17-authoring-experience)):\n\n",
    );
    out.push_str("```text\n");
    out.push_str("path:line:col: error[FB0102]: message\n");
    out.push_str("help: what to do about it\n");
    out.push_str("```\n\n");
    out.push_str(
        "- `path` is relative to the repository root (`/`-separated).\n\
         - `line` and `col` are 1-based (`1:1` for a finding about a whole file), and `col` counts Unicode code points.\n\
         - `severity` is `error` or `warning`. Codes whose catalogue severity is `strict` (`FB0118`, `FB0230`, `FB0320`) are reported as `warning` by default and as `error` under `fieldbook check --strict`.\n\
         - Diagnostics are sorted by `(path, line, col, code)` with one blank line between findings, followed by any `note: text` lines and the summary line `N errors, M warnings` (`clean` when both are zero).\n\n",
    );
    out.push_str("## Exit codes\n\n");
    out.push_str(
        "| Code | Meaning |\n\
         |---|---|\n\
         | `0` | Clean (zero errors; warnings do not change the exit code) |\n\
         | `1` | Findings (at least one error) |\n\
         | `2` | Usage error (invalid command-line arguments or configuration refusal) |\n\
         | `3` | I/O or operational failure |\n",
    );

    let mut current_range = String::new();
    for row in &rows {
        let code = Code::parse(&row.code)
            .map_err(|_| format!("catalogue code `{}` is absent from `Code`", row.code))?;

        if row.range != current_range {
            current_range.clone_from(&row.range);
            let _ = write!(
                out,
                "\n## {}\n\n| Code | Severity | Check | Known-bad example | Help |\n|---|---|---|---|---|\n",
                row.range_title
            );
        }

        let known_bad = if row.range == "08" || row.range == "09" {
            format!("`tests/known-bad/{}.toml`", row.code)
        } else {
            row.example.clone()
        };

        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} |",
            row.code,
            row.severity,
            row.check,
            known_bad,
            code.help()
        );
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use fieldbook_core::check::Code;

    use super::{parse_catalogue, render_from_catalogue};
    use crate::proc::repo_root;

    #[test]
    fn catalogue_matches_the_code() {
        let path = repo_root().join("docs/reference/corpus-format.md");
        let text = std::fs::read_to_string(&path).expect("read corpus-format.md");
        let rows = parse_catalogue(&text).expect("parse catalogue tables");

        let mut seen = BTreeSet::new();
        for row in &rows {
            assert!(
                seen.insert(row.code.as_str()),
                "duplicate code in corpus-format.md catalogue: {}",
                row.code
            );
        }

        let code_ids: Vec<&str> = Code::ALL.iter().map(|c| c.id()).collect();
        let row_ids: Vec<&str> = rows.iter().map(|r| r.code.as_str()).collect();

        for &id in &code_ids {
            assert!(
                seen.contains(id),
                "Code variant `{id}` has no row in docs/reference/corpus-format.md"
            );
        }
        for &id in &row_ids {
            assert!(
                Code::parse(id).is_ok(),
                "catalogue row `{id}` has no variant in fieldbook_core::check::Code"
            );
        }
        assert_eq!(
            row_ids, code_ids,
            "catalogue code order and Code::ALL order must match"
        );

        for (row, &code) in rows.iter().zip(Code::ALL.iter()) {
            assert_eq!(
                code.severity_text(),
                row.severity,
                "severity mismatch on {}: Code has `{}`, catalogue has `{}`",
                code.id(),
                code.severity_text(),
                row.severity
            );
        }

        let rendered = render_from_catalogue(&text).expect("render errors.md");
        assert!(rendered.contains("tests/known-bad/FB0800.toml"));
        assert!(rendered.contains("tests/known-bad/FB0955.toml"));
    }
}
