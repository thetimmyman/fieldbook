//! Typed representation of `claims.csv` (35 columns), `ClaimUnit`, `AppliesTo`,
//! `ClaimDraft`, `ClaimRow::entry`, and status-raise checks (`is_raise`).
//!
//! **Responsibility.** Parse and format rows of `claims.csv`, validate single-cell
//! forms (including `unit`, structured `applies_to`, and `ext:<kind>` locators),
//! construct new claims in entry state, and detect raises of protected status fields.
//!
//! **Rules.** Pure: no I/O. Cross-column rules (`check-register-cells`) and decimal
//! grammar (`calc-cases`) are out of scope here.

use std::fmt;
use std::str::FromStr;

use fieldbook_calc::{Currency, Unit};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::date::{CivilDate, PartialDate, Volatility};
use crate::ids::{AreaCode, ClaimId, ConflictId, QuestionId, SourceId};
use crate::schema::locator::{Locator, LocatorKind};
use crate::schema::rows::{
    CellProblem, CellProblemKind, MAX_QUOTE_CHARS, MAX_TEXT_CHARS, cell_at, join_pipe, opt_date,
    opt_id, opt_id_list, opt_pdate, opt_positive_u32, opt_text, opt_vocab, req_bool, req_id,
    req_text, req_vocab,
};
use crate::schema::table::Row;
use crate::schema::vocab::{
    Adoption, Applicability, ClaimKind, DataOrigin, DraftedBy, EvidenceGrade, Instrument,
    TextStatus, Verification, VerifiedByRole, VocabError,
};

/// Value of the `unit` cell in `claims.csv`: either a three-letter uppercase currency code
/// ([`Currency`]) or a non-currency [`Unit`] word from `fieldbook_calc::Unit::from_word`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClaimUnit {
    /// Three-letter ASCII uppercase currency code (`USD`, `EUR`, ...).
    Currency(Currency),
    /// Non-currency unit word (`percent`, `basis_points`, `days`, ...).
    Unit(Unit),
}

impl ClaimUnit {
    /// Parses a `unit` cell value.
    pub fn parse(input: &str) -> Result<Self, VocabError> {
        if let Ok(u) = Unit::from_word(input) {
            return Ok(Self::Unit(u));
        }
        if let Ok(c) = Currency::parse(input) {
            return Ok(Self::Currency(c));
        }
        Err(VocabError::new("unit", input))
    }

    /// Returns the canonical string slice for this unit or currency code.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Currency(c) => c.as_str(),
            Self::Unit(u) => u.as_str(),
        }
    }
}

impl fmt::Display for ClaimUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ClaimUnit {
    type Err = VocabError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Serialize for ClaimUnit {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ClaimUnit {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// Allowed values of the `event` key in a structured `applies_to` cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AppliesToEvent {
    /// `award_date`
    AwardDate,
    /// `solicitation_date`
    SolicitationDate,
    /// `clause_date`
    ClauseDate,
    /// `cdrl_date`
    CdrlDate,
    /// `query_date`
    QueryDate,
    /// `event_date`
    EventDate,
}

impl AppliesToEvent {
    /// Parses an `event` value in `applies_to`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "award_date" => Some(Self::AwardDate),
            "solicitation_date" => Some(Self::SolicitationDate),
            "clause_date" => Some(Self::ClauseDate),
            "cdrl_date" => Some(Self::CdrlDate),
            "query_date" => Some(Self::QueryDate),
            "event_date" => Some(Self::EventDate),
            _ => None,
        }
    }

    /// Returns the canonical string spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AwardDate => "award_date",
            Self::SolicitationDate => "solicitation_date",
            Self::ClauseDate => "clause_date",
            Self::CdrlDate => "cdrl_date",
            Self::QueryDate => "query_date",
            Self::EventDate => "event_date",
        }
    }
}

impl fmt::Display for AppliesToEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Structured fields of an `applies_to` cell (`key=value` pairs separated by `; `).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuredAppliesTo {
    /// `event`
    pub event: Option<AppliesToEvent>,
    /// `entered_from`
    pub entered_from: Option<PartialDate>,
    /// `entered_to`
    pub entered_to: Option<PartialDate>,
    /// `contract_type`
    pub contract_type: Option<String>,
    /// `fiscal_year`
    pub fiscal_year: Option<String>,
    /// `calendar`
    pub calendar: Option<String>,
    /// `basis`
    pub basis: Option<String>,
}

/// Parsed representation of the `applies_to` column in `claims.csv`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppliesTo {
    /// A cell with no `=`, treated as free prose.
    Prose(String),
    /// A structured `key=value; ...` cell.
    Structured(StructuredAppliesTo),
}

/// Error returned when a structured `applies_to` cell violates the grammar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliesToProblem {
    /// The offending key name if one was identified, or the full cell text.
    pub target: String,
    /// Human-readable description of the refusal.
    pub message: String,
}

impl AppliesToProblem {
    fn new(target: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            message: message.into(),
        }
    }

    /// Returns the offending key or cell text named by this refusal.
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }
}

