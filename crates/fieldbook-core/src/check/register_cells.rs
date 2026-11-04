//! Pass `register_cells`: `FB0114`, `FB0204`, `FB0205`, `FB0206` and `FB0210` (register cell
//! types, vocabularies, lengths, calendar dates, overlay-only values and rules across columns)
//! (`docs/reference/corpus-format.md#registers`).

use std::collections::BTreeSet;

use crate::check::registers::register_path;
use crate::check::{Code, Diagnostic, Model, Options, Report};
use crate::ids::{AreaCode, ClaimId};
use crate::schema::Register;
use crate::schema::locator::LocatorKind;
use crate::schema::rows::{CellProblem, CellProblemKind};
use crate::schema::table::Row;
use crate::schema::vocab::{
    Adoption, Applicability, AuthorityTier, ClaimKind, ConflictStatus, DefinitionStatus,
    EvidenceGrade, Instrument, QuestionStatus, SourceKind, SourceRole, Verification,
};

/// One predicate within a rule across columns (`docs/reference/corpus-format.md#registers`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RulePredicate {
    /// Diagnostic code raised when this predicate is broken, or `None` for a `Convention` row.
    pub code: Option<Code>,
    /// The check pass or card that owns this predicate.
    pub owner: &'static str,
}

impl RulePredicate {
    /// Construct a [`RulePredicate`] with `code` and `owner`.
    #[must_use]
    pub const fn new(code: Option<Code>, owner: &'static str) -> Self {
        Self { code, owner }
    }
}

/// One entry in [`RULES`]: one row of a "Rules across columns" table or one rule sentence under
/// a register in `docs/reference/corpus-format.md#registers`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleEntry {
    /// The register whose "Rules across columns" lists this rule.
    pub register: Register,
    /// Rule text or `If` condition from the reference.
    pub rule: &'static str,
    /// Predicates and owning passes/cards for this rule.
    pub predicates: &'static [RulePredicate],
}

/// All rules across columns for the five registers (`docs/reference/corpus-format.md#registers`).
pub const RULES: &[RuleEntry] = &[
    // sources.csv (10 table rows)
    RuleEntry {
        register: Register::Sources,
        rule: "`source_family` is set exactly when `family_basis` is set",
        predicates: &[RulePredicate::new(Some(Code::Fb0206), "register_cells")],
    },
    RuleEntry {
        register: Register::Sources,
        rule: "`authority_tier` is empty exactly when `source_kind` is `synthetic_fixture` or a company kind, and otherwise one of the tiers the kind allows",
        predicates: &[
            RulePredicate::new(Some(Code::Fb0206), "register_cells"),
            RulePredicate::new(Some(Code::Fb0204), "register_cells"),
        ],
    },
    RuleEntry {
        register: Register::Sources,
        rule: "A `published`, `effective_from` or `as_of` that is empty for a reason the cell cannot show says why in `note`",
        predicates: &[RulePredicate::new(None, "convention")],
    },
    RuleEntry {
        register: Register::Sources,
        rule: "Each id in `supersedes` is an existing version, not this one, and the chain has no loop",
        predicates: &[
            RulePredicate::new(Some(Code::Fb0207), "check-register-links"),
            RulePredicate::new(Some(Code::Fb0208), "check-register-links"),
        ],
    },
    RuleEntry {
        register: Register::Sources,
        rule: "`origin_system`, `origin_stream` and `origin_key` are set together, or all three are empty. A set `origin_system` names a system the store declares",
        predicates: &[RulePredicate::new(Some(Code::Fb0215), "r2-pull-and-sync")],
    },
    RuleEntry {
        register: Register::Sources,
        rule: "For ordinary documents, each version of an `origin_key` after the first names the one it replaces in `supersedes`, and a revision stream’s `as_of` does not go back. Validated record-load observation sources retain empty `supersedes` and original dates; observation manifests select current evidence. Missing or malformed observation bindings do not qualify for this exception. Two sources of one stream never share an `origin_key`; in a vintage stream the key includes the `as_of`",
        predicates: &[RulePredicate::new(Some(Code::Fb0216), "r2-pull-and-sync")],
    },
    RuleEntry {
        register: Register::Sources,
        rule: "`doc_kind` is empty or a word of the `doc_kind` vocabulary, `series` is empty or has the form of a stream id (`[a-z][a-z0-9-]{0,31}`), and both are empty in the repository Corpus",
        predicates: &[RulePredicate::new(Some(Code::Fb0215), "r2-pull-and-sync")],
    },
    RuleEntry {
        register: Register::Sources,
        rule: "A stored original `corpus/sources/public/<version-id>.<ext>` (in a company store, `sources/org/<version-id>.<ext>`) exists and its bytes hash to `sha256`. A `metadata-only` version has no such file",
        predicates: &[RulePredicate::new(
            Some(Code::Fb0211),
            "check-register-links",
        )],
    },
    RuleEntry {
        register: Register::Sources,
        rule: "A claim that is `primary_source_supported` rests on a version whose `lifecycle` is `active`",
        predicates: &[RulePredicate::new(Some(Code::Fb0213), "check-promotion")],
    },
    RuleEntry {
        register: Register::Sources,
        rule: "A `quarantined` version says why in `note`",
        predicates: &[RulePredicate::new(None, "convention")],
    },
    // claims.csv (18 table rows)
    RuleEntry {
        register: Register::Claims,
        rule: "`kind` is `fact` or `interpretation`",
        predicates: &[
            RulePredicate::new(Some(Code::Fb0206), "register_cells"),
            RulePredicate::new(Some(Code::Fb0207), "check-register-links"),
        ],
    },
    RuleEntry {
        register: Register::Claims,
        rule: "`kind` is `assumption`",
        predicates: &[RulePredicate::new(Some(Code::Fb0206), "register_cells")],
    },
    RuleEntry {
        register: Register::Claims,
        rule: "`locator_kind` is `no_fine_locator`",
        predicates: &[
            RulePredicate::new(Some(Code::Fb0206), "register_cells"),
            RulePredicate::new(Some(Code::Fb0306), "check-claim-cells"),
        ],
    },
    RuleEntry {
        register: Register::Claims,
        rule: "`locator_kind` is another kind",
        predicates: &[RulePredicate::new(Some(Code::Fb0206), "register_cells")],
    },
    RuleEntry {
        register: Register::Claims,
        rule: "`locator_kind` is `record_field`",
        predicates: &[
            RulePredicate::new(Some(Code::Fb0207), "r2-records"),
            RulePredicate::new(Some(Code::Fb0210), "register_cells"),
        ],
    },
    RuleEntry {
        register: Register::Claims,
        rule: "`evidence_grade` is `G`",
        predicates: &[RulePredicate::new(Some(Code::Fb0206), "register_cells")],
    },
    RuleEntry {
        register: Register::Claims,
        rule: "`consequential` is `true`",
        predicates: &[RulePredicate::new(Some(Code::Fb0307), "check-claim-cells")],
    },
    RuleEntry {
        register: Register::Claims,
        rule: "`value` is set",
        predicates: &[RulePredicate::new(Some(Code::Fb0206), "register_cells")],
    },
    RuleEntry {
        register: Register::Claims,
        rule: "`verification` is `unchecked`",
        predicates: &[RulePredicate::new(Some(Code::Fb0206), "register_cells")],
    },
    RuleEntry {
        register: Register::Claims,
        rule: "`verification` is `primary_source_supported` or `contradicted`",
        predicates: &[RulePredicate::new(Some(Code::Fb0206), "register_cells")],
    },
    RuleEntry {
        register: Register::Claims,
        rule: "`verification` is `unresolved`",
        predicates: &[RulePredicate::new(Some(Code::Fb0206), "register_cells")],
    },
    RuleEntry {
        register: Register::Claims,
        rule: "`verification` is `contradicted`",
        predicates: &[RulePredicate::new(Some(Code::Fb0206), "register_cells")],
    },
    RuleEntry {
        register: Register::Claims,
        rule: "`verification` is `stale`",
        predicates: &[RulePredicate::new(Some(Code::Fb0206), "register_cells")],
    },
    RuleEntry {
        register: Register::Claims,
        rule: "`as_of` is empty",
        predicates: &[RulePredicate::new(Some(Code::Fb0206), "register_cells")],
    },
    RuleEntry {
        register: Register::Claims,
        rule: "`instrument` is `class_deviation`, `rfo_model_text` or `proposed_rule`",
        predicates: &[RulePredicate::new(Some(Code::Fb0206), "register_cells")],
    },
    RuleEntry {
        register: Register::Claims,
        rule: "`text_status` is set",
        predicates: &[RulePredicate::new(Some(Code::Fb0206), "register_cells")],
    },
    RuleEntry {
        register: Register::Claims,
        rule: "`superseded_by` is set",
        predicates: &[
            RulePredicate::new(Some(Code::Fb0206), "register_cells"),
            RulePredicate::new(Some(Code::Fb0208), "check-register-links"),
        ],
    },
    RuleEntry {
        register: Register::Claims,
        rule: "`conflict_id` is set",
        predicates: &[RulePredicate::new(
            Some(Code::Fb0207),
            "check-register-links",
        )],
    },
    // questions.csv (2 rule sentences)
    RuleEntry {
        register: Register::Questions,
        rule: "a row with `day_one` set to `true` has cells 4 to 8 filled",
        predicates: &[RulePredicate::new(Some(Code::Fb0230), "check-promotion")],
    },
    RuleEntry {
        register: Register::Questions,
        rule: "A value in `answer`, `answered_on` or `adoption`, or a `status` of `answered`, is a company-store-only value, refused in the repository Corpus",
        predicates: &[RulePredicate::new(Some(Code::Fb0210), "register_cells")],
    },
    // conflicts.csv (2 rule sentences)
    RuleEntry {
        register: Register::Conflicts,
        rule: "a `resolved` row has cells 5 to 8 filled and an `unresolved` row has `question_id` filled",
        predicates: &[RulePredicate::new(Some(Code::Fb0206), "register_cells")],
    },
    RuleEntry {
        register: Register::Conflicts,
        rule: "Each id in `claim_ids` and `evidence` exists",
        predicates: &[RulePredicate::new(
            Some(Code::Fb0207),
            "check-register-links",
        )],
    },
    // glossary.csv (2 rule sentences)
    RuleEntry {
        register: Register::Glossary,
        rule: "a `public_reference` row has `source_ref` filled",
        predicates: &[RulePredicate::new(Some(Code::Fb0206), "register_cells")],
    },
    RuleEntry {
        register: Register::Glossary,
        rule: "A label or alias is unique across the whole glossary",
        predicates: &[RulePredicate::new(
            Some(Code::Fb0214),
            "check-register-links",
        )],
    },
];

