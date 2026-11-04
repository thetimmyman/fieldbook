//! Corpus format v1 identifiers, ordering and sequence allocation.
//!
//! **Responsibility.** Parses and formats every identifier form in the
//! Identifiers table of `docs/reference/corpus-format.md`, enforces the
//! 64-character limit on page and term ids, orders `ClaimId` by `(area, number)`
//! and every other id byte by byte, and allocates the next id in the public
//! source, claim, question and conflict series.
//!
//! **Rules.** Pure: no regex crate, no clock, no I/O, no environment.

pub mod families;

use core::cmp::Ordering;
use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

pub use families::{CASE_PREFIXES, FAMILIES, Family};
use families::{
    IDX_ADR, IDX_AREA, IDX_CASE, IDX_CLAIM, IDX_CONFLICT, IDX_EDITION, IDX_ERROR_CODE, IDX_PAGE,
    IDX_QUESTION, IDX_SOURCE, IDX_SOURCE_VERSION, IDX_TERM, IDX_TOOL_RELEASE, matches_shape,
};

/// Maximum length in bytes/ASCII characters of a [`PageId`] or [`TermId`].
pub const MAX_SLUG_ID_LEN: usize = 64;

/// Maximum public sequence number for [`SourceId`] (`S-9999`).
pub const MAX_SOURCE_NUMBER: u32 = 9_999;

/// Maximum public sequence number for [`ClaimId`] (`CL-AAA-99999`).
pub const MAX_CLAIM_NUMBER: u32 = 99_999;

/// Maximum public sequence number for [`QuestionId`] (`Q-999`).
pub const MAX_QUESTION_NUMBER: u32 = 999;

/// Classification of an [`IdError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdErrorKind {
    /// The input did not match the identifier form's grammar or length limit.
    InvalidFormat,
    /// The identifier series has reached its maximum defined capacity.
    Exhausted,
    /// Incrementing or parsing the sequence number overflowed the integer representation.
    Overflow,
}

/// Error returned when parsing or allocating a Corpus identifier fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdError {
    /// The snake_case name of the identifier form (from [`FAMILIES`]).
    pub form: &'static str,
    /// The offending input text (or the highest existing id on exhaustion/overflow).
    pub input: String,
    /// Why the operation failed.
    pub kind: IdErrorKind,
}

impl IdError {
    /// Creates an invalid-format error for `form` and `input`.
    #[must_use]
    pub fn invalid(form: &'static str, input: impl Into<String>) -> Self {
        Self {
            form,
            input: input.into(),
            kind: IdErrorKind::InvalidFormat,
        }
    }

    /// Creates a sequence-exhausted error for `form` and `input`.
    #[must_use]
    pub fn exhausted(form: &'static str, input: impl Into<String>) -> Self {
        Self {
            form,
            input: input.into(),
            kind: IdErrorKind::Exhausted,
        }
    }

    /// Creates a representation-overflow error for `form` and `input`.
    #[must_use]
    pub fn overflow(form: &'static str, input: impl Into<String>) -> Self {
        Self {
            form,
            input: input.into(),
            kind: IdErrorKind::Overflow,
        }
    }

    /// The snake_case identifier form name.
    #[must_use]
    pub const fn form(&self) -> &'static str {
        self.form
    }

    /// The input string that failed.
    #[must_use]
    pub fn input(&self) -> &str {
        &self.input
    }

    /// The failure kind.
    #[must_use]
    pub const fn kind(&self) -> IdErrorKind {
        self.kind
    }

    /// Whether the failure is capacity exhaustion or integer overflow.
    #[must_use]
    pub const fn is_exhausted(&self) -> bool {
        matches!(self.kind, IdErrorKind::Exhausted | IdErrorKind::Overflow)
    }
}

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            IdErrorKind::InvalidFormat => {
                write!(f, "invalid {} id: {:?}", self.form, self.input)
            }
            IdErrorKind::Exhausted => {
                write!(f, "{} id series exhausted at {:?}", self.form, self.input)
            }
            IdErrorKind::Overflow => {
                write!(
                    f,
                    "{} id integer representation overflow at {:?}",
                    self.form, self.input
                )
            }
        }
    }
}

