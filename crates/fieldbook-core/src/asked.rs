//! The question log (`asked/asked.csv` in the state folder): its rows, their CSV form and the
//! pure rules the logger applies (`ask-log`).
//!
//! **Responsibility.** [`AskedRow`] and its columns, [`parse`] and [`render`] over the CSV rules
//! of [`crate::schema::table`], the next row id ([`next_id`]), what is not logged
//! ([`is_bare_id`], [`is_repeat`]), expiry ([`expire`]), the hashed question ([`hashed`]), the
//! `status` counts ([`summary`]), the one allowed outcome change ([`with_outcome`]) and the
//! current labels of a row's inputs ([`InputGraph`], [`resolve`]).
//!
//! **Rules.** Pure: no I/O and no clock. The caller passes the time in.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::Deserialize;

use crate::audit::{Now, Principal};
use crate::check::Model;
use crate::ids::{
    AdrId, CaseId, ClaimId, ConflictId, EditionId, PageId, QuestionId, ReleaseId, SourceId,
    VersionId, is_error_code,
};
use crate::labels::{
    ItemRef, LabelGraph, LabelId, LabelNode, LabelSet, Ledger, StoreId, effective_item,
};
use crate::schema::table;

/// The door a question came through, written as its lowercase word in the `via` column.
pub use crate::audit::AskedVia as Via;

/// The columns of `asked/asked.csv`, in order.
pub const HEADER: [&str; 13] = [
    "id",
    "at",
    "via",
    "question",
    "labels",
    "inputs",
    "by",
    "tool_version",
    "fingerprint",
    "result_count",
    "top_ids",
    "outcome",
    "state",
];

/// A repeat of the last question within this many seconds is not a new row.
pub const REPEAT_SECONDS: i64 = 60;

/// The highest row number an [`AskedId`] can hold (`A-9999`).
pub const MAX_ID: u16 = 9999;

/// The id of one row: `A-` and four digits, from `A-0001` to `A-9999`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AskedId(u16);

impl AskedId {
    /// The id with number `n`, or `None` outside `1..=9999`.
    #[must_use]
    pub const fn new(n: u16) -> Option<Self> {
        if n >= 1 && n <= MAX_ID {
            Some(Self(n))
        } else {
            None
        }
    }

    /// The row number.
    #[must_use]
    pub const fn number(self) -> u16 {
        self.0
    }

    /// Parses `A-nnnn`.
    ///
    /// # Errors
    ///
    /// Returns [`AskedError`] when `text` is not `A-` and four digits from `0001`.
    pub fn parse(text: &str) -> Result<Self, AskedError> {
        let bad = || AskedError::new(format!("not an asked id: `{text}`"));
        let digits = text.strip_prefix("A-").ok_or_else(bad)?;
        if digits.len() != 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad());
        }
        digits
            .parse::<u16>()
            .ok()
            .and_then(Self::new)
            .ok_or_else(bad)
    }
}

impl fmt::Display for AskedId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "A-{:04}", self.0)
    }
}

/// The id space is used up: the next id would be above `A-9999`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exhausted;

impl fmt::Display for Exhausted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("id space exhausted")
    }
}

impl std::error::Error for Exhausted {}

/// What a question got on the day it was asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Outcome {
    /// At least one result.
    Answered,
    /// No result.
    NoResult,
    /// A caller found a broken citation in the answer.
    CiteFailed,
    /// The owner flagged the answer.
    Flagged,
}

impl Outcome {
    /// The column word.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Answered => "answered",
            Self::NoResult => "no_result",
            Self::CiteFailed => "cite_failed",
            Self::Flagged => "flagged",
        }
    }

    /// Parses the column word.
    ///
    /// # Errors
    ///
    /// Returns [`AskedError`] for any other word.
    pub fn parse(text: &str) -> Result<Self, AskedError> {
        match text {
            "answered" => Ok(Self::Answered),
            "no_result" => Ok(Self::NoResult),
            "cite_failed" => Ok(Self::CiteFailed),
            "flagged" => Ok(Self::Flagged),
            _ => Err(AskedError::new(format!("not an outcome: `{text}`"))),
        }
    }
}

/// Where the owner has taken a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RowState {
    /// Not yet looked at.
    Open,
    /// Turned into a regression case or a gap question.
    Promoted,
    /// Looked at and set aside.
    Dismissed,
}

