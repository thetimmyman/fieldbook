//! Retained classification kernel for the Classifier `rules@1` adapter (published
//! compatibility binding).
//!
//! **Responsibility.** Frozen deterministic classification behavior of `rules@1` over a parsed
//! [`RulesFile`], a [`Candidate`], and a store [`Vocab`].
//! A change of contract requires a new `rules@2` version module; this file and its archived vectors
//! stay executable.
//!
//! **Rules.** Pure: no I/O.

use crate::pipeline::rules::{RuleError, Rules, RulesFile};
use crate::pipeline::{Candidate, Classifier, StageError, Suggestions, Vocab};

/// Adapter identifier retained by this kernel (`rules@1`).
pub const ADAPTER_ID: &str = "rules@1";

/// Parses `rules_toml` under `vocab` and classifies `candidate` using the frozen `rules@1` rules.
///
/// # Errors
///
/// Returns `Ok(Err(errors))` when `rules_toml` fails `RulesFile::parse`, or `Err(StageError)` if
/// the classifier itself refuses.
pub fn parse_and_classify(
    rules_toml: &str,
    candidate: &Candidate,
    vocab: &Vocab,
) -> Result<Result<Suggestions, Vec<RuleError>>, StageError> {
    let parsed = match RulesFile::parse(rules_toml, vocab) {
        Ok(file) => file,
        Err(errors) => return Ok(Err(errors)),
    };
    let suggestions = Rules::new(parsed).classify(candidate, vocab)?;
    Ok(Ok(suggestions))
}
