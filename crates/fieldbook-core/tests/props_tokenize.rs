//! Property tests for search tokenization and normalisation.

#[path = "../../../tests/support/rng.rs"]
mod rng;

use proptest::prelude::*;
use proptest::test_runner::{Config as ProptestConfig, RngAlgorithm, RngSeed};
use rng::Rng;
use unicode_normalization::UnicodeNormalization;

use fieldbook_core::search::lexer::{index_tokens, lex, query_tokens};
use fieldbook_core::search::normalise::normalise;

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 256,
        failure_persistence: None,
        rng_algorithm: RngAlgorithm::default(),
        rng_seed: RngSeed::Fixed(Rng::new(1).next_u64()),
        ..ProptestConfig::default()
    }
}

/// Generates strings of 0 to 24 pieces mixing ASCII letters, digits, punctuation,
/// dashes, combining marks, compatibility characters, soft hyphens, and arbitrary chars.
fn mixed_text() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        prop::char::range('a', 'z'),
        prop::char::range('A', 'Z'),
        prop::char::range('0', '9'),
        Just(' '),
        Just(','),
        Just('.'),
        Just('('),
        Just(')'),
        Just('-'),
        Just('_'),
        prop::char::range('\u{2010}', '\u{2015}'),
        Just('\u{2212}'),
        prop::char::range('\u{0300}', '\u{036F}'),
        prop::char::range('\u{fb00}', '\u{fb06}'),
        prop::char::range('\u{ff01}', '\u{ff5e}'),
        Just('\u{00b2}'),
        Just('\u{00b3}'),
        Just('\u{00b9}'),
        prop::char::range('\u{2070}', '\u{2079}'),
        prop::char::range('\u{2080}', '\u{2089}'),
        Just('\u{00ad}'),
        any::<char>(),
    ];
    prop::collection::vec(piece, 0..=24).prop_map(|v| v.into_iter().collect::<String>())
}

/// Generates strings structured according to lexer identifier and number shapes.
fn lexer_shaped_text() -> impl Strategy<Value = String> {
    let pieces = prop::collection::vec(
        prop_oneof![
            "s-[0-9]{4}(-v[0-9]{1,2})?",
            "cl-[a-z]{3}-[0-9]{4}",
            "q-[0-9]{3}",
            "xc-[0-9]{1,3}",
            r"[0-9]{1,4}\.[0-9]{3}(-[0-9]{1,4})?(\([a-z0-9]{1,3}\)){0,2}",
            r"[0-9]{1,3}(,[0-9]{3}){0,3}(\.[0-9]{1,3})?",
            "[a-z]{1,8}",
        ],
        0..=6,
    );
    let sep = prop_oneof![Just(" "), Just(", "), Just("; "), Just("-")];
    (pieces, sep).prop_map(|(p, s)| p.join(s))
}

/// Returns `true` when `c` is accepted by the restricted lexer property strategies.
fn allowed(c: char) -> bool {
    let nfkc: String = std::iter::once(c).nfkc().collect();
    !nfkc.contains('\u{00ad}') && nfkc == nfkc.to_lowercase()
}

/// Generates strings from `mixed_text()` restricted to characters safe for normalisation idempotence.
fn restricted_text() -> impl Strategy<Value = String> {
    mixed_text().prop_map(|s| s.chars().filter(|&c| allowed(c)).collect::<String>())
}

proptest! {
    #![proptest_config(config())]

    /// Lexing and normalisation never panic on arbitrary, mixed, or lexer-shaped text, and query tokens are capped at 12.
    #[test]
    fn lex_never_panics(
        s in prop_oneof![any::<String>(), mixed_text(), lexer_shaped_text()],
    ) {
        let _ = normalise(&s);
        let _ = lex(&s);
        let _ = index_tokens(&s);
        let qry = query_tokens(&s);
        prop_assert!(qry.len() <= 12);
    }
}

proptest! {
    #![proptest_config(config())]

    /// Every token produced by `index_tokens` is non-empty and a fixed point of `normalise`.
    #[test]
    fn index_tokens_are_normalised_and_nonempty(
        s in prop_oneof![restricted_text(), lexer_shaped_text()],
    ) {
        for tok in index_tokens(&s) {
            prop_assert!(!tok.is_empty());
            prop_assert_eq!(normalise(&tok), tok);
        }
    }
}

proptest! {
    #![proptest_config(config())]

    /// Every token emitted by `query_tokens` is present in `index_tokens` for the same input.
    #[test]
    fn query_tokens_is_a_subset_of_index_tokens_for_the_same_text(
        s in prop_oneof![any::<String>(), mixed_text(), lexer_shaped_text()],
    ) {
        let idx = index_tokens(&s);
        let qry = query_tokens(&s);
        for tok in &qry {
            prop_assert!(
                idx.contains(tok),
                "query token `{tok}` not found in index tokens {idx:?} for input `{s:?}`"
            );
        }
    }
}

proptest! {
    #![proptest_config(config())]

    /// Re-lexing space-joined tokens produces the identical tokens on both lex and query paths.
    #[test]
    fn lexing_the_joined_tokens_gives_the_same_tokens(
        s in prop_oneof![restricted_text(), lexer_shaped_text()],
    ) {
        let toks: Vec<String> = lex(&s).into_iter().map(|tok| tok.text).collect();
        let joined = toks.join(" ");
        let relexed: Vec<String> = lex(&joined).into_iter().map(|tok| tok.text).collect();
        prop_assert_eq!(relexed, toks);

        let qry = query_tokens(&s);
        let qry_joined = qry.join(" ");
        prop_assert_eq!(query_tokens(&qry_joined), qry);
    }
}

proptest! {
    #![proptest_config(config())]

    /// Normalising already-normalised text produces the identical string.
    #[test]
    fn normalise_is_idempotent(s in prop_oneof![restricted_text(), lexer_shaped_text()]) {
        prop_assert_eq!(normalise(&normalise(&s)), normalise(&s));
    }
}
