//! Pure planner for `fieldbook verify <claim-id> --on YYYY-MM-DD`.
//!
//! **Responsibility.** Plans the verification of one claim (the owner or lead raising
//! `verification` to `primary_source_supported` at a terminal). Eligibility is governed
//! by the promotion rules of `docs/reference/corpus-format.md#promotion-rules` (`FB0209`,
//! `FB0213`) shared with `check` through [`crate::check::promotion::promotion_problems`].
//!
//! **Rules.** Pure: no I/O, no clock. The date `on` stands in for today: this module
//! reads no clock.

use std::fmt;

use crate::audit::{Role, Sha256, request_sha256};
use crate::check::Model;
use crate::check::promotion::{PromotionProblem, consequential_role_problem, promotion_problems};
use crate::date::CivilDate;
use crate::ids::VersionId;
use crate::schema::locator::LocatorKind;
use crate::schema::table::{is_normal, normalize as normalize_register, write as write_csv};
use crate::schema::vocab::{Lifecycle, SourceRole, Verification, VerifiedByRole};
use crate::schema::{ClaimRow, Register, SourceRow};

/// The line required in `settings.toml` for the repository owner.
pub const ROLE_LINE: &str = "role = \"owner\"";

/// Message printed when standard input is not a terminal.
#[must_use]
pub fn terminal_message() -> String {
    "verify needs a terminal; it is for the owner at home".to_owned()
}

/// Message printed when `settings.toml` does not configure an authorized role.
#[must_use]
pub fn role_message() -> String {
    format!(
        "verify needs a role: add the line {ROLE_LINE} (or `role = \"lead\"`) to settings.toml in the state folder"
    )
}

/// The only verification status written by this command.
pub const STATUS: &str = "primary_source_supported";

/// Reasons why planning verification of a claim is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The caller holds no authorized role (`verify` requires `owner` or `lead`).
    NoRole,
    /// The named claim id does not exist in `claims.csv`.
    UnknownClaim(String),
    /// Either `claims.csv` or `sources.csv` did not load.
    MissingRegister,
    /// The claim is already verified on the specified date.
    AlreadyVerified {
        /// Identifier of the claim.
        claim: String,
        /// Date on which it was verified.
        on: CivilDate,
    },
    /// A consequential claim requires the repository owner to verify.
    NeedsOwner(String),
    /// The claim cites a source version whose lifecycle is not active.
    Inactive {
        /// Identifier of the claim.
        claim: String,
        /// Source version identifier.
        version: String,
        /// Inactive lifecycle status.
        lifecycle: String,
    },
    /// The claim's known `as_of` date is after the verification date.
    AsOfAfter {
        /// Identifier of the claim.
        claim: String,
        /// String representation of the claim's `as_of` date.
        as_of: String,
        /// The verification date.
        on: CivilDate,
    },
    /// The claim violates promotion rules (`FB0209`).
    Rule(Vec<PromotionProblem>),
    /// Catch-all for when `claims.csv` cannot be rewritten in normal form or hashed.
    Unplannable(String),
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoRole => f.write_str(&role_message()),
            Self::UnknownClaim(id) => write!(f, "no claim `{id}` in claims.csv"),
            Self::MissingRegister => {
                f.write_str("claims.csv or sources.csv did not load; run `fieldbook check`")
            }
            Self::AlreadyVerified { claim, on } => {
                write!(f, "claim `{claim}` is already verified on {on}")
            }
            Self::NeedsOwner(claim) => {
                write!(
                    f,
                    "FB0209: consequential claim `{claim}` may only be verified by `owner`"
                )
            }
            Self::Inactive {
                claim,
                version,
                lifecycle,
            } => {
                write!(
                    f,
                    "FB0213: claim `{claim}` rests on `{version}`, whose `lifecycle` is `{lifecycle}`"
                )
            }
            Self::AsOfAfter { claim, as_of, on } => {
                write!(
                    f,
                    "claim `{claim}` has `as_of = {as_of}` after the verification date {on}"
                )
            }
            Self::Rule(problems) => {
                let mut first = true;
                for prob in problems {
                    if !first {
                        f.write_str("\n")?;
                    }
                    first = false;
                    write!(f, "FB0209: {}", prob.message)?;
                }
                Ok(())
            }
            Self::Unplannable(detail) => {
                if detail.is_empty() {
                    f.write_str("claims.csv cannot be rewritten in normal form")
                } else {
                    f.write_str(detail)
                }
            }
        }
    }
}

