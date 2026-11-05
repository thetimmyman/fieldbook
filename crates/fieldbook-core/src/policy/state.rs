//! Policy runtime states (`PolicyState::{Floor, Loaded, Closed}`, `ClosedWhy`,
//! `docs/reference/policy.md#states`).

use super::floor::FloorKind;
use super::parse::Policy;

/// Why the policy runtime entered [`PolicyState::Closed`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClosedWhy {
    /// A policy revision was previously recorded in the state folder, and the policy file is now missing.
    Absent,
    /// The policy file is unreadable, fails schema-1 validation (`FB0413`), or is an example file.
    Invalid,
    /// The policy file's `revision` is lower than the highest recorded revision (`FB0413`).
    LowerRevision,
    /// The policy file or its folder is writable by a non-administrator (`FB0418`).
    Writable,
}

/// The three policy states (`Floor`, `Loaded`, `Closed`).
#[expect(
    clippy::large_enum_variant,
    reason = "PolicyState::Loaded(Policy) is the card-specified variant shape"
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyState {
    /// No policy file is present and no revision has been recorded on this machine before.
    Floor(FloorKind),
    /// A valid policy file was loaded and passed the revision and writer checks.
    Loaded(Policy),
    /// Policy loading failed or a previously loaded policy disappeared; every destination except
    /// `Dest::Audit` is denied (`FB0413`).
    Closed(ClosedWhy),
}

impl PolicyState {
    /// Returns `true` when this state is [`PolicyState::Closed`].
    #[must_use]
    pub const fn is_closed(&self) -> bool {
        matches!(self, Self::Closed(_))
    }

    /// Returns the loaded or floor [`Policy`], or `None` when [`PolicyState::Closed`].
    #[must_use]
    pub fn policy(&self) -> Option<Policy> {
        match self {
            Self::Floor(kind) => Some(Policy::floor(*kind)),
            Self::Loaded(p) => Some(p.clone()),
            Self::Closed(_) => None,
        }
    }
}
