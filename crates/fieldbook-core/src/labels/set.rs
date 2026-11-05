//! Data label sets (`LabelSet`) and permission allow-lists (`LabelAllow`)
//! (`docs/decisions/0004-company-boundary.md#2-labels`).

use std::fmt;

use serde::{Deserialize, Serialize};

use super::id::{LabelId, LabelIdError, MARKED, OWNER_SUPPLIED, PUBLIC, UNLABELLED};

/// Error returned when parsing a `|`-separated `handling` cell into a [`LabelSet`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LabelSetError {
    /// The cell was empty (`""`), which is a defect in a file.
    EmptyCell,
    /// One of the `|`-separated segments was not a valid [`LabelId`].
    InvalidId(LabelIdError),
}

impl fmt::Display for LabelSetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyCell => f.write_str("handling cell must not be empty"),
            Self::InvalidId(err) => Display::fmt(err, f),
        }
    }
}

use std::fmt::Display;
impl std::error::Error for LabelSetError {}

/// A non-empty, sorted, deduplicated set of data labels carried by an item or request.
///
/// The inner field is private so that all construction goes through ADR 0004's normalization rules:
/// - An empty input becomes `unlabelled`.
/// - `public` is dropped when any other label is present.
/// - `unlabelled` and `marked` survive beside every other label.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LabelSet {
    ids: Vec<LabelId>,
}

impl LabelSet {
    /// Returns the singleton `public` label set.
    #[must_use]
    pub fn public() -> Self {
        Self { ids: vec![PUBLIC] }
    }

    /// Returns the singleton `unlabelled` label set.
    #[must_use]
    pub fn unlabelled() -> Self {
        Self {
            ids: vec![UNLABELLED],
        }
    }

    /// Returns the singleton `marked` label set.
    #[must_use]
    pub fn marked() -> Self {
        Self { ids: vec![MARKED] }
    }

    /// Returns the singleton `owner-supplied` label set.
    #[must_use]
    pub fn owner_supplied() -> Self {
        Self {
            ids: vec![OWNER_SUPPLIED],
        }
    }

    /// Builds a normalized [`LabelSet`] from an iterator of [`LabelId`] values.
    ///
    /// - Empty input becomes `unlabelled`.
    /// - `public` is dropped when any other label is present.
    /// - `unlabelled` and `marked` are retained beside every other label.
    /// - The resulting slice is sorted and free of duplicates.
    #[must_use]
    pub fn from_ids<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = LabelId>,
    {
        let mut ids: Vec<LabelId> = iter.into_iter().collect();
        ids.sort();
        ids.dedup();
        if ids.is_empty() {
            return Self::unlabelled();
        }
        if ids.len() > 1 {
            ids.retain(|id| *id != PUBLIC);
        }
        Self { ids }
    }

    /// Returns a new [`LabelSet`] with `id` added under the data-label rules.
    ///
    /// Both `unlabelled` and `marked` survive; `public` is dropped if another label is present.
    #[must_use]
    pub fn with(&self, id: LabelId) -> Self {
        let mut next = Vec::with_capacity(self.ids.len() + 1);
        next.extend(self.ids.iter().cloned());
        next.push(id);
        Self::from_ids(next)
    }

