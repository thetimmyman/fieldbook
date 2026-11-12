//! The one shared contract for every text extractor (`extractor_contract`).
//!
//! Included by a test target with `#[path = "..."] mod extractor_suite;`. Adapter constructors and
//! wrappers stay in the including target; this file holds only what every adapter shares: the
//! determinism and no-panic checks, the typed-outcome vectors and the method validator hook.
//! A later adapter (staged, CSV, workbook) supplies its own vectors with its own expected
//! outcomes and runs the same `extractor_contract`.

use fieldbook_core::extraction::{ExtractError, Extraction, Extractor, thin_page_numbers};

#[path = "rng.rs"]
mod rng;

/// What an adapter must do with one input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expect {
    /// Extraction succeeds with exactly `pages` pages, of which `thin` (ascending numbers) are thin.
    Pages {
        /// The number of pages.
        pages: usize,
        /// The numbers of the thin pages.
        thin: Vec<u32>,
    },
    /// Extraction is refused with the named [`ExtractError`] variant (see [`error_name`]).
    Refused(&'static str),
}

/// One named input with its expected typed outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vector {
    /// The case name, printed when it fails.
    pub name: &'static str,
    /// The original bytes.
    pub input: Vec<u8>,
    /// The expected outcome.
    pub expect: Expect,
}

/// The variant name of an [`ExtractError`], as written in [`Expect::Refused`].
pub const fn error_name(error: &ExtractError) -> &'static str {
    match error {
        ExtractError::NotText => "NotText",
        ExtractError::TooLarge { .. } => "TooLarge",
        ExtractError::ToolMissing(_) => "ToolMissing",
        ExtractError::ToolFailed { .. } => "ToolFailed",
    }
}

/// The vectors of the text adapters (`Plain`, `HtmlStrip`): empty input is one thin page, and
/// bytes that are not valid UTF-8 are `NotText`.
pub fn text_vectors() -> Vec<Vector> {
    vec![
        Vector {
            name: "empty input is one thin page",
            input: Vec::new(),
            expect: Expect::Pages {
                pages: 1,
                thin: vec![1],
            },
        },
        Vector {
            name: "invalid UTF-8 is not text",
            input: vec![0xff, 0xfe, 0xfd, b'a', 0x80],
            expect: Expect::Refused("NotText"),
        },
    ]
}

/// Runs the shared contract over `extractor`:
///
/// - every vector gives its expected typed outcome, and the same input gives the same output twice;
/// - the method of a successful extraction passes `validate_method`;
/// - pages are numbered 1, 2, ... in order;
/// - 1 MB of bytes from `Rng::new(1)` is a value or a typed error and never a panic.
pub fn extractor_contract(
    extractor: &dyn Extractor,
    vectors: &[Vector],
    validate_method: &dyn Fn(&Extraction) -> Result<(), String>,
) {
    for vector in vectors {
        let first = extractor.extract(&vector.input);
        let second = extractor.extract(&vector.input);
        assert_eq!(
            first,
            second,
            "{}: {}: the same input must give the same output",
            extractor.id(),
            vector.name
        );
        match (&vector.expect, first) {
            (Expect::Pages { pages, thin }, Ok(extraction)) => {
                validate_method(&extraction)
                    .unwrap_or_else(|reason| panic!("{}: method: {reason}", extractor.id()));
                assert_eq!(
                    extraction.pages.len(),
                    *pages,
                    "{}: {}: page count",
                    extractor.id(),
                    vector.name
                );
                assert_eq!(
                    &thin_page_numbers(&extraction.pages),
                    thin,
                    "{}: {}: thin pages",
                    extractor.id(),
                    vector.name
                );
            }
            (Expect::Refused(name), Err(error)) => {
                assert_eq!(
                    error_name(&error),
                    *name,
                    "{}: {}: refusal ({error})",
                    extractor.id(),
                    vector.name
                );
            }
            (expect, outcome) => {
                panic!(
                    "{}: {}: expected {expect:?}, got {outcome:?}",
                    extractor.id(),
                    vector.name
                );
            }
        }
    }

    let big = rng::Rng::new(1).bytes(1_048_576);
    if let Ok(extraction) = extractor.extract(&big) {
        assert!(!extraction.pages.is_empty(), "{}: no pages", extractor.id());
        validate_method(&extraction)
            .unwrap_or_else(|reason| panic!("{}: method: {reason}", extractor.id()));
        for (index, page) in extraction.pages.iter().enumerate() {
            assert_eq!(
                usize::try_from(page.number).ok(),
                index.checked_add(1),
                "{}: pages are numbered from 1 in order",
                extractor.id()
            );
        }
    }
}
