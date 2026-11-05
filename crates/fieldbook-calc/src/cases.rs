//! Synthetic calculation cases (`Case`, `ReplayCase`), evaluation (`evaluate`,
//! `evaluate_replay`), comparison (`compare`), and fixture rule validation.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::children::SumMismatch;
use crate::date::CivilDate;
use crate::decimal::Dec;
use crate::error::CalcError;
use crate::fns::{self, CALC_EPOCH};
use crate::rounding::{RoundingPolicy, Stage};
use crate::run::{Run, RunOutput};
use crate::value::Value;

/// Valid overhead base words in a [`Case`].
pub const OVERHEAD_BASES: &[&str] = &["direct_labour", "material", "landed", "total_cost", "none"];

/// Rounding configuration in a [`Case`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseRounding {
    /// Policy name (defaults to `"default"`).
    #[serde(default)]
    pub policy: Option<String>,
    /// Decimal scale (`0..=28`).
    pub scale: u32,
}

/// Inputs table `[inputs]` for `build_up@1` cases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseInputs {
    /// Quoted price as decimal text.
    pub quote_price: String,
    /// Unit basis of `quote_price` (e.g. `"per_100"`, `"each"`).
    #[serde(default)]
    pub quote_unit: Option<String>,
    /// Order quantity as decimal text.
    pub quantity: String,
    /// Unit of `quantity` (e.g. `"each"`).
    #[serde(default)]
    pub quantity_unit: Option<String>,
    /// One-time setup charge as decimal text.
    #[serde(default)]
    pub setup: Option<String>,
    /// Freight charge as decimal text.
    #[serde(default)]
    pub freight: Option<String>,
    /// Direct labour hours as decimal text.
    #[serde(default)]
    pub labour_hours: Option<String>,
    /// Direct labour hourly rate as decimal text.
    #[serde(default)]
    pub labour_rate: Option<String>,
    /// Overhead percentage as decimal text.
    #[serde(default)]
    pub overhead_percent: Option<String>,
    /// Markup percentage on cost as decimal text.
    #[serde(default)]
    pub markup_percent: Option<String>,
    /// Margin percentage on sales as decimal text.
    #[serde(default)]
    pub margin_percent: Option<String>,
}

/// Expected outputs table `[outputs]` for `build_up@1` cases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseOutputs {
    /// Unit price in the `quantity_unit` basis.
    #[serde(default)]
    pub unit: Option<String>,
    /// Extended material price.
    #[serde(default)]
    pub extension: Option<String>,
    /// Landed material cost (`extension + setup + freight`).
    #[serde(default)]
    pub landed: Option<String>,
    /// Direct labour cost.
    #[serde(default)]
    pub labour: Option<String>,
    /// Overhead cost.
    #[serde(default)]
    pub overhead: Option<String>,
    /// Total cost (`landed + labour + overhead`).
    #[serde(default)]
    pub cost: Option<String>,
    /// Profit amount.
    #[serde(default)]
    pub profit: Option<String>,
    /// Total selling price.
    #[serde(default)]
    pub price: Option<String>,
}

/// One `[[budget]]` reconciliation row in a `children_sum@1` case (`CALC-04`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetRow {
    /// Parent budget as decimal text.
    pub parent: String,
    /// Child budgets as decimal text.
    pub children: Vec<String>,
    /// Expected result (`"accepted"` or `"refused"`).
    pub expect: String,
    /// Expected children total when `expect == "refused"`.
    #[serde(default)]
    pub total: Option<String>,
    /// Expected absolute gap when `expect == "refused"`.
    #[serde(default)]
    pub gap: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCase {
    #[serde(default)]
    id: String,
    #[serde(rename = "fn", default)]
    func: String,
    #[serde(default)]
    synthetic: bool,
    #[serde(default)]
    as_of: String,
    #[serde(default)]
    currency: Option<String>,
    #[serde(default)]
    overhead_base: String,
    #[serde(default)]
    stages: Option<Vec<Stage>>,
    #[serde(default)]
    tolerance: Option<String>,
    #[serde(default)]
    refuse: Option<String>,
    #[serde(default)]
    rounding: Option<CaseRounding>,
    #[serde(default)]
    inputs: Option<CaseInputs>,
    #[serde(default)]
    outputs: Option<CaseOutputs>,
    #[serde(default)]
    budget: Vec<BudgetRow>,
}

