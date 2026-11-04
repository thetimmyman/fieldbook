//! The `FB0100` pass for `corpus/corpus.toml`.

use crate::schema::{CorpusProblem, CorpusToml};
use crate::version;

use super::code::Code;
use super::diag::{Diagnostic, Report};
use super::tree::{Entry, Tree};

/// Path of the Corpus configuration file relative to the repository root.
pub const CORPUS_TOML_PATH: &str = "corpus/corpus.toml";

/// Read, parse and validate `corpus/corpus.toml` from `tree`, appending `FB0100` diagnostics to `report`.
///
/// Returns `Some(CorpusToml)` when `corpus/corpus.toml` is present, valid TOML and passes [`CorpusToml::validate`]
/// with zero problems; returns `None` when any `FB0100` diagnostic was raised.
pub fn load_corpus_toml(tree: &Tree, strict: bool, report: &mut Report) -> Option<CorpusToml> {
    let Some(entry) = tree.get(CORPUS_TOML_PATH) else {
        report.push(Diagnostic::new(
            Code::Fb0100,
            strict,
            CORPUS_TOML_PATH,
            1,
            1,
            "missing corpus/corpus.toml",
        ));
        return None;
    };

    let Entry::Bytes(bytes) = entry else {
        report.push(Diagnostic::new(
            Code::Fb0100,
            strict,
            CORPUS_TOML_PATH,
            1,
            1,
            "corpus/corpus.toml is not a text file",
        ));
        return None;
    };

    let Ok(text) = std::str::from_utf8(bytes) else {
        report.push(Diagnostic::new(
            Code::Fb0100,
            strict,
            CORPUS_TOML_PATH,
            1,
            1,
            "corpus/corpus.toml is not valid UTF-8",
        ));
        return None;
    };

    let parsed = match CorpusToml::parse(text) {
        Ok(corpus) => corpus,
        Err(err) => {
            report.push(Diagnostic::new(
                Code::Fb0100,
                strict,
                CORPUS_TOML_PATH,
                usize::try_from(err.line()).unwrap_or(1),
                usize::try_from(err.col()).unwrap_or(1),
                err.message(),
            ));
            return None;
        }
    };

    let problems = parsed.validate();
    if !problems.is_empty() {
        for problem in problems {
            let msg = match &problem {
                CorpusProblem::UnsupportedSchemaVersion(v) => format!(
                    "schema_version {v} is not supported (this tool reads schema {})",
                    version::SCHEMA_VERSION_MAX
                ),
                other => other.to_string(),
            };
            report.push(Diagnostic::new(
                Code::Fb0100,
                strict,
                CORPUS_TOML_PATH,
                1,
                1,
                msg,
            ));
        }
        return None;
    }

    Some(parsed)
}
