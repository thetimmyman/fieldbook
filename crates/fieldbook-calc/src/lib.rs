//! The calculation core: the leaf of the crate chain.
//!
//! **Responsibility.** Exact decimal money, values that can be unknown, named
//! rounding stages, the non-currency `Unit` vocabulary, civil and partial dates,
//! effective periods and durations, and the named calculation functions with the
//! record of a run (AD-14). Money and dates must not depend on workspace parsing,
//! so this crate depends on nothing else in the workspace.
//!
//! **Allowed dependencies.** No workspace crate. `rust_decimal` and `serde` are
//! wired from `[workspace.dependencies]` (`day1-dependencies-3`).
//!
//! **Rules.** No I/O and no clock (workspace `clippy.toml`). No floating-point
//! arithmetic and no unchecked indexing (the deny line below). The weekly
//! `lint-controls` job plants a violation in this file and requires clippy to fail,
//! so the line cannot be deleted unseen.

#![deny(clippy::float_arithmetic, clippy::indexing_slicing)]

pub mod date;
pub mod decimal;
pub mod error;
pub mod money;
pub mod ops;
pub mod period;
pub mod rounding;
pub mod unit;
pub mod value;

pub use date::{CivilDate, PartialDate};
pub use decimal::Dec;
pub use error::CalcError;
pub use money::{Currency, Money};
pub use ops::{add, add_dec, div, div_at_stage, div_dec, mul, mul_dec, percent_of, sub, sub_dec};
pub use period::{Duration, EffectivePeriod};
pub use rounding::{RoundingPolicy, Stage, round_at_stage, round_money_at_stage};
pub use unit::{Unit, UnitFamily, convert};
pub use value::{Reason, Value};

/// This crate's name in the chain calc, core, render, fieldbook.
pub const LAYER: &str = "calc";

#[cfg(test)]
mod tests {
    use super::LAYER;

    #[test]
    fn layer_matches_the_package_name() {
        assert_eq!(format!("fieldbook-{LAYER}"), env!("CARGO_PKG_NAME"));
    }
}
