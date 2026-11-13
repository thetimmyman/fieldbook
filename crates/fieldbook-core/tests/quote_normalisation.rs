//! Integration tests for quote normalisation, quote search, and locator resolution.
//!
//! **Responsibility.** Validates fixture pairs from `pairs.toml`, asserts normalisation
//! invariants (case preservation, whitespace collapsing, quote/dash folding), verifies
//! bounded page search, and exercises locator-to-page-range resolution against `ExtractionToml`.
//!
//! **Rules.** Pure tests: no filesystem, clock, or environment access.

use fieldbook_core::check::Code;
use fieldbook_core::extraction::ExtractionToml;
use fieldbook_core::quote::{self, Span};
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
    name: String,
    quote: String,
    page: String,
    expect: String,
    matched: Option<String>,
}

#[test]
fn every_fixture_pair_gives_its_expected_result() {
    let fixture: Fixture =
        toml::from_str(FIXTURE_TOML).expect("pairs.toml fixture must parse cleanly");

    for pair in &fixture.pair {
        let span_opt = quote::find(&pair.quote, &[&pair.page], 1..=1);
        match pair.expect.as_str() {
            "match" => {
                let Some(span) = span_opt else {
                    panic!("pair `{}` expected a match, got None", pair.name);
                };
                assert_eq!(
                    span.page, 1,
                    "pair `{}` expected match on page 1, got page {}",
                    pair.name, span.page
                );
                if let Some(expected_matched) = &pair.matched {
                    let actual_matched = &pair.page[span.start..span.end];
                    assert_eq!(
                        actual_matched, expected_matched,
                        "pair `{}` matched slice mismatch: actual `{actual_matched}`, expected `{expected_matched}`",
                        pair.name
                    );
                }
            }
            "none" => {
                assert!(
                    span_opt.is_none(),
                    "pair `{}` expected None, got Some({:?})",
                    pair.name,
                    span_opt
                );
            }
            other => panic!("unknown expect `{other}` in pair `{}`", pair.name),
        }
    }
}

#[test]
fn normalise_folds_quotes_dashes_and_whitespace() {
    // Case is kept
    assert_eq!(quote::normalise("Case Kept"), "Case Kept");
    assert_eq!(quote::normalise("withhold"), "withhold");
    assert_eq!(quote::normalise("Withhold"), "Withhold");

    // Accents are kept
    assert_eq!(quote::normalise("café"), "café");

    // Result is trimmed of leading and trailing whitespace
    assert_eq!(quote::normalise("   trimmed   "), "trimmed");
    assert_eq!(quote::normalise("\t\n  hello \r\n"), "hello");

    // Runs of whitespace become one ASCII space
    assert_eq!(quote::normalise("a \t \n \r  b"), "a b");
    assert_eq!(quote::normalise("a\u{00A0}b"), "a b");

    // Curly quotes fold to straight quotes
    assert_eq!(
        quote::normalise("\u{2018}single\u{2019} and \u{201C}double\u{201D}"),
        "'single' and \"double\""
    );

    // Dashes fold to hyphen-minus
    assert_eq!(
        quote::normalise("en\u{2013}dash em\u{2014}dash minus\u{2212}sign"),
        "en-dash em-dash minus-sign"
    );
}

#[test]
fn find_searches_only_pages_in_range() {
    let page1 = "The target quote appears exclusively here on the opening page.";
    let page2 = "Middle text discusses unrelated procedural questions.";
    let page3 = "Closing remarks with the target quote repeated here.";
    let pages = [page1, page2, page3];

    // Quote on page 1 only: searching 2..=3 yields None
    assert_eq!(
        quote::find("exclusively here", &pages, 2..=3),
        None,
        "searching pages 2..=3 must not see page 1"
    );

    // Searching 1..=1 finds page 1
    assert_eq!(
        quote::find("exclusively here", &pages, 1..=1),
        Some(Span {
            page: 1,
            start: 25,
            end: 41
        })
    );

    // The quote on pages 1 and 3 with range 3..=3 returns page 3
    let hit_p3 = quote::find("target quote", &pages, 3..=3);
    assert_eq!(
        hit_p3,
        Some(Span {
            page: 3,
            start: 25,
            end: 37
        })
    );
}

