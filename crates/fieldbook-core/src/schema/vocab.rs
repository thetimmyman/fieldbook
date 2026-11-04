//! Closed vocabularies for Corpus pages (and, in later cards, registers and claims).
//!
//! **Responsibility.** Define each page vocabulary once as a typed enum with
//! `ALL`, `as_str` and `parse`, re-export [`Volatility`](crate::date::Volatility),
//! provide [`label_form`] for handling label syntax, and verify in tests that
//! every enum's `ALL` matches its table row in `docs/reference/corpus-format.md`.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub use crate::date::Volatility;

/// Error returned when a string is outside a closed vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VocabError {
    vocabulary: &'static str,
    value: String,
}

impl VocabError {
    /// Creates a new vocabulary error naming the vocabulary and offending value.
    #[must_use]
    pub fn new(vocabulary: &'static str, value: impl Into<String>) -> Self {
        Self {
            vocabulary,
            value: value.into(),
        }
    }

    /// Returns the name of the vocabulary that rejected the value.
    #[must_use]
    pub const fn vocabulary(&self) -> &'static str {
        self.vocabulary
    }

    /// Returns the rejected string value.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

impl fmt::Display for VocabError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "value {:?} is not in vocabulary {}",
            self.value, self.vocabulary
        )
    }
}

impl std::error::Error for VocabError {}

macro_rules! define_vocab_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident ($vocab_str:literal, $count:expr) {
            $(
                $(#[$vmeta:meta])*
                $variant:ident $([overlay = $overlay:tt])? => $str_val:expr
            ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        $vis enum $name {
            $(
                $(#[$vmeta])*
                $variant,
            )+
        }

        impl $name {
            /// Every variant of this vocabulary, in reference table order.
            pub const ALL: [Self; $count] = [
                $(Self::$variant),+
            ];

            /// Returns the canonical string spelling in the Corpus format.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $str_val),+
                }
            }

            /// Returns `true` when this value is reachable only in a company store (FB0210).
            #[must_use]
            pub const fn is_overlay(self) -> bool {
                match self {
                    $(Self::$variant => define_vocab_enum!(@overlay $($overlay)?)),+
                }
            }

            /// Parses a value from its exact Corpus string spelling.
            pub fn parse(input: &str) -> Result<Self, VocabError> {
                for candidate in Self::ALL {
                    if candidate.as_str() == input {
                        return Ok(candidate);
                    }
                }
                Err(VocabError::new($vocab_str, input))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = VocabError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::parse(s)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let s = String::deserialize(deserializer)?;
                Self::parse(&s).map_err(serde::de::Error::custom)
            }
        }
    };
    (@overlay true) => { true };
    (@overlay false) => { false };
    (@overlay) => { false };
}

define_vocab_enum! {
    /// The nine v1 page types (`type` in page front matter; `note` arrives at Day 30 with schema 2).
    pub enum PageType("type", 9) {
        /// Define an idea and its boundaries.
        Concept => "concept",
        /// A public method done in steps.
        Procedure => "procedure",
        /// A lookup of values, rules or lists.
        Reference => "reference",
        /// A public case, read for what it shows.
        Case => "case",
        /// One clause or provision of a regulation.
        ClauseCard => "clause-card",
        /// A calculation `fieldbook-calc` can check.
        Calc => "calc",
        /// Tasks to run through.
        Checklist => "checklist",
        /// A fill-in form shown as text.
        Template => "template",
        /// A one-page decision paper.
        Brief => "brief",
    }
}

define_vocab_enum! {
    /// Page readiness (`readiness` in page front matter).
    pub enum Readiness("readiness", 5) {
        /// Written from public sources and usable before any company answer.
        PublicNow => "PUBLIC-NOW",
        /// Needs made-up, labelled example data to be useful; uses no company data.
        SyntheticNow => "SYNTHETIC-NOW",
        /// Maps what the owner will learn at onboarding; depends on company answers.
        MapOnboarding => "MAP-ONBOARDING",
        /// Depends on a company policy that is not public.
        PolicyGated => "POLICY-GATED",
        /// Depends on the terms of a contract the company holds.
        ContractSpecific => "CONTRACT-SPECIFIC",
    }
}

