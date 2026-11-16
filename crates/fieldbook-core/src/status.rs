//! `fieldbook status` (`AD-17`, `status-command`).
//!
//! **Responsibility.** The read-only status report over a parsed [`Model`]: the three
//! finding groups, the counts, the open day-one questions, this ISO week's upkeep and
//! review minutes against the D-16 budget, and the next actions as chip strings. It also
//! holds the pure reader of `docs/maintenance.csv` ([`parse_maintenance`]), the
//! [`StateSummary`] the binary fills from the state folder, and the ISO-week helpers that
//! both use.
//!
//! **Rules.** Pure: no files, no clock, no environment. The binary reads the files and the
//! clock and passes values in. Nothing here writes, and no verb in this module calls a model.

use std::fmt;

use crate::check::Model;
use crate::cite::KeyKind;
use crate::date::{CivilDate, PartialDate, review_by};
use crate::ids::VersionId;
use crate::review::{newest_change_note_date, page_review_by};
use crate::schema::vocab::{
    ConflictStatus, Lifecycle, PageStatus, QuestionStatus, Verification, VerifiedByRole,
};
use crate::strings;

/// The weekly upkeep budget in minutes: low and high ends (D-16 of ADR 0001).
pub const UPKEEP: (u32, u32) = (30, 60);

/// The weekly review budget in minutes: low and high ends (D-16 of ADR 0001).
pub const REVIEW: (u32, u32) = (60, 120);

/// The kind of minute a `docs/maintenance.csv` row records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MinuteKind {
    /// Upkeep time: fixing links, dates and small defects.
    Upkeep,
    /// Review time: reading and checking claims and pages.
    Review,
}

impl MinuteKind {
    /// Parses the `kind` cell.
    #[must_use]
    pub fn parse(input: &str) -> Option<Self> {
        match input {
            "upkeep" => Some(Self::Upkeep),
            "review" => Some(Self::Review),
            _ => None,
        }
    }

    /// Returns the exact word of the `kind` cell.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Upkeep => "upkeep",
            Self::Review => "review",
        }
    }

    /// Returns the sentence label of the minutes line.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Upkeep => "Upkeep",
            Self::Review => "Review",
        }
    }

    /// Returns the budget of this kind.
    #[must_use]
    pub const fn budget(self) -> (u32, u32) {
        match self {
            Self::Upkeep => UPKEEP,
            Self::Review => REVIEW,
        }
    }
}

/// One row of `docs/maintenance.csv`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinuteRow {
    /// The ISO week the minutes were worked in (`YYYY-Www`).
    pub week: String,
    /// Whole minutes worked in that week.
    pub minutes: u32,
    /// Whether the minutes were upkeep or review.
    pub kind: MinuteKind,
}

/// A malformed row of `docs/maintenance.csv`, with its 1-based line number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaintenanceError {
    /// 1-based line number of the offending row (the header is line 1).
    pub line: u32,
    /// What is wrong with the row.
    pub message: String,
}

impl MaintenanceError {
    fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line: u32::try_from(line).unwrap_or(u32::MAX),
            message: message.into(),
        }
    }
}

impl fmt::Display for MaintenanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for MaintenanceError {}

/// The exact header line of `docs/maintenance.csv`.
pub const MAINTENANCE_HEADER: &str = "week,minutes,kind,note";

