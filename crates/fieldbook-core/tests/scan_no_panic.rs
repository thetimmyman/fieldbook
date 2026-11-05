//! Seeded random and byte-mutated no-panic tests for `fieldbook_core::scan`.

#[path = "../../../tests/support/rng.rs"]
mod rng;

use fieldbook_core::scan::{allow, claim_language, formula, identity, marking};
use rng::Rng;

const VALID_SCAN_ALLOW: &str = "\
[[allow]]
sha256 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"
match = \"CONTROLLED UNCLASSIFIED INFORMATION\"
reason = \"Public regulation quoting the marking banner in full.\"
";

fn exercise_all(input: &str) {
    match allow::parse(input) {
        Ok(allowances) => {
            for entry in allowances.entries() {
                assert_eq!(entry.sha256.len(), 64);
                assert!(!entry.matched.is_empty());
                assert!(!entry.reason.trim().is_empty());
            }
        }
        Err(errors) => {
            assert!(!errors.is_empty());
            for err in errors {
                assert!(err.line >= 1);
                assert!(err.col >= 1);
                assert!(!err.message.is_empty());
            }
        }
    }

    for f in marking::scan(input) {
        assert!(f.span.start < f.span.end && f.span.end <= input.len());
    }
    for f in identity::scan(input) {
        assert!(f.span.start < f.span.end && f.span.end <= input.len());
    }
    for f in claim_language::scan(input) {
        assert!(f.span.start < f.span.end && f.span.end <= input.len());
    }
    if let Some(f) = formula::scan_cell(input) {
        assert!(f.span.start < f.span.end && f.span.end <= input.len());
    }
}

fn run_seeded_and_mutated() {
    let mut prng = Rng::new(1);
    for _ in 0..200 {
        let len = usize::try_from(prng.next_u64() % 129).unwrap_or(0);
        let raw = prng.bytes(len);
        let text = String::from_utf8_lossy(&raw);
        exercise_all(&text);
    }

    exercise_all(VALID_SCAN_ALLOW);
    let bytes = VALID_SCAN_ALLOW.as_bytes();
    for idx in 0..bytes.len() {
        let mut flipped = bytes.to_vec();
        flipped[idx] ^= 0x5A;
        let text_flip = String::from_utf8_lossy(&flipped);
        exercise_all(&text_flip);

        let mut deleted = Vec::with_capacity(bytes.len() - 1);
        deleted.extend_from_slice(&bytes[..idx]);
        deleted.extend_from_slice(&bytes[idx + 1..]);
        let text_del = String::from_utf8_lossy(&deleted);
        exercise_all(&text_del);
    }
}

#[test]
fn parser_never_panics_on_200_seeded_inputs() {
    run_seeded_and_mutated();
}

mod scan_no_panic {
    #[test]
    fn parser_never_panics_on_200_seeded_inputs() {
        super::run_seeded_and_mutated();
    }
}
