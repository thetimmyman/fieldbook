//! Handling labels, data label sets (`LabelSet`), permission allow-lists (`LabelAllow`),
//! defined label tables (`LabelTable`), store admission (`admit`), text labelling (`for_text`),
//! folder placement (`git_guard`, `placement`), the append-only label ledger (`ledger`),
//! label change rules (`change`), and provenance dependency evaluation (`effective_item`)
//! (`docs/decisions/0004-company-boundary.md`).

pub mod change;
pub mod dependencies;
pub mod id;
pub mod ledger;
pub mod marking;
pub mod placement;
pub mod set;
pub mod store;
pub mod table;

pub use change::{AddContext, DeclaredRole, Preset, confirmation_names_count, may_add};
pub use dependencies::{ItemRef, LabelGraph, LabelNode, effective_item};
pub use id::{BUILTINS, LabelId, LabelIdError, MARKED, OWNER_SUPPLIED, PUBLIC, UNLABELLED};
pub use ledger::{
    LEDGER_HEADER, Ledger, LedgerEntry, LedgerError, LedgerKind, LedgerOrigin, LedgerPlan,
    effective,
};
pub use marking::{LabelContext, MarkingRow, MarkingRules, PublicAct, for_text, markings_in};
pub use placement::{
    GitContext, PlaceFacts, PlaceKind, PlaceRole, Placement, fieldbook_is_ignored, git_guard,
    placement,
};
pub use set::{LabelAllow, LabelSet, LabelSetError};
pub use store::{Refusal, RefuseReason, StoreId, StoreRule, admit};
pub use table::{LabelFinding, LabelTable, check_known};
