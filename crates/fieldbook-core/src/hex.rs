//! Lowercase hexadecimal, the one spelling of a digest.
//!
//! **Responsibility.** `sha2` output has no `{:x}` formatting, so every digest and hash
//! in the program goes through [`lower`]. One spelling keeps two runs comparable.
//!
//! **Rules.** Pure: no I/O, no clock, no environment. Lowercase only, two digits per
//! byte, no separators, no prefix.

/// The lowercase hexadecimal text of `bytes`, two digits per byte.
///
/// ```
/// assert_eq!(fieldbook_core::hex::lower(&[0x00, 0xff]), "00ff");
/// assert_eq!(fieldbook_core::hex::lower(&[]), "");
/// ```
#[must_use]
pub fn lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(digit(byte >> 4));
        out.push(digit(byte & 0x0f));
    }
    out
}

/// The lowercase hexadecimal digit for a nibble.
fn digit(nibble: u8) -> char {
    match nibble {
        0..=9 => char::from(b'0' + nibble),
        _ => char::from(b'a' + nibble - 10),
    }
}

#[cfg(test)]
mod tests {
    use super::lower;

    #[test]
    fn empty_input_is_empty() {
        assert_eq!(lower(&[]), "");
    }

    #[test]
    fn the_extremes_are_two_lowercase_digits_each() {
        assert_eq!(lower(&[0x00, 0xff]), "00ff");
    }

    #[test]
    fn a_32_byte_digest_is_64_lowercase_digits() {
        // SHA-256 of the empty input, as bytes; sha2 itself arrives with a later card.
        let digest: [u8; 32] = [
            0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f,
            0xb9, 0x24, 0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c, 0xa4, 0x95, 0x99, 0x1b,
            0x78, 0x52, 0xb8, 0x55,
        ];
        let text = lower(&digest);
        assert_eq!(
            text,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(text.len(), 64);
        assert!(
            text.chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
        );
    }
}
