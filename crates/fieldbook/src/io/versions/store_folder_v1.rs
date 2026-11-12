//! Retained validation and ordering kernel for the Store `folder@1` adapter (published compatibility binding).
//!
//! **Responsibility.** The frozen rules of the folder store checked by the contract suite
//! (`tests/support/store_suite.rs`, items 1, 3, 4):
//! - Relative path refusal for empty paths, absolute paths, parent traversals, overly long paths, and invalid paths.
//! - Deterministic byte-order listing sorting.
//! - Content SHA-256 lowercase hexadecimal digest computation.
//!
//! The path rejection rule equals `CorpusPath::parse` in `crates/fieldbook-core/src/audit/vocab.rs`.
//! `crates/fieldbook/src/io/journal.rs` calls `check_rel_path` on every write-set path in the
//! store's begin step.
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
    /// The path is longer than 256 bytes.
    TooLong,
    /// The path is invalid (trailing slash, backslash, empty or dot segment, or invalid character).
    Invalid,
}

impl PathRefusal {
    /// Return the static reason word for this refusal.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Absolute => "absolute",
            Self::Parent => "parent",
            Self::TooLong => "too-long",
            Self::Invalid => "invalid",
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
/// - [`PathRefusal::TooLong`]: when `rel` is longer than 256 bytes.
/// - [`PathRefusal::Invalid`]: when `rel` has a trailing `/`, a `\` anywhere, an empty segment,
///   a `.` segment, or a byte outside `[A-Za-z0-9._-]`.
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
    if rel.len() > 256 {
        return Err(PathRefusal::TooLong);
    }
    if rel.ends_with('/') || rel.contains('\\') {
        return Err(PathRefusal::Invalid);
    }
    for segment in rel.split('/') {
        if segment.is_empty() || segment == "." {
            return Err(PathRefusal::Invalid);
        }
        if !segment
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        {
            return Err(PathRefusal::Invalid);
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
        assert_eq!(PathRefusal::TooLong.word(), "too-long");
        assert_eq!(PathRefusal::Invalid.word(), "invalid");
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
    fn check_rel_path_refuses_what_the_store_refuses() {
        assert_eq!(check_rel_path(r"a\b"), Err(PathRefusal::Invalid));
        assert_eq!(check_rel_path("corpus//a.txt"), Err(PathRefusal::Invalid));
        assert_eq!(check_rel_path("corpus/./a.txt"), Err(PathRefusal::Invalid));
        assert_eq!(check_rel_path("corpus/a/"), Err(PathRefusal::Invalid));
        assert_eq!(check_rel_path("corpus/a b.txt"), Err(PathRefusal::Invalid));
        assert_eq!(check_rel_path(&"a".repeat(257)), Err(PathRefusal::TooLong));
        assert_eq!(check_rel_path(&"a".repeat(256)), Ok(()));
    }

    #[test]
    fn every_store_vector_path_gets_the_same_answer_as_corpus_path() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../compat/adapter/store/folder@1");
        let entries =
            std::fs::read_dir(&dir).expect("compat/adapter/store/folder@1 directory should exist");

        let mut files = Vec::new();
        for entry in entries {
            let entry = entry.expect("directory entry should be readable");
            let path = entry.path();
            if path.is_file()
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(".in.toml"))
            {
                files.push(path);
            }
        }
        files.sort();

        let mut path_ops_seen = 0;
        for file in &files {
            let content = std::fs::read_to_string(file).expect("file should be readable");
            let table: toml::Table =
                toml::from_str(&content).expect("vector file should parse as TOML");
            let ops = table
                .get("ops")
                .and_then(toml::Value::as_array)
                .expect("ops array should be present in in.toml");
            for op_val in ops {
                let op = op_val.as_str().expect("op should be string");
                if let Some(rel) = op.strip_prefix("path ") {
                    path_ops_seen += 1;
                    assert_eq!(
                        check_rel_path(rel).is_ok(),
                        fieldbook_core::audit::CorpusPath::parse(rel).is_ok(),
                        "vector path mismatch for {rel:?} in {file:?}"
                    );
                }
            }
        }

        let long_path = "a".repeat(257);
        let extra_paths = [
            r"a\b",
            "corpus//a.txt",
            "corpus/./a.txt",
            "corpus/a/",
            "corpus/a b.txt",
            &long_path,
            "",
            "/a",
            "a/..",
            r"a\..\b",
            "C:/a",
            "c:foo",
            "a..b",
            "...",
            "a/b/c.txt",
            "Z",
            "a-b_c.d",
            "é",
            "a\u{0}b",
        ];

        for rel in extra_paths {
            path_ops_seen += 1;
            assert_eq!(
                check_rel_path(rel).is_ok(),
                fieldbook_core::audit::CorpusPath::parse(rel).is_ok(),
                "extra path mismatch for {rel:?}"
            );
        }

        assert!(
            path_ops_seen >= 20,
            "at least 20 path ops were seen so the test cannot pass on an empty directory (got {path_ops_seen})"
        );
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
