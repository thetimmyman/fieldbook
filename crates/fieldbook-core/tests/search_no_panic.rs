//! Property and mutation test ensuring `search::normalise`, `index_tokens`, and `query_tokens` never panic.

#[path = "../../../tests/support/rng.rs"]
mod rng;

use std::collections::BTreeSet;

use fieldbook_core::search::lexer::{index_tokens, query_tokens};
use fieldbook_core::search::normalise::normalise;
use rng::Rng;

fn check_input(text: &str) {
    let _norm = normalise(text);
    let idx = index_tokens(text);
    for tok in &idx {
        assert!(
            !tok.is_empty(),
            "index_tokens produced empty token on {text:?}"
        );
    }

    let qry = query_tokens(text);
    assert!(
        qry.len() <= 12,
        "query_tokens produced {} > 12 tokens on {text:?}",
        qry.len()
    );
    let mut seen = BTreeSet::new();
    for tok in &qry {
        assert!(
            !tok.is_empty(),
            "query_tokens produced empty token on {text:?}"
        );
        assert!(
            seen.insert(tok.as_str()),
            "query_tokens produced duplicate token {tok:?} on {text:?}"
        );
    }
}

#[test]
fn random_and_mutated_bytes_never_panic() {
    // 1. Explicit empty boundary input.
    check_input("");

    // 2. 200 byte strings of length 0 to 96 from Rng::new(1).
    let mut prng = Rng::new(1);
    for _ in 0..200 {
        let len = usize::try_from(prng.next_u64() % 97).unwrap_or(0);
        let raw = prng.bytes(len);
        let text = String::from_utf8_lossy(&raw);
        check_input(&text);
    }

    // 3. Valid input with each byte once flipped and once deleted.
    let seed =
        "Cost-type contracts under 252.204-7012 cite CL-CMP-0011, S-0042-v2 and 1,000,000.50.";
    check_input(seed);
    let bytes = seed.as_bytes();
    for idx in 0..bytes.len() {
        let mut flipped = bytes.to_vec();
        flipped[idx] ^= 0x5a;
        let text_flip = String::from_utf8_lossy(&flipped);
        check_input(&text_flip);

        let mut deleted = bytes.to_vec();
        deleted.remove(idx);
        let text_del = String::from_utf8_lossy(&deleted);
        check_input(&text_del);
    }
}
