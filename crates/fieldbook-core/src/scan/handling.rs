//! Inline handling line (`FB0410`, `docs/reference/corpus-format.md#handling-and-labels`).
//!
//! **Responsibility.** Tell whether a text holds a `handling = …` line whose value is not
//! public. `manifest` refuses such a shipped file, and `cargo xtask extract` refuses
//! supplied text that holds one before it writes any page.
//!
//! **Rules.** Pure: no I/O, no regex.

/// Whether `text` holds an inline `handling` line whose value is not public.
///
/// The line is read as a page's front matter writes it: `handling` first, then `=`.
/// `["public"]` and `"public"` are the two public values; anything else, including a
/// built-in label such as `unlabelled` or a company label, is refused.
#[must_use]
pub fn has_non_public_inline(text: &str) -> bool {
    text.lines().any(|line| {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("handling") else {
            return false;
        };
        let Some(value) = rest.trim_start().strip_prefix('=') else {
            return false;
        };
        let value = value.trim();
        value != "[\"public\"]" && value != "\"public\""
    })
}

#[cfg(test)]
mod tests {
    use super::has_non_public_inline;

    #[test]
    fn a_public_handling_line_is_allowed() {
        for text in [
            "handling = [\"public\"]\nbody text\n",
            "handling = \"public\"\nbody text\n",
            "  handling = [\"public\"]  \nbody text\n",
            "no handling line at all\n",
            "handling_note = [\"np-0007\"]\n",
        ] {
            assert!(!has_non_public_inline(text), "{text:?} is public");
        }
    }

    #[test]
    fn a_non_public_handling_line_is_found() {
        for text in [
            "handling = [\"np-0007\"]\nbody text\n",
            "handling = \"internal\"\nbody text\n",
            "body text\nhandling = [\"unlabelled\"]\n",
            "handling = [\"public\", \"np-0007\"]\n",
        ] {
            assert!(has_non_public_inline(text), "{text:?} is not public");
        }
    }
}
