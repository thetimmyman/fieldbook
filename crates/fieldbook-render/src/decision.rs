//! Synthetic decision package workbook (`templates/decision-package.xlsx`).
//!
//! Renders a five-sheet preview workbook (`Read-me`, `Summary`, `Cost build`, `Assumptions`,
//! `Price change bridge`) using INDEX/MATCH formulas only, with every formula cell carrying a
//! cached value computed by `fieldbook-calc` (AD-13, XL-02, DT-01).
//!
//! The builder exposes two public functions, [`decision_workbook`] and [`save_workbook`], so that
//! `decide-port` can append an `AI proposal` sheet between them without editing any existing sheet.

use fieldbook_calc::{
    CalcError, Dec, Money, Reason, RoundingPolicy, Stage, Value, add, div_at_stage, mul,
    percent_of, round_money_at_stage, sub, sub_dec,
};
use fieldbook_core::edition::EditionInfo;
use fieldbook_core::strings;
use rust_xlsxwriter::{DocProperties, ExcelDateTime, Formula, Workbook, Worksheet, XlsxError};

/// Site-relative output path of the decision package workbook.
pub const OUTPUT_PATH: &str = "templates/decision-package.xlsx";

/// Preview flag: remains `true` until spike S2 passes in corporate Excel.
pub const PREVIEW: bool = true;

/// Preview notice placed on the `Read-me` and `Summary` sheets while [`PREVIEW`] is `true`.
pub const PREVIEW_LABEL: &str = "Preview. Not yet checked in the corporate Excel (spike S2).";

/// Sheet name for the read-me instructions.
pub const SHEET_README: &str = "Read-me";
/// Sheet name for the one-page summary.
pub const SHEET_SUMMARY: &str = "Summary";
/// Sheet name for the unit cost build-up.
pub const SHEET_COST_BUILD: &str = "Cost build";
/// Sheet name for the assumptions appendix.
pub const SHEET_ASSUMPTIONS: &str = "Assumptions";
/// Sheet name for the v1-to-v2 price change bridge (TP-06).
pub const SHEET_PRICE_BRIDGE: &str = "Price change bridge";

/// Assumption ID for direct material per unit.
pub const ASSUMPTION_DIRECT_MATERIAL: &str = "SYN-AS-001";
/// Assumption ID for material escalation percent.
pub const ASSUMPTION_ESCALATION_PCT: &str = "SYN-AS-002";
/// Assumption ID for direct labour hours per unit.
pub const ASSUMPTION_LABOUR_HOURS: &str = "SYN-AS-003";
/// Assumption ID for direct labour rate per hour.
pub const ASSUMPTION_LABOUR_RATE: &str = "SYN-AS-004";
/// Assumption ID for overhead percent of direct labour.
pub const ASSUMPTION_OVERHEAD_PCT: &str = "SYN-AS-005";
/// Assumption ID for G&A percent of total cost input.
pub const ASSUMPTION_GA_PCT: &str = "SYN-AS-006";
/// Assumption ID for profit percent of burdened cost.
pub const ASSUMPTION_PROFIT_PCT: &str = "SYN-AS-007";

/// Cost build line labels, in sheet order.
pub const LINE_MATERIAL: &str = "Material with escalation";
/// Direct labour line label.
pub const LINE_LABOUR: &str = "Labour";
/// Overhead line label.
pub const LINE_OVERHEAD: &str = "Overhead";
/// Total cost input subtotal label.
pub const LINE_TOTAL_COST_INPUT: &str = "Total cost input";
/// G&A line label.
pub const LINE_GA: &str = "G&A";
/// Burdened unit cost subtotal label.
pub const LINE_BURDENED_COST: &str = "Burdened unit cost";
/// Profit line label.
pub const LINE_PROFIT: &str = "Profit";
/// Unit price total label.
pub const LINE_UNIT_PRICE: &str = "Unit price";
/// Profit on cost ratio line label (percent of burdened unit cost).
pub const LINE_PROFIT_ON_COST: &str = "Profit on cost (%)";
/// Self-check row label on `Cost build` and `Price change bridge`.
pub const LINE_CHECK: &str = "Check";

/// First row label on `Price change bridge` (baseline v1 unit price).
pub const BRIDGE_V1_PRICE: &str = "Unit price, v1";
/// Total change row label on `Price change bridge`.
pub const BRIDGE_TOTAL_CHANGE: &str = "Total change";
/// Final price row label on `Price change bridge` (revised v2 unit price).
pub const BRIDGE_V2_PRICE: &str = "Unit price, v2";

/// Cost build line labels forming the five bridge steps, in sheet order.
pub const BRIDGE_STEP_LABELS: [&str; 5] = [
    LINE_MATERIAL,
    LINE_LABOUR,
    LINE_OVERHEAD,
    LINE_GA,
    LINE_PROFIT,
];

const NOT_ESTABLISHED: &str = "Not established";
const ZERO_DENOMINATOR_TEXT: &str = "n/a: zero denominator";

const ASSUMPTION_HEADERS: [&str; 11] = [
    "id",
    "text",
    "unit",
    "v1",
    "v2",
    "owner role",
    "claim id",
    "source version",
    "locator",
    "origin",
    "status",
];

const COST_BUILD_HEADERS: [&str; 6] = ["line", "v1", "v2", "change", "assumption id", "origin"];

const PRICE_BRIDGE_HEADERS: [&str; 4] =
    ["step", "change", "share of the price change (%)", "origin"];

/// Version column of a decision package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    /// Baseline version 1.
    V1,
    /// Revised version 2.
    V2,
}

/// One assumption row for the `Assumptions` sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assumption {
    /// Synthetic placeholder identifier (e.g. `SYN-AS-001`).
    pub id: String,
    /// Assumption description (marked synthetic).
    pub text: String,
    /// Unit word (e.g. `USD/unit`, `%`, `hours/unit`, `USD/hour`).
    pub unit: String,
    /// Version 1 decimal string under the AD-14 grammar.
    pub v1: String,
    /// Version 2 decimal string under the AD-14 grammar.
    pub v2: String,
    /// Owning role (never a personal name).
    pub owner_role: String,
    /// Trace claim id, or `"Not established"`.
    pub claim_id: String,
    /// Trace source version id, or `"Not established"`.
    pub source_version: String,
    /// Trace locator, or `"Not established"`.
    pub locator: String,
    /// Figure-origin word from [`fieldbook_core::strings::ORIGIN_LABELS`].
    pub origin: &'static str,
    /// Review status word (e.g. `draft`, `in_review`).
    pub status: String,
}