/// Run the `register_cells` check pass (`FB0114`, `FB0204`, `FB0205`, `FB0206`, `FB0210`) over `model`.
pub fn check(model: &Model, options: &Options, report: &mut Report) {
    if model.corpus().is_none() {
        return;
    }

    for reg in Register::ALL {
        let rows = model.register_rows(reg);
        if rows.is_empty() {
            continue;
        }
        let problems_by_line = model.register_problems(reg);
        for row in rows {
            let row_problems = problems_by_line
                .iter()
                .find(|(line, _)| *line == row.line)
                .map_or(&[][..], |(_, probs)| probs.as_slice());
            check_row(reg, row, row_problems, options.strict, report);
        }
    }
}

fn is_key_column(reg: Register, col_name: &str) -> bool {
    match reg {
        Register::Sources => matches!(col_name, "source_id" | "version"),
        Register::Claims => col_name == "claim_id",
        Register::Questions => col_name == "question_id",
        Register::Conflicts => col_name == "conflict_id",
        Register::Glossary => col_name == "term_id",
    }
}

fn col_index(reg: Register, col_name: &str) -> usize {
    reg.columns()
        .iter()
        .position(|c| c.name == col_name)
        .unwrap_or(0)
}

fn col_1based(reg: Register, row: &Row, col_name: &str) -> usize {
    let idx = col_index(reg, col_name);
    row.cols
        .get(idx)
        .copied()
        .and_then(|c| usize::try_from(c).ok())
        .unwrap_or(1)
}

fn cell_str<'a>(reg: Register, row: &'a Row, col_name: &str) -> &'a str {
    let idx = col_index(reg, col_name);
    row.cells.get(idx).map_or("", String::as_str)
}

fn push_cell_diag(
    reg: Register,
    row: &Row,
    col_name: &str,
    code: Code,
    strict: bool,
    message: String,
    report: &mut Report,
) {
    let line = usize::try_from(row.line).unwrap_or(1);
    let col = col_1based(reg, row, col_name);
    report.push(Diagnostic::new(
        code,
        strict,
        register_path(reg),
        line,
        col,
        message,
    ));
}

fn check_row(
    reg: Register,
    row: &Row,
    problems: &[CellProblem],
    strict: bool,
    report: &mut Report,
) {
    let mut bad_cols: BTreeSet<&'static str> = BTreeSet::new();

    for problem in problems {
        // Key columns were already checked and reported by `registers::check` (FB0203).
        if is_key_column(reg, problem.column) {
            bad_cols.insert(problem.column);
            continue;
        }
        // An empty `locator` on a claim with `ext:<kind>` is decided by the cross-column
        // locator rule (FB0206), not FB0204.
        if reg == Register::Claims
            && problem.column == "locator"
            && cell_str(reg, row, "locator").is_empty()
        {
            continue;
        }

        bad_cols.insert(problem.column);
        let raw = cell_str(reg, row, problem.column);
        let (code, msg) = match problem.kind {
            CellProblemKind::CalendarDate => (
                Code::Fb0114,
                format!("invalid calendar date `{raw}` in `{}`", problem.column),
            ),
            CellProblemKind::Empty => (
                Code::Fb0205,
                format!("required cell `{}` is empty", problem.column),
            ),
            CellProblemKind::Form => (
                Code::Fb0204,
                format!("cell `{}` has invalid form `{raw}`", problem.column),
            ),
            CellProblemKind::Vocabulary => (
                Code::Fb0204,
                format!(
                    "cell `{}` value `{raw}` is outside its vocabulary",
                    problem.column
                ),
            ),
            CellProblemKind::TooLong => (
                Code::Fb0204,
                format!("cell `{}` exceeds maximum length", problem.column),
            ),
        };
        push_cell_diag(reg, row, problem.column, code, strict, msg, report);
    }

    match reg {
        Register::Sources => check_source_row(row, &mut bad_cols, strict, report),
        Register::Claims => check_claim_row(row, &mut bad_cols, strict, report),
        Register::Questions => check_question_row(row, &mut bad_cols, strict, report),
        Register::Conflicts => check_conflict_row(row, &bad_cols, strict, report),
        Register::Glossary => check_glossary_row(row, &bad_cols, strict, report),
    }
}

fn source_role_used_in_corpus(sr: SourceRole) -> bool {
    !matches!(
        sr,
        SourceRole::GeneratedSynthesis | SourceRole::WorkingDraft
    )
}

pub(crate) fn source_kind_allows_tier(sk: SourceKind, tier: AuthorityTier) -> bool {
    match sk {
        SourceKind::PrimaryLaw
        | SourceKind::OfficialData
        | SourceKind::CompanyFiling
        | SourceKind::OfficialAnnouncement => matches!(tier, AuthorityTier::A | AuthorityTier::D),
        SourceKind::StandardOrGuidance => {
            matches!(tier, AuthorityTier::A | AuthorityTier::B | AuthorityTier::D)
        }
        SourceKind::CompanyStatement => {
            matches!(tier, AuthorityTier::B | AuthorityTier::C | AuthorityTier::D)
        }
        SourceKind::LicensedDatabase | SourceKind::VendorDocumentation => {
            matches!(tier, AuthorityTier::B | AuthorityTier::D)
        }
        SourceKind::StrategicDataset | SourceKind::TradePress => {
            matches!(tier, AuthorityTier::C | AuthorityTier::D)
        }
        SourceKind::SyntheticFixture
        | SourceKind::VendorQuote
        | SourceKind::CompanyRecord
        | SourceKind::CompanyDocument => false,
    }
}

const fn instrument_requires_rfo_dual_numbering(inst: Instrument) -> bool {
    matches!(
        inst,
        Instrument::ClassDeviation | Instrument::RfoModelText | Instrument::ProposedRule
    )
}

fn check_source_row(
    row: &Row,
    bad_cols: &mut BTreeSet<&'static str>,
    strict: bool,
    report: &mut Report,
) {
    let reg = Register::Sources;

    let raw_kind = cell_str(reg, row, "source_kind");
    let parsed_kind = SourceKind::parse(raw_kind).ok();
    if let Some(sk) = parsed_kind
        && sk.is_overlay()
    {
        bad_cols.insert("source_kind");
        push_cell_diag(
            reg,
            row,
            "source_kind",
            Code::Fb0210,
            strict,
            format!("`source_kind` value `{raw_kind}` is company-store only"),
            report,
        );
    }

    let raw_role = cell_str(reg, row, "source_role");
    if let Ok(sr) = SourceRole::parse(raw_role)
        && !source_role_used_in_corpus(sr)
    {
        bad_cols.insert("source_role");
        push_cell_diag(
            reg,
            row,
            "source_role",
            Code::Fb0204,
            strict,
            format!("`source_role` value `{raw_role}` is not used in the Corpus"),
            report,
        );
    }

    // Rule 1: `source_family` is set exactly when `family_basis` is set (FB0206).
    if !bad_cols.contains("source_family") && !bad_cols.contains("family_basis") {
        let has_family = !cell_str(reg, row, "source_family").is_empty();
        let has_basis = !cell_str(reg, row, "family_basis").is_empty();
        if has_family && !has_basis {
            push_cell_diag(
                reg,
                row,
                "family_basis",
                Code::Fb0206,
                strict,
                "`family_basis` must be set when `source_family` is set".to_owned(),
                report,
            );
        } else if !has_family && has_basis {
            push_cell_diag(
                reg,
                row,
                "source_family",
                Code::Fb0206,
                strict,
                "`source_family` must be set when `family_basis` is set".to_owned(),
                report,
            );
        }
    }

    // Rule 2: `authority_tier` is empty exactly when `source_kind` is `synthetic_fixture` or a
    // company kind, and otherwise one of the tiers the kind allows (FB0206, FB0204).
    if let Some(sk) = parsed_kind
        && !bad_cols.contains("authority_tier")
    {
        let raw_tier = cell_str(reg, row, "authority_tier");
        let parsed_tier = if raw_tier.is_empty() {
            None
        } else {
            AuthorityTier::parse(raw_tier).ok()
        };
        let untiered = sk == SourceKind::SyntheticFixture || sk.is_overlay();
        if untiered {
            if parsed_tier.is_some() {
                push_cell_diag(
                    reg,
                    row,
                    "authority_tier",
                    Code::Fb0206,
                    strict,
                    format!(
                        "`authority_tier` must be empty when `source_kind` is `{}`",
                        sk.as_str()
                    ),
                    report,
                );
            }
        } else if let Some(tier) = parsed_tier {
            if !source_kind_allows_tier(sk, tier) {
                bad_cols.insert("authority_tier");
                push_cell_diag(
                    reg,
                    row,
                    "authority_tier",
                    Code::Fb0204,
                    strict,
                    format!(
                        "`authority_tier` `{raw_tier}` is not allowed for `source_kind` `{}`",
                        sk.as_str()
                    ),
                    report,
                );
            }
        } else {
            push_cell_diag(
                reg,
                row,
                "authority_tier",
                Code::Fb0206,
                strict,
                format!(
                    "`authority_tier` is required when `source_kind` is `{}`",
                    sk.as_str()
                ),
                report,
            );
        }
    }
}

fn locator_matches_kind_prefix(lk: &LocatorKind, locator: &str) -> bool {
    match lk {
        LocatorKind::NoFineLocator => locator.is_empty(),
        LocatorKind::PdfPageBox => {
            locator
                .strip_prefix("p. ")
                .is_some_and(|rest| !rest.trim().is_empty())
                || locator
                    .strip_prefix("pp. ")
                    .is_some_and(|rest| !rest.trim().is_empty())
        }
        LocatorKind::TextSection => {
            locator
                .strip_prefix("sec. ")
                .is_some_and(|rest| !rest.trim().is_empty())
                || locator
                    .strip_prefix("§ ")
                    .is_some_and(|rest| !rest.trim().is_empty())
                || locator
                    .strip_prefix("para. ")
                    .is_some_and(|rest| !rest.trim().is_empty())
        }
        LocatorKind::SheetCells => locator
            .strip_prefix("sheet ")
            .is_some_and(|rest| !rest.trim().is_empty()),
        LocatorKind::RecordKey => locator
            .strip_prefix("rec ")
            .is_some_and(|rest| !rest.trim().is_empty()),
        LocatorKind::RecordField => locator
            .strip_prefix("field ")
            .is_some_and(|rest| !rest.trim().is_empty()),
        LocatorKind::Extension(ext_kind) => locator
            .strip_prefix("ext ")
            .and_then(|rest| rest.split_once(' '))
            .is_some_and(|(k, payload)| k == ext_kind && !payload.trim().is_empty()),
    }
}