impl RowState {
    /// The column word.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Promoted => "promoted",
            Self::Dismissed => "dismissed",
        }
    }

    /// Parses the column word.
    ///
    /// # Errors
    ///
    /// Returns [`AskedError`] for any other word.
    pub fn parse(text: &str) -> Result<Self, AskedError> {
        match text {
            "open" => Ok(Self::Open),
            "promoted" => Ok(Self::Promoted),
            "dismissed" => Ok(Self::Dismissed),
            _ => Err(AskedError::new(format!("not a row state: `{text}`"))),
        }
    }
}

/// One row of `asked/asked.csv`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskedRow {
    /// `A-nnnn`.
    pub id: AskedId,
    /// When it was asked (UTC).
    pub at: Now,
    /// The door it came through.
    pub via: Via,
    /// The question text, or its [`hashed`] form in mode `hash`.
    pub question: String,
    /// The labels it was asked under: its own scanned labels and the request labels.
    pub labels: LabelSet,
    /// The store-qualified items the answer rested on, sorted and unique.
    pub inputs: Vec<ItemRef>,
    /// The principal that asked.
    pub by: Principal,
    /// The tool version that answered.
    pub tool_version: String,
    /// The Corpus fingerprint of the files the answer read.
    pub fingerprint: String,
    /// How many results it got.
    pub result_count: u64,
    /// The ids of the top results, in rank order.
    pub top_ids: Vec<String>,
    /// What it got.
    pub outcome: Outcome,
    /// Where the owner has taken it.
    pub state: RowState,
}

/// A question log that does not parse, or a cell that is not valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskedError {
    /// The 1-based line, or 0 when the error is not tied to a line.
    pub line: u32,
    /// What is wrong.
    pub message: String,
}

impl AskedError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            line: 0,
            message: message.into(),
        }
    }

    const fn at(mut self, line: u32) -> Self {
        self.line = line;
        self
    }
}

impl fmt::Display for AskedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            f.write_str(&self.message)
        } else {
            write!(f, "line {}: {}", self.line, self.message)
        }
    }
}

impl std::error::Error for AskedError {}

// --- CSV --------------------------------------------------------------------------------------

/// Parses `asked/asked.csv`: the exact [`HEADER`] and then one row per line.
///
/// # Errors
///
/// Returns [`AskedError`] for bytes that are not CSV, a header that is not [`HEADER`] (an
/// unknown, missing or moved column), a row with the wrong number of cells, or a cell that is
/// not valid, a missing or malformed `inputs` cell included.
pub fn parse(bytes: &[u8]) -> Result<Vec<AskedRow>, AskedError> {
    let parsed = table::read(bytes)
        .map_err(|err| AskedError::new(err.message().to_owned()).at(err.line()))?;
    if parsed.header.cells.iter().map(String::as_str).ne(HEADER) {
        let unknown = parsed
            .header
            .cells
            .iter()
            .find(|cell| !HEADER.contains(&cell.as_str()));
        let message = unknown.map_or_else(
            || "the header is not the question-log header".to_owned(),
            |cell| format!("unknown column `{cell}`"),
        );
        return Err(AskedError::new(message).at(parsed.header.line));
    }
    parsed
        .rows
        .iter()
        .map(|row| parse_row(&row.cells).map_err(|err| err.at(row.line)))
        .collect()
}

fn parse_row(cells: &[String]) -> Result<AskedRow, AskedError> {
    let [
        id,
        at,
        via,
        question,
        labels,
        inputs,
        by,
        tool_version,
        fingerprint,
        result_count,
        top_ids,
        outcome,
        state,
    ] = cells
    else {
        return Err(AskedError::new(format!(
            "expected {} cells, found {}",
            HEADER.len(),
            cells.len()
        )));
    };
    let at = Now::parse(at).map_err(|err| AskedError::new(err.to_string()))?;
    let via = Via::parse(via).map_err(|err| AskedError::new(err.to_string()))?;
    let labels = LabelSet::parse_cell(labels).map_err(|err| AskedError::new(err.to_string()))?;
    let by = Principal::parse(by).map_err(|err| AskedError::new(err.to_string()))?;
    if fingerprint.len() != 64 || !crate::manifest::is_lower_hex(fingerprint, 64) {
        return Err(AskedError::new(format!(
            "not a fingerprint: `{fingerprint}`"
        )));
    }
    if tool_version.is_empty() {
        return Err(AskedError::new("empty tool_version"));
    }
    let result_count = result_count
        .parse::<u64>()
        .map_err(|_| AskedError::new(format!("not a count: `{result_count}`")))?;
    let top_ids = if top_ids.is_empty() {
        Vec::new()
    } else {
        top_ids.split('|').map(str::to_owned).collect::<Vec<_>>()
    };
    if top_ids.iter().any(String::is_empty) {
        return Err(AskedError::new("an empty id in top_ids"));
    }
    Ok(AskedRow {
        id: AskedId::parse(id)?,
        at,
        via,
        question: question.clone(),
        labels,
        inputs: parse_inputs(inputs)?,
        by,
        tool_version: tool_version.clone(),
        fingerprint: fingerprint.clone(),
        result_count,
        top_ids,
        outcome: Outcome::parse(outcome)?,
        state: RowState::parse(state)?,
    })
}