impl std::error::Error for IdError {}

macro_rules! impl_id_common {
    ($ty:ident) => {
        impl $ty {
            /// Returns the canonical identifier slice.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.raw
            }
        }

        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.raw)
            }
        }

        impl FromStr for $ty {
            type Err = IdError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::parse(s)
            }
        }

        impl AsRef<str> for $ty {
            fn as_ref(&self) -> &str {
                &self.raw
            }
        }

        impl Serialize for $ty {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.raw)
            }
        }

        impl<'de> Deserialize<'de> for $ty {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let s = String::deserialize(deserializer)?;
                Self::parse(&s).map_err(de::Error::custom)
            }
        }
    };
}

/// Three lowercase ASCII letters naming a Corpus subject area (`cmp`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AreaCode {
    raw: String,
}

impl AreaCode {
    /// Parses a three-letter lowercase area code (`^[a-z]{3}$`).
    pub fn parse(input: &str) -> Result<Self, IdError> {
        let row = &FAMILIES[IDX_AREA];
        if !matches_shape(row.public_shape, input) {
            return Err(IdError::invalid(row.name, input));
        }
        Ok(Self {
            raw: input.to_owned(),
        })
    }
}

impl_id_common!(AreaCode);

/// A Corpus page identifier (`<area>-<kebab>`, at most 64 characters).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PageId {
    raw: String,
}

impl PageId {
    /// Parses a page identifier (`^[a-z]{3}(-[a-z0-9]+)+$`, at most 64 characters).
    pub fn parse(input: &str) -> Result<Self, IdError> {
        let row = &FAMILIES[IDX_PAGE];
        if input.len() > MAX_SLUG_ID_LEN || !matches_shape(row.public_shape, input) {
            return Err(IdError::invalid(row.name, input));
        }
        Ok(Self {
            raw: input.to_owned(),
        })
    }

    /// Returns the three-letter lowercase [`AreaCode`] prefix of this page id.
    #[must_use]
    pub fn area(&self) -> AreaCode {
        AreaCode {
            raw: self.raw[..3].to_owned(),
        }
    }
}

impl_id_common!(PageId);

/// A public source identifier (`S-0042`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceId {
    raw: String,
    number: u32,
}

impl SourceId {
    /// Parses a public source identifier (`^S-[0-9]{4}$`).
    pub fn parse(input: &str) -> Result<Self, IdError> {
        let row = &FAMILIES[IDX_SOURCE];
        let Some(prefix) = row.prefixes.first().copied() else {
            return Err(IdError::invalid(row.name, input));
        };
        let Some(rest) = input.strip_prefix(prefix).and_then(|s| s.strip_prefix('-')) else {
            return Err(IdError::invalid(row.name, input));
        };
        if rest.len() != 4 || !matches_shape(row.public_shape, input) {
            return Err(IdError::invalid(row.name, input));
        }
        let number = rest
            .parse::<u32>()
            .map_err(|_| IdError::invalid(row.name, input))?;
        Ok(Self {
            raw: input.to_owned(),
            number,
        })
    }

    /// Returns the numeric sequence part of the source id.
    #[must_use]
    pub const fn number(&self) -> u32 {
        self.number
    }
}

impl_id_common!(SourceId);

/// A public source version identifier (`S-0042-v2`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VersionId {
    raw: String,
    source: SourceId,
    number: u32,
}

/// Alias for [`VersionId`].
pub type SourceVersionId = VersionId;

