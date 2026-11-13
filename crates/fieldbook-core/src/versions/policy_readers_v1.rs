//! Retained implementation of the `policy-readers@1` authorisation decision (published compatibility binding).
//!
//! **Responsibility.** The frozen authorisation decision of `policy-readers@1`:
//! evaluating whether an [`Act`] is permitted given store reader and changer requirements
//! and caller group membership.
//!
//! **Rules.** Pure: no I/O.

use crate::audit::Word;
use crate::authz::{Act, AuthzDecision, Groups};

/// Canonical adapter identifier (`policy-readers@1`).
pub const ID: &str = "policy-readers@1";

/// Reason word used when access is denied by `policy-readers@1`.
pub const DENY_REASON: &str = "authz";

/// Evaluates whether `act` is permitted under `policy-readers@1`.
///
/// - [`Act::Read`] follows the `readers` group requirement.
/// - [`Act::Admin`] follows the `changers` group requirement.
/// - [`Act::Propose`], [`Act::Accept`], and [`Act::Export`] are unconditionally allowed.
///
/// A group requirement (`Some(group)`) is satisfied only when `groups.in_group(group)`
/// returns `Some(true)`. When `None` or `Some(false)`, access is denied with
/// [`AuthzDecision::Deny`] carrying the reason word `"authz"`. No requirement (`None`)
/// allows access.
#[must_use]
pub fn allow(
    act: Act,
    readers: Option<&str>,
    changers: Option<&str>,
    groups: &dyn Groups,
) -> AuthzDecision {
    match act {
        Act::Read => check_group(readers, groups),
        Act::Admin => check_group(changers, groups),
        Act::Propose | Act::Accept | Act::Export => AuthzDecision::Allow,
    }
}

fn check_group(required: Option<&str>, groups: &dyn Groups) -> AuthzDecision {
    let Some(group) = required else {
        return AuthzDecision::Allow;
    };
    match groups.in_group(group) {
        Some(true) => AuthzDecision::Allow,
        Some(false) | None => AuthzDecision::Deny(Word::new_static(DENY_REASON)),
    }
}
