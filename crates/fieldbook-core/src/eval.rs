//! The regression set: parse `corpus/eval/regression.toml` and judge each case against the
//! Corpus search index (`docs/reference/search.md#regression-set`).
//!
//! **Responsibility.** [`parse`] turns the file's text into [`Case`]s or typed [`EvalError`]s,
//! each with the 1-based line it points at; [`corpus_index`] builds the in-memory index of the
//! shipped public Corpus that the set and the `search` command share; [`unknown_expected`] and
//! [`miss`] judge one case against that index. The `check` pass that raises `FB0126` to `FB0128`
//! is `check::regression`.
//!
//! **Rules.** Pure: no I/O. The program never sets `critical`; a person does. `from` is not a key
//! here (`ask-promote` adds it).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::Deserialize;
use toml::Spanned;

use crate::check::Tree;
use crate::labels::LabelSet;
use crate::manifest::ManifestError;
use crate::schema::corpus_toml::byte_offset_to_line_col;
use crate::search::{Index, build_shipped_corpus_units, query};

/// Repository-relative path of the regression set.
pub const REGRESSION_PATH: &str = "corpus/eval/regression.toml";

/// The `top` of a case that does not write one.
pub const DEFAULT_TOP: usize = 5;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    #[serde(default)]
    case: Vec<Spanned<RawCase>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCase {
    id: String,
    query: String,
    #[serde(default)]
    expect: Vec<String>,
    top: Option<usize>,
    #[serde(default)]
    expect_none: bool,
    #[serde(default)]
    critical: bool,
}

/// One validated `[[case]]` of the regression set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Case {
    /// Unique case identifier (`rg-NNN`).
    pub id: String,
    /// The text a person would type.
    pub query: String,
    /// Unit ids that must each appear within the first `top` results (empty for `expect_none`).
    pub expect: Vec<String>,
    /// How many leading results an expected id may appear in.
    pub top: usize,
    /// `true` when the result list must be empty.
    pub expect_none: bool,
    /// `true` when a miss is an error rather than a warning.
    pub critical: bool,
    /// 1-based line the case starts on, from the parser's span (its `[[case]]` header, or the inline table).
    pub line: usize,
}

/// A fault in `corpus/eval/regression.toml` (`FB0126`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalError {
    /// 1-based line the fault points at.
    pub line: usize,
    /// 1-based code-point column the fault points at.
    pub col: usize,
    /// Human-readable description of the fault.
    pub message: String,
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.message)
    }
}

impl std::error::Error for EvalError {}

fn to_usize(value: u32) -> usize {
    usize::try_from(value).unwrap_or(1)
}

/// Parses the text of `corpus/eval/regression.toml`.
///
/// # Errors
///
/// Returns every [`EvalError`] found: a TOML or key fault (unknown keys are refused), an empty
/// `id` or `query`, a repeated `id`, a `top` of zero, or a case that has neither `expect` nor
/// `expect_none = true`, or both.
pub fn parse(text: &str) -> Result<Vec<Case>, Vec<EvalError>> {
    let raw: RawFile = match toml::from_str(text) {
        Ok(parsed) => parsed,
        Err(err) => {
            let offset = err.span().map_or(0, |range| range.start);
            let (line, col) = byte_offset_to_line_col(text, offset);
            return Err(vec![EvalError {
                line: to_usize(line),
                col: to_usize(col),
                message: format!("malformed `regression.toml`: {}", err.message()),
            }]);
        }
    };

    let mut errors = Vec::new();
    let mut first_line_of: BTreeMap<String, usize> = BTreeMap::new();
    let mut cases = Vec::with_capacity(raw.case.len());

    for spanned in raw.case {
        let (line, _) = byte_offset_to_line_col(text, spanned.span().start);
        let line = to_usize(line);
        let item = spanned.into_inner();
        let mut fault = |message: String| {
            errors.push(EvalError {
                line,
                col: 1,
                message,
            });
        };

        if item.id.trim().is_empty() {
            fault("a case has an empty `id`".to_owned());
        } else if let Some(first) = first_line_of.get(&item.id) {
            fault(format!(
                "case `{}` repeats the `id` of the case on line {first}",
                item.id
            ));
        } else {
            first_line_of.insert(item.id.clone(), line);
        }
        if item.query.trim().is_empty() {
            fault(format!("case `{}` has an empty `query`", item.id));
        }
        if item.top == Some(0) {
            fault(format!("case `{}` has `top = 0`", item.id));
        }
        match (item.expect.is_empty(), item.expect_none) {
            (true, false) => fault(format!(
                "case `{}` has neither `expect` nor `expect_none = true`",
                item.id
            )),
            (false, true) => fault(format!(
                "case `{}` has both `expect` and `expect_none = true`",
                item.id
            )),
            _ => {}
        }
        let mut seen = BTreeSet::new();
        for id in &item.expect {
            if id.trim().is_empty() || !seen.insert(id.as_str()) {
                fault(format!(
                    "case `{}` lists an empty or repeated expected id `{id}`",
                    item.id
                ));
            }
        }

        cases.push(Case {
            id: item.id,
            query: item.query,
            expect: item.expect,
            top: item.top.unwrap_or(DEFAULT_TOP),
            expect_none: item.expect_none,
            critical: item.critical,
            line,
        });
    }

    if errors.is_empty() {
        Ok(cases)
    } else {
        Err(errors)
    }
}

