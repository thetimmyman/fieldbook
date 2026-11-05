//! Validated label identifier (`LabelId`) and the four built-in handling labels
//! (`docs/decisions/0004-company-boundary.md#2-labels`,
//! `docs/reference/corpus-format.md#handling-and-labels`).

use std::borrow::{Borrow, Cow};
use std::fmt;
use std::ops::Deref;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::schema::vocab::label_form;

/// Built-in label for a public source original or synthetic example (`"public"`).
pub const PUBLIC: LabelId = LabelId(Cow::Borrowed("public"));

/// Built-in label assigned at intake when no label could be found (`"unlabelled"`).
pub const UNLABELLED: LabelId = LabelId(Cow::Borrowed("unlabelled"));

/// Built-in label assigned at intake when a marking or native label has no mapping rule (`"marked"`).
pub const MARKED: LabelId = LabelId(Cow::Borrowed("marked"));

/// Built-in request-only label for text typed or pasted by the owner at a prompt (`"owner-supplied"`).
pub const OWNER_SUPPLIED: LabelId = LabelId(Cow::Borrowed("owner-supplied"));

/// The four built-in label identifiers, in ADR 0004 table order.
pub const BUILTINS: [LabelId; 4] = [PUBLIC, UNLABELLED, MARKED, OWNER_SUPPLIED];

/// Error returned when a string is not a valid [`LabelId`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelIdError {
    value: String,
}

impl LabelIdError {
    /// Returns the rejected string value.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

impl fmt::Display for LabelIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid label id {:?}: expected a built-in id or `[a-z]{{2,4}}-[0-9]{{4}}`",
            self.value
        )
    }
}

impl std::error::Error for LabelIdError {}

/// A validated handling label identifier: one of [`BUILTINS`] or an opaque company code `[a-z]{2,4}-[0-9]{4}`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LabelId(Cow<'static, str>);

impl LabelId {
    /// Parses and validates a label identifier using [`crate::schema::vocab::label_form`].
    pub fn parse(input: &str) -> Result<Self, LabelIdError> {
        for builtin in BUILTINS {
            if input == builtin.as_str() {
                return Ok(builtin);
            }
        }
        if label_form(input) {
            Ok(Self(Cow::Owned(input.to_owned())))
        } else {
            Err(LabelIdError {
                value: input.to_owned(),
            })
        }
    }

    /// Returns the canonical string slice of this label identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns `true` when this identifier is one of the four built-in labels ([`BUILTINS`]).
    #[must_use]
    pub fn is_builtin(&self) -> bool {
        BUILTINS.iter().any(|b| b.as_str() == self.as_str())
    }
}

impl Deref for LabelId {
    type Target = str;

    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<str> for LabelId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for LabelId {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for LabelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for LabelId {
    type Err = LabelIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl PartialEq<str> for LabelId {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for LabelId {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<LabelId> for str {
    fn eq(&self, other: &LabelId) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<LabelId> for &str {
    fn eq(&self, other: &LabelId) -> bool {
        *self == other.as_str()
    }
}

impl Serialize for LabelId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for LabelId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_builtins_and_the_opaque_shape() {
        for builtin in ["public", "unlabelled", "marked", "owner-supplied"] {
            let parsed = LabelId::parse(builtin);
            assert!(parsed.is_ok(), "expected builtin {builtin} to parse");
            assert_eq!(parsed.as_ref().map(LabelId::as_str), Ok(builtin));
            assert!(parsed.is_ok_and(|id| id.is_builtin()));
        }

        for opaque in ["np-0007", "bid-0003", "ctl-0002", "abcd-9999", "xy-0000"] {
            let parsed = LabelId::parse(opaque);
            assert!(parsed.is_ok(), "expected opaque id {opaque} to parse");
            assert_eq!(parsed.as_ref().map(LabelId::as_str), Ok(opaque));
            assert!(parsed.is_ok_and(|id| !id.is_builtin()));
        }
    }

    #[test]
    fn rejects_a_name_upper_case_a_long_prefix_a_short_number_and_empty() {
        for bad in [
            "",
            "acme",
            "acme-corp",
            "NP-0007",
            "Np-0007",
            "PUBLIC",
            "abcde-0007",
            "a-0007",
            "np-007",
            "np-00007",
            "np-000a",
            "np_0007",
            "np-0007 ",
        ] {
            assert!(
                LabelId::parse(bad).is_err(),
                "expected {bad:?} to be rejected by LabelId::parse"
            );
        }
    }

    #[test]
    fn builtins_are_the_four_ids() {
        assert_eq!(
            BUILTINS.each_ref().map(LabelId::as_str),
            ["public", "unlabelled", "marked", "owner-supplied"]
        );
        assert_eq!(PUBLIC.as_str(), "public");
        assert_eq!(UNLABELLED.as_str(), "unlabelled");
        assert_eq!(MARKED.as_str(), "marked");
        assert_eq!(OWNER_SUPPLIED.as_str(), "owner-supplied");
    }
}