impl std::error::Error for Refusal {}

/// The information shown to a person reviewing a claim for verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    /// Identifier of the claim (`CL-...`).
    pub claim_id: String,
    /// Statement text of the claim.
    pub statement: String,
    /// Source version identifier (`S-nnnn-vN`), or empty when none is cited.
    pub version: String,
    /// Title of the cited source version, or empty when none exists.
    pub source_title: String,
    /// Locator kind string as recorded in `claims.csv`.
    pub locator_kind: String,
    /// Fine locator string, or empty when none exists.
    pub locator: String,
    /// Quoted text, or empty when none exists.
    pub quote: String,
    /// The date on which the verification is conducted or claimed.
    pub on: CivilDate,
}

impl Shown {
    /// Formats the summary lines presented to the person before the confirmation question.
    #[must_use]
    pub fn summary(&self) -> String {
        let version = if self.version.is_empty() {
            "-"
        } else {
            &self.version
        };
        let title = if self.source_title.is_empty() {
            "-"
        } else {
            &self.source_title
        };
        let locator_kind = if self.locator_kind.is_empty() {
            "-"
        } else {
            &self.locator_kind
        };
        let locator = if self.locator.is_empty() {
            "-"
        } else {
            &self.locator
        };
        let quote = if self.quote.is_empty() {
            "-"
        } else {
            &self.quote
        };
        format!(
            "claim      {}\nstatement  {}\nsource     {version}  {title}\nlocator    {locator_kind}  {locator}\nquote      {quote}\n",
            self.claim_id, self.statement
        )
    }

    /// The confirmation question presented to the user.
    #[must_use]
    pub fn question(&self) -> String {
        format!("Verified at this locator on {}? Type yes", self.on)
    }

    /// Computes the SHA-256 confirmation hash over `summary()` and `question()`.
    #[must_use]
    pub fn confirmation_sha256(&self) -> Sha256 {
        let mut bytes = self.summary().into_bytes();
        bytes.extend_from_slice(self.question().as_bytes());
        Sha256::digest(&bytes)
    }
}

/// A planned claim verification ready for user confirmation and commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyPlan {
    /// The information shown to the reviewer.
    pub shown: Shown,
    /// Whether the claim is consequential.
    pub consequential: bool,
    /// The verification date.
    pub on: CivilDate,
    /// The role verifying the claim.
    pub role: Role,
    /// Repository-relative path of the register file being updated (`corpus/registers/claims.csv`).
    pub path: String,
    /// Full replacement bytes for `claims.csv` in canonical normal form.
    pub bytes: Vec<u8>,
    /// SHA-256 digest of displayed evidence.
    pub evidence_sha256: Sha256,
}

impl VerifyPlan {
    /// Returns the role word ("owner" or "lead") recorded in the plan.
    #[must_use]
    pub const fn role_word(&self) -> &'static str {
        match self.role {
            Role::Owner => "owner",
            Role::Lead => "lead",
            Role::Agent => "agent",
        }
    }
}

fn column(register: Register, name: &str) -> Option<usize> {
    register.columns().iter().position(|c| c.name == name)
}

fn cited_source<'a>(model: &'a Model, claim: &ClaimRow) -> Option<&'a SourceRow> {
    let is_record_field = matches!(claim.locator_kind, Some(LocatorKind::RecordField));
    if !is_record_field && let (Some(sid), Some(ver)) = (&claim.source_id, claim.version) {
        model
            .sources()
            .iter()
            .find(|s| s.value.source_id == *sid && s.value.version == ver)
            .map(|s| &s.value)
    } else {
        None
    }
}

