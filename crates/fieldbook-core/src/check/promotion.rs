//! Check pass `promotion`: `FB0209`, `FB0213`, `FB0220`, `FB0221`, `FB0222` and `FB0230`
//! (`docs/reference/corpus-format.md#promotion-rules`, `docs/reference/corpus-format.md#dates`).
//!
//! **Responsibility.**
//! - `FB0209`: broken promotion rules on `primary_source_supported` claims (conditions 1 to 5 of
//!   Promotion rules, including the instrument-numbering table and unsupported extension locators)
//!   and any consequential claim verified by a role other than `owner`.
//! - `FB0213`: a `primary_source_supported` claim resting on a `quarantined` or `withdrawn`
//!   source version, reported at the claim's `version` cell.
//! - `FB0220`: a claim whose known `as_of` is after `Model::today()`, reported as a warning at
//!   `as_of`.
//! - `FB0221`: a claim whose computed `review_by` (from the later known `as_of` or `verified_on`,
//!   plus the `volatility` interval) is strictly before `Model::today()`, reported as a warning.
//! - `FB0222`: a claim resting on a source version whose computed `superseded` state is `true`,
//!   reported as a warning at `version`.
//! - `FB0230`: a `day_one = true` question with an empty cell among `owner_role`, `blocks`,
//!   `why_it_matters`, `default_assumption` and `work_can_continue`, reported at the first empty
//!   cell (a warning without `--strict` and an error with `--strict`).

use std::collections::BTreeMap;

use crate::date::{CivilDate, PartialDate, review_by};
use crate::ids::VersionId;
use crate::schema::Register;
use crate::schema::claims::ClaimRow;
use crate::schema::locator::{ExtensionResolution, Locator, LocatorKind};
use crate::schema::questions::QuestionRow;
use crate::schema::sources::SourceRow;
use crate::schema::vocab::{
    AuthorityTier, Instrument, Lifecycle, SourceRole, Verification, VerifiedByRole,
};

use super::registers::register_path;
use super::{Code, Diagnostic, Located, Model, Options, Report};

/// Run the `promotion` check pass (`FB0209`, `FB0213`, `FB0220`..`FB0222`, `FB0230`) over `model`.
pub fn check(model: &Model, options: &Options, report: &mut Report) {
    if model.corpus().is_none() {
        return;
    }

    let strict = options.strict;
    let source_map = build_source_map(model.sources());

    for located in model.claims() {
        check_claim(model, &source_map, located, strict, report);
    }

    for located in model.questions() {
        check_question(model, located, strict, report);
    }
}

fn build_source_map(sources: &[Located<SourceRow>]) -> BTreeMap<VersionId, &SourceRow> {
    let mut map = BTreeMap::new();
    for located in sources {
        if let Ok(vid) = VersionId::parse(&format!(
            "{}-v{}",
            located.value.source_id, located.value.version
        )) {
            map.entry(vid).or_insert(&located.value);
        }
    }
    map
}

fn col_1based(model: &Model, reg: Register, line: u32, col_name: &str) -> usize {
    let idx = reg
        .columns()
        .iter()
        .position(|c| c.name == col_name)
        .unwrap_or(0);
    model
        .register_rows(reg)
        .iter()
        .find(|r| r.line == line)
        .and_then(|r| r.cols.get(idx).copied())
        .and_then(|c| usize::try_from(c).ok())
        .unwrap_or(1)
}

#[expect(
    clippy::too_many_arguments,
    reason = "internal diagnostic emission helper"
)]
fn push_diag(
    model: &Model,
    reg: Register,
    line: u32,
    col_name: &str,
    code: Code,
    strict: bool,
    message: String,
    report: &mut Report,
) {
    report.push(Diagnostic::new(
        code,
        strict,
        register_path(reg),
        line as usize,
        col_1based(model, reg, line, col_name),
        message,
    ));
}

/// Required citation column for a regulatory `instrument` at promotion (`FB0209`).
#[must_use]
pub const fn required_citation_column(instrument: Instrument) -> &'static str {
    match instrument {
        Instrument::FarCodified
        | Instrument::DfarsCodified
        | Instrument::Cas
        | Instrument::FederalRegisterNotice
        | Instrument::Statute => "citation_codified",
        Instrument::RfoModelText | Instrument::ClassDeviation | Instrument::ProposedRule => {
            "citation_rfo"
        }
    }
}

