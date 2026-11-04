//! Property and mutation test ensuring `cite::lexer` never panics and always produces in-bounds spans.

#[path = "../../../tests/support/rng.rs"]
mod rng;

use fieldbook_core::check::Code;
use fieldbook_core::cite::{Mode, Span, lex};
use rng::Rng;

fn assert_span_in_bounds(text: &str, span: Span) {
    assert!(
        span.start <= span.end,
        "span start {} > end {} on {text:?}",
        span.start,
        span.end
    );
    assert!(
        span.end <= text.len(),
        "span end {} > text.len() {} on {text:?}",
        span.end,
        text.len()
    );
    let max_line = 1 + text.bytes().filter(|&b| b == b'\n').count();
    assert!(
        span.line >= 1 && span.line <= max_line,
        "span line {} outside 1..={} on {text:?}",
        span.line,
        max_line
    );
    assert!(span.col >= 1, "span col {} < 1 on {text:?}", span.col);
}

fn check_input(text: &str) {
    for mode in [Mode::Strict, Mode::Tolerant] {
        let out = lex(text, mode);
        for cite in &out.citations {
            assert_span_in_bounds(text, cite.span);
        }
        for finding in &out.findings {
            assert_span_in_bounds(text, finding.span);
        }
    }
}

#[test]
fn random_and_mutated_bytes_never_panic() {
    // 1. Explicit boundary input `[@`: in each mode it returns no citations, an FB0300 finding and in-bounds spans.
    for mode in [Mode::Strict, Mode::Tolerant] {
        let out = lex("[@", mode);
        assert!(
            out.citations.is_empty(),
            "boundary `[@` must return no citations in {mode:?}"
        );
        assert_eq!(out.findings.len(), 1);
        assert_eq!(out.findings[0].code, Code::Fb0300);
        assert_span_in_bounds("[@", out.findings[0].span);
    }

    // 2. 200 byte strings of length 0 to 96 from Rng::new(1).
    let mut prng = Rng::new(1);
    for _ in 0..200 {
        let len = usize::try_from(prng.next_u64() % 97).unwrap_or(0);
        let raw = prng.bytes(len);
        let text = String::from_utf8_lossy(&raw);
        check_input(&text);
    }

    // 3. Valid citation `[@S-0042-v2, p. 14]` and the input of `cit-s-024` (`[@S-0042-v2 p. 14]`)
    // with each byte once flipped and once deleted.
    for seed in ["[@S-0042-v2, p. 14]", "[@S-0042-v2 p. 14]"] {
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
}