impl VersionId {
    /// Parses a public source version identifier (`^S-[0-9]{4}-v[1-9][0-9]*$`).
    pub fn parse(input: &str) -> Result<Self, IdError> {
        let row = &FAMILIES[IDX_SOURCE_VERSION];
        let Some(prefix) = row.prefixes.first().copied() else {
            return Err(IdError::invalid(row.name, input));
        };
        if !input.starts_with(prefix) || !matches_shape(row.public_shape, input) {
            return Err(IdError::invalid(row.name, input));
        }
        let Some((source_part, version_part)) = input.rsplit_once("-v") else {
            return Err(IdError::invalid(row.name, input));
        };
        let source = SourceId::parse(source_part).map_err(|_| IdError::invalid(row.name, input))?;
        let number = version_part
            .parse::<u32>()
            .map_err(|_| IdError::overflow(row.name, input))?;
        if number == 0 {
            return Err(IdError::invalid(row.name, input));
        }
        Ok(Self {
            raw: input.to_owned(),
            source,
            number,
        })
    }

    /// Returns the parent [`SourceId`] (`S-0042` for `S-0042-v2`).
    #[must_use]
    pub fn source(&self) -> SourceId {
        self.source.clone()
    }

    /// Returns the version number (`2` for `S-0042-v2`).
    #[must_use]
    pub const fn number(&self) -> u32 {
        self.number
    }

    /// Returns the version number (`2` for `S-0042-v2`).
    #[must_use]
    pub const fn version(&self) -> u32 {
        self.number
    }
}

impl_id_common!(VersionId);

/// A public claim identifier (`CL-CMP-0011`, `CL-CMP-10000`).
///
/// Orders by lowercase [`AreaCode`] and then by integer sequence number so
/// `CL-CMP-9999` precedes `CL-CMP-10000`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClaimId {
    raw: String,
    area: AreaCode,
    number: u32,
}

impl ClaimId {
    /// Parses a public claim identifier (`^CL-[A-Z]{3}-([0-9]{4}|[1-9][0-9]{4})$`).
    pub fn parse(input: &str) -> Result<Self, IdError> {
        let row = &FAMILIES[IDX_CLAIM];
        let Some(prefix) = row.prefixes.first().copied() else {
            return Err(IdError::invalid(row.name, input));
        };
        let Some(after_prefix) = input.strip_prefix(prefix).and_then(|s| s.strip_prefix('-'))
        else {
            return Err(IdError::invalid(row.name, input));
        };
        let Some((area_upper, digits)) = after_prefix.split_once('-') else {
            return Err(IdError::invalid(row.name, input));
        };
        if area_upper.len() != 3
            || !area_upper.bytes().all(|b| b.is_ascii_uppercase())
            || !matches_shape(row.public_shape, input)
        {
            return Err(IdError::invalid(row.name, input));
        }
        let number = digits
            .parse::<u32>()
            .map_err(|_| IdError::invalid(row.name, input))?;
        let area = AreaCode::parse(&area_upper.to_ascii_lowercase())
            .map_err(|_| IdError::invalid(row.name, input))?;
        Ok(Self {
            raw: input.to_owned(),
            area,
            number,
        })
    }

    /// Returns the lowercase [`AreaCode`] of the claim (`cmp` for `CL-CMP-0011`).
    #[must_use]
    pub fn area(&self) -> AreaCode {
        self.area.clone()
    }

    /// Returns the integer sequence number of the claim.
    #[must_use]
    pub const fn number(&self) -> u32 {
        self.number
    }
}

impl PartialOrd for ClaimId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ClaimId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.area
            .cmp(&other.area)
            .then_with(|| self.number.cmp(&other.number))
    }
}

impl_id_common!(ClaimId);

/// A public question identifier (`Q-025`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QuestionId {
    raw: String,
    number: u32,
}

impl QuestionId {
    /// Parses a public question identifier (`^Q-[0-9]{3}$`).
    pub fn parse(input: &str) -> Result<Self, IdError> {
        let row = &FAMILIES[IDX_QUESTION];
        let Some(prefix) = row.prefixes.first().copied() else {
            return Err(IdError::invalid(row.name, input));
        };
        let Some(digits) = input.strip_prefix(prefix).and_then(|s| s.strip_prefix('-')) else {
            return Err(IdError::invalid(row.name, input));
        };
        if digits.len() != 3 || !matches_shape(row.public_shape, input) {
            return Err(IdError::invalid(row.name, input));
        }
        let number = digits
            .parse::<u32>()
            .map_err(|_| IdError::invalid(row.name, input))?;
        Ok(Self {
            raw: input.to_owned(),
            number,
        })
    }