/// Resolve the information displayed to a reviewer for the given claim.
///
/// Finds the typed claim and any cited source version, filling [`Shown`]. Computable
/// for any existing claim without eligibility checks.
///
/// # Errors
///
/// Returns [`Refusal::MissingRegister`] if either `claims.csv` or `sources.csv` did not
/// load, or [`Refusal::UnknownClaim`] if `claim_id` is not present in `claims.csv`.
pub fn shown(model: &Model, claim_id: &str, on: CivilDate) -> Result<Shown, Refusal> {
    if model.register(Register::Claims).is_none() || model.register(Register::Sources).is_none() {
        return Err(Refusal::MissingRegister);
    }

    let located_claim = model
        .claims()
        .iter()
        .find(|c| c.value.claim_id.to_string() == claim_id)
        .ok_or_else(|| Refusal::UnknownClaim(claim_id.to_owned()))?;
    let claim = &located_claim.value;

    let cited = cited_source(model, claim);

    let version = match (&claim.source_id, claim.version) {
        (Some(sid), Some(ver)) => format!("{sid}-v{ver}"),
        _ => String::new(),
    };
    let source_title = cited.map_or_else(String::new, |s| s.title.clone());
    let locator_kind = claim
        .locator_kind
        .as_ref()
        .map_or_else(String::new, ToString::to_string);
    let locator = claim.locator.clone().unwrap_or_default();
    let quote = claim.quote.clone().unwrap_or_default();

    Ok(Shown {
        claim_id: claim_id.to_owned(),
        statement: claim.statement.clone(),
        version,
        source_title,
        locator_kind,
        locator,
        quote,
        on,
    })
}

/// The two cases of `FB0209` condition 1 that [`promotion_problems`] leaves to other `check` codes
/// (`FB0207` for a version with no `sources.csv` row, `FB0204` for a `generated_synthesis` or
/// `working_draft` role). `check` reports them under those codes; `verify` never runs those codes,
/// so it refuses them here, naming `FB0209`. `check`'s own output is unchanged.
fn unreported_source_problem(
    claim: &ClaimRow,
    source: Option<&SourceRow>,
) -> Option<PromotionProblem> {
    if matches!(claim.locator_kind, Some(LocatorKind::RecordField)) {
        return None;
    }
    let (Some(sid), Some(ver)) = (&claim.source_id, claim.version) else {
        return None;
    };
    let cid = &claim.claim_id;
    match source {
        None => Some(PromotionProblem {
            column: "version",
            message: format!(
                "supported claim `{cid}` cites `{sid}-v{ver}`, which has no row in `sources.csv`"
            ),
        }),
        Some(src) if src.source_role != SourceRole::PrimaryEvidence => Some(PromotionProblem {
            column: "source_id",
            message: format!(
                "supported claim `{cid}` cites `{sid}-v{ver}` (`source_role = {}`), which cannot supply primary support",
                src.source_role
            ),
        }),
        Some(_) => None,
    }
}