/// Writes `rows` under [`HEADER`] with the quoting of [`table::write`], and also quotes a cell
/// that holds a line break, so a question typed over two lines stays one cell.
#[must_use]
pub fn render(rows: &[AskedRow]) -> String {
    let mut out = String::new();
    push_csv_row(&mut out, HEADER.iter().copied());
    for row in rows {
        let cells = [
            row.id.to_string(),
            row.at.to_string(),
            row.via.as_str().to_owned(),
            row.question.clone(),
            row.labels.to_cell(),
            render_inputs(&row.inputs),
            row.by.to_string(),
            row.tool_version.clone(),
            row.fingerprint.clone(),
            row.result_count.to_string(),
            row.top_ids.join("|"),
            row.outcome.as_str().to_owned(),
            row.state.as_str().to_owned(),
        ];
        push_csv_row(&mut out, cells.iter().map(String::as_str));
    }
    out
}

fn push_csv_row<'a>(out: &mut String, cells: impl Iterator<Item = &'a str>) {
    for (idx, cell) in cells.enumerate() {
        if idx > 0 {
            out.push(',');
        }
        if cell.contains([',', '"', '\n', '\r']) {
            out.push('"');
            out.push_str(&cell.replace('"', "\"\""));
            out.push('"');
        } else {
            out.push_str(cell);
        }
    }
    out.push('\n');
}

/// The `inputs` cell: a single-line TOML array of inline tables, each with `store` then `id`,
/// sorted by store and id with no repeats. An empty list is `[]`.
#[must_use]
pub fn render_inputs(inputs: &[ItemRef]) -> String {
    let unique: BTreeSet<&ItemRef> = inputs.iter().collect();
    let parts: Vec<String> = unique
        .iter()
        .map(|item| {
            format!(
                "{{ store = {}, id = {} }}",
                toml_string(item.store.as_str()),
                toml_string(&item.id)
            )
        })
        .collect();
    format!("[{}]", parts.join(", "))
}