    /// Returns the derived-data union of `self` and `other`.
    ///
    /// Data union is associative, commutative and idempotent; `unlabelled` and `marked` survive it.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        let mut acc = self.clone();
        for id in &other.ids {
            acc = acc.with(id.clone());
        }
        acc
    }

    /// Returns `true` when every label in `self` is also in `other`.
    #[must_use]
    pub fn is_subset_of(&self, other: &Self) -> bool {
        self.ids.iter().all(|id| other.contains(id))
    }

    /// Returns `true` when `self` contains `id`.
    #[must_use]
    pub fn contains(&self, id: &LabelId) -> bool {
        self.ids.binary_search(id).is_ok()
    }

    /// Returns `true` when `self` is exactly `["public"]`.
    #[must_use]
    pub fn is_public(&self) -> bool {
        self.ids.as_slice() == [PUBLIC]
    }

    /// Returns `true` when `self` holds `unlabelled` or `marked`.
    #[must_use]
    pub fn is_unresolved(&self) -> bool {
        self.contains(&UNLABELLED) || self.contains(&MARKED)
    }

    /// Returns the sorted slice of [`LabelId`] values in this set.
    #[must_use]
    pub fn ids(&self) -> &[LabelId] {
        &self.ids
    }

    /// Formats the set as a `|`-joined cell for CSV `handling` columns.
    #[must_use]
    pub fn to_cell(&self) -> String {
        self.ids
            .iter()
            .map(LabelId::as_str)
            .collect::<Vec<_>>()
            .join("|")
    }

    /// Strictly parses a `|`-joined `handling` cell.
    ///
    /// An empty cell is an error (`LabelSetError::EmptyCell`).
    pub fn parse_cell(cell: &str) -> Result<Self, LabelSetError> {
        if cell.is_empty() {
            return Err(LabelSetError::EmptyCell);
        }
        let mut parsed = Vec::new();
        for part in cell.split('|') {
            let id = LabelId::parse(part).map_err(LabelSetError::InvalidId)?;
            parsed.push(id);
        }
        Ok(Self::from_ids(parsed))
    }

    /// Replaces `marked` with `found`, refusing (`None`) when `found` itself holds `marked`.
    ///
    /// Called only by `label reapply` (and ledger `reapply` application).
    #[must_use]
    pub fn reapplied(&self, found: &Self) -> Option<Self> {
        if found.contains(&MARKED) {
            return None;
        }
        let mut next: Vec<LabelId> = self
            .ids
            .iter()
            .filter(|&id| *id != MARKED)
            .cloned()
            .collect();
        next.extend(found.ids.iter().cloned());
        Some(Self::from_ids(next))
    }

    /// Applies an explicit person or intake classification from the label ledger.
    ///
    /// Refuses (`None`) if `added` contains any built-in label (`public`, `unlabelled`,
    /// `marked`, or `owner-supplied`). Removes `unlabelled` from `self` and adds `added`,
    /// while preserving `marked`.
    #[must_use]
    pub fn classified(&self, added: &Self) -> Option<Self> {
        if added.ids.iter().any(LabelId::is_builtin) {
            return None;
        }
        let mut next: Vec<LabelId> = self
            .ids
            .iter()
            .filter(|&id| *id != UNLABELLED)
            .cloned()
            .collect();
        next.extend(added.ids.iter().cloned());
        Some(Self::from_ids(next))
    }
}

/// A sorted, deduplicated permission allow-list of [`LabelId`] values.
///
/// Unlike [`LabelSet`], a [`LabelAllow`] may be empty and retains `public` beside company
/// or built-in labels without data-label normalization. Used for `StoreRule::holds` and
/// destination/owner allow-lists.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LabelAllow {
    ids: Vec<LabelId>,
}

impl LabelAllow {
    /// Returns an empty permission set that permits nothing.
    #[must_use]
    pub const fn empty() -> Self {
        Self { ids: Vec::new() }
    }

    /// Builds a sorted, deduplicated permission list without data-label normalization.
    #[must_use]
    pub fn from_ids<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = LabelId>,
    {
        let mut ids: Vec<LabelId> = iter.into_iter().collect();
        ids.sort();
        ids.dedup();
        Self { ids }
    }

    /// Returns the sorted slice of allowed [`LabelId`] values.
    #[must_use]
    pub fn ids(&self) -> &[LabelId] {
        &self.ids
    }

    /// Returns `true` when this permission list is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Returns `true` when `id` is in this permission list.
    #[must_use]
    pub fn contains(&self, id: &LabelId) -> bool {
        self.ids.binary_search(id).is_ok()
    }

    /// Returns the set intersection of `self` and `other`.
    #[must_use]
    pub fn intersection(&self, other: &Self) -> Self {
        let ids = self
            .ids
            .iter()
            .filter(|id| other.contains(id))
            .cloned()
            .collect();
        Self { ids }
    }