/// Parses `docs/maintenance.csv`.
///
/// The header is `week,minutes,kind,note`. `week` is an ISO week (`YYYY-Www`), `minutes` a
/// whole number, `kind` `upkeep` or `review`, and `note` free text. Another header, or a row
/// with another value, is malformed.
///
/// # Errors
///
/// Returns [`MaintenanceError`] with the 1-based line number of the first malformed row.
pub fn parse_maintenance(text: &str) -> Result<Vec<MinuteRow>, MaintenanceError> {
    let mut rows = Vec::new();
    let mut lines = text.lines().enumerate().peekable();
    while let Some((idx, raw)) = lines.next() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.is_empty() && lines.peek().is_none() {
            break;
        }
        if idx == 0 {
            if line != MAINTENANCE_HEADER {
                return Err(MaintenanceError::new(
                    1,
                    format!("expected the header `{MAINTENANCE_HEADER}`"),
                ));
            }
            continue;
        }
        let fields: Vec<&str> = line.splitn(4, ',').collect();
        if fields.len() != 4 {
            return Err(MaintenanceError::new(
                idx + 1,
                "expected four cells: week,minutes,kind,note",
            ));
        }
        let week = fields[0];
        if !is_iso_week(week) {
            return Err(MaintenanceError::new(
                idx + 1,
                format!("`{week}` is not an ISO week `YYYY-Www`"),
            ));
        }
        let minutes = fields[1];
        if minutes.is_empty() || !minutes.bytes().all(|b| b.is_ascii_digit()) {
            return Err(MaintenanceError::new(
                idx + 1,
                format!("`{minutes}` is not a whole number of minutes"),
            ));
        }
        let Ok(minutes) = minutes.parse::<u32>() else {
            return Err(MaintenanceError::new(
                idx + 1,
                format!("`{minutes}` is not a whole number of minutes"),
            ));
        };
        let Some(kind) = MinuteKind::parse(fields[2]) else {
            return Err(MaintenanceError::new(
                idx + 1,
                format!("`{}` is neither `upkeep` nor `review`", fields[2]),
            ));
        };
        rows.push(MinuteRow {
            week: week.to_owned(),
            minutes,
            kind,
        });
    }
    if rows.is_empty() && !text.trim().is_empty() {
        // A header with no rows is a valid, empty file.
        return Ok(Vec::new());
    }
    Ok(rows)
}

/// Returns whether `text` is an ISO week `YYYY-Www` with a real week number.
#[must_use]
pub fn is_iso_week(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 8 || bytes[4] != b'-' || bytes[5] != b'W' {
        return false;
    }
    if !bytes[..4].iter().all(u8::is_ascii_digit) || !bytes[6..].iter().all(u8::is_ascii_digit) {
        return false;
    }
    let Ok(year) = text[..4].parse::<u16>() else {
        return false;
    };
    let Ok(week) = text[6..].parse::<u32>() else {
        return false;
    };
    if year == 0 || !(1..=53).contains(&week) {
        return false;
    }
    iso_week_total(year) >= week
}

/// Returns the number of ISO weeks in `year` (52 or 53).
fn iso_week_total(year: u16) -> u32 {
    let dec31 = CivilDate::from_ymd(year, 12, 31).unwrap_or(CivilDate::MAX);
    let (iso_year, week) = iso_week(dec31);
    if iso_year == i32::from(year) {
        week
    } else {
        52
    }
}

/// Returns the ISO week of `date` as `(year, week)`; the ISO year can differ from the
/// calendar year at the year boundary.
#[must_use]
pub fn iso_week(date: CivilDate) -> (i32, u32) {
    let weekday = iso_weekday(date);
    let thursday_offset = i64::from(4 - weekday);
    let thursday = date.add_days(thursday_offset).unwrap_or(date);
    let year = i32::from(thursday.year());
    let jan1 = CivilDate::from_ymd(thursday.year(), 1, 1).unwrap_or(CivilDate::MIN);
    let week = ((thursday.to_epoch_days() - jan1.to_epoch_days()) / 7) + 1;
    (year, u32::try_from(week).unwrap_or(1))
}

/// Returns the ISO weekday of `date`: Monday is `1` and Sunday is `7`.
#[must_use]
pub fn iso_weekday(date: CivilDate) -> i32 {
    let days = date.to_epoch_days();
    i32::try_from((days + 3).rem_euclid(7)).unwrap_or(0) + 1
}

/// Returns the ISO week label of `date` (`YYYY-Www`).
#[must_use]
pub fn iso_week_label(date: CivilDate) -> String {
    let (year, week) = iso_week(date);
    format!("{year:04}-W{week:02}")
}

/// One line the binary adds after the status sections, from the state folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateLine {
    /// The line text, without its trailing newline.
    pub text: String,
}

impl StateLine {
    /// Builds one line from `text`.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

/// The lines `status` prints after its sections, in the order the binary adds them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StateSummary {
    /// The lines, in order.
    pub lines: Vec<StateLine>,
}

impl StateSummary {
    /// Builds a summary from `lines`.
    #[must_use]
    pub const fn new(lines: Vec<StateLine>) -> Self {
        Self { lines }
    }

