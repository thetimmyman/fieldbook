//! Dedicated implementation of `children_sum@1`.

use crate::cases::{BudgetOutcome, Case, Outcome};
use crate::children::SumMismatch;
use crate::decimal::Dec;
use crate::error::CalcError;
use crate::money::{Currency, Money};
use crate::ops;
use crate::rounding::RoundingPolicy;
use crate::run::{Run, RunInput, RunOutput, RunRounding};
use crate::unit::Unit;
use crate::value::Value;

pub(crate) fn rescale_dec_to(dec: Dec, scale: u32) -> Dec {
    let mut inner = dec.inner();
    if inner.scale() < scale {
        inner.rescale(scale);
    }
    Dec::from_decimal(inner)
}

/// Checked child-sum helper that surfaces [`CalcError`] on currency mismatch or overflow.
pub fn check_children_sum_checked(
    parent: Money,
    children: &[Money],
) -> Result<Result<(), SumMismatch>, CalcError> {
    let mut target_scale = parent.amount.scale();
    let mut total = Value::Known(Money::new(Dec::ZERO, parent.currency));
    for &child in children {
        target_scale = target_scale.max(child.amount.scale());
        total = ops::add(total, Value::Known(child))?;
    }
    let Value::Known(total_money) = total else {
        return Err(CalcError::Overflow);
    };
    let total_scaled = Money::new(
        rescale_dec_to(total_money.amount, target_scale),
        parent.currency,
    );
    let parent_scaled = Money::new(rescale_dec_to(parent.amount, target_scale), parent.currency);

    if total_scaled == parent_scaled {
        return Ok(Ok(()));
    }

    let raw_gap = if total_scaled.amount >= parent_scaled.amount {
        ops::sub(Value::Known(total_scaled), Value::Known(parent_scaled))?
    } else {
        ops::sub(Value::Known(parent_scaled), Value::Known(total_scaled))?
    };
    let Value::Known(gap_money) = raw_gap else {
        return Err(CalcError::Overflow);
    };
    let gap_scaled = Money::new(
        rescale_dec_to(gap_money.amount, target_scale),
        parent.currency,
    );
    Ok(Err(SumMismatch {
        parent: parent_scaled,
        total: total_scaled,
        gap: gap_scaled,
    }))
}

/// Checks that `children` sum to `parent` (`children_sum@1`).
pub fn check_children_sum(parent: Money, children: &[Money]) -> Result<(), SumMismatch> {
    match check_children_sum_checked(parent, children) {
        Ok(res) => res,
        Err(_) => Err(SumMismatch {
            parent,
            total: Money::new(Dec::ZERO, parent.currency),
            gap: parent,
        }),
    }
}

/// Evaluates a `children_sum@1` case at `calculation_epoch`.
pub fn evaluate_children_sum(case: &Case, calculation_epoch: u32) -> Result<Outcome, CalcError> {
    let currency = Currency::parse(case.currency_str())?;
    let policy = RoundingPolicy::new(case.rounding_scale())?;
    if case.budget.is_empty() {
        return Err(CalcError::Grammar);
    }

    let mut run_inputs = Vec::new();
    let mut run_outputs = Vec::new();
    let mut budget_outcomes = Vec::with_capacity(case.budget.len());

    let multi = case.budget.len() > 1;
    for (idx, row) in case.budget.iter().enumerate() {
        if row.children.is_empty() {
            return Err(CalcError::Grammar);
        }
        if row.expect != "accepted" && row.expect != "refused" {
            return Err(CalcError::Grammar);
        }
        if row.expect == "accepted" && (row.total.is_some() || row.gap.is_some()) {
            return Err(CalcError::Grammar);
        }
        if row.expect == "refused" && (row.total.is_none() || row.gap.is_none()) {
            return Err(CalcError::Grammar);
        }
        if let Some(t) = &row.total {
            let _ = Dec::parse(t)?;
        }
        if let Some(g) = &row.gap {
            let _ = Dec::parse(g)?;
        }

        let parent_dec = Dec::parse(&row.parent)?;
        let parent = Money::new(parent_dec, currency);

        let prefix = if multi {
            format!("row_{}_", idx + 1)
        } else {
            String::new()
        };

        run_inputs.push(RunInput {
            name: format!("{prefix}parent"),
            value: Value::Known(parent_dec),
            origin: "synthetic".to_string(),
            pointer: case.id.clone(),
            unit: Unit::Each.word().to_string(),
            currency: Some(currency),
        });

        let mut children = Vec::with_capacity(row.children.len());
        for (c_idx, child_str) in row.children.iter().enumerate() {
            let child_dec = Dec::parse(child_str)?;
            children.push(Money::new(child_dec, currency));
            run_inputs.push(RunInput {
                name: format!("{prefix}child_{}", c_idx + 1),
                value: Value::Known(child_dec),
                origin: "synthetic".to_string(),
                pointer: case.id.clone(),
                unit: Unit::Each.word().to_string(),
                currency: Some(currency),
            });
        }

        // Verify that the error propagation check runs first (so overflow/currency errors return CalcError),
        // then invoke `crate::children::check_children_sum` as the authoritative reconciliation check.
        let _ = check_children_sum_checked(parent, &children)?;
        let res = crate::children::check_children_sum(parent, &children);

        let (total_dec, gap_dec) = match res {
            Ok(()) => (
                rescale_dec_to(parent.amount, policy.scale()),
                rescale_dec_to(Dec::ZERO, policy.scale()),
            ),
            Err(mismatch) => (
                rescale_dec_to(mismatch.total.amount, policy.scale()),
                rescale_dec_to(mismatch.gap.amount, policy.scale()),
            ),
        };

        run_outputs.push(RunOutput {
            name: format!("{prefix}total"),
            value: Value::Known(total_dec),
            unit: Unit::Each.word().to_string(),
            currency: Some(currency),
        });
        run_outputs.push(RunOutput {
            name: format!("{prefix}gap"),
            value: Value::Known(gap_dec),
            unit: Unit::Each.word().to_string(),
            currency: Some(currency),
        });

        budget_outcomes.push(BudgetOutcome { result: res });
    }

    let complete = run_outputs.iter().all(|o| o.value.is_known());
    let stage = case
        .stages_slice()
        .last()
        .map_or_else(String::new, |s| s.word().to_string());

    let run = Run {
        func: case.func.clone(),
        calculation_epoch,
        rounding: RunRounding {
            policy: case.rounding_policy_name().to_string(),
            scale: policy.scale(),
            mode: policy.mode_word().to_string(),
        },
        inputs: run_inputs,
        outputs: run_outputs.clone(),
        stage,
        complete,
    };

    Ok(Outcome {
        outputs: run_outputs,
        run,
        budget_outcomes,
    })
}