#[test]
fn find_returns_absolute_page_numbers() {
    let p1 = "First page content.";
    let p2 = "Second page contains unique target text here.";
    let p3 = "Third page content.";
    let pages = [p1, p2, p3];

    let span =
        quote::find("unique target text", &pages, 1..=3).expect("must find target text on page 2");
    assert_eq!(span.page, 2);
    assert_eq!(&p2[span.start..span.end], "unique target text");

    let span_subrange = quote::find("unique target text", &pages, 2..=2)
        .expect("must find target text within 2..=2");
    assert_eq!(span_subrange.page, 2);
}

#[test]
fn locator_in_range_resolves() {
    let extraction =
        ExtractionToml::parse(SAMPLE_EXTRACTION_TOML).expect("SAMPLE_EXTRACTION_TOML must parse");

    assert_eq!(
        quote::locator_resolves(&extraction, "p. 1"),
        Ok(Some(1..=1))
    );
    assert_eq!(
        quote::locator_resolves(&extraction, "p. 3"),
        Ok(Some(3..=3))
    );
    assert_eq!(
        quote::locator_resolves(&extraction, "pp. 2-3"),
        Ok(Some(2..=3))
    );
    assert_eq!(
        quote::locator_resolves(&extraction, "sec. (e)(1)"),
        Ok(Some(2..=2))
    );
    assert_eq!(
        quote::locator_resolves(&extraction, "§ 15.404-1"),
        Ok(Some(2..=3))
    );
}

#[test]
fn page_beyond_page_count_is_fb0304() {
    let extraction =
        ExtractionToml::parse(SAMPLE_EXTRACTION_TOML).expect("SAMPLE_EXTRACTION_TOML must parse");

    assert_eq!(
        quote::locator_resolves(&extraction, "p. 4"),
        Err(Code::Fb0304)
    );
    assert_eq!(
        quote::locator_resolves(&extraction, "pp. 3-4"),
        Err(Code::Fb0304)
    );
}

#[test]
fn known_label_resolves_and_unknown_is_fb0304() {
    let extraction =
        ExtractionToml::parse(SAMPLE_EXTRACTION_TOML).expect("SAMPLE_EXTRACTION_TOML must parse");

    // Known labels resolve
    assert_eq!(
        quote::locator_resolves(&extraction, "sec. (e)(1)"),
        Ok(Some(2..=2))
    );

    // Unknown label
    assert_eq!(
        quote::locator_resolves(&extraction, "sec. (z)(9)"),
        Err(Code::Fb0304)
    );

    // Case difference
    assert_eq!(
        quote::locator_resolves(&extraction, "sec. (E)(1)"),
        Err(Code::Fb0304)
    );

    // Prefix match (not exact)
    assert_eq!(
        quote::locator_resolves(&extraction, "sec. 15.404"),
        Err(Code::Fb0304)
    );
}

#[test]
fn table_locators_are_accepted_and_not_read() {
    let extraction =
        ExtractionToml::parse(SAMPLE_EXTRACTION_TOML).expect("SAMPLE_EXTRACTION_TOML must parse");

    assert_eq!(
        quote::locator_resolves(&extraction, "sheet Data!A1"),
        Ok(None)
    );
    assert_eq!(quote::locator_resolves(&extraction, "rec 7"), Ok(None));
    assert_eq!(
        quote::locator_resolves(&extraction, "field price"),
        Ok(None)
    );
    assert_eq!(
        quote::locator_resolves(&extraction, "ext structured_path abc"),
        Ok(None)
    );
    assert_eq!(quote::locator_resolves(&extraction, "page one"), Ok(None));
}
