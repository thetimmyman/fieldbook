//! Non-currency `Unit` vocabulary of the Corpus format.

use std::fmt;
use std::str::FromStr;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::CalcError;

/// Closed enum of the non-currency words of the `unit` vocabulary in the Corpus format.
///
/// Currency codes are represented by [`crate::Currency`] on [`crate::Money`], not by `Unit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Unit {
    /// `percent`
    Percent,
    /// `basis_points`
    BasisPoints,
    /// `days`
    Days,
    /// `business_days`
    BusinessDays,
    /// `weeks`
    Weeks,
    /// `months`
    Months,
    /// `years`
    Years,
    /// `hours`
    Hours,
    /// `count`
    Count,
    /// `ratio`
    Ratio,
    /// `each`
    Each,
    /// `per_100`
    Per100,
    /// `per_1000`
    Per1000,
    /// `per_dozen`
    PerDozen,
}

impl Unit {
    /// Every variant of `Unit`, in the order of the `unit` row in `corpus-format.md`.
    pub const ALL: &'static [Self] = &[
        Self::Percent,
        Self::BasisPoints,
        Self::Days,
        Self::BusinessDays,
        Self::Weeks,
        Self::Months,
        Self::Years,
        Self::Hours,
        Self::Count,
        Self::Ratio,
        Self::Each,
        Self::Per100,
        Self::Per1000,
        Self::PerDozen,
    ];

    /// Returns the canonical snake_case vocabulary word for this unit.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Percent => "percent",
            Self::BasisPoints => "basis_points",
            Self::Days => "days",
            Self::BusinessDays => "business_days",
            Self::Weeks => "weeks",
            Self::Months => "months",
            Self::Years => "years",
            Self::Hours => "hours",
            Self::Count => "count",
            Self::Ratio => "ratio",
            Self::Each => "each",
            Self::Per100 => "per_100",
            Self::Per1000 => "per_1000",
            Self::PerDozen => "per_dozen",
        }
    }

    /// Alias for [`Unit::word`].
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.word()
    }

    /// Parses a unit from its vocabulary word.
    ///
    /// An unknown word returns [`CalcError::Grammar`].
    pub fn from_word(s: &str) -> Result<Self, CalcError> {
        match s {
            "percent" => Ok(Self::Percent),
            "basis_points" => Ok(Self::BasisPoints),
            "days" => Ok(Self::Days),
            "business_days" => Ok(Self::BusinessDays),
            "weeks" => Ok(Self::Weeks),
            "months" => Ok(Self::Months),
            "years" => Ok(Self::Years),
            "hours" => Ok(Self::Hours),
            "count" => Ok(Self::Count),
            "ratio" => Ok(Self::Ratio),
            "each" => Ok(Self::Each),
            "per_100" => Ok(Self::Per100),
            "per_1000" => Ok(Self::Per1000),
            "per_dozen" => Ok(Self::PerDozen),
            _ => Err(CalcError::Grammar),
        }
    }

    /// Alias for [`Unit::from_word`].
    pub fn parse(s: &str) -> Result<Self, CalcError> {
        Self::from_word(s)
    }

    /// Returns `true` for time units (`days`, `business_days`, `weeks`, `months`, `years`, `hours`).
    #[must_use]
    pub const fn is_time(self) -> bool {
        matches!(
            self,
            Self::Days
                | Self::BusinessDays
                | Self::Weeks
                | Self::Months
                | Self::Years
                | Self::Hours
        )
    }
}

impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

impl FromStr for Unit {
    type Err = CalcError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_word(s)
    }
}

impl Serialize for Unit {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.word())
    }
}

struct UnitVisitor;

impl Visitor<'_> for UnitVisitor {
    type Value = Unit;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a unit word of the Corpus format")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        Unit::from_word(value).map_err(de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for Unit {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(UnitVisitor)
    }
}
