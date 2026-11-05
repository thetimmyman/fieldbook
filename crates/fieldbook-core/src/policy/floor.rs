//! Built-in floor policy (`FloorKind::{A, B}`, `Policy::floor`, `Policy::store_rule`,
//! `Policy::floor_sentence`, `FLOOR_HASH_TEXT`, `policy_hash`, `docs/reference/policy.md#the-floor-policy`).

use std::collections::BTreeSet;

use crate::audit::DestKind;
use crate::hash::sha256_hex;
use crate::labels::{
    LabelAllow, LabelId, LabelTable, MarkingRules, OWNER_SUPPLIED, PUBLIC, StoreId, StoreRule,
    UNLABELLED,
};

use super::parse::{Policy, PolicyAudit, PolicyDestination};
use super::state::PolicyState;

/// Constant text whose SHA-256 digest is the policy hash while the built-in floor applies.
pub const FLOOR_HASH_TEXT: &str = "fieldbook-floor-policy-1";

/// Built-in floor variant selected by whether a company store exists on the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FloorKind {
    /// Floor A: no company store on the machine.
    A,
    /// Floor B: a company store exists on the machine and no policy has been loaded.
    B,
}

impl FloorKind {
    /// Selects the floor variant for the machine layout (`A` when `has_company_store` is `false`,
    /// `B` when `has_company_store` is `true`).
    #[must_use]
    pub const fn for_machine(has_company_store: bool) -> Self {
        if has_company_store { Self::B } else { Self::A }
    }
}

/// Computes the 64-character lowercase hexadecimal SHA-256 policy hash for `state`.
///
/// - For [`PolicyState::Loaded`], returns the SHA-256 of the policy file's raw bytes.
/// - For [`PolicyState::Floor`] and [`PolicyState::Closed`], returns the SHA-256 of [`FLOOR_HASH_TEXT`].
#[must_use]
pub fn policy_hash(state: &PolicyState) -> String {
    match state {
        PolicyState::Loaded(policy) => policy.hash().to_owned(),
        PolicyState::Floor(_) | PolicyState::Closed(_) => sha256_hex(FLOOR_HASH_TEXT.as_bytes()),
    }
}

