//! Pure egress decision gate, destination catalog, delivery binding, and filtered view
//! (`egress::check`, `egress::covers`, `Allowed`, `LabelledView`, `docs/reference/policy.md`).

pub mod decision;
pub mod delivery;
pub mod dest;
pub mod view;

pub use self::decision::{Allowed, Decision, Denial, Reason, Shipped, check, covers, shipped};
pub use self::delivery::{Delivery, Operation, Target, Transfer, hash_tree_manifest};
pub use self::dest::{DEST_KINDS, Dest, DestSpec, RowBind, RowSource};
pub use self::view::LabelledView;
