//! Shared contract test suite for [`Authz`] implementations (`ADR 0004 §11`).

#![allow(
    dead_code,
    clippy::expect_used,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    reason = "shared test support modules are selectively used across test binaries"
)]

use std::collections::BTreeMap;

use fieldbook_core::audit::{Actor, Principal, Role, Via};
use fieldbook_core::authz::{Act, Authz, AuthzDecision, Groups, Scope, readable};
use fieldbook_core::labels::StoreId;

/// Scripted groups double that answers `Some(true)`, `Some(false)`, or `None` per group name.
#[derive(Debug, Default, Clone)]
pub struct ScriptedGroups {
    answers: BTreeMap<String, Option<bool>>,
}

impl ScriptedGroups {
    /// Construct a new empty [`ScriptedGroups`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            answers: BTreeMap::new(),
        }
    }

    /// Set an answer for `group`.
    #[must_use]
    pub fn with_answer(mut self, group: &str, answer: Option<bool>) -> Self {
        self.answers.insert(group.to_owned(), answer);
        self
    }
}

impl Groups for ScriptedGroups {
    fn in_group(&self, group: &str) -> Option<bool> {
        self.answers.get(group).copied().unwrap_or(None)
    }
}

/// An [`Authz`] adapter wrapping an inner adapter that denies every [`Act::Read`].
pub struct DenyReadAuthz<'a> {
    inner: Box<dyn Authz + 'a>,
}

impl<'a> DenyReadAuthz<'a> {
    /// Construct a new [`DenyReadAuthz`] wrapping `inner`.
    #[must_use]
    pub fn new(inner: Box<dyn Authz + 'a>) -> Self {
        Self { inner }
    }
}

impl Authz for DenyReadAuthz<'_> {
    fn allow(&self, who: &Actor, act: Act, on: &Scope) -> AuthzDecision {
        if act == Act::Read {
            AuthzDecision::Deny(fieldbook_core::audit::Word::new_static("denied"))
        } else {
            self.inner.allow(who, act, on)
        }
    }
}

fn sample_actor() -> Actor {
    Actor {
        principal: Principal::local(),
        role: Role::Owner,
        via: Via::Cli,
    }
}

/// Type alias for the adapter factory used by [`authz_suite`].
pub type AuthzFactory =
    dyn for<'a> Fn(&[(StoreId, Option<&str>, Option<&str>)], &'a dyn Groups) -> Box<dyn Authz + 'a>;

