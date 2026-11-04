//! Seeded random and byte-mutated no-panic tests for `fieldbook_core::schema`.

#[path = "../../../tests/support/rng.rs"]
mod rng;

use fieldbook_core::schema::{
    ChangeClass, CorpusToml, PageStatus, PageType, Readiness, Volatility, label_form, parse_meta,
    split,
};
use rng::Rng;

const REAL_CORPUS_TOML: &str = include_str!("../../../corpus/corpus.toml");

const SYNTHETIC_FRONT_MATTER: &str = "\
id = \"cmp-withholding\"
title = \"Payment withholding for business-system deficiencies\"
area = \"cmp\"
subarea = \"business-systems\"
type = \"concept\"
stage = 3
readiness = \"PUBLIC-NOW\"
status = \"draft\"
as_of = 2025-11-01
handling = [\"public\"]
terms = [\"significant_deficiency\"]
volatility = \"V1\"
";

const COMPLETE_FENCED_PAGE: &str = "\
+++
id = \"cmp-withholding\"
title = \"Payment withholding for business-system deficiencies\"
area = \"cmp\"
subarea = \"business-systems\"
type = \"concept\"
stage = 3
readiness = \"PUBLIC-NOW\"
status = \"draft\"
as_of = 2025-11-01
handling = [\"public\"]
terms = [\"significant_deficiency\"]
volatility = \"V1\"
+++

## Purpose

Not established.
";

fn exercise_split(input: &str) {
    match split(input) {
        Ok(s) => {
            assert!(s.body_line() >= 1);
        }
        Err(e) => {
            assert!(e.line() >= 1, "split error line must be >= 1, got {e:?}");
            assert!(e.col() >= 1, "split error col must be >= 1, got {e:?}");
        }
    }
}

fn exercise_parse_meta(input: &str) {
    match parse_meta(input) {
        Ok(m) => {
            assert!(m.field_location("id").is_some());
        }
        Err(e) => {
            assert!(
                e.line() >= 1,
                "parse_meta error line must be >= 1, got {e:?}"
            );
            assert!(e.col() >= 1, "parse_meta error col must be >= 1, got {e:?}");
        }
    }
}

fn exercise_corpus_toml(input: &str) {
    match CorpusToml::parse(input) {
        Ok(c) => {
            let _ = c.validate();
        }
        Err(e) => {
            assert!(
                e.line() >= 1,
                "CorpusToml error line must be >= 1, got {e:?}"
            );
            assert!(e.col() >= 1, "CorpusToml error col must be >= 1, got {e:?}");
        }
    }
}

fn exercise_vocabs(input: &str) {
    let _ = PageType::parse(input);
    let _ = Readiness::parse(input);
    let _ = PageStatus::parse(input);
    let _ = Volatility::parse(input);
    let _ = ChangeClass::parse(input);
    let _ = label_form(input);
}

fn mutate_and_run(seed: &str, f: fn(&str)) {
    f(seed);
    let bytes = seed.as_bytes();
    for idx in 0..bytes.len() {
        let mut flipped = bytes.to_vec();
        flipped[idx] ^= 0xFF;
        let s_flip = String::from_utf8_lossy(&flipped);
        f(&s_flip);

        let mut deleted = Vec::with_capacity(bytes.len() - 1);
        deleted.extend_from_slice(&bytes[..idx]);
        deleted.extend_from_slice(&bytes[idx + 1..]);
        let s_del = String::from_utf8_lossy(&deleted);
        f(&s_del);
    }
}

#[test]
fn random_and_mutated_bytes_never_panic() {
    let mut rng = Rng::new(1);
    for _ in 0..200 {
        let bytes = rng.bytes(96);
        let s = String::from_utf8_lossy(&bytes);
        exercise_split(&s);
        exercise_parse_meta(&s);
        exercise_corpus_toml(&s);
        exercise_vocabs(&s);
    }

    mutate_and_run(COMPLETE_FENCED_PAGE, exercise_split);
    mutate_and_run(SYNTHETIC_FRONT_MATTER, exercise_parse_meta);
    mutate_and_run(REAL_CORPUS_TOML, exercise_corpus_toml);

    for v in PageType::ALL {
        mutate_and_run(v.as_str(), |s| {
            let _ = PageType::parse(s);
        });
    }
    for v in Readiness::ALL {
        mutate_and_run(v.as_str(), |s| {
            let _ = Readiness::parse(s);
        });
    }
    for v in PageStatus::ALL {
        mutate_and_run(v.as_str(), |s| {
            let _ = PageStatus::parse(s);
        });
    }
    for v in Volatility::ALL {
        mutate_and_run(v.as_str(), |s| {
            let _ = Volatility::parse(s);
        });
    }
    for v in ChangeClass::ALL {
        mutate_and_run(v.as_str(), |s| {
            let _ = ChangeClass::parse(s);
        });
    }
}