    /// Returns the numeric sequence value of the question.
    #[must_use]
    pub const fn number(&self) -> u32 {
        self.number
    }
}

impl_id_common!(QuestionId);

/// A public cross-source conflict identifier (`XC-14`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConflictId {
    raw: String,
    number: u32,
}

impl ConflictId {
    /// Parses a public conflict identifier (`^XC-[0-9]{2,}$`).
    pub fn parse(input: &str) -> Result<Self, IdError> {
        let row = &FAMILIES[IDX_CONFLICT];
        let Some(prefix) = row.prefixes.first().copied() else {
            return Err(IdError::invalid(row.name, input));
        };
        let Some(digits) = input.strip_prefix(prefix).and_then(|s| s.strip_prefix('-')) else {
            return Err(IdError::invalid(row.name, input));
        };
        if digits.len() < 2 || !matches_shape(row.public_shape, input) {
            return Err(IdError::invalid(row.name, input));
        }
        let number = digits
            .parse::<u32>()
            .map_err(|_| IdError::overflow(row.name, input))?;
        Ok(Self {
            raw: input.to_owned(),
            number,
        })
    }

    /// Returns the numeric sequence value of the conflict.
    #[must_use]
    pub const fn number(&self) -> u32 {
        self.number
    }
}

impl_id_common!(ConflictId);

/// A public glossary term identifier (`material_weakness`, at most 64 characters).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TermId {
    raw: String,
}

impl TermId {
    /// Parses a snake_case glossary term identifier (`^[a-z][a-z0-9]*(_[a-z0-9]+)*$`, at most 64 characters).
    pub fn parse(input: &str) -> Result<Self, IdError> {
        let row = &FAMILIES[IDX_TERM];
        if input.len() > MAX_SLUG_ID_LEN || !matches_shape(row.public_shape, input) {
            return Err(IdError::invalid(row.name, input));
        }
        Ok(Self {
            raw: input.to_owned(),
        })
    }
}

impl_id_common!(TermId);

/// A calculation case identifier (`CALC-01`, `CALC-100`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CaseId {
    raw: String,
    prefix_len: usize,
    number: u32,
}

impl CaseId {
    /// Parses a calculation case identifier (`^[A-Z]{2,4}-[0-9]{2,3}$` with a prefix in `FAMILIES`).
    pub fn parse(input: &str) -> Result<Self, IdError> {
        let row = &FAMILIES[IDX_CASE];
        let Some((prefix, digits)) = input.split_once('-') else {
            return Err(IdError::invalid(row.name, input));
        };
        if !row.prefixes.contains(&prefix) || !(2..=3).contains(&digits.len()) {
            return Err(IdError::invalid(row.name, input));
        }
        let number = digits
            .parse::<u32>()
            .map_err(|_| IdError::invalid(row.name, input))?;
        if !matches_shape(row.public_shape, input) {
            return Err(IdError::invalid(row.name, input));
        }
        Ok(Self {
            raw: input.to_owned(),
            prefix_len: prefix.len(),
            number,
        })
    }

    /// Returns the uppercase family prefix (`CALC` for `CALC-01`).
    #[must_use]
    pub fn prefix(&self) -> &str {
        &self.raw[..self.prefix_len]
    }

    /// Returns the numeric part of the case id (`1` for `CALC-01`, `100` for `CALC-100`).
    #[must_use]
    pub const fn number(&self) -> u32 {
        self.number
    }
}

impl_id_common!(CaseId);

/// Returns `true` when `input` has the error-code form (`^FB[0-9]{4}$`).
#[must_use]
pub fn is_error_code(input: &str) -> bool {
    let row = &FAMILIES[IDX_ERROR_CODE];
    let Some(prefix) = row.prefixes.first().copied() else {
        return false;
    };
    input.starts_with(prefix) && matches_shape(row.public_shape, input)
}

/// An architectural decision record identifier (`0002-corpus-format-v1`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AdrId {
    raw: String,
}

