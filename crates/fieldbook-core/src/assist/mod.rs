//! Corpus assist functions (`context_pack` and `cite_check`).
//!
//! **Responsibility.** Builds pure, self-contained [`ContextPack`]s from a
//! [`LabelledView`](crate::egress::LabelledView) for a specific page, bounding size by a
//! character budget and enforcing data-boundary rules.
//!
//! **Rules.** Pure: no I/O, no clock, no environment.

mod pack;

pub use pack::{ContextPack, PackBlock, PackSource, RELATED_LIMIT, index_of, page_pack};
