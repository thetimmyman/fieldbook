//! Typed representation and validation of `corpus/corpus.toml`.
//!
//! **Responsibility.** Parse `corpus.toml` with `deny_unknown_fields` on every
//! table and validate areas, subareas, views and `schema_version`.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ids::AreaCode;
use crate::version;

/// The parsed contents of `corpus/corpus.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusToml {
    /// Format schema version (`1` on Day 1).
    pub schema_version: u32,
    /// `[assistant]` configuration block.
    pub assistant: AssistantConfig,
    /// `[[area]]` entries in display order.
    #[serde(default)]
    pub area: Vec<AreaDef>,
    /// `[[view]]` entries in display order.
    #[serde(default)]
    pub view: Vec<ViewDef>,
}

/// The `[assistant]` section of `corpus/corpus.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssistantConfig {
    /// Character budget of one context pack.
    pub pack_budget_chars: u32,
}

/// One `[[area]]` entry in `corpus/corpus.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AreaDef {
    /// Three-letter lowercase area code (`fnd`, `cmp`, ...).
    pub code: String,
    /// Folder name under `corpus/pages/`.
    pub slug: String,
    /// Short display name.
    pub name: String,
    /// One-line summary for the area header.
    pub summary: String,
    /// Allowed subarea folder names under `corpus/pages/<slug>/`.
    pub subareas: Vec<String>,
}

/// One `[[view]]` entry in `corpus/corpus.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewDef {
    /// Three-letter uppercase view code (`CPY`, `CPT`, `PGM`).
    pub code: String,
    /// Short display name.
    pub name: String,
    /// Area and subarea filter for the view.
    pub filter: ViewFilter,
}

/// The `filter = { area = "...", subarea = "..." }` inline table of a view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewFilter {
    /// Target area code.
    pub area: String,
    /// Target subarea slug within `area`.
    pub subarea: String,
}

/// Error returned when `corpus.toml` is not valid TOML for [`CorpusToml`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorpusTomlError {
    message: String,
    line: u32,
    col: u32,
}

impl CorpusTomlError {
    /// Returns the 1-based line number of the parse error.
    #[must_use]
    pub const fn line(&self) -> u32 {
        self.line
    }

    /// Returns the 1-based code-point column number of the parse error.
    #[must_use]
    pub const fn col(&self) -> u32 {
        self.col
    }

    /// Returns the diagnostic message from the TOML reader.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for CorpusTomlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.message)
    }
}

impl std::error::Error for CorpusTomlError {}

/// A semantic validation fault in a parsed [`CorpusToml`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorpusProblem {
    /// `schema_version` is `0` or above [`version::SCHEMA_VERSION_MAX`].
    UnsupportedSchemaVersion(u32),
    /// An area `code` is not three lowercase letters or starts with `x` in the repository Corpus.
    MalformedAreaCode(String),
    /// An area `code` appears more than once.
    DuplicateAreaCode(String),
    /// An area `slug` does not match `^[a-z][a-z0-9-]*$`.
    MalformedAreaSlug(String),
    /// An area `slug` appears more than once.
    DuplicateAreaSlug(String),
    /// A subarea slug does not match `^[a-z][a-z0-9-]*$`.
    MalformedSubarea {
        /// The owning area code.
        area: String,
        /// The malformed subarea string.
        subarea: String,
    },
    /// A subarea appears more than once within the same area.
    DuplicateSubarea {
        /// The owning area code.
        area: String,
        /// The repeated subarea string.
        subarea: String,
    },
    /// A view `code` is not three uppercase ASCII letters, equals an area code in capitals, or is repeated.
    InvalidViewCode(String),
    /// A view `filter.area` does not name any area in `[[area]]`.
    UnknownViewArea {
        /// The view code.
        view: String,
        /// The unknown area code.
        area: String,
    },
    /// A view `filter.subarea` does not name a subarea of `filter.area`.
    UnknownViewSubarea {
        /// The view code.
        view: String,
        /// The target area code.
        area: String,
        /// The unknown subarea name.
        subarea: String,
    },
}

impl fmt::Display for CorpusProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchemaVersion(v) => {
                write!(
                    f,
                    "schema_version {v} is not supported (max {})",
                    version::SCHEMA_VERSION_MAX
                )
            }
            Self::MalformedAreaCode(code) => write!(f, "malformed area code {code:?}"),
            Self::DuplicateAreaCode(code) => write!(f, "duplicate area code {code:?}"),
            Self::MalformedAreaSlug(slug) => write!(f, "malformed area slug {slug:?}"),
            Self::DuplicateAreaSlug(slug) => write!(f, "duplicate area slug {slug:?}"),
            Self::MalformedSubarea { area, subarea } => {
                write!(f, "malformed subarea {subarea:?} in area {area:?}")
            }
            Self::DuplicateSubarea { area, subarea } => {
                write!(f, "duplicate subarea {subarea:?} in area {area:?}")
            }
            Self::InvalidViewCode(code) => write!(f, "invalid view code {code:?}"),
            Self::UnknownViewArea { view, area } => {
                write!(f, "view {view:?} filter names unknown area {area:?}")
            }
            Self::UnknownViewSubarea {
                view,
                area,
                subarea,
            } => {
                write!(
                    f,
                    "view {view:?} filter names unknown subarea {subarea:?} of area {area:?}"
                )
            }
        }
    }
}

