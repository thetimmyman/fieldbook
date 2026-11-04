//! Three-letter currency code and decimal `Money`.

use std::fmt;
use std::str::FromStr;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::decimal::Dec;
use crate::error::CalcError;

/// Three-letter ASCII uppercase currency code (ISO 4217 shape).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Currency([u8; 3]);

impl Currency {
    /// United States dollar (`USD`).
    pub const USD: Self = Self(*b"USD");

    /// Parses a currency code of exactly three ASCII capital letters (`A`..=`Z`).
    ///
    /// Any other input returns [`CalcError::Grammar`].
    pub fn parse(s: &str) -> Result<Self, CalcError> {
        let [a, b, c] = s.as_bytes() else {
            return Err(CalcError::Grammar);
        };
        if a.is_ascii_uppercase() && b.is_ascii_uppercase() && c.is_ascii_uppercase() {
            Ok(Self([*a, *b, *c]))
        } else {
            Err(CalcError::Grammar)
        }
    }

    /// Views this currency code as a 3-character string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or("")
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Currency {
    type Err = CalcError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Serialize for Currency {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

struct CurrencyVisitor;

impl Visitor<'_> for CurrencyVisitor {
    type Value = Currency;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a three-letter uppercase ASCII currency code")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        Currency::parse(value).map_err(de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for Currency {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(CurrencyVisitor)
    }
}

/// Exact decimal monetary amount tagged with its [`Currency`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Money {
    /// Numeric amount.
    pub amount: Dec,
    /// Currency code.
    pub currency: Currency,
}

impl Money {
    /// Creates a new `Money` value from an amount and currency.
    #[must_use]
    pub const fn new(amount: Dec, currency: Currency) -> Self {
        Self { amount, currency }
    }

    /// Creates a `USD` money value.
    #[must_use]
    pub const fn usd(amount: Dec) -> Self {
        Self {
            amount,
            currency: Currency::USD,
        }
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.amount, self.currency)
    }
}
