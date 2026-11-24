//! Pass `pipeline`: `FB0826` (rules file) (`docs/reference/corpus-format.md#08-audit-backup-and-pipeline`).
//!
//! **Responsibility.** Replay every `[[rule]]` of `corpus/pipeline/rules.toml` on its own
//! `example` and raise `FB0826` for a rule that cannot be read, a repeated id, a missing `example`
//! or `expect`, a `set` value outside the vocabulary, or a result that differs from `expect`.

use crate::check::{Code, Diagnostic, Model, Options, Report};
use crate::extraction::{Extraction, PageText};
use crate::hash::sha256_hex;
use crate::labels::LabelSet;
use crate::pipeline::rules::{Rule, Rules, RulesFile};
use crate::pipeline::{Candidate, Classifier, Suggestions, Support, Vocab};

/// Path of the intake rules file relative to the repository root.
pub const RULES_PATH: &str = "corpus/pipeline/rules.toml";

/// Run the `pipeline` check pass (`FB0826`) over `model`.
pub fn check(model: &Model, options: &Options, report: &mut Report) {
    if model.corpus().is_none() {
        return;
    }
    let Some(bytes) = model.tree().get_bytes(RULES_PATH) else {
        return;
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        // FB0120 already reported the non-UTF-8 file.
        return;
    };

    let vocab = Vocab::from_schema();
    let rules = match RulesFile::parse(text, &vocab) {
        Ok(rules) => rules,
        Err(errors) => {
            for err in errors {
                report.push(Diagnostic::new(
                    Code::Fb0826,
                    options.strict,
                    RULES_PATH,
                    usize::try_from(err.line).unwrap_or(1),
                    1,
                    err.message,
                ));
            }
            return;
        }
    };

    let classifier = Rules::new(rules.clone());
    for rule in &rules.rules {
        let candidate = synthetic_candidate(&rule.example);
        let Ok(suggestions) = classifier.classify(&candidate, &vocab) else {
            continue;
        };
        if let Some(message) = expectation_mismatch(rule, &suggestions) {
            report.push(Diagnostic::new(
                Code::Fb0826,
                options.strict,
                RULES_PATH,
                usize::try_from(rule.line).unwrap_or(1),
                1,
                message,
            ));
        }
    }
}

fn synthetic_candidate(example: &str) -> Candidate {
    Candidate {
        file: "example.txt".to_owned(),
        sha256: sha256_hex(example.as_bytes()),
        bytes: example.len() as u64,
        ext: "txt".to_owned(),
        labels: LabelSet::public(),
        origin: None,
        extraction: Some(Extraction {
            method: "plain",
            pages: vec![PageText {
                number: 1,
                text: example.to_owned(),
            }],
        }),
    }
}

fn expectation_mismatch(rule: &Rule, suggestions: &Suggestions) -> Option<String> {
    let expect = &rule.expect;
    let produced: Vec<&str> = suggestions
        .fields
        .iter()
        .filter(|s| s.field == expect.field)
        .map(|s| s.value.as_str())
        .collect();

    let value_matches = produced.iter().any(|v| *v == expect.value);
    let quote_matches = match &expect.quote {
        Some(wanted) => suggestions.fields.iter().any(|s| {
            s.field == expect.field
                && s.support
                    .iter()
                    .any(|sup| matches!(sup, Support::Quote { quote, .. } if quote == wanted))
        }),
        None => true,
    };

    if value_matches && quote_matches {
        return None;
    }

    if value_matches {
        return Some(format!(
            "rule `{}` reproduced `{}` for `{}` without the expected quote",
            rule.id, expect.value, expect.field
        ));
    }

    let produced_desc = if produced.is_empty() {
        "nothing".to_owned()
    } else {
        produced
            .iter()
            .map(|v| format!("`{v}`"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    Some(format!(
        "rule `{}` does not reproduce its example: produced {} for `{}`, expected `{}`",
        rule.id, produced_desc, expect.field, expect.value
    ))
}