    /// Returns the set union of `self` and `other`, keeping `public` beside other labels.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        Self::from_ids(self.ids.iter().cloned().chain(other.ids.iter().cloned()))
    }

    /// Returns `true` when every label in `set` is present in `self`.
    #[must_use]
    pub fn permits(&self, set: &LabelSet) -> bool {
        set.ids().iter().all(|id| self.contains(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> LabelId {
        LabelId::parse(s).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn empty_becomes_unlabelled() {
        let set = LabelSet::from_ids(std::iter::empty());
        assert_eq!(set, LabelSet::unlabelled());
        assert_eq!(set.ids(), &[UNLABELLED]);
    }

    #[test]
    fn public_is_dropped_beside_another_label() {
        let np = id("np-0007");
        let set = LabelSet::from_ids([PUBLIC, np.clone()]);
        assert_eq!(set.ids(), std::slice::from_ref(&np));
        assert!(!set.is_public());

        let with_pub = LabelSet::public().with(np.clone());
        assert_eq!(with_pub.ids(), &[np]);
    }

    #[test]
    fn unlabelled_survives_derived_union() {
        let np = id("np-0007");
        let with_real = LabelSet::unlabelled().with(np.clone());
        assert!(with_real.contains(&UNLABELLED));
        assert!(with_real.contains(&np));
        assert!(with_real.is_unresolved());

        let unioned = LabelSet::unlabelled().union(&LabelSet::from_ids([np.clone()]));
        assert_eq!(unioned.ids(), &[np, UNLABELLED]);
    }

    #[test]
    fn marked_survives_with_union_and_from_ids() {
        let np = id("np-0007");
        let s1 = LabelSet::marked().with(np.clone());
        assert!(s1.contains(&MARKED));
        assert!(s1.contains(&np));

        let s2 = LabelSet::marked().union(&LabelSet::from_ids([np.clone()]));
        assert!(s2.contains(&MARKED));
        assert!(s2.contains(&np));

        let s3 = LabelSet::from_ids([MARKED, PUBLIC, np]);
        assert!(s3.contains(&MARKED));
        assert!(!s3.contains(&PUBLIC));
    }

    #[test]
    fn reapplied_is_the_only_way_to_drop_marked() {
        let np = id("np-0007");
        let bid = id("bid-0003");
        let initial = LabelSet::from_ids([MARKED, np.clone()]);

        // Reapplying with a set that still holds `marked` is refused (`None`).
        assert_eq!(initial.reapplied(&LabelSet::marked()), None);
        assert_eq!(
            initial.reapplied(&LabelSet::from_ids([MARKED, bid.clone()])),
            None
        );

        // Reapplying with a mapped label replaces `marked` and keeps existing labels.
        let resolved = initial.reapplied(&LabelSet::from_ids([bid.clone()]));
        assert_eq!(resolved, Some(LabelSet::from_ids([bid, np])));
    }

    #[test]
    fn union_is_the_derived_rule() {
        let np = id("np-0007");
        let bid = id("bid-0003");
        let a = LabelSet::from_ids([np.clone()]);
        let b = LabelSet::from_ids([bid.clone()]);
        assert_eq!(a.union(&b).ids(), &[bid, np]);
        assert_eq!(
            LabelSet::public().union(&LabelSet::public()),
            LabelSet::public()
        );
    }

    #[test]
    fn subset_is_the_sending_rule() {
        let np = id("np-0007");
        let bid = id("bid-0003");
        let single = LabelSet::from_ids([np.clone()]);
        let both = LabelSet::from_ids([np, bid]);
        assert!(single.is_subset_of(&both));
        assert!(!both.is_subset_of(&single));
    }

    #[test]
    fn a_set_is_sorted_and_unique() {
        let np = id("np-0007");
        let bid = id("bid-0003");
        let set = LabelSet::from_ids([np.clone(), bid.clone(), np.clone(), bid.clone()]);
        assert_eq!(set.ids(), &[bid, np]);
    }

    #[test]
    fn cell_round_trip() {
        let np = id("np-0007");
        let bid = id("bid-0003");
        let set = LabelSet::from_ids([np, bid]);
        let cell = set.to_cell();
        assert_eq!(cell, "bid-0003|np-0007");
        assert_eq!(LabelSet::parse_cell(&cell), Ok(set));
    }

    #[test]
    fn an_empty_cell_is_an_error() {
        assert_eq!(LabelSet::parse_cell(""), Err(LabelSetError::EmptyCell));
        assert!(LabelSet::parse_cell("np-0007|").is_err());
    }

    #[test]
    fn permission_sets_keep_empty_and_mixed_public() {
        let empty = LabelAllow::empty();
        assert!(empty.is_empty());
        assert!(!empty.permits(&LabelSet::public()));
        assert!(!empty.permits(&LabelSet::unlabelled()));

        let np = id("np-0007");
        let mixed = LabelAllow::from_ids([PUBLIC, np.clone()]);
        assert_eq!(mixed.ids(), &[np.clone(), PUBLIC]);
        assert!(mixed.permits(&LabelSet::public()));
        assert!(mixed.permits(&LabelSet::from_ids([np])));
        assert_eq!(mixed.intersection(&empty), empty);
    }

    #[test]
    fn data_union_preserves_unresolved_inputs() {
        let np = id("np-0007");
        let u1 = LabelSet::unlabelled().union(&LabelSet::from_ids([np.clone()]));
        assert!(u1.contains(&UNLABELLED));
        assert!(u1.contains(&np));

        let u2 = LabelSet::marked().union(&LabelSet::from_ids([np.clone()]));
        assert!(u2.contains(&MARKED));
        assert!(u2.contains(&np));
    }

    #[test]
    fn label_union_laws() {
        let samples = [
            LabelSet::public(),
            LabelSet::unlabelled(),
            LabelSet::marked(),
            LabelSet::from_ids([id("np-0007")]),
            LabelSet::from_ids([id("bid-0003"), UNLABELLED]),
            LabelSet::from_ids([id("ctl-0002"), MARKED]),
        ];
        for a in &samples {
            // Idempotence
            assert_eq!(&a.union(a), a);
            for b in &samples {
                // Commutativity
                assert_eq!(a.union(b), b.union(a));
                for c in &samples {
                    // Associativity
                    assert_eq!(a.union(b).union(c), a.union(&b.union(c)));
                }
            }
        }
    }
}
