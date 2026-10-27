//! The calculation core: the leaf of the crate chain.
//!
//! **Responsibility.** Exact decimal money, values that can be unknown, named
//! rounding stages, units, civil dates and periods, and the named calculation
//! functions with the record of a run. Money must not depend on parsing, so this
//! crate depends on nothing else in the workspace. Its types arrive with the
//! `calc-day1` card (money, values, units and dates); the function table and the
//! run record arrive with the `calc-cases` card.
//!
//! **Allowed dependencies.** No workspace crate. One external crate, `rust_decimal`,
//! joins with the `calc-day1` card.
//!
//! **Rules.** No I/O and no clock (workspace `clippy.toml`). No floating-point
//! arithmetic and no unchecked indexing (the deny line below). The weekly
//! `lint-controls` job plants a violation in this file and requires clippy to fail,
//! so the line cannot be deleted unseen.

#![deny(clippy::float_arithmetic, clippy::indexing_slicing)]

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