fn has_available_fine_locator(claim: &ClaimRow) -> Result<(), &'static str> {
    let Some(ref kind) = claim.locator_kind else {
        return Err("locator_kind");
    };
    match kind {
        LocatorKind::NoFineLocator => Err("locator_kind"),
        LocatorKind::Extension(_) => {
            let Some(ref raw_loc) = claim.locator else {
                return Err("locator");
            };
            match Locator::parse(raw_loc)
                .ok()
                .and_then(|l| l.resolve_extension())
            {
                Some(ExtensionResolution::Unsupported { .. }) | None => Err("locator"),
            }
        }
        LocatorKind::PdfPageBox
        | LocatorKind::TextSection
        | LocatorKind::SheetCells
        | LocatorKind::RecordKey
        | LocatorKind::RecordField => {
            if claim.locator.is_none() {
                Err("locator")
            } else {
                Ok(())
            }
        }
    }
}

fn check_claim(
    model: &Model,
    source_map: &BTreeMap<VersionId, &SourceRow>,
    located: &Located<ClaimRow>,
    strict: bool,
    report: &mut Report,
) {
    let reg = Register::Claims;
    let line = located.line;
    let claim = &located.value;
    let cid = &claim.claim_id;

    let is_supported = claim.verification == Verification::PrimarySourceSupported;
    let is_record_field = matches!(claim.locator_kind, Some(LocatorKind::RecordField));

    if is_supported {
        // Condition 1: source-version claim requires `source_role = primary_evidence` and
        // (`authority_tier` in {A, B} or a company `source_kind`).
        if !is_record_field {
            match (&claim.source_id, claim.version) {
                (None, _) => {
                    push_diag(
                        model,
                        reg,
                        line,
                        "source_id",
                        Code::Fb0209,
                        strict,
                        format!(
                            "supported claim `{cid}` requires a primary source version (`source_id` is empty)"
                        ),
                        report,
                    );
                }
                (Some(_), None) => {
                    push_diag(
                        model,
                        reg,
                        line,
                        "version",
                        Code::Fb0209,
                        strict,
                        format!(
                            "supported claim `{cid}` requires a primary source version (`version` is empty)"
                        ),
                        report,
                    );
                }
                (Some(sid), Some(ver)) => {
                    if let Ok(vid) = VersionId::parse(&format!("{sid}-v{ver}"))
                        && let Some(src) = source_map.get(&vid)
                    {
                        let role_already_invalid = matches!(
                            src.source_role,
                            SourceRole::GeneratedSynthesis | SourceRole::WorkingDraft
                        );
                        let role_ok =
                            src.source_role == SourceRole::PrimaryEvidence || role_already_invalid;
                        let tier_ok = matches!(
                            src.authority_tier,
                            Some(AuthorityTier::A | AuthorityTier::B)
                        ) || src.source_kind.is_overlay();
                        if !role_ok || !tier_ok {
                            push_diag(
                                model,
                                reg,
                                line,
                                "source_id",
                                Code::Fb0209,
                                strict,
                                format!(
                                    "supported claim `{cid}` cites `{vid}` (`source_role = {}`, `authority_tier = {}`), which cannot supply primary support",
                                    src.source_role,
                                    src.authority_tier.map_or("none", AuthorityTier::as_str)
                                ),
                                report,
                            );
                        }
                    }
                }
            }
        }

        // Condition 2: available fine locator, plus a `quote` when consequential.
        if let Err(col_name) = has_available_fine_locator(claim) {
            push_diag(
                model,
                reg,
                line,
                col_name,
                Code::Fb0209,
                strict,
                format!("supported claim `{cid}` requires an available fine locator"),
                report,
            );
        }
        if claim.consequential && claim.quote.is_none() {
            push_diag(
                model,
                reg,
                line,
                "quote",
                Code::Fb0209,
                strict,
                format!("supported consequential claim `{cid}` requires a `quote`"),
                report,
            );
        }

        // Condition 3: `as_of` is set; regulatory claims also have `effective_from` and the
        // required numbering cell for their `instrument`.
        if claim.as_of.is_none() {
            push_diag(
                model,
                reg,
                line,
                "as_of",
                Code::Fb0209,
                strict,
                format!("supported claim `{cid}` requires `as_of`"),
                report,
            );
        }
        if let Some(inst) = claim.instrument {
            if claim.effective_from.is_none() {
                push_diag(
                    model,
                    reg,
                    line,
                    "effective_from",
                    Code::Fb0209,
                    strict,
                    format!(
                        "supported regulatory claim `{cid}` (`instrument = {inst}`) requires `effective_from`"
                    ),
                    report,
                );
            }
            let req_col = required_citation_column(inst);
            let has_req_citation = match req_col {
                "citation_codified" => claim.citation_codified.is_some(),
                "citation_rfo" => claim.citation_rfo.is_some(),
                _ => true,
            };
            if !has_req_citation {
                push_diag(
                    model,
                    reg,
                    line,
                    req_col,
                    Code::Fb0209,
                    strict,
                    format!(
                        "supported regulatory claim `{cid}` (`instrument = {inst}`) requires `{req_col}`"
                    ),
                    report,
                );
            }
        }

        // Condition 4: a number has both `value` and `unit`.
        if claim.value.is_some() && claim.unit.is_none() {
            push_diag(
                model,
                reg,
                line,
                "unit",
                Code::Fb0209,
                strict,
                format!("supported numeric claim `{cid}` has `value` without `unit`"),
                report,
            );
        } else if claim.value.is_none() && claim.unit.is_some() {
            push_diag(
                model,
                reg,
                line,
                "value",
                Code::Fb0209,
                strict,
                format!("supported numeric claim `{cid}` has `unit` without `value`"),
                report,
            );
        }

        // Condition 5: `verified_on` and `verified_by_role` are set.
        if claim.verified_on.is_none() {
            push_diag(
                model,
                reg,
                line,
                "verified_on",
                Code::Fb0209,
                strict,
                format!("supported claim `{cid}` requires `verified_on`"),
                report,
            );
        }
        if claim.verified_by_role.is_none() {
            push_diag(
                model,
                reg,
                line,
                "verified_by_role",
                Code::Fb0209,
                strict,
                format!("supported claim `{cid}` requires `verified_by_role`"),
                report,
            );
        }
    }

    // Consequential claim verified by a role other than `owner` (`FB0209`).
    if claim.consequential
        && let Some(role) = claim.verified_by_role
        && role != VerifiedByRole::Owner
    {
        push_diag(
            model,
            reg,
            line,
            "verified_by_role",
            Code::Fb0209,
            strict,
            format!("consequential claim `{cid}` may only be verified by `owner` (found `{role}`)"),
            report,
        );
    }

    // `FB0213` (supported claim on quarantined/withdrawn version) and `FB0222` (claim on superseded version),
    // both reported at the claim's `version` cell.
    if !is_record_field
        && let (Some(sid), Some(ver)) = (&claim.source_id, claim.version)
        && let Ok(vid) = VersionId::parse(&format!("{sid}-v{ver}"))
        && let Some(entry) = model.index().source_versions.get(&vid)
    {
        if is_supported
            && matches!(
                entry.lifecycle,
                Lifecycle::Quarantined | Lifecycle::Withdrawn
            )
        {
            push_diag(
                model,
                reg,
                line,
                "version",
                Code::Fb0213,
                strict,
                format!(
                    "supported claim `{cid}` rests on `{vid}`, whose `lifecycle` is `{}`",
                    entry.lifecycle
                ),
                report,
            );
        }
        if entry.superseded {
            push_diag(
                model,
                reg,
                line,
                "version",
                Code::Fb0222,
                strict,
                format!("claim `{cid}` rests on superseded source version `{vid}`"),
                report,
            );
        }
    }

    // `FB0220` (`as_of` after today) and `FB0221` (`review_by` passed).
    let today = model.today();
    if let Some(as_of) = claim.as_of
        && as_of.first_day() > today
    {
        push_diag(
            model,
            reg,
            line,
            "as_of",
            Code::Fb0220,
            strict,
            format!("claim `{cid}` has `as_of = {as_of}` after today (`{today}`)"),
            report,
        );
    }

    let base_date: Option<CivilDate> =
        match (claim.as_of.map(PartialDate::first_day), claim.verified_on) {
            (Some(a), Some(v)) => Some(a.max(v)),
            (Some(a), None) => Some(a),
            (None, Some(v)) => Some(v),
            (None, None) => None,
        };
    if let Some(base) = base_date {
        let review_date = review_by(base, claim.volatility);
        if review_date < today {
            let col_name = if claim.as_of.is_some() {
                "as_of"
            } else {
                "verified_on"
            };
            push_diag(
                model,
                reg,
                line,
                col_name,
                Code::Fb0221,
                strict,
                format!(
                    "claim `{cid}` `review_by` (`{review_date}`) has passed (`today = {today}`)"
                ),
                report,
            );
        }
    }
}

