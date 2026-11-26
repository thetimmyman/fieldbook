//! Provenance labels for backup snapshots.
//!
//! **Responsibility.** Represent copied files with their kinds and effective labels,
//! and aggregate snapshot labels by exclusion.
//!
//! **Rules.** Pure: no I/O, no clock, no environment.

use std::collections::BTreeSet;

use crate::CivilDate;
use crate::labels::{
    ItemRef, LabelGraph, LabelId, LabelSet, MarkingRules, StoreId, UNLABELLED, markings_in,
};
use crate::ports::{HandoffId, Template};
use crate::schema_json::{JsonValue, SchemaToml};

/// Classification of a file copied into a backup snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackupFileKind {
    /// Validated pending write file.
    PendingWrite,
    /// Corpus or overlay page.
    Page,
    /// Source version metadata/content.
    SourceVersion,
    /// Business or company record.
    Record,
    /// Original file under sources.
    Original,
    /// Extracted text file.
    ExtractionText,
    /// Queue proposal.
    QueueProposal,
    /// Question log line.
    QuestionLogLine,
    /// Call record.
    CallRecord,
    /// Handoff pack.
    HandoffPack,
    /// Label ledger.
    LabelLedger,
    /// Validated known text-free control file (journal, locks, sync cursors, backup metadata).
    TextFreeControl,
    /// Text-bearing public Corpus support file.
    CorpusSupport,
    /// Text-bearing last backup cache.
    LastBackupCache,
    /// Text-bearing last add report.
    LastAddReport,
    /// Text-bearing settings file.
    Settings,
    /// Unclassified or stray file.
    Stray,
}

/// A file scheduled for inclusion in a backup snapshot, with its kind and resolved labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopiedFile {
    /// Relative path within the store layout.
    pub path: String,
    /// Classified kind of the file.
    pub kind: BackupFileKind,
    /// Resolved effective labels for this file.
    pub labels: LabelSet,
}

impl CopiedFile {
    /// Creates a new [`CopiedFile`].
    #[must_use]
    pub fn new(path: impl Into<String>, kind: BackupFileKind, labels: LabelSet) -> Self {
        Self {
            path: path.into(),
            kind,
            labels,
        }
    }
}

/// Computes the aggregated [`LabelSet`] for a collection of snapshot files by exclusion.
///
/// Rules:
/// - An empty selection or a selection containing only [`BackupFileKind::TextFreeControl`]
///   yields [`LabelSet::public()`].
/// - [`BackupFileKind::TextFreeControl`] files contribute no labels to a mixed selection.
/// - [`BackupFileKind::Stray`] files add `unlabelled`.
/// - A file with an empty label set adds `unlabelled`.
/// - Otherwise every id of a file's resolved set is unioned, `unlabelled` and `marked` included.
#[must_use]
pub fn snapshot_labels(files: &[CopiedFile]) -> LabelSet {
    if files.is_empty() {
        return LabelSet::public();
    }

    let all_controls = files
        .iter()
        .all(|f| f.kind == BackupFileKind::TextFreeControl);
    if all_controls {
        return LabelSet::public();
    }

    let mut ids = BTreeSet::new();

    for file in files {
        match file.kind {
            BackupFileKind::TextFreeControl => {
                // Text-free control files add nothing to the union
            }
            BackupFileKind::Stray => {
                ids.insert(UNLABELLED.clone());
            }
            _ => {
                if file.labels.ids().is_empty() {
                    ids.insert(UNLABELLED.clone());
                } else {
                    for id in file.labels.ids() {
                        ids.insert(id.clone());
                    }
                }
            }
        }
    }

    if ids.is_empty() {
        LabelSet::unlabelled()
    } else {
        LabelSet::from_ids(ids)
    }
}

/// The labels a backup copy of the question log (`state/asked/asked.csv`) carries: for every
/// row, its own labels, any marking `rules` find in its question, and the current effective
/// labels of its inputs in `graph`. A log that does not parse, or a row with inputs and no
/// `graph` to resolve them in, adds `unlabelled`; a log with no rows is `owner-supplied`.
#[must_use]
pub fn question_log_labels(
    bytes: &[u8],
    graph: Option<&dyn LabelGraph>,
    rules: &MarkingRules,
) -> LabelSet {
    let Ok(rows) = crate::asked::parse(bytes) else {
        return LabelSet::unlabelled();
    };
    let mut labels = LabelSet::owner_supplied();
    for row in &rows {
        labels = labels.union(&row.labels);
        if let Some(found) = markings_in(&row.question, rules) {
            labels = labels.union(&found);
        }
        match graph {
            Some(graph) => labels = crate::asked::resolve(&labels, &row.inputs, graph),
            None if !row.inputs.is_empty() => labels = labels.union(&LabelSet::unlabelled()),
            None => {}
        }
    }
    labels
}