impl AdrId {
    /// Parses an ADR identifier (`^[0-9]{4}-[a-z0-9]+(-[a-z0-9]+)*$`).
    pub fn parse(input: &str) -> Result<Self, IdError> {
        let row = &FAMILIES[IDX_ADR];
        if !matches_shape(row.public_shape, input) {
            return Err(IdError::invalid(row.name, input));
        }
        Ok(Self {
            raw: input.to_owned(),
        })
    }
}

impl_id_common!(AdrId);

/// A Corpus edition identifier (`edition-007`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EditionId {
    raw: String,
    number: u32,
}

impl EditionId {
    /// Parses an edition identifier (`^edition-[0-9]{3,}$`).
    pub fn parse(input: &str) -> Result<Self, IdError> {
        let row = &FAMILIES[IDX_EDITION];
        let Some(prefix) = row.prefixes.first().copied() else {
            return Err(IdError::invalid(row.name, input));
        };
        let Some(digits) = input.strip_prefix(prefix).and_then(|s| s.strip_prefix('-')) else {
            return Err(IdError::invalid(row.name, input));
        };
        if digits.len() < 3 || !matches_shape(row.public_shape, input) {
            return Err(IdError::invalid(row.name, input));
        }
        let number = digits
            .parse::<u32>()
            .map_err(|_| IdError::overflow(row.name, input))?;
        Ok(Self {
            raw: input.to_owned(),
            number,
        })
    }

    /// Returns the numeric edition sequence value.
    #[must_use]
    pub const fn number(&self) -> u32 {
        self.number
    }
}

impl_id_common!(EditionId);

/// A tool release tag identifier (`v0.2.0`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReleaseId {
    raw: String,
}

/// Alias for [`ReleaseId`].
pub type ToolRelease = ReleaseId;

impl ReleaseId {
    /// Parses a tool release identifier (`^v[0-9]+\.[0-9]+\.[0-9]+$`).
    pub fn parse(input: &str) -> Result<Self, IdError> {
        let row = &FAMILIES[IDX_TOOL_RELEASE];
        let Some(prefix) = row.prefixes.first().copied() else {
            return Err(IdError::invalid(row.name, input));
        };
        let Some(rest) = input.strip_prefix(prefix) else {
            return Err(IdError::invalid(row.name, input));
        };
        if !matches_shape(row.public_shape, input) {
            return Err(IdError::invalid(row.name, input));
        }
        for part in rest.split('.') {
            let _ = part
                .parse::<u32>()
                .map_err(|_| IdError::overflow(row.name, input))?;
        }
        Ok(Self {
            raw: input.to_owned(),
        })
    }
}

impl_id_common!(ReleaseId);

/// Allocates the next [`SourceId`] after the highest existing source id
/// (retired rows included), starting at `S-0001` and exhausting after `S-9999`.
pub fn next_source_id<I, S>(existing: I) -> Result<SourceId, IdError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let form = FAMILIES[IDX_SOURCE].name;
    let mut max_num = 0u32;
    let mut max_raw = String::new();
    for item in existing {
        let parsed = SourceId::parse(item.as_ref())?;
        if parsed.number > max_num {
            max_num = parsed.number;
            max_raw = parsed.raw;
        }
    }
    let next_num = max_num
        .checked_add(1)
        .ok_or_else(|| IdError::overflow(form, max_raw.clone()))?;
    if next_num > MAX_SOURCE_NUMBER {
        return Err(IdError::exhausted(form, max_raw));
    }
    SourceId::parse(&format!("S-{next_num:04}"))
}