impl Policy {
    /// Returns the built-in floor policy for `kind` (`FloorKind::A` or `FloorKind::B`).
    #[must_use]
    pub fn floor(kind: FloorKind) -> Self {
        let holds_marked = matches!(kind, FloorKind::B);
        let stores = vec![
            StoreRule::corpus(),
            StoreRule::floor(StoreId::Overlay, holds_marked),
            StoreRule::floor(StoreId::Working, holds_marked),
            StoreRule::floor(StoreId::State, holds_marked),
        ];

        let public_only = LabelAllow::from_ids([PUBLIC]);
        let public_and_owner = LabelAllow::from_ids([PUBLIC, OWNER_SUPPLIED]);

        let model_allow = match kind {
            FloorKind::A => public_only.clone(),
            FloorKind::B => LabelAllow::empty(),
        };
        let export_allow = match kind {
            FloorKind::A => public_and_owner.clone(),
            FloorKind::B => LabelAllow::empty(),
        };
        let (backup_allow, backup_company_storage_only) = match kind {
            FloorKind::A => (public_and_owner.clone(), false),
            FloorKind::B => (
                LabelAllow::from_ids([PUBLIC, UNLABELLED, OWNER_SUPPLIED]),
                true,
            ),
        };

        let destinations = vec![
            PolicyDestination {
                kind: DestKind::Handoff,
                allow: public_and_owner,
                name: None,
                url: None,
                expires: None,
                keep_calls_days_max: None,
                reference: None,
                clients: None,
                paths: None,
                company_storage_only: false,
            },
            PolicyDestination {
                kind: DestKind::Model,
                allow: model_allow,
                name: None,
                url: None,
                expires: None,
                keep_calls_days_max: None,
                reference: None,
                clients: None,
                paths: None,
                company_storage_only: false,
            },
            PolicyDestination {
                kind: DestKind::Mcp,
                allow: public_only.clone(),
                name: None,
                url: None,
                expires: None,
                keep_calls_days_max: None,
                reference: None,
                clients: None,
                paths: None,
                company_storage_only: false,
            },
            PolicyDestination {
                kind: DestKind::Bridge,
                allow: public_only.clone(),
                name: None,
                url: None,
                expires: None,
                keep_calls_days_max: None,
                reference: None,
                clients: None,
                paths: None,
                company_storage_only: false,
            },
            PolicyDestination {
                kind: DestKind::Print,
                allow: public_only.clone(),
                name: None,
                url: None,
                expires: None,
                keep_calls_days_max: None,
                reference: None,
                clients: None,
                paths: None,
                company_storage_only: false,
            },
            PolicyDestination {
                kind: DestKind::Clipboard,
                allow: public_only,
                name: None,
                url: None,
                expires: None,
                keep_calls_days_max: None,
                reference: None,
                clients: None,
                paths: None,
                company_storage_only: false,
            },
            PolicyDestination {
                kind: DestKind::Export,
                allow: export_allow,
                name: None,
                url: None,
                expires: None,
                keep_calls_days_max: None,
                reference: None,
                clients: None,
                paths: None,
                company_storage_only: false,
            },
            PolicyDestination {
                kind: DestKind::Backup,
                allow: backup_allow,
                name: None,
                url: None,
                expires: None,
                keep_calls_days_max: None,
                reference: None,
                clients: None,
                paths: None,
                company_storage_only: backup_company_storage_only,
            },
        ];

        Self {
            revision: 0,
            hash: sha256_hex(FLOOR_HASH_TEXT.as_bytes()),
            labels: Vec::new(),
            label_table: LabelTable::builtin(),
            markings: MarkingRules::builtin(),
            sources: Vec::new(),
            stores,
            explicit_stores: BTreeSet::new(),
            destinations,
            audit: PolicyAudit::default(),
            floor_kind: Some(kind),
        }
    }

    /// Returns the effective [`StoreRule`] for `store`:
    /// - Always [`StoreRule::corpus`] for [`StoreId::Corpus`].
    /// - The policy's explicit `[[store]]` row if one was loaded.
    /// - The floor table's own row when this `Policy` is a floor policy (`FloorKind::A` or `FloorKind::B`).
    /// - [`StoreRule::floor`]`(*store, false)` (without `marked`) when this is a loaded policy and `store` has no row.
    #[must_use]
    pub fn store_rule(&self, store: &StoreId) -> StoreRule {
        if *store == StoreId::Corpus {
            return StoreRule::corpus();
        }
        if let Some(row) = self.store_row(store) {
            return row.clone();
        }
        if self.floor_kind.is_some()
            && let Some(row) = self.stores.iter().find(|r| &r.store == store)
        {
            return row.clone();
        }
        StoreRule::floor(*store, false)
    }

    /// Updates the `allow` set of the destination row for `kind` (used by floor-sentence tests).
    pub fn set_dest_allow(&mut self, kind: DestKind, allow: LabelAllow) {
        if let Some(row) = self.destinations.iter_mut().find(|d| d.kind == kind) {
            row.allow = allow;
        }
    }

    /// Updates the `holds` set of `store` in the policy's store list (used by floor-sentence tests).
    pub fn set_store_holds(&mut self, store: StoreId, holds: LabelAllow) {
        if let Some(row) = self.stores.iter_mut().find(|r| r.store == store) {
            row.holds = holds;
        }
    }

    /// Writes the Floor block of `docs/reference/policy.md` (the text after `> **Floor.** `)
    /// from the built-in Floor A and Floor B tables.
    #[must_use]
    pub fn floor_sentence() -> String {
        let floor_a = Self::floor(FloorKind::A);
        let floor_b = Self::floor(FloorKind::B);
        Self::floor_sentence_from(&floor_a, &floor_b)
    }

