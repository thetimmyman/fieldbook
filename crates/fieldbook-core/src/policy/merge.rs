//! Merging a [`PolicyState`] with the owner's `settings.toml` narrowing (`OwnerSettings`,
//! `KEY_TABLE`, `EffectivePolicy`, `Cut`, `docs/reference/policy.md#narrow-only-the-owners-settings`).

use std::collections::BTreeMap;

use crate::audit::DestKind;
use crate::egress::Dest;
use crate::labels::{
    LabelAllow, LabelId, LabelTable, MarkingRules, OWNER_SUPPLIED, PUBLIC, StoreId, StoreRule,
};

use super::floor::{FloorKind, policy_hash};
use super::parse::{Policy, PolicyAudit, PolicyDestination, path_matches_prefix};
use super::state::PolicyState;

/// Merge rule classification for an owner `settings.toml` key that meets the company policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MergeKind {
    /// List intersection (`List: intersect`).
    List,
    /// Boolean conjunction (`Switch: AND`).
    Switch,
    /// Minimum of two numbers (`Number: minimum`).
    Number,
    /// Word list governed by the policy (`A word list: the policy decides which entries are usable`).
    Word,
}

impl MergeKind {
    /// Returns the prose description in the "Narrow only" table of `docs/reference/policy.md`.
    #[must_use]
    pub const fn doc_rule(self) -> &'static str {
        match self {
            Self::List => "List: intersect",
            Self::Switch => "Switch: AND",
            Self::Number => "Number: minimum",
            Self::Word => "A word list: the policy decides which entries are usable",
        }
    }
}

/// One row of [`KEY_TABLE`], mapping an owner `settings.toml` key to its [`MergeKind`] and policy side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyRow {
    /// Settings key path as documented in `docs/reference/policy.md`.
    pub key: &'static str,
    /// Merge rule kind.
    pub kind: MergeKind,
    /// Policy-side rule description as documented in `docs/reference/policy.md`.
    pub policy_side: &'static str,
}

/// The table of `settings.toml` keys that meet the company policy (`docs/reference/policy.md#narrow-only-the-owners-settings`).
pub const KEY_TABLE: &[KeyRow] = &[
    KeyRow {
        key: "[model.endpoint.<name>] allow",
        kind: MergeKind::List,
        policy_side: "The `model` row's `allow`",
    },
    KeyRow {
        key: "[model.handoff] allow",
        kind: MergeKind::List,
        policy_side: "The `handoff` row's `allow`",
    },
    KeyRow {
        key: "[mcp] allow",
        kind: MergeKind::List,
        policy_side: "The `mcp` row's `allow`",
    },
    KeyRow {
        key: "[model] keep_calls",
        kind: MergeKind::Switch,
        policy_side: "`keep_calls_days_max` above 0",
    },
    KeyRow {
        key: "[model] keep_days",
        kind: MergeKind::Number,
        policy_side: "`keep_calls_days_max`",
    },
    KeyRow {
        key: "[model] order",
        kind: MergeKind::Word,
        policy_side: "Each entry's effective allow set",
    },
];

/// Which side (`Policy` or `Owner`) cut a label from a destination's effective `allow` set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CutBy {
    /// Cut by the company policy row.
    Policy,
    /// Cut by the owner's `settings.toml` narrowing list.
    Owner,
}

/// One label cut from a destination's effective `allow` set, with the side that cut it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cut {
    /// The label id that was excluded.
    pub label: LabelId,
    /// Whether the policy or the owner cut it.
    pub by: CutBy,
}

/// The owner's narrowing settings (`settings.toml`), merged with a [`PolicyState`] by [`EffectivePolicy::new`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OwnerSettings {
    /// Per-endpoint `allow` lists (`[model.endpoint.<name>] allow`); an endpoint absent from this map
    /// accepts nothing under a loaded policy.
    pub endpoints: BTreeMap<String, LabelAllow>,
    /// Optional narrowing list for `handoff` (`[model.handoff] allow`); `None` means no owner narrowing.
    pub handoff: Option<LabelAllow>,
    /// Optional narrowing list for `mcp` (`[mcp] allow`); `None` means no owner narrowing.
    pub mcp: Option<LabelAllow>,
    /// Optional switch for call-record retention (`[model] keep_calls`).
    pub keep_calls: Option<bool>,
    /// Optional retention limit in days (`[model] keep_days`).
    pub keep_days: Option<u32>,
    /// Optional ordered list of model endpoint names (`[model] order`).
    pub order: Vec<String>,
}

impl OwnerSettings {
    /// Returns the settings of a run with no owner narrowing configured.
    #[must_use]
    pub fn unset() -> Self {
        Self::default()
    }
}