impl fmt::Display for AppliesToProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.message, self.target)
    }
}

impl std::error::Error for AppliesToProblem {}

fn is_applies_to_word(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

impl AppliesTo {
    /// Parses an `applies_to` cell.
    ///
    /// A cell without `=` is returned as [`AppliesTo::Prose`]. A cell containing `=`
    /// must consist of `key=value` pairs separated by `"; "` with each key appearing
    /// at most once.
    pub fn parse(input: &str) -> Result<Self, AppliesToProblem> {
        if !input.contains('=') {
            return Ok(Self::Prose(input.to_owned()));
        }

        // Check that every `;` is followed by exactly one space (and not at the end).
        let bytes = input.as_bytes();
        for (i, &b) in bytes.iter().enumerate() {
            if b == b';' && (bytes.get(i + 1) != Some(&b' ') || bytes.get(i + 2) == Some(&b' ')) {
                return Err(AppliesToProblem::new(
                    input,
                    "pairs in applies_to must be separated by `; `",
                ));
            }
        }

        let mut structured = StructuredAppliesTo::default();
        for pair in input.split("; ") {
            let Some((key, val)) = pair.split_once('=') else {
                return Err(AppliesToProblem::new(
                    input,
                    "every segment in structured applies_to must be key=value",
                ));
            };
            if key.is_empty() || key.contains(' ') {
                return Err(AppliesToProblem::new(
                    input,
                    "malformed key in structured applies_to",
                ));
            }
            if val.is_empty() || val.contains('=') || val.contains(' ') {
                return Err(AppliesToProblem::new(
                    key,
                    format!("invalid value for key `{key}` in applies_to"),
                ));
            }

            match key {
                "event" => {
                    if structured.event.is_some() {
                        return Err(AppliesToProblem::new(
                            key,
                            "repeated key `event` in applies_to",
                        ));
                    }
                    let Some(ev) = AppliesToEvent::parse(val) else {
                        return Err(AppliesToProblem::new(
                            key,
                            format!("unknown event `{val}` in applies_to"),
                        ));
                    };
                    structured.event = Some(ev);
                }
                "entered_from" => {
                    if structured.entered_from.is_some() {
                        return Err(AppliesToProblem::new(
                            key,
                            "repeated key `entered_from` in applies_to",
                        ));
                    }
                    let Ok(pd) = PartialDate::parse(val) else {
                        return Err(AppliesToProblem::new(
                            key,
                            format!("invalid partial date `{val}` for `entered_from`"),
                        ));
                    };
                    structured.entered_from = Some(pd);
                }
                "entered_to" => {
                    if structured.entered_to.is_some() {
                        return Err(AppliesToProblem::new(
                            key,
                            "repeated key `entered_to` in applies_to",
                        ));
                    }
                    let Ok(pd) = PartialDate::parse(val) else {
                        return Err(AppliesToProblem::new(
                            key,
                            format!("invalid partial date `{val}` for `entered_to`"),
                        ));
                    };
                    structured.entered_to = Some(pd);
                }
                "contract_type" => {
                    if structured.contract_type.is_some() {
                        return Err(AppliesToProblem::new(
                            key,
                            "repeated key `contract_type` in applies_to",
                        ));
                    }
                    if !is_applies_to_word(val) {
                        return Err(AppliesToProblem::new(
                            key,
                            format!("invalid word `{val}` for `contract_type`"),
                        ));
                    }
                    structured.contract_type = Some(val.to_owned());
                }
                "fiscal_year" => {
                    if structured.fiscal_year.is_some() {
                        return Err(AppliesToProblem::new(
                            key,
                            "repeated key `fiscal_year` in applies_to",
                        ));
                    }
                    if !is_applies_to_word(val) {
                        return Err(AppliesToProblem::new(
                            key,
                            format!("invalid word `{val}` for `fiscal_year`"),
                        ));
                    }
                    structured.fiscal_year = Some(val.to_owned());
                }
                "calendar" => {
                    if structured.calendar.is_some() {
                        return Err(AppliesToProblem::new(
                            key,
                            "repeated key `calendar` in applies_to",
                        ));
                    }
                    if !is_applies_to_word(val) {
                        return Err(AppliesToProblem::new(
                            key,
                            format!("invalid word `{val}` for `calendar`"),
                        ));
                    }
                    structured.calendar = Some(val.to_owned());
                }
                "basis" => {
                    if structured.basis.is_some() {
                        return Err(AppliesToProblem::new(
                            key,
                            "repeated key `basis` in applies_to",
                        ));
                    }
                    if !is_applies_to_word(val) {
                        return Err(AppliesToProblem::new(
                            key,
                            format!("invalid word `{val}` for `basis`"),
                        ));
                    }
                    structured.basis = Some(val.to_owned());
                }
                _ => {
                    return Err(AppliesToProblem::new(
                        key,
                        format!("unknown key `{key}` in applies_to"),
                    ));
                }
            }
        }

        Ok(Self::Structured(structured))
    }
}

/// Author-supplied cells for [`ClaimRow::entry`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimDraft {
    /// Column 1: `claim_id`.
    pub claim_id: ClaimId,
    /// Column 2: `area`.
    pub area: AreaCode,
    /// Column 3: `kind`.
    pub kind: ClaimKind,
    /// Column 4: `statement`.
    pub statement: String,
    /// Column 5: `as_of`.
    pub as_of: Option<PartialDate>,
    /// Column 6: `source_id` (`None` for an assumption).
    pub source_id: Option<SourceId>,
    /// Column 7: `version` (`None` for an assumption).
    pub version: Option<u32>,
    /// Column 8: `locator_kind` (`None` for an assumption).
    pub locator_kind: Option<LocatorKind>,
    /// Column 9: `locator` (`None` for an assumption or `no_fine_locator`).
    pub locator: Option<String>,
    /// Column 10: `quote` (`None` for an assumption or non-consequential claim without quote).
    pub quote: Option<String>,
    /// Column 11: `consequential`.
    pub consequential: bool,
    /// Column 12: `drafted_by`.
    pub drafted_by: DraftedBy,
    /// Column 17: `volatility`.
    pub volatility: Volatility,
}

