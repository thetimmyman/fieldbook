//! Deterministic fuzz and contract verification for quote matching and normalisation.
//!
//! **Responsibility.** Verifies that `normalise`, `find`, and `locator_resolves`
//! never panic on seeded random and byte-mutated inputs, and verifies that `normalise`
//! is idempotent.
//!
//! **Rules.** Pure tests: no filesystem, clock, or environment access.

#[path = "../../../tests/support/rng.rs"]
mod rng;

use fieldbook_core::extraction::ExtractionToml;
use fieldbook_core::quote;
use rng::Rng;
use serde::Deserialize;

const FIXTURE_TOML: &str = include_str!("../../../tests/fixtures/quote-normalisation/pairs.toml");

const SAMPLE_EXTRACTION_TOML: &str = "\
tool = \"pdftotext\"
tool_version = \"0.0.0\"
method = \"pdftotext-layout\"
page_count = 3
thin_pages = []
original_sha256 = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"

[pages]
p0001 = \"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"
p0002 = \"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc\"
p0003 = \"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd\"

[locators]
\"(e)(1)\" = 2
\"15.404-1\" = [2, 3]
";

#[derive(Debug, Deserialize)]
struct Fixture {
    pair: Vec<Pair>,
}

#[derive(Debug, Deserialize)]
struct Pair {
    quote: String,
}

#[test]
fn parser_never_panics_on_200_seeded_inputs() {
    let extraction =
        ExtractionToml::parse(SAMPLE_EXTRACTION_TOML).expect("SAMPLE_EXTRACTION_TOML must parse");

    let fixture: Fixture = toml::from_str(FIXTURE_TOML).expect("pairs.toml fixture must parse");
    let fixture_pair = fixture
        .pair
        .first()
        .expect("fixture must contain at least one pair");
    let fixture_quote = &fixture_pair.quote;

    let base_locator = "§ 15.404-1";

    let page1 = "Introductory synthetic section on billing and verification rules.";
    let page2 = format!("The primary requirement states {fixture_quote} across all accounts.");
    let page3 = "Concluding synthetic audit guidance and dispute escalation rules.";
    let three_pages = [page1, page2.as_str(), page3];

    let exercise_input = |text: &str| {
        // 1. normalise and idempotency check
        let norm = quote::normalise(text);
        let norm_again = quote::normalise(&norm);
        assert_eq!(
            norm, norm_again,
            "normalise must be idempotent for input: {text:?}"
        );

        // 2. find against a one-page set whose only page is text with range 1..=1
        let _ = quote::find(text, &[text], 1..=1);
        let _ = quote::find(fixture_quote, &[text], 1..=1);

        // 3. find against three page texts of a small made-up extraction with range 2..=3
        let _ = quote::find(text, &three_pages, 2..=3);

        // 4. locator_resolves against extraction
        let _ = quote::locator_resolves(&extraction, text);
    };

    // 200 byte strings of length i % 129 from Rng::new(1)
    let mut rng = Rng::new(1);
    for i in 0..200usize {
        let len = i % 129;
        let bytes = rng.bytes(len);
        let text = String::from_utf8_lossy(&bytes);
        exercise_input(&text);
    }

    // Mutate fixture quote and base locator
    for base_str in [fixture_quote.as_str(), base_locator] {
        let base_bytes = base_str.as_bytes();
        for idx in 0..base_bytes.len() {
            // Byte flipped
            let mut flipped = base_bytes.to_vec();
            if let Some(b) = flipped.get_mut(idx) {
                *b ^= 0xff;
            }
            let text = String::from_utf8_lossy(&flipped);
            exercise_input(&text);

            // Byte deleted
            let mut deleted = Vec::with_capacity(base_bytes.len().saturating_sub(1));
            deleted.extend_from_slice(&base_bytes[..idx]);
            if idx.saturating_add(1) <= base_bytes.len() {
                deleted.extend_from_slice(&base_bytes[idx + 1..]);
            }
            let text = String::from_utf8_lossy(&deleted);
            exercise_input(&text);
        }
    }
}