fn check_claim_row(
    row: &Row,
    bad_cols: &mut BTreeSet<&'static str>,
    strict: bool,
    report: &mut Report,
) {
    let reg = Register::Claims;

    // Single-cell: `area` must equal the `claim_id`'s area letters in lowercase (FB0204).
    if !bad_cols.contains("claim_id")
        && !bad_cols.contains("area")
        && let (Ok(cid), Ok(area)) = (
            ClaimId::parse(cell_str(reg, row, "claim_id")),
            AreaCode::parse(cell_str(reg, row, "area")),
        )
        && cid.area() != area
    {
        bad_cols.insert("area");
        push_cell_diag(
            reg,
            row,
            "area",
            Code::Fb0204,
            strict,
            format!(
                "claim `area` `{area}` differs from `{cid}` area `{}`",
                cid.area()
            ),
            report,
        );
    }

    // Single-cell overlay values (FB0210).
    let raw_lk = cell_str(reg, row, "locator_kind");
    let parsed_lk = if raw_lk.is_empty() {
        None
    } else {
        LocatorKind::parse(raw_lk).ok()
    };
    if let Some(ref lk) = parsed_lk
        && lk.is_overlay()
    {
        bad_cols.insert("locator_kind");
        push_cell_diag(
            reg,
            row,
            "locator_kind",
            Code::Fb0210,
            strict,
            format!("`locator_kind` value `{raw_lk}` is company-store only"),
            report,
        );
    }

    let raw_app = cell_str(reg, row, "applicability");
    if let Ok(app) = Applicability::parse(raw_app)
        && app.is_overlay()
    {
        bad_cols.insert("applicability");
        push_cell_diag(
            reg,
            row,
            "applicability",
            Code::Fb0210,
            strict,
            format!("`applicability` value `{raw_app}` is company-store only"),
            report,
        );
    }

    let raw_adopt = cell_str(reg, row, "adoption");
    let parsed_adopt = Adoption::parse(raw_adopt).ok();
    if let Some(adopt) = parsed_adopt
        && adopt.is_overlay()
    {
        bad_cols.insert("adoption");
        push_cell_diag(
            reg,
            row,
            "adoption",
            Code::Fb0210,
            strict,
            format!("`adoption` value `{raw_adopt}` is company-store only"),
            report,
        );
    }

    let raw_eg = cell_str(reg, row, "evidence_grade");
    let parsed_eg = if raw_eg.is_empty() {
        None
    } else {
        EvidenceGrade::parse(raw_eg).ok()
    };
    if let Some(eg) = parsed_eg
        && eg.is_overlay()
    {
        bad_cols.insert("evidence_grade");
        push_cell_diag(
            reg,
            row,
            "evidence_grade",
            Code::Fb0210,
            strict,
            format!("`evidence_grade` value `{raw_eg}` is company-store only"),
            report,
        );
    }

    let parsed_kind = ClaimKind::parse(cell_str(reg, row, "kind")).ok();
    let parsed_ver = Verification::parse(cell_str(reg, row, "verification")).ok();

    // Rules 1 & 2: `kind` vs source/assumption cells (FB0206).
    if let Some(kind) = parsed_kind {
        match kind {
            ClaimKind::Fact | ClaimKind::Interpretation => {
                for col in ["source_id", "version", "locator_kind"] {
                    if !bad_cols.contains(col) && cell_str(reg, row, col).is_empty() {
                        push_cell_diag(
                            reg,
                            row,
                            col,
                            Code::Fb0206,
                            strict,
                            format!("`{col}` is required when `kind` is `{}`", kind.as_str()),
                            report,
                        );
                    }
                }
            }
            ClaimKind::Assumption => {
                for col in ["source_id", "version", "locator_kind", "locator"] {
                    if !bad_cols.contains(col) && !cell_str(reg, row, col).is_empty() {
                        push_cell_diag(
                            reg,
                            row,
                            col,
                            Code::Fb0206,
                            strict,
                            format!("`{col}` must be empty when `kind` is `assumption`"),
                            report,
                        );
                    }
                }
                if !bad_cols.contains("evidence_grade") && raw_eg.is_empty() {
                    push_cell_diag(
                        reg,
                        row,
                        "evidence_grade",
                        Code::Fb0206,
                        strict,
                        "`evidence_grade` is required when `kind` is `assumption`".to_owned(),
                        report,
                    );
                }
                if !bad_cols.contains("verification")
                    && let Some(ver) = parsed_ver
                    && ver != Verification::Unchecked
                {
                    push_cell_diag(
                        reg,
                        row,
                        "verification",
                        Code::Fb0206,
                        strict,
                        "`verification` must be `unchecked` when `kind` is `assumption`".to_owned(),
                        report,
                    );
                }
            }
            ClaimKind::Recommendation => {}
        }
    }

    // Rules 3 & 4: `locator_kind` vs `locator` (FB0206).
    if let Some(ref lk) = parsed_lk
        && !bad_cols.contains("locator")
    {
        let raw_loc = cell_str(reg, row, "locator");
        if *lk == LocatorKind::NoFineLocator {
            if !raw_loc.is_empty() {
                push_cell_diag(
                    reg,
                    row,
                    "locator",
                    Code::Fb0206,
                    strict,
                    "`locator` must be empty when `locator_kind` is `no_fine_locator`".to_owned(),
                    report,
                );
            }
        } else if raw_loc.is_empty() {
            push_cell_diag(
                reg,
                row,
                "locator",
                Code::Fb0206,
                strict,
                format!("`locator` is required when `locator_kind` is `{lk}`"),
                report,
            );
        } else if !locator_matches_kind_prefix(lk, raw_loc) {
            push_cell_diag(
                reg,
                row,
                "locator",
                Code::Fb0206,
                strict,
                format!("`locator` `{raw_loc}` does not begin with a valid prefix for `{lk}`"),
                report,
            );
        }
    }

    // Rule 6: `evidence_grade` is `G` -> `note` starts with `replace by ` + non-whitespace (FB0206).
    if parsed_eg == Some(EvidenceGrade::G) && !bad_cols.contains("note") {
        let raw_note = cell_str(reg, row, "note");
        let valid_note = raw_note
            .strip_prefix("replace by ")
            .is_some_and(|rest| !rest.trim().is_empty());
        if !valid_note {
            push_cell_diag(
                reg,
                row,
                "note",
                Code::Fb0206,
                strict,
                "`note` must start with `replace by ` followed by date or event text when `evidence_grade` is `G`".to_owned(),
                report,
            );
        }
    }

    // Rule 8: `value` is set <-> `unit` is set (FB0206).
    if !bad_cols.contains("value") && !bad_cols.contains("unit") {
        let has_val = !cell_str(reg, row, "value").is_empty();
        let has_unit = !cell_str(reg, row, "unit").is_empty();
        if has_val && !has_unit {
            push_cell_diag(
                reg,
                row,
                "unit",
                Code::Fb0206,
                strict,
                "`unit` must be set when `value` is set".to_owned(),
                report,
            );
        } else if !has_val && has_unit {
            push_cell_diag(
                reg,
                row,
                "value",
                Code::Fb0206,
                strict,
                "`value` must be set when `unit` is set".to_owned(),
                report,
            );
        }
    }

    // Rules 9, 10, 11, 12, 13: `verification` rules (FB0206).
    if let Some(ver) = parsed_ver {
        match ver {
            Verification::Unchecked => {
                for col in ["verified_on", "verified_by_role"] {
                    if !bad_cols.contains(col) && !cell_str(reg, row, col).is_empty() {
                        push_cell_diag(
                            reg,
                            row,
                            col,
                            Code::Fb0206,
                            strict,
                            format!("`{col}` must be empty when `verification` is `unchecked`"),
                            report,
                        );
                    }
                }
            }
            Verification::PrimarySourceSupported | Verification::Contradicted => {
                for col in ["verified_on", "verified_by_role"] {
                    if !bad_cols.contains(col) && cell_str(reg, row, col).is_empty() {
                        push_cell_diag(
                            reg,
                            row,
                            col,
                            Code::Fb0206,
                            strict,
                            format!(
                                "`{col}` is required when `verification` is `{}`",
                                ver.as_str()
                            ),
                            report,
                        );
                    }
                }
                if ver == Verification::Contradicted
                    && !bad_cols.contains("conflict_id")
                    && cell_str(reg, row, "conflict_id").is_empty()
                {
                    push_cell_diag(
                        reg,
                        row,
                        "conflict_id",
                        Code::Fb0206,
                        strict,
                        "`conflict_id` is required when `verification` is `contradicted`"
                            .to_owned(),
                        report,
                    );
                }
            }
            Verification::Unresolved => {
                if !bad_cols.contains("question_id") && cell_str(reg, row, "question_id").is_empty()
                {
                    push_cell_diag(
                        reg,
                        row,
                        "question_id",
                        Code::Fb0206,
                        strict,
                        "`question_id` is required when `verification` is `unresolved`".to_owned(),
                        report,
                    );
                }
            }
            Verification::Stale => {
                if !bad_cols.contains("superseded_by")
                    && !bad_cols.contains("conflict_id")
                    && !bad_cols.contains("note")
                {
                    let has_superseded_by = !cell_str(reg, row, "superseded_by").is_empty();
                    let has_conflict_id = !cell_str(reg, row, "conflict_id").is_empty();
                    let note = cell_str(reg, row, "note");
                    let has_stale_note =
                        note.starts_with("review passed") || note.starts_with("as_of unknown");
                    if !has_superseded_by && !has_conflict_id && !has_stale_note {
                        push_cell_diag(
                            reg,
                            row,
                            "verification",
                            Code::Fb0206,
                            strict,
                            "`stale` claim requires `superseded_by`, `conflict_id`, or `note` starting with `review passed` or `as_of unknown`".to_owned(),
                            report,
                        );
                    }
                }
            }
        }
    }

    // Rule 14: `as_of` is empty -> `verification` is `stale` and `note` begins `as_of unknown` (FB0206).
    if !bad_cols.contains("as_of")
        && cell_str(reg, row, "as_of").is_empty()
        && !bad_cols.contains("verification")
        && !bad_cols.contains("note")
    {
        let is_stale = parsed_ver == Some(Verification::Stale);
        let note_unknown = cell_str(reg, row, "note").starts_with("as_of unknown");
        if !(is_stale && note_unknown) {
            push_cell_diag(
                reg,
                row,
                "as_of",
                Code::Fb0206,
                strict,
                "empty `as_of` requires `verification = stale` and `note` starting with `as_of unknown`".to_owned(),
                report,
            );
        }
    }

    // Rule 15: `instrument` is `class_deviation`, `rfo_model_text` or `proposed_rule` ->
    // `citation_rfo`, `effective_from` and `text_status` are set (FB0206).
    let raw_inst = cell_str(reg, row, "instrument");
    let parsed_inst = if raw_inst.is_empty() {
        None
    } else {
        Instrument::parse(raw_inst).ok()
    };
    if let Some(inst) = parsed_inst
        && instrument_requires_rfo_dual_numbering(inst)
    {
        for col in ["citation_rfo", "effective_from", "text_status"] {
            if !bad_cols.contains(col) && cell_str(reg, row, col).is_empty() {
                push_cell_diag(
                    reg,
                    row,
                    col,
                    Code::Fb0206,
                    strict,
                    format!(
                        "`{col}` is required when `instrument` is `{}`",
                        inst.as_str()
                    ),
                    report,
                );
            }
        }
    }

    // Rule 16: `text_status` is set -> `instrument` is set (FB0206).
    if !bad_cols.contains("text_status")
        && !bad_cols.contains("instrument")
        && !cell_str(reg, row, "text_status").is_empty()
        && raw_inst.is_empty()
    {
        push_cell_diag(
            reg,
            row,
            "instrument",
            Code::Fb0206,
            strict,
            "`instrument` is required when `text_status` is set".to_owned(),
            report,
        );
    }

    // Rule 17: `superseded_by` is set -> `adoption` is `superseded` (FB0206).
    if !bad_cols.contains("superseded_by")
        && !bad_cols.contains("adoption")
        && !cell_str(reg, row, "superseded_by").is_empty()
        && parsed_adopt != Some(Adoption::Superseded)
    {
        push_cell_diag(
            reg,
            row,
            "adoption",
            Code::Fb0206,
            strict,
            "`adoption` must be `superseded` when `superseded_by` is set".to_owned(),
            report,
        );
    }
}