/// Returns `true` when `s` matches the slug rule `^[a-z][a-z0-9-]*$`.
#[must_use]
pub fn is_slug(s: &str) -> bool {
    let mut bytes = s.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    first.is_ascii_lowercase()
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

pub(crate) fn byte_offset_to_line_col(text: &str, byte_offset: usize) -> (u32, u32) {
    let clamped = byte_offset.min(text.len());
    let mut end = clamped;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let prefix = &text[..end];
    let mut line = 1u32;
    let mut line_start_byte = 0usize;
    for (idx, b) in prefix.bytes().enumerate() {
        if b == b'\n' {
            line = line.saturating_add(1);
            line_start_byte = idx + 1;
        }
    }
    let col = prefix[line_start_byte..].chars().count().saturating_add(1);
    let col_u32 = u32::try_from(col).unwrap_or(u32::MAX);
    (line, col_u32)
}

impl CorpusToml {
    /// Parses `corpus.toml` text with `deny_unknown_fields`, returning 1-based
    /// line and code-point column on failure.
    pub fn parse(input: &str) -> Result<Self, CorpusTomlError> {
        toml::from_str::<Self>(input).map_err(|err| {
            let offset = err.span().map_or(0, |r| r.start);
            let (line, col) = byte_offset_to_line_col(input, offset);
            CorpusTomlError {
                message: err.message().to_owned(),
                line,
                col,
            }
        })
    }

    /// Returns the slice of area definitions in display order.
    #[must_use]
    pub fn areas(&self) -> &[AreaDef] {
        &self.area
    }

    /// Returns the slice of view definitions in display order.
    #[must_use]
    pub fn views(&self) -> &[ViewDef] {
        &self.view
    }

    /// Validates all semantic invariants of `corpus.toml`, returning one
    /// [`CorpusProblem`] per fault.
    #[must_use]
    pub fn validate(&self) -> Vec<CorpusProblem> {
        let mut problems = Vec::new();

        if !(1..=version::SCHEMA_VERSION_MAX).contains(&self.schema_version) {
            problems.push(CorpusProblem::UnsupportedSchemaVersion(self.schema_version));
        }

        let mut seen_codes = BTreeSet::new();
        let mut seen_slugs = BTreeSet::new();

        for area in &self.area {
            if AreaCode::parse(&area.code).is_err() || area.code.starts_with('x') {
                problems.push(CorpusProblem::MalformedAreaCode(area.code.clone()));
            } else if !seen_codes.insert(area.code.as_str()) {
                problems.push(CorpusProblem::DuplicateAreaCode(area.code.clone()));
            }

            if !is_slug(&area.slug) {
                problems.push(CorpusProblem::MalformedAreaSlug(area.slug.clone()));
            } else if !seen_slugs.insert(area.slug.as_str()) {
                problems.push(CorpusProblem::DuplicateAreaSlug(area.slug.clone()));
            }

            let mut seen_subareas = BTreeSet::new();
            for sub in &area.subareas {
                if !is_slug(sub) {
                    problems.push(CorpusProblem::MalformedSubarea {
                        area: area.code.clone(),
                        subarea: sub.clone(),
                    });
                } else if !seen_subareas.insert(sub.as_str()) {
                    problems.push(CorpusProblem::DuplicateSubarea {
                        area: area.code.clone(),
                        subarea: sub.clone(),
                    });
                }
            }
        }

        let mut seen_views = BTreeSet::new();
        for view in &self.view {
            let is_three_caps =
                view.code.len() == 3 && view.code.bytes().all(|b| b.is_ascii_uppercase());
            let collides_with_area = self
                .area
                .iter()
                .any(|a| a.code.eq_ignore_ascii_case(&view.code));
            if !is_three_caps || collides_with_area || !seen_views.insert(view.code.as_str()) {
                problems.push(CorpusProblem::InvalidViewCode(view.code.clone()));
            }

            if let Some(target_area) = self.area.iter().find(|a| a.code == view.filter.area) {
                if !target_area.subareas.contains(&view.filter.subarea) {
                    problems.push(CorpusProblem::UnknownViewSubarea {
                        view: view.code.clone(),
                        area: view.filter.area.clone(),
                        subarea: view.filter.subarea.clone(),
                    });
                }
            } else {
                problems.push(CorpusProblem::UnknownViewArea {
                    view: view.code.clone(),
                    area: view.filter.area.clone(),
                });
            }
        }

        problems
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL_CORPUS_TOML: &str = include_str!("../../../../corpus/corpus.toml");

    #[test]
    fn real_corpus_toml_has_13_areas_3_views_and_zero_problems() {
        let parsed = CorpusToml::parse(REAL_CORPUS_TOML).expect("real corpus.toml must parse");
        assert_eq!(parsed.area.len(), 13);
        assert_eq!(parsed.view.len(), 3);
        let problems = parsed.validate();
        assert!(
            problems.is_empty(),
            "expected zero problems on real corpus.toml, got {problems:?}"
        );
    }

    #[test]
    fn a_view_filter_must_name_a_real_subarea() {
        let mut parsed = CorpusToml::parse(REAL_CORPUS_TOML).expect("real corpus.toml must parse");
        parsed.view[0].filter.subarea = "no-such-subarea".to_owned();
        let problems = parsed.validate();
        assert_eq!(
            problems,
            vec![CorpusProblem::UnknownViewSubarea {
                view: "CPY".to_owned(),
                area: "mkt".to_owned(),
                subarea: "no-such-subarea".to_owned(),
            }]
        );
    }

    #[test]
    fn malformed_and_repeated_area_code_are_reported() {
        let mut parsed = CorpusToml::parse(REAL_CORPUS_TOML).expect("real corpus.toml must parse");
        parsed.area[0].code = "xnd".to_owned();
        assert!(
            parsed
                .validate()
                .contains(&CorpusProblem::MalformedAreaCode("xnd".to_owned()))
        );

        let mut dup = CorpusToml::parse(REAL_CORPUS_TOML).expect("real corpus.toml must parse");
        dup.area[1].code = dup.area[0].code.clone();
        assert!(
            dup.validate()
                .contains(&CorpusProblem::DuplicateAreaCode("fnd".to_owned()))
        );
    }

    #[test]
    fn malformed_and_repeated_area_slug_are_reported() {
        let mut parsed = CorpusToml::parse(REAL_CORPUS_TOML).expect("real corpus.toml must parse");
        parsed.area[0].slug = "1bad-slug".to_owned();
        assert!(
            parsed
                .validate()
                .contains(&CorpusProblem::MalformedAreaSlug("1bad-slug".to_owned()))
        );

        let mut dup = CorpusToml::parse(REAL_CORPUS_TOML).expect("real corpus.toml must parse");
        dup.area[1].slug = dup.area[0].slug.clone();
        assert!(
            dup.validate()
                .contains(&CorpusProblem::DuplicateAreaSlug("foundation".to_owned()))
        );
    }

    #[test]
    fn repeated_subarea_is_reported() {
        let mut parsed = CorpusToml::parse(REAL_CORPUS_TOML).expect("real corpus.toml must parse");
        parsed.area[0].subareas.push("evidence".to_owned());
        assert_eq!(
            parsed.validate(),
            vec![CorpusProblem::DuplicateSubarea {
                area: "fnd".to_owned(),
                subarea: "evidence".to_owned(),
            }]
        );
    }

    #[test]
    fn invalid_or_colliding_view_code_is_reported() {
        let mut parsed = CorpusToml::parse(REAL_CORPUS_TOML).expect("real corpus.toml must parse");
        parsed.view[0].code = "cpy".to_owned();
        assert_eq!(
            parsed.validate(),
            vec![CorpusProblem::InvalidViewCode("cpy".to_owned())]
        );

        let mut collides =
            CorpusToml::parse(REAL_CORPUS_TOML).expect("real corpus.toml must parse");
        collides.view[0].code = "MKT".to_owned();
        assert_eq!(
            collides.validate(),
            vec![CorpusProblem::InvalidViewCode("MKT".to_owned())]
        );
    }

    #[test]
    fn view_filter_with_unknown_area_is_reported() {
        let mut parsed = CorpusToml::parse(REAL_CORPUS_TOML).expect("real corpus.toml must parse");
        parsed.view[0].filter.area = "zzz".to_owned();
        assert_eq!(
            parsed.validate(),
            vec![CorpusProblem::UnknownViewArea {
                view: "CPY".to_owned(),
                area: "zzz".to_owned(),
            }]
        );
    }

    #[test]
    fn schema_version_above_supported_is_reported() {
        let mut parsed = CorpusToml::parse(REAL_CORPUS_TOML).expect("real corpus.toml must parse");
        parsed.schema_version = version::SCHEMA_VERSION_MAX + 1;
        assert_eq!(
            parsed.validate(),
            vec![CorpusProblem::UnsupportedSchemaVersion(
                version::SCHEMA_VERSION_MAX + 1
            )]
        );
    }

    #[test]
    fn unknown_key_in_corpus_toml_is_refused() {
        let with_unknown = format!("{REAL_CORPUS_TOML}\nextra_key = true\n");
        let err = CorpusToml::parse(&with_unknown).expect_err("unknown key must fail");
        assert!(err.line() >= 1 && err.col() >= 1);
    }
}