define_vocab_enum! {
    /// Maturity status of a page (`status` in page front matter).
    pub enum PageStatus("status", 3) {
        /// Being written; may hold open items.
        Draft => "draft",
        /// A person other than the author read it against its claims.
        Reviewed => "reviewed",
        /// The owner accepted it.
        Stable => "stable",
    }
}

define_vocab_enum! {
    /// Change-note entry class (`Change class` in the Corpus format).
    pub enum ChangeClass("change_class", 5) {
        /// A source or fact with no earlier row.
        NewKnowledge => "NEW_KNOWLEDGE",
        /// The same source in a new version.
        SourceRefresh => "SOURCE_REFRESH",
        /// Two sources or rows disagree.
        ClaimConflict => "CLAIM_CONFLICT",
        /// Detail is added and no existing claim changes.
        CompatibleExtension => "COMPATIBLE_EXTENSION",
        /// Company store only: a company contract term or clause changes a calculation.
        MaterialContractChange => "MATERIAL_CONTRACT_CHANGE",
    }
}

/// The four built-in handling label identifiers.
pub const BUILTIN_LABELS: [&str; 4] = ["public", "unlabelled", "marked", "owner-supplied"];

define_vocab_enum! {
    /// Source publisher and document kind (`source_kind` in `sources.csv`).
    pub enum SourceKind("source_kind", 14) {
        /// Statutes, FAR, DFARS and CAS text, Federal Register notices, class deviations, eCFR.
        PrimaryLaw => "primary_law",
        /// Federal award and entity records, contract-award APIs, trade and price statistics.
        OfficialData => "official_data",
        /// Audited annual reports and securities filings of peers and the parent.
        CompanyFiling => "company_filing",
        /// Army and DoD releases, daily contract announcements.
        OfficialAnnouncement => "official_announcement",
        /// NIST publications, audit-agency manuals, accounting standards.
        StandardOrGuidance => "standard_or_guidance",
        /// Peer or parent press releases, investor decks, unaudited releases.
        CompanyStatement => "company_statement",
        /// Commercial defense and trade databases.
        LicensedDatabase => "licensed_database",
        /// Arms-transfer data, customs and trade flows.
        StrategicDataset => "strategic_dataset",
        /// News and trade press.
        TradePress => "trade_press",
        /// ERP vendor manuals and release notes.
        VendorDocumentation => "vendor_documentation",
        /// Made-up test data.
        SyntheticFixture => "synthetic_fixture",
        /// Supplier quotation or proposal letter (company store only).
        VendorQuote [overlay = true] => "vendor_quote",
        /// Record or export from a company system (company store only).
        CompanyRecord [overlay = true] => "company_record",
        /// Company policy, contract, specification, memo or procedure (company store only).
        CompanyDocument [overlay = true] => "company_document",
    }
}

define_vocab_enum! {
    /// Evidentiary role of a source (`source_role` in `sources.csv`).
    pub enum SourceRole("source_role", 6) {
        /// The original text of the thing it describes.
        PrimaryEvidence => "primary_evidence",
        /// A reading by a body whose reading is accepted, such as agency guidance.
        AcceptedInterpretation => "accepted_interpretation",
        /// Made-up data for tests and worked examples.
        SyntheticFixture => "synthetic_fixture",
        /// Not yet reviewed.
        Unknown => "unknown",
        /// Generated synthesis (parsed so `check` can refuse it later).
        GeneratedSynthesis => "generated_synthesis",
        /// Working draft (parsed so `check` can refuse it later).
        WorkingDraft => "working_draft",
    }
}

define_vocab_enum! {
    /// Public publisher weight tier (`authority_tier` in `sources.csv`).
    pub enum AuthorityTier("authority_tier", 4) {
        /// Official: law, official data, official announcements, filings.
        A => "A",
        /// Manufacturer, licensed or standards body.
        B => "B",
        /// Strategic or trade-flow dataset, trade press.
        C => "C",
        /// Discovery only.
        D => "D",
    }
}

define_vocab_enum! {
    /// Confidence that sources in a `source_family` share one original (`family_basis` in `sources.csv`).
    pub enum FamilyBasis("family_basis", 3) {
        /// Not yet reviewed.
        Unknown => "unknown",
        /// Inferred automatically or heuristically.
        Inferred => "inferred",
        /// Reviewed by a person; counts towards corroboration.
        Reviewed => "reviewed",
    }
}