/// A TOML basic string holding `text`, every control character escaped.
fn toml_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputsCell {
    v: Vec<InputPair>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputPair {
    store: StoreId,
    id: String,
}

/// Parses an `inputs` cell written by [`render_inputs`]. The pairs come back sorted and unique.
///
/// # Errors
///
/// Returns [`AskedError`] for an empty cell, a cell over more than one line, or one that is not
/// an array of `{ store, id }` tables with a known store and a non-empty id.
pub fn parse_inputs(cell: &str) -> Result<Vec<ItemRef>, AskedError> {
    if cell.trim().is_empty() {
        return Err(AskedError::new("missing inputs"));
    }
    if cell.contains(['\n', '\r']) {
        return Err(AskedError::new("inputs must be on one line"));
    }
    let parsed: InputsCell = toml::from_str(&format!("v = {cell}"))
        .map_err(|err| AskedError::new(format!("malformed inputs: {}", err.message())))?;
    let mut items = BTreeSet::new();
    for pair in parsed.v {
        if pair.id.is_empty() {
            return Err(AskedError::new("an empty id in inputs"));
        }
        items.insert(ItemRef::new(pair.store, pair.id));
    }
    Ok(items.into_iter().collect())
}

// --- Rules ------------------------------------------------------------------------------------

/// The id after the larger of the highest id in `rows` and `journal_last`, the `asked` of the
/// newest `ask` event that has one. The journal is never rewritten, so an id is never reused
/// after rows expire or the log is deleted.
///
/// # Errors
///
/// Returns [`Exhausted`] when that id would be above `A-9999`.
pub fn next_id(rows: &[AskedRow], journal_last: Option<AskedId>) -> Result<AskedId, Exhausted> {
    let highest = rows
        .iter()
        .map(|row| row.id.number())
        .chain(journal_last.map(AskedId::number))
        .max()
        .unwrap_or(0);
    highest
        .checked_add(1)
        .and_then(AskedId::new)
        .ok_or(Exhausted)
}

/// Whether the whole of `query` (trimmed) is one Corpus id of the
/// [Identifiers](../../../docs/reference/corpus-format.md#identifiers) grammar, or a local
/// `A-`/`P-` id: a lookup, not a question. Area codes and terms are not counted, because their
/// shapes are ordinary lowercase words.
#[must_use]
pub fn is_bare_id(query: &str) -> bool {
    let text = query.trim();
    if text.is_empty() || text.chars().any(char::is_whitespace) {
        return false;
    }
    PageId::parse(text).is_ok()
        || SourceId::parse(text).is_ok()
        || VersionId::parse(text).is_ok()
        || ClaimId::parse(text).is_ok()
        || QuestionId::parse(text).is_ok()
        || ConflictId::parse(text).is_ok()
        || CaseId::parse(text).is_ok()
        || AdrId::parse(text).is_ok()
        || EditionId::parse(text).is_ok()
        || ReleaseId::parse(text).is_ok()
        || is_error_code(text)
        || is_local_id(text)
}

fn is_local_id(text: &str) -> bool {
    let Some((prefix, digits)) = text.split_once('-') else {
        return false;
    };
    matches!(prefix, "A" | "P")
        && digits.len() == 4
        && digits.bytes().all(|b| b.is_ascii_digit())
        && digits != "0000"
}

/// Seconds from `from` to `to` (negative when `to` is earlier).
fn seconds_between(from: Now, to: Now) -> i64 {
    from.date.days_between(to.date) * 86_400 + i64::from(to.seconds) - i64::from(from.seconds)
}

/// Whether `query` repeats `last`: the same text (or its hash, in mode `hash`), via, labels and
/// input pairs, asked at most [`REPEAT_SECONDS`] after it.
#[must_use]
pub fn is_repeat(
    last: &AskedRow,
    query: &str,
    via: Via,
    labels: &LabelSet,
    inputs: &[ItemRef],
    now: Now,
) -> bool {
    let unique: Vec<ItemRef> = inputs
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let gap = seconds_between(last.at, now);
    (last.question == query || last.question == hashed(query))
        && last.via == via
        && last.labels == *labels
        && last.inputs == unique
        && (0..=REPEAT_SECONDS).contains(&gap)
}

/// Splits `rows` into the rows to keep and the rows older than `keep_days` whole days on `now`.
#[must_use]
pub fn expire(rows: Vec<AskedRow>, now: Now, keep_days: u32) -> (Vec<AskedRow>, Vec<AskedRow>) {
    rows.into_iter()
        .partition(|row| row.at.date.days_between(now.date) <= i64::from(keep_days))
}

/// `sha256:` and the 64 lowercase hex digits of the SHA-256 of `question`.
#[must_use]
pub fn hashed(question: &str) -> String {
    format!("sha256:{}", crate::hash::sha256_hex(question.as_bytes()))
}

/// The counts `status` prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AskedSummary {
    /// Rows in the log.
    pub asked: usize,
    /// Rows whose outcome is not `answered`.
    pub misses: usize,
    /// Rows in state `promoted`.
    pub promoted: usize,
}

impl fmt::Display for AskedSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} questions asked, {} misses, {} promoted",
            self.asked, self.misses, self.promoted
        )
    }
}

/// Counts `rows` for `status`.
#[must_use]
pub fn summary(rows: &[AskedRow]) -> AskedSummary {
    AskedSummary {
        asked: rows.len(),
        misses: rows
            .iter()
            .filter(|row| row.outcome != Outcome::Answered)
            .count(),
        promoted: rows
            .iter()
            .filter(|row| row.state == RowState::Promoted)
            .count(),
    }
}

/// `rows` with the outcome of row `id` changed from `answered` to `to` (`cite_failed` or
/// `flagged`), every other cell kept. `None` for an unknown id, a row whose outcome is not
/// `answered` (so the first-run result of a miss is never overwritten), or any other `to`.
#[must_use]
pub fn with_outcome(rows: &[AskedRow], id: AskedId, to: Outcome) -> Option<Vec<AskedRow>> {
    if !matches!(to, Outcome::CiteFailed | Outcome::Flagged) {
        return None;
    }
    let position = rows.iter().position(|row| row.id == id)?;
    let mut next = rows.to_vec();
    let row = next.get_mut(position)?;
    if row.outcome != Outcome::Answered {
        return None;
    }
    row.outcome = to;
    Some(next)
}

// --- Current labels of the inputs ---------------------------------------------------------------

