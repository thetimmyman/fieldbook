//! Defined label table (`LabelTable`) and unknown-label check (`FB0411`)
//! (`docs/decisions/0004-company-boundary.md#2-labels`).

use std::collections::BTreeSet;

use crate::check::Code;

use super::id::{BUILTINS, LabelId};
use super::set::LabelSet;

/// One warning finding (`FB0411`) for a label id in a [`LabelSet`] that is outside [`LabelTable`].
///
/// Names the opaque label id and never a human title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelFinding {
    /// Always [`Code::Fb0411`].
    pub code: Code,
    /// The unknown label identifier.
    pub label: LabelId,
    /// Human-readable diagnostic message naming only the opaque id.
    pub message: String,
}

/// The table of defined label identifiers and which of them are restricted (`restricted = true`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelTable {
    known: BTreeSet<LabelId>,
    restricted: BTreeSet<LabelId>,
}

impl LabelTable {
    /// Returns the built-in table containing only the four built-in ids (`BUILTINS`), none restricted.
    #[must_use]
    pub fn builtin() -> Self {
        Self {
            known: BUILTINS.into_iter().collect(),
            restricted: BTreeSet::new(),
        }
    }

    /// Returns a table containing the four built-in ids plus `ids`, marking `restricted` ids as restricted.
    #[must_use]
    pub fn with<I, R>(ids: I, restricted: R) -> Self
    where
        I: IntoIterator<Item = LabelId>,
        R: IntoIterator<Item = LabelId>,
    {
        let mut known: BTreeSet<LabelId> = BUILTINS.into_iter().collect();
        known.extend(ids);
        let restricted: BTreeSet<LabelId> = restricted
            .into_iter()
            .filter(|id| !id.is_builtin())
            .collect();
        known.extend(restricted.iter().cloned());
        Self { known, restricted }
    }

    /// Returns `true` when `id` is in the table (including built-ins).
    #[must_use]
    pub fn is_known(&self, id: &LabelId) -> bool {
        self.known.contains(id)
    }

    /// Returns `true` when `id` is marked `restricted = true` in the table.
    #[must_use]
    pub fn is_restricted(&self, id: &LabelId) -> bool {
        self.restricted.contains(id)
    }

    /// Returns an iterator over all known label ids in sorted order.
    pub fn known_ids(&self) -> impl Iterator<Item = &LabelId> {
        self.known.iter()
    }
}

impl Default for LabelTable {
    fn default() -> Self {
        Self::builtin()
    }
}

/// Checks every label in `set` against `table`, returning one [`LabelFinding`] (`FB0411`)
/// for each label outside the table.
///
/// A caller treats a set with any unknown label finding as unresolved.
#[must_use]
pub fn check_known(set: &LabelSet, table: &LabelTable) -> Vec<LabelFinding> {
    let mut findings = Vec::new();
    for id in set.ids() {
        if !table.is_known(id) {
            findings.push(LabelFinding {
                code: Code::Fb0411,
                label: id.clone(),
                message: format!("label `{id}` is not defined in the company label table"),
            });
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> LabelId {
        LabelId::parse(s).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn unknown_id_is_fb0411() {
        let table = LabelTable::with([id("np-0007")], []);
        let set = LabelSet::from_ids([id("np-0007"), id("bid-0003")]);
        let findings = check_known(&set, &table);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, Code::Fb0411);
        assert_eq!(findings[0].label, id("bid-0003"));
        assert!(findings[0].message.contains("bid-0003"));
    }

    #[test]
    fn builtins_are_always_known() {
        let table = LabelTable::builtin();
        for builtin in BUILTINS {
            assert!(table.is_known(&builtin));
            let set = LabelSet::from_ids([builtin]);
            assert!(check_known(&set, &table).is_empty());
        }
    }

    #[test]
    fn a_known_set_has_no_finding() {
        let np = id("np-0007");
        let bid = id("bid-0003");
        let table = LabelTable::with([np.clone(), bid.clone()], []);
        let set = LabelSet::from_ids([np, bid]);
        assert!(check_known(&set, &table).is_empty());
    }

    #[test]
    fn restricted_ids_are_reported() {
        let np = id("np-0007");
        let ctl = id("ctl-0002");
        let table = LabelTable::with([np.clone(), ctl.clone()], [ctl.clone()]);
        assert!(!table.is_restricted(&np));
        assert!(table.is_restricted(&ctl));
        for builtin in BUILTINS {
            assert!(!table.is_restricted(&builtin));
        }
    }
}