define_vocab_enum! {
    /// Reliance status of a source version (`lifecycle` in `sources.csv`).
    pub enum Lifecycle("lifecycle", 3) {
        /// May be relied on.
        Active => "active",
        /// On hold; only its register row ships.
        Quarantined => "quarantined",
        /// Withdrawn by publisher or owner; kept for the record.
        Withdrawn => "withdrawn",
    }
}

define_vocab_enum! {
    /// Company-store document classification (`doc_kind` in `sources.csv`).
    pub enum DocKind("doc_kind", 11) {
        /// A contract or task order.
        Contract => "contract",
        /// A modification to a contract or task order.
        Modification => "modification",
        /// A supplier quotation or proposal letter.
        Quote => "quote",
        /// A set of rates for a period, with their bases.
        RatePackage => "rate_package",
        /// An estimate-at-completion package for a project.
        EacPackage => "eac_package",
        /// An earned-value export.
        EvmsExport => "evms_export",
        /// A baseline or a log of changes to one.
        BaselineLog => "baseline_log",
        /// A project schedule.
        ScheduleFile => "schedule_file",
        /// A billing or invoice export.
        BillingExport => "billing_export",
        /// A receivables export.
        ReceivablesExport => "receivables_export",
        /// Classified, and none of the words above.
        Other => "other",
    }
}

/// Every value of [`DocKind`], in reference table order.
pub const DOC_KINDS: [DocKind; 11] = DocKind::ALL;

define_vocab_enum! {
    /// Status of a question in `questions.csv` (`question_status`).
    pub enum QuestionStatus("question_status", 3) {
        /// Asked and not answered.
        Open => "open",
        /// No longer asked.
        Withdrawn => "withdrawn",
        /// Answered in a company store (company store only).
        Answered [overlay = true] => "answered",
    }
}

define_vocab_enum! {
    /// Whether work can proceed on the default assumption (`work_can_continue` in `questions.csv`).
    pub enum WorkCanContinue("work_can_continue", 3) {
        /// Work can proceed cleanly on the default assumption.
        Yes => "yes",
        /// Work can proceed in part.
        Partly => "partly",
        /// Work is blocked until answered.
        No => "no",
    }
}

define_vocab_enum! {
    /// Resolution state of a conflict in `conflicts.csv` (`conflict_status`).
    pub enum ConflictStatus("conflict_status", 2) {
        /// Settled by a rule or source.
        Resolved => "resolved",
        /// Open and tied to a question.
        Unresolved => "unresolved",
    }
}

define_vocab_enum! {
    /// Provenance/authority of a glossary definition (`definition_status` in `glossary.csv`).
    pub enum DefinitionStatus("definition_status", 4) {
        /// A regulatory or standards term with a citation in `source_ref`.
        PublicReference => "public_reference",
        /// Accepted finance or engineering usage.
        GeneralPractice => "general_practice",
        /// A modelling rule this project chose.
        DesignConvention => "design_convention",
        /// The company must confirm or replace it.
        PendingCompanyCalibration => "pending_company_calibration",
    }
}

define_vocab_enum! {
    /// Approval state of a claim or question answer (`adoption` in `claims.csv` and `questions.csv`).
    pub enum Adoption("adoption", 6) {
        /// Entry value: offered as the method or number to use.
        Proposed => "proposed",
        /// Reviewed by the lead or owner for use in prototypes and examples.
        PrototypeReviewed => "prototype_reviewed",
        /// Waiting for the company to approve it (company store only).
        PendingCompanyApproval [overlay = true] => "pending_company_approval",
        /// Approved by the company with an approval record (company store only).
        CompanyApproved [overlay = true] => "company_approved",
        /// Considered and not adopted.
        Rejected => "rejected",
        /// Replaced by another claim.
        Superseded => crate::strings::CITE_STATUSES[3],
    }
}

define_vocab_enum! {
    /// Kind of statement in `claims.csv` (`claim_kind`).
    pub enum ClaimKind("claim_kind", 4) {
        /// A statement a source supports: a rule, a number, a date.
        Fact => "fact",
        /// A value the author chose, such as a placeholder rate in a worked example.
        Assumption => "assumption",
        /// How a source's text is read, for example agency guidance.
        Interpretation => "interpretation",
        /// What to do.
        Recommendation => "recommendation",
    }
}

