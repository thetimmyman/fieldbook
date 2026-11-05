//! Company policy file parser (`Policy::parse`), schema-1 structures, built-in floor policy
//! (`FloorKind`, `Policy::floor`, `Policy::floor_sentence`), runtime state (`PolicyState`),
//! and owner narrowing merge (`OwnerSettings`, `KEY_TABLE`, `EffectivePolicy`)
//! (`core-policy`, `ADR 0004 §3`, `docs/reference/policy.md`).

pub mod floor;
pub mod merge;
pub mod parse;
pub mod schema;
pub mod state;

pub use floor::{FLOOR_HASH_TEXT, FloorKind, policy_hash};
pub use merge::{
    Cut, CutBy, EffectivePolicy, KEY_TABLE, KeyRow, MergeKind, OwnerSettings, merge_list,
    merge_number, merge_switch,
};
pub use parse::{
    LOADER_DEST_KINDS, MAX_POLICY_BYTES, Policy, PolicyAudit, PolicyDestination, PolicyError,
    PolicyLabel, PolicySource, PolicySourceMap, PolicySourceRule, canonical_path_components,
    path_matches_prefix,
};
pub use state::{ClosedWhy, PolicyState};
