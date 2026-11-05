//! Non-currency `Unit` vocabulary of the Corpus format.

use std::fmt;
use std::str::FromStr;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::decimal::Dec;
use crate::error::CalcError;
use crate::ops::{gcd_u128, scale_dec_by_ratio};
use crate::value::Value;

/// Basis family of a [`Unit`] (AD-14).
///
/// Conversion exists only within a single [`UnitFamily`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnitFamily {
    /// `days` (1) and `weeks` (7).
    Days,
    /// `months` (1) and `years` (12).
    Months,
    /// `hours` (1).
    Hours,
    /// `business_days` (1).
    BusinessDays,
    /// `percent` (1) and `basis_points` (1/100).
    Percent,
    /// `ratio` (1).
    Ratio,
    /// `count` (1).
    Count,
    /// Price bases: `each` (1), `per_100` (100), `per_1000` (1000), `per_dozen` (12).
    PriceBasis,
}

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

    /// Returns the [`UnitFamily`] to which this unit belongs.
    #[must_use]
    pub const fn family(self) -> UnitFamily {
        match self {
            Self::Days | Self::Weeks => UnitFamily::Days,
            Self::Months | Self::Years => UnitFamily::Months,
            Self::Hours => UnitFamily::Hours,
            Self::BusinessDays => UnitFamily::BusinessDays,
            Self::Percent | Self::BasisPoints => UnitFamily::Percent,
            Self::Ratio => UnitFamily::Ratio,
            Self::Count => UnitFamily::Count,
            Self::Each | Self::Per100 | Self::Per1000 | Self::PerDozen => UnitFamily::PriceBasis,
        }
    }

    /// Exact rational conversion factor `(numerator, denominator)` to this family's base unit
    /// for ordinary (non-price-basis) families. For price-basis units, returns the basis
    /// denominator `(basis, 1)`.
    #[must_use]
    pub const fn factor(self) -> (u128, u128) {
        match self {
            Self::Days
            | Self::Months
            | Self::Hours
            | Self::BusinessDays
            | Self::Percent
            | Self::Ratio
            | Self::Count
            | Self::Each => (1, 1),
            Self::Weeks => (7, 1),
            Self::Years | Self::PerDozen => (12, 1),
            Self::BasisPoints => (1, 100),
            Self::Per100 => (100, 1),
            Self::Per1000 => (1000, 1),
        }
    }

    /// Returns the denominator basis (`1`, `100`, `1000`, `12`) when `self` is in
    /// [`UnitFamily::PriceBasis`], or `None` for ordinary families.
    #[must_use]
    pub const fn price_basis(self) -> Option<u128> {
        match self {
            Self::Each => Some(1),
            Self::Per100 => Some(100),
            Self::Per1000 => Some(1000),
            Self::PerDozen => Some(12),
            Self::Percent
            | Self::BasisPoints
            | Self::Days
            | Self::BusinessDays
            | Self::Weeks
            | Self::Months
            | Self::Years
            | Self::Hours
            | Self::Count
            | Self::Ratio => None,
        }
    }
}

/// Converts `amount` from `from` to `to` within the same [`UnitFamily`].
///
/// - Across two different families, returns `Err(CalcError::Mismatch)` (`FB0502`).
/// - Ordinary families compute `amount × from_factor / to_factor`.
/// - Price-basis units (`each`, `per_100`, `per_1000`, `per_dozen`) are denominators and
///   compute `amount × to_basis / from_basis`.
/// - Integer factors are cancelled before the representability test; a non-representable
///   exact result returns `Ok(Value::Unknown(Reason::Inexact))`.
pub fn convert(amount: Dec, from: Unit, to: Unit) -> Result<Value<Dec>, CalcError> {
    if from.family() != to.family() {
        return Err(CalcError::Mismatch);
    }
    let (raw_num, raw_den) = match (from.price_basis(), to.price_basis()) {
        (Some(from_basis), Some(to_basis)) => (to_basis, from_basis),
        _ => {
            let (fn_num, fn_den) = from.factor();
            let (tn_num, tn_den) = to.factor();
            (fn_num * tn_den, fn_den * tn_num)
        }
    };
    let g = gcd_u128(raw_num, raw_den);
    scale_dec_by_ratio(amount, raw_num / g, raw_den / g)
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