/// Plan the verification of one claim.
///
/// # Errors
///
/// Returns [`Refusal`] if the role is unauthorized, registers did not load, the claim is
/// unknown or already verified, consequential ownership requirements fail, promotion rules
/// are violated, the cited version lifecycle is inactive, `as_of` is after `on`, or rewritten
/// bytes cannot be rendered.
pub fn plan(
    model: &Model,
    claim_id: &str,
    on: CivilDate,
    role: Role,
) -> Result<VerifyPlan, Refusal> {
    if role == Role::Agent {
        return Err(Refusal::NoRole);
    }

    let shown = shown(model, claim_id, on)?;

    let located_claim = model
        .claims()
        .iter()
        .find(|c| c.value.claim_id.to_string() == claim_id)
        .ok_or_else(|| Refusal::UnknownClaim(claim_id.to_owned()))?;
    let claim = &located_claim.value;

    if claim.verification == Verification::PrimarySourceSupported && claim.verified_on == Some(on) {
        return Err(Refusal::AlreadyVerified {
            claim: claim_id.to_owned(),
            on,
        });
    }

    if claim.consequential && role != Role::Owner {
        return Err(Refusal::NeedsOwner(claim_id.to_owned()));
    }

    let cited_source = cited_source(model, claim);

    let mut candidate = claim.clone();
    candidate.verification = Verification::PrimarySourceSupported;
    candidate.verified_on = Some(on);
    candidate.verified_by_role = match role {
        Role::Owner => Some(VerifiedByRole::Owner),
        Role::Lead => Some(VerifiedByRole::Lead),
        Role::Agent => None,
    };

    let mut problems = promotion_problems(&candidate, cited_source);
    problems.extend(unreported_source_problem(&candidate, cited_source));
    if let Some(crp) = consequential_role_problem(&candidate) {
        problems.push(crp);
    }
    if !problems.is_empty() {
        return Err(Refusal::Rule(problems));
    }

    let is_record_field = matches!(claim.locator_kind, Some(LocatorKind::RecordField));
    if !is_record_field && let (Some(sid), Some(ver)) = (&claim.source_id, claim.version) {
        let vid_str = format!("{sid}-v{ver}");
        if let Ok(vid) = VersionId::parse(&vid_str)
            && let Some(entry) = model.index().source_versions.get(&vid)
            && entry.lifecycle != Lifecycle::Active
        {
            return Err(Refusal::Inactive {
                claim: claim_id.to_owned(),
                version: vid.to_string(),
                lifecycle: entry.lifecycle.to_string(),
            });
        }
    }

    if let Some(as_of) = claim.as_of
        && as_of.first_day() > on
    {
        return Err(Refusal::AsOfAfter {
            claim: claim_id.to_owned(),
            as_of: as_of.to_string(),
            on,
        });
    }

    let bytes = after_bytes(model, claim_id, on, role)?;
    let evidence_sha = evidence_sha256(model, claim_id)?;

    Ok(VerifyPlan {
        shown,
        consequential: claim.consequential,
        on,
        role,
        path: "corpus/registers/claims.csv".to_owned(),
        bytes,
        evidence_sha256: evidence_sha,
    })
}

/// Compute the updated bytes of `claims.csv` in normal form with verification fields set.
///
/// # Errors
///
/// Returns [`Refusal::NoRole`] if `role` is [`Role::Agent`], [`Refusal::MissingRegister`] if
/// `claims.csv` did not load, [`Refusal::UnknownClaim`] if `claim_id` is not present, or
/// [`Refusal::Unplannable`] if rewriting or normal-form validation fails.
pub fn after_bytes(
    model: &Model,
    claim_id: &str,
    on: CivilDate,
    role: Role,
) -> Result<Vec<u8>, Refusal> {
    let role_word = match role {
        Role::Owner => "owner",
        Role::Lead => "lead",
        Role::Agent => return Err(Refusal::NoRole),
    };

    let Some(table_ref) = model.register(Register::Claims) else {
        return Err(Refusal::MissingRegister);
    };
    let mut table = table_ref.clone();

    let row = table
        .rows
        .iter_mut()
        .find(|r| r.cells.first().is_some_and(|id| id == claim_id))
        .ok_or_else(|| Refusal::UnknownClaim(claim_id.to_owned()))?;

    let verif_col = column(Register::Claims, "verification")
        .ok_or_else(|| Refusal::Unplannable("missing verification column".to_owned()))?;
    let verif_on_col = column(Register::Claims, "verified_on")
        .ok_or_else(|| Refusal::Unplannable("missing verified_on column".to_owned()))?;
    let verif_role_col = column(Register::Claims, "verified_by_role")
        .ok_or_else(|| Refusal::Unplannable("missing verified_by_role column".to_owned()))?;

    if row.cells.len() <= verif_col
        || row.cells.len() <= verif_on_col
        || row.cells.len() <= verif_role_col
    {
        return Err(Refusal::Unplannable(
            "claims.csv row has insufficient cells".to_owned(),
        ));
    }

    row.cells[verif_col] = STATUS.to_owned();
    row.cells[verif_on_col] = on.to_string();
    row.cells[verif_role_col] = role_word.to_owned();

    let csv_text = write_csv(&table);
    let bytes = normalize_register(Register::Claims, csv_text.as_bytes())
        .map(String::into_bytes)
        .map_err(|e| Refusal::Unplannable(e.to_string()))?;

    if !is_normal(Register::Claims, &bytes) {
        return Err(Refusal::Unplannable(
            "claims.csv cannot be rewritten in normal form".to_owned(),
        ));
    }

    Ok(bytes)
}

