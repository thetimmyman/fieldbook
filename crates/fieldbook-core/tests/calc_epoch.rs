//! Epoch and prefix checks for `fieldbook_core::cases::CASES` (`AD-14`, `corpus-format.md#identifiers`).

use std::collections::BTreeMap;

use fieldbook_calc::CALC_EPOCH;
use fieldbook_core::cases::{CASES, case_set_sha256};
use fieldbook_core::ids::FAMILIES;

const EPOCH_FILE: &str = include_str!("../../fieldbook-calc/tests/cases/EPOCH");
const CORPUS_FORMAT: &str = include_str!("../../../docs/reference/corpus-format.md");

fn parse_case_prefix_table(doc: &str) -> Vec<(String, String)> {
    let marker = "**Calculation case prefixes.**";
    let Some(start) = doc.find(marker) else {
        panic!("docs/reference/corpus-format.md must contain `{marker}`");
    };
    let section = &doc[start..];
    let mut lines = section.lines().peekable();
    while let Some(line) = lines.next() {
        if line
            .trim_start()
            .starts_with("| Prefix | Cases for | State |")
        {
            let Some(sep) = lines.next() else {
                panic!("Calculation case prefixes table header must be followed by separator");
            };
            assert!(
                sep.trim_start().starts_with("|---"),
                "expected separator row in Calculation case prefixes table"
            );
            let mut out = Vec::new();
            while let Some(&row_line) = lines.peek() {
                let trimmed = row_line.trim();
                if !trimmed.starts_with('|') {
                    break;
                }
                let _ = lines.next();
                let inner = trimmed
                    .strip_prefix('|')
                    .unwrap_or(trimmed)
                    .strip_suffix('|')
                    .unwrap_or(trimmed);
                let cells: Vec<&str> = inner.split('|').map(str::trim).collect();
                assert_eq!(
                    cells.len(),
                    3,
                    "Calculation case prefixes row must have 3 columns: {row_line:?}"
                );
                let state = cells[2].to_owned();
                let mut rest = cells[0];
                while let Some(open) = rest.find('`') {
                    let after_open = &rest[open + 1..];
                    let Some(close) = after_open.find('`') else {
                        panic!("unbalanced backtick in Calculation case prefixes cell");
                    };
                    out.push((after_open[..close].to_owned(), state.clone()));
                    rest = &after_open[close + 1..];
                }
            }
            return out;
        }
    }
    panic!("Calculation case prefixes table not found in docs/reference/corpus-format.md");
}

fn check_case_file_prefix(
    file_name: &str,
    prefix_states: &BTreeMap<String, String>,
) -> Result<(), String> {
    let Some(stem) = file_name.strip_suffix(".toml") else {
        return Err(format!("{file_name}: case file must end with .toml"));
    };
    let Some((raw_prefix, _rest)) = stem.split_once('-') else {
        return Err(format!("{file_name}: case file has no prefix before `-`"));
    };
    if raw_prefix.is_empty() || !raw_prefix.bytes().all(|b| b.is_ascii_lowercase()) {
        return Err(format!(
            "{file_name}: case file prefix `{raw_prefix}` must be lowercase ASCII letters"
        ));
    }
    let upper = raw_prefix.to_ascii_uppercase();
    match prefix_states.get(&upper) {
        Some(state) if state == "Now" => Ok(()),
        Some(state) => Err(format!(
            "{file_name}: prefix `{upper}` has state `{state}` (must be `Now`)"
        )),
        None => Err(format!(
            "{file_name}: prefix `{upper}` is not listed in the Calculation case prefixes table"
        )),
    }
}

#[test]
fn calc_epoch_matches_the_case_set() {
    let full_sha = case_set_sha256();
    let short_sha = &full_sha[..16];
    let expected = format!("{CALC_EPOCH} {short_sha}");
    let actual = EPOCH_FILE.trim_end_matches(['\r', '\n']);
    assert_eq!(
        actual, expected,
        "EPOCH mismatch: expected `{expected}` (full sha256: {full_sha}), found `{actual}`"
    );
    assert!(
        EPOCH_FILE.ends_with('\n') && !EPOCH_FILE[..EPOCH_FILE.len() - 1].contains('\n'),
        "EPOCH must be a single line ending with LF"
    );
}

#[test]
fn case_prefixes_match_the_reference() {
    let rows = parse_case_prefix_table(CORPUS_FORMAT);
    let doc_prefixes: Vec<&str> = rows.iter().map(|(p, _)| p.as_str()).collect();
    let case_family = FAMILIES
        .iter()
        .find(|f| f.name == "case")
        .unwrap_or_else(|| panic!("case family in FAMILIES"));
    assert_eq!(
        doc_prefixes.as_slice(),
        case_family.prefixes,
        "Calculation case prefixes table in corpus-format.md differs from FAMILIES case prefixes"
    );
}

#[test]
fn case_files_use_a_current_prefix() {
    let rows = parse_case_prefix_table(CORPUS_FORMAT);
    let states: BTreeMap<String, String> = rows.into_iter().collect();

    let mut failures = Vec::new();
    for &(file_name, _body) in CASES {
        if let Err(err) = check_case_file_prefix(file_name, &states) {
            failures.push(err);
        }
    }
    assert!(
        failures.is_empty(),
        "case_files_use_a_current_prefix failed:\n  {}",
        failures.join("\n  ")
    );

    // Direct negative checks on helper: reserved prefix and missing prefix fail and name the file/prefix.
    let reserved_err = check_case_file_prefix("est-90.toml", &states)
        .expect_err("reserved prefix EST must be refused");
    assert!(
        reserved_err.contains("est-90.toml") && reserved_err.contains("EST"),
        "unexpected error for reserved prefix: {reserved_err}"
    );
    let missing_err = check_case_file_prefix("90.toml", &states)
        .expect_err("file without prefix must be refused");
    assert!(
        missing_err.contains("90.toml"),
        "unexpected error for missing prefix: {missing_err}"
    );
}
