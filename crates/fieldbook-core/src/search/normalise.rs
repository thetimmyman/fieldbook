//! Search text normalisation (`docs/reference/search.md#normalisation`).
//!
//! One pass applies, in order, on both the index side and the query side:
//! 1. Unicode NFKC.
//! 2. Full Unicode lowercase (`str::to_lowercase`).
//! 3. Fold `U+2010..=U+2015` and `U+2212` to ASCII `-` (`U+002D`).
//! 4. Delete `U+00AD` (soft hyphen) together with the run of combining marks
//!    (`General_Category=Mark`) that immediately follows it.
//!
//! Passes repeat until one leaves the text unchanged, so `normalise` is idempotent: lowercasing
//! can leave text that NFKC composes or reorders (`J` U+030C, U+0130 U+0316), and a mark that
//! a deleted soft hyphen would otherwise leave beside a letter is deleted with it.

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Normalises `input` according to `docs/reference/search.md#normalisation`.
#[must_use]
pub fn normalise(input: &str) -> String {
    let mut text = pass(input);
    loop {
        let next = pass(&text);
        if next == text {
            return text;
        }
        text = next;
    }
}

/// One pass of steps 1 to 4.
fn pass(input: &str) -> String {
    let nfkc: String = input.nfkc().collect();
    let lower = nfkc.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut after_soft_hyphen = false;
    for ch in lower.chars() {
        if after_soft_hyphen && is_combining_mark(ch) {
            continue;
        }
        after_soft_hyphen = ch == '\u{00AD}';
        match ch {
            '\u{00AD}' => {}
            '\u{2010}'..='\u{2015}' | '\u{2212}' => out.push('-'),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::normalise;

    #[test]
    fn nfkc_folds_ligatures_fullwidth_superscripts_and_combining_accents() {
        assert_eq!(normalise("\u{fb01}le \u{ff11}\u{ff12}\u{ff13}"), "file 123");
        assert_eq!(normalise("x\u{00b2} cm\u{00b3}"), "x2 cm3");
        assert_eq!(normalise("Cafe\u{0301}"), "caf\u{00e9}");
        assert_eq!(normalise("9904.401\u{ff0d}40"), "9904.401-40");
    }

    #[test]
    fn lowercase_folds_ascii_and_unicode_and_keeps_accents() {
        assert_eq!(
            normalise("CAF\u{00c9} \u{00c9}T\u{00c9}"),
            "caf\u{00e9} \u{00e9}t\u{00e9}"
        );
        assert_eq!(normalise("DFARS Safeguarding"), "dfars safeguarding");
    }

    #[test]
    fn folds_specified_dashes_and_leaves_outside_characters_untouched() {
        for dash in [
            '\u{2010}', '\u{2011}', '\u{2012}', '\u{2013}', '\u{2014}', '\u{2015}', '\u{2212}',
        ] {
            let s = format!("252.204{dash}7012");
            assert_eq!(
                normalise(&s),
                "252.204-7012",
                "dash U+{:04X} must fold to '-'",
                u32::from(dash)
            );
        }
        // U+2016 (double vertical line) is just outside U+2010..=U+2015 and is not folded.
        assert_eq!(normalise("15.404\u{2016}1"), "15.404\u{2016}1");
    }

    #[test]
    fn deletes_soft_hyphen() {
        assert_eq!(normalise("con\u{00ad}tractor"), "contractor");
        assert_eq!(normalise("\u{00ad}start\u{00ad}end\u{00ad}"), "startend");
    }
    #[test]
    fn the_plan_sequences_give_the_same_text_on_a_second_pass() {
        for (input, expected) in [
            ("J\u{030c}", "\u{01f0}"),
            ("\u{0130}\u{0316}", "i\u{0316}\u{0307}"),
            ("cafe\u{00ad}\u{0301}", "cafe"),
            ("e\u{00ad}\u{0301}", "e"),
        ] {
            let once = normalise(input);
            assert_eq!(once, expected, "{input:?}");
            assert_eq!(normalise(&once), once, "{input:?}");
        }
    }

    #[test]
    fn a_soft_hyphen_takes_the_marks_that_follow_it() {
        // Repeated soft hyphens: each is deleted and the mark after the last goes with it.
        assert_eq!(normalise("cafe\u{00ad}\u{00ad}\u{0301}"), "cafe");
        assert_eq!(normalise("cafe\u{00ad}\u{0301}\u{00ad}\u{0301}"), "cafe");
        // Several marks after one soft hyphen, in any canonical order, all go.
        assert_eq!(normalise("cafe\u{00ad}\u{0301}\u{0316}\u{0308}"), "cafe");
        assert_eq!(normalise("a\u{00ad}\u{0316}\u{0301}b"), "ab");
        // A spacing mark (Mc) and an enclosing mark (Me) are marks too.
        assert_eq!(normalise("x\u{00ad}\u{0903}\u{20dd}y"), "xy");
    }

    #[test]
    fn a_soft_hyphen_before_a_non_mark_leaves_the_marks_after_it() {
        // The soft hyphen is followed by `e`, so the acute after `e` is kept and composes.
        assert_eq!(normalise("caf\u{00ad}e\u{0301}"), "caf\u{00e9}");
        assert_eq!(normalise("con\u{00ad} tractor"), "con tractor");
    }

    #[test]
    fn a_mark_not_after_a_soft_hyphen_is_kept() {
        assert_eq!(normalise("cafe\u{0301}"), "caf\u{00e9}");
        assert_eq!(normalise("CAFE\u{0301}"), "caf\u{00e9}");
        assert_eq!(normalise("x\u{0301}"), "x\u{0301}");
        // A mark at the start of the text has no soft hyphen before it.
        assert_eq!(normalise("\u{0301}e"), "\u{0301}e");
    }

    #[test]
    fn normalise_is_idempotent_over_awkward_inputs() {
        let inputs = [
            "",
            "\u{00ad}",
            "\u{00ad}\u{0301}",
            "\u{0301}\u{00ad}",
            "J\u{030c}",
            "\u{0130}\u{0316}",
            "\u{0130}\u{0301}\u{0316}",
            "cafe\u{00ad}\u{0301}",
            "CAFE\u{00ad}\u{0301}",
            "caf\u{00ad}e\u{0301}",
            "e\u{00ad}\u{00ad}\u{0316}\u{0301}",
            "\u{1d2c}\u{0301}",
            "\u{2160}\u{0301}",
            "\u{212b}\u{0323}",
            "\u{1e9e}\u{0301}",
            "\u{03a3}\u{0345}\u{0301}",
            "\u{1f88}\u{0345}",
            "\u{fb01}\u{00ad}\u{0301}",
            "\u{ff21}\u{030a}",
            "\u{2011}\u{0338}",
            "\u{2212}\u{00ad}\u{0301}",
            "\u{1100}\u{1161}\u{11a8}",
            "\u{1100}\u{00ad}\u{1161}",
            "k\u{0307}\u{0323}\u{00ad}\u{0323}",
        ];
        for input in inputs {
            let once = normalise(input);
            assert_eq!(normalise(&once), once, "not idempotent for {input:?}");
        }
    }

    #[test]
    fn normalise_is_idempotent_for_every_letter_with_a_trailing_mark() {
        for ch in ('\u{0}'..='\u{2fff}').chain('\u{fb00}'..='\u{ffff}') {
            for tail in ["", "\u{0301}", "\u{0316}", "\u{030c}", "\u{00ad}\u{0301}"] {
                let input = format!("{ch}{tail}");
                let once = normalise(&input);
                assert_eq!(normalise(&once), once, "not idempotent for {input:?}");
            }
        }
    }
}
