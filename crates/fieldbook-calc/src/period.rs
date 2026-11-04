//! Effective periods (`YYYY-MM-DD/YYYY-MM-DD` and `YYYY-MM-DD/`) and time durations.

use std::fmt;
use std::str::FromStr;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::date::CivilDate;
use crate::error::CalcError;
use crate::unit::Unit;

/// An inclusive effective-date interval (`R-15`), where `end: None` means "until superseded".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EffectivePeriod {
    /// Inclusive start date.
    pub start: CivilDate,
    /// Optional inclusive end date (`None` means open-ended / until superseded).
    pub end: Option<CivilDate>,
}

impl EffectivePeriod {
    /// Constructs an `EffectivePeriod`, returning [`CalcError::Grammar`] if `end < start`.
    pub fn new(start: CivilDate, end: Option<CivilDate>) -> Result<Self, CalcError> {
        if let Some(end_date) = end
            && end_date < start
        {
            return Err(CalcError::Grammar);
        }
        Ok(Self { start, end })
    }

    /// Parses `YYYY-MM-DD/YYYY-MM-DD` or `YYYY-MM-DD/`.
    ///
    /// Returns [`CalcError::Grammar`] if the syntax is invalid or `end < start`.
    pub fn parse(s: &str) -> Result<Self, CalcError> {
        let Some((start_str, end_str)) = s.split_once('/') else {
            return Err(CalcError::Grammar);
        };
        let start = CivilDate::parse(start_str)?;
        let end = if end_str.is_empty() {
            None
        } else {
            Some(CivilDate::parse(end_str)?)
        };
        Self::new(start, end)
    }

    /// Returns `true` if `date` lies within `[start, end]` inclusive.
    #[must_use]
    pub fn contains(&self, date: CivilDate) -> bool {
        date >= self.start && self.end.is_none_or(|end_date| date <= end_date)
    }

    /// Returns `true` if `self` and `other` share at least one calendar day.
    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        self.end.is_none_or(|self_end| other.start <= self_end)
            && other.end.is_none_or(|other_end| self.start <= other_end)
    }
}

impl fmt::Display for EffectivePeriod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.end {
            Some(end_date) => write!(f, "{}/{end_date}", self.start),
            None => write!(f, "{}/", self.start),
        }
    }
}

impl FromStr for EffectivePeriod {
    type Err = CalcError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Serialize for EffectivePeriod {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

struct EffectivePeriodVisitor;

impl Visitor<'_> for EffectivePeriodVisitor {
    type Value = EffectivePeriod;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an effective period YYYY-MM-DD/YYYY-MM-DD or YYYY-MM-DD/")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        EffectivePeriod::parse(value).map_err(de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for EffectivePeriod {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(EffectivePeriodVisitor)
    }
}

/// A signed integer time duration (`amount` and a time [`Unit`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Duration {
    amount: i64,
    unit: Unit,
}

impl Duration {
    /// Creates a `Duration` if `unit.is_time()` is true; otherwise returns [`CalcError::Mismatch`].
    pub const fn new(amount: i64, unit: Unit) -> Result<Self, CalcError> {
        if !unit.is_time() {
            return Err(CalcError::Mismatch);
        }
        Ok(Self { amount, unit })
    }

    /// Returns the numeric amount of this duration.
    #[must_use]
    pub const fn amount(&self) -> i64 {
        self.amount
    }

    /// Returns the time [`Unit`] of this duration.
    #[must_use]
    pub const fn unit(&self) -> Unit {
        self.unit
    }

    /// Returns the exact length in calendar days for `days` and `weeks` (`R-17`),
    /// `None` for other time units, and [`CalcError::Overflow`] if week-to-day
    /// multiplication overflows `i64`.
    pub const fn days(&self) -> Result<Option<i64>, CalcError> {
        match self.unit {
            Unit::Days => Ok(Some(self.amount)),
            Unit::Weeks => match self.amount.checked_mul(7) {
                Some(total) => Ok(Some(total)),
                None => Err(CalcError::Overflow),
            },
            _ => Ok(None),
        }
    }
}
