//! Append-only label ledger (`labels.csv`, `Ledger`, `effective`)
//! (`docs/reference/company-store.md#the-label-ledger`, `FB0412`).

use std::fmt;
use std::ops::Deref;

use serde::{Deserialize, Serialize};

use crate::check::Code;

use super::id::{LabelId, UNLABELLED};
use super::set::LabelSet;

/// Required CSV header of `labels.csv`.
pub const LEDGER_HEADER: &str = "seq,item_id,kind,labels,origin";

/// Kind of a ledger line in version 1 (`add`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LedgerKind {
    /// Adds labels to an item (`"add"`).
    Add,
}

impl LedgerKind {
    /// Returns the wire spelling (`"add"`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "add" => Some(Self::Add),
            _ => None,
        }
    }
}

impl fmt::Display for LedgerKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Origin of a ledger line (`person`, `reapply`, or `intake`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LedgerOrigin {
    /// Added interactively by a person via `label add` (`"person"`).
    Person,
    /// Added by `label reapply` when a marking is mapped (`"reapply"`).
    Reapply,
    /// Added as a correction while a batch was staged (`"intake"`).
    Intake,
}

impl LedgerOrigin {
    /// Returns the wire spelling (`"person"`, `"reapply"`, `"intake"`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Reapply => "reapply",
            Self::Intake => "intake",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "person" => Some(Self::Person),
            "reapply" => Some(Self::Reapply),
            "intake" => Some(Self::Intake),
            _ => None,
        }
    }
}

impl fmt::Display for LedgerOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One validated row of `labels.csv`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerEntry {
    /// 1-based monotonic sequence number.
    pub seq: u64,
    /// Item identifier (`page`, source version, or record version).
    pub item_id: String,
    /// Line kind (`add` in version 1).
    pub kind: LedgerKind,
    /// Non-builtin labels added by this line.
    pub labels: LabelSet,
    /// Origin of this line (`person`, `reapply`, `intake`).
    pub origin: LedgerOrigin,
}

/// Error returned when `labels.csv` cannot be parsed (`FB0412` with 1-based line number).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerError {
    /// Always [`Code::Fb0412`].
    pub code: Code,
    /// 1-based line number where the error occurred.
    pub line: usize,
    /// Diagnostic description.
    pub message: String,
}

impl LedgerError {
    fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            code: Code::Fb0412,
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for LedgerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} (line {}): {}",
            self.code.id(),
            self.line,
            self.message
        )
    }
}

impl std::error::Error for LedgerError {}

/// Planned update to `labels.csv` returned by [`Ledger::plan_add`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerPlan {
    /// Complete new CSV text of `labels.csv` (including the header and all lines).
    pub text: String,
    /// Newly appended ledger entries.
    pub added: Vec<LedgerEntry>,
}

impl LedgerPlan {
    /// Returns the planned CSV file text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }
}

impl Deref for LedgerPlan {
    type Target = str;

    fn deref(&self) -> &str {
        &self.text
    }
}

impl AsRef<str> for LedgerPlan {
    fn as_ref(&self) -> &str {
        &self.text
    }
}

/// The parsed label ledger of a store, or its broken state (`FB0412`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Ledger {
    entries: Vec<LedgerEntry>,
    broken: Option<LedgerError>,
}