fn check_question_row(
    row: &Row,
    bad_cols: &mut BTreeSet<&'static str>,
    strict: bool,
    report: &mut Report,
) {
    let reg = Register::Questions;

    let raw_status = cell_str(reg, row, "status");
    if let Ok(st) = QuestionStatus::parse(raw_status)
        && st.is_overlay()
    {
        bad_cols.insert("status");
        push_cell_diag(
            reg,
            row,
            "status",
            Code::Fb0210,
            strict,
            format!("`status` value `{raw_status}` is company-store only"),
            report,
        );
    }

    for col in ["answer", "answered_on", "adoption"] {
        if !bad_cols.contains(col) && !cell_str(reg, row, col).is_empty() {
            bad_cols.insert(col);
            push_cell_diag(
                reg,
                row,
                col,
                Code::Fb0210,
                strict,
                format!("question `{col}` must be empty in the repository Corpus"),
                report,
            );
        }
    }
}

fn check_conflict_row(
    row: &Row,
    bad_cols: &BTreeSet<&'static str>,
    strict: bool,
    report: &mut Report,
) {
    let reg = Register::Conflicts;
    if bad_cols.contains("status") {
        return;
    }
    let Ok(status) = ConflictStatus::parse(cell_str(reg, row, "status")) else {
        return;
    };
    match status {
        ConflictStatus::Resolved => {
            for col in ["resolution", "rationale", "evidence", "resolved_on"] {
                if !bad_cols.contains(col) && cell_str(reg, row, col).is_empty() {
                    push_cell_diag(
                        reg,
                        row,
                        col,
                        Code::Fb0206,
                        strict,
                        format!("`{col}` is required when conflict `status` is `resolved`"),
                        report,
                    );
                }
            }
        }
        ConflictStatus::Unresolved => {
            if !bad_cols.contains("question_id") && cell_str(reg, row, "question_id").is_empty() {
                push_cell_diag(
                    reg,
                    row,
                    "question_id",
                    Code::Fb0206,
                    strict,
                    "`question_id` is required when conflict `status` is `unresolved`".to_owned(),
                    report,
                );
            }
        }
    }
}

