//! Search text normalisation (`docs/reference/search.md#normalisation`).
//!
//! Applied in order on both the index side and the query side:
//! 1. Unicode NFKC.
//! 2. Full Unicode lowercase (`str::to_lowercase`).
//! 3. Fold `U+2010..=U+2015` and `U+2212` to ASCII `-` (`U+002D`).
//! 4. Delete `U+00AD` (soft hyphen).
//! 5. Unicode NFKC again, composing or reordering newly adjacent characters.

use unicode_normalization::UnicodeNormalization;

/// Normalises `input` according to `docs/reference/search.md#normalisation`.
#[must_use]
pub fn normalise(input: &str) -> String {
    let nfkc: String = input.nfkc().collect();
    let lower = nfkc.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    for ch in lower.chars() {
        match ch {
            '\u{00AD}' => {}
            '\u{2010}'..='\u{2015}' | '\u{2212}' => out.push('-'),
            other => out.push(other),
        }
    }
    out.nfkc().collect()
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
}
