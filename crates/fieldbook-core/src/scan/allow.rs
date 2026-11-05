//! Parser and matcher for `corpus/scan-allow.toml` (`FB0404`, `FB0405`,
//! `docs/reference/corpus-format.md#scans`).
//!
//! **Responsibility.** Parse `scan-allow.toml` with `serde` and `deny_unknown_fields`,
//! validate each `[[allow]]` entry (`sha256` is 64 lowercase hex characters, `match`
//! and `reason` are non-empty, no duplicate `(sha256, match)` pair), match allowances
//! against `FB0400` findings by `(sha256, matched)`, and report unused entries (`FB0405`).

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::schema::corpus_toml::byte_offset_to_line_col;

/// Raw TOML structure of `corpus/scan-allow.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawScanAllowFile {
    #[serde(default)]
    allow: Vec<RawAllowEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAllowEntry {
    sha256: String,
    #[serde(rename = "match")]
    matched: String,
    reason: String,
}

/// One validated allowance entry from `corpus/scan-allow.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowEntry {
    /// 64 lowercase hexadecimal characters: the SHA-256 of the file that holds the match.
    pub sha256: String,
    /// The matched banner text as the marking scan reports it.
    pub matched: String,
    /// Non-empty explanation of why this public text may stay.
    pub reason: String,
    /// 1-based line number of this entry in `scan-allow.toml`.
    pub line: usize,
    /// 1-based column number of this entry in `scan-allow.toml`.
    pub col: usize,
}

/// A validation or syntax error in `corpus/scan-allow.toml` (`FB0404`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowError {
    /// 1-based line number of the fault.
    pub line: usize,
    /// 1-based code-point column number of the fault.
    pub col: usize,
    /// Human-readable description of the fault.
    pub message: String,
}

impl fmt::Display for AllowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.message)
    }
}

impl std::error::Error for AllowError {}

/// Parsed and validated `scan-allow.toml` table, tracking which entries were matched during a run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Allowances {
    entries: Vec<AllowEntry>,
    used: Vec<bool>,
}

impl Allowances {
    /// Returns the validated entries in file order.
    #[must_use]
    pub fn entries(&self) -> &[AllowEntry] {
        &self.entries
    }

    /// Returns `true` if an allowance covers `(file_sha256, matched)`, marking that entry as used.
    pub fn allows(&mut self, file_sha256: &str, matched: &str) -> bool {
        let mut hit = false;
        for (idx, entry) in self.entries.iter().enumerate() {
            if entry.sha256 == file_sha256 && entry.matched == matched {
                self.used[idx] = true;
                hit = true;
            }
        }
        hit
    }

    /// Returns all entries that did not match anything during this run (`FB0405`).
    #[must_use]
    pub fn unused(&self) -> Vec<&AllowEntry> {
        self.entries
            .iter()
            .zip(self.used.iter())
            .filter_map(|(entry, &was_used)| (!was_used).then_some(entry))
            .collect()
    }
}

fn is_lower_hex_sha256(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn find_allow_header_lines(text: &str) -> Vec<usize> {
    let mut lines = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        if line.trim() == "[[allow]]" {
            lines.push(idx + 1);
        }
    }
    lines
}

/// Parse and validate `corpus/scan-allow.toml` text.
///
/// Returns `Err(Vec<AllowError>)` when the file is not valid TOML for the schema,
/// when `sha256` is not 64 lowercase hex characters, when `match` or `reason` is empty,
/// or when two entries have the same `(sha256, match)` (`FB0404`).
pub fn parse(text: &str) -> Result<Allowances, Vec<AllowError>> {
    let raw: RawScanAllowFile = match toml::from_str(text) {
        Ok(parsed) => parsed,
        Err(err) => {
            let offset = err.span().map_or(0, |r| r.start);
            let (line, col) = byte_offset_to_line_col(text, offset);
            return Err(vec![AllowError {
                line: usize::try_from(line).unwrap_or(1),
                col: usize::try_from(col).unwrap_or(1),
                message: format!("malformed `scan-allow.toml`: {}", err.message()),
            }]);
        }
    };

    let header_lines = find_allow_header_lines(text);
    let mut errors = Vec::new();
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    let mut entries = Vec::with_capacity(raw.allow.len());

    for (idx, item) in raw.allow.into_iter().enumerate() {
        let line = header_lines.get(idx).copied().unwrap_or(1);
        let col = 1usize;

        if !is_lower_hex_sha256(&item.sha256) {
            errors.push(AllowError {
                line,
                col,
                message: format!(
                    "`scan-allow.toml` entry has invalid `sha256` `{}` (expected 64 lowercase hex characters)",
                    item.sha256
                ),
            });
        }
        if item.matched.trim().is_empty() {
            errors.push(AllowError {
                line,
                col,
                message: "`scan-allow.toml` entry has an empty `match`".to_owned(),
            });
        }
        if item.reason.trim().is_empty() {
            errors.push(AllowError {
                line,
                col,
                message: "`scan-allow.toml` entry has an empty `reason`".to_owned(),
            });
        }
        let key = (item.sha256.clone(), item.matched.clone());
        if !seen.insert(key) {
            errors.push(AllowError {
                line,
                col,
                message: format!(
                    "duplicate `scan-allow.toml` entry for `sha256 = \"{}\"` and `match = \"{}\"`",
                    item.sha256, item.matched
                ),
            });
        }

        entries.push(AllowEntry {
            sha256: item.sha256,
            matched: item.matched,
            reason: item.reason,
            line,
            col,
        });
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let used = vec![false; entries.len()];
    Ok(Allowances { entries, used })
}
