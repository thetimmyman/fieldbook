//! Named rounding stages and `RoundingPolicy` (AD-14, D-8).
//!
//! Bare `Decimal::round` and `Decimal::round_dp` are banned across the workspace by
//! `clippy.toml`. Every rounding decision goes through [`round_at_stage`] (or the
//! stage quotient helper shared with [`crate::ops::div_at_stage`]).

use std::fmt;
use std::str::FromStr;

use rust_decimal::{Decimal, RoundingStrategy};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::decimal::Dec;
use crate::error::CalcError;
use crate::money::Money;
use crate::ops::{MAX_MANTISSA, U384};
use crate::value::Value;

/// Closed enum of the three named rounding stages of AD-14.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stage {
    /// `unit_price`
    UnitPrice,
    /// `extension`
    Extension,
    /// `total`
    Total,
}

impl Stage {
    /// All three rounding stages in canonical order.
    pub const ALL: &'static [Self] = &[Self::UnitPrice, Self::Extension, Self::Total];

    /// Returns the snake_case vocabulary word for this stage (`unit_price`, `extension`, `total`).
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::UnitPrice => "unit_price",
            Self::Extension => "extension",
            Self::Total => "total",
        }
    }

    /// Alias for [`Stage::word`].
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.word()
    }

    /// Parses a stage from its snake_case word.
    ///
    /// An unknown word returns [`CalcError::Grammar`].
    pub fn from_word(s: &str) -> Result<Self, CalcError> {
        match s {
            "unit_price" => Ok(Self::UnitPrice),
            "extension" => Ok(Self::Extension),
            "total" => Ok(Self::Total),
            _ => Err(CalcError::Grammar),
        }
    }

    /// Alias for [`Stage::from_word`].
    pub fn parse(s: &str) -> Result<Self, CalcError> {
        Self::from_word(s)
    }
}

impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

impl FromStr for Stage {
    type Err = CalcError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_word(s)
    }
}

impl Serialize for Stage {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.word())
    }
}

struct StageVisitor;

impl Visitor<'_> for StageVisitor {
    type Value = Stage;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("one of `unit_price`, `extension` or `total`")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        Stage::from_word(value).map_err(de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for Stage {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(StageVisitor)
    }
}

/// Shared rounding policy for `UnitPrice`, `Extension`, and `Total` (AD-14, D-8).
///
/// Holds one validated decimal scale in `0..=28` (default `2`) and uses
/// [`RoundingStrategy::MidpointAwayFromZero`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct RoundingPolicy {
    scale: u32,
}

impl RoundingPolicy {
    /// Maximum supported decimal scale (`28`).
    pub const MAX_SCALE: u32 = 28;
    /// Default decimal scale (`2`).
    pub const DEFAULT_SCALE: u32 = 2;

    /// Constructs a [`RoundingPolicy`] with the given `scale` (`0..=28`).
    ///
    /// Returns [`CalcError::InvalidScale`] (`FB0500`) if `scale > 28`.
    pub const fn new(scale: u32) -> Result<Self, CalcError> {
        if scale > Self::MAX_SCALE {
            return Err(CalcError::InvalidScale);
        }
        Ok(Self { scale })
    }

    /// Returns the shared decimal scale (`0..=28`).
    #[must_use]
    pub const fn scale(&self) -> u32 {
        self.scale
    }

    /// Returns the rounding strategy (`MidpointAwayFromZero`).
    #[must_use]
    pub const fn strategy(&self) -> RoundingStrategy {
        RoundingStrategy::MidpointAwayFromZero
    }

    /// Returns the canonical snake_case name of the rounding mode.
    #[must_use]
    pub const fn mode_word(&self) -> &'static str {
        "midpoint_away_from_zero"
    }
}

impl Default for RoundingPolicy {
    fn default() -> Self {
        Self {
            scale: Self::DEFAULT_SCALE,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRoundingPolicy {
    scale: u32,
}

impl<'de> Deserialize<'de> for RoundingPolicy {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = RawRoundingPolicy::deserialize(deserializer)?;
        Self::new(raw.scale).map_err(de::Error::custom)
    }
}

/// Rounds a [`Value<Dec>`] at the named [`Stage`] according to `policy`.
///
/// Preserves [`Value::Unknown`] unchanged and rounds [`Value::Known`] values using
/// `round_dp_with_strategy` with [`RoundingStrategy::MidpointAwayFromZero`].
pub fn round_at_stage(
    value: Value<Dec>,
    stage: Stage,
    policy: &RoundingPolicy,
) -> Result<Value<Dec>, CalcError> {
    if policy.scale > RoundingPolicy::MAX_SCALE {
        return Err(CalcError::InvalidScale);
    }
    let _ = stage;
    Ok(value.map(|dec| {
        let rounded = dec
            .inner()
            .round_dp_with_strategy(policy.scale, policy.strategy());
        Dec::from_decimal(rounded)
    }))
}

/// Rounds a [`Value<Money>`] at the named [`Stage`] according to `policy`, preserving currency.
pub fn round_money_at_stage(
    value: Value<Money>,
    stage: Stage,
    policy: &RoundingPolicy,
) -> Result<Value<Money>, CalcError> {
    match value {
        Value::Unknown(reason) => {
            if policy.scale > RoundingPolicy::MAX_SCALE {
                return Err(CalcError::InvalidScale);
            }
            Ok(Value::Unknown(reason))
        }
        Value::Known(money) => {
            let rounded = round_at_stage(Value::Known(money.amount), stage, policy)?;
            Ok(rounded.map(|amount| Money::new(amount, money.currency)))
        }
    }
}

/// Rounds an exact unsigned integer quotient and remainder (`quot + rem / den`, with `0 <= rem < den`)
/// once at the named [`Stage`] under `policy` (`MidpointAwayFromZero`).
pub(crate) fn round_quotient_at_stage(
    negative: bool,
    quot: U384,
    rem: U384,
    den: U384,
    stage: Stage,
    policy: &RoundingPolicy,
) -> Result<Dec, CalcError> {
    if policy.scale > RoundingPolicy::MAX_SCALE {
        return Err(CalcError::InvalidScale);
    }
    let _ = stage;
    // MidpointAwayFromZero: increment quotient magnitude when 2 * rem >= den.
    let rounded_quot = if rem.add(rem) >= den {
        quot.add(U384::from_u128(1))
    } else {
        quot
    };
    let q_u128 = rounded_quot
        .to_u128()
        .filter(|&v| v <= MAX_MANTISSA)
        .ok_or(CalcError::Overflow)?;
    let q_i128 = i128::try_from(q_u128).map_err(|_| CalcError::Overflow)?;
    let signed_m = if negative && q_i128 != 0 {
        -q_i128
    } else {
        q_i128
    };
    Ok(Dec::from_decimal(Decimal::from_i128_with_scale(
        signed_m,
        policy.scale,
    )))
}