/// Complete input to [`cost_build`] and [`decision_workbook`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionInput {
    /// Synthetic package identifier.
    pub package_id: String,
    /// Package title (marked synthetic).
    pub title: String,
    /// Decision question (`BRIEF_PARTS[0]`).
    pub question: String,
    /// Recommendation (`BRIEF_PARTS[1]`).
    pub recommendation: String,
    /// Evidence summary (`BRIEF_PARTS[2]`).
    pub evidence: String,
    /// Options compared (`BRIEF_PARTS[3]`).
    pub options: Vec<String>,
    /// Open unknowns (`BRIEF_PARTS[4]`).
    pub unknowns: Vec<String>,
    /// Next action (`BRIEF_PARTS[5]`).
    pub next_action: String,
    /// Review state (seventh summary row).
    pub review_state: String,
    /// Assumptions driving the cost build.
    pub assumptions: Vec<Assumption>,
}

/// One calculated line of the `Cost build` sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CostLine {
    /// Display label used for INDEX/MATCH lookups from `Summary` and `Price change bridge`.
    pub label: &'static str,
    /// Computed exact decimal value, or `Value::Unknown(Reason::ZeroDenominator)`.
    pub value: Value<Dec>,
    /// Assumption id(s) that drive this line.
    pub assumption_id: &'static str,
    /// Figure-origin word from [`fieldbook_core::strings::ORIGIN_LABELS`].
    pub origin: &'static str,
}

/// One step (or total change) of the `Price change bridge` sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeStep {
    /// Display label (matching a `Cost build` line label for the five steps, or
    /// [`BRIDGE_TOTAL_CHANGE`]).
    pub label: &'static str,
    /// Exact dollar change (`v2 - v1`) from `fieldbook-calc`.
    pub change: Value<Dec>,
    /// Share of the total price change in percent, or `Value::Unknown(Reason::ZeroDenominator)`
    /// when the total price change is zero.
    pub share_pct: Value<Dec>,
    /// Figure-origin word from [`fieldbook_core::strings::ORIGIN_LABELS`].
    pub origin: &'static str,
}

/// Compiled-in synthetic example decision package (`SYN-DP-001`).
///
/// Every figure, identifier and description is synthetic. Overhead in v1 (`12.5%` of `33.33` =
/// `4.16625` -> `4.17`) and v2 (`13.5%` of `42.18` = `5.6943` -> `5.69`) is intentionally sub-cent
/// before rounding so skipped rounding is caught by recomputation tests.
#[must_use]
pub fn synthetic_example() -> DecisionInput {
    let quoted = strings::ORIGIN_LABELS[0];
    let judgment = strings::ORIGIN_LABELS[2];

    DecisionInput {
        package_id: "SYN-DP-001".to_owned(),
        title: "Synthetic follow-on lot pricing decision package".to_owned(),
        question: "Synthetic question: at what unit price should the synthetic follow-on lot be quoted?"
            .to_owned(),
        recommendation: "Synthetic recommendation: quote the v2 unit price once the synthetic overhead rate is confirmed."
            .to_owned(),
        evidence: "Synthetic evidence: five placeholder assumptions carry SYN trace ids; two remain Not established."
            .to_owned(),
        options: vec![
            "Option A (recommended): quote synthetic v2 unit price with escalation clause."
                .to_owned(),
            "Option B: hold synthetic v1 unit price and absorb the synthetic rate change."
                .to_owned(),
            format!(
                "Option C: lower base price with adjustment clause — unit price {NOT_ESTABLISHED}."
            ),
        ],
        unknowns: vec![
            format!("Synthetic overhead rate source ({NOT_ESTABLISHED})"),
            format!("Synthetic profit target approval ({NOT_ESTABLISHED})"),
        ],
        next_action: "Synthetic next action: Save As into working\\ and replace all SYN placeholders with approved company records."
            .to_owned(),
        review_state: "Synthetic draft — preview (not approved)".to_owned(),
        assumptions: vec![
            Assumption {
                id: ASSUMPTION_DIRECT_MATERIAL.to_owned(),
                text: "Synthetic direct material cost per unit".to_owned(),
                unit: "USD/unit".to_owned(),
                v1: "120.00".to_owned(),
                v2: "125.00".to_owned(),
                owner_role: "Pricing lead".to_owned(),
                claim_id: "SYN-CL-001".to_owned(),
                source_version: "SYN-S-001-v1".to_owned(),
                locator: "SYN-p.1".to_owned(),
                origin: quoted,
                status: "draft".to_owned(),
            },
            Assumption {
                id: ASSUMPTION_ESCALATION_PCT.to_owned(),
                text: "Synthetic material escalation percent".to_owned(),
                unit: "%".to_owned(),
                v1: "4.5".to_owned(),
                v2: "6.25".to_owned(),
                owner_role: "Pricing lead".to_owned(),
                claim_id: "SYN-CL-002".to_owned(),
                source_version: "SYN-S-002-v1".to_owned(),
                locator: "SYN-sec.2".to_owned(),
                origin: quoted,
                status: "draft".to_owned(),
            },
            Assumption {
                id: ASSUMPTION_LABOUR_HOURS.to_owned(),
                text: "Synthetic direct labour hours per unit".to_owned(),
                unit: "hours/unit".to_owned(),
                v1: "1.00".to_owned(),
                v2: "1.20".to_owned(),
                owner_role: "Estimating lead".to_owned(),
                claim_id: "SYN-CL-003".to_owned(),
                source_version: "SYN-S-003-v1".to_owned(),
                locator: "SYN-p.4".to_owned(),
                origin: quoted,
                status: "draft".to_owned(),
            },
            Assumption {
                id: ASSUMPTION_LABOUR_RATE.to_owned(),
                text: "Synthetic direct labour rate per hour".to_owned(),
                unit: "USD/hour".to_owned(),
                v1: "33.33".to_owned(),
                v2: "35.15".to_owned(),
                owner_role: "Cost accounting lead".to_owned(),
                claim_id: "SYN-CL-004".to_owned(),
                source_version: "SYN-S-004-v1".to_owned(),
                locator: "SYN-p.2".to_owned(),
                origin: quoted,
                status: "draft".to_owned(),
            },
            Assumption {
                id: ASSUMPTION_OVERHEAD_PCT.to_owned(),
                text: "Synthetic overhead percent of direct labour".to_owned(),
                unit: "%".to_owned(),
                v1: "12.5".to_owned(),
                v2: "13.5".to_owned(),
                owner_role: "Cost accounting lead".to_owned(),
                claim_id: NOT_ESTABLISHED.to_owned(),
                source_version: NOT_ESTABLISHED.to_owned(),
                locator: NOT_ESTABLISHED.to_owned(),
                origin: judgment,
                status: "in_review".to_owned(),
            },
            Assumption {
                id: ASSUMPTION_GA_PCT.to_owned(),
                text: "Synthetic G&A percent of total cost input".to_owned(),
                unit: "%".to_owned(),
                v1: "8.0".to_owned(),
                v2: "8.5".to_owned(),
                owner_role: "Controller".to_owned(),
                claim_id: "SYN-CL-006".to_owned(),
                source_version: "SYN-S-006-v1".to_owned(),
                locator: "SYN-sec.5".to_owned(),
                origin: quoted,
                status: "draft".to_owned(),
            },
            Assumption {
                id: ASSUMPTION_PROFIT_PCT.to_owned(),
                text: "Synthetic profit percent of burdened cost".to_owned(),
                unit: "%".to_owned(),
                v1: "10.0".to_owned(),
                v2: "10.5".to_owned(),
                owner_role: "Finance leadership".to_owned(),
                claim_id: NOT_ESTABLISHED.to_owned(),
                source_version: NOT_ESTABLISHED.to_owned(),
                locator: NOT_ESTABLISHED.to_owned(),
                origin: judgment,
                status: "in_review".to_owned(),
            },
        ],
    }
}