impl Ledger {
    /// Returns an empty, valid ledger (for a store with no `labels.csv` yet).
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            entries: Vec::new(),
            broken: None,
        }
    }

    /// Constructs a broken ledger state from a [`LedgerError`].
    ///
    /// Every item in a store with a broken ledger is treated as unresolved by [`effective`].
    #[must_use]
    pub const fn broken(err: LedgerError) -> Self {
        Self {
            entries: Vec::new(),
            broken: Some(err),
        }
    }

    /// Strictly parses `labels.csv` text.
    ///
    /// - Line 1 must be `seq,item_id,kind,labels,origin`.
    /// - `seq` starts at 1 and increments by 1 on each line.
    /// - `kind` must be `add`.
    /// - `labels` must parse via [`LabelSet::parse_cell`] and contain no built-in label
    ///   (`public`, `unlabelled`, `marked`, or `owner-supplied`).
    /// - `origin` must be `person`, `reapply`, or `intake`.
    pub fn parse(text: &str) -> Result<Self, LedgerError> {
        if text.is_empty() {
            return Err(LedgerError::new(1, "empty label ledger: missing header"));
        }

        let raw_lines: Vec<&str> = text.split('\n').collect();
        let line_count = if raw_lines.last() == Some(&"") {
            raw_lines.len() - 1
        } else {
            raw_lines.len()
        };

        if line_count == 0 {
            return Err(LedgerError::new(1, "empty label ledger: missing header"));
        }

        let header = raw_lines[0].strip_suffix('\r').unwrap_or(raw_lines[0]);
        if header != LEDGER_HEADER {
            return Err(LedgerError::new(
                1,
                format!("invalid ledger header: expected `{LEDGER_HEADER}`"),
            ));
        }

        let mut entries = Vec::with_capacity(line_count.saturating_sub(1));

        for (idx, &raw_line) in raw_lines[1..line_count].iter().enumerate() {
            let line_no = idx + 2;
            let expected_seq = (idx as u64) + 1;
            let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
            if line.is_empty() {
                return Err(LedgerError::new(line_no, "empty line in label ledger"));
            }
            if line.chars().any(char::is_control) {
                return Err(LedgerError::new(
                    line_no,
                    "control character in label ledger",
                ));
            }

            let fields: Vec<&str> = line.split(',').collect();
            if fields.len() != 5 {
                return Err(LedgerError::new(
                    line_no,
                    format!("expected 5 columns, found {}", fields.len()),
                ));
            }

            let seq_str = fields[0];
            if seq_str.is_empty()
                || seq_str.starts_with('0')
                || !seq_str.bytes().all(|b| b.is_ascii_digit())
            {
                return Err(LedgerError::new(
                    line_no,
                    format!("invalid seq `{seq_str}`"),
                ));
            }
            let seq: u64 = seq_str
                .parse()
                .map_err(|_| LedgerError::new(line_no, format!("invalid seq `{seq_str}`")))?;
            if seq != expected_seq {
                return Err(LedgerError::new(
                    line_no,
                    format!("expected seq {expected_seq}, found {seq}"),
                ));
            }

            let item_id = fields[1];
            if item_id.is_empty() || item_id.chars().any(char::is_whitespace) {
                return Err(LedgerError::new(line_no, "invalid item_id in label ledger"));
            }

            let kind = LedgerKind::parse(fields[2]).ok_or_else(|| {
                LedgerError::new(line_no, format!("unknown ledger kind `{}`", fields[2]))
            })?;

            let labels = LabelSet::parse_cell(fields[3]).map_err(|e| {
                LedgerError::new(line_no, format!("invalid labels cell `{}`: {e}", fields[3]))
            })?;
            if labels.ids().iter().any(LabelId::is_builtin) {
                return Err(LedgerError::new(
                    line_no,
                    format!(
                        "ledger line may not add built-in label in `{}`",
                        labels.to_cell()
                    ),
                ));
            }

            let origin = LedgerOrigin::parse(fields[4]).ok_or_else(|| {
                LedgerError::new(line_no, format!("unknown ledger origin `{}`", fields[4]))
            })?;

            entries.push(LedgerEntry {
                seq,
                item_id: item_id.to_owned(),
                kind,
                labels,
                origin,
            });
        }

        Ok(Self {
            entries,
            broken: None,
        })
    }

    /// Returns `true` when this ledger is in a broken state (`FB0412`).
    #[must_use]
    pub const fn is_broken(&self) -> bool {
        self.broken.is_some()
    }

    /// Returns the [`LedgerError`] if this ledger is broken.
    #[must_use]
    pub const fn error(&self) -> Option<&LedgerError> {
        self.broken.as_ref()
    }

    /// Returns the parsed ledger entries in sequence order.
    #[must_use]
    pub fn entries(&self) -> &[LedgerEntry] {
        &self.entries
    }

    /// Renders this ledger as CSV text (`seq,item_id,kind,labels,origin\n...`).
    #[must_use]
    pub fn to_csv(&self) -> String {
        let mut out = String::from(LEDGER_HEADER);
        out.push('\n');
        for entry in &self.entries {
            out.push_str(&format!(
                "{},{},{},{},{}\n",
                entry.seq,
                entry.item_id,
                entry.kind.as_str(),
                entry.labels.to_cell(),
                entry.origin.as_str()
            ));
        }
        out
    }

    /// Plans appending one `add` line for each id in `ids` with `labels` and `origin`.
    ///
    /// Writes the header when the ledger is new/empty, retains all existing lines, and appends
    /// the new lines continuing `seq`. Writes nothing to disk.
    #[must_use]
    pub fn plan_add<I, S>(&self, ids: I, labels: &LabelSet, origin: LedgerOrigin) -> LedgerPlan
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut text = self.to_csv();
        let start_seq = self.entries.last().map_or(1, |last| last.seq + 1);
        let mut added = Vec::new();
        for (offset, id) in ids.into_iter().enumerate() {
            let next_seq = start_seq + (offset as u64);
            let item_id = id.as_ref().to_owned();
            let entry = LedgerEntry {
                seq: next_seq,
                item_id,
                kind: LedgerKind::Add,
                labels: labels.clone(),
                origin,
            };
            text.push_str(&format!(
                "{},{},{},{},{}\n",
                entry.seq,
                entry.item_id,
                entry.kind.as_str(),
                entry.labels.to_cell(),
                entry.origin.as_str()
            ));
            added.push(entry);
        }
        LedgerPlan { text, added }
    }
}

