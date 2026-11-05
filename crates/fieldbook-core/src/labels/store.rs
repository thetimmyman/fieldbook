//! Store identifiers (`StoreId`), store rules (`StoreRule`), and store admission (`admit`)
//! (`docs/decisions/0004-company-boundary.md#4-stores-and-admission`, `FB0400`, `FB0410`, `FB0412`).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::check::Code;

use super::id::{MARKED, OWNER_SUPPLIED, PUBLIC, UNLABELLED};
use super::set::{LabelAllow, LabelSet};

/// Identifier of one of the four data roots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StoreId {
    /// The repository Corpus at home or the shipped edition on the laptop (`"corpus"`).
    Corpus,
    /// The company store (`"overlay"`).
    Overlay,
    /// The owner's working notes and drafts (`"working"`).
    Working,
    /// State, logs, inbox, staging, queue and derived site (`"state"`).
    State,
}

impl StoreId {
    /// All four store roots in canonical order.
    pub const ALL: [Self; 4] = [Self::Corpus, Self::Overlay, Self::Working, Self::State];

    /// Returns the root directory name (`"corpus"`, `"overlay"`, `"working"`, `"state"`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Corpus => "corpus",
            Self::Overlay => "overlay",
            Self::Working => "working",
            Self::State => "state",
        }
    }

    /// Parses a store identifier from its root name.
    #[must_use]
    pub fn from_root_name(name: &str) -> Option<Self> {
        match name {
            "corpus" => Some(Self::Corpus),
            "overlay" => Some(Self::Overlay),
            "working" => Some(Self::Working),
            "state" => Some(Self::State),
            _ => None,
        }
    }
}

impl fmt::Display for StoreId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Closed reason word recorded in the journal `refuse` event for a store, git, place, or change refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RefuseReason {
    /// Store admission refused (`"store"`).
    Store,
    /// Git work-tree guard refused (`"git"`).
    Git,
    /// Folder placement check refused (`"place"`).
    Place,
    /// Label change or typed public act refused (`"label"`).
    Label,
}

impl RefuseReason {
    /// Returns the canonical journal reason word.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Store => "store",
            Self::Git => "git",
            Self::Place => "place",
            Self::Label => "label",
        }
    }
}

impl fmt::Display for RefuseReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A refusal returned by store admission, the git guard, placement, or `label add` / `PublicAct::typed`.
///
/// Holds only the error [`Code`], the closed [`RefuseReason`] word, and the [`LabelSet`] ids—never
/// document text or a human label title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// The diagnostic code (`FB0400`, `FB0410`, `FB0412`, `FB0415`, or `FB0822`).
    pub code: Code,
    /// The closed journal reason word (`store`, `git`, `place`).
    pub reason: RefuseReason,
    /// The label ids involved in the refusal.
    pub labels: LabelSet,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: refused ({}) for labels `{}`",
            self.code.id(),
            self.reason.as_str(),
            self.labels.to_cell()
        )
    }
}

impl std::error::Error for Refusal {}

/// What a store may hold, its default label set, its optional OS reader/changer groups, and its hold switch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreRule {
    /// Which store this rule governs.
    pub store: StoreId,
    /// Labels this store is permitted to hold (`LabelAllow`).
    pub holds: LabelAllow,
    /// Default label set given to notes the owner writes in this store (`LabelSet`).
    pub default: LabelSet,
    /// Optional operating-system group name required to read this store.
    pub readers: Option<String>,
    /// Optional operating-system group name required to run `label add` on this store.
    pub changers: Option<String>,
    /// Whether the store is on hold (refusing purge and label changes).
    pub hold: bool,
}

impl StoreRule {
    /// Returns the fixed rule for the `corpus` store: holds `public` only.
    #[must_use]
    pub fn corpus() -> Self {
        Self {
            store: StoreId::Corpus,
            holds: LabelAllow::from_ids([PUBLIC]),
            default: LabelSet::public(),
            readers: None,
            changers: None,
            hold: false,
        }
    }

    /// Returns the floor rule for `store` under ADR 0004 section 3.
    ///
    /// - `corpus` holds `public` only.
    /// - `overlay` and `working` hold `public` and `unlabelled` (plus `marked` when `holds_marked` is `true`).
    /// - `state` holds `public`, `unlabelled`, and `owner-supplied` (plus `marked` when `holds_marked` is `true`).
    /// - `overlay`, `working`, and `state` all default to `unlabelled`.
    #[must_use]
    pub fn floor(store: StoreId, holds_marked: bool) -> Self {
        if store == StoreId::Corpus {
            return Self::corpus();
        }
        let mut holds_ids = vec![PUBLIC, UNLABELLED];
        if store == StoreId::State {
            holds_ids.push(OWNER_SUPPLIED);
        }
        if holds_marked {
            holds_ids.push(MARKED);
        }
        Self {
            store,
            holds: LabelAllow::from_ids(holds_ids),
            default: LabelSet::unlabelled(),
            readers: None,
            changers: None,
            hold: false,
        }
    }
}