fn assumption_dec(input: &DecisionInput, id: &str, version: Version) -> Result<Dec, CalcError> {
    let Some(assumption) = input.assumptions.iter().find(|a| a.id == id) else {
        return Err(CalcError::Grammar);
    };
    let raw = match version {
        Version::V1 => &assumption.v1,
        Version::V2 => &assumption.v2,
    };
    Dec::parse(raw)
}

/// Evaluates the cost build for `version` using exact `fieldbook-calc` decimal arithmetic.
///
/// Each line extension is rounded to cents (scale 2, half away from zero) at [`Stage::Extension`],
/// and each subtotal/total sums the rounded lines at [`Stage::Total`]. Profit on cost uses
/// [`div_at_stage`] over the burdened unit cost and returns `Value::Unknown(Reason::ZeroDenominator)`
/// when burdened unit cost is zero.
///
/// # Errors
///
/// Returns [`CalcError`] if an assumption is missing, malformed, or overflows.
pub fn cost_build(input: &DecisionInput, version: Version) -> Result<Vec<CostLine>, CalcError> {
    let cents = RoundingPolicy::new(2)?;
    let computed = strings::ORIGIN_LABELS[1];

    let direct_mat = Money::usd(assumption_dec(input, ASSUMPTION_DIRECT_MATERIAL, version)?);
    let esc_pct = assumption_dec(input, ASSUMPTION_ESCALATION_PCT, version)?;
    let labour_hours = assumption_dec(input, ASSUMPTION_LABOUR_HOURS, version)?;
    let labour_rate = Money::usd(assumption_dec(input, ASSUMPTION_LABOUR_RATE, version)?);
    let overhead_pct = assumption_dec(input, ASSUMPTION_OVERHEAD_PCT, version)?;
    let ga_pct = assumption_dec(input, ASSUMPTION_GA_PCT, version)?;
    let profit_pct = assumption_dec(input, ASSUMPTION_PROFIT_PCT, version)?;

    let esc_amount = percent_of(Value::Known(direct_mat), Value::Known(esc_pct))?;
    let mat_raw = add(Value::Known(direct_mat), esc_amount)?;
    let mat_line = round_money_at_stage(mat_raw, Stage::Extension, &cents)?;

    let labour_raw = mul(Value::Known(labour_rate), Value::Known(labour_hours))?;
    let labour_line = round_money_at_stage(labour_raw, Stage::Extension, &cents)?;

    let overhead_raw = percent_of(labour_line, Value::Known(overhead_pct))?;
    let overhead_line = round_money_at_stage(overhead_raw, Stage::Extension, &cents)?;

    let tci_sum = add(add(mat_line, labour_line)?, overhead_line)?;
    let total_cost_input = round_money_at_stage(tci_sum, Stage::Total, &cents)?;

    let ga_raw = percent_of(total_cost_input, Value::Known(ga_pct))?;
    let ga_line = round_money_at_stage(ga_raw, Stage::Extension, &cents)?;

    let burdened_sum = add(total_cost_input, ga_line)?;
    let burdened_cost = round_money_at_stage(burdened_sum, Stage::Total, &cents)?;

    let profit_raw = percent_of(burdened_cost, Value::Known(profit_pct))?;
    let profit_line = round_money_at_stage(profit_raw, Stage::Extension, &cents)?;

    let price_sum = add(burdened_cost, profit_line)?;
    let unit_price = round_money_at_stage(price_sum, Stage::Total, &cents)?;

    let profit_times_100 = mul(profit_line, Value::Known(Dec::ONE_HUNDRED))?;
    let profit_on_cost = div_at_stage(
        profit_times_100,
        burdened_cost.map(|m| m.amount),
        Stage::Total,
        &cents,
    )?;

    Ok(vec![
        CostLine {
            label: LINE_MATERIAL,
            value: mat_line.map(|m| m.amount),
            assumption_id: "SYN-AS-001, SYN-AS-002",
            origin: computed,
        },
        CostLine {
            label: LINE_LABOUR,
            value: labour_line.map(|m| m.amount),
            assumption_id: "SYN-AS-003, SYN-AS-004",
            origin: computed,
        },
        CostLine {
            label: LINE_OVERHEAD,
            value: overhead_line.map(|m| m.amount),
            assumption_id: ASSUMPTION_OVERHEAD_PCT,
            origin: computed,
        },
        CostLine {
            label: LINE_TOTAL_COST_INPUT,
            value: total_cost_input.map(|m| m.amount),
            assumption_id: "SYN-AS-001, SYN-AS-002, SYN-AS-003, SYN-AS-004, SYN-AS-005",
            origin: computed,
        },
        CostLine {
            label: LINE_GA,
            value: ga_line.map(|m| m.amount),
            assumption_id: ASSUMPTION_GA_PCT,
            origin: computed,
        },
        CostLine {
            label: LINE_BURDENED_COST,
            value: burdened_cost.map(|m| m.amount),
            assumption_id: "SYN-AS-001, SYN-AS-002, SYN-AS-003, SYN-AS-004, SYN-AS-005, SYN-AS-006",
            origin: computed,
        },
        CostLine {
            label: LINE_PROFIT,
            value: profit_line.map(|m| m.amount),
            assumption_id: ASSUMPTION_PROFIT_PCT,
            origin: computed,
        },
        CostLine {
            label: LINE_UNIT_PRICE,
            value: unit_price.map(|m| m.amount),
            assumption_id: "SYN-AS-001, SYN-AS-002, SYN-AS-003, SYN-AS-004, SYN-AS-005, SYN-AS-006, SYN-AS-007",
            origin: computed,
        },
        CostLine {
            label: LINE_PROFIT_ON_COST,
            value: profit_on_cost.map(|m| m.amount),
            assumption_id: ASSUMPTION_PROFIT_PCT,
            origin: computed,
        },
    ])
}