/// Compute the SHA-256 digest over the claim's invariant evidence and cited source cells.
///
/// Excludes the verification, verified_on, and verified_by_role columns, ensuring its value
/// is identical before and after verification.
///
/// # Errors
///
/// Returns [`Refusal::MissingRegister`] if `claims.csv` or `sources.csv` did not load,
/// [`Refusal::UnknownClaim`] if `claim_id` is not present, or [`Refusal::Unplannable`] if
/// digest computation fails.
pub fn evidence_sha256(model: &Model, claim_id: &str) -> Result<Sha256, Refusal> {
    let claims_table = model
        .register(Register::Claims)
        .ok_or(Refusal::MissingRegister)?;
    let sources_table = model
        .register(Register::Sources)
        .ok_or(Refusal::MissingRegister)?;

    let claim_row = claims_table
        .rows
        .iter()
        .find(|r| r.cells.first().is_some_and(|id| id == claim_id))
        .ok_or_else(|| Refusal::UnknownClaim(claim_id.to_owned()))?;

    let mut claim_names = Vec::with_capacity(32);
    for col in Register::Claims.columns() {
        if matches!(
            col.name,
            "verification" | "verified_on" | "verified_by_role"
        ) {
            continue;
        }
        claim_names.push(format!("claim:{}", col.name));
    }

    let mut entries: Vec<(&str, &[u8])> = Vec::with_capacity(64);
    let mut name_idx = 0;
    for (col_idx, col) in Register::Claims.columns().iter().enumerate() {
        if matches!(
            col.name,
            "verification" | "verified_on" | "verified_by_role"
        ) {
            continue;
        }
        let cell_text = claim_row.cells.get(col_idx).map_or("", String::as_str);
        entries.push((claim_names[name_idx].as_str(), cell_text.as_bytes()));
        name_idx += 1;
    }

    let sid_idx = column(Register::Claims, "source_id");
    let ver_idx = column(Register::Claims, "version");
    let claim_sid = sid_idx
        .and_then(|i| claim_row.cells.get(i))
        .map_or("", String::as_str);
    let claim_ver = ver_idx
        .and_then(|i| claim_row.cells.get(i))
        .map_or("", String::as_str);

    let src_ver_idx = column(Register::Sources, "version").unwrap_or(1);
    let source_row = if claim_sid.is_empty() || claim_ver.is_empty() {
        None
    } else {
        sources_table.rows.iter().find(|r| {
            r.cells.first().is_some_and(|sid| sid == claim_sid)
                && r.cells.get(src_ver_idx).is_some_and(|v| v == claim_ver)
        })
    };

    let mut source_names = Vec::new();
    if let Some(src_row) = source_row {
        source_names.reserve(Register::Sources.columns().len());
        for col in Register::Sources.columns() {
            source_names.push(format!("source:{}", col.name));
        }
        for (col_idx, _) in Register::Sources.columns().iter().enumerate() {
            let cell_text = src_row.cells.get(col_idx).map_or("", String::as_str);
            entries.push((source_names[col_idx].as_str(), cell_text.as_bytes()));
        }
    } else {
        entries.push(("source:absent", b""));
    }

    request_sha256(&entries).map_err(|e| Refusal::Unplannable(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::{Host, Options, Tree, load};

    const CORPUS_TOML: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/corpus.toml");
    const SOURCES: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/sources.csv");
    const CLAIMS: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/claims.csv");
    const QUESTIONS: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/questions.csv");
    const CONFLICTS: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/conflicts.csv");
    const GLOSSARY: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/glossary.csv");

    fn mini_model(claims_csv: &str) -> Model {
        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_dir("corpus/registers");
        tree.insert_bytes("corpus/corpus.toml", CORPUS_TOML.as_bytes());
        tree.insert_bytes("corpus/registers/sources.csv", SOURCES.as_bytes());
        tree.insert_bytes("corpus/registers/claims.csv", claims_csv.as_bytes());
        tree.insert_bytes("corpus/registers/questions.csv", QUESTIONS.as_bytes());
        tree.insert_bytes("corpus/registers/conflicts.csv", CONFLICTS.as_bytes());
        tree.insert_bytes("corpus/registers/glossary.csv", GLOSSARY.as_bytes());
        let today = CivilDate::from_ymd(2026, 3, 1).expect("valid date");
        load(&Host::new(tree, today), &Options::new(false)).0
    }

    #[test]
    fn terminal_and_role_messages_are_exact() {
        assert_eq!(
            terminal_message(),
            "verify needs a terminal; it is for the owner at home"
        );
        assert_eq!(
            role_message(),
            "verify needs a role: add the line role = \"owner\" (or `role = \"lead\"`) to settings.toml in the state folder"
        );
    }

    #[test]
    fn status_is_the_only_value_written() {
        assert_eq!(STATUS, "primary_source_supported");
    }

    #[test]
    fn agent_role_is_refused_before_any_lookup() {
        let today = CivilDate::from_ymd(2026, 1, 1).expect("valid date");
        let (model, _) = load(&Host::new(Tree::new(), today), &Options::new(false));
        let result = plan(&model, "CL-CMP-0001", today, Role::Agent);
        assert_eq!(result, Err(Refusal::NoRole));
    }

    #[test]
    fn shown_computable_for_verified_and_unverified_claims() {
        let model = mini_model(CLAIMS);
        let today = CivilDate::from_ymd(2026, 3, 1).expect("valid date");

        // Computable for an already verified claim
        let shown_verified = shown(&model, "CL-CMP-0001", today).expect("shown for verified claim");
        assert_eq!(shown_verified.claim_id, "CL-CMP-0001");
        assert_eq!(
            shown_verified.statement,
            "Synthetic compliance reviews occur every 30 days under the synthetic rule."
        );
        assert_eq!(shown_verified.version, "S-0001-v1");
        assert_eq!(
            shown_verified.source_title,
            "Synthetic Cost Rule, made up for tests"
        );
        assert_eq!(shown_verified.locator_kind, "text_section");
        assert_eq!(shown_verified.locator, "sec. 1");
        assert_eq!(
            shown_verified.quote,
            "Synthetic compliance reviews occur every 30 days."
        );
        assert_eq!(shown_verified.on, today);

        let summary = shown_verified.summary();
        assert!(summary.contains("claim      CL-CMP-0001\n"));
        assert!(summary.contains("source     S-0001-v1  Synthetic Cost Rule, made up for tests\n"));
        assert_eq!(
            shown_verified.question(),
            "Verified at this locator on 2026-03-01? Type yes"
        );
        let confirm_hash = shown_verified.confirmation_sha256();
        assert_eq!(confirm_hash.as_str().len(), 64);

        // Computable for an unchecked claim with source
        let shown_unchecked =
            shown(&model, "CL-CMP-0002", today).expect("shown for unchecked claim");
        assert_eq!(shown_unchecked.claim_id, "CL-CMP-0002");
        assert_eq!(shown_unchecked.version, "S-0001-v1");
        assert_eq!(
            shown_unchecked.source_title,
            "Synthetic Cost Rule, made up for tests"
        );

        // Computable for an assumption without cited source
        let shown_assumption =
            shown(&model, "CL-EST-0001", today).expect("shown for claim without source");
        assert_eq!(shown_assumption.claim_id, "CL-EST-0001");
        assert_eq!(shown_assumption.version, "");
        assert_eq!(shown_assumption.source_title, "");
        assert_eq!(shown_assumption.locator_kind, "");
        assert_eq!(shown_assumption.locator, "");
        assert_eq!(shown_assumption.quote, "");
        let summary_assumption = shown_assumption.summary();
        assert!(summary_assumption.contains("source     -  -\n"));
        assert!(summary_assumption.contains("locator    -  -\n"));
        assert!(summary_assumption.contains("quote      -\n"));
    }

    #[test]
    fn shown_and_evidence_sha256_refuse_missing_register_and_unknown_claim() {
        let today = CivilDate::from_ymd(2026, 3, 1).expect("valid date");
        let (empty_model, _) = load(&Host::new(Tree::new(), today), &Options::new(false));

        assert_eq!(
            shown(&empty_model, "CL-CMP-0001", today),
            Err(Refusal::MissingRegister)
        );
        assert_eq!(
            evidence_sha256(&empty_model, "CL-CMP-0001"),
            Err(Refusal::MissingRegister)
        );

        let model = mini_model(CLAIMS);
        assert_eq!(
            shown(&model, "CL-NOT-EXIST", today),
            Err(Refusal::UnknownClaim("CL-NOT-EXIST".to_owned()))
        );
        assert_eq!(
            evidence_sha256(&model, "CL-NOT-EXIST"),
            Err(Refusal::UnknownClaim("CL-NOT-EXIST".to_owned()))
        );
    }

    #[test]
    fn evidence_sha256_identical_before_and_after_verification() {
        let model = mini_model(CLAIMS);
        let today = CivilDate::from_ymd(2026, 3, 1).expect("valid date");

        // CL-CMP-0001 is already primary_source_supported in the fixture
        let sha_verified =
            evidence_sha256(&model, "CL-CMP-0001").expect("hash on already verified claim");
        assert_eq!(sha_verified.as_str().len(), 64);

        // CL-CMP-0002 is unchecked; plan its verification as owner
        let sha_before = evidence_sha256(&model, "CL-CMP-0002").expect("hash before verification");
        let plan =
            plan(&model, "CL-CMP-0002", today, Role::Owner).expect("plan succeeds for owner");

        assert_eq!(plan.evidence_sha256, sha_before);
        assert_eq!(plan.role_word(), "owner");
        assert_eq!(plan.shown, shown(&model, "CL-CMP-0002", today).unwrap());
        assert_eq!(plan.on, today);
        assert!(plan.consequential);

        // Simulate applying the plan's replacement bytes to claims.csv
        let updated_claims = String::from_utf8(plan.bytes.clone()).expect("valid utf-8");
        assert!(updated_claims.contains("CL-CMP-0002"));
        assert!(updated_claims.contains(STATUS));

        let model_after = mini_model(&updated_claims);
        let sha_after =
            evidence_sha256(&model_after, "CL-CMP-0002").expect("hash after verification");

        // The evidence hash MUST be identical before and after verification
        assert_eq!(sha_before, sha_after);
    }

    #[test]
    fn plan_refusals_lead_consequential_and_already_verified() {
        let model = mini_model(CLAIMS);
        let today = CivilDate::from_ymd(2026, 3, 1).expect("valid date");

        // Consequential claim cannot be verified by Lead
        let lead_result = plan(&model, "CL-CMP-0002", today, Role::Lead);
        assert_eq!(
            lead_result,
            Err(Refusal::NeedsOwner("CL-CMP-0002".to_owned()))
        );

        // CL-CMP-0001 was already verified on 2026-02-10 in fixture
        let verified_on = CivilDate::from_ymd(2026, 2, 10).expect("valid date");
        let already_res = plan(&model, "CL-CMP-0001", verified_on, Role::Owner);
        assert_eq!(
            already_res,
            Err(Refusal::AlreadyVerified {
                claim: "CL-CMP-0001".to_owned(),
                on: verified_on,
            })
        );
    }
}
