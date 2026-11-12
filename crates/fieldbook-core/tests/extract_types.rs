//! Verification of extractor type registrations and dispatch.
//!
//! **Responsibility.** Asserts that `fieldbook_core::add::TEXT_TYPES` and
//! `fieldbook_core::extraction::SELECT_TYPES` stay in lockstep, and that
//! `fieldbook_core::extraction::select` dispatches exactly the supported extensions.
//!
//! **Rules.** Pure tests: no filesystem, clock, or environment access.

use std::collections::BTreeSet;

use fieldbook_core::add::TEXT_TYPES;
use fieldbook_core::extraction::{SELECT_TYPES, select};

#[test]
fn text_types_equal_the_extractor_list() {
    let mut text_set = BTreeSet::new();
    for &t in TEXT_TYPES {
        assert!(
            text_set.insert(t),
            "duplicate entry in TEXT_TYPES: {t}; a later card appends to both lists in its own pull request"
        );
    }

    let mut select_set = BTreeSet::new();
    for &s in SELECT_TYPES {
        assert!(
            select_set.insert(s),
            "duplicate entry in SELECT_TYPES: {s}; a later card appends to both lists in its own pull request"
        );
    }

    let mut sorted_text = TEXT_TYPES.to_vec();
    sorted_text.sort_unstable();
    let mut sorted_select = SELECT_TYPES.to_vec();
    sorted_select.sort_unstable();

    assert_eq!(
        sorted_text, sorted_select,
        "TEXT_TYPES and SELECT_TYPES must hold the same entries; a later card appends to both lists in its own pull request"
    );
    assert_eq!(
        text_set, select_set,
        "TEXT_TYPES and SELECT_TYPES sets must match; a later card appends to both lists in its own pull request"
    );
}

#[test]
fn select_returns_an_extractor_for_exactly_the_listed_types() {
    for &entry in SELECT_TYPES {
        assert!(
            select(entry, b"").is_some(),
            "select should return Some for lowercase {entry}"
        );
        let upper = entry.to_ascii_uppercase();
        assert!(
            select(&upper, b"").is_some(),
            "select should return Some for uppercase {upper}"
        );
    }

    for non_type in ["pdf", "csv", "docx", "", "exe"] {
        assert!(
            select(non_type, b"").is_none(),
            "select should return None for {non_type:?}"
        );
    }
}