fn find_cost_line<'a>(lines: &'a [CostLine], label: &str) -> Result<&'a CostLine, CalcError> {
    lines
        .iter()
        .find(|line| line.label == label)
        .ok_or(CalcError::Grammar)
}

/// Computes the five-step v1-to-v2 price change bridge and its total change using exact
/// `fieldbook-calc` arithmetic over the rounded [`CostLine`] values from [`cost_build`].
///
/// Returns six [`BridgeStep`] rows in fixed Cost build order: [`LINE_MATERIAL`],
/// [`LINE_LABOUR`], [`LINE_OVERHEAD`], [`LINE_GA`], [`LINE_PROFIT`], and
/// [`BRIDGE_TOTAL_CHANGE`]. Each share of the total change is rounded at [`Stage::Total`] to
/// two decimal places, or `Value::Unknown(Reason::ZeroDenominator)` when total change is zero.
///
/// # Errors
///
/// Returns [`CalcError`] if any required cost line is missing or an arithmetic operation fails.
pub fn bridge(v1: &[CostLine], v2: &[CostLine]) -> Result<Vec<BridgeStep>, CalcError> {
    let cents = RoundingPolicy::new(2)?;
    let computed = strings::ORIGIN_LABELS[1];

    let mut step_changes: Vec<(&'static str, Value<Dec>)> =
        Vec::with_capacity(BRIDGE_STEP_LABELS.len());
    let mut sum = Value::Known(Money::usd(Dec::ZERO));

    for &label in &BRIDGE_STEP_LABELS {
        let l1 = find_cost_line(v1, label)?;
        let l2 = find_cost_line(v2, label)?;
        let change = money_diff(l2.value, l1.value)?;
        sum = add(sum, change.map(Money::usd))?;
        step_changes.push((label, change));
    }

    let total_change_money = round_money_at_stage(sum, Stage::Total, &cents)?;
    let total_change = total_change_money.map(|m| m.amount);

    let mut steps = Vec::with_capacity(step_changes.len() + 1);
    for (label, change) in step_changes {
        let scaled = mul(change.map(Money::usd), Value::Known(Dec::ONE_HUNDRED))?;
        let share_pct = div_at_stage(scaled, total_change, Stage::Total, &cents)?.map(|m| m.amount);
        steps.push(BridgeStep {
            label,
            change,
            share_pct,
            origin: computed,
        });
    }

    let total_scaled = mul(total_change_money, Value::Known(Dec::ONE_HUNDRED))?;
    let total_share_pct =
        div_at_stage(total_scaled, total_change, Stage::Total, &cents)?.map(|m| m.amount);
    steps.push(BridgeStep {
        label: BRIDGE_TOTAL_CHANGE,
        change: total_change,
        share_pct: total_share_pct,
        origin: computed,
    });

    Ok(steps)
}

struct EvaluatedBuild {
    v1_lines: Vec<CostLine>,
    v2_lines: Vec<CostLine>,
    changes: Vec<Value<Dec>>,
    v1_check: &'static str,
    v2_check: &'static str,
    change_check: &'static str,
    bridge_steps: Vec<BridgeStep>,
    bridge_check: &'static str,
}

fn calc_err(err: &CalcError) -> XlsxError {
    XlsxError::ParameterError(err.to_string())
}

fn format_value(val: Value<Dec>) -> String {
    match val {
        Value::Known(d) => d.to_string(),
        Value::Unknown(Reason::ZeroDenominator) => ZERO_DENOMINATOR_TEXT.to_owned(),
        Value::Unknown(other) => format!("n/a: {other}"),
    }
}

fn diff_values(v2: Value<Dec>, v1: Value<Dec>) -> Result<Value<Dec>, CalcError> {
    match (v2, v1) {
        (Value::Unknown(r), _) | (Value::Known(_), Value::Unknown(r)) => Ok(Value::Unknown(r)),
        (Value::Known(a), Value::Known(b)) => sub_dec(Value::Known(a), Value::Known(b)),
    }
}

fn money_diff(v2: Value<Dec>, v1: Value<Dec>) -> Result<Value<Dec>, CalcError> {
    let m2 = v2.map(Money::usd);
    let m1 = v1.map(Money::usd);
    let d = sub(m2, m1)?;
    Ok(d.map(|m| m.amount))
}

fn check_reconciles(lines: &[CostLine]) -> Result<&'static str, CalcError> {
    let cents = RoundingPolicy::new(2)?;
    let mut sum = Value::Known(Money::usd(Dec::ZERO));
    for idx in [0usize, 1, 2, 4, 6] {
        let Some(line) = lines.get(idx) else {
            return Err(CalcError::Grammar);
        };
        sum = add(sum, line.value.map(Money::usd))?;
    }
    let rounded_sum = round_money_at_stage(sum, Stage::Total, &cents)?;
    let Some(price) = lines.get(7) else {
        return Err(CalcError::Grammar);
    };
    let rounded_price = round_money_at_stage(price.value.map(Money::usd), Stage::Total, &cents)?;
    if rounded_sum == rounded_price {
        Ok("OK")
    } else {
        Ok("MISMATCH")
    }
}

fn check_changes_reconcile(changes: &[Value<Dec>]) -> Result<&'static str, CalcError> {
    let cents = RoundingPolicy::new(2)?;
    let mut sum = Value::Known(Money::usd(Dec::ZERO));
    for idx in [0usize, 1, 2, 4, 6] {
        let Some(&val) = changes.get(idx) else {
            return Err(CalcError::Grammar);
        };
        sum = add(sum, val.map(Money::usd))?;
    }
    let rounded_sum = round_money_at_stage(sum, Stage::Total, &cents)?;
    let Some(&price_change) = changes.get(7) else {
        return Err(CalcError::Grammar);
    };
    let rounded_price = round_money_at_stage(price_change.map(Money::usd), Stage::Total, &cents)?;
    if rounded_sum == rounded_price {
        Ok("OK")
    } else {
        Ok("MISMATCH")
    }
}

