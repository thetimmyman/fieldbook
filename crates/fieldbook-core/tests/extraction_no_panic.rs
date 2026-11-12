//! Deterministic fuzz and contract verification for text extraction.
//!
//! **Responsibility.** Verifies that `ExtractionToml::parse`, `HtmlStrip::extract`,
//! and `supplied_text` never panic on seeded random and byte-mutated inputs,
//! validates extraction round-tripping, and asserts adapter conformance to the
//! shared extractor contract.
//!
//! **Rules.** Pure tests: no filesystem, clock, or environment access.

#[path = "../../../tests/support/extractor_suite.rs"]
mod extractor_suite;
#[expect(
    clippy::duplicate_mod,
    reason = "rng is included by both this test and extractor_suite"
)]
#[path = "../../../tests/support/rng.rs"]
mod rng;

use fieldbook_core::extraction::methods::{HTML_STRIP, PLAIN};
use fieldbook_core::extraction::{
    ExtractError, Extraction, ExtractionToml, Extractor, HtmlStrip, Plain, supplied_text,
};
use rng::Rng;

const VALID_EXTRACTION_TOML: &str = "\
tool = \"pdftotext\"
tool_version = \"26.10.0\"
method = \"pdftotext-layout\"
page_count = 3
thin_pages = [3]
original_sha256 = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"

[pages]
p0001 = \"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"
p0002 = \"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc\"
p0003 = \"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd\"

[locators]
\"(e)(1)\" = 2
\"15.404-1\" = [2, 3]
";

#[test]
fn the_valid_extraction_toml_parses() {
    let record =
        ExtractionToml::parse(VALID_EXTRACTION_TOML).expect("VALID_EXTRACTION_TOML should parse");
    let rendered = record.render();
    let reparsed =
        ExtractionToml::parse(&rendered).expect("rendered extraction TOML should re-parse");
    assert_eq!(reparsed, record);
}

#[test]
fn parser_never_panics_on_200_seeded_inputs() {
    let mut rng = Rng::new(1);
    for i in 0..200usize {
        let len = i % 129;
        let bytes = rng.bytes(len);
        let text = String::from_utf8_lossy(&bytes);
        if let Err(err) = ExtractionToml::parse(&text) {
            assert!(err.line >= 1, "error line must be >= 1");
            assert!(err.col >= 1, "error col must be >= 1");
        }
    }

    let mut any_rejected = false;
    let mut any_accepted = false;
    let valid_bytes = VALID_EXTRACTION_TOML.as_bytes();

    for idx in 0..valid_bytes.len() {
        let mut flipped = valid_bytes.to_vec();
        flipped[idx] ^= 0xff;
        let text = String::from_utf8_lossy(&flipped);
        match ExtractionToml::parse(&text) {
            Ok(_) => any_accepted = true,
            Err(err) => {
                any_rejected = true;
                assert!(err.line >= 1, "error line must be >= 1");
                assert!(err.col >= 1, "error col must be >= 1");
            }
        }

        let mut deleted = Vec::with_capacity(valid_bytes.len().saturating_sub(1));
        deleted.extend_from_slice(&valid_bytes[..idx]);
        deleted.extend_from_slice(&valid_bytes[idx + 1..]);
        let text = String::from_utf8_lossy(&deleted);
        match ExtractionToml::parse(&text) {
            Ok(_) => any_accepted = true,
            Err(err) => {
                any_rejected = true;
                assert!(err.line >= 1, "error line must be >= 1");
                assert!(err.col >= 1, "error col must be >= 1");
            }
        }
    }

    assert!(any_rejected, "at least one mutated input must be rejected");
    assert!(any_accepted, "at least one mutated input must be accepted");
}

fn check_html_strip_outcome(res: Result<Extraction, ExtractError>) {
    match res {
        Ok(extraction) => {
            assert_eq!(extraction.method, HTML_STRIP);
            for (idx, page) in extraction.pages.iter().enumerate() {
                let expected_num = u32::try_from(idx + 1).unwrap_or(0);
                assert_eq!(page.number, expected_num, "page numbers must start at 1");
            }
        }
        Err(err) => {
            let _ = format!("{err}");
        }
    }
}