    /// Writes the Floor block from two floor [`Policy`] values (`floor_a` and `floor_b`).
    #[must_use]
    pub fn floor_sentence_from(floor_a: &Self, floor_b: &Self) -> String {
        let non_corpus = [StoreId::Overlay, StoreId::Working, StoreId::State];
        let overlay_holds = floor_a.store_rule(&StoreId::Overlay).holds;
        let working_holds = floor_a.store_rule(&StoreId::Working).holds;
        let state_holds = floor_a.store_rule(&StoreId::State).holds;
        let corpus_holds = floor_a.store_rule(&StoreId::Corpus).holds;

        // Shared base holds across overlay, working, state
        let shared_holds = overlay_holds
            .intersection(&working_holds)
            .intersection(&state_holds);
        let shared_store_names: Vec<String> = non_corpus
            .iter()
            .filter(|s| {
                shared_holds
                    .ids()
                    .iter()
                    .all(|id| floor_a.store_rule(s).holds.contains(id))
            })
            .map(|s| format!("`{}`", s.as_str()))
            .collect();

        let state_extra_ids: Vec<LabelId> = state_holds
            .ids()
            .iter()
            .filter(|id| !shared_holds.contains(id))
            .cloned()
            .collect();

        let stores_sentence = format!(
            "The stores {} hold {}, `state` also holds {}, and `corpus` holds {}.",
            join_prose(&shared_store_names),
            format_label_list(shared_holds.ids()),
            format_label_list(&state_extra_ids),
            format_label_list(corpus_holds.ids()),
        );

        // Group Floor A destination kinds by distinct non-empty `allow` set in DEST_KINDS order,
        // with the set having more labels first.
        let mut groups: Vec<(LabelAllow, Vec<DestKind>)> = Vec::new();
        for spec in crate::egress::DEST_KINDS {
            let Some(row) = floor_a.destinations.iter().find(|d| d.kind == spec.kind) else {
                continue;
            };
            if row.allow.is_empty() {
                continue;
            }
            if let Some((_, kinds)) = groups.iter_mut().find(|(a, _)| *a == row.allow) {
                kinds.push(spec.kind);
            } else {
                groups.push((row.allow.clone(), vec![spec.kind]));
            }
        }
        groups.sort_by_key(|(a, _)| std::cmp::Reverse(a.ids().len()));

        let mut dest_sentences = Vec::with_capacity(groups.len());
        for (allow, kinds) in &groups {
            let phrases: Vec<&str> = kinds
                .iter()
                .enumerate()
                .map(|(idx, &k)| floor_dest_phrase(k, idx == 0))
                .collect();
            let verb =
                if kinds.len() == 1 && !matches!(kinds[0], DestKind::Export | DestKind::Backup) {
                    "receives"
                } else {
                    "receive"
                };
            dest_sentences.push(format!(
                "{} {verb} {}.",
                join_prose(&phrases),
                format_label_list(allow.ids()),
            ));
        }

        // Floor B differences
        let mut b_parts: Vec<String> = Vec::new();

        let mut expanded_stores: Vec<String> = Vec::new();
        let mut expanded_extra: Vec<LabelId> = Vec::new();
        for s in non_corpus {
            let a_holds = floor_a.store_rule(&s).holds;
            let b_holds = floor_b.store_rule(&s).holds;
            let extra: Vec<LabelId> = b_holds
                .ids()
                .iter()
                .filter(|id| !a_holds.contains(id))
                .cloned()
                .collect();
            if !extra.is_empty() {
                expanded_stores.push(format!("`{}`", s.as_str()));
                for id in extra {
                    if !expanded_extra.contains(&id) {
                        expanded_extra.push(id);
                    }
                }
            }
        }
        if !expanded_stores.is_empty() {
            b_parts.push(format!(
                "the stores {} also hold {}",
                join_prose(&expanded_stores),
                format_label_list(&expanded_extra),
            ));
        }

        for spec in crate::egress::DEST_KINDS {
            let Some(a_row) = floor_a.destinations.iter().find(|d| d.kind == spec.kind) else {
                continue;
            };
            let Some(b_row) = floor_b.destinations.iter().find(|d| d.kind == spec.kind) else {
                continue;
            };
            if a_row.allow == b_row.allow {
                continue;
            }
            if b_row.allow.is_empty() {
                match spec.kind {
                    DestKind::Model => {
                        b_parts.push("no model endpoint receives anything".to_owned());
                    }
                    DestKind::Export => b_parts.push("exports are refused".to_owned()),
                    other => b_parts.push(format!(
                        "{} receives nothing",
                        floor_dest_phrase(other, false)
                    )),
                }
            } else {
                let extra: Vec<LabelId> = b_row
                    .allow
                    .ids()
                    .iter()
                    .filter(|id| !a_row.allow.contains(id))
                    .cloned()
                    .collect();
                if spec.kind == DestKind::Backup && !extra.is_empty() {
                    b_parts.push(format!(
                        "a backup to a folder the owner recorded as company storage may also hold {}",
                        format_label_list(&extra),
                    ));
                } else {
                    b_parts.push(format!(
                        "{} receives {}",
                        floor_dest_phrase(spec.kind, false),
                        format_label_list(b_row.allow.ids()),
                    ));
                }
            }
        }

        let b_clause = format!(
            "On a machine that holds a company store, {}.",
            join_clauses(&b_parts),
        );

        format!(
            "Until a policy file has been loaded on this machine, the floor applies. {stores_sentence} {} {b_clause} No endpoint is approved, and nothing marked leaves.",
            dest_sentences.join(" "),
        )
    }
}

