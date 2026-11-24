//! Retained `bm25f@1` index version for the published compatibility runner.
//!
//! **Responsibility.** Re-expose the one `IndexPort` adapter so the compatibility runner replays
//! its archived query vectors. A change to what it returns needs a new `bm25f@2` version.
//!
//! **Rules.** Pure: no I/O.

pub use crate::pipeline::bm25_index::{Bm25Index, ID};