fn check_bridge_reconciles(
    v1: &[CostLine],
    v2: &[CostLine],
    steps: &[BridgeStep],
) -> Result<&'static str, CalcError> {
    let cents = RoundingPolicy::new(2)?;
    let v1_price = find_cost_line(v1, LINE_UNIT_PRICE)?.value.map(Money::usd);
    let v2_price = find_cost_line(v2, LINE_UNIT_PRICE)?.value.map(Money::usd);
    let mut step_sum = Value::Known(Money::usd(Dec::ZERO));
    for step in steps {
        if step.label != BRIDGE_TOTAL_CHANGE {
            step_sum = add(step_sum, step.change.map(Money::usd))?;
        }
    }
    let rounded_steps = round_money_at_stage(step_sum, Stage::Total, &cents)?;
    let rounded_diff = round_money_at_stage(sub(v2_price, v1_price)?, Stage::Total, &cents)?;
    let rounded_v1_plus_steps =
        round_money_at_stage(add(v1_price, rounded_steps)?, Stage::Total, &cents)?;
    let rounded_v2 = round_money_at_stage(v2_price, Stage::Total, &cents)?;
    if rounded_steps == rounded_diff && rounded_v1_plus_steps == rounded_v2 {
        Ok("OK")
    } else {
        Ok("MISMATCH")
    }
}

fn lookup_assumption(col: char, id: &str, last_row: usize) -> String {
    format!(
        "INDEX({SHEET_ASSUMPTIONS}!${col}$2:${col}${last_row},MATCH(\"{id}\",{SHEET_ASSUMPTIONS}!$A$2:$A${last_row},0))"
    )
}

fn lookup_cost_build(col: char, label: &str, last_row: usize) -> String {
    format!(
        "=INDEX('{SHEET_COST_BUILD}'!${col}$2:${col}${last_row},MATCH(\"{label}\",'{SHEET_COST_BUILD}'!$A$2:$A${last_row},0))"
    )
}

fn write_formula_cached(
    sheet: &mut Worksheet,
    row: u32,
    col: u16,
    formula: &str,
    cached: &str,
) -> Result<(), XlsxError> {
    sheet.write_formula(row, col, Formula::new(formula).set_result(cached))?;
    Ok(())
}

fn write_readme_sheet(
    workbook: &mut Workbook,
    input: &DecisionInput,
    edition: &EditionInfo,
) -> Result<(), XlsxError> {
    let sheet = workbook.add_worksheet();
    sheet.set_name(SHEET_README)?;
    sheet.set_column_width(0, 22.0)?;
    sheet.set_column_width(1, 108.0)?;

    let as_of = if edition.built.is_empty() {
        strings::unreleased_build()
    } else {
        edition.built.as_str()
    };
    let status_text = if PREVIEW {
        PREVIEW_LABEL
    } else {
        "Checked in corporate Excel (spike S2 passed)."
    };

    let rows: [(&str, String); 10] = [
        (
            "Decision package",
            format!("{} — {}", input.package_id, input.title),
        ),
        ("Preview status", status_text.to_owned()),
        ("Edition", edition.edition.clone()),
        ("Tool version", edition.tool_version.clone()),
        ("As-of date", as_of.to_owned()),
        (
            "Synthetic example",
            "This workbook is a synthetic example with made-up figures and SYN-... placeholders; replace them with company records in your working copy."
                .to_owned(),
        ),
        (
            "Save prompt",
            "Excel asks to save after opening because the file recalculates on load; choose Save As into working\\ and keep the edition copy unchanged."
                .to_owned(),
        ),
        (
            "Versioning",
            "An approved version is never overwritten, only superseded (O-05). Start the next revision as a new version or file."
                .to_owned(),
        ),
        ("Rounding", strings::rounding().to_owned()),
        (
            "Sheets",
            format!(
                "{SHEET_README}, {SHEET_SUMMARY}, {SHEET_COST_BUILD}, {SHEET_ASSUMPTIONS}, {SHEET_PRICE_BRIDGE}"
            ),
        ),
    ];

    for (r_idx, (label, value)) in rows.iter().enumerate() {
        let row = u32::try_from(r_idx).map_err(|_| XlsxError::RowColumnLimitError)?;
        sheet.write_string(row, 0, *label)?;
        sheet.write_string(row, 1, value.as_str())?;
    }

    Ok(())
}

fn write_summary_sheet(
    workbook: &mut Workbook,
    input: &DecisionInput,
    eval: &EvaluatedBuild,
) -> Result<(), XlsxError> {
    let sheet = workbook.add_worksheet();
    sheet.set_name(SHEET_SUMMARY)?;
    sheet.set_formula_result_default("");
    sheet.set_column_width(0, 42.0)?;
    sheet.set_column_width(1, 92.0)?;
    sheet.set_column_width(2, 16.0)?;

    let computed = strings::ORIGIN_LABELS[1];

    sheet.write_string(0, 0, "Package")?;
    sheet.write_string(
        0,
        1,
        format!("{} — {} (synthetic example)", input.package_id, input.title),
    )?;
    if PREVIEW {
        sheet.write_string(1, 0, "Preview")?;
        sheet.write_string(1, 1, PREVIEW_LABEL)?;
    }

    let summary_rows: [(&str, String); 7] = [
        (strings::BRIEF_PARTS[0], input.question.clone()),
        (strings::BRIEF_PARTS[1], input.recommendation.clone()),
        (strings::BRIEF_PARTS[2], input.evidence.clone()),
        (strings::BRIEF_PARTS[3], input.options.join(" | ")),
        (strings::BRIEF_PARTS[4], input.unknowns.join("; ")),
        (strings::BRIEF_PARTS[5], input.next_action.clone()),
        ("Review state", input.review_state.clone()),
    ];

    for (idx, (heading, body)) in summary_rows.iter().enumerate() {
        let row = u32::try_from(idx + 2).map_err(|_| XlsxError::RowColumnLimitError)?;
        sheet.write_string(row, 0, *heading)?;
        sheet.write_string(row, 1, body.as_str())?;
    }

    // Row 10 (index 9) left blank as a visual separator before Key figures.
    sheet.write_string(10, 0, "Key figure (with basis)")?;
    sheet.write_string(10, 1, "Value")?;
    sheet.write_string(10, 2, "Origin")?;

    let cost_last_data_row = eval.v1_lines.len() + 1; // 10 for 9 cost lines
    let cost_last_row = cost_last_data_row + 1; // 11 including Check row

    let v1_price = format_value(eval.v1_lines[7].value);
    let v2_price = format_value(eval.v2_lines[7].value);
    let delta_price = format_value(eval.changes[7]);
    let v2_burdened = format_value(eval.v2_lines[5].value);
    let v2_poc = format_value(eval.v2_lines[8].value);

    let key_figures: [(&str, String, String, &str); 6] = [
        (
            "Unit price, v1 (USD per unit)",
            lookup_cost_build('B', LINE_UNIT_PRICE, cost_last_data_row),
            v1_price,
            computed,
        ),
        (
            "Unit price, v2 (USD per unit)",
            lookup_cost_build('C', LINE_UNIT_PRICE, cost_last_data_row),
            v2_price,
            computed,
        ),
        (
            "Price change, v2 minus v1 (USD per unit)",
            lookup_cost_build('D', LINE_UNIT_PRICE, cost_last_data_row),
            delta_price,
            computed,
        ),
        (
            "Burdened unit cost, v2 (USD per unit)",
            lookup_cost_build('C', LINE_BURDENED_COST, cost_last_data_row),
            v2_burdened,
            computed,
        ),
        (
            "Profit on cost, v2 (% of burdened unit cost)",
            lookup_cost_build('C', LINE_PROFIT_ON_COST, cost_last_data_row),
            v2_poc,
            computed,
        ),
        (
            "Self-check (Cost build reconciliation)",
            lookup_cost_build('B', LINE_CHECK, cost_last_row),
            eval.v1_check.to_owned(),
            computed,
        ),
    ];

    for (idx, (label, formula, cached, origin)) in key_figures.iter().enumerate() {
        let row = u32::try_from(idx + 11).map_err(|_| XlsxError::RowColumnLimitError)?;
        sheet.write_string(row, 0, *label)?;
        write_formula_cached(sheet, row, 1, formula, cached)?;
        sheet.write_string(row, 2, *origin)?;
    }

    Ok(())
}