fn check_question(
    model: &Model,
    located: &Located<QuestionRow>,
    strict: bool,
    report: &mut Report,
) {
    let q = &located.value;
    if !q.day_one {
        return;
    }

    let first_empty = if q.owner_role.is_none() {
        Some("owner_role")
    } else if q.blocks.is_none() {
        Some("blocks")
    } else if q.why_it_matters.is_none() {
        Some("why_it_matters")
    } else if q.default_assumption.is_none() {
        Some("default_assumption")
    } else if q.work_can_continue.is_none() {
        Some("work_can_continue")
    } else {
        None
    };

    if let Some(col_name) = first_empty {
        push_diag(
            model,
            Register::Questions,
            located.line,
            col_name,
            Code::Fb0230,
            strict,
            format!(
                "day-one question `{}` has an empty `{col_name}` cell",
                q.question_id
            ),
            report,
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::check::{Code, Host, Options, Severity, Tree, run};
    use crate::date::CivilDate;
    use crate::schema::vocab::Instrument;

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

    #[test]
    fn fb0209_each_condition_and_lead_verified_consequential_claim() {
        // Condition 1: supported claim on a source with `source_role != primary_evidence` or tier D (`S-0003-v1`)
        let mut tree = mini_tree();
        let claims_cond1 = MINI_CLAIMS_CSV.replace(
            "CL-CMP-0001,cmp,fact,Synthetic compliance reviews occur every 30 days under the synthetic rule.,2026-02-01,S-0001,1,",
            "CL-CMP-0001,cmp,fact,Synthetic compliance reviews occur every 30 days under the synthetic rule.,2026-02-01,S-0003,1,",
        );
        tree.insert_bytes("corpus/registers/claims.csv", claims_cond1.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == Code::Fb0209 && d.line == 2),
            "expected FB0209 for condition 1: {:?}",
            report.diagnostics
        );

        // Condition 2a: supported non-consequential claim with `no_fine_locator`
        let mut tree = mini_tree();
        let claims_cond2a = MINI_CLAIMS_CSV.replace(
            "CL-CMP-0001,cmp,fact,Synthetic compliance reviews occur every 30 days under the synthetic rule.,2026-02-01,S-0001,1,text_section,sec. 1,Synthetic compliance reviews occur every 30 days.,true,codex,primary_source_supported",
            "CL-CMP-0001,cmp,fact,Synthetic compliance reviews occur every 30 days under the synthetic rule.,2026-02-01,S-0001,1,no_fine_locator,,Synthetic compliance reviews occur every 30 days.,false,codex,primary_source_supported",
        );
        tree.insert_bytes("corpus/registers/claims.csv", claims_cond2a.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        let codes: Vec<Code> = report.diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(
            codes,
            vec![Code::Fb0209],
            "diagnostics: {:?}",
            report.diagnostics
        );

        // Condition 2b: supported consequential claim with empty `quote`
        let mut tree = mini_tree();
        let claims_cond2b = MINI_CLAIMS_CSV.replace(
            "text_section,sec. 1,Synthetic compliance reviews occur every 30 days.,true,codex,primary_source_supported",
            "text_section,sec. 1,,true,codex,primary_source_supported",
        );
        tree.insert_bytes("corpus/registers/claims.csv", claims_cond2b.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == Code::Fb0209 && d.line == 2),
            "expected FB0209 for condition 2b (missing quote): {:?}",
            report.diagnostics
        );

        // Condition 3a: supported claim without `as_of`
        let mut tree = mini_tree();
        let claims_cond3a = MINI_CLAIMS_CSV.replace(
            "CL-CMP-0001,cmp,fact,Synthetic compliance reviews occur every 30 days under the synthetic rule.,2026-02-01,",
            "CL-CMP-0001,cmp,fact,Synthetic compliance reviews occur every 30 days under the synthetic rule.,,",
        );
        tree.insert_bytes("corpus/registers/claims.csv", claims_cond3a.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == Code::Fb0209 && d.line == 2),
            "expected FB0209 for condition 3a (missing as_of): {:?}",
            report.diagnostics
        );

        // Condition 3b: supported regulatory claim without `effective_from`
        let mut tree = mini_tree();
        let claims_cond3b = MINI_CLAIMS_CSV.replace(
            ",V2,30,days,2026-01-15,,far_codified,",
            ",V2,30,days,,,far_codified,",
        );
        tree.insert_bytes("corpus/registers/claims.csv", claims_cond3b.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        let codes: Vec<Code> = report.diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(
            codes,
            vec![Code::Fb0209],
            "diagnostics: {:?}",
            report.diagnostics
        );

        // Condition 4: supported numeric claim with `value` and no `unit` (or `unit` and no `value`)
        let mut tree = mini_tree();
        let claims_cond4 =
            MINI_CLAIMS_CSV.replace(",V2,30,days,2026-01-15,", ",V2,30,,2026-01-15,");
        tree.insert_bytes("corpus/registers/claims.csv", claims_cond4.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == Code::Fb0209 && d.line == 2),
            "expected FB0209 for condition 4 (value without unit): {:?}",
            report.diagnostics
        );

        // Condition 5: supported claim without `verified_on`
        let mut tree = mini_tree();
        let claims_cond5 = MINI_CLAIMS_CSV.replace(",2026-02-10,owner,", ",,owner,");
        tree.insert_bytes("corpus/registers/claims.csv", claims_cond5.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == Code::Fb0209 && d.line == 2),
            "expected FB0209 for condition 5 (missing verified_on): {:?}",
            report.diagnostics
        );

        // Consequential claim verified by `lead` instead of `owner`
        let mut tree = mini_tree();
        let claims_lead = MINI_CLAIMS_CSV.replace(",2026-02-10,owner,", ",2026-02-10,lead,");
        tree.insert_bytes("corpus/registers/claims.csv", claims_lead.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        let codes: Vec<Code> = report.diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(
            codes,
            vec![Code::Fb0209],
            "diagnostics: {:?}",
            report.diagnostics
        );
    }

    #[test]
    fn promotion_numbering_requirements_match_instrument() {
        for inst in Instrument::ALL {
            let inst_str = inst.as_str();
            let req_col = super::required_citation_column(inst);

            // 1. Valid supported claim for this instrument (with required cell filled and permitted counterpart empty)
            let (codified, rfo) = if req_col == "citation_codified" {
                ("48 CFR 9900.101", "")
            } else {
                ("", "RFO-2026-01")
            };
            let mut tree = mini_tree();
            let valid_claims = MINI_CLAIMS_CSV.replace(
                "far_codified,48 CFR 9900.101,,2026-01,final",
                &format!("{inst_str},{codified},{rfo},2026-01,final"),
            );
            tree.insert_bytes("corpus/registers/claims.csv", valid_claims.as_bytes());
            let report = run(&Host::new(tree, sample_today()), &Options::new(false));
            assert!(
                report.diagnostics.is_empty(),
                "expected valid supported claim for instrument `{inst_str}` to be clean, got: {:?}",
                report.diagnostics
            );

            // 2. Required citation cell empty -> must raise FB0209 (and retain independent FB0206 for the 3 dual-numbering instruments)
            let (empty_cod, empty_rfo) = if req_col == "citation_codified" {
                ("", "RFO-2026-01")
            } else {
                ("48 CFR 9900.101", "")
            };
            let mut tree_bad = mini_tree();
            let bad_claims = MINI_CLAIMS_CSV.replace(
                "far_codified,48 CFR 9900.101,,2026-01,final",
                &format!("{inst_str},{empty_cod},{empty_rfo},2026-01,final"),
            );
            tree_bad.insert_bytes("corpus/registers/claims.csv", bad_claims.as_bytes());
            let report_bad = run(&Host::new(tree_bad, sample_today()), &Options::new(false));
            assert!(
                report_bad
                    .diagnostics
                    .iter()
                    .any(|d| d.code == Code::Fb0209 && d.line == 2),
                "expected FB0209 when required citation `{req_col}` is empty for `{inst_str}`: {:?}",
                report_bad.diagnostics
            );
            if matches!(
                inst,
                Instrument::RfoModelText | Instrument::ClassDeviation | Instrument::ProposedRule
            ) {
                assert!(
                    report_bad
                        .diagnostics
                        .iter()
                        .any(|d| d.code == Code::Fb0206 && d.line == 2),
                    "expected independent FB0206 for `{inst_str}` when `citation_rfo` is empty: {:?}",
                    report_bad.diagnostics
                );
            } else {
                let codes: Vec<Code> = report_bad.diagnostics.iter().map(|d| d.code).collect();
                assert_eq!(
                    codes,
                    vec![Code::Fb0209],
                    "expected only FB0209 for `{inst_str}`: {:?}",
                    report_bad.diagnostics
                );
            }
        }

        // 3. Non-regulatory supported claim (instrument empty, both citation cells empty, text_status empty)
        let mut tree_nonreg = mini_tree();
        let nonreg_exact = MINI_CLAIMS_CSV.replace(
            ",2026-01-15,,far_codified,48 CFR 9900.101,,2026-01,final,",
            ",,,,,,,,",
        );
        tree_nonreg.insert_bytes("corpus/registers/claims.csv", nonreg_exact.as_bytes());
        let report_nonreg = run(
            &Host::new(tree_nonreg, sample_today()),
            &Options::new(false),
        );
        assert!(
            report_nonreg.diagnostics.is_empty(),
            "expected non-regulatory supported claim with empty citations to be clean: {:?}",
            report_nonreg.diagnostics
        );
    }

    #[test]
    fn unsupported_extension_cannot_support_a_claim() {
        // Supported claim with `ext:structured_path` and `ext structured_path %2Froot` raises FB0209
        let mut tree = mini_tree();
        let ext_supported = MINI_CLAIMS_CSV.replace(
            "S-0001,1,text_section,sec. 1,",
            "S-0001,1,ext:structured_path,ext structured_path %2Froot%2Fitem,",
        );
        tree.insert_bytes("corpus/registers/claims.csv", ext_supported.as_bytes());
        let (model, report) =
            crate::check::load(&Host::new(tree, sample_today()), &Options::new(false));
        // Parsed kind and payload are preserved on the model's ClaimRow
        let claim = &model.claims()[0].value;
        assert_eq!(
            claim.locator_kind,
            Some(crate::schema::locator::LocatorKind::Extension(
                "structured_path".to_owned()
            ))
        );
        assert_eq!(
            claim.locator.as_deref(),
            Some("ext structured_path %2Froot%2Fitem")
        );
        let codes: Vec<Code> = report.diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(
            codes,
            vec![Code::Fb0209],
            "diagnostics: {:?}",
            report.diagnostics
        );

        // Ordinary unchecked row with an extension locator remains readable and clean
        let mut tree_unchecked = mini_tree();
        let ext_unchecked = MINI_CLAIMS_CSV.replace(
            "CL-CMP-0002,cmp,fact,Synthetic withholding threshold is 5 percent of billed amounts.,2026-02-01,S-0001,1,text_section,sec. 2,",
            "CL-CMP-0002,cmp,fact,Synthetic withholding threshold is 5 percent of billed amounts.,2026-02-01,S-0001,1,ext:structured_path,ext structured_path %2Fsec%2F2,",
        );
        tree_unchecked.insert_bytes("corpus/registers/claims.csv", ext_unchecked.as_bytes());
        let (model_u, report_u) = crate::check::load(
            &Host::new(tree_unchecked, sample_today()),
            &Options::new(false),
        );
        assert!(
            report_u.diagnostics.is_empty(),
            "expected unchecked extension-locator claim to be clean: {:?}",
            report_u.diagnostics
        );
        assert_eq!(
            model_u.claims()[1].value.locator.as_deref(),
            Some("ext structured_path %2Fsec%2F2")
        );
    }

    #[test]
    fn fb0213_supported_claim_on_quarantined_or_withdrawn_version_and_fb0222_superseded() {
        for lifecycle in ["quarantined", "withdrawn"] {
            let mut tree = mini_tree();
            let sources = MINI_SOURCES_CSV.replace(
                "2026-02-01,2026-02-01,active,,V2,public,Synthetic fixture source",
                &format!("2026-02-01,2026-02-01,{lifecycle},,V2,public,Synthetic fixture source"),
            );
            tree.insert_bytes("corpus/registers/sources.csv", sources.as_bytes());
            let report = run(&Host::new(tree, sample_today()), &Options::new(false));
            let codes: Vec<Code> = report.diagnostics.iter().map(|d| d.code).collect();
            assert_eq!(
                codes,
                vec![Code::Fb0213],
                "expected FB0213 for lifecycle `{lifecycle}`: {:?}",
                report.diagnostics
            );
            assert_eq!(report.diagnostics[0].path, "corpus/registers/claims.csv");
            assert_eq!(report.diagnostics[0].line, 2);
            assert_eq!(report.diagnostics[0].col, 115); // 1-based code-point column of `version`
        }

        // FB0222: claim resting on `S-0002-v1`, which is superseded by `S-0002-v2`
        let mut tree = mini_tree();
        let claims = MINI_CLAIMS_CSV.replace(
            "CL-EST-0003,est,fact,Synthetic rate review window is 10 days under current guidance.,2026-02-01,S-0002,2,",
            "CL-EST-0003,est,fact,Synthetic rate review window is 10 days under current guidance.,2026-02-01,S-0002,1,",
        );
        tree.insert_bytes("corpus/registers/claims.csv", claims.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(true));
        let codes: Vec<Code> = report.diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(
            codes,
            vec![Code::Fb0222],
            "diagnostics: {:?}",
            report.diagnostics
        );
        assert_eq!(report.diagnostics[0].severity, Severity::Warning);
        assert_eq!(report.diagnostics[0].line, 6);
        assert_eq!(report.diagnostics[0].col, 104); // 1-based code-point column of `version`
    }

    #[test]
    fn fb0220_partial_as_of_and_fb0221_review_by_boundary() {
        // Partial `as_of = 2026-04` compares as `2026-04-01`:
        // - on `today = 2026-03-31`, `2026-04-01 > 2026-03-31` -> raises FB0220 (warning even under strict)
        // - on `today = 2026-04-01`, `2026-04-01 == 2026-04-01` -> clean
        let mut tree = mini_tree();
        let claims_partial = MINI_CLAIMS_CSV.replace(
            "CL-CMP-0002,cmp,fact,Synthetic withholding threshold is 5 percent of billed amounts.,2026-02-01,",
            "CL-CMP-0002,cmp,fact,Synthetic withholding threshold is 5 percent of billed amounts.,2026-04,",
        );
        tree.insert_bytes("corpus/registers/claims.csv", claims_partial.as_bytes());

        let march_31 = CivilDate::from_ymd(2026, 3, 31).expect("valid date");
        let report_before = run(&Host::new(tree.clone(), march_31), &Options::new(true));
        assert_eq!(report_before.diagnostics.len(), 1);
        assert_eq!(report_before.diagnostics[0].code, Code::Fb0220);
        assert_eq!(report_before.diagnostics[0].severity, Severity::Warning);

        let april_1 = CivilDate::from_ymd(2026, 4, 1).expect("valid date");
        let report_on = run(&Host::new(tree, april_1), &Options::new(true));
        assert!(
            report_on.diagnostics.is_empty(),
            "expected partial as_of equal to today to be clean: {:?}",
            report_on.diagnostics
        );

        // Boundary day of `review_by`:
        // In corpus-mini, the earliest `review_by` is on `CL-EST-0002` (`as_of = 2026-01-15`, `V2` = +180d -> `2026-07-14`).
        // - On `today = 2026-07-14`, `review_by == today` has NOT passed -> 0 diagnostics.
        // - On `today = 2026-07-15`, `review_by < today` -> raises FB0221 on `CL-EST-0002` (warning even under `--strict`).
        let tree_boundary = mini_tree();
        let on_boundary = CivilDate::from_ymd(2026, 7, 14).expect("valid date");
        let report_boundary = run(
            &Host::new(tree_boundary.clone(), on_boundary),
            &Options::new(true),
        );
        assert!(
            report_boundary.diagnostics.is_empty(),
            "expected review_by equal to today not to pass: {:?}",
            report_boundary.diagnostics
        );

        let after_boundary = CivilDate::from_ymd(2026, 7, 15).expect("valid date");
        let report_after = run(
            &Host::new(tree_boundary, after_boundary),
            &Options::new(true),
        );
        assert_eq!(report_after.diagnostics.len(), 1);
        assert_eq!(report_after.diagnostics[0].code, Code::Fb0221);
        assert_eq!(report_after.diagnostics[0].severity, Severity::Warning);
        assert_eq!(report_after.diagnostics[0].line, 5); // CL-EST-0002
    }

    #[test]
    fn unknown_claim_dates_remain_unknown() {
        // An allowed `stale` claim with empty `as_of`, empty `verified_on`, and `note` starting `as_of unknown`
        // has neither `as_of` nor `review_by` and raises neither FB0220 nor FB0221.
        let mut tree = mini_tree();
        let claims = MINI_CLAIMS_CSV.replace(
            "CL-CMP-0002,cmp,fact,Synthetic withholding threshold is 5 percent of billed amounts.,2026-02-01,S-0001,1,text_section,sec. 2,Synthetic withholding threshold is 5 percent.,true,codex,unchecked,pending_company_confirmation,proposed,observed,V2,5,percent,2026-01-15,,,,,,,,,,,,,,Q-001,Synthetic unchecked consequential fact for fixture tests.",
            "CL-CMP-0002,cmp,fact,Synthetic withholding threshold is 5 percent of billed amounts.,,S-0001,1,text_section,sec. 2,Synthetic withholding threshold is 5 percent.,true,codex,stale,pending_company_confirmation,proposed,observed,V1,5,percent,2026-01-15,,,,,,,,,,,,,,Q-001,as_of unknown for legacy synthetic entry.",
        );
        tree.insert_bytes("corpus/registers/claims.csv", claims.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(true));
        assert!(
            report.diagnostics.is_empty(),
            "expected stale claim with unknown as_of to raise neither FB0220 nor FB0221: {:?}",
            report.diagnostics
        );
    }

    #[test]
    fn fb0230_is_warning_without_strict_and_error_with_strict() {
        let mut tree = mini_tree();
        let questions = MINI_QUESTIONS_CSV.replace(
            ",Synthetic reviews follow the 30-day cycle until confirmed.,yes,true,",
            ",,yes,true,",
        );
        tree.insert_bytes("corpus/registers/questions.csv", questions.as_bytes());

        let report_non_strict = run(
            &Host::new(tree.clone(), sample_today()),
            &Options::new(false),
        );
        assert_eq!(report_non_strict.diagnostics.len(), 1);
        assert_eq!(report_non_strict.diagnostics[0].code, Code::Fb0230);
        assert_eq!(report_non_strict.diagnostics[0].severity, Severity::Warning);
        assert_eq!(
            report_non_strict.diagnostics[0].path,
            "corpus/registers/questions.csv"
        );
        assert_eq!(report_non_strict.diagnostics[0].line, 2);
        assert_eq!(report_non_strict.diagnostics[0].col, 225); // 1-based code-point column of `default_assumption`
        assert_eq!(report_non_strict.errors(), 0);

        let report_strict = run(&Host::new(tree, sample_today()), &Options::new(true));
        assert_eq!(report_strict.diagnostics.len(), 1);
        assert_eq!(report_strict.diagnostics[0].code, Code::Fb0230);
        assert_eq!(report_strict.diagnostics[0].severity, Severity::Error);
        assert_eq!(report_strict.errors(), 1);
    }
}
