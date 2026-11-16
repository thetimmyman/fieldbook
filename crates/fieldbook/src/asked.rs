//! The question logger (`ask-log`): one row of `asked/asked.csv` and one `ask` event for each
//! question that is not a bare id.
//!
//! **Responsibility.** [`log_question`] decides whether a question is recorded and records it:
//! it skips a bare id and a repeat, scans the question for markings, resolves the current labels
//! of the answer's inputs, refuses retention the State store cannot hold, expires old rows with
//! one `purge` event, appends the `ask` event and then writes the row. [`precheck`] runs the
//! checks that need no state, so a caller can skip opening the journal for a question that would
//! be refused anyway.
//!
//! **Rules.** A log that cannot be written never stops the caller: every failure comes back as
//! one `question not recorded: <reason>` line in [`Logged::warning`]. No row is written without
//! its event. The journal holds the question only as a hash.

use fieldbook_core::CivilDate;
use fieldbook_core::asked::{self, AskedId, AskedRow, InputGraph, Outcome, RowState};
use fieldbook_core::audit::{Count, EventPayload, Id, Sha256, Word};
use fieldbook_core::check::{Entry, Host, Options, Tree, load};
use fieldbook_core::labels::{ItemRef, LabelSet, StoreId, admit, git_guard, markings_in};
use fieldbook_core::policy::EffectivePolicy;

use crate::World;
use crate::io::asked as file;
use crate::io::journal::{Journal, read_valid_events_prefix};
use crate::io::state::{AskedMode, AskedSettings};

pub use fieldbook_core::asked::Via;

/// What a caller knows about the citations of the answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cited {
    /// Not checked (`search` has no citations).
    NotChecked,
    /// Checked, and at least one citation is broken: the row's outcome is `cite_failed`.
    Broken,
}

/// The result a question got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hits {
    /// How many results.
    pub count: u64,
    /// The ids of the top results, in rank order.
    pub top_ids: Vec<String>,
    /// The Corpus fingerprint of the files the answer read ([`fingerprint`]).
    pub fingerprint: String,
}

/// What [`log_question`] did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Logged {
    /// The id of the row written; `None` for a bare id, a repeat, mode `off` or a failure.
    pub id: Option<AskedId>,
    /// One `question not recorded: <reason>` line (without its newline) when the question was
    /// meant to be recorded and was not.
    pub warning: Option<String>,
}

impl Logged {
    fn nothing() -> Self {
        Self::default()
    }

    fn failed(reason: impl std::fmt::Display) -> Self {
        Self {
            id: None,
            warning: Some(warning(reason)),
        }
    }
}

/// The one warning line for a question that was not recorded: `question not recorded:` and
/// `reason` with its lines and runs of spaces folded into single spaces.
#[must_use]
pub fn warning(reason: impl std::fmt::Display) -> String {
    let reason = reason.to_string();
    let folded: Vec<&str> = reason.split_whitespace().collect();
    format!("question not recorded: {}", folded.join(" "))
}

/// Why a question with a line break is not recorded: a cell of the log is one line.
const LINE_BREAK: &str = "the question holds a line break, and a row of the log is one line";

/// How many result ids a row keeps.
pub const TOP_IDS: usize = 5;

/// The Corpus fingerprint of `tree`: the SHA-256 of one `<sha256 hex>  <path>` line per Corpus
/// file, sorted by path as bytes, made by the function that writes the `fingerprint` of
/// `edition.toml`.
#[must_use]
pub fn fingerprint(tree: &Tree) -> String {
    let entries: Vec<(String, String)> = tree
        .iter()
        .filter(|(path, _)| path.starts_with("corpus/"))
        .map(|(path, entry)| {
            let sha = match entry {
                Entry::Bytes(bytes) => fieldbook_core::hash::sha256_hex(bytes),
                Entry::Original { sha256, .. } => sha256.clone(),
            };
            (path.to_owned(), sha)
        })
        .collect();
    fieldbook_core::manifest::fingerprint(&entries)
}

/// The question's own labels: `labels` united with the markings `policy` finds in `query`.
fn own_labels(policy: &EffectivePolicy, labels: &LabelSet, query: &str) -> LabelSet {
    markings_in(query, &policy.marking_rules())
        .map_or_else(|| labels.clone(), |found| labels.union(&found))
}

/// Whether `labels` may be kept in the question log: the State store holds them, and the
/// Git boundary admits them at the log's path.
fn retention(world: &dyn World, policy: &EffectivePolicy, labels: &LabelSet) -> Result<(), String> {
    if let Err(refusal) = admit(labels, &policy.store_rule(&StoreId::State)) {
        return Err(format!(
            "error[{}]: the State store does not hold the question's labels",
            refusal.code.id()
        ));
    }
    let target = file::path(world.state_dir());
    let context = crate::io::root::git_context(world, &target).map_err(|err| err.to_string())?;
    if let Err(refusal) = git_guard(labels, context) {
        return Err(format!(
            "error[{}]: the question log would sit under Git",
            refusal.code.id()
        ));
    }
    Ok(())
}

/// The checks that need no state: `None` when `query` is a bare id (nothing to record), else
/// whether its own labels may be kept. A caller runs it before opening the journal, so a refused
/// question creates no state folder.
///
/// # Errors
///
/// Returns the `question not recorded` warning line when the question holds a line break or
/// retention is refused.
pub fn precheck(
    world: &dyn World,
    policy: &EffectivePolicy,
    labels: &LabelSet,
    query: &str,
) -> Option<Result<(), String>> {
    if asked::is_bare_id(query) {
        return None;
    }
    if query.contains(['\n', '\r']) {
        return Some(Err(warning(LINE_BREAK)));
    }
    Some(retention(world, policy, &own_labels(policy, labels, query)).map_err(warning))
}