fn floor_dest_phrase(kind: DestKind, capitalize: bool) -> &'static str {
    match (kind, capitalize) {
        (DestKind::Handoff, _) => "Handoff",
        (DestKind::Export, true) => "Exports",
        (DestKind::Export, false) => "exports",
        (DestKind::Backup, true) => "Backups",
        (DestKind::Backup, false) => "backups",
        (DestKind::Model, true) => "A model endpoint named in the owner's settings",
        (DestKind::Model, false) => "a model endpoint named in the owner's settings",
        (DestKind::Mcp, true) => "The tool server",
        (DestKind::Mcp, false) => "the tool server",
        (DestKind::Bridge, true) => "The chat Bridge",
        (DestKind::Bridge, false) => "the chat Bridge",
        (DestKind::Print, true) => "Print",
        (DestKind::Print, false) => "print",
        (DestKind::Clipboard, true) => "The clipboard",
        (DestKind::Clipboard, false) => "the clipboard",
        (DestKind::Audit, _) => "audit",
        (DestKind::Git, _) => "git",
        (DestKind::Edition, _) => "edition",
    }
}

fn label_prose_rank(id: &LabelId) -> (u8, &str) {
    match id.as_str() {
        "public" => (0, "public"),
        "unlabelled" => (1, "unlabelled"),
        "owner-supplied" => (2, "owner-supplied"),
        "marked" => (3, "marked"),
        other => (4, other),
    }
}

fn format_label_list(ids: &[LabelId]) -> String {
    let mut sorted: Vec<LabelId> = ids.to_vec();
    sorted.sort_by(|a, b| label_prose_rank(a).cmp(&label_prose_rank(b)));
    let tick: Vec<String> = sorted
        .iter()
        .map(|id| format!("`{}`", id.as_str()))
        .collect();
    join_prose(&tick)
}

fn join_prose<S: AsRef<str>>(items: &[S]) -> String {
    match items {
        [] => String::new(),
        [one] => one.as_ref().to_owned(),
        [first, second] => format!("{} and {}", first.as_ref(), second.as_ref()),
        [head @ .., last] => {
            let head_str = head
                .iter()
                .map(AsRef::as_ref)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{head_str} and {}", last.as_ref())
        }
    }
}

fn join_clauses(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [first, second] => format!("{first}, and {second}"),
        [head @ .., last] => {
            let head_str = head.join(", ");
            format!("{head_str}, and {last}")
        }
    }
}
