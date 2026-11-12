//! Extraction methods registry.
//!
//! **Responsibility.** Defines the static registry of extraction methods, their canonical
//! representation, and the [`ExtractionMethod`] lookup enum.
//!
//! **Rules.** Pure: no I/O. Methods are append-only.

/// The canonical keys common to all extraction records.
pub const COMMON_KEYS: &[&str] = &[
    "tool",
    "tool_version",
    "method",
    "page_count",
    "thin_pages",
    "original_sha256",
    "pages",
    "locators",
];

/// The `pdftotext-layout` method name.
pub const PDFTOTEXT_LAYOUT: &str = "pdftotext-layout";
/// The `html-strip` method name.
pub const HTML_STRIP: &str = "html-strip";
/// The `plain` method name.
pub const PLAIN: &str = "plain";
/// The `supplied` method name.
pub const SUPPLIED: &str = "supplied";

/// Adapter ID for the `pdftotext-layout` extractor.
pub const PDFTOTEXT_LAYOUT_ID: &str = "pdftotext-layout@1";
/// Adapter ID for the `html-strip` extractor.
pub const HTML_STRIP_ID: &str = "html-strip@1";
/// Adapter ID for the `plain` extractor.
pub const PLAIN_ID: &str = "plain@1";

/// Specification of an extraction method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Method {
    /// Canonical method name.
    pub name: &'static str,
    /// Associated file extensions.
    pub types: &'static [&'static str],
    /// Locator kind or prefix.
    pub locator: &'static str,
    /// Keys allowed or expected in the extraction record.
    pub keys: &'static [&'static str],
    /// Extractor adapter ID, if implemented as an adapter.
    pub extractor: Option<&'static str>,
    /// Repository-relative path to a sample fixture.
    pub fixture: &'static str,
}

impl Method {
    /// Format this method as a canonical pipe-delimited golden line.
    pub fn line(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}",
            self.name,
            self.types.join(","),
            self.locator,
            self.keys.join(","),
            self.extractor.unwrap_or("-"),
            self.fixture,
        )
    }
}

/// The append-only registry of extraction methods.
pub const METHODS: &[Method] = &[
    Method {
        name: PDFTOTEXT_LAYOUT,
        types: &["pdf"],
        locator: "p",
        keys: COMMON_KEYS,
        extractor: Some(PDFTOTEXT_LAYOUT_ID),
        fixture: "xtask/tests/fixtures/synthetic-two-pages.pdf",
    },
    Method {
        name: HTML_STRIP,
        types: &["html", "htm", "xhtml", "svg"],
        locator: "p",
        keys: COMMON_KEYS,
        extractor: Some(HTML_STRIP_ID),
        fixture: "tests/fixtures/extract/html-strip/sample.html",
    },
    Method {
        name: PLAIN,
        types: &["txt", "md", "markdown"],
        locator: "p",
        keys: COMMON_KEYS,
        extractor: Some(PLAIN_ID),
        fixture: "tests/fixtures/corpus-mini/corpus/sources/public/S-0001-v1.txt",
    },
    Method {
        name: SUPPLIED,
        types: &[],
        locator: "p",
        keys: COMMON_KEYS,
        extractor: None,
        fixture: "tests/fixtures/corpus-mini/corpus/sources/public/S-0001-v1.txt",
    },
];

/// Known or unsupported extraction method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractionMethod {
    /// A known, registered extraction method.
    Known(&'static Method),
    /// An unsupported or unrecognized extraction method name.
    Unsupported(String),
}

impl ExtractionMethod {
    /// Parse an extraction method by name.
    pub fn parse(name: &str) -> Self {
        for method in METHODS {
            if method.name == name {
                return Self::Known(method);
            }
        }
        Self::Unsupported(name.to_owned())
    }

    /// The name of this extraction method.
    pub fn name(&self) -> &str {
        match self {
            Self::Known(m) => m.name,
            Self::Unsupported(name) => name.as_str(),
        }
    }

    /// Whether this method is known and supported.
    pub fn is_supported(&self) -> bool {
        matches!(self, Self::Known(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn methods_only_grow() {
        let golden = include_str!("../../../../tests/golden/extract/methods.txt");
        let golden_lines: Vec<&str> = golden.lines().filter(|l| !l.is_empty()).collect();
        assert!(
            METHODS.len() >= golden_lines.len(),
            "METHODS cannot have fewer rows than golden"
        );
        for (idx, expected) in golden_lines.iter().enumerate() {
            let actual = METHODS[idx].line();
            assert_eq!(&actual, expected, "METHODS[{idx}] line mismatch");
        }
    }

    #[test]
    fn names_are_unique() {
        let mut seen = BTreeSet::new();
        for method in METHODS {
            assert!(
                seen.insert(method.name),
                "duplicate method name: {}",
                method.name
            );
        }
    }

    #[test]
    fn extractor_ids_follow_convention() {
        for method in METHODS {
            if let Some(id) = method.extractor {
                assert_eq!(id, format!("{}@1", method.name));
            }
        }
    }

    #[test]
    fn types_cover_select_types() {
        let plain = METHODS.iter().find(|m| m.name == PLAIN).expect("plain");
        let html_strip = METHODS
            .iter()
            .find(|m| m.name == HTML_STRIP)
            .expect("html-strip");
        let union_types: BTreeSet<&str> = plain
            .types
            .iter()
            .chain(html_strip.types.iter())
            .copied()
            .collect();
        let select_types: BTreeSet<&str> =
            crate::extraction::SELECT_TYPES.iter().copied().collect();
        assert_eq!(union_types, select_types);
    }

    #[test]
    fn extraction_method_parse() {
        for method in METHODS {
            let parsed = ExtractionMethod::parse(method.name);
            assert_eq!(parsed, ExtractionMethod::Known(method));
            assert_eq!(parsed.name(), method.name);
            assert!(parsed.is_supported());
        }
        let unsupported = ExtractionMethod::parse("csv-rows-future");
        assert_eq!(
            unsupported,
            ExtractionMethod::Unsupported("csv-rows-future".to_owned())
        );
        assert_eq!(unsupported.name(), "csv-rows-future");
        assert!(!unsupported.is_supported());
    }
}