/// A parsed row of `claims.csv` (35 columns).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimRow {
    /// 1. `claim_id`: `CL-AAA-nnnn` or five digits from `10000`.
    pub claim_id: ClaimId,
    /// 2. `area`: Three-letter lowercase area code.
    pub area: AreaCode,
    /// 3. `kind`: `claim_kind`.
    pub kind: ClaimKind,
    /// 4. `statement`: Plain sentence (max 1,000 chars).
    pub statement: String,
    /// 5. `as_of`: Optional partial/full date (`None` when empty).
    pub as_of: Option<PartialDate>,
    /// 6. `source_id`: Optional source ID (`S-nnnn`).
    pub source_id: Option<SourceId>,
    /// 7. `version`: Optional positive source version integer.
    pub version: Option<u32>,
    /// 8. `locator_kind`: Optional [`LocatorKind`].
    pub locator_kind: Option<LocatorKind>,
    /// 9. `locator`: Optional locator text (max 1,000 chars).
    pub locator: Option<String>,
    /// 10. `quote`: Optional verbatim quote (max 300 chars).
    pub quote: Option<String>,
    /// 11. `consequential`: Boolean.
    pub consequential: bool,
    /// 12. `drafted_by`: [`DraftedBy`].
    pub drafted_by: DraftedBy,
    /// 13. `verification`: [`Verification`].
    pub verification: Verification,
    /// 14. `applicability`: [`Applicability`].
    pub applicability: Applicability,
    /// 15. `adoption`: [`Adoption`].
    pub adoption: Adoption,
    /// 16. `data_origin`: [`DataOrigin`].
    pub data_origin: DataOrigin,
    /// 17. `volatility`: [`Volatility`].
    pub volatility: Volatility,
    /// 18. `value`: Optional decimal text (grammar checked in `calc-cases`).
    pub value: Option<String>,
    /// 19. `unit`: Optional [`ClaimUnit`].
    pub unit: Option<ClaimUnit>,
    /// 20. `effective_from`: Optional partial/full date.
    pub effective_from: Option<PartialDate>,
    /// 21. `effective_to`: Optional partial/full date.
    pub effective_to: Option<PartialDate>,
    /// 22. `instrument`: Optional [`Instrument`].
    pub instrument: Option<Instrument>,
    /// 23. `citation_codified`: Optional codified number (max 1,000 chars).
    pub citation_codified: Option<String>,
    /// 24. `citation_rfo`: Optional RFO/deviation number (max 1,000 chars).
    pub citation_rfo: Option<String>,
    /// 25. `edition_date`: Optional partial/full date.
    pub edition_date: Option<PartialDate>,
    /// 26. `text_status`: Optional [`TextStatus`].
    pub text_status: Option<TextStatus>,
    /// 27. `applies_to`: Optional prose or structured `key=value` string (max 1,000 chars).
    pub applies_to: Option<String>,
    /// 28. `evidence_grade`: Optional [`EvidenceGrade`].
    pub evidence_grade: Option<EvidenceGrade>,
    /// 29. `verified_on`: Optional full calendar date (`YYYY-MM-DD`).
    pub verified_on: Option<CivilDate>,
    /// 30. `verified_by_role`: Optional [`VerifiedByRole`].
    pub verified_by_role: Option<VerifiedByRole>,
    /// 31. `supersedes`: List of [`ClaimId`]s this claim replaces.
    pub supersedes: Vec<ClaimId>,
    /// 32. `superseded_by`: List of [`ClaimId`]s that replace this claim.
    pub superseded_by: Vec<ClaimId>,
    /// 33. `conflict_id`: Optional [`ConflictId`].
    pub conflict_id: Option<ConflictId>,
    /// 34. `question_id`: Optional [`QuestionId`].
    pub question_id: Option<QuestionId>,
    /// 35. `note`: Optional free text (max 1,000 chars).
    pub note: Option<String>,
}

