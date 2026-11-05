//! Typed calculation errors and their check-catalogue codes.

use std::fmt;

/// Errors returned by `fieldbook-calc` parsers and operations.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CalcError {
    /// Input does not follow the decimal, currency, unit, date or period grammar (`FB0500`).
    Grammar,
    /// Rounding policy scale is outside `0..=28` (`FB0500`).
    InvalidScale,
    /// Value or result is outside the representable range (`FB0501`).
    Overflow,
    /// Operation mixes incompatible currencies or units (`FB0502`).
    Mismatch,
    /// Calculation case or run names a function not in `CALC_FNS` (`FB0504`).
    UnknownFn(String),
}

impl CalcError {
    /// Returns the `FB05xx` catalogue code for this error.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Grammar | Self::InvalidScale => "FB0500",
            Self::Overflow => "FB0501",
            Self::Mismatch => "FB0502",
            Self::UnknownFn(_) => "FB0504",
        }
    }
}

impl fmt::Display for CalcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Grammar => f.write_str("FB0500: value does not follow the calculation grammar"),
            Self::InvalidScale => f.write_str("FB0500: rounding scale is outside 0..=28"),
            Self::Overflow => f.write_str("FB0501: value is outside the calculation range"),
            Self::Mismatch => f.write_str("FB0502: calculation mixes currencies or units"),
            Self::UnknownFn(id) => {
                write!(f, "FB0504: calculation function `{id}` is not in CALC_FNS")
            }
        }
    }
}

impl std::error::Error for CalcError {}
