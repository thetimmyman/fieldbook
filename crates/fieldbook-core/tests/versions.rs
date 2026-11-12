//! Tests for retained version implementations (`ai-kernel` step 15).

use std::error::Error;

use fieldbook_core::audit::Word;
use fieldbook_core::authz::{Act, AuthzDecision, Groups};
use fieldbook_core::extraction::{ExtractError, Extractor, HtmlStrip, Plain};
use fieldbook_core::versions::{extractor_v1, html_strip_v1, plain_v1, policy_readers_v1};

const SAMPLE_HTML: &[u8] = include_bytes!("../../../tests/fixtures/extract/html-strip/sample.html");
const EMPTY_INPUT: &[u8] = b"";
const ASCII_INPUT: &[u8] = b"Hello, world!\nSecond line of plain ASCII text.\n";
const BOM_INPUT: &[u8] = b"\xef\xbb\xbfText with a UTF-8 BOM.\nSecond line.\n";
const INVALID_UTF8: &[u8] = &[0xff, 0xfe];

struct ScriptedGroups {
    answer: Option<bool>,
}

impl Groups for ScriptedGroups {
    fn in_group(&self, _group: &str) -> Option<bool> {
        self.answer
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GroupScenario {
    NoRequirement,
    Member,
    NonMember,
    Unreadable,
}

#[test]
fn plain_v1_matches_the_current_extractor() -> Result<(), Box<dyn Error>> {
    let cases: &[(&str, &[u8])] = &[
        ("empty", EMPTY_INPUT),
        ("ascii", ASCII_INPUT),
        ("bom", BOM_INPUT),
        ("invalid_utf8", INVALID_UTF8),
        ("html_sample", SAMPLE_HTML),
    ];

    let current = Plain;
    for (label, case) in cases {
        let expected = current.extract(case);
        let actual = plain_v1::extract(case);
        assert_eq!(
            actual, expected,
            "plain_v1 mismatch for test case '{label}'"
        );
    }

    // Verify invalid UTF-8 specifically yields ExtractError::NotText
    assert_eq!(
        plain_v1::extract(INVALID_UTF8),
        Err(ExtractError::NotText),
        "invalid UTF-8 must yield ExtractError::NotText"
    );

    Ok(())
}

#[test]
fn html_strip_v1_matches_the_current_extractor() -> Result<(), Box<dyn Error>> {
    let cases: &[(&str, &[u8])] = &[
        ("empty", EMPTY_INPUT),
        ("ascii", ASCII_INPUT),
        ("bom", BOM_INPUT),
        ("invalid_utf8", INVALID_UTF8),
        ("html_sample", SAMPLE_HTML),
    ];

    let current = HtmlStrip;
    for (label, case) in cases {
        let expected = current.extract(case);
        let actual = html_strip_v1::extract(case);
        assert_eq!(
            actual, expected,
            "html_strip_v1 mismatch for test case '{label}'"
        );
    }

    // Verify invalid UTF-8 specifically yields ExtractError::NotText
    assert_eq!(
        html_strip_v1::extract(INVALID_UTF8),
        Err(ExtractError::NotText),
        "invalid UTF-8 must yield ExtractError::NotText"
    );

    // Verify that the HTML sample stripped content matches expectations
    let extraction = html_strip_v1::extract(SAMPLE_HTML)?;
    assert_eq!(extraction.pages.len(), 1);
    let page_text = &extraction.pages[0].text;
    assert!(
        page_text.contains("Synthetic Heading"),
        "HTML strip should include heading text"
    );
    assert!(
        page_text.contains("Synthetic Café"),
        "HTML strip should include link text"
    );
    assert!(
        !page_text.contains("synthetic script"),
        "HTML strip should strip script content"
    );

    Ok(())
}

#[test]
fn extractor_v1_dispatch_knows_only_the_published_ids() -> Result<(), Box<dyn Error>> {
    let plain_fn = extractor_v1("plain@1");
    assert!(plain_fn.is_some(), "plain@1 must be known to extractor_v1");
    if let Some(f) = plain_fn {
        assert_eq!(f(ASCII_INPUT), plain_v1::extract(ASCII_INPUT));
        assert_eq!(f(INVALID_UTF8), plain_v1::extract(INVALID_UTF8));
    }

    let html_strip_fn = extractor_v1("html-strip@1");
    assert!(
        html_strip_fn.is_some(),
        "html-strip@1 must be known to extractor_v1"
    );
    if let Some(f) = html_strip_fn {
        assert_eq!(f(SAMPLE_HTML), html_strip_v1::extract(SAMPLE_HTML));
        assert_eq!(f(INVALID_UTF8), html_strip_v1::extract(INVALID_UTF8));
    }

    assert!(
        extractor_v1("pdftotext-layout@1").is_none(),
        "pdftotext-layout@1 must be None in fieldbook-core"
    );
    assert!(
        extractor_v1("plain@2").is_none(),
        "plain@2 must be None in extractor_v1"
    );
    assert!(extractor_v1("html-strip@2").is_none());
    assert!(extractor_v1("folder@1").is_none());
    assert!(extractor_v1("").is_none());
    assert!(extractor_v1("unknown").is_none());

    Ok(())
}

#[test]
fn policy_readers_v1_follows_readers_and_changers() -> Result<(), Box<dyn Error>> {
    let all_acts = [
        Act::Read,
        Act::Admin,
        Act::Propose,
        Act::Accept,
        Act::Export,
    ];

    let all_scenarios = [
        GroupScenario::NoRequirement,
        GroupScenario::Member,
        GroupScenario::NonMember,
        GroupScenario::Unreadable,
    ];

    for act in all_acts {
        for scenario in all_scenarios {
            let (readers, changers, groups) = match scenario {
                GroupScenario::NoRequirement => (None, None, ScriptedGroups { answer: None }),
                GroupScenario::Member => (
                    Some("readers-grp"),
                    Some("changers-grp"),
                    ScriptedGroups { answer: Some(true) },
                ),
                GroupScenario::NonMember => (
                    Some("readers-grp"),
                    Some("changers-grp"),
                    ScriptedGroups {
                        answer: Some(false),
                    },
                ),
                GroupScenario::Unreadable => (
                    Some("readers-grp"),
                    Some("changers-grp"),
                    ScriptedGroups { answer: None },
                ),
            };

            let decision = policy_readers_v1::allow(act, readers, changers, &groups);

            let expected = match (act, scenario) {
                (Act::Read | Act::Admin, GroupScenario::NonMember | GroupScenario::Unreadable) => {
                    AuthzDecision::Deny(Word::new_static("authz"))
                }
                (Act::Read | Act::Admin, GroupScenario::NoRequirement | GroupScenario::Member)
                | (Act::Propose | Act::Accept | Act::Export, _) => AuthzDecision::Allow,
            };

            assert_eq!(
                decision, expected,
                "mismatch for act {act:?} in scenario {scenario:?}"
            );

            if let AuthzDecision::Deny(ref word) = decision {
                assert_eq!(word.as_str(), "authz");
            }
        }
    }

    // Additional cross-checks:
    // 1. Act::Read ignores changers requirement:
    let allow_read_despite_changers = policy_readers_v1::allow(
        Act::Read,
        Some("readers-grp"),
        Some("changers-grp"),
        &ScriptedGroups { answer: Some(true) },
    );
    assert_eq!(allow_read_despite_changers, AuthzDecision::Allow);

    // 2. Act::Admin ignores readers requirement:
    let allow_admin_despite_readers = policy_readers_v1::allow(
        Act::Admin,
        Some("readers-grp"),
        Some("changers-grp"),
        &ScriptedGroups { answer: Some(true) },
    );
    assert_eq!(allow_admin_despite_readers, AuthzDecision::Allow);

    // 3. When no readers requirement, even non-member or unreadable is allowed:
    for answer in [Some(true), Some(false), None] {
        let decision = policy_readers_v1::allow(
            Act::Read,
            None,
            Some("changers-grp"),
            &ScriptedGroups { answer },
        );
        assert_eq!(
            decision,
            AuthzDecision::Allow,
            "Read without readers requirement must allow regardless of groups"
        );
    }

    // 4. When no changers requirement, even non-member or unreadable is allowed:
    for answer in [Some(true), Some(false), None] {
        let decision = policy_readers_v1::allow(
            Act::Admin,
            Some("readers-grp"),
            None,
            &ScriptedGroups { answer },
        );
        assert_eq!(
            decision,
            AuthzDecision::Allow,
            "Admin without changers requirement must allow regardless of groups"
        );
    }

    Ok(())
}