define_vocab_enum! {
    /// Verification state of a claim in `claims.csv` (`verification`).
    pub enum Verification("verification", 5) {
        /// Entry value: no one has checked the claim against a primary source.
        Unchecked => "unchecked",
        /// A person read the original at the locator and it supports the claim.
        PrimarySourceSupported => "primary_source_supported",
        /// Primary text for the claim's own scope and date conflicts with it.
        Contradicted => "contradicted",
        /// The claim was right for its date and the instrument has since changed, or review date passed.
        Stale => "stale",
        /// Two primary sources disagree, or none can be reached.
        Unresolved => "unresolved",
    }
}

define_vocab_enum! {
    /// Company applicability state of a claim in `claims.csv` (`applicability`).
    pub enum Applicability("applicability", 5) {
        /// Holds in general and makes no claim about the company.
        GeneralReference => "general_reference",
        /// A stand-in used in an example until a company value arrives.
        PrototypeAssumption => "prototype_assumption",
        /// Entry value for facts: may apply to the company; only the company can confirm.
        PendingCompanyConfirmation => "pending_company_confirmation",
        /// Confirmed by a named role at the company (company store only).
        ConfirmedApplicable [overlay = true] => "confirmed_applicable",
        /// Known not to apply: replaced text, or out of scope.
        NotApplicable => "not_applicable",
    }
}

define_vocab_enum! {
    /// How a claim's value came to exist (`data_origin` in `claims.csv`).
    pub enum DataOrigin("data_origin", 4) {
        /// Read directly from a source.
        Observed => "observed",
        /// Computed or judged by the author from other numbers.
        Estimated => "estimated",
        /// Concluded from facts that do not state it.
        Inferred => "inferred",
        /// Entry value: the origin is not established.
        Unknown => "unknown",
    }
}

define_vocab_enum! {
    /// The role that drafted a claim row (`drafted_by` in `claims.csv`).
    pub enum DraftedBy("drafted_by", 4) {
        /// The repository owner.
        Owner => "owner",
        /// The project lead.
        Lead => "lead",
        /// An automated agent (`codex`).
        Codex => "codex",
        /// Text produced by an assistant outside the repository and pasted in (Day 30).
        Ai => "ai",
    }
}

define_vocab_enum! {
    /// The role that verified a claim row (`verified_by_role` in `claims.csv`).
    pub enum VerifiedByRole("verified_by_role", 2) {
        /// The repository owner.
        Owner => "owner",
        /// The project lead.
        Lead => "lead",
    }
}

define_vocab_enum! {
    /// Built-in locator kinds in `claims.csv` (`locator_kind`; extensions use [`super::locator::LocatorKind`]).
    pub enum BuiltinLocatorKind("locator_kind", 6) {
        /// PDF page or page range (`p.`, `pp.`).
        PdfPageBox => "pdf_page_box",
        /// Text section, section sign or paragraph (`sec.`, `§`, `para.`).
        TextSection => "text_section",
        /// Spreadsheet sheet and cell/range (`sheet`).
        SheetCells => "sheet_cells",
        /// Record row key (`rec`).
        RecordKey => "record_key",
        /// No fine locator (`no_fine_locator`).
        NoFineLocator => "no_fine_locator",
        /// Record field column (`field`, company store only).
        RecordField [overlay = true] => "record_field",
    }
}

define_vocab_enum! {
    /// Regulatory text status (`text_status` in `claims.csv`).
    pub enum TextStatus("text_status", 3) {
        /// The text in force.
        Final => "final",
        /// Class-deviation or model-deviation text differing from codified text.
        DeviationText => "deviation_text",
        /// A proposed rule; not final.
        Proposed => "proposed",
    }
}

define_vocab_enum! {
    /// Regulatory instrument (`instrument` in `claims.csv`).
    pub enum Instrument("instrument", 8) {
        /// Codified FAR.
        FarCodified => "far_codified",
        /// Codified DFARS.
        DfarsCodified => "dfars_codified",
        /// Cost Accounting Standards (48 CFR 99xx).
        Cas => "cas",
        /// Revolutionary FAR Overhaul model deviation text.
        RfoModelText => "rfo_model_text",
        /// A DoD class deviation, with its number in `citation_rfo`.
        ClassDeviation => "class_deviation",
        /// A proposed rule, with the FAR case or docket in `citation_rfo`.
        ProposedRule => "proposed_rule",
        /// A Federal Register notice.
        FederalRegisterNotice => "federal_register_notice",
        /// A statute.
        Statute => "statute",
    }
}

