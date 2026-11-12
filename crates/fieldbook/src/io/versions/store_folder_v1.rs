//! Retained validation and ordering kernel for the Store `folder@1` adapter (published compatibility binding).
//!
//! **Responsibility.** The frozen rules of the folder store checked by the contract suite
//! (`tests/support/store_suite.rs`, items 1, 3, 4):
//! - Relative path refusal for empty paths, absolute paths, and parent traversals.
//! - Deterministic byte-order listing sorting.
//! - Content SHA-256 lowercase hexadecimal digest computation.
//!
//! The path rejection rules reproduce the path validation behaviour enforced by
//! `CorpusPath::parse` in `crates/fieldbook-core/src/audit/vocab.rs` and the WriteSet path
//! traversal checks in `crates/fieldbook/src/io/journal.rs` (lines 1431 and 1612), which refuse
//! empty paths, absolute paths (leading `/` or `\`, or drive prefixes like `C:`), and `..`
//! directory traversal segments.
//!
//! **Rules.** Pure: no I/O. This module is frozen. Any change is a new `store_folder_v2` file.

/// Category of path refusal for invalid relative paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathRefusal {
    /// The path is empty.
    Empty,
    /// The path is absolute (leading slash or Windows drive prefix).
    Absolute,
    /// The path contains a parent directory traversal (`..`) segment.
    Parent,
}

impl PathRefusal {
    /// Return the static reason word for this refusal.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Absolute => "absolute",
            Self::Parent => "parent",
        }
    }
}

/// Validate a relative store path according to the frozen `folder@1` rules.
///
/// Refuses:
/// - [`PathRefusal::Empty`]: when `rel` is empty.
/// - [`PathRefusal::Absolute`]: when `rel` starts with `/` or `\`, has a drive prefix
///   like `C:`, or is considered absolute by the platform.
/// - [`PathRefusal::Parent`]: when `rel` contains a `..` component when split on `/` or `\`.
///
/// Accepts paths such as `corpus/a.txt`, `a..b`, and `...`.
///
/// # Errors
///
/// Returns [`PathRefusal`] if `rel` violates any of the rules above.
pub fn check_rel_path(rel: &str) -> Result<(), PathRefusal> {
    if rel.is_empty() {
        return Err(PathRefusal::Empty);
    }
    let bytes = rel.as_bytes();
    if rel.starts_with('/')
        || rel.starts_with('\\')
        || (bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
        || std::path::Path::new(rel).is_absolute()
    {
        return Err(PathRefusal::Absolute);
    }
    for comp in rel.split(['/', '\\']) {
        if comp == ".." {
            return Err(PathRefusal::Parent);
        }
    }
    Ok(())
}

/// Sort a slice of names in ascending byte order (`as_bytes`).
pub fn byte_order(names: &mut [String]) {
    names.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
}

/// Compute the lowercase hexadecimal SHA-256 digest of `bytes`.
#[must_use]
pub fn content_sha256_hex(bytes: &[u8]) -> String {
    fieldbook_core::hash::sha256_hex(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_refusal_words() {
        assert_eq!(PathRefusal::Empty.word(), "empty");
        assert_eq!(PathRefusal::Absolute.word(), "absolute");
        assert_eq!(PathRefusal::Parent.word(), "parent");
    }

    #[test]
    fn check_rel_path_refusals() {
        assert_eq!(check_rel_path(""), Err(PathRefusal::Empty));
        assert_eq!(check_rel_path("/a"), Err(PathRefusal::Absolute));
        assert_eq!(check_rel_path("\\a"), Err(PathRefusal::Absolute));
        assert_eq!(check_rel_path("C:\\a"), Err(PathRefusal::Absolute));
        assert_eq!(check_rel_path("C:/a"), Err(PathRefusal::Absolute));
        assert_eq!(check_rel_path("c:foo"), Err(PathRefusal::Absolute));
        assert_eq!(check_rel_path("a/../b"), Err(PathRefusal::Parent));
        assert_eq!(check_rel_path(".."), Err(PathRefusal::Parent));
        assert_eq!(check_rel_path("a/.."), Err(PathRefusal::Parent));
        assert_eq!(check_rel_path("../b"), Err(PathRefusal::Parent));
        assert_eq!(check_rel_path("a\\..\\b"), Err(PathRefusal::Parent));
        assert_eq!(check_rel_path("a\\.."), Err(PathRefusal::Parent));
    }

    #[test]
    fn check_rel_path_acceptances() {
        assert_eq!(check_rel_path("corpus/a.txt"), Ok(()));
        assert_eq!(check_rel_path("a..b"), Ok(()));
        assert_eq!(check_rel_path("..."), Ok(()));
        assert_eq!(check_rel_path("single"), Ok(()));
        assert_eq!(check_rel_path("dir/subdir/file.dat"), Ok(()));
    }

    #[test]
    fn byte_order_sorting() {
        let mut names = vec![
            "A".to_owned(),
            "a".to_owned(),
            "b".to_owned(),
            "_".to_owned(),
        ];
        byte_order(&mut names);
        assert_eq!(
            names,
            vec![
                "A".to_owned(),
                "_".to_owned(),
                "a".to_owned(),
                "b".to_owned(),
            ]
        );
    }

    #[test]
    fn content_sha256_hex_computation() {
        let empty_hash = content_sha256_hex(b"");
        assert_eq!(
            empty_hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let sample_hash = content_sha256_hex(b"hello world\n");
        assert_eq!(
            sample_hash,
            fieldbook_core::hash::sha256_hex(b"hello world\n")
        );
    }
}