/// The `asked` of the newest `ask` event in the journal that has one.
fn journal_last(world: &dyn World) -> Option<AskedId> {
    read_valid_events_prefix(world.state_dir())
        .iter()
        .rev()
        .find_map(|event| match &event.payload {
            EventPayload::Ask {
                asked: Some(id), ..
            } => AskedId::parse(id.as_str()).ok(),
            _ => None,
        })
}

/// The current labels of `own` and `inputs`, resolved in the Corpus read through the lease.
fn resolve_inputs(tree: Tree, today: CivilDate, own: &LabelSet, inputs: &[ItemRef]) -> LabelSet {
    if inputs.is_empty() {
        return own.clone();
    }
    let (model, _) = load(&Host::new(tree, today), &Options::new(false));
    asked::resolve(own, inputs, &InputGraph::new(&model))
}

/// Records one question: a row of `asked/asked.csv` and an `ask` event in `journal`.
///
/// `via` is the door, `labels` the set the question was asked under (`owner-supplied`, plus a
/// pack's labels), `inputs` the store-qualified items the answer rests on, `hits` the result
/// and `cited` what the caller found of its citations. Nothing is recorded for a bare id, a
/// repeat of the last row within a minute, or labels the State store cannot hold. In mode `hash`
/// the row keeps only the hash of the question; in mode `off` no row is written and the event has
/// no `asked` field.
#[expect(
    clippy::too_many_arguments,
    reason = "the logger's contract names each input of a question so later callers pass their own"
)]
#[must_use]
pub fn log_question(
    world: &dyn World,
    journal: &Journal<'_>,
    settings: &AskedSettings,
    policy: &EffectivePolicy,
    via: Via,
    labels: &LabelSet,
    inputs: &[ItemRef],
    query: &str,
    hits: &Hits,
    cited: Cited,
) -> Logged {
    if asked::is_bare_id(query) {
        return Logged::nothing();
    }
    if query.contains(['\n', '\r']) {
        return Logged::failed(LINE_BREAK);
    }
    let own = own_labels(policy, labels, query);
    if let Err(reason) = retention(world, policy, &own) {
        return Logged::failed(reason);
    }

    let lease = match world.write_lease() {
        Ok(lease) => lease,
        Err(err) => return Logged::failed(err),
    };
    let audited = match journal.state_scope(&lease) {
        Ok(audited) => audited,
        Err(err) => return Logged::failed(err),
    };

    let tree = match world.read_tree_under(&lease) {
        Ok(tree) => tree,
        Err(err) => return Logged::failed(err),
    };
    let resolved = resolve_inputs(tree, world.today(), &own, inputs);
    if let Err(reason) = retention(world, policy, &resolved) {
        return Logged::failed(reason);
    }

    let rows = match file::read_rows(world.state_dir()) {
        Ok(rows) => rows.unwrap_or_default(),
        Err(err) => return Logged::failed(err),
    };
    let now = world.now();
    let (mut kept, removed) = asked::expire(rows, now, settings.keep_days);
    if !removed.is_empty() {
        let purge = EventPayload::Purge {
            what: Word::new_static("asked"),
            count: Count::new(removed.len() as u64),
        };
        if let Err(err) = journal.append_payload(purge) {
            return Logged::failed(err);
        }
        if let Err(err) = file::write_rows(world, &audited, &kept) {
            return Logged::failed(err);
        }
    }

    let mut inputs: Vec<ItemRef> = inputs.to_vec();
    inputs.sort();
    inputs.dedup();
    if settings.mode != AskedMode::Off
        && let Some(last) = kept.last()
        && asked::is_repeat(last, query, via, &own, &inputs, now)
    {
        return Logged::nothing();
    }

    let outcome = match cited {
        Cited::Broken => Outcome::CiteFailed,
        Cited::NotChecked if hits.count == 0 => Outcome::NoResult,
        Cited::NotChecked => Outcome::Answered,
    };
    let event = |asked: Option<Id>| EventPayload::Ask {
        asked,
        asked_via: via,
        question_sha256: Sha256::digest(query.as_bytes()),
        result_count: Count::new(hits.count),
        outcome: Word::new_static(outcome.as_str()),
        labels: resolved.clone(),
    };

    if settings.mode == AskedMode::Off {
        return match journal.append_payload(event(None)) {
            Ok(_) => Logged::nothing(),
            Err(err) => Logged::failed(err),
        };
    }

    let id = match asked::next_id(&kept, journal_last(world)) {
        Ok(id) => id,
        Err(exhausted) => return Logged::failed(exhausted),
    };
    let journal_id = match Id::parse(&id.to_string()) {
        Ok(journal_id) => journal_id,
        Err(err) => return Logged::failed(err),
    };
    if let Err(err) = journal.append_payload(event(Some(journal_id))) {
        return Logged::failed(err);
    }

    kept.push(AskedRow {
        id,
        at: now,
        via,
        question: if settings.mode == AskedMode::Hash {
            asked::hashed(query)
        } else {
            query.to_owned()
        },
        labels: own,
        inputs,
        by: world.principal(),
        tool_version: env!("CARGO_PKG_VERSION").to_owned(),
        fingerprint: hits.fingerprint.clone(),
        result_count: hits.count,
        top_ids: hits.top_ids.clone(),
        outcome,
        state: RowState::Open,
    });
    match file::write_rows(world, &audited, &kept) {
        Ok(()) => Logged {
            id: Some(id),
            warning: None,
        },
        Err(err) => Logged::failed(err),
    }
}