/// One synthetic calculation case parsed from TOML (`tests/cases/*.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawCase")]
pub struct Case {
    /// Case identifier (`CALC-01`, `CALC-90`, etc.).
    pub id: String,
    /// Versioned calculation function (`build_up@1`, `children_sum@1`).
    #[serde(rename = "fn")]
    pub func: String,
    /// Whether the case is synthetic (`true` required by FM0).
    pub synthetic: bool,
    /// Pinned synthetic `as_of` date (`YYYY-MM-DD`).
    pub as_of: String,
    /// Currency code (defaults to `"USD"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// Overhead allocation base word or `"none"`.
    pub overhead_base: String,
    /// Named rounding stages called by the case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stages: Option<Vec<Stage>>,
    /// Optional tolerance (only permitted on iterative functions).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<String>,
    /// Expected refusal code (`FB05xx`) for negative cases.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refuse: Option<String>,
    /// Rounding policy table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rounding: Option<CaseRounding>,
    /// Inputs table for `build_up@1`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs: Option<CaseInputs>,
    /// Expected outputs table for `build_up@1`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outputs: Option<CaseOutputs>,
    /// Budget rows for `children_sum@1`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub budget: Vec<BudgetRow>,
}

impl TryFrom<RawCase> for Case {
    type Error = String;

    fn try_from(raw: RawCase) -> Result<Self, Self::Error> {
        Ok(Self {
            id: raw.id,
            func: raw.func,
            synthetic: raw.synthetic,
            as_of: raw.as_of,
            currency: raw.currency,
            overhead_base: raw.overhead_base,
            stages: raw.stages,
            tolerance: raw.tolerance,
            refuse: raw.refuse,
            rounding: raw.rounding,
            inputs: raw.inputs,
            outputs: raw.outputs,
            budget: raw.budget,
        })
    }
}

impl Case {
    /// Returns the currency string (defaulting to `"USD"`).
    #[must_use]
    pub fn currency_str(&self) -> &str {
        self.currency.as_deref().unwrap_or("USD")
    }

    /// Returns the rounding scale (defaulting to `RoundingPolicy::DEFAULT_SCALE`).
    #[must_use]
    pub fn rounding_scale(&self) -> u32 {
        self.rounding
            .as_ref()
            .map_or(RoundingPolicy::DEFAULT_SCALE, |r| r.scale)
    }

    /// Returns the rounding policy identifier (defaulting to `"default"`).
    #[must_use]
    pub fn rounding_policy_name(&self) -> &str {
        self.rounding
            .as_ref()
            .and_then(|r| r.policy.as_deref())
            .unwrap_or("default")
    }

    /// Returns the declared rounding stages slice.
    #[must_use]
    pub fn stages_slice(&self) -> &[Stage] {
        self.stages.as_deref().unwrap_or(&[])
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawReplayCase {
    replay_schema: u32,
    calculation_epoch: u32,
    case: Case,
}

/// Compatibility-only historical replay envelope (`compatibility.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawReplayCase")]
pub struct ReplayCase {
    /// Envelope schema version (must be `1`).
    pub replay_schema: u32,
    /// Pinned calculation epoch (must be `>= 1`).
    pub calculation_epoch: u32,
    /// Underlying strict [`Case`].
    pub case: Case,
}

impl TryFrom<RawReplayCase> for ReplayCase {
    type Error = String;

    fn try_from(raw: RawReplayCase) -> Result<Self, Self::Error> {
        if raw.replay_schema != 1 {
            return Err(format!(
                "unsupported replay_schema {} (expected 1)",
                raw.replay_schema
            ));
        }
        if raw.calculation_epoch == 0 {
            return Err("calculation_epoch must be a positive integer".to_string());
        }
        Ok(Self {
            replay_schema: raw.replay_schema,
            calculation_epoch: raw.calculation_epoch,
            case: raw.case,
        })
    }
}

/// Recorded result of one `[[budget]]` row during evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetOutcome {
    /// Result of [`crate::children::check_children_sum`].
    pub result: Result<(), SumMismatch>,
}

/// Result of evaluating a [`Case`] (`outputs` and `run`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// Computed outputs in declared order.
    pub outputs: Vec<RunOutput>,
    /// Complete [`Run`] record of the evaluation.
    pub run: Run,
    /// Per-row outcomes when the case has `[[budget]]` rows.
    pub budget_outcomes: Vec<BudgetOutcome>,
}