    /// Adds one line.
    pub fn push(&mut self, text: impl Into<String>) {
        self.lines.push(StateLine::new(text));
    }

    /// Returns whether the summary holds no lines.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// One finding line: an identifier and the reason it is listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusItem {
    /// The claim, page, conflict or question identifier.
    pub id: String,
    /// Why the item is listed.
    pub detail: String,
}

/// One minutes line against a budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetLine {
    /// Minutes recorded in the reported ISO week.
    pub this_week: u128,
    /// The budget this kind is measured against (low, high), in minutes a week.
    pub budget: (u32, u32),
}

impl BudgetLine {
    /// Returns the line text: `Upkeep this week: 45 min (budget 30 to 60)`.
    #[must_use]
    pub fn render(&self, kind: MinuteKind) -> String {
        let over = if self.this_week > u128::from(self.budget.1) {
            " over budget"
        } else {
            ""
        };
        format!(
            "{} this week: {} min (budget {} to {}){}",
            kind.label(),
            self.this_week,
            self.budget.0,
            self.budget.1,
            over
        )
    }
}

/// The status report of one Corpus at one date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusReport {
    /// Claims and pages that rest on a quarantined, withdrawn or superseded version.
    pub held: Vec<StatusItem>,
    /// Conflicts that are still unresolved.
    pub conflicts: Vec<StatusItem>,
    /// Consequential claims that are unreviewed or stale.
    pub unreviewed: Vec<StatusItem>,
    /// Other stale pages and claims.
    pub stale: Vec<StatusItem>,
    /// Typed register rows: sources, claims, questions, conflicts and glossary terms.
    pub register_counts: [usize; 5],
    /// Parsed pages in the Corpus.
    pub pages: usize,
    /// Pages per maturity status, in the order `draft`, `reviewed`, `stable`.
    pub pages_by_status: [usize; 3],
    /// Day-one questions that are still open.
    pub open_questions: Vec<StatusItem>,
    /// This week's upkeep minutes, or `None` when the file is unavailable.
    pub upkeep: Option<BudgetLine>,
    /// This week's review minutes, or `None` when the file is unavailable.
    pub review: Option<BudgetLine>,
    /// The next actions, one chip string per kind of finding, in the order of the sections.
    pub next_actions: Vec<&'static str>,
    /// The state lines the binary added, in order.
    pub summary: Vec<StateLine>,
}

fn claim_version(claim: &crate::schema::ClaimRow) -> Option<VersionId> {
    let source = claim.source_id.as_ref()?;
    let version = claim.version?;
    VersionId::parse(&format!("{source}-v{version}")).ok()
}

/// The source version the claim whose identifier is `claim_id` rests on, when the register
/// names one (an assumption names none).
fn cited_claim_version(model: &Model, claim_id: &str) -> Option<VersionId> {
    model
        .claims()
        .iter()
        .find(|row| row.value.claim_id.as_str() == claim_id)
        .and_then(|row| claim_version(&row.value))
}

fn held_state(model: &Model, version: &VersionId) -> Option<&'static str> {
    let entry = model.index().source_versions.get(version)?;
    if entry.superseded {
        return Some("replaced by a newer version");
    }
    match entry.lifecycle {
        Lifecycle::Active => None,
        Lifecycle::Quarantined => Some("quarantined"),
        Lifecycle::Withdrawn => Some("withdrawn"),
    }
}

fn claim_is_stale(claim: &crate::schema::ClaimRow, today: CivilDate) -> Option<CivilDate> {
    if claim.verification == Verification::Stale {
        return Some(today);
    }
    let base = match (claim.as_of.map(PartialDate::first_day), claim.verified_on) {
        (Some(a), Some(v)) => Some(a.max(v)),
        (Some(a), None) => Some(a),
        (None, Some(v)) => Some(v),
        (None, None) => None,
    }?;
    let date = review_by(base, claim.volatility);
    (date < today).then_some(date)
}

