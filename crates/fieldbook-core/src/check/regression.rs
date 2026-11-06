//! The `regression` pass: `FB0126` to `FB0128` (`docs/reference/search.md#regression-set`,
//! `docs/reference/corpus-format.md#check-catalogue`).
//!
//! **Responsibility.**
//! - `FB0126` (error): `corpus/eval/regression.toml` is malformed, repeats a case `id`, names an
//!   expected unit id that is not a unit of the Corpus, or has cases that cannot run because the
//!   Corpus cannot be indexed and no other error says so.
//! - `FB0127` (warning): a case misses: an expected id is not within `top`, or an `expect_none`
//!   query returns results.
//! - `FB0128` (error): a case marked `critical` misses.
//!
//! Each finding points at the case's `[[case]]` line. The cases run against the index built from
//! the shipped public Corpus, the index that `fieldbook search` uses.

use crate::eval::{self, REGRESSION_PATH};

use super::{Code, Diagnostic, Model, Options, Report};

/// Run the `regression` check pass over `model`.
pub fn check(model: &Model, options: &Options, report: &mut Report) {
    if model.corpus().is_none() {
        return;
    }
    let strict = options.strict;
    let Some(bytes) = model.tree().get_bytes(REGRESSION_PATH) else {
        return;
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return;
    };

    let cases = match eval::parse(text) {
        Ok(cases) => cases,
        Err(errors) => {
            for err in errors {
                report.push(Diagnostic::new(
                    Code::Fb0126,
                    strict,
                    REGRESSION_PATH,
                    err.line,
                    err.col,
                    err.message,
                ));
            }
            return;
        }
    };
    if cases.is_empty() {
        return;
    }

    let index = match eval::corpus_index(model.tree()) {
        Ok(index) => index,
        Err(err) => {
            // A refused Corpus is usually reported by its own pass. When no pass has said so, the
            // set must not silently not run: its critical cases are the release gate.
            if report.errors() == 0 {
                report.push(Diagnostic::new(
                    Code::Fb0126,
                    strict,
                    REGRESSION_PATH,
                    1,
                    1,
                    format!(
                        "the regression set cannot run because the Corpus cannot be indexed ({err})"
                    ),
                ));
            } else {
                report.push_note(format!(
                    "regression set skipped: the Corpus cannot be indexed ({err})"
                ));
            }
            return;
        }
    };

    for case in &cases {
        let unknown = eval::unknown_expected(&index, case);
        if !unknown.is_empty() {
            report.push(Diagnostic::new(
                Code::Fb0126,
                strict,
                REGRESSION_PATH,
                case.line,
                1,
                format!(
                    "case `{}` expects {} that {} not in the Corpus",
                    case.id,
                    unknown
                        .iter()
                        .map(|id| format!("`{id}`"))
                        .collect::<Vec<_>>()
                        .join(", "),
                    if unknown.len() == 1 { "is" } else { "are" },
                ),
            ));
            continue;
        }
        if let Some(message) = eval::miss(&index, case) {
            let code = if case.critical {
                Code::Fb0128
            } else {
                Code::Fb0127
            };
            report.push(Diagnostic::new(
                code,
                strict,
                REGRESSION_PATH,
                case.line,
                1,
                message,
            ));
        }
    }
}
