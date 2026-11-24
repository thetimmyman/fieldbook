//! Retained `exact-ids+bm25@1` correlator version for the published compatibility runner.
//!
//! **Responsibility.** Re-expose the one `Correlator` adapter so the compatibility runner replays
//! its archived link vectors. A change to what it returns needs a new `exact-ids+bm25@2` version.
//!
//! **Rules.** Pure: no I/O.

pub use crate::pipeline::correlate::{ExactIdsBm25, ID};
