//! Exact decimal wrapper parsed by the AD-14 decimal grammar.

use std::fmt;
use std::str::FromStr;

use rust_decimal::Decimal;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::CalcError;

/// Exact decimal value in `fieldbook-calc`.
///
/// Values enter through [`Dec::parse`], which validates the AD-14 grammar
/// (`^-?(0|[1-9][0-9]*)(\.[0-9]+)?$`) before calling [`Decimal::from_str_exact`].
/// `Serialize` and `Deserialize` operate on strings only, preserving the written scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Dec(pub(crate) Decimal);

impl Dec {
    /// Zero (`0`).
    pub const ZERO: Self = Self(Decimal::ZERO);
    /// One (`1`).
    pub const ONE: Self = Self(Decimal::ONE);
    /// One hundred (`100`).
    pub const ONE_HUNDRED: Self = Self(Decimal::ONE_HUNDRED);
    /// Minimum representable decimal.
    pub const MIN: Self = Self(Decimal::MIN);
    /// Maximum representable decimal.
    pub const MAX: Self = Self(Decimal::MAX);

    /// Parses a decimal string according to the AD-14 grammar.
    ///
    /// Strings that violate `^-?(0|[1-9][0-9]*)(\.[0-9]+)?$` (or negative zero) return
    /// [`CalcError::Grammar`]. Strings that match the grammar but exceed the range or
    /// 28-digit fractional scale of [`Decimal`] return [`CalcError::Overflow`].
    pub fn parse(s: &str) -> Result<Self, CalcError> {
        if !matches_decimal_grammar(s) {
            return Err(CalcError::Grammar);
        }
        let dec = Decimal::from_str_exact(s).map_err(|_| CalcError::Overflow)?;
        Ok(Self(dec))
    }

    /// Wraps a raw [`Decimal`].
    #[must_use]
    pub const fn from_decimal(value: Decimal) -> Self {
        Self(value)
    }

    /// Returns the underlying [`Decimal`].
    #[must_use]
    pub const fn inner(self) -> Decimal {
        self.0
    }

    /// Returns the unscaled 96-bit signed mantissa as `i128`.
    #[must_use]
    pub const fn mantissa(self) -> i128 {
        self.0.mantissa()
    }

    /// Returns the scale (number of digits after the decimal point).
    #[must_use]
    pub const fn scale(self) -> u32 {
        self.0.scale()
    }

    /// Returns `true` if the value is zero.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0.is_zero()
    }

    /// Returns `true` if the value is strictly negative (`< 0`).
    #[must_use]
    pub const fn is_negative(self) -> bool {
        self.0.is_sign_negative() && !self.0.is_zero()
    }
}

/// Hand-written check for `^-?(0|[1-9][0-9]*)(\.[0-9]+)?$`, excluding `-0`.
fn matches_decimal_grammar(s: &str) -> bool {
    let (negative, magnitude) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s),
    };
    if magnitude.is_empty() {
        return false;
    }

    let (int_part, frac_part) = match magnitude.split_once('.') {
        Some((int_p, frac_p)) => {
            if frac_p.is_empty() || !frac_p.bytes().all(|b| b.is_ascii_digit()) {
                return false;
            }
            (int_p, Some(frac_p))
        }
        None => (magnitude, None),
    };

    if int_part.is_empty() || !int_part.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if int_part.len() > 1 && int_part.starts_with('0') {
        return false;
    }

    if negative {
        let int_all_zero = int_part.bytes().all(|b| b == b'0');
        let frac_all_zero = frac_part.is_none_or(|f| f.bytes().all(|b| b == b'0'));
        if int_all_zero && frac_all_zero {
            return false;
        }
    }

    true
}

impl fmt::Display for Dec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl FromStr for Dec {
    type Err = CalcError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl From<i64> for Dec {
    fn from(value: i64) -> Self {
        Self(Decimal::from(value))
    }
}

impl From<i32> for Dec {
    fn from(value: i32) -> Self {
        Self(Decimal::from(value))
    }
}

impl From<u64> for Dec {
    fn from(value: u64) -> Self {
        Self(Decimal::from(value))
    }
}

impl From<u32> for Dec {
    fn from(value: u32) -> Self {
        Self(Decimal::from(value))
    }
}

impl Serialize for Dec {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

struct DecVisitor;

impl Visitor<'_> for DecVisitor {
    type Value = Dec;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a decimal string matching ^-?(0|[1-9][0-9]*)(\\.[0-9]+)?$")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        Dec::parse(value).map_err(de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for Dec {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(DecVisitor)
    }
}