/// Allocates the next [`ClaimId`] in `area` after the highest existing claim id
/// for that area (retired rows included), starting at `CL-AAA-0001`, continuing
/// with five digits at `CL-AAA-10000`, and exhausting after `CL-AAA-99999`.
pub fn next_claim_id<A, I, S>(area: A, existing: I) -> Result<ClaimId, IdError>
where
    A: AsRef<str>,
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let form = FAMILIES[IDX_CLAIM].name;
    let target_area = AreaCode::parse(area.as_ref())?;
    let mut max_num = 0u32;
    let mut max_raw = String::new();
    for item in existing {
        let parsed = ClaimId::parse(item.as_ref())?;
        if parsed.area == target_area && parsed.number > max_num {
            max_num = parsed.number;
            max_raw = parsed.raw;
        }
    }
    let next_num = max_num
        .checked_add(1)
        .ok_or_else(|| IdError::overflow(form, max_raw.clone()))?;
    if next_num > MAX_CLAIM_NUMBER {
        return Err(IdError::exhausted(form, max_raw));
    }
    let area_upper = target_area.as_str().to_ascii_uppercase();
    let formatted = if next_num < 10_000 {
        format!("CL-{area_upper}-{next_num:04}")
    } else {
        format!("CL-{area_upper}-{next_num}")
    };
    ClaimId::parse(&formatted)
}

/// Allocates the next [`QuestionId`] after the highest existing question id
/// (retired rows included), starting at `Q-001` and exhausting after `Q-999`.
pub fn next_question_id<I, S>(existing: I) -> Result<QuestionId, IdError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let form = FAMILIES[IDX_QUESTION].name;
    let mut max_num = 0u32;
    let mut max_raw = String::new();
    for item in existing {
        let parsed = QuestionId::parse(item.as_ref())?;
        if parsed.number > max_num {
            max_num = parsed.number;
            max_raw = parsed.raw;
        }
    }
    let next_num = max_num
        .checked_add(1)
        .ok_or_else(|| IdError::overflow(form, max_raw.clone()))?;
    if next_num > MAX_QUESTION_NUMBER {
        return Err(IdError::exhausted(form, max_raw));
    }
    QuestionId::parse(&format!("Q-{next_num:03}"))
}