/// Builds the in-memory search index of the shipped public Corpus held by `tree`, the index that
/// the `search` command and the regression set share.
///
/// # Errors
///
/// Returns the [`ManifestError`] when `tree` is not a coherent public Corpus.
pub fn corpus_index(tree: &Tree) -> Result<Index, ManifestError> {
    let (units, glossary) =
        build_shipped_corpus_units(tree, &BTreeSet::new(), LabelSet::is_public)?;
    Ok(Index::build(units, &glossary))
}

/// The ids of `case.expect` that are not units of `index`.
#[must_use]
pub fn unknown_expected<'a>(index: &Index, case: &'a Case) -> Vec<&'a str> {
    case.expect
        .iter()
        .filter(|id| index.unit_index(id).is_none())
        .map(String::as_str)
        .collect()
}

/// Runs `case` against `index` and describes how it misses, or `None` when it holds.
///
/// An expected id misses when it is not within the first `top` results; an `expect_none` case
/// misses when the query returns any result.
#[must_use]
pub fn miss(index: &Index, case: &Case) -> Option<String> {
    let results = query(index, &case.query, index.docs.len());
    if case.expect_none {
        let first = results.hits.first()?;
        return Some(format!(
            "case `{}`: `{}` should return nothing but returns {} result(s), the first `{}`",
            case.id, case.query, results.total, first.id
        ));
    }
    let mut problems = Vec::new();
    for expected in &case.expect {
        let rank = results
            .hits
            .iter()
            .position(|hit| &hit.id == expected)
            .map(|pos| pos + 1);
        match rank {
            Some(rank) if rank <= case.top => {}
            Some(rank) => problems.push(format!("`{expected}` ranks {rank}")),
            None => problems.push(format!("`{expected}` is not in the results")),
        }
    }
    if problems.is_empty() {
        None
    } else {
        Some(format!(
            "case `{}`: `{}` should have its answer within the first {} results, but {}",
            case.id,
            case.query,
            case.top,
            problems.join(" and ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{Case, DEFAULT_TOP, miss, parse};
    use crate::search::{Glossary, GoldenFields, GoldenUnit, Index};

    #[test]
    fn a_case_takes_its_defaults_and_its_header_line() {
        let text = "# note\n\n[[case]]\nid = \"rg-001\"\nquery = \"a b\"\nexpect = [\"x\"]\n\n[[case]] # two\nid = \"rg-002\"\nquery = \"zzz\"\nexpect_none = true\ncritical = true\ntop = 3\n";
        let cases = parse(text).expect("valid file");
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].line, 3);
        assert_eq!(cases[0].top, DEFAULT_TOP);
        assert!(!cases[0].critical && !cases[0].expect_none);
        assert_eq!(cases[1].line, 8);
        assert!(cases[1].critical && cases[1].expect_none);
        assert_eq!(cases[1].top, 3);
    }

    #[test]
    fn an_empty_file_has_no_cases() {
        assert_eq!(parse("").expect("empty file"), Vec::new());
    }

    #[test]
    fn unknown_keys_including_from_are_refused() {
        let text = "[[case]]\nid = \"rg-001\"\nquery = \"q\"\nexpect = [\"x\"]\nfrom = \"Q-001\"\n";
        let errs = parse(text).expect_err("`from` is not a key yet");
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("from"), "{}", errs[0].message);
    }

    #[test]
    fn faults_point_at_the_case_header() {
        let text = "[[case]]\nid = \"rg-001\"\nquery = \"q\"\nexpect = [\"x\"]\n\n[[case]]\nid = \"rg-001\"\nquery = \"q\"\nexpect = [\"x\"]\n\n[[case]]\nid = \"rg-003\"\nquery = \"q\"\n";
        let errs = parse(text).expect_err("repeat and missing expectation");
        assert_eq!(errs.len(), 2);
        assert_eq!(errs[0].line, 6);
        assert!(errs[0].message.contains("repeats"), "{}", errs[0].message);
        assert_eq!(errs[1].line, 11);
        assert!(errs[1].message.contains("neither"), "{}", errs[1].message);
    }

    #[test]
    fn the_default_top_is_five() {
        assert_eq!(DEFAULT_TOP, 5);
        let cases = parse("[[case]]\nid = \"rg-001\"\nquery = \"q\"\nexpect = [\"x\"]\n")
            .expect("valid file");
        assert_eq!(cases[0].top, 5);
    }

    #[test]
    fn lines_come_from_the_parser_not_from_the_spelling() {
        let spaced = "[[case]]\nid = \"rg-001\"\nquery = \"q\"\nexpect = [\"x\"]\n\n[[ case ]]\nid = \"rg-002\"\nquery = \"q\"\nexpect = [\"x\"]\n\n[[case]]\nid = \"rg-003\"\nquery = \"q\"\nexpect = [\"x\"]\n";
        let lines: Vec<usize> = parse(spaced)
            .expect("valid")
            .iter()
            .map(|c| c.line)
            .collect();
        assert_eq!(lines, vec![1, 6, 11]);

        let quoted = "[[case]]\nid = \"rg-001\"\nquery = \"q\"\nexpect = [\"x\"]\n\n[[\"case\"]]\nid = \"rg-002\"\nquery = \"q\"\nexpect = [\"x\"]\n";
        let lines: Vec<usize> = parse(quoted)
            .expect("valid")
            .iter()
            .map(|c| c.line)
            .collect();
        assert_eq!(lines, vec![1, 6]);

        let inline = "case = [\n  { id = \"rg-001\", query = \"q\", expect = [\"x\"] },\n  { id = \"rg-001\", query = \"q\", expect = [\"x\"] },\n]\n";
        let errs = parse(inline).expect_err("repeated id");
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].line, 3, "{errs:?}");
    }

    #[test]
    fn a_byte_order_mark_never_shifts_a_pointer() {
        let text = "\u{feff}[[case]]\nid = \"rg-001\"\nquery = \"q\"\n\n[[case]]\nid = \"rg-002\"\nquery = \"q\"\nexpect = [\"x\"]\n";
        match parse(text) {
            Ok(cases) => {
                assert_eq!(cases.iter().map(|c| c.line).collect::<Vec<_>>(), vec![1, 5]);
            }
            Err(errs) => {
                for err in errs {
                    assert!(matches!(err.line, 1 | 5), "{err:?}");
                }
            }
        }
    }

    fn unit(id: &str, fields: GoldenFields) -> GoldenUnit {
        GoldenUnit {
            id: id.to_owned(),
            kind: "claim".to_owned(),
            fields,
            ..GoldenUnit::default()
        }
    }

    /// `u-one` ranks 1 for "alpha" (title), `u-two` 2 (heading), `u-three` 3 (body).
    fn ranked_index() -> Index {
        let blank = || GoldenFields::default();
        Index::from_golden(
            &[
                unit(
                    "u-one",
                    GoldenFields {
                        title: "alpha".to_owned(),
                        ..blank()
                    },
                ),
                unit(
                    "u-two",
                    GoldenFields {
                        headings: "alpha".to_owned(),
                        ..blank()
                    },
                ),
                unit(
                    "u-three",
                    GoldenFields {
                        body: "alpha".to_owned(),
                        ..blank()
                    },
                ),
                unit(
                    "u-other",
                    GoldenFields {
                        body: "zzz".to_owned(),
                        ..blank()
                    },
                ),
            ],
            &Glossary::default(),
        )
    }

    fn case(query: &str, expect: &[&str], top: usize) -> Case {
        Case {
            id: "rg-001".to_owned(),
            query: query.to_owned(),
            expect: expect.iter().map(|id| (*id).to_owned()).collect(),
            top,
            expect_none: false,
            critical: false,
            line: 1,
        }
    }

    #[test]
    fn an_answer_within_top_holds_and_one_past_it_misses() {
        let index = ranked_index();
        assert_eq!(miss(&index, &case("alpha", &["u-one"], 1)), None);
        let late = miss(&index, &case("alpha", &["u-two"], 1)).expect("rank 2 misses top 1");
        assert!(late.contains("`u-two` ranks 2"), "{late}");
        assert!(late.contains("first 1 results"), "{late}");
        assert_eq!(miss(&index, &case("alpha", &["u-two"], 2)), None);
        assert_eq!(miss(&index, &case("alpha", &["u-three"], 3)), None);
        assert!(miss(&index, &case("alpha", &["u-three"], 2)).is_some());
    }

    #[test]
    fn every_expected_id_must_be_within_top() {
        let index = ranked_index();
        let message = miss(&index, &case("alpha", &["u-one", "u-three"], 2)).expect("one late");
        assert!(message.contains("`u-three` ranks 3"), "{message}");
        assert!(!message.contains("`u-one`"), "{message}");
        assert_eq!(miss(&index, &case("alpha", &["u-one", "u-three"], 3)), None);
    }

    #[test]
    fn an_answer_that_is_not_found_misses() {
        let index = ranked_index();
        let message = miss(&index, &case("alpha", &["u-other"], 5)).expect("not a hit");
        assert!(message.contains("is not in the results"), "{message}");
    }

    #[test]
    fn expect_none_misses_exactly_when_the_query_returns_something() {
        let index = ranked_index();
        let mut none = case("alpha", &[], 5);
        none.expect_none = true;
        let message = miss(&index, &none).expect("alpha returns results");
        assert!(message.contains("should return nothing"), "{message}");
        assert!(message.contains("`u-one`"), "{message}");
        none.query = "qqq".to_owned();
        assert_eq!(miss(&index, &none), None);
    }
}