/// Merge helper for a list key: returns the intersection of `policy` and `owner`.
#[must_use]
pub fn merge_list(policy: &LabelAllow, owner: &LabelAllow) -> LabelAllow {
    policy.intersection(owner)
}

/// Merge helper for a switch key: returns the boolean AND of `policy` and `owner`.
#[must_use]
pub const fn merge_switch(policy: bool, owner: bool) -> bool {
    policy && owner
}

/// Merge helper for a number key: returns the minimum of `policy` and `owner`.
#[must_use]
pub fn merge_number(policy: u32, owner: u32) -> u32 {
    policy.min(owner)
}

/// The effective policy produced by merging a [`PolicyState`] with [`OwnerSettings`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectivePolicy {
    state: PolicyState,
    owner: OwnerSettings,
    hash: String,
    policy: Option<Policy>,
}

impl EffectivePolicy {
    /// Merges `state` with `owner` to form an [`EffectivePolicy`].
    #[must_use]
    pub fn new(state: &PolicyState, owner: &OwnerSettings) -> Self {
        let hash = policy_hash(state);
        let policy = state.policy();
        Self {
            state: state.clone(),
            owner: owner.clone(),
            hash,
            policy,
        }
    }

    /// Returns the built-in floor [`EffectivePolicy`] for the machine (`has_company_store`) with
    /// [`OwnerSettings::unset`].
    #[must_use]
    pub fn floor(has_company_store: bool) -> Self {
        Self::new(
            &PolicyState::Floor(FloorKind::for_machine(has_company_store)),
            &OwnerSettings::unset(),
        )
    }

    /// Returns the underlying [`PolicyState`].
    #[must_use]
    pub const fn state(&self) -> &PolicyState {
        &self.state
    }

    /// Returns the [`OwnerSettings`] used to build this effective policy.
    #[must_use]
    pub const fn owner(&self) -> &OwnerSettings {
        &self.owner
    }

    /// Returns the 64-character lowercase hex SHA-256 policy hash.
    #[must_use]
    pub fn hash(&self) -> &str {
        &self.hash
    }

    /// Returns the [`LabelTable`] of defined labels (built-ins for `Floor` and `Closed`).
    #[must_use]
    pub fn label_table(&self) -> LabelTable {
        match &self.policy {
            Some(p) => p.label_table().clone(),
            None => LabelTable::builtin(),
        }
    }

    /// Returns the [`MarkingRules`] of a loaded policy, or [`MarkingRules::builtin`] for `Floor` and `Closed`.
    #[must_use]
    pub fn marking_rules(&self) -> MarkingRules {
        match &self.state {
            PolicyState::Loaded(p) => p.marking_rules(),
            PolicyState::Floor(_) | PolicyState::Closed(_) => MarkingRules::builtin(),
        }
    }

    /// Returns the banner of `id` from its `[[label]]` row, if one is defined.
    #[must_use]
    pub fn banner(&self, id: &LabelId) -> Option<&str> {
        self.policy
            .as_ref()
            .and_then(|p| p.label(id))
            .and_then(|l| l.banner.as_deref())
    }

    /// Returns the `[audit]` configuration of the policy (`PolicyAudit::default()` for `Floor` and `Closed`).
    #[must_use]
    pub fn audit(&self) -> PolicyAudit {
        match &self.policy {
            Some(p) => p.audit().clone(),
            None => PolicyAudit::default(),
        }
    }

    /// Returns the [`StoreRule`] for `store`.
    #[must_use]
    pub fn store_rule(&self, store: &StoreId) -> StoreRule {
        match &self.policy {
            Some(p) => p.store_rule(store),
            None => StoreRule::floor(*store, false),
        }
    }

    /// Returns the effective `keep_calls` switch for a model endpoint `name`.
    #[must_use]
    pub fn effective_keep_calls(&self, name: &str) -> bool {
        let Some(p) = &self.policy else {
            return false;
        };
        let Some(row) = p
            .destinations()
            .iter()
            .find(|d| d.kind == DestKind::Model && d.name.as_deref() == Some(name))
        else {
            return false;
        };
        let policy_allows = row.keep_calls_days_max.unwrap_or(0) > 0;
        let owner_allows = self.owner.keep_calls.unwrap_or(false);
        merge_switch(policy_allows, owner_allows)
    }

    /// Returns the effective `keep_days` retention limit for a model endpoint `name`.
    #[must_use]
    pub fn effective_keep_days(&self, name: &str) -> u32 {
        let Some(p) = &self.policy else {
            return 0;
        };
        let Some(row) = p
            .destinations()
            .iter()
            .find(|d| d.kind == DestKind::Model && d.name.as_deref() == Some(name))
        else {
            return 0;
        };
        let policy_max = row.keep_calls_days_max.unwrap_or(0);
        match self.owner.keep_days {
            Some(owner_days) => merge_number(policy_max, owner_days),
            None => policy_max,
        }
    }