/// Allocates the next [`ConflictId`] after the highest existing conflict id
/// (retired rows included), starting at `XC-01` and continuing past `XC-9999`
/// (`XC-10000`) up to checked integer representation overflow.
pub fn next_conflict_id<I, S>(existing: I) -> Result<ConflictId, IdError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let form = FAMILIES[IDX_CONFLICT].name;
    let mut max_num = 0u32;
    let mut max_raw = String::new();
    for item in existing {
        let parsed = ConflictId::parse(item.as_ref())?;
        if parsed.number > max_num {
            max_num = parsed.number;
            max_raw = parsed.raw;
        }
    }
    let next_num = max_num
        .checked_add(1)
        .ok_or_else(|| IdError::overflow(form, max_raw))?;
    let formatted = if next_num < 100 {
        format!("XC-{next_num:02}")
    } else {
        format!("XC-{next_num}")
    };
    ConflictId::parse(&formatted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_id_rejects_a_two_letter_area() {
        let err = ClaimId::parse("CL-CM-0011").expect_err("two-letter area must be rejected");
        assert_eq!(err.form(), "claim");
        assert_eq!(err.input(), "CL-CM-0011");
    }

    #[test]
    fn question_allocator_exhausts_after_999() {
        // Active and withdrawn/retired question rows together reach Q-999.
        let before_cap = ["Q-001", "Q-450", "Q-998"];
        let q999 = next_question_id(before_cap).expect("Q-999 is the last valid question id");
        assert_eq!(q999.as_str(), "Q-999");

        let with_retired_999 = ["Q-001", "Q-025", "Q-999"];
        let err =
            next_question_id(with_retired_999).expect_err("question series must exhaust after 999");
        assert_eq!(err.form(), "question");
        assert_eq!(err.kind(), IdErrorKind::Exhausted);
    }

    #[test]
    fn conflict_allocator_continues_after_9999() {
        // Resolved/retired conflict rows included: XC-9999 is followed by XC-10000.
        let existing_rows = ["XC-01", "XC-14", "XC-9999"];
        let next = next_conflict_id(existing_rows)
            .expect("conflict allocator has no four-digit cap and continues at XC-10000");
        assert_eq!(next.as_str(), "XC-10000");
        assert_eq!(next.number(), 10_000);

        let after_10000 = [next.as_str()];
        let next_2 = next_conflict_id(after_10000).expect("continues at XC-10001");
        assert_eq!(next_2.as_str(), "XC-10001");

        // Representation overflow only at u32::MAX.
        let at_u32_max = ["XC-4294967295"];
        let overflow =
            next_conflict_id(at_u32_max).expect_err("u32::MAX must return representation overflow");
        assert_eq!(overflow.form(), "conflict");
        assert_eq!(overflow.kind(), IdErrorKind::Overflow);
    }

    #[test]
    fn table_driven_identifier_cases() {
        let page_64 = "cmp-012345678901234567890123456789012345678901234567890123456789";
        let page_65 = "cmp-0123456789012345678901234567890123456789012345678901234567890";
        let term_64 = "a012345678901234567890123456789012345678901234567890123456789012";
        let term_65 = "a0123456789012345678901234567890123456789012345678901234567890123";
        assert_eq!(page_64.len(), 64);
        assert_eq!(page_65.len(), 65);
        assert_eq!(term_64.len(), 64);
        assert_eq!(term_65.len(), 65);

        let cases: &[(&str, &str, bool)] = &[
            // Area code (5 cases)
            ("area", "cmp", true),
            ("area", "est", true),
            ("area", "cm", false),
            ("area", "cmpx", false),
            ("area", "CMP", false),
            // Page (6 cases)
            ("page", "cmp-withholding", true),
            ("page", "est-cost-build-01", true),
            ("page", page_64, true),
            ("page", page_65, false),
            ("page", "cmp", false),
            ("page", "cmp-withholding-", false),
            // Source (5 cases)
            ("source", "S-0042", true),
            ("source", "S-0001", true),
            ("source", "S-42", false),
            ("source", "S-00042", false),
            ("source", "s-0042", false),
            // Source version (5 cases)
            ("source_version", "S-0042-v2", true),
            ("source_version", "S-0042-v10", true),
            ("source_version", "S-0042-v0", false),
            ("source_version", "S-0042-v01", false),
            ("source_version", "S-42-v1", false),
            // Claim (6 cases)
            ("claim", "CL-CMP-0011", true),
            ("claim", "CL-CMP-10000", true),
            ("claim", "CL-CM-0011", false),
            ("claim", "CL-CMP-01000", false),
            ("claim", "CL-cmp-0011", false),
            ("claim", "CL-CMP-100000", false),
            // Question (4 cases)
            ("question", "Q-025", true),
            ("question", "Q-999", true),
            ("question", "Q-1000", false),
            ("question", "Q-25", false),
            // Conflict (4 cases)
            ("conflict", "XC-14", true),
            ("conflict", "XC-10000", true),
            ("conflict", "XC-1", false),
            ("conflict", "xc-14", false),
            // Term (5 cases)
            ("term", "material_weakness", true),
            ("term", term_64, true),
            ("term", term_65, false),
            ("term", "1term", false),
            ("term", "material__weakness", false),
            // Calculation case (7 cases)
            ("case", "CALC-01", true),
            ("case", "CALC-100", true),
            ("case", "RTE-99", true),
            ("case", "CTF-001", true),
            ("case", "CALC-1", false),
            ("case", "CALC-1000", false),
            ("case", "ZZ-01", false),
            // Error code (4 cases)
            ("error_code", "FB0102", true),
            ("error_code", "FB0100", true),
            ("error_code", "FB-0102", false),
            ("error_code", "FB102", false),
            // ADR (3 cases)
            ("adr", "0002-corpus-format-v1", true),
            ("adr", "002-short", false),
            ("adr", "0002-", false),
            // Edition (3 cases)
            ("edition", "edition-007", true),
            ("edition", "edition-1000", true),
            ("edition", "edition-07", false),
            // Tool release (3 cases)
            ("tool_release", "v0.2.0", true),
            ("tool_release", "0.2.0", false),
            ("tool_release", "v0.2", false),
        ];

        assert!(
            cases.len() >= 40,
            "expected at least 40 table-driven cases, got {}",
            cases.len()
        );

        for &(family, input, expected_ok) in cases {
            let actual_ok = match family {
                "area" => AreaCode::parse(input)
                    .map(|v| v.to_string() == input)
                    .unwrap_or(false),
                "page" => PageId::parse(input)
                    .map(|v| v.to_string() == input)
                    .unwrap_or(false),
                "source" => SourceId::parse(input)
                    .map(|v| v.to_string() == input)
                    .unwrap_or(false),
                "source_version" => VersionId::parse(input)
                    .map(|v| v.to_string() == input)
                    .unwrap_or(false),
                "claim" => ClaimId::parse(input)
                    .map(|v| v.to_string() == input)
                    .unwrap_or(false),
                "question" => QuestionId::parse(input)
                    .map(|v| v.to_string() == input)
                    .unwrap_or(false),
                "conflict" => ConflictId::parse(input)
                    .map(|v| v.to_string() == input)
                    .unwrap_or(false),
                "term" => TermId::parse(input)
                    .map(|v| v.to_string() == input)
                    .unwrap_or(false),
                "case" => CaseId::parse(input)
                    .map(|v| v.to_string() == input)
                    .unwrap_or(false),
                "error_code" => is_error_code(input),
                "adr" => AdrId::parse(input)
                    .map(|v| v.to_string() == input)
                    .unwrap_or(false),
                "edition" => EditionId::parse(input)
                    .map(|v| v.to_string() == input)
                    .unwrap_or(false),
                "tool_release" => ReleaseId::parse(input)
                    .map(|v| v.to_string() == input)
                    .unwrap_or(false),
                other => panic!("unknown family in table: {other}"),
            };
            assert_eq!(
                actual_ok, expected_ok,
                "family {family} on input {input:?}: expected {expected_ok}, got {actual_ok}"
            );
        }
    }

    #[test]
    fn ordering_and_derived_parts() {
        let c_9999 = ClaimId::parse("CL-CMP-9999").expect("valid 4-digit claim");
        let c_10000 = ClaimId::parse("CL-CMP-10000").expect("valid 5-digit claim");
        let c_est = ClaimId::parse("CL-EST-0001").expect("valid est claim");
        assert!(c_9999 < c_10000);
        assert!(c_10000 < c_est);
        assert_eq!(c_9999.area().as_str(), "cmp");
        assert_eq!(c_9999.number(), 9_999);
        assert_eq!(c_10000.number(), 10_000);

        let v = VersionId::parse("S-0042-v2").expect("valid version id");
        assert_eq!(v.source().as_str(), "S-0042");
        assert_eq!(v.number(), 2);
        assert_eq!(v.version(), 2);

        let p = PageId::parse("cmp-withholding").expect("valid page id");
        assert_eq!(p.area().as_str(), "cmp");

        let q = QuestionId::parse("Q-025").expect("valid question id");
        assert_eq!(q.number(), 25);

        let xc = ConflictId::parse("XC-14").expect("valid conflict id");
        assert_eq!(xc.number(), 14);

        let case_id = CaseId::parse("CALC-100").expect("valid case id");
        assert_eq!(case_id.prefix(), "CALC");
        assert_eq!(case_id.number(), 100);
    }

    #[test]
    fn source_and_claim_allocators_start_skip_gaps_and_exhaust() {
        let empty: [&str; 0] = [];
        assert_eq!(
            next_source_id(empty).expect("first source").as_str(),
            "S-0001"
        );
        assert_eq!(
            next_source_id(["S-0001", "S-0041"])
                .expect("gap source")
                .as_str(),
            "S-0042"
        );
        assert!(next_source_id(["S-9999"]).is_err());

        assert_eq!(
            next_claim_id("cmp", empty).expect("first claim").as_str(),
            "CL-CMP-0001"
        );
        assert_eq!(
            next_claim_id("cmp", ["CL-EST-0050", "CL-CMP-9999"])
                .expect("5-digit claim transition")
                .as_str(),
            "CL-CMP-10000"
        );
        assert!(next_claim_id("cmp", ["CL-CMP-99999"]).is_err());
    }
}