fn write_cost_build_sheet(
    workbook: &mut Workbook,
    input: &DecisionInput,
    eval: &EvaluatedBuild,
) -> Result<(), XlsxError> {
    let sheet = workbook.add_worksheet();
    sheet.set_name(SHEET_COST_BUILD)?;
    sheet.set_formula_result_default("");

    for (c_idx, &header) in COST_BUILD_HEADERS.iter().enumerate() {
        let col = u16::try_from(c_idx).map_err(|_| XlsxError::RowColumnLimitError)?;
        sheet.write_string(0, col, header)?;
    }
    sheet.set_column_width(0, 28.0)?;
    sheet.set_column_width(1, 24.0)?;
    sheet.set_column_width(2, 24.0)?;
    sheet.set_column_width(3, 24.0)?;
    sheet.set_column_width(4, 52.0)?;
    sheet.set_column_width(5, 16.0)?;

    let a_last = input.assumptions.len() + 1;

    let v1_formulas = [
        format!(
            "=ROUND({}*(1+{}/100),2)",
            lookup_assumption('D', ASSUMPTION_DIRECT_MATERIAL, a_last),
            lookup_assumption('D', ASSUMPTION_ESCALATION_PCT, a_last)
        ),
        format!(
            "=ROUND({}*{},2)",
            lookup_assumption('D', ASSUMPTION_LABOUR_HOURS, a_last),
            lookup_assumption('D', ASSUMPTION_LABOUR_RATE, a_last)
        ),
        format!(
            "=ROUND(B3*{}/100,2)",
            lookup_assumption('D', ASSUMPTION_OVERHEAD_PCT, a_last)
        ),
        "=ROUND(SUM(B2:B4),2)".to_owned(),
        format!(
            "=ROUND(B5*{}/100,2)",
            lookup_assumption('D', ASSUMPTION_GA_PCT, a_last)
        ),
        "=ROUND(B5+B6,2)".to_owned(),
        format!(
            "=ROUND(B7*{}/100,2)",
            lookup_assumption('D', ASSUMPTION_PROFIT_PCT, a_last)
        ),
        "=ROUND(B7+B8,2)".to_owned(),
        format!("=IF(B7=0,\"{ZERO_DENOMINATOR_TEXT}\",ROUND(B8*100/B7,2))"),
    ];

    let v2_formulas = [
        format!(
            "=ROUND({}*(1+{}/100),2)",
            lookup_assumption('E', ASSUMPTION_DIRECT_MATERIAL, a_last),
            lookup_assumption('E', ASSUMPTION_ESCALATION_PCT, a_last)
        ),
        format!(
            "=ROUND({}*{},2)",
            lookup_assumption('E', ASSUMPTION_LABOUR_HOURS, a_last),
            lookup_assumption('E', ASSUMPTION_LABOUR_RATE, a_last)
        ),
        format!(
            "=ROUND(C3*{}/100,2)",
            lookup_assumption('E', ASSUMPTION_OVERHEAD_PCT, a_last)
        ),
        "=ROUND(SUM(C2:C4),2)".to_owned(),
        format!(
            "=ROUND(C5*{}/100,2)",
            lookup_assumption('E', ASSUMPTION_GA_PCT, a_last)
        ),
        "=ROUND(C5+C6,2)".to_owned(),
        format!(
            "=ROUND(C7*{}/100,2)",
            lookup_assumption('E', ASSUMPTION_PROFIT_PCT, a_last)
        ),
        "=ROUND(C7+C8,2)".to_owned(),
        format!("=IF(C7=0,\"{ZERO_DENOMINATOR_TEXT}\",ROUND(C8*100/C7,2))"),
    ];

    for idx in 0..eval.v1_lines.len() {
        let row = u32::try_from(idx + 1).map_err(|_| XlsxError::RowColumnLimitError)?;
        let excel_row = idx + 2;
        let line = &eval.v1_lines[idx];
        let v1_cached = format_value(line.value);
        let v2_cached = format_value(eval.v2_lines[idx].value);
        let diff_cached = format_value(eval.changes[idx]);
        let diff_formula = if idx == 8 {
            format!(
                "=IF(B7=0,\"{ZERO_DENOMINATOR_TEXT}\",IF(C7=0,\"{ZERO_DENOMINATOR_TEXT}\",ROUND(C{excel_row}-B{excel_row},2)))"
            )
        } else {
            format!("=ROUND(C{excel_row}-B{excel_row},2)")
        };

        sheet.write_string(row, 0, line.label)?;
        write_formula_cached(sheet, row, 1, &v1_formulas[idx], &v1_cached)?;
        write_formula_cached(sheet, row, 2, &v2_formulas[idx], &v2_cached)?;
        write_formula_cached(sheet, row, 3, &diff_formula, &diff_cached)?;
        sheet.write_string(row, 4, line.assumption_id)?;
        sheet.write_string(row, 5, line.origin)?;
    }

    let check_row =
        u32::try_from(eval.v1_lines.len() + 1).map_err(|_| XlsxError::RowColumnLimitError)?;
    let computed = strings::ORIGIN_LABELS[1];
    sheet.write_string(check_row, 0, LINE_CHECK)?;
    write_formula_cached(
        sheet,
        check_row,
        1,
        "=IF(ROUND(SUM(B2:B4,B6,B8),2)=ROUND(B9,2),\"OK\",\"MISMATCH\")",
        eval.v1_check,
    )?;
    write_formula_cached(
        sheet,
        check_row,
        2,
        "=IF(ROUND(SUM(C2:C4,C6,C8),2)=ROUND(C9,2),\"OK\",\"MISMATCH\")",
        eval.v2_check,
    )?;
    write_formula_cached(
        sheet,
        check_row,
        3,
        "=IF(ROUND(SUM(D2:D4,D6,D8),2)=ROUND(D9,2),\"OK\",\"MISMATCH\")",
        eval.change_check,
    )?;
    sheet.write_string(check_row, 5, computed)?;
    sheet.set_freeze_panes(1, 0)?;

    Ok(())
}