/// One output mismatch (`FB0503`) between a [`Case`] and its [`Outcome`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    /// Case identifier (`CALC-01`, etc.).
    pub case_id: String,
    /// Output or budget row name that differed.
    pub output: String,
    /// Expected decimal or status text.
    pub expected: String,
    /// Found decimal or status text.
    pub found: String,
}

impl Mismatch {
    /// Returns the catalogue code for a case mismatch (`"FB0503"`).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        "FB0503"
    }
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "FB0503: case {}: output `{}`: expected {}, found {}",
            self.case_id, self.output, self.expected, self.found
        )
    }
}

/// Checks whether `s` is a valid overhead base word or `"none"`.
#[must_use]
pub fn is_valid_overhead_base(s: &str) -> bool {
    OVERHEAD_BASES.contains(&s)
}

/// Checks whether `id` matches the calculation case id shape `^[A-Z]{2,4}-[0-9]{2,3}$`.
#[must_use]
pub fn is_valid_case_id(id: &str) -> bool {
    let Some((prefix, digits)) = id.split_once('-') else {
        return false;
    };
    if !(2..=4).contains(&prefix.len()) || !prefix.bytes().all(|b| b.is_ascii_uppercase()) {
        return false;
    }
    (2..=3).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit())
}

/// Validates the fixture rules of `case` (FM0, AD-14).
///
/// Returns the list of broken rule names (`"synthetic"`, `"as_of"`, `"fn"`,
/// `"overhead_base"`, `"stages"`, `"id"`).
#[must_use]
pub fn broken_fixture_rules(
    file_name: &str,
    case: &Case,
    enforce_stem_match: bool,
) -> Vec<&'static str> {
    let mut broken = Vec::new();

    if !case.synthetic {
        broken.push("synthetic");
    }
    if case.as_of.is_empty() || CivilDate::parse(&case.as_of).is_err() {
        broken.push("as_of");
    }
    if case.func.is_empty() || fns::lookup(&case.func).is_none() {
        broken.push("fn");
    }
    if !is_valid_overhead_base(&case.overhead_base) {
        broken.push("overhead_base");
    }
    let stages_ok = match &case.stages {
        Some(stages) => {
            if case.overhead_base == "none" {
                stages.is_empty()
            } else {
                !stages.is_empty()
            }
        }
        None => false,
    };
    if !stages_ok {
        broken.push("stages");
    }
    let stem = file_name.strip_suffix(".toml").unwrap_or(file_name);
    let stem_ok = !enforce_stem_match || stem.to_ascii_uppercase() == case.id;
    if !is_valid_case_id(&case.id) || !stem_ok {
        broken.push("id");
    }

    broken
}

