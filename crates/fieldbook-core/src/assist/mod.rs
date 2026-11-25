//! Corpus assist functions (`context_pack`, `apply_edits`, `resolve_pack`, `export_units`,
//! and `cite_check`).
//!
//! **Responsibility.** Builds pure, self-contained [`ContextPack`]s from a
//! [`LabelledView`](crate::egress::LabelledView), enforces Unicode-scalar [`RequestBudget`]s
//! and data-boundary rules, resolves saved [`NamedPack`]s, and exports view-scoped [`UnitRecord`]s.
//!
//! **Rules.** Pure: no I/O, no clock, no environment.

mod context;
mod edits;
mod pack;
mod view;

pub use context::{
    Budget, ContextIds, ContextItem, Include, NamedPack, Origin, RequestBudget, Scope, UsageError,
    Why, scalar_count,
};
pub use edits::{Edits, apply_edits};
pub use pack::{
    ContextPack, PackBlock, PackSource, RELATED_LIMIT, context_pack, context_pack_for, index_of,
    page_pack,
};
pub use view::{UnitRecord, export_units, resolve_pack, view_fingerprint};