/// Runs the shared authorization contract suite over the adapter constructed by `make`.
///
/// Verifies:
/// - `allow` is pure and repeatable (the same call twice gives the same answer, for every [`Act`]);
/// - [`Act::Read`] on a store whose row names no readers is allowed;
/// - on a store that names readers, [`Act::Read`] is allowed for `Some(true)` and denied for `Some(false)` and `None`;
/// - [`Act::Admin`] follows `changers` in the same way;
/// - [`Act::Propose`], [`Act::Accept`], and [`Act::Export`] are allowed;
/// - every [`AuthzDecision::Deny`] carries a non-empty reason word;
/// - `readable(&authz, &actor, &stores)` lists exactly the stores for which [`Act::Read`] is allowed in the order given;
/// - a denying adapter narrows `readable` and never widens it.
pub fn authz_suite(make: &AuthzFactory) {
    let actor = sample_actor();

    let all_acts = [
        Act::Read,
        Act::Propose,
        Act::Accept,
        Act::Export,
        Act::Admin,
    ];

    // 1. Pure and repeatable: verify with multiple store configurations
    let store_specs: &[(StoreId, Option<&str>, Option<&str>)] = &[
        (StoreId::Corpus, None, None),
        (StoreId::Working, None, None),
        (
            StoreId::Overlay,
            Some("overlay-readers"),
            Some("overlay-changers"),
        ),
        (
            StoreId::State,
            Some("state-readers"),
            Some("state-changers"),
        ),
    ];

    let groups_all_true = ScriptedGroups::new()
        .with_answer("overlay-readers", Some(true))
        .with_answer("overlay-changers", Some(true))
        .with_answer("state-readers", Some(true))
        .with_answer("state-changers", Some(true));

    let authz_true = make(store_specs, &groups_all_true);

    for &(store, _, _) in store_specs {
        let scope = Scope::for_store(store);
        for &act in &all_acts {
            let first = authz_true.allow(&actor, act, &scope);
            let second = authz_true.allow(&actor, act, &scope);
            assert_eq!(
                first, second,
                "allow must be pure and repeatable for store {store:?}, act {act:?}"
            );
        }
    }

    // 2. Read on store without readers is allowed
    for &store in &[StoreId::Corpus, StoreId::Working] {
        let scope = Scope::for_store(store);
        let groups_all_false = ScriptedGroups::new()
            .with_answer("overlay-readers", Some(false))
            .with_answer("overlay-changers", Some(false))
            .with_answer("state-readers", Some(false))
            .with_answer("state-changers", Some(false));
        let authz_false = make(store_specs, &groups_all_false);
        assert_eq!(
            authz_false.allow(&actor, Act::Read, &scope),
            AuthzDecision::Allow,
            "read on store without readers ({store:?}) must be allowed even when groups answer false"
        );
    }

    // 3. Read on store with readers: Some(true) -> allow, Some(false) -> deny, None -> deny
    for (ans, expected_allow) in [(Some(true), true), (Some(false), false), (None, false)] {
        let groups = ScriptedGroups::new().with_answer("overlay-readers", ans);
        let authz = make(store_specs, &groups);
        let scope = Scope::for_store(StoreId::Overlay);
        let decision = authz.allow(&actor, Act::Read, &scope);
        if expected_allow {
            assert_eq!(
                decision,
                AuthzDecision::Allow,
                "overlay read must be allowed when readers group is Some(true)"
            );
        } else {
            match decision {
                AuthzDecision::Deny(word) => {
                    assert!(
                        !word.as_str().is_empty(),
                        "deny reason must be non-empty for read with answer {ans:?}"
                    );
                }
                AuthzDecision::Allow => {
                    panic!("overlay read must be denied when readers group is {ans:?}");
                }
            }
        }
    }

    // 4. Admin on store with changers: Some(true) -> allow, Some(false) -> deny, None -> deny
    // And on store without changers -> allow
    for &store in &[StoreId::Corpus, StoreId::Working] {
        let scope = Scope::for_store(store);
        let groups = ScriptedGroups::new();
        let authz = make(store_specs, &groups);
        assert_eq!(
            authz.allow(&actor, Act::Admin, &scope),
            AuthzDecision::Allow,
            "admin on store without changers ({store:?}) must be allowed"
        );
    }

    for (ans, expected_allow) in [(Some(true), true), (Some(false), false), (None, false)] {
        let groups = ScriptedGroups::new().with_answer("state-changers", ans);
        let authz = make(store_specs, &groups);
        let scope = Scope::for_store(StoreId::State);
        let decision = authz.allow(&actor, Act::Admin, &scope);
        if expected_allow {
            assert_eq!(
                decision,
                AuthzDecision::Allow,
                "state admin must be allowed when changers group is Some(true)"
            );
        } else {
            match decision {
                AuthzDecision::Deny(word) => {
                    assert!(
                        !word.as_str().is_empty(),
                        "deny reason must be non-empty for admin with answer {ans:?}"
                    );
                }
                AuthzDecision::Allow => {
                    panic!("state admin must be denied when changers group is {ans:?}");
                }
            }
        }
    }

    // 5. Propose, Accept, Export are allowed for every store and any group state
    let groups_none = ScriptedGroups::new();
    let authz_none = make(store_specs, &groups_none);
    for &(store, _, _) in store_specs {
        let scope = Scope::for_store(store);
        for &act in &[Act::Propose, Act::Accept, Act::Export] {
            assert_eq!(
                authz_none.allow(&actor, act, &scope),
                AuthzDecision::Allow,
                "{act:?} on {store:?} must be allowed"
            );
        }
    }

    // 6. readable(&authz, &actor, &stores) lists exactly stores where Read is allowed, in order
    // Case A: overlay allowed, state denied
    let groups_mixed = ScriptedGroups::new()
        .with_answer("overlay-readers", Some(true))
        .with_answer("state-readers", Some(false));
    let authz_mixed = make(store_specs, &groups_mixed);
    let check_order = [
        StoreId::State,
        StoreId::Working,
        StoreId::Overlay,
        StoreId::Corpus,
    ];
    let readable_result = readable(authz_mixed.as_ref(), &actor, &check_order);
    assert_eq!(
        readable_result.stores(),
        &[StoreId::Working, StoreId::Overlay, StoreId::Corpus],
        "readable must preserve input order and filter out denied stores"
    );

    // 7. Denying adapter narrows readable and never widens it
    let deny_authz = DenyReadAuthz::new(authz_mixed);
    let narrowed = readable(&deny_authz, &actor, &check_order);
    assert!(
        narrowed.is_empty(),
        "denying adapter must yield empty readable when denying all reads"
    );
    for s in narrowed.stores() {
        assert!(
            readable_result.contains(*s),
            "denying adapter must never widen readable"
        );
    }
}