fn evaluate_with_epoch(case: &Case, calculation_epoch: u32) -> Result<Outcome, CalcError> {
    if !case.synthetic {
        return Err(CalcError::Grammar);
    }
    if case.as_of.is_empty() {
        return Err(CalcError::Grammar);
    }
    let _ = CivilDate::parse(&case.as_of)?;
    if case.func.is_empty() {
        return Err(CalcError::Grammar);
    }
    let Some(fn_row) = fns::lookup(&case.func) else {
        return Err(CalcError::UnknownFn(case.func.clone()));
    };
    if let Some(tol) = &case.tolerance {
        if !fn_row.iterative {
            return Err(CalcError::Grammar);
        }
        let _ = Dec::parse(tol)?;
    }
    if !is_valid_overhead_base(&case.overhead_base) {
        return Err(CalcError::Grammar);
    }
    let Some(stages) = &case.stages else {
        return Err(CalcError::Grammar);
    };
    if (case.overhead_base == "none") != stages.is_empty() {
        return Err(CalcError::Grammar);
    }

    match fn_row.id {
        "build_up@1" => crate::versions::build_up_v1::evaluate_build_up(case, calculation_epoch),
        "children_sum@1" => {
            crate::versions::children_sum_v1::evaluate_children_sum(case, calculation_epoch)
        }
        other => Err(CalcError::UnknownFn(other.to_string())),
    }
}

/// Evaluates `case` under the compiled [`CALC_EPOCH`].
pub fn evaluate(case: &Case) -> Result<Outcome, CalcError> {
    evaluate_with_epoch(case, CALC_EPOCH)
}

/// Evaluates a compatibility [`ReplayCase`] using its pinned `calculation_epoch`
/// through the current version lookup.
pub fn evaluate_replay(replay: &ReplayCase) -> Result<Outcome, CalcError> {
    if replay.replay_schema != 1 || replay.calculation_epoch == 0 {
        return Err(CalcError::Grammar);
    }
    evaluate_with_epoch(&replay.case, replay.calculation_epoch)
}

fn compare_output_field(
    case_id: &str,
    name: &str,
    expected_opt: Option<&String>,
    outputs: &[RunOutput],
    mismatches: &mut Vec<Mismatch>,
) {
    let Some(expected_str) = expected_opt else {
        return;
    };
    let found_output = outputs.iter().find(|o| o.name == name);
    let Some(out) = found_output else {
        mismatches.push(Mismatch {
            case_id: case_id.to_string(),
            output: name.to_string(),
            expected: expected_str.clone(),
            found: "missing".to_string(),
        });
        return;
    };
    match (Dec::parse(expected_str), out.value) {
        (Ok(exp_dec), Value::Known(found_dec)) if exp_dec == found_dec => {}
        (_, Value::Known(found_dec)) => {
            mismatches.push(Mismatch {
                case_id: case_id.to_string(),
                output: name.to_string(),
                expected: expected_str.clone(),
                found: found_dec.to_string(),
            });
        }
        (_, Value::Unknown(reason)) => {
            mismatches.push(Mismatch {
                case_id: case_id.to_string(),
                output: name.to_string(),
                expected: expected_str.clone(),
                found: format!("unknown:{reason}"),
            });
        }
    }
}

