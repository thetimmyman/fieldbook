//! Dedicated per-version calculation and canonical-run implementations.
//!
//! Each file in this module is hashed whole by `compat/published.toml` so that adding a
//! new version or row to `CALC_FNS` leaves existing bindings byte-for-byte unchanged.

pub mod build_up_v1;
pub mod children_sum_v1;
pub mod run_v1;
