//! ISO 8601 civil dates (`YYYY-MM-DD`) and partial dates (`YYYY`, `YYYY-MM`, `YYYY-MM-DD`).
//!
//! Nothing in this module reads a clock.

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::CalcError;

/// Epoch day of `0001-01-01` relative to `1970-01-01` (day `0`).
const MIN_EPOCH_DAYS: i64 = -719_162;

/// Epoch day of `9999-12-31` relative to `1970-01-01` (day `0`).
const MAX_EPOCH_DAYS: i64 = 2_932_896;

/// Seconds per civil day (`24 * 60 * 60`).
const SECONDS_PER_DAY: i64 = 86_400;

/// Proleptic Gregorian calendar date (`YYYY-MM-DD`) in years `0001` through `9999`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CivilDate {
    year: u16,
    month: u8,
    day: u8,
}

impl CivilDate {
    /// Earliest representable date (`0001-01-01`).
    pub const MIN: Self = Self {
        year: 1,
        month: 1,
        day: 1,
    };

    /// Latest representable date (`9999-12-31`).
    pub const MAX: Self = Self {
        year: 9999,
        month: 12,
        day: 31,
    };

    /// Constructs a `CivilDate` from year (`1..=9999`), month (`1..=12`) and day (`1..=31`),
    /// checking leap-year rules.
    pub const fn from_ymd(year: u16, month: u8, day: u8) -> Result<Self, CalcError> {
        if year < 1 || year > 9999 {
            return Err(CalcError::Grammar);
        }
        let Some(max_day) = Self::days_in_month(year, month) else {
            return Err(CalcError::Grammar);
        };
        if day < 1 || day > max_day {
            return Err(CalcError::Grammar);
        }
        Ok(Self { year, month, day })
    }

    /// Parses a `YYYY-MM-DD` string into a `CivilDate`.
    ///
    /// Any other form or impossible calendar date returns [`CalcError::Grammar`].
    pub fn parse(s: &str) -> Result<Self, CalcError> {
        let [y0, y1, y2, y3, b'-', m0, m1, b'-', d0, d1] = s.as_bytes() else {
            return Err(CalcError::Grammar);
        };
        let year = parse_four_digits(*y0, *y1, *y2, *y3)?;
        let month = parse_two_digits(*m0, *m1)?;
        let day = parse_two_digits(*d0, *d1)?;
        Self::from_ymd(year, month, day)
    }

    /// Returns the four-digit year (`1..=9999`).
    #[must_use]
    pub const fn year(self) -> u16 {
        self.year
    }

    /// Returns the month (`1..=12`).
    #[must_use]
    pub const fn month(self) -> u8 {
        self.month
    }

    /// Returns the day of the month (`1..=31`).
    #[must_use]
    pub const fn day(self) -> u8 {
        self.day
    }

    /// Returns `true` if `year` is a Gregorian leap year.
    #[must_use]
    pub const fn is_leap_year(year: u16) -> bool {
        (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
    }

    /// Returns the number of days in `month` (`1..=12`) of `year`, or `None` if `month` is out of range.
    #[must_use]
    pub const fn days_in_month(year: u16, month: u8) -> Option<u8> {
        match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
            4 | 6 | 9 | 11 => Some(30),
            2 => Some(if Self::is_leap_year(year) { 29 } else { 28 }),
            _ => None,
        }
    }

    /// Adds a signed number of days to `self`.
    ///
    /// Returns [`CalcError::Overflow`] if the resulting date lies outside `0001-01-01..=9999-12-31`.
    pub const fn add_days(self, days: i64) -> Result<Self, CalcError> {
        let Some(target) = self.to_epoch_days().checked_add(days) else {
            return Err(CalcError::Overflow);
        };
        Self::from_epoch_days(target)
    }

    /// Returns the signed number of days from `self` to `other` (`other - self`).
    ///
    /// Positive when `other > self`, negative when `other < self`, and `0` when equal.
    #[must_use]
    pub const fn days_between(self, other: Self) -> i64 {
        other.to_epoch_days() - self.to_epoch_days()
    }

    /// Converts a Unix timestamp in seconds (such as `SOURCE_DATE_EPOCH`) to its UTC `CivilDate`.
    ///
    /// Returns [`CalcError::Overflow`] if the date lies outside `0001-01-01..=9999-12-31`.
    pub const fn from_epoch_seconds(seconds: i64) -> Result<Self, CalcError> {
        let days = seconds.div_euclid(SECONDS_PER_DAY);
        Self::from_epoch_days(days)
    }

    /// Converts `self` to days since `1970-01-01` (Howard Hinnant's `days_from_civil`).
    #[must_use]
    pub const fn to_epoch_days(self) -> i64 {
        let y = self.year as i64 - if self.month <= 2 { 1 } else { 0 };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = y - era * 400;
        let m = self.month as i64;
        let d = self.day as i64;
        let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// Converts days since `1970-01-01` to a `CivilDate` in `0001-01-01..=9999-12-31`.
    pub const fn from_epoch_days(z: i64) -> Result<Self, CalcError> {
        if z < MIN_EPOCH_DAYS || z > MAX_EPOCH_DAYS {
            return Err(CalcError::Overflow);
        }
        let z_shifted = z + 719_468;
        let era = if z_shifted >= 0 {
            z_shifted
        } else {
            z_shifted - 146_096
        } / 146_097;
        let doe = z_shifted - era * 146_097;
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = y + if m <= 2 { 1 } else { 0 };
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "range check on z guarantees year in 1..=9999, m in 1..=12, d in 1..=31"
        )]
        Ok(Self {
            year: year as u16,
            month: m as u8,
            day: d as u8,
        })
    }
}