/// Compares `outcome` with the expected outputs or `[[budget]]` expectations of `case`,
/// returning every output that differs (`FB0503`).
#[must_use]
pub fn compare(case: &Case, outcome: &Outcome) -> Vec<Mismatch> {
    let mut mismatches = Vec::new();

    if let Some(exp) = &case.outputs {
        compare_output_field(
            &case.id,
            "unit",
            exp.unit.as_ref(),
            &outcome.outputs,
            &mut mismatches,
        );
        compare_output_field(
            &case.id,
            "extension",
            exp.extension.as_ref(),
            &outcome.outputs,
            &mut mismatches,
        );
        compare_output_field(
            &case.id,
            "landed",
            exp.landed.as_ref(),
            &outcome.outputs,
            &mut mismatches,
        );
        compare_output_field(
            &case.id,
            "labour",
            exp.labour.as_ref(),
            &outcome.outputs,
            &mut mismatches,
        );
        compare_output_field(
            &case.id,
            "overhead",
            exp.overhead.as_ref(),
            &outcome.outputs,
            &mut mismatches,
        );
        compare_output_field(
            &case.id,
            "cost",
            exp.cost.as_ref(),
            &outcome.outputs,
            &mut mismatches,
        );
        compare_output_field(
            &case.id,
            "profit",
            exp.profit.as_ref(),
            &outcome.outputs,
            &mut mismatches,
        );
        compare_output_field(
            &case.id,
            "price",
            exp.price.as_ref(),
            &outcome.outputs,
            &mut mismatches,
        );
    }

    for (idx, row) in case.budget.iter().enumerate() {
        let row_label = format!("budget[{}]", idx + 1);
        let Some(budget_outcome) = outcome.budget_outcomes.get(idx) else {
            mismatches.push(Mismatch {
                case_id: case.id.clone(),
                output: row_label,
                expected: row.expect.clone(),
                found: "missing".to_string(),
            });
            continue;
        };

        match (row.expect.as_str(), &budget_outcome.result) {
            ("accepted", Ok(())) => {}
            ("accepted", Err(found_mm)) => {
                mismatches.push(Mismatch {
                    case_id: case.id.clone(),
                    output: row_label,
                    expected: "accepted".to_string(),
                    found: format!(
                        "refused (parent={}, total={}, gap={})",
                        found_mm.parent.amount, found_mm.total.amount, found_mm.gap.amount
                    ),
                });
            }
            ("refused", Ok(())) => {
                let exp_total = row.total.as_deref().unwrap_or("?");
                let exp_gap = row.gap.as_deref().unwrap_or("?");
                mismatches.push(Mismatch {
                    case_id: case.id.clone(),
                    output: row_label,
                    expected: format!(
                        "refused (parent={}, total={exp_total}, gap={exp_gap})",
                        row.parent
                    ),
                    found: "accepted".to_string(),
                });
            }
            ("refused", Err(found_mm)) => {
                let exp_parent = Dec::parse(&row.parent).ok();
                let exp_total = row.total.as_deref().and_then(|s| Dec::parse(s).ok());
                let exp_gap = row.gap.as_deref().and_then(|s| Dec::parse(s).ok());
                if exp_parent != Some(found_mm.parent.amount)
                    || exp_total != Some(found_mm.total.amount)
                    || exp_gap != Some(found_mm.gap.amount)
                {
                    mismatches.push(Mismatch {
                        case_id: case.id.clone(),
                        output: row_label,
                        expected: format!(
                            "refused (parent={}, total={}, gap={})",
                            row.parent,
                            row.total.as_deref().unwrap_or("?"),
                            row.gap.as_deref().unwrap_or("?")
                        ),
                        found: format!(
                            "refused (parent={}, total={}, gap={})",
                            found_mm.parent.amount, found_mm.total.amount, found_mm.gap.amount
                        ),
                    });
                }
            }
            (other, _) => {
                mismatches.push(Mismatch {
                    case_id: case.id.clone(),
                    output: row_label,
                    expected: "accepted or refused".to_string(),
                    found: other.to_string(),
                });
            }
        }
    }

    mismatches
}

/// Evaluates `case` and verifies that its outputs or expected `refuse` code match.
///
/// Any difference returns `Err(Vec<Mismatch>)` with code `FB0503`.
pub fn verify_case(case: &Case) -> Result<(), Vec<Mismatch>> {
    if let Some(expected_code) = &case.refuse {
        match evaluate(case) {
            Err(err) if err.code() == expected_code => Ok(()),
            Err(err) => Err(vec![Mismatch {
                case_id: case.id.clone(),
                output: "refuse".to_string(),
                expected: expected_code.clone(),
                found: err.code().to_string(),
            }]),
            Ok(_) => Err(vec![Mismatch {
                case_id: case.id.clone(),
                output: "refuse".to_string(),
                expected: expected_code.clone(),
                found: "ok".to_string(),
            }]),
        }
    } else {
        match evaluate(case) {
            Ok(outcome) => {
                let mismatches = compare(case, &outcome);
                if mismatches.is_empty() {
                    Ok(())
                } else {
                    Err(mismatches)
                }
            }
            Err(err) => Err(vec![Mismatch {
                case_id: case.id.clone(),
                output: "evaluate".to_string(),
                expected: "ok".to_string(),
                found: err.code().to_string(),
            }]),
        }
    }
}