define_vocab_enum! {
    /// Support grade for a calculation input (`evidence_grade` in `claims.csv`).
    pub enum EvidenceGrade("evidence_grade", 7) {
        /// An official public benchmark.
        E => "E",
        /// An industry secondary source.
        F => "F",
        /// Management judgment (requires `replace by ...` note).
        G => "G",
        /// Company actual (company store only).
        A [overlay = true] => "A",
        /// Company committed (company store only).
        B [overlay = true] => "B",
        /// Company estimate, unapproved (company store only).
        C [overlay = true] => "C",
        /// Parent or analogous operation, adjusted (company store only).
        D [overlay = true] => "D",
    }
}

/// Returns `true` when `input` matches the stream/series identifier form
/// `^[a-z][a-z0-9-]{0,31}$` (1 to 32 ASCII bytes).
#[must_use]
pub fn stream_id_form(input: &str) -> bool {
    let bytes = input.as_bytes();
    let Some((&first, rest)) = bytes.split_first() else {
        return false;
    };
    first.is_ascii_lowercase()
        && rest.len() <= 31
        && rest
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

/// Returns `true` when `input` is one of the four built-in handling labels
/// (`public`, `unlabelled`, `marked`, `owner-supplied`) or matches the opaque
/// company label pattern `^[a-z]{2,4}-[0-9]{4}$`.
#[must_use]
pub fn label_form(input: &str) -> bool {
    if BUILTIN_LABELS.contains(&input) {
        return true;
    }
    let Some((prefix, digits)) = input.split_once('-') else {
        return false;
    };
    (2..=4).contains(&prefix.len())
        && prefix.bytes().all(|b| b.is_ascii_lowercase())
        && digits.len() == 4
        && digits.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CORPUS_FORMAT_MD: &str = include_str!("../../../../docs/reference/corpus-format.md");

    fn extract_backticked(cell: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = cell;
        while let Some(start) = rest.find('`') {
            let after_start = &rest[start + 1..];
            let Some(end) = after_start.find('`') else {
                break;
            };
            let token = &after_start[..end];
            if !token.is_empty() {
                out.push(token.to_owned());
            }
            rest = &after_start[end + 1..];
        }
        out
    }

    fn strip_backticks(s: &str) -> String {
        s.chars().filter(|&c| c != '`').collect()
    }

    fn reference_vocab_values(doc: &str, vocab_key: &str) -> Vec<String> {
        if vocab_key == "type" {
            // Read the `type` row in the Front matter fields table of `## Page files`,
            // taking the v1 values before `. \`note\` joins`.
            let mut in_page_files = false;
            for line in doc.lines() {
                if line.starts_with("## Page files") {
                    in_page_files = true;
                    continue;
                }
                if in_page_files && line.starts_with("## ") {
                    break;
                }
                if in_page_files && line.starts_with('|') {
                    let cells: Vec<&str> = line
                        .trim()
                        .trim_matches('|')
                        .split('|')
                        .map(str::trim)
                        .collect();
                    if cells.len() == 4 && cells[0] == "`type`" {
                        let allowed_v1 = cells[3].split('.').next().unwrap_or(cells[3]);
                        return extract_backticked(allowed_v1);
                    }
                }
            }
            return Vec::new();
        }

        // Look in `## Status vocabularies and promotion rules`.
        let Some(start_idx) = doc.find("## Status vocabularies and promotion rules") else {
            return Vec::new();
        };
        let section = &doc[start_idx..];
        let mut lines = section.lines().skip(1).peekable();

        while let Some(line) = lines.next() {
            if line.starts_with("## ") {
                break;
            }
            if !line.trim_start().starts_with('|') {
                continue;
            }
            let header_cells: Vec<&str> = line
                .trim()
                .trim_matches('|')
                .split('|')
                .map(str::trim)
                .collect();
            // Skip separator line `|---|...|`
            if lines
                .peek()
                .is_some_and(|next| next.trim_start().starts_with("|---"))
            {
                let _ = lines.next();
            }

            let first_header = strip_backticks(header_cells.first().copied().unwrap_or(""));
            if first_header == vocab_key {
                // Table where column 0 of each row holds the vocabulary value(s),
                // such as `| Change class | Used when |` or `| source_kind | ... |`.
                let mut values = Vec::new();
                while let Some(&row_line) = lines.peek() {
                    if !row_line.trim_start().starts_with('|') {
                        break;
                    }
                    let _ = lines.next();
                    let cells: Vec<&str> = row_line
                        .trim()
                        .trim_matches('|')
                        .split('|')
                        .map(str::trim)
                        .collect();
                    if let Some(&first_cell) = cells.first() {
                        values.extend(extract_backticked(first_cell));
                    }
                }
                return values;
            }

            if first_header == "Field" && header_cells.get(1) == Some(&"Value") {
                let mut active = false;
                let mut values = Vec::new();
                while let Some(&row_line) = lines.peek() {
                    if !row_line.trim_start().starts_with('|') {
                        break;
                    }
                    let _ = lines.next();
                    let cells: Vec<&str> = row_line
                        .trim()
                        .trim_matches('|')
                        .split('|')
                        .map(str::trim)
                        .collect();
                    if cells.len() < 2 {
                        continue;
                    }
                    let field_cell = strip_backticks(cells[0]);
                    let field_trimmed = field_cell.trim();
                    if !field_trimmed.is_empty() {
                        if active {
                            return values;
                        }
                        if field_trimmed == vocab_key
                            || field_trimmed
                                .strip_prefix(vocab_key)
                                .is_some_and(|rest| rest.starts_with(' '))
                        {
                            active = true;
                        }
                    }
                    if active {
                        let cell_to_scan = if vocab_key == "locator_kind" {
                            cells[1].split(';').next().unwrap_or(cells[1])
                        } else {
                            cells[1]
                        };
                        values.extend(extract_backticked(cell_to_scan));
                    }
                }
                if active {
                    return values;
                }
            }
        }
        Vec::new()
    }

    fn assert_vocab_matches(vocab_name: &str, doc_key: &str, code_values: &[&str]) {
        let doc_values = reference_vocab_values(CORPUS_FORMAT_MD, doc_key);
        assert!(
            !doc_values.is_empty(),
            "vocabulary {vocab_name} ({doc_key}) was not found in corpus-format.md"
        );
        let doc_strs: Vec<&str> = doc_values.iter().map(String::as_str).collect();
        assert_eq!(
            code_values,
            doc_strs.as_slice(),
            "vocabulary {vocab_name} does not match corpus-format.md row {doc_key:?}"
        );
    }

    #[test]
    fn every_enum_matches_the_reference() {
        assert_vocab_matches("type", "type", &PageType::ALL.map(PageType::as_str));
        assert_vocab_matches(
            "readiness",
            "readiness",
            &Readiness::ALL.map(Readiness::as_str),
        );
        assert_vocab_matches(
            "page status",
            "page status",
            &PageStatus::ALL.map(PageStatus::as_str),
        );
        assert_vocab_matches(
            "volatility",
            "volatility",
            &Volatility::ALL.map(Volatility::as_str),
        );
        assert_vocab_matches(
            "change_class",
            "Change class",
            &ChangeClass::ALL.map(ChangeClass::as_str),
        );
        assert_vocab_matches(
            "source_kind",
            "source_kind",
            &SourceKind::ALL.map(SourceKind::as_str),
        );
        assert_vocab_matches(
            "source_role",
            "source_role",
            &SourceRole::ALL.map(SourceRole::as_str),
        );
        assert_vocab_matches(
            "authority_tier",
            "authority_tier",
            &AuthorityTier::ALL.map(AuthorityTier::as_str),
        );
        assert_vocab_matches(
            "family_basis",
            "family_basis",
            &FamilyBasis::ALL.map(FamilyBasis::as_str),
        );
        assert_vocab_matches(
            "lifecycle",
            "lifecycle",
            &Lifecycle::ALL.map(Lifecycle::as_str),
        );
        assert_vocab_matches("doc_kind", "doc_kind", &DOC_KINDS.map(DocKind::as_str));
        assert_vocab_matches(
            "question_status",
            "question_status",
            &QuestionStatus::ALL.map(QuestionStatus::as_str),
        );
        assert_vocab_matches(
            "work_can_continue",
            "work_can_continue",
            &WorkCanContinue::ALL.map(WorkCanContinue::as_str),
        );
        assert_vocab_matches(
            "conflict_status",
            "conflict_status",
            &ConflictStatus::ALL.map(ConflictStatus::as_str),
        );
        assert_vocab_matches(
            "definition_status",
            "definition_status",
            &DefinitionStatus::ALL.map(DefinitionStatus::as_str),
        );
        assert_vocab_matches("adoption", "adoption", &Adoption::ALL.map(Adoption::as_str));
        assert_vocab_matches(
            "claim_kind",
            "claim_kind",
            &ClaimKind::ALL.map(ClaimKind::as_str),
        );
        assert_vocab_matches(
            "verification",
            "verification",
            &Verification::ALL.map(Verification::as_str),
        );
        assert_vocab_matches(
            "applicability",
            "applicability",
            &Applicability::ALL.map(Applicability::as_str),
        );
        assert_vocab_matches(
            "data_origin",
            "data_origin",
            &DataOrigin::ALL.map(DataOrigin::as_str),
        );
        assert_vocab_matches(
            "drafted_by",
            "drafted_by",
            &DraftedBy::ALL.map(DraftedBy::as_str),
        );
        assert_vocab_matches(
            "verified_by_role",
            "verified_by_role",
            &VerifiedByRole::ALL.map(VerifiedByRole::as_str),
        );
        assert_vocab_matches(
            "locator_kind",
            "locator_kind",
            &BuiltinLocatorKind::ALL.map(BuiltinLocatorKind::as_str),
        );
        assert_vocab_matches(
            "text_status",
            "text_status",
            &TextStatus::ALL.map(TextStatus::as_str),
        );
        assert_vocab_matches(
            "instrument",
            "instrument",
            &Instrument::ALL.map(Instrument::as_str),
        );
        assert_vocab_matches(
            "evidence_grade",
            "evidence_grade",
            &EvidenceGrade::ALL.map(EvidenceGrade::as_str),
        );
    }

    #[test]
    fn overlay_values_are_marked() {
        use std::collections::BTreeSet;

        let mut actual = BTreeSet::new();
        macro_rules! collect_overlays {
            ($($ty:ty),+ $(,)?) => {
                $(
                    for v in <$ty>::ALL {
                        if v.is_overlay() {
                            actual.insert(v.as_str());
                        }
                    }
                )+
            };
        }

        collect_overlays!(
            PageType,
            Readiness,
            PageStatus,
            ChangeClass,
            SourceKind,
            SourceRole,
            AuthorityTier,
            FamilyBasis,
            Lifecycle,
            DocKind,
            QuestionStatus,
            WorkCanContinue,
            ConflictStatus,
            DefinitionStatus,
            Adoption,
            ClaimKind,
            Verification,
            Applicability,
            DataOrigin,
            DraftedBy,
            VerifiedByRole,
            BuiltinLocatorKind,
            TextStatus,
            Instrument,
            EvidenceGrade,
        );

        let expected: BTreeSet<&str> = [
            "confirmed_applicable",
            "record_field",
            "A",
            "B",
            "C",
            "D",
            "answered",
            "vendor_quote",
            "company_record",
            "company_document",
            "pending_company_approval",
            "company_approved",
        ]
        .into_iter()
        .collect();

        assert_eq!(actual, expected);
    }

    #[test]
    fn stream_id_form_and_doc_kind_parse_rules() {
        assert!(DocKind::parse("memo").is_err());
        assert_eq!(DocKind::parse("quote"), Ok(DocKind::Quote));
        assert!(stream_id_form("commitments"));
        assert!(stream_id_form("a"));
        assert!(stream_id_form("a1234567890123456789012345678901")); // 32 chars
        assert!(!stream_id_form(""));
        assert!(!stream_id_form("Bad Name"));
        assert!(!stream_id_form("1commitments"));
        assert!(!stream_id_form("a12345678901234567890123456789012")); // 33 chars
    }

    #[test]
    fn label_form_accepts_builtins_and_opaque_company_labels() {
        for builtin in BUILTIN_LABELS {
            assert!(
                label_form(builtin),
                "expected builtin {builtin} to be valid"
            );
        }
        assert!(label_form("np-0007"));
        assert!(label_form("acme-1234"));
        assert!(!label_form(""));
        assert!(!label_form("PUBLIC"));
        assert!(!label_form("a-0001"));
        assert!(!label_form("abcde-0001"));
        assert!(!label_form("np-001"));
        assert!(!label_form("np-00001"));
        assert!(!label_form("NP-0007"));
    }
}