const CALL_RECORD_KEYS: &[&str] = &[
    "seq",
    "adapter",
    "endpoint",
    "model",
    "template",
    "purpose",
    "labels",
    "inputs",
    "policy",
    "prompt_sha256",
    "output_sha256",
    "system",
    "prompt",
    "reply",
    "tokens_in",
    "tokens_out",
    "attempts",
    "outcome",
    "keep_days",
    "written_on",
];

fn parse_label_strings(raw: &[String]) -> Option<LabelSet> {
    if raw.is_empty() {
        return None;
    }
    let mut ids = Vec::with_capacity(raw.len());
    for item in raw {
        ids.push(LabelId::parse(item).ok()?);
    }
    let set = LabelSet::from_ids(ids);
    (!set.contains(&UNLABELLED)).then_some(set)
}

fn parse_json_inputs(value: &JsonValue) -> Option<Vec<ItemRef>> {
    let JsonValue::Array(items) = value else {
        return None;
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let JsonValue::Object(entries) = item else {
            return None;
        };
        if entries.len() != 2 {
            return None;
        }
        let mut store = None;
        let mut id = None;
        for (k, v) in entries {
            match (k.as_str(), v) {
                ("store", JsonValue::String(s)) if store.is_none() => {
                    store = Some(StoreId::from_root_name(s)?);
                }
                ("id", JsonValue::String(s)) if id.is_none() && !s.is_empty() => {
                    id = Some(s.clone());
                }
                _ => return None,
            }
        }
        out.push(ItemRef::new(store?, id?));
    }
    Some(out)
}

/// The labels a backup copy of a saved model call record (`state/ai/calls/C-nnnn.json`) carries:
/// its own recorded `labels`, any marking `rules` find in `system`, `prompt` or `reply`, and the
/// current effective labels of its `inputs` in `graph`. A record that does not match the known
/// call-record schema, omits `labels` or `inputs`, or has `inputs` with no `graph` to resolve
/// them in is `unlabelled`.
#[must_use]
pub fn call_record_labels(
    value: &JsonValue,
    graph: Option<&dyn LabelGraph>,
    rules: &MarkingRules,
) -> LabelSet {
    let Some(labels) = try_call_record_labels(value, graph, rules) else {
        return LabelSet::unlabelled();
    };
    labels
}