/// The provenance graph of Corpus items as a question's inputs name them, over one loaded
/// [`Model`]: a source or source version carries its `handling`; a claim is `public` and rests on
/// its source version; a page carries its `handling`; a question, conflict or term is `public`.
/// A section (`page#anchor`) or a text window (`version#window`) is the item before the `#`. Any
/// other item has no node, which counts as `unlabelled`.
pub struct InputGraph<'a> {
    model: &'a Model,
    ledgers: BTreeMap<StoreId, Ledger>,
    empty: Ledger,
}

impl<'a> InputGraph<'a> {
    /// The graph of `model`, with the ledgers its tree holds (`<store>/labels.csv`).
    #[must_use]
    pub fn new(model: &'a Model) -> Self {
        let mut ledgers = BTreeMap::new();
        for store in StoreId::ALL {
            let rel = format!("{}/labels.csv", store.as_str());
            let ledger = match model.tree().get_bytes(&rel) {
                Some(bytes) => match std::str::from_utf8(bytes) {
                    Ok(text) => Ledger::parse(text).unwrap_or_else(Ledger::broken),
                    Err(_) => Ledger::parse("\0").unwrap_or_else(Ledger::broken),
                },
                None => Ledger::empty(),
            };
            ledgers.insert(store, ledger);
        }
        Self {
            model,
            ledgers,
            empty: Ledger::default(),
        }
    }
}

fn handling(items: &[String]) -> LabelSet {
    if items.is_empty() {
        return LabelSet::unlabelled();
    }
    let mut ids = Vec::with_capacity(items.len());
    for item in items {
        let Ok(id) = LabelId::parse(item) else {
            return LabelSet::unlabelled();
        };
        ids.push(id);
    }
    LabelSet::from_ids(ids)
}

impl LabelGraph for InputGraph<'_> {
    fn node(&self, item: &ItemRef) -> Option<LabelNode> {
        if item.store != StoreId::Corpus {
            return None;
        }
        // A section of a page (`P-0001#anchor`) and a text window of a version
        // (`S-0001-v1#w2`) rest on the item before the `#`: the fragment is a node of its own
        // whose one input is that base item, so the base item's ledger entries reach it.
        if let Some((base, _)) = item.id.split_once('#') {
            let base = ItemRef::new(StoreId::Corpus, base.to_owned());
            self.node(&base)?;
            return Some(LabelNode::new(LabelSet::public(), vec![base]));
        }
        let id = item.id.as_str();
        let model = self.model;

        let versions: Vec<LabelSet> = model
            .sources()
            .iter()
            .filter(|s| {
                s.value.source_id.as_str() == id
                    || format!("{}-v{}", s.value.source_id, s.value.version) == id
            })
            .map(|s| handling(&s.value.handling))
            .collect();
        if let Some(first) = versions.first() {
            let own = versions
                .iter()
                .fold(first.clone(), |acc, set| acc.union(set));
            return Some(LabelNode::new(own, Vec::new()));
        }

        if let Some(claim) = model
            .claims()
            .iter()
            .find(|c| c.value.claim_id.as_str() == id)
        {
            let inputs = match (&claim.value.source_id, claim.value.version) {
                (Some(source), Some(version)) => {
                    vec![ItemRef::new(
                        StoreId::Corpus,
                        format!("{source}-v{version}"),
                    )]
                }
                _ => Vec::new(),
            };
            return Some(LabelNode::new(LabelSet::public(), inputs));
        }

        if let Some(page) = model.pages().iter().find(|p| p.id.as_str() == id) {
            return Some(LabelNode::new(handling(&page.meta.handling), Vec::new()));
        }

        let is_record = model
            .questions()
            .iter()
            .any(|q| q.value.question_id.as_str() == id)
            || model
                .conflicts()
                .iter()
                .any(|c| c.value.conflict_id.as_str() == id)
            || model
                .glossary()
                .iter()
                .any(|t| t.value.term_id.as_str() == id);
        is_record.then(|| LabelNode::new(LabelSet::public(), Vec::new()))
    }

    fn ledger(&self, store: StoreId) -> &Ledger {
        self.ledgers.get(&store).unwrap_or(&self.empty)
    }
}

/// `own` united with the current effective labels of every input in `graph`. An input with no
/// node adds `unlabelled`.
#[must_use]
pub fn resolve(own: &LabelSet, inputs: &[ItemRef], graph: &dyn LabelGraph) -> LabelSet {
    inputs.iter().fold(own.clone(), |acc, item| {
        acc.union(&effective_item(graph, item))
    })
}