    /// Returns the narrowed [`LabelAllow`] for `dest` (empty when there is no matching row or when
    /// the policy state is `Closed`, except `Dest::Audit` which has no content labels).
    #[must_use]
    pub fn allow_set(&self, dest: &Dest) -> LabelAllow {
        if self.state.is_closed() {
            return LabelAllow::empty();
        }
        let Some(policy) = &self.policy else {
            return LabelAllow::empty();
        };

        match dest {
            Dest::Git | Dest::Edition | Dest::Audit => LabelAllow::from_ids([PUBLIC]),
            Dest::Model { name, .. } => {
                let row = match &self.state {
                    PolicyState::Floor(_) => policy
                        .destinations()
                        .iter()
                        .find(|d| d.kind == DestKind::Model),
                    PolicyState::Loaded(_) => policy.destinations().iter().find(|d| {
                        d.kind == DestKind::Model && d.name.as_deref() == Some(name.as_str())
                    }),
                    PolicyState::Closed(_) => None,
                };
                let Some(row) = row else {
                    return LabelAllow::empty();
                };
                if let Some(owner_allow) = self.owner.endpoints.get(name) {
                    merge_list(&row.allow, owner_allow)
                } else if matches!(self.state, PolicyState::Floor(_))
                    && self.owner.endpoints.is_empty()
                {
                    row.allow.clone()
                } else {
                    LabelAllow::empty()
                }
            }
            Dest::Handoff => {
                let Some(row) = policy
                    .destinations()
                    .iter()
                    .find(|d| d.kind == DestKind::Handoff)
                else {
                    return LabelAllow::empty();
                };
                match &self.owner.handoff {
                    Some(owner_allow) => merge_list(&row.allow, owner_allow),
                    None => row.allow.clone(),
                }
            }
            Dest::Mcp { .. } => {
                let Some(row) = policy
                    .destinations()
                    .iter()
                    .find(|d| d.kind == DestKind::Mcp)
                else {
                    return LabelAllow::empty();
                };
                match &self.owner.mcp {
                    Some(owner_allow) => merge_list(&row.allow, owner_allow),
                    None => row.allow.clone(),
                }
            }
            Dest::Bridge => policy
                .destinations()
                .iter()
                .find(|d| d.kind == DestKind::Bridge)
                .map_or_else(LabelAllow::empty, |r| r.allow.clone()),
            Dest::Print => policy
                .destinations()
                .iter()
                .find(|d| d.kind == DestKind::Print)
                .map_or_else(LabelAllow::empty, |r| r.allow.clone()),
            Dest::Clipboard => policy
                .destinations()
                .iter()
                .find(|d| d.kind == DestKind::Clipboard)
                .map_or_else(LabelAllow::empty, |r| r.allow.clone()),
            Dest::Export { path } => {
                if matches!(self.state, PolicyState::Floor(_)) {
                    return policy
                        .destinations()
                        .iter()
                        .find(|d| d.kind == DestKind::Export)
                        .map_or_else(LabelAllow::empty, |r| r.allow.clone());
                }
                let mut unioned = LabelAllow::empty();
                for row in policy
                    .destinations()
                    .iter()
                    .filter(|d| d.kind == DestKind::Export)
                {
                    if let Some(prefixes) = &row.paths
                        && prefixes.iter().any(|pfx| path_matches_prefix(path, pfx))
                    {
                        unioned = unioned.union(&row.allow);
                    }
                }
                unioned
            }
            Dest::Backup {
                path,
                company_storage,
            } => {
                if matches!(self.state, PolicyState::Floor(_)) {
                    let Some(row) = policy
                        .destinations()
                        .iter()
                        .find(|d| d.kind == DestKind::Backup)
                    else {
                        return LabelAllow::empty();
                    };
                    if row.company_storage_only && !company_storage {
                        return LabelAllow::empty();
                    }
                    return row.allow.clone();
                }
                let mut unioned = LabelAllow::empty();
                for row in policy
                    .destinations()
                    .iter()
                    .filter(|d| d.kind == DestKind::Backup)
                {
                    if let Some(prefixes) = &row.paths
                        && prefixes.iter().any(|pfx| path_matches_prefix(path, pfx))
                    {
                        unioned = unioned.union(&row.allow);
                    }
                }
                unioned
            }
        }
    }