fn page_is_stale(
    model: &Model,
    page: &crate::check::PageEntry,
    today: CivilDate,
) -> Option<CivilDate> {
    let cited: Vec<&crate::schema::ClaimRow> = model
        .citations(&page.id)
        .iter()
        .filter(|citation| citation.key_kind == KeyKind::Claim)
        .flat_map(|citation| {
            model
                .claims()
                .iter()
                .filter(move |row| row.value.claim_id.as_str() == citation.key)
                .map(|row| &row.value)
        })
        .collect();
    let newest = newest_change_note_date(&page.body);
    let date = page_review_by(&page.meta, newest, cited);
    (date < today).then_some(date)
}

fn held_versions_of_page(
    model: &Model,
    page: &crate::check::PageEntry,
) -> Vec<(String, &'static str)> {
    let mut out: Vec<(String, &'static str)> = Vec::new();
    for citation in model.citations(&page.id) {
        let version = match citation.key_kind {
            KeyKind::Version => VersionId::parse(&citation.key).ok(),
            // A claim citation rests on the source version the claim row names.
            KeyKind::Claim => cited_claim_version(model, &citation.key),
            KeyKind::Question | KeyKind::Conflict | KeyKind::Note => None,
        };
        let Some(version) = version else {
            continue;
        };
        if let Some(state) = held_state(model, &version) {
            let named = model
                .index()
                .source_versions
                .get_key_value(&version)
                .map_or_else(
                    || version.as_str().to_owned(),
                    |(id, _)| id.as_str().to_owned(),
                );
            out.push((named, state));
        }
    }
    out.sort();
    out.dedup();
    out
}

fn held_detail(pairs: &[(String, &'static str)]) -> String {
    let joined = pairs
        .iter()
        .map(|(version, state)| format!("{version} ({state})"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("rests on {joined}")
}

/// Builds the status report of `model` at `today`.
///
/// `minutes` is `None` when `docs/maintenance.csv` is unavailable. `summary` holds the state
/// lines the binary read; a card that keeps state of its own adds one line there and changes
/// neither this function nor the binary's status handler.
#[must_use]
pub fn report(
    model: &Model,
    today: CivilDate,
    minutes: Option<&[MinuteRow]>,
    summary: &StateSummary,
) -> StatusReport {
    let mut held: Vec<StatusItem> = Vec::new();
    let mut unreviewed: Vec<StatusItem> = Vec::new();
    let mut stale: Vec<StatusItem> = Vec::new();

    for located in model.claims() {
        let claim = &located.value;
        let id = claim.claim_id.as_str().to_owned();
        if let Some(version) = claim_version(claim)
            && let Some(state) = held_state(model, &version)
        {
            held.push(StatusItem {
                id: id.clone(),
                detail: format!("rests on {} ({state})", version.as_str()),
            });
        }
        let stale_on = claim_is_stale(claim, today);
        if claim.consequential {
            let unreviewed_claim = claim.verified_by_role != Some(VerifiedByRole::Owner);
            if unreviewed_claim || stale_on.is_some() {
                let mut reasons: Vec<String> = Vec::new();
                if unreviewed_claim {
                    reasons.push("unreviewed".to_owned());
                }
                if let Some(date) = stale_on {
                    reasons.push(format!("review_by {date} has passed"));
                }
                unreviewed.push(StatusItem {
                    id,
                    detail: format!("consequential claim; {}", reasons.join("; ")),
                });
                continue;
            }
        }
        if let Some(date) = stale_on {
            stale.push(StatusItem {
                id,
                detail: format!("stale claim; review_by {date}"),
            });
        }
    }

    let mut conflicts: Vec<StatusItem> = Vec::new();
    for located in model.conflicts() {
        let row = &located.value;
        if row.status == ConflictStatus::Unresolved {
            conflicts.push(StatusItem {
                id: row.conflict_id.as_str().to_owned(),
                detail: format!("open conflict: {}", row.topic),
            });
        }
    }

    for page in model.pages() {
        let id = page.id.as_str().to_owned();
        let held_pairs = held_versions_of_page(model, page);
        if !held_pairs.is_empty() {
            held.push(StatusItem {
                id: id.clone(),
                detail: held_detail(&held_pairs),
            });
        }
        if let Some(date) = page_is_stale(model, page, today) {
            stale.push(StatusItem {
                id,
                detail: format!("stale page; review_by {date}"),
            });
        }
    }

    let mut open_questions: Vec<StatusItem> = Vec::new();
    for located in model.questions() {
        let row = &located.value;
        if row.day_one && row.status == QuestionStatus::Open {
            open_questions.push(StatusItem {
                id: row.question_id.as_str().to_owned(),
                detail: row.question.clone(),
            });
        }
    }

    for list in [
        &mut held,
        &mut conflicts,
        &mut unreviewed,
        &mut stale,
        &mut open_questions,
    ] {
        list.sort_by(|a, b| (&a.id, &a.detail).cmp(&(&b.id, &b.detail)));
    }

    let week = iso_week_label(today);
    let week_minutes = |kind: MinuteKind| -> Option<BudgetLine> {
        let rows = minutes?;
        let this_week = rows
            .iter()
            .filter(|row| row.kind == kind && row.week == week)
            .map(|row| u128::from(row.minutes))
            .sum();
        Some(BudgetLine {
            this_week,
            budget: kind.budget(),
        })
    };

    let mut next_actions: Vec<&'static str> = Vec::new();
    if !held.is_empty() {
        next_actions.push(chip("Add a new version of a source"));
    }
    if !conflicts.is_empty() {
        next_actions.push(chip("Find an id or a word; list the versions of a source"));
    }
    if !unreviewed.is_empty() {
        next_actions.push(chip("Record that the owner verified a claim"));
    }
    if !stale.is_empty() {
        next_actions.push(chip("Check the Corpus"));
    }
    if !open_questions.is_empty() {
        next_actions.push(chip("Find an id or a word; list the versions of a source"));
    }
    let mut seen = std::collections::BTreeSet::new();
    next_actions.retain(|action| seen.insert(*action));

    let mut pages_by_status = [0usize; 3];
    for page in model.pages() {
        match page.meta.status {
            PageStatus::Draft => pages_by_status[0] += 1,
            PageStatus::Reviewed => pages_by_status[1] += 1,
            PageStatus::Stable => pages_by_status[2] += 1,
        }
    }

    StatusReport {
        held,
        conflicts,
        unreviewed,
        stale,
        register_counts: [
            model.sources().len(),
            model.claims().len(),
            model.questions().len(),
            model.conflicts().len(),
            model.glossary().len(),
        ],
        pages: model.pages().len(),
        pages_by_status,
        open_questions,
        upkeep: week_minutes(MinuteKind::Upkeep),
        review: week_minutes(MinuteKind::Review),
        next_actions,
        summary: summary.lines.clone(),
    }
}

fn chip(intent: &str) -> &'static str {
    strings::chips()
        .iter()
        .find(|row| row.intent == intent)
        .map_or("", |row| row.chip)
}

/// The heading of the first section.
pub const HELD_HEADING: &str = "Held, withdrawn or superseded versions, and open conflicts";
/// The heading of the second section.
pub const UNREVIEWED_HEADING: &str = "Unreviewed or stale consequential claims";
/// The heading of the third section.
pub const STALE_HEADING: &str = "Other stale items";
/// The heading of the counts section.
pub const COUNTS_HEADING: &str = "Counts";
/// The heading of the day-one questions section.
pub const QUESTIONS_HEADING: &str = "Open day-one questions";
/// The heading of the minutes section.
pub const MINUTES_HEADING: &str = "Minutes this week";
/// The heading of the next actions section.
pub const ACTIONS_HEADING: &str = "Next actions";

impl StatusReport {
    /// Renders the report as the text `fieldbook status` prints.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        section(
            &mut out,
            HELD_HEADING,
            self.held.iter().chain(&self.conflicts),
        );
        section(&mut out, UNREVIEWED_HEADING, self.unreviewed.iter());
        section(&mut out, STALE_HEADING, self.stale.iter());

        out.push_str(COUNTS_HEADING);
        out.push('\n');
        out.push_str(&format!(
            "  sources {}, claims {}, questions {}, conflicts {}, glossary {}, pages {}\n",
            self.register_counts[0],
            self.register_counts[1],
            self.register_counts[2],
            self.register_counts[3],
            self.register_counts[4],
            self.pages
        ));
        out.push_str(&format!(
            "  pages: draft {}, reviewed {}, stable {}\n",
            self.pages_by_status[0], self.pages_by_status[1], self.pages_by_status[2]
        ));
        out.push('\n');

        section(&mut out, QUESTIONS_HEADING, self.open_questions.iter());

        out.push_str(MINUTES_HEADING);
        out.push('\n');
        if let Some(upkeep) = self.upkeep {
            out.push_str("  ");
            out.push_str(&upkeep.render(MinuteKind::Upkeep));
            out.push('\n');
        } else {
            out.push_str("  Upkeep this week: Unavailable\n");
        }
        if let Some(review) = self.review {
            out.push_str("  ");
            out.push_str(&review.render(MinuteKind::Review));
            out.push('\n');
        } else {
            out.push_str("  Review this week: Unavailable\n");
        }
        out.push('\n');

        out.push_str(ACTIONS_HEADING);
        out.push('\n');
        if self.next_actions.is_empty() {
            out.push_str("  none\n");
        } else {
            for action in &self.next_actions {
                out.push_str("  ");
                out.push_str(action);
                out.push('\n');
            }
        }
        if !self.summary.is_empty() {
            out.push('\n');
        }

        for line in &self.summary {
            out.push_str(&line.text);
            out.push('\n');
        }
        out
    }
}

fn section<'a>(out: &mut String, heading: &str, items: impl Iterator<Item = &'a StatusItem>) {
    out.push_str(heading);
    out.push('\n');
    let mut any = false;
    for item in items {
        any = true;
        out.push_str("  ");
        out.push_str(&item.id);
        out.push_str("  ");
        out.push_str(&item.detail);
        out.push('\n');
    }
    if !any {
        out.push_str("  none\n");
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::{
        BudgetLine, MinuteKind, UPKEEP, is_iso_week, iso_week, iso_week_label, parse_maintenance,
    };
    use crate::date::CivilDate;

    fn date(s: &str) -> CivilDate {
        CivilDate::parse(s).expect("valid test date")
    }

    #[test]
    fn iso_weeks_match_the_reference_calendar() {
        assert_eq!(iso_week_label(date("2026-03-01")), "2026-W09");
        assert_eq!(iso_week_label(date("2026-01-01")), "2026-W01");
        assert_eq!(iso_week_label(date("2025-12-29")), "2026-W01");
        assert_eq!(iso_week_label(date("2027-01-01")), "2026-W53");
        assert_eq!(iso_week(date("2020-12-31")).0, 2020);
        assert!(is_iso_week("2026-W09"));
        assert!(!is_iso_week("2026-W54"));
        assert!(!is_iso_week("2021-W53"));
        assert!(!is_iso_week("2026-09"));
    }

    #[test]
    fn maintenance_reads_rows_and_refuses_bad_ones() {
        let text = "week,minutes,kind,note\n2026-W09,45,upkeep,Fixed links\n2026-W09,90,review,Read claims\n";
        let rows = parse_maintenance(text).expect("valid rows");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].kind, MinuteKind::Upkeep);
        assert_eq!(rows[1].minutes, 90);

        let header = parse_maintenance("week,minutes,kind\0,note\n").expect_err("bad header");
        assert_eq!(header.line, 1);

        let kind = parse_maintenance("week,minutes,kind,note\n2026-W09,5,chores,No\n")
            .expect_err("bad kind");
        assert_eq!(kind.line, 2);
        assert!(kind.to_string().contains("neither"));

        let minutes = parse_maintenance("week,minutes,kind,note\n2026-W09,five,review,No\n")
            .expect_err("bad minutes");
        assert_eq!(minutes.line, 2);

        let week = parse_maintenance("week,minutes,kind,note\n2026-W54,5,review,No\n")
            .expect_err("bad week");
        assert_eq!(week.line, 2);

        assert!(parse_maintenance("week,minutes,kind,note\n").is_ok());
    }

    #[test]
    fn budget_line_marks_an_overrun() {
        let within = BudgetLine {
            this_week: u128::from(UPKEEP.1),
            budget: UPKEEP,
        };
        assert_eq!(
            within.render(MinuteKind::Upkeep),
            "Upkeep this week: 60 min (budget 30 to 60)"
        );
        let over = BudgetLine {
            this_week: 61,
            budget: UPKEEP,
        };
        assert!(over.render(MinuteKind::Upkeep).ends_with("over budget"));
    }
}
