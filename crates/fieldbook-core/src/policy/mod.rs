//! Company policy file parser (`Policy::parse`), schema-1 structures, built-in floor policy
//! (`FloorKind`, `Policy::floor`, `Policy::floor_sentence`), runtime state (`PolicyState`),
//! and owner narrowing merge (`OwnerSettings`, `KEY_TABLE`, `EffectivePolicy`)
//! (`core-policy`, `ADR 0004 §3`, `docs/reference/policy.md`).

pub mod floor;
pub mod merge;
pub mod parse;
pub mod place;
pub mod schema;
pub mod seen;
pub mod show;
pub mod state;
pub mod writers;

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
pub use place::{Flags, LaptopFolders, PlaceWhy, Placement, Source, flags_allowed, place};
pub use seen::{
    FileFact, FileObservation, OVER_LIMIT_PREFIX_BYTES, ObservationDigest, Resolved, Seen,
    closed_observation_hash, resolve,
};
pub use show::{PolicyShowText, policy_line, policy_line_from, show, show_at};
pub use state::{ClosedWhy, PolicyState};
pub use writers::{Writer, WriterFacts, WriterWhy, writer_check};
