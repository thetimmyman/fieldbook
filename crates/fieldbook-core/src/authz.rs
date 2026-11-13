//! Authorisation port (`Authz`), group membership lookup (`Groups`), default store-reader
//! adapter (`PolicyReaders`, id `policy-readers@1`), and store readability helper
//! (`readable`, `ReadableStores`, `docs/decisions/0004-company-boundary.md#11-actor-session-and-authorisation`).

use crate::audit::{Actor, Word};
use crate::labels::{LabelSet, StoreId, StoreRule};
use crate::policy::{FloorKind, Policy, PolicyState};

/// What an [`Act`] is performed on: a [`StoreId`] and its [`LabelSet`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    /// Target store.
    pub store: StoreId,
    /// Labels carried by the target item or store.
    pub labels: LabelSet,
}

impl Scope {
    /// Constructs a [`Scope`] for `store` and `labels`.
    #[must_use]
    pub const fn new(store: StoreId, labels: LabelSet) -> Self {
        Self { store, labels }
    }

    /// Constructs a store-level [`Scope`] with `public` labels (used before a store is opened).
    #[must_use]
    pub fn for_store(store: StoreId) -> Self {
        Self {
            store,
            labels: LabelSet::public(),
        }
    }
}

/// Closed list of acts checked by [`Authz::allow`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Act {
    /// Opening or reading from a store.
    Read,
    /// Creating a proposal (`add --propose`, `sync`).
    Propose,
    /// Accepting a proposal (`queue accept`).
    Accept,
    /// Exporting or backing up data.
    Export,
    /// Administrative label mutation (`label add`).
    Admin,
}

/// Result of [`Authz::allow`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthzDecision {
    /// The act is permitted.
    Allow,
    /// The act is denied with a closed-list reason word (written as `refuse` with `reason = authz`, `FB0807`).
    Deny(Word),
}

impl AuthzDecision {
    /// Returns `true` when the decision is [`Self::Allow`].
    #[must_use]
    pub const fn is_allow(&self) -> bool {
        matches!(self, Self::Allow)
    }
}

/// Port answering who may see or do what (`ADR 0004 §11`).
pub trait Authz {
    /// Evaluates whether `who` may perform `act` on `on`.
    fn allow(&self, who: &Actor, act: Act, on: &Scope) -> AuthzDecision;
}

/// Group-membership port queried by [`PolicyReaders`].
///
/// `in_group` returns:
/// - `Some(true)` when the logon token is a member of `group`,
/// - `Some(false)` when the logon token was read and is not in `group`,
/// - `None` when the logon token could not be read.
pub trait Groups {
    /// Checks whether the current logon token is in `group`.
    fn in_group(&self, group: &str) -> Option<bool>;
}

/// Default [`Authz`] adapter (`policy-readers@1`).
///
/// - [`Act::Read`] on a store is allowed when the store's [`StoreRule`] names no `readers` or
///   `groups.in_group(readers)` is `Some(true)`, and denied (`Word("authz")`) when the token is
///   not in `readers` or cannot be read (`None`).
/// - [`Act::Admin`] (for `label add`) follows `changers` the same way.
/// - Every other [`Act`] is allowed.
pub struct PolicyReaders<'a> {
    policy: PolicyState,
    groups: &'a dyn Groups,
}

impl<'a> PolicyReaders<'a> {
    /// Canonical adapter identifier (`policy-readers@1`).
    pub const ID: &'static str = "policy-readers@1";

    /// Constructs a [`PolicyReaders`] adapter from a [`Policy`] and a [`Groups`] lookup.
    #[must_use]
    pub fn new(policy: &Policy, groups: &'a dyn Groups) -> Self {
        Self {
            policy: PolicyState::Loaded(policy.clone()),
            groups,
        }
    }

    /// Constructs a [`PolicyReaders`] adapter from a [`PolicyState`] and a [`Groups`] lookup.
    #[must_use]
    pub fn from_state(policy: &PolicyState, groups: &'a dyn Groups) -> Self {
        Self {
            policy: policy.clone(),
            groups,
        }
    }

    /// Returns the adapter identifier (`"policy-readers@1"`).
    #[must_use]
    pub const fn id(&self) -> &'static str {
        Self::ID
    }

    fn store_rule(&self, store: StoreId) -> StoreRule {
        match &self.policy {
            PolicyState::Loaded(p) => p.store_rule(&store),
            PolicyState::Floor(kind) => Policy::floor(*kind).store_rule(&store),
            PolicyState::Closed(_) => Policy::floor(FloorKind::A).store_rule(&store),
        }
    }
}

impl Authz for PolicyReaders<'_> {
    fn allow(&self, _who: &Actor, act: Act, on: &Scope) -> AuthzDecision {
        let rule = self.store_rule(on.store);
        crate::versions::policy_readers_v1::allow(
            act,
            rule.readers.as_deref(),
            rule.changers.as_deref(),
            self.groups,
        )
    }
}

/// Ordered list of stores that an [`Actor`] is authorised to read (`authz::readable`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadableStores {
    stores: Vec<StoreId>,
}

impl ReadableStores {
    /// Constructs a [`ReadableStores`] from an iterator of [`StoreId`]s, preserving order and
    /// removing duplicates.
    #[must_use]
    pub fn new(stores: impl IntoIterator<Item = StoreId>) -> Self {
        let mut out = Vec::new();
        for store in stores {
            if !out.contains(&store) {
                out.push(store);
            }
        }
        Self { stores: out }
    }

    /// Returns a [`ReadableStores`] containing only [`StoreId::Corpus`] (used by `build` and
    /// edition rendering, which read no other store).
    #[must_use]
    pub fn corpus() -> Self {
        Self {
            stores: vec![StoreId::Corpus],
        }
    }

    /// Returns the slice of readable stores in order.
    #[must_use]
    pub fn stores(&self) -> &[StoreId] {
        &self.stores
    }

    /// Returns `true` when `store` is in this readable set.
    #[must_use]
    pub fn contains(&self, store: StoreId) -> bool {
        self.stores.contains(&store)
    }

    /// Returns `true` when no store is readable.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.stores.is_empty()
    }
}

/// Determines which stores from `stores` `who` may read under `authz`.
///
/// Called before any store is opened; there is no item-level `visible` function because
/// [`crate::egress::LabelledView`] takes `ReadableStores` and filters records by label subset.
#[must_use]
pub fn readable(authz: &dyn Authz, who: &Actor, stores: &[StoreId]) -> ReadableStores {
    let mut allowed = Vec::new();
    for &store in stores {
        let scope = Scope::for_store(store);
        if authz.allow(who, Act::Read, &scope).is_allow() && !allowed.contains(&store) {
            allowed.push(store);
        }
    }
    ReadableStores { stores: allowed }
}