/// Decides whether a record carrying `labels` may be admitted to the store governed by `rule`.
///
/// - In `StoreId::Corpus`, a set holding `marked` is refused with `FB0400`, and any other set that
///   is not `public` is refused with `FB0410`.
/// - In every other store, a set with any label outside `rule.holds` is refused with `FB0412`.
/// - An item in `state\inbox`, `staging`, or the queue is admitted by passing the `StoreRule` of
///   the store it is bound for.
pub fn admit(labels: &LabelSet, rule: &StoreRule) -> Result<(), Refusal> {
    if rule.store == StoreId::Corpus {
        if labels.contains(&MARKED) {
            return Err(Refusal {
                code: Code::Fb0400,
                reason: RefuseReason::Store,
                labels: labels.clone(),
            });
        }
        if !labels.is_public() {
            return Err(Refusal {
                code: Code::Fb0410,
                reason: RefuseReason::Store,
                labels: labels.clone(),
            });
        }
        return Ok(());
    }

    if rule.holds.permits(labels) {
        Ok(())
    } else {
        Err(Refusal {
            code: Code::Fb0412,
            reason: RefuseReason::Store,
            labels: labels.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::labels::id::LabelId;

    fn id(s: &str) -> LabelId {
        LabelId::parse(s).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn corpus_refuses_marked_with_fb0400() {
        let rule = StoreRule::corpus();
        let err = admit(&LabelSet::marked(), &rule).unwrap_err();
        assert_eq!(err.code, Code::Fb0400);
        assert_eq!(err.reason, RefuseReason::Store);
        assert_eq!(err.labels, LabelSet::marked());
    }

    #[test]
    fn corpus_refuses_another_label_with_fb0410() {
        let rule = StoreRule::corpus();
        for set in [
            LabelSet::unlabelled(),
            LabelSet::owner_supplied(),
            LabelSet::from_ids([id("np-0007")]),
        ] {
            let err = admit(&set, &rule).unwrap_err();
            assert_eq!(err.code, Code::Fb0410);
            assert_eq!(err.reason, RefuseReason::Store);
            assert_eq!(err.labels, set);
        }
    }

    #[test]
    fn corpus_admits_public() {
        let rule = StoreRule::corpus();
        assert_eq!(admit(&LabelSet::public(), &rule), Ok(()));
    }

    #[test]
    fn overlay_floor_admits_unlabelled_and_refuses_owner_supplied() {
        let rule = StoreRule::floor(StoreId::Overlay, false);
        assert_eq!(admit(&LabelSet::public(), &rule), Ok(()));
        assert_eq!(admit(&LabelSet::unlabelled(), &rule), Ok(()));
        let err = admit(&LabelSet::owner_supplied(), &rule).unwrap_err();
        assert_eq!(err.code, Code::Fb0412);
    }

    #[test]
    fn state_floor_admits_owner_supplied_for_its_logs() {
        let rule = StoreRule::floor(StoreId::State, false);
        assert_eq!(admit(&LabelSet::owner_supplied(), &rule), Ok(()));
        assert_eq!(admit(&LabelSet::unlabelled(), &rule), Ok(()));
        assert_eq!(admit(&LabelSet::public(), &rule), Ok(()));
    }

    #[test]
    fn overlay_floor_refuses_a_company_label_with_fb0412() {
        let rule = StoreRule::floor(StoreId::Overlay, false);
        let set = LabelSet::from_ids([id("np-0007")]);
        let err = admit(&set, &rule).unwrap_err();
        assert_eq!(err.code, Code::Fb0412);
        assert_eq!(err.reason, RefuseReason::Store);
        assert_eq!(err.labels, set);
    }

    #[test]
    fn floor_b_rows_admit_marked_and_floor_a_rows_refuse_it() {
        for store in [StoreId::Overlay, StoreId::Working, StoreId::State] {
            let floor_a = StoreRule::floor(store, false);
            let err = admit(&LabelSet::marked(), &floor_a).unwrap_err();
            assert_eq!(err.code, Code::Fb0412, "floor A {store} must refuse marked");

            let floor_b = StoreRule::floor(store, true);
            assert_eq!(
                admit(&LabelSet::marked(), &floor_b),
                Ok(()),
                "floor B {store} must admit marked"
            );
        }
    }

    #[test]
    fn a_set_is_admitted_only_if_every_label_is_held() {
        let np = id("np-0007");
        let bid = id("bid-0003");
        let rule = StoreRule {
            store: StoreId::Overlay,
            holds: LabelAllow::from_ids([PUBLIC, UNLABELLED, np.clone()]),
            default: LabelSet::unlabelled(),
            readers: None,
            changers: None,
            hold: false,
        };
        assert_eq!(admit(&LabelSet::from_ids([np.clone()]), &rule), Ok(()));
        let both = LabelSet::from_ids([np, bid]);
        let err = admit(&both, &rule).unwrap_err();
        assert_eq!(err.code, Code::Fb0412);
    }

    #[test]
    fn a_staged_item_is_admitted_by_the_store_it_is_bound_for() {
        let np = id("np-0007");
        // Policy lists `np-0007` once on `overlay` and not on `state`.
        let overlay_rule = StoreRule {
            store: StoreId::Overlay,
            holds: LabelAllow::from_ids([PUBLIC, UNLABELLED, np.clone()]),
            default: LabelSet::unlabelled(),
            readers: None,
            changers: None,
            hold: false,
        };
        let state_rule = StoreRule::floor(StoreId::State, false);
        let item_labels = LabelSet::from_ids([np]);

        // Direct admission to `state` (for logs/derived site) refuses `np-0007`,
        // while a staged item in `state\inbox` bound for `overlay` is admitted by `overlay_rule`.
        assert_eq!(
            admit(&item_labels, &state_rule).unwrap_err().code,
            Code::Fb0412
        );
        assert_eq!(admit(&item_labels, &overlay_rule), Ok(()));
    }

    #[test]
    fn the_refusal_holds_ids_and_no_text() {
        let np = id("np-0007");
        let rule = StoreRule::floor(StoreId::Overlay, false);
        let err = admit(&LabelSet::from_ids([np.clone()]), &rule).unwrap_err();
        let rendered = err.to_string();
        assert!(rendered.contains("FB0412"));
        assert!(rendered.contains("np-0007"));
        assert_eq!(err.labels.ids(), &[np]);
    }
}