fn try_call_record_labels(
    value: &JsonValue,
    graph: Option<&dyn LabelGraph>,
    rules: &MarkingRules,
) -> Option<LabelSet> {
    let JsonValue::Object(entries) = value else {
        return None;
    };
    let mut seen = BTreeSet::new();
    for (k, _) in entries {
        if !CALL_RECORD_KEYS.contains(&k.as_str()) || !seen.insert(k.as_str()) {
            return None;
        }
    }
    let find = |key: &str| entries.iter().find(|(k, _)| k == key).map(|(_, v)| v);
    let non_empty_str = |key: &str| match find(key)? {
        JsonValue::String(s) if !s.is_empty() => Some(s.as_str()),
        _ => None,
    };
    let any_str = |key: &str| match find(key)? {
        JsonValue::String(s) => Some(s.as_str()),
        _ => None,
    };
    let non_neg_int = |key: &str| match find(key)? {
        JsonValue::Integer(n) if *n >= 0 => Some(*n),
        _ => None,
    };

    for key in [
        "adapter",
        "endpoint",
        "model",
        "template",
        "purpose",
        "policy",
        "prompt_sha256",
        "outcome",
    ] {
        non_empty_str(key)?;
    }
    Template::new(non_empty_str("template")?).ok()?;
    let system = any_str("system")?;
    let prompt = any_str("prompt")?;
    let reply = any_str("reply")?;
    for key in ["tokens_in", "tokens_out", "attempts"] {
        non_neg_int(key)?;
    }
    if let Some(seq) = find("seq") {
        match seq {
            JsonValue::Null => {}
            JsonValue::Integer(n) if *n >= 1 => {}
            _ => return None,
        }
    }
    if let Some(out_sha) = find("output_sha256") {
        match out_sha {
            JsonValue::Null => {}
            JsonValue::String(s) if !s.is_empty() => {}
            _ => return None,
        }
    }
    if let Some(keep_days) = find("keep_days") {
        match keep_days {
            JsonValue::Null => {}
            JsonValue::Integer(n) if *n >= 0 => {}
            _ => return None,
        }
    }
    if let Some(written_on) = find("written_on") {
        match written_on {
            JsonValue::Null => {}
            JsonValue::String(s) if CivilDate::parse(s).is_ok() => {}
            _ => return None,
        }
    }

    let JsonValue::Array(label_vals) = find("labels")? else {
        return None;
    };
    let mut raw_labels = Vec::with_capacity(label_vals.len());
    for item in label_vals {
        let JsonValue::String(s) = item else {
            return None;
        };
        raw_labels.push(s.clone());
    }
    let mut labels = parse_label_strings(&raw_labels)?;
    let inputs = parse_json_inputs(find("inputs")?)?;

    for text in [system, prompt, reply] {
        if let Some(found) = markings_in(text, rules) {
            labels = labels.union(&found);
        }
    }
    match graph {
        Some(graph) => labels = crate::asked::resolve(&labels, &inputs, graph),
        None if !inputs.is_empty() => labels = labels.union(&LabelSet::unlabelled()),
        None => {}
    }
    Some(labels)
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BackupInputToml {
    store: String,
    id: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BackupTicketToml {
    ticket_version: u32,
    id: String,
    pack_seq: u64,
    template: String,
    used: bool,
    labels: Vec<String>,
    context_ids: Vec<String>,
    inputs: Vec<BackupInputToml>,
    #[serde(default)]
    response_schema: Option<SchemaToml>,
}

/// The labels a backup copy of a Handoff ticket (`state/handoff/H-nnnn.toml`) or pack
/// (`state/handoff/H-nnnn.txt` paired with its companion `H-nnnn.toml`) carries: the ticket's
/// `labels`, any marking `rules` find in the ticket or pack text, and the current effective
/// labels of its `inputs` in `graph`. Missing or malformed ticket metadata, an unknown or
/// malformed `response_schema`, or non-empty `inputs` with no `graph` yields `unlabelled`.
#[must_use]
pub fn handoff_ticket_labels(
    expected_id: &str,
    ticket_bytes: &[u8],
    pack_bytes: Option<&[u8]>,
    graph: Option<&dyn LabelGraph>,
    rules: &MarkingRules,
) -> LabelSet {
    let Some(labels) =
        try_handoff_ticket_labels(expected_id, ticket_bytes, pack_bytes, graph, rules)
    else {
        return LabelSet::unlabelled();
    };
    labels
}

fn try_handoff_ticket_labels(
    expected_id: &str,
    ticket_bytes: &[u8],
    pack_bytes: Option<&[u8]>,
    graph: Option<&dyn LabelGraph>,
    rules: &MarkingRules,
) -> Option<LabelSet> {
    let handoff_id = HandoffId::parse(expected_id).ok()?;
    let ticket_text = std::str::from_utf8(ticket_bytes).ok()?;
    let ticket: BackupTicketToml = toml::from_str(ticket_text).ok()?;
    let _ = (ticket.used, &ticket.response_schema);
    if ticket.ticket_version != 1 || ticket.id != handoff_id.format() || ticket.pack_seq == 0 {
        return None;
    }
    Template::new(&ticket.template).ok()?;
    if ticket.context_ids.iter().any(String::is_empty) {
        return None;
    }
    let mut labels = parse_label_strings(&ticket.labels)?;
    let mut inputs = Vec::with_capacity(ticket.inputs.len());
    for inp in &ticket.inputs {
        if inp.id.is_empty() {
            return None;
        }
        let store = StoreId::from_root_name(&inp.store)?;
        inputs.push(ItemRef::new(store, inp.id.clone()));
    }
    if let Some(found) = markings_in(ticket_text, rules) {
        labels = labels.union(&found);
    }
    if let Some(p_bytes) = pack_bytes {
        let pack_text = std::str::from_utf8(p_bytes).ok()?;
        if let Some(found) = markings_in(pack_text, rules) {
            labels = labels.union(&found);
        }
    }
    match graph {
        Some(graph) => labels = crate::asked::resolve(&labels, &inputs, graph),
        None if !inputs.is_empty() => labels = labels.union(&LabelSet::unlabelled()),
        None => {}
    }
    Some(labels)
}