    /// Explains, for each candidate label (`public`, `owner-supplied`, and every defined company
    /// label) that is absent from `allow_set(dest)`, whether [`CutBy::Policy`] or [`CutBy::Owner`]
    /// cut it.
    #[must_use]
    pub fn explain(&self, dest: &Dest) -> Vec<Cut> {
        let Some(policy) = &self.policy else {
            return Vec::new();
        };
        let policy_allow = self.policy_allow_before_owner(policy, dest);
        let effective_allow = self.allow_set(dest);

        let mut candidates = vec![PUBLIC, OWNER_SUPPLIED];
        for l in policy.labels() {
            candidates.push(l.id.clone());
        }
        candidates.sort();
        candidates.dedup();

        let mut cuts = Vec::new();
        for id in candidates {
            if effective_allow.contains(&id) {
                continue;
            }
            let by = if policy_allow.contains(&id) {
                CutBy::Owner
            } else {
                CutBy::Policy
            };
            cuts.push(Cut { label: id, by });
        }
        cuts
    }

    pub(crate) fn matching_rows(&self, dest: &Dest) -> Vec<&PolicyDestination> {
        let Some(policy) = &self.policy else {
            return Vec::new();
        };
        match dest {
            Dest::Model { name, .. } => match &self.state {
                PolicyState::Floor(_) => policy
                    .destinations()
                    .iter()
                    .filter(|d| d.kind == DestKind::Model)
                    .collect(),
                PolicyState::Loaded(_) => policy
                    .destinations()
                    .iter()
                    .filter(|d| {
                        d.kind == DestKind::Model && d.name.as_deref() == Some(name.as_str())
                    })
                    .collect(),
                PolicyState::Closed(_) => Vec::new(),
            },
            Dest::Handoff => policy
                .destinations()
                .iter()
                .filter(|d| d.kind == DestKind::Handoff)
                .collect(),
            Dest::Mcp { .. } => policy
                .destinations()
                .iter()
                .filter(|d| d.kind == DestKind::Mcp)
                .collect(),
            Dest::Bridge => policy
                .destinations()
                .iter()
                .filter(|d| d.kind == DestKind::Bridge)
                .collect(),
            Dest::Print => policy
                .destinations()
                .iter()
                .filter(|d| d.kind == DestKind::Print)
                .collect(),
            Dest::Clipboard => policy
                .destinations()
                .iter()
                .filter(|d| d.kind == DestKind::Clipboard)
                .collect(),
            Dest::Export { path } => match &self.state {
                PolicyState::Floor(_) => policy
                    .destinations()
                    .iter()
                    .filter(|d| d.kind == DestKind::Export && !d.allow.is_empty())
                    .collect(),
                PolicyState::Loaded(_) => policy
                    .destinations()
                    .iter()
                    .filter(|d| {
                        d.kind == DestKind::Export
                            && d.paths.as_ref().is_some_and(|ps| {
                                ps.iter().any(|pfx| path_matches_prefix(path, pfx))
                            })
                    })
                    .collect(),
                PolicyState::Closed(_) => Vec::new(),
            },
            Dest::Backup { path, .. } => match &self.state {
                PolicyState::Floor(_) => policy
                    .destinations()
                    .iter()
                    .filter(|d| d.kind == DestKind::Backup && !d.allow.is_empty())
                    .collect(),
                PolicyState::Loaded(_) => policy
                    .destinations()
                    .iter()
                    .filter(|d| {
                        d.kind == DestKind::Backup
                            && d.paths.as_ref().is_some_and(|ps| {
                                ps.iter().any(|pfx| path_matches_prefix(path, pfx))
                            })
                    })
                    .collect(),
                PolicyState::Closed(_) => Vec::new(),
            },
            Dest::Audit | Dest::Git | Dest::Edition => Vec::new(),
        }
    }

    fn policy_allow_before_owner(&self, policy: &Policy, dest: &Dest) -> LabelAllow {
        match dest {
            Dest::Git | Dest::Edition | Dest::Audit => LabelAllow::from_ids([PUBLIC]),
            Dest::Model { name, .. } => {
                let row = match &self.state {
                    PolicyState::Floor(_) => policy
                        .destinations()
                        .iter()
                        .find(|d| d.kind == DestKind::Model),
                    PolicyState::Loaded(_) => policy.destinations().iter().find(|d| {
                        d.kind == DestKind::Model && d.name.as_deref() == Some(name.as_str())
                    }),
                    PolicyState::Closed(_) => None,
                };
                row.map_or_else(LabelAllow::empty, |r| r.allow.clone())
            }
            Dest::Handoff => policy
                .destinations()
                .iter()
                .find(|d| d.kind == DestKind::Handoff)
                .map_or_else(LabelAllow::empty, |r| r.allow.clone()),
            Dest::Mcp { .. } => policy
                .destinations()
                .iter()
                .find(|d| d.kind == DestKind::Mcp)
                .map_or_else(LabelAllow::empty, |r| r.allow.clone()),
            _ => self.allow_set(dest),
        }
    }
}