fn parse_input_number(raw: &str) -> Result<f64, XlsxError> {
    let _ = Dec::parse(raw).map_err(|e| calc_err(&e))?;
    raw.parse::<f64>()
        .map_err(|e| XlsxError::ParameterError(format!("invalid decimal `{raw}`: {e}")))
}

fn write_assumptions_sheet(
    workbook: &mut Workbook,
    input: &DecisionInput,
) -> Result<(), XlsxError> {
    let sheet = workbook.add_worksheet();
    sheet.set_name(SHEET_ASSUMPTIONS)?;

    for (c_idx, &header) in ASSUMPTION_HEADERS.iter().enumerate() {
        let col = u16::try_from(c_idx).map_err(|_| XlsxError::RowColumnLimitError)?;
        sheet.write_string(0, col, header)?;
    }

    for (r_idx, assumption) in input.assumptions.iter().enumerate() {
        let row = u32::try_from(r_idx + 1).map_err(|_| XlsxError::RowColumnLimitError)?;
        let v1_num = parse_input_number(&assumption.v1)?;
        let v2_num = parse_input_number(&assumption.v2)?;

        sheet.write_string(row, 0, assumption.id.as_str())?;
        sheet.write_string(row, 1, assumption.text.as_str())?;
        sheet.write_string(row, 2, assumption.unit.as_str())?;
        sheet.write_number(row, 3, v1_num)?;
        sheet.write_number(row, 4, v2_num)?;
        sheet.write_string(row, 5, assumption.owner_role.as_str())?;
        sheet.write_string(row, 6, assumption.claim_id.as_str())?;
        sheet.write_string(row, 7, assumption.source_version.as_str())?;
        sheet.write_string(row, 8, assumption.locator.as_str())?;
        sheet.write_string(row, 9, assumption.origin)?;
        sheet.write_string(row, 10, assumption.status.as_str())?;
    }

    sheet.set_column_width(0, 16.0)?;
    sheet.set_column_width(1, 46.0)?;
    sheet.set_column_width(2, 14.0)?;
    sheet.set_column_width(3, 14.0)?;
    sheet.set_column_width(4, 14.0)?;
    sheet.set_column_width(5, 24.0)?;
    sheet.set_column_width(6, 18.0)?;
    sheet.set_column_width(7, 18.0)?;
    sheet.set_column_width(8, 18.0)?;
    sheet.set_column_width(9, 14.0)?;
    sheet.set_column_width(10, 14.0)?;

    sheet.set_freeze_panes(1, 0)?;

    let last_row = u32::try_from(input.assumptions.len().max(1))
        .map_err(|_| XlsxError::RowColumnLimitError)?;
    let last_col = u16::try_from(ASSUMPTION_HEADERS.len().saturating_sub(1))
        .map_err(|_| XlsxError::RowColumnLimitError)?;
    sheet.autofilter(0, 0, last_row, last_col)?;

    Ok(())
}