fn parse_volatility(s: &str) -> Result<Volatility, VocabError> {
    Volatility::parse(s).ok_or_else(|| VocabError::new("volatility", s))
}

fn parse_claim_locator(
    row: &Row,
    idx: usize,
    locator_kind: Option<&LocatorKind>,
    problems: &mut Vec<CellProblem>,
) -> Option<Option<String>> {
    let raw = cell_at(row, idx);
    if raw.is_empty() {
        if matches!(locator_kind, Some(LocatorKind::Extension(_))) {
            problems.push(CellProblem::new("locator", CellProblemKind::Form));
            return None;
        }
        return Some(None);
    }
    if raw.chars().count() > MAX_TEXT_CHARS {
        problems.push(CellProblem::new("locator", CellProblemKind::TooLong));
        return None;
    }
    if raw.contains('|') {
        problems.push(CellProblem::new("locator", CellProblemKind::Form));
        return None;
    }

    if let Some(LocatorKind::Extension(expected_kind)) = locator_kind {
        match Locator::parse_extension(raw) {
            Ok(ref loc @ Locator::Extension { ref kind, .. }) if kind == expected_kind => {
                return Some(Some(loc.to_canonical_string()));
            }
            _ => {
                problems.push(CellProblem::new("locator", CellProblemKind::Form));
                return None;
            }
        }
    }

    if raw.starts_with("ext ") || raw == "ext" {
        // An `ext ...` locator without a matching `ext:<kind>` in `locator_kind` is invalid.
        problems.push(CellProblem::new("locator", CellProblemKind::Form));
        return None;
    }

    Some(Some(raw.to_owned()))
}

fn parse_applies_to_cell(
    row: &Row,
    idx: usize,
    problems: &mut Vec<CellProblem>,
) -> Option<Option<String>> {
    let opt_str = opt_text(row, idx, "applies_to", MAX_TEXT_CHARS, problems)?;
    let Some(s) = opt_str else {
        return Some(None);
    };
    if s.contains('=') && AppliesTo::parse(&s).is_err() {
        problems.push(CellProblem::new("applies_to", CellProblemKind::Form));
        return None;
    }
    Some(Some(s))
}

impl ClaimRow {
    /// Builds a new [`ClaimRow`] from `draft` with the entry state fixed by the
    /// promotion rules:
    /// - `verification = Verification::Unchecked`
    /// - `applicability = Applicability::PendingCompanyConfirmation`
    /// - `adoption = Adoption::Proposed`
    /// - `data_origin = DataOrigin::Unknown`
    /// - `verified_on = None` and `verified_by_role = None`
    /// - every other optional cell empty.
    #[must_use]
    pub fn entry(draft: ClaimDraft) -> Self {
        Self {
            claim_id: draft.claim_id,
            area: draft.area,
            kind: draft.kind,
            statement: draft.statement,
            as_of: draft.as_of,
            source_id: draft.source_id,
            version: draft.version,
            locator_kind: draft.locator_kind,
            locator: draft.locator,
            quote: draft.quote,
            consequential: draft.consequential,
            drafted_by: draft.drafted_by,
            verification: Verification::Unchecked,
            applicability: Applicability::PendingCompanyConfirmation,
            adoption: Adoption::Proposed,
            data_origin: DataOrigin::Unknown,
            volatility: draft.volatility,
            value: None,
            unit: None,
            effective_from: None,
            effective_to: None,
            instrument: None,
            citation_codified: None,
            citation_rfo: None,
            edition_date: None,
            text_status: None,
            applies_to: None,
            evidence_grade: None,
            verified_on: None,
            verified_by_role: None,
            supersedes: Vec::new(),
            superseded_by: Vec::new(),
            conflict_id: None,
            question_id: None,
            note: None,
        }
    }

    /// Returns `true` when `verification`, `applicability`, `adoption` and `data_origin`
    /// hold their entry values and `verified_on` and `verified_by_role` are empty.
    #[must_use]
    pub const fn is_entry_state(&self) -> bool {
        matches!(self.verification, Verification::Unchecked)
            && matches!(
                self.applicability,
                Applicability::PendingCompanyConfirmation
            )
            && matches!(self.adoption, Adoption::Proposed)
            && matches!(self.data_origin, DataOrigin::Unknown)
            && self.verified_on.is_none()
            && self.verified_by_role.is_none()
    }