fn check_glossary_row(
    row: &Row,
    bad_cols: &BTreeSet<&'static str>,
    strict: bool,
    report: &mut Report,
) {
    let reg = Register::Glossary;
    if bad_cols.contains("definition_status") || bad_cols.contains("source_ref") {
        return;
    }
    if let Ok(DefinitionStatus::PublicReference) =
        DefinitionStatus::parse(cell_str(reg, row, "definition_status"))
        && cell_str(reg, row, "source_ref").is_empty()
    {
        push_cell_diag(
            reg,
            row,
            "source_ref",
            Code::Fb0206,
            strict,
            "`source_ref` is required when `definition_status` is `public_reference`".to_owned(),
            report,
        );
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::check::{Code, Host, Options, Tree, load, run};
    use crate::date::CivilDate;
    use crate::schema::Register;
    use crate::schema::vocab::{AuthorityTier, SourceKind};

    use super::RULES;

    const CORPUS_FORMAT_MD: &str = include_str!("../../../../docs/reference/corpus-format.md");

    const MINI_CORPUS_TOML: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/corpus.toml");
    const MINI_ASSISTANT_MD: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/ASSISTANT.md");
    const MINI_CHANGES_MD: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/changes.md");
    const MINI_SOURCES_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/sources.csv");
    const MINI_CLAIMS_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/claims.csv");
    const MINI_QUESTIONS_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/questions.csv");
    const MINI_CONFLICTS_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/conflicts.csv");
    const MINI_GLOSSARY_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/glossary.csv");

    fn sample_today() -> CivilDate {
        CivilDate::from_ymd(2026, 3, 1).expect("valid date")
    }

    fn mini_tree() -> Tree {
        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_dir("corpus/registers");
        tree.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes());
        tree.insert_bytes("corpus/ASSISTANT.md", MINI_ASSISTANT_MD.as_bytes());
        tree.insert_bytes("corpus/changes.md", MINI_CHANGES_MD.as_bytes());
        tree.insert_bytes("corpus/registers/sources.csv", MINI_SOURCES_CSV.as_bytes());
        tree.insert_bytes("corpus/registers/claims.csv", MINI_CLAIMS_CSV.as_bytes());
        tree.insert_bytes(
            "corpus/registers/questions.csv",
            MINI_QUESTIONS_CSV.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/registers/conflicts.csv",
            MINI_CONFLICTS_CSV.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/registers/glossary.csv",
            MINI_GLOSSARY_CSV.as_bytes(),
        );
        tree
    }

    fn extract_fb_codes(text: &str) -> BTreeSet<Code> {
        let mut codes = BTreeSet::new();
        let bytes = text.as_bytes();
        let mut i = 0;
        while i + 6 <= bytes.len() {
            if bytes[i] == b'F'
                && bytes[i + 1] == b'B'
                && bytes[i + 2..i + 6].iter().all(u8::is_ascii_digit)
            {
                let token = &text[i..i + 6];
                if let Ok(code) = Code::parse(token) {
                    codes.insert(code);
                }
                i += 6;
            } else {
                i += 1;
            }
        }
        codes
    }

    fn register_section(md: &str, reg: Register) -> &str {
        let heading = format!("### {}", reg.file_name());
        let start = md
            .find(&heading)
            .unwrap_or_else(|| panic!("missing `{heading}` in corpus-format.md"));
        let rest = &md[start + heading.len()..];
        let end = rest
            .find("\n### ")
            .or_else(|| rest.find("\n## "))
            .unwrap_or(rest.len());
        &rest[..end]
    }

    #[test]
    fn rules_match_the_reference() {
        for reg in Register::ALL {
            let section = register_section(CORPUS_FORMAT_MD, reg);
            let rules_pos = section.find("Rules across columns").unwrap_or_else(|| {
                panic!(
                    "missing `Rules across columns` in section for {}",
                    reg.file_name()
                )
            });
            let after_rules = &section[rules_pos..];
            let mut lines = after_rules.lines();
            let caption_line = lines.next().unwrap_or("");

            let code_entries: Vec<_> = RULES.iter().filter(|e| e.register == reg).collect();
            for entry in &code_entries {
                assert!(
                    !entry.predicates.is_empty(),
                    "rule `{}` in {} has no predicates",
                    entry.rule,
                    reg.file_name()
                );
                for pred in entry.predicates {
                    assert!(
                        !pred.owner.is_empty(),
                        "rule `{}` in {} has empty owner",
                        entry.rule,
                        reg.file_name()
                    );
                }
            }

            let mut ref_codes = extract_fb_codes(caption_line);
            if matches!(reg, Register::Sources | Register::Claims) {
                let mut table_rows = Vec::new();
                let mut in_table = false;
                let mut seen_sep = false;
                for line in lines {
                    let trimmed = line.trim();
                    if trimmed.starts_with('|') {
                        in_table = true;
                        if trimmed.contains("---") {
                            seen_sep = true;
                            continue;
                        }
                        if seen_sep {
                            table_rows.push(trimmed);
                            ref_codes.extend(extract_fb_codes(trimmed));
                        }
                    } else if in_table {
                        break;
                    }
                }

                assert_eq!(
                    code_entries.len(),
                    table_rows.len(),
                    "RULES entry count for {} ({}) does not match reference table row count ({})",
                    reg.file_name(),
                    code_entries.len(),
                    table_rows.len()
                );

                for (idx, (entry, raw_row)) in
                    code_entries.iter().zip(table_rows.iter()).enumerate()
                {
                    let first_col = raw_row
                        .trim_start_matches('|')
                        .split('|')
                        .next()
                        .unwrap_or("")
                        .trim();
                    assert_eq!(
                        entry.rule,
                        first_col,
                        "mismatch at row {} of {}: code has `{}`, reference has `{}`",
                        idx + 1,
                        reg.file_name(),
                        entry.rule,
                        first_col
                    );
                }
            }

            let code_set: BTreeSet<Code> = code_entries
                .iter()
                .flat_map(|e| e.predicates.iter().filter_map(|p| p.code))
                .collect();

            assert_eq!(
                code_set,
                ref_codes,
                "code set mismatch for {}: code has {code_set:?}, reference has {ref_codes:?}",
                reg.file_name()
            );
        }
    }

    #[test]
    fn typed_accessors_keep_register_order_and_skip_bad_rows() {
        let mut tree = mini_tree();

        // Append a bad row (line 6) and a good row (line 7) to sources.csv.
        let mut sources = MINI_SOURCES_CSV.to_owned();
        sources.push_str("S-0004,1,bad-date,Bad Date Source,Synthetic Board,primary_law,primary_evidence,A,Public domain,,,metadata-only: test,5555555555555555555555555555555555555555555555555555555555555555,,,2026-02-01,2025-02-30,active,,V2,public,,,,,,,\n");
        sources.push_str("S-0005,1,good-tail,Good Tail Source,Synthetic Board,primary_law,primary_evidence,A,Public domain,,,metadata-only: test,6666666666666666666666666666666666666666666666666666666666666666,,,2026-02-01,2026-02-02,active,,V2,public,,,,,,,\n");
        tree.insert_bytes("corpus/registers/sources.csv", sources.into_bytes());

        // Append a bad row (line 7: verification = checked) and a good row (line 8) to claims.csv.
        let mut claims = MINI_CLAIMS_CSV.to_owned();
        claims.push_str("CL-EST-0004,est,fact,Bad verification cell.,2026-02-01,S-0001,1,text_section,sec. 1,Quote.,false,codex,checked,pending_company_confirmation,proposed,observed,V2,,,,,,,,,,,,,,,,,,\n");
        claims.push_str("CL-EST-0005,est,fact,Valid tail claim.,2026-02-01,S-0001,1,text_section,sec. 1,Quote.,false,codex,unchecked,pending_company_confirmation,proposed,observed,V2,,,,,,,,,,,,,,,,,,\n");
        tree.insert_bytes("corpus/registers/claims.csv", claims.into_bytes());

        // Append a bad row (line 4: empty question) and a good row (line 5) to questions.csv.
        let mut questions = MINI_QUESTIONS_CSV.to_owned();
        questions.push_str("Q-003,cmp,,,,,,,false,open,,,\n");
        questions.push_str("Q-004,cmp,Valid tail question?,,,,,,false,open,,,\n");
        tree.insert_bytes("corpus/registers/questions.csv", questions.into_bytes());

        // Append a bad row (line 3: status = pending) and a good row (line 4) to conflicts.csv.
        let mut conflicts = MINI_CONFLICTS_CSV.to_owned();
        conflicts.push_str("XC-02,cmp,Topic,Pos A|Pos B,,,,,pending,CL-CMP-0001,Q-001\n");
        conflicts.push_str(
            "XC-03,cmp,Valid tail conflict,Pos A|Pos B,,,,,unresolved,CL-CMP-0001,Q-001\n",
        );
        tree.insert_bytes("corpus/registers/conflicts.csv", conflicts.into_bytes());

        // Append a bad row (line 6: empty label) and a good row (line 7) to glossary.csv.
        let mut glossary = MINI_GLOSSARY_CSV.to_owned();
        glossary.push_str("tail_bad,,Def.,Boundary.,cmp,,general_practice,,\n");
        glossary.push_str("tail_term,tail term,Def.,Boundary.,cmp,,general_practice,,\n");
        tree.insert_bytes("corpus/registers/glossary.csv", glossary.into_bytes());

        let (model, _) = load(&Host::new(tree, sample_today()), &Options::default());

        let src_lines: Vec<_> = model
            .sources()
            .iter()
            .map(|r| (r.line, r.value.source_id.as_str(), r.value.version))
            .collect();
        assert_eq!(
            src_lines,
            vec![
                (2, "S-0001", 1),
                (3, "S-0002", 1),
                (4, "S-0002", 2),
                (5, "S-0003", 1),
                (7, "S-0005", 1),
            ]
        );

        let claim_lines: Vec<_> = model
            .claims()
            .iter()
            .map(|r| (r.line, r.value.claim_id.as_str()))
            .collect();
        assert_eq!(
            claim_lines,
            vec![
                (2, "CL-CMP-0001"),
                (3, "CL-CMP-0002"),
                (4, "CL-EST-0001"),
                (5, "CL-EST-0002"),
                (6, "CL-EST-0003"),
                (8, "CL-EST-0005"),
            ]
        );

        let q_lines: Vec<_> = model
            .questions()
            .iter()
            .map(|r| (r.line, r.value.question_id.as_str()))
            .collect();
        assert_eq!(q_lines, vec![(2, "Q-001"), (3, "Q-002"), (5, "Q-004")]);

        let c_lines: Vec<_> = model
            .conflicts()
            .iter()
            .map(|r| (r.line, r.value.conflict_id.as_str()))
            .collect();
        assert_eq!(c_lines, vec![(2, "XC-01"), (4, "XC-03")]);

        let g_lines: Vec<_> = model
            .glossary()
            .iter()
            .map(|r| (r.line, r.value.term_id.as_str()))
            .collect();
        assert_eq!(
            g_lines,
            vec![
                (2, "billing_withhold"),
                (3, "cost_baseline"),
                (4, "rate_window"),
                (5, "system_review"),
                (7, "tail_term"),
            ]
        );
    }

    #[test]
    fn stale_claim_with_unknown_as_of_is_valid() {
        let mut tree = mini_tree();
        let mut claims = MINI_CLAIMS_CSV.to_owned();
        claims.push_str(
            "CL-EST-0004,est,fact,Synthetic legacy fact with unknown as-of date.,,S-0001,1,text_section,sec. 1,Quote.,false,codex,stale,pending_company_confirmation,proposed,observed,V2,,,,,,,,,,,,,,,,,,as_of unknown: legacy table printed no date.\n",
        );
        tree.insert_bytes("corpus/registers/claims.csv", claims.into_bytes());

        let report = run(&Host::new(tree, sample_today()), &Options::default());
        assert!(
            report.diagnostics.is_empty(),
            "expected no diagnostics for stale claim with `as_of unknown`, got: {:?}",
            report.diagnostics
        );
    }

    #[test]
    fn empty_as_of_without_stale_reason_is_fb0206() {
        // 1. Empty as_of with verification = unchecked (even when note begins `as_of unknown`).
        let mut tree1 = mini_tree();
        let mut claims1 = MINI_CLAIMS_CSV.to_owned();
        claims1.push_str(
            "CL-EST-0004,est,fact,Missing stale verification.,,S-0001,1,text_section,sec. 1,Quote.,false,codex,unchecked,pending_company_confirmation,proposed,observed,V2,,,,,,,,,,,,,,,,,,as_of unknown: missing stale status.\n",
        );
        tree1.insert_bytes("corpus/registers/claims.csv", claims1.into_bytes());
        let report1 = run(&Host::new(tree1, sample_today()), &Options::default());
        assert_eq!(report1.diagnostics.len(), 1, "{:?}", report1.diagnostics);
        assert_eq!(report1.diagnostics[0].code, Code::Fb0206);
        assert_eq!(report1.diagnostics[0].line, 7);

        // 2. Empty as_of with verification = stale but note does not begin `as_of unknown` (e.g. `review passed`).
        let mut tree2 = mini_tree();
        let mut claims2 = MINI_CLAIMS_CSV.to_owned();
        claims2.push_str(
            "CL-EST-0004,est,fact,Missing as_of unknown note.,,S-0001,1,text_section,sec. 1,Quote.,false,codex,stale,pending_company_confirmation,proposed,observed,V2,,,,,,,,,,,,,,,,,,review passed without re-check.\n",
        );
        tree2.insert_bytes("corpus/registers/claims.csv", claims2.into_bytes());
        let report2 = run(&Host::new(tree2, sample_today()), &Options::default());
        assert_eq!(report2.diagnostics.len(), 1, "{:?}", report2.diagnostics);
        assert_eq!(report2.diagnostics[0].code, Code::Fb0206);
        assert_eq!(report2.diagnostics[0].line, 7);
    }

    #[test]
    fn question_company_cells_are_fb0210() {
        let plants = [
            // 1. status = answered
            "Q-003,cmp,Question text?,,,,,,false,answered,,,\n",
            // 2. nonempty answer
            "Q-003,cmp,Question text?,,,,,,false,open,Company answer text.,, \n".trim_end(),
            // 3. nonempty answered_on
            "Q-003,cmp,Question text?,,,,,,false,open,,2026-02-01,\n",
            // 4. nonempty adoption (even a public adoption word)
            "Q-003,cmp,Question text?,,,,,,false,open,,,proposed\n",
        ];

        for (idx, plant) in plants.iter().enumerate() {
            let mut tree = mini_tree();
            let mut questions = MINI_QUESTIONS_CSV.to_owned();
            questions.push_str(plant.trim_end());
            questions.push('\n');
            tree.insert_bytes("corpus/registers/questions.csv", questions.into_bytes());
            let report = run(&Host::new(tree, sample_today()), &Options::default());
            assert_eq!(
                report.diagnostics.len(),
                1,
                "plant {} ({plant:?}) expected 1 diagnostic, got {:?}",
                idx + 1,
                report.diagnostics
            );
            assert_eq!(report.diagnostics[0].code, Code::Fb0210);
            assert_eq!(report.diagnostics[0].path, "corpus/registers/questions.csv");
            assert_eq!(report.diagnostics[0].line, 4);
        }
    }

    #[test]
    fn question_empty_answer_cells_are_valid() {
        let mut tree = mini_tree();
        let mut questions = MINI_QUESTIONS_CSV.to_owned();
        questions.push_str(
            "Q-003,cmp,Withdrawn question with empty answer cells?,,,,,,false,withdrawn,,,\n",
        );
        tree.insert_bytes("corpus/registers/questions.csv", questions.into_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::default());
        assert!(
            report.diagnostics.is_empty(),
            "expected no diagnostics, got {:?}",
            report.diagnostics
        );
    }

    #[test]
    fn impossible_register_date_is_fb0114() {
        let mut tree = mini_tree();
        let bad_sources =
            MINI_SOURCES_CSV.replacen(",2026-01-15,2026-01-15,", ",2026-01-15,2025-02-30,", 1);
        tree.insert_bytes("corpus/registers/sources.csv", bad_sources.into_bytes());

        let report = run(&Host::new(tree, sample_today()), &Options::default());
        assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
        assert_eq!(report.diagnostics[0].code, Code::Fb0114);
        assert_eq!(report.diagnostics[0].path, "corpus/registers/sources.csv");
        assert_eq!(report.diagnostics[0].line, 3);
    }

    #[test]
    fn partial_full_date_is_fb0114() {
        // `retrieved` in sources.csv and `verified_on` in claims.csv require full YYYY-MM-DD dates.
        let mut tree1 = mini_tree();
        let bad_sources =
            MINI_SOURCES_CSV.replacen(",2026-01-15,2026-01-15,", ",2026-01-15,2026-01,", 1);
        tree1.insert_bytes("corpus/registers/sources.csv", bad_sources.into_bytes());
        let report1 = run(&Host::new(tree1, sample_today()), &Options::default());
        assert_eq!(report1.diagnostics.len(), 1, "{:?}", report1.diagnostics);
        assert_eq!(report1.diagnostics[0].code, Code::Fb0114);

        let mut tree2 = mini_tree();
        let bad_claims = MINI_CLAIMS_CSV.replacen(",2026-02-10,owner,", ",2026-02,owner,", 1);
        tree2.insert_bytes("corpus/registers/claims.csv", bad_claims.into_bytes());
        let report2 = run(&Host::new(tree2, sample_today()), &Options::default());
        assert_eq!(report2.diagnostics.len(), 1, "{:?}", report2.diagnostics);
        assert_eq!(report2.diagnostics[0].code, Code::Fb0114);
    }

    #[test]
    fn partial_claim_date_is_valid() {
        let mut tree = mini_tree();
        let mut claims = MINI_CLAIMS_CSV.to_owned();
        claims.push_str(
            "CL-EST-0004,est,fact,Synthetic claim with year and month partial dates.,2026,S-0001,1,text_section,sec. 1,Quote.,false,codex,unchecked,pending_company_confirmation,proposed,observed,V2,,,2025-11,2026,far_codified,48 CFR 9900.101,,2025-11,final,,,,,,,,,\n",
        );
        tree.insert_bytes("corpus/registers/claims.csv", claims.into_bytes());

        let report = run(&Host::new(tree, sample_today()), &Options::default());
        assert!(
            report.diagnostics.is_empty(),
            "expected partial pdate cells on claim to be valid, got {:?}",
            report.diagnostics
        );
    }

    #[test]
    fn fact_and_interpretation_need_source_cells() {
        for kind in ["fact", "interpretation"] {
            // Valid counterpart with all three source cells and a valid locator.
            let valid_row = format!(
                "CL-EST-0004,est,{kind},Valid {kind} row.,2026-02-01,S-0001,1,text_section,sec. 1,,false,codex,unchecked,pending_company_confirmation,proposed,observed,V2,,,,,,,,,,,,,,,,,,\n"
            );
            let mut valid_tree = mini_tree();
            valid_tree.insert_bytes(
                "corpus/registers/claims.csv",
                format!("{MINI_CLAIMS_CSV}{valid_row}").into_bytes(),
            );
            let valid_report = run(&Host::new(valid_tree, sample_today()), &Options::default());
            assert!(
                valid_report.diagnostics.is_empty(),
                "expected valid {kind} row to pass, got {:?}",
                valid_report.diagnostics
            );

            // Missing source_id, missing version, missing locator_kind (with empty locator so Rule 4 does not fire).
            let missing_cases = [
                format!(
                    "CL-EST-0004,est,{kind},Missing source_id.,2026-02-01,,1,text_section,sec. 1,,false,codex,unchecked,pending_company_confirmation,proposed,observed,V2,,,,,,,,,,,,,,,,,,\n"
                ),
                format!(
                    "CL-EST-0004,est,{kind},Missing version.,2026-02-01,S-0001,,text_section,sec. 1,,false,codex,unchecked,pending_company_confirmation,proposed,observed,V2,,,,,,,,,,,,,,,,,,\n"
                ),
                format!(
                    "CL-EST-0004,est,{kind},Missing locator_kind.,2026-02-01,S-0001,1,,,Quote.,false,codex,unchecked,pending_company_confirmation,proposed,observed,V2,,,,,,,,,,,,,,,,,,\n"
                ),
            ];

            for (idx, broken) in missing_cases.iter().enumerate() {
                let mut tree = mini_tree();
                tree.insert_bytes(
                    "corpus/registers/claims.csv",
                    format!("{MINI_CLAIMS_CSV}{broken}").into_bytes(),
                );
                let report = run(&Host::new(tree, sample_today()), &Options::default());
                assert_eq!(
                    report.diagnostics.len(),
                    1,
                    "{kind} missing case {idx} expected 1 FB0206 diagnostic, got {:?}",
                    report.diagnostics
                );
                assert_eq!(report.diagnostics[0].code, Code::Fb0206);
                assert_eq!(report.diagnostics[0].line, 7);
            }
        }

        // Valid assumption with empty source cells and Grade E/F.
        let mut assumption_tree = mini_tree();
        let assumption_row = "CL-EST-0004,est,assumption,Valid assumption row.,2026-02-01,,,,,,false,codex,unchecked,prototype_assumption,proposed,estimated,V3,5,percent,,,,,,,,,E,,,,,,,\n";
        assumption_tree.insert_bytes(
            "corpus/registers/claims.csv",
            format!("{MINI_CLAIMS_CSV}{assumption_row}").into_bytes(),
        );
        let assumption_report = run(
            &Host::new(assumption_tree, sample_today()),
            &Options::default(),
        );
        assert!(
            assumption_report.diagnostics.is_empty(),
            "expected valid assumption to pass, got {:?}",
            assumption_report.diagnostics
        );
    }

    #[test]
    fn no_fine_locator_requires_empty_text() {
        // Passing: unchecked, non-consequential claim with no_fine_locator and empty locator.
        let mut pass_tree = mini_tree();
        let pass_row = "CL-EST-0004,est,fact,Unchecked non-consequential with no_fine_locator.,2026-02-01,S-0001,1,no_fine_locator,,,false,codex,unchecked,pending_company_confirmation,proposed,observed,V2,,,,,,,,,,,,,,,,,,\n";
        pass_tree.insert_bytes(
            "corpus/registers/claims.csv",
            format!("{MINI_CLAIMS_CSV}{pass_row}").into_bytes(),
        );
        let pass_report = run(&Host::new(pass_tree, sample_today()), &Options::default());
        assert!(
            pass_report.diagnostics.is_empty(),
            "expected empty locator with no_fine_locator to pass, got {:?}",
            pass_report.diagnostics
        );

        // Failing: unchecked, non-consequential claim with no_fine_locator and non-empty locator.
        let mut fail_tree = mini_tree();
        let fail_row = "CL-EST-0004,est,fact,Non-empty locator on no_fine_locator.,2026-02-01,S-0001,1,no_fine_locator,sec. 1,,false,codex,unchecked,pending_company_confirmation,proposed,observed,V2,,,,,,,,,,,,,,,,,,\n";
        fail_tree.insert_bytes(
            "corpus/registers/claims.csv",
            format!("{MINI_CLAIMS_CSV}{fail_row}").into_bytes(),
        );
        let fail_report = run(&Host::new(fail_tree, sample_today()), &Options::default());
        assert_eq!(
            fail_report.diagnostics.len(),
            1,
            "{:?}",
            fail_report.diagnostics
        );
        assert_eq!(fail_report.diagnostics[0].code, Code::Fb0206);
        assert_eq!(fail_report.diagnostics[0].line, 7);
    }

    #[test]
    fn superseded_claim_requires_superseded_adoption() {
        // In mini_tree, CL-EST-0002 (line 5) has superseded_by = CL-EST-0003 and CL-EST-0003 (line 6)
        // has supersedes = CL-EST-0002 (valid reciprocal links), with adoption = superseded.
        let pass_report = run(&Host::new(mini_tree(), sample_today()), &Options::default());
        assert!(pass_report.diagnostics.is_empty());

        // Change CL-EST-0002's adoption from `superseded` to `proposed` while keeping reciprocal links intact.
        let broken_claims = MINI_CLAIMS_CSV.replacen(
            "CL-EST-0002,est,fact,Synthetic rate review window was 15 days under prior guidance.,2026-01-15,S-0002,2,text_section,sec. 3,Synthetic rate review window was 15 days.,true,codex,unchecked,not_applicable,superseded,",
            "CL-EST-0002,est,fact,Synthetic rate review window was 15 days under prior guidance.,2026-01-15,S-0002,2,text_section,sec. 3,Synthetic rate review window was 15 days.,true,codex,unchecked,not_applicable,proposed,",
            1,
        );
        let mut fail_tree = mini_tree();
        fail_tree.insert_bytes("corpus/registers/claims.csv", broken_claims.into_bytes());
        let fail_report = run(&Host::new(fail_tree, sample_today()), &Options::default());
        assert_eq!(
            fail_report.diagnostics.len(),
            1,
            "{:?}",
            fail_report.diagnostics
        );
        assert_eq!(fail_report.diagnostics[0].code, Code::Fb0206);
        assert_eq!(fail_report.diagnostics[0].line, 5);
    }

    #[test]
    fn grade_g_requires_replacement_note() {
        // Valid date note and valid free-text event note on Grade G assumptions.
        for valid_note in [
            "replace by 2026-06-01",
            "replace by first rate-package submission after onboarding",
        ] {
            let row = format!(
                "CL-EST-0004,est,assumption,Grade G assumption.,2026-02-01,,,,,,false,codex,unchecked,prototype_assumption,proposed,estimated,V3,3,percent,,,,,,,,,G,,,,,,,{valid_note}\n"
            );
            let mut tree = mini_tree();
            tree.insert_bytes(
                "corpus/registers/claims.csv",
                format!("{MINI_CLAIMS_CSV}{row}").into_bytes(),
            );
            let report = run(&Host::new(tree, sample_today()), &Options::default());
            assert!(
                report.diagnostics.is_empty(),
                "expected Grade G note {valid_note:?} to pass, got {:?}",
                report.diagnostics
            );
        }

        // Failing Grade G notes: empty note, missing literal `replace by ` prefix, empty suffix, whitespace-only suffix.
        for bad_note in [
            "",
            "Replace by 2026-06-01",
            "temporary estimate",
            "replace by ",
            "replace by    ",
        ] {
            let row = format!(
                "CL-EST-0004,est,assumption,Grade G assumption with bad note.,2026-02-01,,,,,,false,codex,unchecked,prototype_assumption,proposed,estimated,V3,3,percent,,,,,,,,,G,,,,,,,{bad_note}\n"
            );
            let mut tree = mini_tree();
            tree.insert_bytes(
                "corpus/registers/claims.csv",
                format!("{MINI_CLAIMS_CSV}{row}").into_bytes(),
            );
            let report = run(&Host::new(tree, sample_today()), &Options::default());
            assert_eq!(
                report.diagnostics.len(),
                1,
                "expected Grade G note {bad_note:?} to fail with 1 FB0206, got {:?}",
                report.diagnostics
            );
            assert_eq!(report.diagnostics[0].code, Code::Fb0206);
            assert_eq!(report.diagnostics[0].line, 7);
        }

        // Non-G counterparts (E, F, and empty grade on a fact) do not require `replace by `.
        for grade in ["E", "F"] {
            let row = format!(
                "CL-EST-0004,est,assumption,Grade {grade} assumption.,2026-02-01,,,,,,false,codex,unchecked,prototype_assumption,proposed,estimated,V3,3,percent,,,,,,,,,{grade},,,,,,,Ordinary note without prefix.\n"
            );
            let mut tree = mini_tree();
            tree.insert_bytes(
                "corpus/registers/claims.csv",
                format!("{MINI_CLAIMS_CSV}{row}").into_bytes(),
            );
            let report = run(&Host::new(tree, sample_today()), &Options::default());
            assert!(
                report.diagnostics.is_empty(),
                "expected Grade {grade} without prefix to pass, got {:?}",
                report.diagnostics
            );
        }
    }

    #[test]
    fn discovery_tier_matches_kind() {
        for kind in SourceKind::ALL {
            let is_untiered = kind == SourceKind::SyntheticFixture || kind.is_overlay();
            if is_untiered {
                // Untiered kinds must reject D (and every other tier) with FB0206.
                let row = format!(
                    "S-0004,1,tier-test,Tier Test,Publisher,{},synthetic_fixture,D,Public domain,,,metadata-only: test,5555555555555555555555555555555555555555555555555555555555555555,,,2026-02-01,2026-02-01,active,,V2,public,,,,,,,\n",
                    kind.as_str()
                );
                let mut tree = mini_tree();
                tree.insert_bytes(
                    "corpus/registers/sources.csv",
                    format!("{MINI_SOURCES_CSV}{row}").into_bytes(),
                );
                let report = run(&Host::new(tree, sample_today()), &Options::default());
                assert!(
                    report.diagnostics.iter().any(|d| d.code == Code::Fb0206),
                    "expected untiered kind `{}` with tier D to raise FB0206, got {:?}",
                    kind.as_str(),
                    report.diagnostics
                );

                // With empty tier, synthetic_fixture passes (and overlay kinds raise only FB0210, not FB0206).
                let empty_tier_row = format!(
                    "S-0004,1,tier-test,Tier Test,Publisher,{},synthetic_fixture,,Public domain,,,metadata-only: test,5555555555555555555555555555555555555555555555555555555555555555,,,2026-02-01,2026-02-01,active,,V2,public,,,,,,,\n",
                    kind.as_str()
                );
                let mut tree2 = mini_tree();
                tree2.insert_bytes(
                    "corpus/registers/sources.csv",
                    format!("{MINI_SOURCES_CSV}{empty_tier_row}").into_bytes(),
                );
                let report2 = run(&Host::new(tree2, sample_today()), &Options::default());
                assert!(
                    !report2.diagnostics.iter().any(|d| d.code == Code::Fb0206),
                    "unexpected FB0206 on untiered kind `{}` with empty tier: {:?}",
                    kind.as_str(),
                    report2.diagnostics
                );
            } else {
                // Every public kind (other than synthetic_fixture) permits D.
                let d_row = format!(
                    "S-0004,1,tier-test,Tier Test,Publisher,{},primary_evidence,D,Public domain,,,metadata-only: test,5555555555555555555555555555555555555555555555555555555555555555,,,2026-02-01,2026-02-01,active,,V2,public,,,,,,,\n",
                    kind.as_str()
                );
                let mut d_tree = mini_tree();
                d_tree.insert_bytes(
                    "corpus/registers/sources.csv",
                    format!("{MINI_SOURCES_CSV}{d_row}").into_bytes(),
                );
                let d_report = run(&Host::new(d_tree, sample_today()), &Options::default());
                assert!(
                    d_report.diagnostics.is_empty(),
                    "expected public kind `{}` to permit tier D, got {:?}",
                    kind.as_str(),
                    d_report.diagnostics
                );

                // Every tier in AuthorityTier::ALL is accepted iff kind.allows_tier(Some(tier)), else FB0204.
                for tier in AuthorityTier::ALL {
                    let row = format!(
                        "S-0004,1,tier-test,Tier Test,Publisher,{},primary_evidence,{},Public domain,,,metadata-only: test,5555555555555555555555555555555555555555555555555555555555555555,,,2026-02-01,2026-02-01,active,,V2,public,,,,,,,\n",
                        kind.as_str(),
                        tier.as_str()
                    );
                    let mut tree = mini_tree();
                    tree.insert_bytes(
                        "corpus/registers/sources.csv",
                        format!("{MINI_SOURCES_CSV}{row}").into_bytes(),
                    );
                    let report = run(&Host::new(tree, sample_today()), &Options::default());
                    if super::source_kind_allows_tier(kind, tier) {
                        assert!(
                            report.diagnostics.is_empty(),
                            "expected `{}` with tier `{}` to pass, got {:?}",
                            kind.as_str(),
                            tier.as_str(),
                            report.diagnostics
                        );
                    } else {
                        assert_eq!(
                            report.diagnostics.len(),
                            1,
                            "expected `{}` with disallowed tier `{}` to raise 1 FB0204, got {:?}",
                            kind.as_str(),
                            tier.as_str(),
                            report.diagnostics
                        );
                        assert_eq!(report.diagnostics[0].code, Code::Fb0204);
                    }
                }
            }
        }
    }

    #[test]
    fn each_code_and_each_fb0206_entry_and_no_fb0206_after_fb0204() {
        // 1. FB0205: empty required cell (`statement` in claims.csv).
        let mut tree_205 = mini_tree();
        let bad_205 = MINI_CLAIMS_CSV.replacen(
            "Synthetic withholding threshold is 5 percent of billed amounts.",
            "",
            1,
        );
        tree_205.insert_bytes("corpus/registers/claims.csv", bad_205.into_bytes());
        let rep_205 = run(&Host::new(tree_205, sample_today()), &Options::default());
        assert_eq!(rep_205.diagnostics.len(), 1);
        assert_eq!(rep_205.diagnostics[0].code, Code::Fb0205);

        // 2. FB0204: claim area differs from claim_id area, and unused source_role.
        let mut tree_area = mini_tree();
        let bad_area = MINI_CLAIMS_CSV.replacen("CL-CMP-0002,cmp,", "CL-CMP-0002,est,", 1);
        tree_area.insert_bytes("corpus/registers/claims.csv", bad_area.into_bytes());
        let rep_area = run(&Host::new(tree_area, sample_today()), &Options::default());
        assert_eq!(rep_area.diagnostics.len(), 1);
        assert_eq!(rep_area.diagnostics[0].code, Code::Fb0204);

        for unused_role in ["generated_synthesis", "working_draft"] {
            let mut tree_role = mini_tree();
            let bad_role = MINI_SOURCES_CSV.replacen(
                ",primary_law,primary_evidence,",
                &format!(",primary_law,{unused_role},"),
                1,
            );
            tree_role.insert_bytes("corpus/registers/sources.csv", bad_role.into_bytes());
            let rep_role = run(&Host::new(tree_role, sample_today()), &Options::default());
            assert_eq!(rep_role.diagnostics.len(), 1);
            assert_eq!(rep_role.diagnostics[0].code, Code::Fb0204);
        }

        // 3. A row whose cell already failed FB0204 raises no FB0206 from the same cell:
        //    - claims.csv: `value = 5` and `unit = dollars` (unit fails FB0204; must not raise FB0206 for missing unit).
        let mut tree_no_double = mini_tree();
        let bad_unit = MINI_CLAIMS_CSV.replacen(",5,percent,", ",5,dollars,", 1);
        tree_no_double.insert_bytes("corpus/registers/claims.csv", bad_unit.into_bytes());
        let rep_no_double = run(
            &Host::new(tree_no_double, sample_today()),
            &Options::default(),
        );
        assert_eq!(
            rep_no_double.diagnostics.len(),
            1,
            "expected only FB0204 and no FB0206, got {:?}",
            rep_no_double.diagnostics
        );
        assert_eq!(rep_no_double.diagnostics[0].code, Code::Fb0204);

        //    - sources.csv: `source_family = far_15` and `family_basis = bad_basis` (family_basis fails FB0204; no FB0206).
        let mut tree_src_no_double = mini_tree();
        let bad_basis = MINI_SOURCES_CSV.replacen(
            ",synthetic_cost_rule,reviewed,",
            ",synthetic_cost_rule,bad_basis,",
            1,
        );
        tree_src_no_double.insert_bytes("corpus/registers/sources.csv", bad_basis.into_bytes());
        let rep_src_no_double = run(
            &Host::new(tree_src_no_double, sample_today()),
            &Options::default(),
        );
        assert_eq!(
            rep_src_no_double.diagnostics.len(),
            1,
            "expected only FB0204 and no FB0206, got {:?}",
            rep_src_no_double.diagnostics
        );
        assert_eq!(rep_src_no_double.diagnostics[0].code, Code::Fb0204);

        // 4. Remaining FB0206 entries (each with a breaking row and a passing counterpart):
        //    - sources.csv: `source_family` without `family_basis`
        let mut tree_fam = mini_tree();
        let bad_fam = MINI_SOURCES_CSV.replacen(
            ",synthetic_cost_rule,reviewed,",
            ",synthetic_cost_rule,,",
            1,
        );
        tree_fam.insert_bytes("corpus/registers/sources.csv", bad_fam.into_bytes());
        let rep_fam = run(&Host::new(tree_fam, sample_today()), &Options::default());
        assert_eq!(rep_fam.diagnostics.len(), 1);
        assert_eq!(rep_fam.diagnostics[0].code, Code::Fb0206);

        //    - claims.csv: `kind = assumption` with source_id set (breaks), vs clean assumption (passes in mini_tree)
        let mut tree_assump = mini_tree();
        let bad_assump = MINI_CLAIMS_CSV.replacen(
            "CL-EST-0001,est,assumption,Synthetic escalation placeholder is 3 percent per year for fixture examples.,2026-02-01,,,,,,",
            "CL-EST-0001,est,assumption,Synthetic escalation placeholder is 3 percent per year for fixture examples.,2026-02-01,S-0001,,,,,",
            1,
        );
        tree_assump.insert_bytes("corpus/registers/claims.csv", bad_assump.into_bytes());
        let rep_assump = run(&Host::new(tree_assump, sample_today()), &Options::default());
        assert_eq!(rep_assump.diagnostics.len(), 1);
        assert_eq!(rep_assump.diagnostics[0].code, Code::Fb0206);

        //    - claims.csv: `locator_kind = pdf_page_box` with `locator = sec. 1` (breaks), vs `p. 1` (passes)
        let mut tree_loc = mini_tree();
        let bad_loc = MINI_CLAIMS_CSV.replacen(",text_section,sec. 2,", ",pdf_page_box,sec. 2,", 1);
        tree_loc.insert_bytes("corpus/registers/claims.csv", bad_loc.into_bytes());
        let rep_loc = run(&Host::new(tree_loc, sample_today()), &Options::default());
        assert_eq!(rep_loc.diagnostics.len(), 1);
        assert_eq!(rep_loc.diagnostics[0].code, Code::Fb0206);

        //    - claims.csv: `value` without `unit` (breaks), vs `5,percent` (passes)
        let mut tree_val = mini_tree();
        let bad_val = MINI_CLAIMS_CSV.replacen(",5,percent,", ",5,,", 1);
        tree_val.insert_bytes("corpus/registers/claims.csv", bad_val.into_bytes());
        let rep_val = run(&Host::new(tree_val, sample_today()), &Options::default());
        assert_eq!(rep_val.diagnostics.len(), 1);
        assert_eq!(rep_val.diagnostics[0].code, Code::Fb0206);

        //    - claims.csv: `verification = unchecked` with `verified_on` set (breaks)
        let mut tree_unchk = mini_tree();
        let bad_unchk = MINI_CLAIMS_CSV.replacen(
            ",5,percent,2026-01-15,,,,,,,,,,,,,,Q-001,",
            ",5,percent,2026-01-15,,,,,,,,,2026-02-10,,,,,Q-001,",
            1,
        );
        tree_unchk.insert_bytes("corpus/registers/claims.csv", bad_unchk.into_bytes());
        let rep_unchk = run(&Host::new(tree_unchk, sample_today()), &Options::default());
        assert_eq!(rep_unchk.diagnostics.len(), 1);
        assert_eq!(rep_unchk.diagnostics[0].code, Code::Fb0206);

        //    - claims.csv: `verification = primary_source_supported` without `verified_on` (breaks)
        let mut tree_supp = mini_tree();
        let bad_supp = MINI_CLAIMS_CSV.replacen(",2026-02-10,owner,", ",,owner,", 1);
        tree_supp.insert_bytes("corpus/registers/claims.csv", bad_supp.into_bytes());
        let rep_supp = run(&Host::new(tree_supp, sample_today()), &Options::default());
        assert_eq!(rep_supp.diagnostics.len(), 1);
        assert_eq!(rep_supp.diagnostics[0].code, Code::Fb0206);

        //    - claims.csv: `verification = unresolved` with and without `question_id`
        let pass_unres = "CL-EST-0004,est,fact,Unresolved claim.,2026-02-01,S-0001,1,text_section,sec. 1,Quote.,false,codex,unresolved,pending_company_confirmation,proposed,observed,V2,,,,,,,,,,,,,,,,,Q-001,\n";
        let fail_unres = "CL-EST-0004,est,fact,Unresolved claim.,2026-02-01,S-0001,1,text_section,sec. 1,Quote.,false,codex,unresolved,pending_company_confirmation,proposed,observed,V2,,,,,,,,,,,,,,,,,,\n";
        for (row, expect_len) in [(pass_unres, 0usize), (fail_unres, 1usize)] {
            let mut t = mini_tree();
            t.insert_bytes(
                "corpus/registers/claims.csv",
                format!("{MINI_CLAIMS_CSV}{row}").into_bytes(),
            );
            let r = run(&Host::new(t, sample_today()), &Options::default());
            assert_eq!(r.diagnostics.len(), expect_len, "{:?}", r.diagnostics);
            if expect_len == 1 {
                assert_eq!(r.diagnostics[0].code, Code::Fb0206);
            }
        }

        //    - claims.csv: `verification = contradicted` with and without `conflict_id`
        let pass_contra = "CL-EST-0004,est,fact,Contradicted claim.,2026-02-01,S-0001,1,text_section,sec. 1,Quote.,false,codex,contradicted,pending_company_confirmation,proposed,observed,V2,,,,,,,,,,,,2026-02-10,owner,,,XC-01,,\n";
        let fail_contra = "CL-EST-0004,est,fact,Contradicted claim.,2026-02-01,S-0001,1,text_section,sec. 1,Quote.,false,codex,contradicted,pending_company_confirmation,proposed,observed,V2,,,,,,,,,,,,2026-02-10,owner,,,,,\n";
        for (row, expect_len) in [(pass_contra, 0usize), (fail_contra, 1usize)] {
            let mut t = mini_tree();
            t.insert_bytes(
                "corpus/registers/claims.csv",
                format!("{MINI_CLAIMS_CSV}{row}").into_bytes(),
            );
            let r = run(&Host::new(t, sample_today()), &Options::default());
            assert_eq!(r.diagnostics.len(), expect_len, "{:?}", r.diagnostics);
            if expect_len == 1 {
                assert_eq!(r.diagnostics[0].code, Code::Fb0206);
            }
        }

        //    - claims.csv: `verification = stale` with and without reason
        let pass_stale = "CL-EST-0004,est,fact,Stale claim.,2026-02-01,S-0001,1,text_section,sec. 1,Quote.,false,codex,stale,pending_company_confirmation,proposed,observed,V2,,,,,,,,,,,,,,,,,,review passed on 2026-03-01.\n";
        let fail_stale = "CL-EST-0004,est,fact,Stale claim.,2026-02-01,S-0001,1,text_section,sec. 1,Quote.,false,codex,stale,pending_company_confirmation,proposed,observed,V2,,,,,,,,,,,,,,,,,,ordinary note.\n";
        for (row, expect_len) in [(pass_stale, 0usize), (fail_stale, 1usize)] {
            let mut t = mini_tree();
            t.insert_bytes(
                "corpus/registers/claims.csv",
                format!("{MINI_CLAIMS_CSV}{row}").into_bytes(),
            );
            let r = run(&Host::new(t, sample_today()), &Options::default());
            assert_eq!(r.diagnostics.len(), expect_len, "{:?}", r.diagnostics);
            if expect_len == 1 {
                assert_eq!(r.diagnostics[0].code, Code::Fb0206);
            }
        }

        //    - claims.csv: `instrument = class_deviation` dual numbering (pass vs missing citation_rfo)
        let pass_inst = "CL-EST-0004,est,fact,Deviation claim.,2026-02-01,S-0001,1,text_section,sec. 1,Quote.,false,codex,unchecked,pending_company_confirmation,proposed,observed,V2,,,2026-01-15,,class_deviation,,2026-O0001,,deviation_text,,,,,,,,,\n";
        let fail_inst = "CL-EST-0004,est,fact,Deviation claim.,2026-02-01,S-0001,1,text_section,sec. 1,Quote.,false,codex,unchecked,pending_company_confirmation,proposed,observed,V2,,,2026-01-15,,class_deviation,,,,deviation_text,,,,,,,,,\n";
        for (row, expect_len) in [(pass_inst, 0usize), (fail_inst, 1usize)] {
            let mut t = mini_tree();
            t.insert_bytes(
                "corpus/registers/claims.csv",
                format!("{MINI_CLAIMS_CSV}{row}").into_bytes(),
            );
            let r = run(&Host::new(t, sample_today()), &Options::default());
            assert_eq!(r.diagnostics.len(), expect_len, "{:?}", r.diagnostics);
            if expect_len == 1 {
                assert_eq!(r.diagnostics[0].code, Code::Fb0206);
            }
        }

        //    - claims.csv: `text_status` set without `instrument`
        let fail_tstat = "CL-EST-0004,est,fact,Text status without instrument.,2026-02-01,S-0001,1,text_section,sec. 1,Quote.,false,codex,unchecked,pending_company_confirmation,proposed,observed,V2,,,,,,,,,final,,,,,,,,,\n";
        let mut t_tstat = mini_tree();
        t_tstat.insert_bytes(
            "corpus/registers/claims.csv",
            format!("{MINI_CLAIMS_CSV}{fail_tstat}").into_bytes(),
        );
        let r_tstat = run(&Host::new(t_tstat, sample_today()), &Options::default());
        assert_eq!(r_tstat.diagnostics.len(), 1);
        assert_eq!(r_tstat.diagnostics[0].code, Code::Fb0206);

        //    - conflicts.csv: resolved row (pass vs missing resolution) and unresolved row without question_id
        let pass_conf = "XC-02,cmp,Resolved topic,Pos 1|Pos 2,Use Pos 1.,Primary text.,CL-CMP-0001,2026-02-15,resolved,CL-CMP-0001|CL-CMP-0002,\n";
        let fail_conf_res = "XC-02,cmp,Resolved topic,Pos 1|Pos 2,,Primary text.,CL-CMP-0001,2026-02-15,resolved,CL-CMP-0001|CL-CMP-0002,\n";
        let fail_conf_unres =
            "XC-02,cmp,Unresolved topic,Pos 1|Pos 2,,,,,unresolved,CL-CMP-0001|CL-CMP-0002,\n";
        for (row, expect_len) in [
            (pass_conf, 0usize),
            (fail_conf_res, 1usize),
            (fail_conf_unres, 1usize),
        ] {
            let mut t = mini_tree();
            t.insert_bytes(
                "corpus/registers/conflicts.csv",
                format!("{MINI_CONFLICTS_CSV}{row}").into_bytes(),
            );
            let r = run(&Host::new(t, sample_today()), &Options::default());
            assert_eq!(r.diagnostics.len(), expect_len, "{:?}", r.diagnostics);
            if expect_len == 1 {
                assert_eq!(r.diagnostics[0].code, Code::Fb0206);
            }
        }

        //    - glossary.csv: `public_reference` without `source_ref` (breaks) vs with `source_ref` (passes in mini_tree)
        let mut t_gloss = mini_tree();
        let bad_gloss = MINI_GLOSSARY_CSV.replacen(
            ",cmp,CL-CMP-0002,public_reference,",
            ",cmp,,public_reference,",
            1,
        );
        t_gloss.insert_bytes("corpus/registers/glossary.csv", bad_gloss.into_bytes());
        let r_gloss = run(&Host::new(t_gloss, sample_today()), &Options::default());
        assert_eq!(r_gloss.diagnostics.len(), 1);
        assert_eq!(r_gloss.diagnostics[0].code, Code::Fb0206);
    }
}