/// Computes the effective own labels of item `id` from its initial `cell` and the store's `ledger`.
///
/// - If `ledger` is broken, every item is treated as unresolved (`cell.with(UNLABELLED)`).
/// - Otherwise, applies every `add` line for `id` in `seq` order:
///   - `person` and `intake` lines go through [`LabelSet::classified`], removing `unlabelled` from
///     this item while preserving `marked`.
///   - `reapply` lines go through [`LabelSet::reapplied`], replacing `marked` with the line's labels.
#[must_use]
pub fn effective(cell: &LabelSet, id: &str, ledger: &Ledger) -> LabelSet {
    if ledger.is_broken() {
        return cell.with(UNLABELLED);
    }

    let mut current = cell.clone();
    for entry in &ledger.entries {
        if entry.item_id != id {
            continue;
        }
        match entry.origin {
            LedgerOrigin::Person | LedgerOrigin::Intake => {
                if let Some(next) = current.classified(&entry.labels) {
                    current = next;
                }
            }
            LedgerOrigin::Reapply => {
                if let Some(next) = current.reapplied(&entry.labels) {
                    current = next;
                }
            }
        }
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> LabelId {
        LabelId::parse(s).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn ledger_round_trip() {
        let csv = "\
seq,item_id,kind,labels,origin
1,SO-000042-v1,add,np-0007,person
2,CM-000012-v1,add,bid-0003|np-0007,intake
3,SO-000042-v1,add,ctl-0002,reapply
";
        let ledger = Ledger::parse(csv).unwrap();
        assert_eq!(ledger.entries().len(), 3);
        assert_eq!(ledger.to_csv(), csv);
    }

    #[test]
    fn a_gap_in_seq_is_an_error() {
        let csv = "\
seq,item_id,kind,labels,origin
1,SO-000042-v1,add,np-0007,person
3,SO-000042-v1,add,bid-0003,person
";
        let err = Ledger::parse(csv).unwrap_err();
        assert_eq!(err.code, Code::Fb0412);
        assert_eq!(err.line, 3);
    }

    #[test]
    fn a_line_that_adds_public_is_an_error() {
        for builtin in ["public", "unlabelled", "marked", "owner-supplied"] {
            let csv =
                format!("seq,item_id,kind,labels,origin\n1,SO-000042-v1,add,{builtin},person\n");
            let err = Ledger::parse(&csv).unwrap_err();
            assert_eq!(err.code, Code::Fb0412);
            assert_eq!(err.line, 2);
        }
    }

    #[test]
    fn an_unknown_kind_is_an_error() {
        let csv = "\
seq,item_id,kind,labels,origin
1,SO-000042-v1,remove,np-0007,person
";
        let err = Ledger::parse(csv).unwrap_err();
        assert_eq!(err.code, Code::Fb0412);
        assert_eq!(err.line, 2);
    }

    // control: LB-02
    #[test]
    fn effective_is_the_cell_plus_the_add_lines() {
        let csv = "\
seq,item_id,kind,labels,origin
1,SO-000042-v1,add,bid-0003,person
2,OTHER-0001,add,ctl-0002,person
3,SO-000042-v1,add,np-0007,intake
";
        let ledger = Ledger::parse(csv).unwrap();
        let cell = LabelSet::from_ids([id("np-0007")]);
        let eff = effective(&cell, "SO-000042-v1", &ledger);
        assert_eq!(eff, LabelSet::from_ids([id("bid-0003"), id("np-0007")]));
    }

    #[test]
    fn an_add_line_resolves_unlabelled() {
        let csv = "\
seq,item_id,kind,labels,origin
1,SO-000042-v1,add,np-0007,person
";
        let ledger = Ledger::parse(csv).unwrap();
        let eff = effective(&LabelSet::unlabelled(), "SO-000042-v1", &ledger);
        assert_eq!(eff, LabelSet::from_ids([id("np-0007")]));
        assert!(!eff.is_unresolved());
    }

    #[test]
    fn marked_survives_an_add_line() {
        let csv = "\
seq,item_id,kind,labels,origin
1,SO-000042-v1,add,np-0007,person
";
        let ledger = Ledger::parse(csv).unwrap();
        let eff = effective(&LabelSet::marked(), "SO-000042-v1", &ledger);
        assert_eq!(
            eff,
            LabelSet::from_ids([super::super::id::MARKED, id("np-0007")])
        );
        assert!(eff.is_unresolved());
    }

    #[test]
    fn a_reapply_line_replaces_marked() {
        let csv = "\
seq,item_id,kind,labels,origin
1,SO-000042-v1,add,np-0007,reapply
";
        let ledger = Ledger::parse(csv).unwrap();
        let eff = effective(&LabelSet::marked(), "SO-000042-v1", &ledger);
        assert_eq!(eff, LabelSet::from_ids([id("np-0007")]));
        assert!(!eff.is_unresolved());
    }

    #[test]
    fn plan_add_continues_seq() {
        let initial = "\
seq,item_id,kind,labels,origin
1,SO-000042-v1,add,np-0007,person
";
        let ledger = Ledger::parse(initial).unwrap();
        let plan = ledger.plan_add(
            ["SO-000043-v1", "SO-000044-v1"],
            &LabelSet::from_ids([id("bid-0003")]),
            LedgerOrigin::Person,
        );
        let reparsed = Ledger::parse(&plan.text).unwrap();
        assert_eq!(reparsed.entries().len(), 3);
        assert_eq!(reparsed.entries()[1].seq, 2);
        assert_eq!(reparsed.entries()[1].item_id, "SO-000043-v1");
        assert_eq!(reparsed.entries()[2].seq, 3);
        assert_eq!(reparsed.entries()[2].item_id, "SO-000044-v1");
    }

    #[test]
    fn plan_add_on_a_new_ledger_writes_the_header() {
        let ledger = Ledger::empty();
        let plan = ledger.plan_add(
            ["SO-000042-v1"],
            &LabelSet::from_ids([id("np-0007")]),
            LedgerOrigin::Person,
        );
        assert!(plan.text.starts_with("seq,item_id,kind,labels,origin\n1,"));
        let reparsed = Ledger::parse(&plan.text).unwrap();
        assert_eq!(reparsed.entries().len(), 1);
        assert_eq!(reparsed.entries()[0].seq, 1);
    }

    #[test]
    fn a_broken_ledger_makes_every_item_unresolved() {
        let err = Ledger::parse("bad header\n").unwrap_err();
        assert_eq!(err.code, Code::Fb0412);
        assert_eq!(err.line, 1);
        let broken = Ledger::broken(err);
        let eff_public = effective(&LabelSet::public(), "SO-000042-v1", &broken);
        assert!(eff_public.is_unresolved());
        let eff_company = effective(
            &LabelSet::from_ids([id("np-0007")]),
            "SO-000042-v1",
            &broken,
        );
        assert!(eff_company.is_unresolved());
        assert!(eff_company.contains(&id("np-0007")));
    }
}