    /// Parses a 35-column `claims.csv` row one cell at a time.
    pub fn from_row(row: &Row) -> Result<Self, Vec<CellProblem>> {
        let mut problems = Vec::new();

        let claim_id = req_id(row, 0, "claim_id", ClaimId::parse, &mut problems);
        let area = req_id(row, 1, "area", AreaCode::parse, &mut problems);
        let kind = req_vocab(row, 2, "kind", ClaimKind::parse, &mut problems);
        let statement = req_text(row, 3, "statement", MAX_TEXT_CHARS, &mut problems);
        let as_of = opt_pdate(row, 4, "as_of", &mut problems);
        let source_id = opt_id(row, 5, "source_id", SourceId::parse, &mut problems);
        let version = opt_positive_u32(row, 6, "version", &mut problems);
        let locator_kind = opt_vocab(row, 7, "locator_kind", LocatorKind::parse, &mut problems);
        let locator_kind_ref = locator_kind.as_ref().and_then(Option::as_ref);
        let locator = parse_claim_locator(row, 8, locator_kind_ref, &mut problems);
        let quote = opt_text(row, 9, "quote", MAX_QUOTE_CHARS, &mut problems);
        let consequential = req_bool(row, 10, "consequential", &mut problems);
        let drafted_by = req_vocab(row, 11, "drafted_by", DraftedBy::parse, &mut problems);
        let verification = req_vocab(row, 12, "verification", Verification::parse, &mut problems);
        let applicability = req_vocab(
            row,
            13,
            "applicability",
            Applicability::parse,
            &mut problems,
        );
        let adoption = req_vocab(row, 14, "adoption", Adoption::parse, &mut problems);
        let data_origin = req_vocab(row, 15, "data_origin", DataOrigin::parse, &mut problems);
        let volatility = req_vocab(row, 16, "volatility", parse_volatility, &mut problems);
        let value = opt_text(row, 17, "value", MAX_TEXT_CHARS, &mut problems);
        let unit = opt_vocab(row, 18, "unit", ClaimUnit::parse, &mut problems);
        let effective_from = opt_pdate(row, 19, "effective_from", &mut problems);
        let effective_to = opt_pdate(row, 20, "effective_to", &mut problems);
        let instrument = opt_vocab(row, 21, "instrument", Instrument::parse, &mut problems);
        let citation_codified =
            opt_text(row, 22, "citation_codified", MAX_TEXT_CHARS, &mut problems);
        let citation_rfo = opt_text(row, 23, "citation_rfo", MAX_TEXT_CHARS, &mut problems);
        let edition_date = opt_pdate(row, 24, "edition_date", &mut problems);
        let text_status = opt_vocab(row, 25, "text_status", TextStatus::parse, &mut problems);
        let applies_to = parse_applies_to_cell(row, 26, &mut problems);
        let evidence_grade = opt_vocab(
            row,
            27,
            "evidence_grade",
            EvidenceGrade::parse,
            &mut problems,
        );
        let verified_on = opt_date(row, 28, "verified_on", &mut problems);
        let verified_by_role = opt_vocab(
            row,
            29,
            "verified_by_role",
            VerifiedByRole::parse,
            &mut problems,
        );
        let supersedes = opt_id_list(row, 30, "supersedes", ClaimId::parse, &mut problems);
        let superseded_by = opt_id_list(row, 31, "superseded_by", ClaimId::parse, &mut problems);
        let conflict_id = opt_id(row, 32, "conflict_id", ConflictId::parse, &mut problems);
        let question_id = opt_id(row, 33, "question_id", QuestionId::parse, &mut problems);
        let note = opt_text(row, 34, "note", MAX_TEXT_CHARS, &mut problems);

        if !problems.is_empty() {
            return Err(problems);
        }

        let (
            Some(claim_id),
            Some(area),
            Some(kind),
            Some(statement),
            Some(as_of),
            Some(source_id),
            Some(version),
            Some(locator_kind),
            Some(locator),
            Some(quote),
            Some(consequential),
            Some(drafted_by),
            Some(verification),
            Some(applicability),
            Some(adoption),
            Some(data_origin),
            Some(volatility),
            Some(value),
            Some(unit),
            Some(effective_from),
            Some(effective_to),
            Some(instrument),
            Some(citation_codified),
            Some(citation_rfo),
            Some(edition_date),
            Some(text_status),
            Some(applies_to),
            Some(evidence_grade),
            Some(verified_on),
            Some(verified_by_role),
            Some(supersedes),
            Some(superseded_by),
            Some(conflict_id),
            Some(question_id),
            Some(note),
        ) = (
            claim_id,
            area,
            kind,
            statement,
            as_of,
            source_id,
            version,
            locator_kind,
            locator,
            quote,
            consequential,
            drafted_by,
            verification,
            applicability,
            adoption,
            data_origin,
            volatility,
            value,
            unit,
            effective_from,
            effective_to,
            instrument,
            citation_codified,
            citation_rfo,
            edition_date,
            text_status,
            applies_to,
            evidence_grade,
            verified_on,
            verified_by_role,
            supersedes,
            superseded_by,
            conflict_id,
            question_id,
            note,
        )
        else {
            return Err(problems);
        };

        Ok(Self {
            claim_id,
            area,
            kind,
            statement,
            as_of,
            source_id,
            version,
            locator_kind,
            locator,
            quote,
            consequential,
            drafted_by,
            verification,
            applicability,
            adoption,
            data_origin,
            volatility,
            value,
            unit,
            effective_from,
            effective_to,
            instrument,
            citation_codified,
            citation_rfo,
            edition_date,
            text_status,
            applies_to,
            evidence_grade,
            verified_on,
            verified_by_role,
            supersedes,
            superseded_by,
            conflict_id,
            question_id,
            note,
        })
    }