#[test]
fn html_strip_never_panics_on_seeded_inputs() {
    let extractor = HtmlStrip;
    let mut rng = Rng::new(2);

    for i in 0..200usize {
        let len = i % 129;
        let bytes = rng.bytes(len);
        check_html_strip_outcome(extractor.extract(&bytes));
    }

    const VALID_HTML: &str = "<html><body><p class=\"a>b\" title='x'>caf\u{e9} na\u{ef}ve \u{4e2d}\u{6587} &amp; &#x41; &unknown;</p><script>var s=\"<p>\";</script><!-- note --><div>end</div></body></html>";
    let valid_bytes = VALID_HTML.as_bytes();

    for idx in 0..valid_bytes.len() {
        let mut flipped = valid_bytes.to_vec();
        flipped[idx] ^= 0xff;
        check_html_strip_outcome(extractor.extract(&flipped));

        let mut deleted = Vec::with_capacity(valid_bytes.len().saturating_sub(1));
        deleted.extend_from_slice(&valid_bytes[..idx]);
        deleted.extend_from_slice(&valid_bytes[idx + 1..]);
        check_html_strip_outcome(extractor.extract(&deleted));
    }

    let hostile_inputs: [&[u8]; 8] = [
        b"<",
        b"<a",
        b"<a href=\"",
        b"<!--",
        b"<script>",
        b"&#",
        b"&#x",
        b"&#99999999999;",
    ];
    for hostile in hostile_inputs {
        check_html_strip_outcome(extractor.extract(hostile));
    }

    let run_of_lt = vec![b'<'; 100_000];
    check_html_strip_outcome(extractor.extract(&run_of_lt));

    let repeated_p = "<p".repeat(10_000).into_bytes();
    check_html_strip_outcome(extractor.extract(&repeated_p));
}

fn check_supplied_outcome(res: Result<Extraction, ExtractError>) {
    match res {
        Ok(extraction) => {
            assert!(
                !extraction.pages.is_empty(),
                "result must have at least one page"
            );
            for (idx, page) in extraction.pages.iter().enumerate() {
                let expected_num = u32::try_from(idx + 1).unwrap_or(0);
                assert_eq!(
                    page.number, expected_num,
                    "page numbers must run 1.. in order"
                );
            }
        }
        Err(err) => {
            let _ = format!("{err}");
        }
    }
}

#[test]
fn supplied_text_never_panics_on_seeded_inputs() {
    let mut rng = Rng::new(3);

    for i in 0..200usize {
        let len = i % 129;
        let bytes = rng.bytes(len);
        check_supplied_outcome(supplied_text(&bytes));
    }

    const VALID_SUPPLIED: &str = "\u{feff}a\r\n[page break]\r\n\r\n[page break]\nb\rc\n";
    let valid_bytes = VALID_SUPPLIED.as_bytes();

    for idx in 0..valid_bytes.len() {
        let mut flipped = valid_bytes.to_vec();
        flipped[idx] ^= 0xff;
        check_supplied_outcome(supplied_text(&flipped));

        let mut deleted = Vec::with_capacity(valid_bytes.len().saturating_sub(1));
        deleted.extend_from_slice(&valid_bytes[..idx]);
        deleted.extend_from_slice(&valid_bytes[idx + 1..]);
        check_supplied_outcome(supplied_text(&deleted));
    }
}

#[test]
fn extractor_refusals_match_adapter_contract() {
    let plain = Plain;
    let vectors = extractor_suite::text_vectors();
    extractor_suite::extractor_contract(&plain, &vectors, &|extraction| {
        if extraction.method == PLAIN {
            Ok(())
        } else {
            Err(format!(
                "expected method {PLAIN}, got {}",
                extraction.method
            ))
        }
    });

    let html_strip = HtmlStrip;
    extractor_suite::extractor_contract(&html_strip, &vectors, &|extraction| {
        if extraction.method == HTML_STRIP {
            Ok(())
        } else {
            Err(format!(
                "expected method {HTML_STRIP}, got {}",
                extraction.method
            ))
        }
    });
}

#[test]
fn plain_and_html_strip_extract_a_valid_sample() {
    let plain = Plain;
    let extraction = plain.extract(b"x").expect("plain extract should succeed");
    assert_eq!(extraction.method, PLAIN);
    assert_eq!(extraction.pages.len(), 1);
    assert_eq!(extraction.pages[0].number, 1);
    assert_eq!(extraction.pages[0].text, "x");

    let html_strip = HtmlStrip;
    let html_extraction = html_strip
        .extract(b"<p>x</p>")
        .expect("html_strip extract should succeed");
    assert_eq!(html_extraction.method, HTML_STRIP);
    assert_eq!(html_extraction.pages.len(), 1);
    assert_eq!(html_extraction.pages[0].number, 1);
    assert_eq!(html_extraction.pages[0].text, "x\n");
}
