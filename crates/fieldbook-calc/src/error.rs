//! Typed calculation errors and their check-catalogue codes.

use std::fmt;

/// Errors returned by `fieldbook-calc` parsers and operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CalcError {
    /// Input does not follow the decimal, currency, unit, date or period grammar (`FB0500`).
    Grammar,
    /// Value or result is outside the representable range (`FB0501`).
    Overflow,
    /// Operation mixes incompatible currencies or units (`FB0502`).
    Mismatch,
}

impl CalcError {
    /// Returns the `FB05xx` catalogue code for this error.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Grammar => "FB0500",
            Self::Overflow => "FB0501",
            Self::Mismatch => "FB0502",
        }
    }
}

impl fmt::Display for CalcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Grammar => f.write_str("FB0500: value does not follow the calculation grammar"),
            Self::Overflow => f.write_str("FB0501: value is outside the calculation range"),
            Self::Mismatch => f.write_str("FB0502: calculation mixes currencies or units"),
        }
    }
}

impl std::error::Error for CalcError {}