    /// Formats this [`ClaimRow`] back into its 35 CSV cell strings in column order.
    #[must_use]
    pub fn to_cells(&self) -> Vec<String> {
        vec![
            self.claim_id.to_string(),
            self.area.to_string(),
            self.kind.as_str().to_owned(),
            self.statement.clone(),
            self.as_of.map_or_else(String::new, |d| d.to_string()),
            self.source_id
                .as_ref()
                .map_or_else(String::new, ToString::to_string),
            self.version.map_or_else(String::new, |v| v.to_string()),
            self.locator_kind
                .as_ref()
                .map_or_else(String::new, ToString::to_string),
            self.locator.clone().unwrap_or_default(),
            self.quote.clone().unwrap_or_default(),
            self.consequential.to_string(),
            self.drafted_by.as_str().to_owned(),
            self.verification.as_str().to_owned(),
            self.applicability.as_str().to_owned(),
            self.adoption.as_str().to_owned(),
            self.data_origin.as_str().to_owned(),
            self.volatility.as_str().to_owned(),
            self.value.clone().unwrap_or_default(),
            self.unit.map_or_else(String::new, |u| u.to_string()),
            self.effective_from
                .map_or_else(String::new, |d| d.to_string()),
            self.effective_to
                .map_or_else(String::new, |d| d.to_string()),
            self.instrument
                .map_or_else(String::new, |i| i.as_str().to_owned()),
            self.citation_codified.clone().unwrap_or_default(),
            self.citation_rfo.clone().unwrap_or_default(),
            self.edition_date
                .map_or_else(String::new, |d| d.to_string()),
            self.text_status
                .map_or_else(String::new, |t| t.as_str().to_owned()),
            self.applies_to.clone().unwrap_or_default(),
            self.evidence_grade
                .map_or_else(String::new, |g| g.as_str().to_owned()),
            self.verified_on.map_or_else(String::new, |d| d.to_string()),
            self.verified_by_role
                .map_or_else(String::new, |r| r.as_str().to_owned()),
            join_pipe(&self.supersedes),
            join_pipe(&self.superseded_by),
            self.conflict_id
                .as_ref()
                .map_or_else(String::new, ToString::to_string),
            self.question_id
                .as_ref()
                .map_or_else(String::new, ToString::to_string),
            self.note.clone().unwrap_or_default(),
        ]
    }
}