fn write_price_bridge_sheet(
    workbook: &mut Workbook,
    eval: &EvaluatedBuild,
) -> Result<(), XlsxError> {
    let sheet = workbook.add_worksheet();
    sheet.set_name(SHEET_PRICE_BRIDGE)?;
    sheet.set_formula_result_default("");

    for (c_idx, &header) in PRICE_BRIDGE_HEADERS.iter().enumerate() {
        let col = u16::try_from(c_idx).map_err(|_| XlsxError::RowColumnLimitError)?;
        sheet.write_string(0, col, header)?;
    }
    sheet.set_column_width(0, 28.0)?;
    sheet.set_column_width(1, 24.0)?;
    sheet.set_column_width(2, 30.0)?;
    sheet.set_column_width(3, 16.0)?;

    let computed = strings::ORIGIN_LABELS[1];
    let cost_last_data_row = eval.v1_lines.len() + 1;
    let v1_price = find_cost_line(&eval.v1_lines, LINE_UNIT_PRICE)
        .map_err(|e| calc_err(&e))?
        .value;
    let v2_price = find_cost_line(&eval.v2_lines, LINE_UNIT_PRICE)
        .map_err(|e| calc_err(&e))?
        .value;

    let step_rows: Vec<&BridgeStep> = eval
        .bridge_steps
        .iter()
        .filter(|s| s.label != BRIDGE_TOTAL_CHANGE)
        .collect();
    if step_rows.is_empty() {
        return Err(calc_err(&CalcError::Grammar));
    }
    let total_step = eval
        .bridge_steps
        .iter()
        .find(|s| s.label == BRIDGE_TOTAL_CHANGE)
        .ok_or_else(|| calc_err(&CalcError::Grammar))?;

    let first_step_excel_row = 3usize;
    let last_step_excel_row = first_step_excel_row + step_rows.len() - 1;
    let total_excel_row = last_step_excel_row + 1;
    let v2_excel_row = total_excel_row + 1;

    // Row 2 (0-based 1): v1 unit price via INDEX/MATCH.
    sheet.write_string(1, 0, BRIDGE_V1_PRICE)?;
    write_formula_cached(
        sheet,
        1,
        1,
        &lookup_cost_build('B', LINE_UNIT_PRICE, cost_last_data_row),
        &format_value(v1_price),
    )?;
    sheet.write_string(1, 3, computed)?;

    // Rows 3..=7 (0-based 2..=6): the five bridge steps.
    for (idx, step) in step_rows.iter().enumerate() {
        let row = u32::try_from(idx + 2).map_err(|_| XlsxError::RowColumnLimitError)?;
        let excel_row = idx + first_step_excel_row;
        let share_formula = format!(
            "=IF($B${total_excel_row}=0,\"{ZERO_DENOMINATOR_TEXT}\",ROUND(B{excel_row}*100/$B${total_excel_row},2))"
        );
        sheet.write_string(row, 0, step.label)?;
        write_formula_cached(
            sheet,
            row,
            1,
            &lookup_cost_build('D', step.label, cost_last_data_row),
            &format_value(step.change),
        )?;
        write_formula_cached(sheet, row, 2, &share_formula, &format_value(step.share_pct))?;
        sheet.write_string(row, 3, step.origin)?;
    }

    // Row 8 (0-based 7): Total change.
    let total_row =
        u32::try_from(total_excel_row - 1).map_err(|_| XlsxError::RowColumnLimitError)?;
    let total_formula = format!("=ROUND(SUM(B{first_step_excel_row}:B{last_step_excel_row}),2)");
    let total_share_formula = format!(
        "=IF($B${total_excel_row}=0,\"{ZERO_DENOMINATOR_TEXT}\",ROUND(B{total_excel_row}*100/$B${total_excel_row},2))"
    );
    sheet.write_string(total_row, 0, total_step.label)?;
    write_formula_cached(
        sheet,
        total_row,
        1,
        &total_formula,
        &format_value(total_step.change),
    )?;
    write_formula_cached(
        sheet,
        total_row,
        2,
        &total_share_formula,
        &format_value(total_step.share_pct),
    )?;
    sheet.write_string(total_row, 3, total_step.origin)?;

    // Row 9 (0-based 8): v2 unit price via INDEX/MATCH.
    let v2_row = u32::try_from(v2_excel_row - 1).map_err(|_| XlsxError::RowColumnLimitError)?;
    sheet.write_string(v2_row, 0, BRIDGE_V2_PRICE)?;
    write_formula_cached(
        sheet,
        v2_row,
        1,
        &lookup_cost_build('C', LINE_UNIT_PRICE, cost_last_data_row),
        &format_value(v2_price),
    )?;
    sheet.write_string(v2_row, 3, computed)?;

    // Row 10 (0-based 9): Check row (v1 price + steps = v2 price at cents).
    let check_row = v2_row + 1;
    let check_formula = format!(
        "=IF(ROUND(B2+SUM(B{first_step_excel_row}:B{last_step_excel_row}),2)=ROUND(B{v2_excel_row},2),\"OK\",\"MISMATCH\")"
    );
    sheet.write_string(check_row, 0, LINE_CHECK)?;
    write_formula_cached(sheet, check_row, 1, &check_formula, eval.bridge_check)?;
    sheet.write_string(check_row, 3, computed)?;

    sheet.set_freeze_panes(1, 0)?;

    Ok(())
}

/// Builds the five-sheet decision package [`Workbook`] (`Read-me`, `Summary`, `Cost build`,
/// `Assumptions`, `Price change bridge`) with a fixed creation timestamp and exact cached formula
/// values.
///
/// # Errors
///
/// Returns [`XlsxError`] if any assumption fails `fieldbook-calc` validation/evaluation or if a
/// worksheet write fails.
pub fn decision_workbook(
    input: &DecisionInput,
    edition: &EditionInfo,
    workbook_epoch: i64,
) -> Result<Workbook, XlsxError> {
    let v1_lines = cost_build(input, Version::V1).map_err(|e| calc_err(&e))?;
    let v2_lines = cost_build(input, Version::V2).map_err(|e| calc_err(&e))?;

    let mut changes = Vec::with_capacity(v1_lines.len());
    for (idx, (l1, l2)) in v1_lines.iter().zip(v2_lines.iter()).enumerate() {
        let diff = if idx == 8 {
            diff_values(l2.value, l1.value).map_err(|e| calc_err(&e))?
        } else {
            money_diff(l2.value, l1.value).map_err(|e| calc_err(&e))?
        };
        changes.push(diff);
    }

    let v1_check = check_reconciles(&v1_lines).map_err(|e| calc_err(&e))?;
    let v2_check = check_reconciles(&v2_lines).map_err(|e| calc_err(&e))?;
    let change_check = check_changes_reconcile(&changes).map_err(|e| calc_err(&e))?;
    let bridge_steps = bridge(&v1_lines, &v2_lines).map_err(|e| calc_err(&e))?;
    let bridge_check =
        check_bridge_reconciles(&v1_lines, &v2_lines, &bridge_steps).map_err(|e| calc_err(&e))?;

    let evaluated = EvaluatedBuild {
        v1_lines,
        v2_lines,
        changes,
        v1_check,
        v2_check,
        change_check,
        bridge_steps,
        bridge_check,
    };

    let datetime = ExcelDateTime::from_timestamp(workbook_epoch).unwrap_or_else(|_| {
        ExcelDateTime::from_timestamp(crate::meta::DEFAULT_WORKBOOK_EPOCH)
            .unwrap_or_else(|e| unreachable!("DEFAULT_WORKBOOK_EPOCH is a valid timestamp: {e}"))
    });
    let properties = DocProperties::new().set_creation_datetime(&datetime);
    let mut workbook = Workbook::new();
    workbook.set_properties(&properties);

    write_readme_sheet(&mut workbook, input, edition)?;
    write_summary_sheet(&mut workbook, input, &evaluated)?;
    write_cost_build_sheet(&mut workbook, input, &evaluated)?;
    write_assumptions_sheet(&mut workbook, input)?;
    write_price_bridge_sheet(&mut workbook, &evaluated)?;

    Ok(workbook)
}

/// Serializes `workbook` to deterministic `.xlsx` bytes in memory.
///
/// # Errors
///
/// Returns [`XlsxError`] if workbook serialization fails.
pub fn save_workbook(workbook: &mut Workbook) -> Result<Vec<u8>, XlsxError> {
    workbook.save_to_buffer()
}

/// Renders the synthetic example decision package workbook (`templates/decision-package.xlsx`).
///
/// Calls [`synthetic_example`], [`decision_workbook`], and [`save_workbook`] in that order.
///
/// # Errors
///
/// Returns [`XlsxError`] if building or saving the workbook fails.
pub fn render(edition: &EditionInfo, workbook_epoch: i64) -> Result<Vec<u8>, XlsxError> {
    let input = synthetic_example();
    let mut workbook = decision_workbook(&input, edition, workbook_epoch)?;
    save_workbook(&mut workbook)
}