impl fmt::Display for CivilDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

impl FromStr for CivilDate {
    type Err = CalcError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Serialize for CivilDate {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

struct CivilDateVisitor;

impl Visitor<'_> for CivilDateVisitor {
    type Value = CivilDate;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an ISO 8601 calendar date YYYY-MM-DD")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        CivilDate::parse(value).map_err(de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for CivilDate {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(CivilDateVisitor)
    }
}

/// A date written at year (`YYYY`), month (`YYYY-MM`) or day (`YYYY-MM-DD`) precision.
///
/// Prints at its original precision and orders by its first calendar day (`YYYY-01-01` or
/// `YYYY-MM-01`), without ever turning a partial date into a full [`CivilDate`] (R-06).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PartialDate {
    /// Year-only date (`YYYY`).
    Year(u16),
    /// Year-and-month date (`YYYY-MM`).
    YearMonth(u16, u8),
    /// Full calendar date (`YYYY-MM-DD`).
    Full(CivilDate),
}

impl PartialDate {
    /// Parses `YYYY`, `YYYY-MM` or `YYYY-MM-DD`.
    ///
    /// Any other form or impossible calendar date returns [`CalcError::Grammar`].
    pub fn parse(s: &str) -> Result<Self, CalcError> {
        match s.as_bytes() {
            [y0, y1, y2, y3] => {
                let year = parse_four_digits(*y0, *y1, *y2, *y3)?;
                if !(1..=9999).contains(&year) {
                    return Err(CalcError::Grammar);
                }
                Ok(Self::Year(year))
            }
            [y0, y1, y2, y3, b'-', m0, m1] => {
                let year = parse_four_digits(*y0, *y1, *y2, *y3)?;
                let month = parse_two_digits(*m0, *m1)?;
                if !(1..=9999).contains(&year) || !(1..=12).contains(&month) {
                    return Err(CalcError::Grammar);
                }
                Ok(Self::YearMonth(year, month))
            }
            _ => CivilDate::parse(s).map(Self::Full),
        }
    }

    /// Returns the first calendar day of this partial date (`YYYY-01-01`, `YYYY-MM-01`, or the full date).
    #[must_use]
    pub const fn first_day(self) -> CivilDate {
        match self {
            Self::Year(year) => CivilDate {
                year,
                month: 1,
                day: 1,
            },
            Self::YearMonth(year, month) => CivilDate {
                year,
                month,
                day: 1,
            },
            Self::Full(date) => date,
        }
    }

    /// Returns `Some(CivilDate)` only when the date was written in full (`YYYY-MM-DD`).
    ///
    /// Year-only and year-month dates return `None` (R-06).
    #[must_use]
    pub const fn to_civil(self) -> Option<CivilDate> {
        match self {
            Self::Full(date) => Some(date),
            Self::Year(_) | Self::YearMonth(_, _) => None,
        }
    }

    /// Returns the four-digit year (`1..=9999`).
    #[must_use]
    pub const fn year(self) -> u16 {
        match self {
            Self::Year(year) | Self::YearMonth(year, _) => year,
            Self::Full(date) => date.year(),
        }
    }

    const fn precision_rank(self) -> u8 {
        match self {
            Self::Year(_) => 0,
            Self::YearMonth(_, _) => 1,
            Self::Full(_) => 2,
        }
    }
}

impl PartialOrd for PartialDate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PartialDate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.first_day()
            .cmp(&other.first_day())
            .then_with(|| self.precision_rank().cmp(&other.precision_rank()))
    }
}

impl fmt::Display for PartialDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Year(year) => write!(f, "{year:04}"),
            Self::YearMonth(year, month) => write!(f, "{year:04}-{month:02}"),
            Self::Full(date) => fmt::Display::fmt(date, f),
        }
    }
}

impl FromStr for PartialDate {
    type Err = CalcError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl From<CivilDate> for PartialDate {
    fn from(date: CivilDate) -> Self {
        Self::Full(date)
    }
}

impl Serialize for PartialDate {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

struct PartialDateVisitor;

impl Visitor<'_> for PartialDateVisitor {
    type Value = PartialDate;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a partial or full ISO date (YYYY, YYYY-MM or YYYY-MM-DD)")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        PartialDate::parse(value).map_err(de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for PartialDate {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(PartialDateVisitor)
    }
}

fn parse_digit(b: u8) -> Result<u8, CalcError> {
    if b.is_ascii_digit() {
        Ok(b - b'0')
    } else {
        Err(CalcError::Grammar)
    }
}

fn parse_two_digits(b0: u8, b1: u8) -> Result<u8, CalcError> {
    let d0 = parse_digit(b0)?;
    let d1 = parse_digit(b1)?;
    Ok(d0 * 10 + d1)
}

fn parse_four_digits(b0: u8, b1: u8, b2: u8, b3: u8) -> Result<u16, CalcError> {
    let hi = parse_two_digits(b0, b1)?;
    let lo = parse_two_digits(b2, b3)?;
    Ok((hi as u16) * 100 + (lo as u16))
}
