//! Seeded random and mutated byte tests for all identifier parsers.

#[path = "../../../tests/support/rng.rs"]
mod rng;

use fieldbook_core::ids::{
    AdrId, AreaCode, CaseId, ClaimId, ConflictId, EditionId, FAMILIES, IdError, PageId, QuestionId,
    ReleaseId, SourceId, TermId, VersionId, is_error_code,
};
use rng::Rng;

fn check_parse_round_trip<T, F>(input: &str, parse: F)
where
    T: core::fmt::Display,
    F: FnOnce(&str) -> Result<T, IdError>,
{
    match parse(input) {
        Ok(parsed) => {
            assert_eq!(
                parsed.to_string(),
                input,
                "accepted id did not print back to input {input:?}"
            );
        }
        Err(err) => {
            assert!(!err.form().is_empty());
            assert_eq!(err.input(), input);
        }
    }
}

fn feed_all_parsers(input: &str) {
    check_parse_round_trip(input, AreaCode::parse);
    check_parse_round_trip(input, PageId::parse);
    check_parse_round_trip(input, SourceId::parse);
    check_parse_round_trip(input, VersionId::parse);
    check_parse_round_trip(input, ClaimId::parse);
    check_parse_round_trip(input, QuestionId::parse);
    check_parse_round_trip(input, ConflictId::parse);
    check_parse_round_trip(input, TermId::parse);
    check_parse_round_trip(input, CaseId::parse);
    check_parse_round_trip(input, AdrId::parse);
    check_parse_round_trip(input, EditionId::parse);
    check_parse_round_trip(input, ReleaseId::parse);
    let _ = is_error_code(input);
}

fn feed_family_parser(family_name: &str, input: &str) {
    match family_name {
        "area" => check_parse_round_trip(input, AreaCode::parse),
        "page" => check_parse_round_trip(input, PageId::parse),
        "source" => check_parse_round_trip(input, SourceId::parse),
        "source_version" => check_parse_round_trip(input, VersionId::parse),
        "claim" => check_parse_round_trip(input, ClaimId::parse),
        "question" => check_parse_round_trip(input, QuestionId::parse),
        "conflict" => check_parse_round_trip(input, ConflictId::parse),
        "term" => check_parse_round_trip(input, TermId::parse),
        "case" => check_parse_round_trip(input, CaseId::parse),
        "error_code" => {
            let _ = is_error_code(input);
        }
        "adr" => check_parse_round_trip(input, AdrId::parse),
        "edition" => check_parse_round_trip(input, EditionId::parse),
        "tool_release" => check_parse_round_trip(input, ReleaseId::parse),
        other => panic!("unknown family {other}"),
    }
}

fn mutations_of(seed: &str) -> Vec<Vec<u8>> {
    let bytes = seed.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 2 + 1);
    out.push(bytes.to_vec());
    for i in 0..bytes.len() {
        let mut flipped = bytes.to_vec();
        flipped[i] ^= 0x5A;
        out.push(flipped);

        // Also flip the low bit so ASCII digits become adjacent bytes (`0` -> `1` or `/` -> `.`).
        let mut flipped_low = bytes.to_vec();
        flipped_low[i] ^= 0x1F;
        out.push(flipped_low);

        let mut deleted = Vec::with_capacity(bytes.len() - 1);
        deleted.extend_from_slice(&bytes[..i]);
        deleted.extend_from_slice(&bytes[i + 1..]);
        out.push(deleted);
    }
    out
}

#[test]
fn random_and_mutated_bytes_never_panic() {
    let mut rng = Rng::new(1);
    for _ in 0..200 {
        let len = usize::try_from(rng.next_u64() % 65).unwrap_or(0);
        let raw = rng.bytes(len);
        let s = String::from_utf8_lossy(&raw);
        feed_all_parsers(&s);
    }

    for family in &FAMILIES {
        for mutated in mutations_of(family.example) {
            let s = String::from_utf8_lossy(&mutated);
            feed_family_parser(family.name, &s);
        }
    }

    for mutated in mutations_of("cmp") {
        let s = String::from_utf8_lossy(&mutated);
        check_parse_round_trip(&s, AreaCode::parse);
    }

    for mutated in mutations_of("FB0100") {
        let s = String::from_utf8_lossy(&mutated);
        let _ = is_error_code(&s);
    }
}
