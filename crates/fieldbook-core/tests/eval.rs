//! The regression-set parser (`search-cli-and-regression`,
//! `docs/reference/search.md#regression-set`). Judging cases against the Corpus and the
//! `FB0126` to `FB0128` pass are exercised through the known-bad harness and the fieldbook tests.

#[path = "../../../tests/support/rng.rs"]
mod rng;

use fieldbook_core::eval::{self, Case};
use rng::Rng;

const FIXTURE_REGRESSION: &str =
    include_str!("../../../tests/fixtures/corpus-mini/corpus/eval/regression.toml");

fn probe(text: &str) {
    match eval::parse(text) {
        Ok(cases) => {
            for case in &cases {
                assert!(case.line >= 1 && case.top >= 1, "{case:?}");
            }
        }
        Err(errors) => {
            assert!(!errors.is_empty(), "an error list is never empty");
            for err in &errors {
                assert!(err.line >= 1 && err.col >= 1, "{err:?}");
            }
        }
    }
}

#[test]
fn the_fixture_set_parses_with_its_three_shapes() {
    let cases: Vec<Case> = eval::parse(FIXTURE_REGRESSION).expect("fixture set parses");
    assert_eq!(cases.len(), 3);
    assert_eq!(
        cases
            .iter()
            .map(|c| (c.id.as_str(), c.line))
            .collect::<Vec<_>>(),
        vec![("rg-001", 3), ("rg-002", 9), ("rg-003", 14)]
    );
    assert!(
        cases.iter().all(|c| !c.critical),
        "no committed case is critical"
    );
    assert!(!cases[0].expect_none);
    assert!(cases[1].expect_none && cases[1].expect.is_empty());
    assert_eq!(cases[2].top, 3);
}

#[test]
fn regression_toml_never_panics_on_random_and_mutated_bytes() {
    probe("");

    let mut prng = Rng::new(1);
    for _ in 0..200 {
        let len = usize::try_from(prng.next_u64() % 97).unwrap_or(0);
        let raw = prng.bytes(len);
        probe(&String::from_utf8_lossy(&raw));
    }

    probe(FIXTURE_REGRESSION);
    let bytes = FIXTURE_REGRESSION.as_bytes();
    for idx in 0..bytes.len() {
        let mut flipped = bytes.to_vec();
        flipped[idx] ^= 0x5a;
        probe(&String::from_utf8_lossy(&flipped));
    }
}