/// Returns `true` when `after` raises a protected status field compared with `before`.
///
/// - With `Some(before)`: `true` when `verification`, `applicability` or `adoption`
///   changes to a value other than its entry value, or when `verified_on` or
///   `verified_by_role` changes to a non-empty value (setting a field back to its
///   entry value / clearing `verified_on` or `verified_by_role` is not a raise).
/// - With `None` (a new row): `true` exactly when `!after.is_entry_state()`.
#[must_use]
pub fn is_raise(before: Option<&ClaimRow>, after: &ClaimRow) -> bool {
    let Some(prev) = before else {
        return !after.is_entry_state();
    };

    let ver_raised =
        after.verification != prev.verification && after.verification != Verification::Unchecked;
    let app_raised = after.applicability != prev.applicability
        && after.applicability != Applicability::PendingCompanyConfirmation;
    let ado_raised = after.adoption != prev.adoption && after.adoption != Adoption::Proposed;
    let ver_on_changed = after.verified_on != prev.verified_on && after.verified_on.is_some();
    let ver_role_changed =
        after.verified_by_role != prev.verified_by_role && after.verified_by_role.is_some();

    ver_raised || app_raised || ado_raised || ver_on_changed || ver_role_changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row_from(cells: Vec<String>) -> Row {
        let cols = (1..=u32::try_from(cells.len()).unwrap_or(1)).collect();
        Row {
            line: 2,
            cols,
            cells,
        }
    }

    fn sample_draft() -> ClaimDraft {
        ClaimDraft {
            claim_id: ClaimId::parse("CL-CMP-0011").expect("claim_id"),
            area: AreaCode::parse("cmp").expect("area"),
            kind: ClaimKind::Fact,
            statement: "Single-system payment withholding is capped at five percent.".to_owned(),
            as_of: Some(PartialDate::parse("2025-11-01").expect("as_of")),
            source_id: Some(SourceId::parse("S-0042").expect("source_id")),
            version: Some(2),
            locator_kind: Some(LocatorKind::PdfPageBox),
            locator: Some("p. 14".to_owned()),
            quote: Some("five percent for a single business system".to_owned()),
            consequential: true,
            drafted_by: DraftedBy::Codex,
            volatility: Volatility::V1,
        }
    }

    #[test]
    fn entry_is_in_entry_state() {
        let row = ClaimRow::entry(sample_draft());
        assert_eq!(row.verification, Verification::Unchecked);
        assert_eq!(row.applicability, Applicability::PendingCompanyConfirmation);
        assert_eq!(row.adoption, Adoption::Proposed);
        assert_eq!(row.data_origin, DataOrigin::Unknown);
        assert!(row.verified_on.is_none());
        assert!(row.verified_by_role.is_none());
        assert!(row.is_entry_state());
        assert!(!is_raise(None, &row));
    }

    #[test]
    fn a_change_of_verified_by_role_is_a_raise() {
        let base = ClaimRow::entry(sample_draft());
        let mut after = base.clone();
        after.verified_by_role = Some(VerifiedByRole::Owner);
        assert!(is_raise(Some(&base), &after));
    }

    #[test]
    fn a_change_of_verified_on_is_a_raise() {
        let base = ClaimRow::entry(sample_draft());
        let mut after = base.clone();
        after.verified_on = Some(CivilDate::parse("2025-11-04").expect("date"));
        assert!(is_raise(Some(&base), &after));
    }

    #[test]
    fn a_reset_to_an_entry_value_is_not_a_raise() {
        let mut checked = ClaimRow::entry(sample_draft());
        checked.verification = Verification::PrimarySourceSupported;
        checked.applicability = Applicability::GeneralReference;
        checked.adoption = Adoption::PrototypeReviewed;
        checked.verified_on = Some(CivilDate::parse("2025-11-04").expect("date"));
        checked.verified_by_role = Some(VerifiedByRole::Owner);

        let reset = ClaimRow::entry(sample_draft());
        assert!(!is_raise(Some(&checked), &reset));
    }

    #[test]
    fn a_statement_change_is_not_a_raise() {
        let base = ClaimRow::entry(sample_draft());
        let mut after = base.clone();
        after.statement = "Updated synthetic wording of the claim.".to_owned();
        assert!(!is_raise(Some(&base), &after));
    }

    #[test]
    fn is_raise_covers_all_ten_cases() {
        let base = ClaimRow::entry(sample_draft());

        // 1. verification raised
        let mut c1 = base.clone();
        c1.verification = Verification::PrimarySourceSupported;
        assert!(is_raise(Some(&base), &c1));

        // 2. applicability raised
        let mut c2 = base.clone();
        c2.applicability = Applicability::GeneralReference;
        assert!(is_raise(Some(&base), &c2));

        // 3. adoption raised
        let mut c3 = base.clone();
        c3.adoption = Adoption::PrototypeReviewed;
        assert!(is_raise(Some(&base), &c3));

        // 4. verified_on changed
        let mut c4 = base.clone();
        c4.verified_on = Some(CivilDate::parse("2025-11-04").expect("date"));
        assert!(is_raise(Some(&base), &c4));

        // 5. verified_by_role changed
        let mut c5 = base.clone();
        c5.verified_by_role = Some(VerifiedByRole::Lead);
        assert!(is_raise(Some(&base), &c5));

        // 6. reset to entry value -> false
        assert!(!is_raise(Some(&c1), &base));
        assert!(!is_raise(Some(&c2), &base));
        assert!(!is_raise(Some(&c3), &base));

        // 7. statement change only -> false
        let mut c7 = base.clone();
        c7.statement = "Another statement.".to_owned();
        assert!(!is_raise(Some(&base), &c7));

        // 8. new row in entry state -> false
        assert!(!is_raise(None, &base));

        // 9. new row with verification = primary_source_supported -> true
        assert!(is_raise(None, &c1));

        // 10. new row with data_origin != unknown -> true
        let mut c10 = base;
        c10.data_origin = DataOrigin::Observed;
        assert!(is_raise(None, &c10));
    }

    #[test]
    fn synthetic_rows_of_each_claim_kind_and_problem_kinds() {
        for kind in ClaimKind::ALL {
            let mut draft = sample_draft();
            draft.kind = kind;
            if kind == ClaimKind::Assumption {
                draft.source_id = None;
                draft.version = None;
                draft.locator_kind = None;
                draft.locator = None;
                draft.quote = None;
            }
            let row = ClaimRow::entry(draft);
            let cells = row.to_cells();
            let parsed = ClaimRow::from_row(&row_from(cells.clone())).expect("valid row");
            assert_eq!(parsed, row);
            assert_eq!(parsed.to_cells(), cells);
        }

        // Empty as_of is preserved as None, not CellProblem::Empty
        let mut draft_no_as_of = sample_draft();
        draft_no_as_of.as_of = None;
        let row_no_as_of = ClaimRow::entry(draft_no_as_of);
        let cells_no_as_of = row_no_as_of.to_cells();
        assert_eq!(cells_no_as_of[4], "");
        let parsed_no_as_of =
            ClaimRow::from_row(&row_from(cells_no_as_of)).expect("empty as_of must parse");
        assert!(parsed_no_as_of.as_of.is_none());

        let base_cells = ClaimRow::entry(sample_draft()).to_cells();

        // Problem: Empty statement
        let mut empty_stmt = base_cells.clone();
        empty_stmt[3] = String::new();
        assert_eq!(
            ClaimRow::from_row(&row_from(empty_stmt)).expect_err("empty statement"),
            vec![CellProblem::new("statement", CellProblemKind::Empty)]
        );

        // Problem: Form (bad claim_id)
        let mut bad_id = base_cells.clone();
        bad_id[0] = "CL-CMP-01".to_owned();
        assert_eq!(
            ClaimRow::from_row(&row_from(bad_id)).expect_err("bad claim_id"),
            vec![CellProblem::new("claim_id", CellProblemKind::Form)]
        );

        // Problem: CalendarDate (impossible verified_on)
        let mut bad_date = base_cells.clone();
        bad_date[28] = "2025-02-30".to_owned();
        assert_eq!(
            ClaimRow::from_row(&row_from(bad_date)).expect_err("bad verified_on"),
            vec![CellProblem::new(
                "verified_on",
                CellProblemKind::CalendarDate
            )]
        );

        // Problem: Vocabulary (unit = "dollars" refused; "weeks" and "EUR" accepted)
        let mut bad_unit = base_cells.clone();
        bad_unit[18] = "dollars".to_owned();
        assert_eq!(
            ClaimRow::from_row(&row_from(bad_unit)).expect_err("unit = dollars"),
            vec![CellProblem::new("unit", CellProblemKind::Vocabulary)]
        );

        for valid_u in ["weeks", "EUR"] {
            let mut ok_unit = base_cells.clone();
            ok_unit[18] = valid_u.to_owned();
            let parsed_u = ClaimRow::from_row(&row_from(ok_unit)).expect("valid unit");
            assert_eq!(
                parsed_u.unit.map(|u| u.to_string()).as_deref(),
                Some(valid_u)
            );
        }

        // Problem: TooLong (quote > 300 chars)
        let mut long_quote = base_cells;
        long_quote[9] = "q".repeat(301);
        assert_eq!(
            ClaimRow::from_row(&row_from(long_quote)).expect_err("301-char quote"),
            vec![CellProblem::new("quote", CellProblemKind::TooLong)]
        );
    }

    #[test]
    fn applies_to_reads_these_cells() {
        // 1. Prose with no `=`
        let prose = "fixed-price DoD contracts entered after 2025-01-01";
        assert_eq!(
            AppliesTo::parse(prose),
            Ok(AppliesTo::Prose(prose.to_owned()))
        );

        // 2. Full structured example
        let s1 = "event=award_date; entered_from=2025-01-01; contract_type=fixed_price";
        assert_eq!(
            AppliesTo::parse(s1),
            Ok(AppliesTo::Structured(StructuredAppliesTo {
                event: Some(AppliesToEvent::AwardDate),
                entered_from: Some(PartialDate::parse("2025-01-01").expect("pdate")),
                entered_to: None,
                contract_type: Some("fixed_price".to_owned()),
                fiscal_year: None,
                calendar: None,
                basis: None,
            }))
        );

        // 3. Partial date (year)
        let s2 = "entered_from=2025";
        assert_eq!(
            AppliesTo::parse(s2),
            Ok(AppliesTo::Structured(StructuredAppliesTo {
                entered_from: Some(PartialDate::parse("2025").expect("pdate")),
                ..StructuredAppliesTo::default()
            }))
        );

        // Refusals: each must fail and name the key or the cell
        let refusals: [(&str, &str); 7] = [
            ("event=award_day", "event"),
            ("basis=accrual; basis=cash", "basis"),
            ("color=red", "color"),
            ("entered_from=2025-13-01", "entered_from"),
            (
                "event=award_date;entered_from=2025",
                "event=award_date;entered_from=2025",
            ),
            ("basis=", "basis"),
            ("Basis=fixed", "Basis"),
        ];

        for (cell, expected_name) in refusals {
            let err = AppliesTo::parse(cell)
                .expect_err(&format!("expected applies_to cell {cell:?} to fail"));
            assert!(
                err.target().contains(expected_name) || err.to_string().contains(expected_name),
                "refusal for {cell:?} did not name {expected_name:?}: got {err:?}"
            );
        }
    }

    #[test]
    fn unknown_extension_claim_round_trips() {
        let mut draft = sample_draft();
        draft.locator_kind = Some(LocatorKind::Extension("future_graph_node".to_owned()));
        draft.locator = Some("ext future_graph_node %2Fnodes%2F%CE%B1%3B1%5D%0A".to_owned());
        let row = ClaimRow::entry(draft);
        let cells = row.to_cells();
        let parsed = ClaimRow::from_row(&row_from(cells.clone()))
            .expect("unknown extension kind with valid canonical payload must round-trip");
        assert_eq!(parsed, row);
        assert_eq!(parsed.to_cells(), cells);
    }

    #[test]
    fn extension_kind_and_payload_must_agree() {
        let mut draft = sample_draft();
        draft.locator_kind = Some(LocatorKind::Extension("structured_path".to_owned()));
        draft.locator = Some("ext other_kind %2Fitems%2F0".to_owned());
        let cells = ClaimRow::entry(draft).to_cells();
        let errs = ClaimRow::from_row(&row_from(cells))
            .expect_err("mismatched ext:<kind> and locator kind must fail");
        assert_eq!(
            errs,
            vec![CellProblem::new("locator", CellProblemKind::Form)]
        );
    }
}
