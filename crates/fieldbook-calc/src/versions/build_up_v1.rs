//! Dedicated implementation of `build_up@1` (AD-14, FM0 CALC-01 to CALC-03, CALC-90).

use crate::cases::{Case, Outcome};
use crate::decimal::Dec;
use crate::error::CalcError;
use crate::money::{Currency, Money};
use crate::ops;
use crate::rounding::{RoundingPolicy, Stage, round_money_at_stage};
use crate::run::{Run, RunInput, RunOutput, RunRounding};
use crate::unit::{Unit, convert};
use crate::value::Value;
use crate::versions::children_sum_v1::rescale_dec_to;

fn scale_money_value(val: Value<Money>, scale: u32) -> Value<Money> {
    val.map(|m| Money::new(rescale_dec_to(m.amount, scale), m.currency))
}

fn round_and_scale_money(
    val: Value<Money>,
    stage: Stage,
    policy: &RoundingPolicy,
) -> Result<Value<Money>, CalcError> {
    let rounded = round_money_at_stage(val, stage, policy)?;
    Ok(scale_money_value(rounded, policy.scale()))
}

/// Evaluates a `build_up@1` case at `calculation_epoch`.
pub fn evaluate_build_up(case: &Case, calculation_epoch: u32) -> Result<Outcome, CalcError> {
    let currency = Currency::parse(case.currency_str())?;
    let policy = RoundingPolicy::new(case.rounding_scale())?;
    let Some(inputs) = &case.inputs else {
        return Err(CalcError::Grammar);
    };

    if inputs.markup_percent.is_some() && inputs.margin_percent.is_some() {
        return Err(CalcError::Grammar);
    }

    let quote_price_dec = Dec::parse(&inputs.quote_price)?;
    let quote_unit_str = inputs.quote_unit.as_deref().unwrap_or("each");
    let quote_unit = Unit::from_word(quote_unit_str)?;

    let quantity_dec = Dec::parse(&inputs.quantity)?;
    let quantity_unit_str = inputs.quantity_unit.as_deref().unwrap_or("each");
    let quantity_unit = Unit::from_word(quantity_unit_str)?;

    let setup_dec = Dec::parse(inputs.setup.as_deref().unwrap_or("0.00"))?;
    let freight_dec = Dec::parse(inputs.freight.as_deref().unwrap_or("0.00"))?;
    let labour_hours_dec = Dec::parse(inputs.labour_hours.as_deref().unwrap_or("0"))?;
    let labour_rate_dec = Dec::parse(inputs.labour_rate.as_deref().unwrap_or("0.00"))?;
    let overhead_percent_dec = Dec::parse(inputs.overhead_percent.as_deref().unwrap_or("0"))?;

    let (rate_field_name, rate_percent_dec, is_margin) =
        if let Some(margin_str) = &inputs.margin_percent {
            ("margin_percent", Dec::parse(margin_str)?, true)
        } else if let Some(markup_str) = &inputs.markup_percent {
            ("markup_percent", Dec::parse(markup_str)?, false)
        } else {
            ("markup_percent", Dec::ZERO, false)
        };

    // 1. Unit price: convert from `quote_unit` to `quantity_unit` and round at `UnitPrice`.
    let unit_converted = convert(quote_price_dec, quote_unit, quantity_unit)?;
    let unit_money_raw = unit_converted.map(|d| Money::new(d, currency));
    let unit_money = round_and_scale_money(unit_money_raw, Stage::UnitPrice, &policy)?;

    // 2. Extension: unit price * quantity, rounded at `Extension`.
    let ext_raw = ops::mul(unit_money, Value::Known(quantity_dec))?;
    let ext_money = round_and_scale_money(ext_raw, Stage::Extension, &policy)?;

    // 3. Landed cost: extension + one-time setup + freight, rounded at `Total`.
    let setup_money = Value::Known(Money::new(setup_dec, currency));
    let freight_money = Value::Known(Money::new(freight_dec, currency));
    let with_setup = ops::add(ext_money, setup_money)?;
    let landed_raw = ops::add(with_setup, freight_money)?;
    let landed_money = round_and_scale_money(landed_raw, Stage::Total, &policy)?;

    // 4. Direct labour: labour_rate * labour_hours, rounded at `Extension`.
    let labour_rate_money = Value::Known(Money::new(labour_rate_dec, currency));
    let labour_raw = ops::mul(labour_rate_money, Value::Known(labour_hours_dec))?;
    let labour_money = round_and_scale_money(labour_raw, Stage::Extension, &policy)?;

    // 5. Overhead on declared overhead base, rounded at `Extension`.
    let overhead_base_amount = match case.overhead_base.as_str() {
        "direct_labour" => labour_money,
        "material" | "landed" => landed_money,
        "none" => Value::Known(Money::new(Dec::ZERO, currency)),
        _ => return Err(CalcError::Grammar),
    };
    let overhead_raw = ops::percent_of(overhead_base_amount, Value::Known(overhead_percent_dec))?;
    let overhead_money = round_and_scale_money(overhead_raw, Stage::Extension, &policy)?;

    // 6. Total cost: landed + labour + overhead, rounded at `Total`.
    let landed_plus_labour = ops::add(landed_money, labour_money)?;
    let cost_raw = ops::add(landed_plus_labour, overhead_money)?;
    let cost_money = round_and_scale_money(cost_raw, Stage::Total, &policy)?;

    // 7. Profit and price at the case's named price stage (`Total`).
    let price_stage = case.stages_slice().last().copied().unwrap_or(Stage::Total);
    let (profit_money, price_money) = if is_margin {
        let margin_fraction = ops::div_dec(
            Value::Known(rate_percent_dec),
            Value::Known(Dec::ONE_HUNDRED),
        )?;
        let one_minus_margin = ops::sub_dec(Value::Known(Dec::ONE), margin_fraction)?;
        let price_unscaled = ops::div_at_stage(cost_money, one_minus_margin, price_stage, &policy)?;
        let price_scaled = scale_money_value(price_unscaled, policy.scale());
        let profit_raw = ops::sub(price_scaled, cost_money)?;
        let profit_scaled = round_and_scale_money(profit_raw, price_stage, &policy)?;
        (profit_scaled, price_scaled)
    } else {
        let profit_raw = ops::percent_of(cost_money, Value::Known(rate_percent_dec))?;
        let profit_scaled = round_and_scale_money(profit_raw, price_stage, &policy)?;
        let price_raw = ops::add(cost_money, profit_scaled)?;
        let price_scaled = round_and_scale_money(price_raw, price_stage, &policy)?;
        (profit_scaled, price_scaled)
    };

    let run_inputs = vec![
        RunInput {
            name: "quote_price".to_string(),
            value: Value::Known(quote_price_dec),
            origin: "synthetic".to_string(),
            pointer: case.id.clone(),
            unit: quote_unit.word().to_string(),
            currency: Some(currency),
        },
        RunInput {
            name: "quantity".to_string(),
            value: Value::Known(quantity_dec),
            origin: "synthetic".to_string(),
            pointer: case.id.clone(),
            unit: quantity_unit.word().to_string(),
            currency: None,
        },
        RunInput {
            name: "setup".to_string(),
            value: Value::Known(setup_dec),
            origin: "synthetic".to_string(),
            pointer: case.id.clone(),
            unit: Unit::Each.word().to_string(),
            currency: Some(currency),
        },
        RunInput {
            name: "freight".to_string(),
            value: Value::Known(freight_dec),
            origin: "synthetic".to_string(),
            pointer: case.id.clone(),
            unit: Unit::Each.word().to_string(),
            currency: Some(currency),
        },
        RunInput {
            name: "labour_hours".to_string(),
            value: Value::Known(labour_hours_dec),
            origin: "synthetic".to_string(),
            pointer: case.id.clone(),
            unit: Unit::Hours.word().to_string(),
            currency: None,
        },
        RunInput {
            name: "labour_rate".to_string(),
            value: Value::Known(labour_rate_dec),
            origin: "synthetic".to_string(),
            pointer: case.id.clone(),
            unit: Unit::Hours.word().to_string(),
            currency: Some(currency),
        },
        RunInput {
            name: "overhead_percent".to_string(),
            value: Value::Known(overhead_percent_dec),
            origin: "synthetic".to_string(),
            pointer: case.id.clone(),
            unit: Unit::Percent.word().to_string(),
            currency: None,
        },
        RunInput {
            name: rate_field_name.to_string(),
            value: Value::Known(rate_percent_dec),
            origin: "synthetic".to_string(),
            pointer: case.id.clone(),
            unit: Unit::Percent.word().to_string(),
            currency: None,
        },
    ];

    let each_word = Unit::Each.word();
    let run_outputs = vec![
        RunOutput {
            name: "unit".to_string(),
            value: unit_money.map(|m| m.amount),
            unit: quantity_unit.word().to_string(),
            currency: Some(currency),
        },
        RunOutput {
            name: "extension".to_string(),
            value: ext_money.map(|m| m.amount),
            unit: each_word.to_string(),
            currency: Some(currency),
        },
        RunOutput {
            name: "landed".to_string(),
            value: landed_money.map(|m| m.amount),
            unit: each_word.to_string(),
            currency: Some(currency),
        },
        RunOutput {
            name: "labour".to_string(),
            value: labour_money.map(|m| m.amount),
            unit: each_word.to_string(),
            currency: Some(currency),
        },
        RunOutput {
            name: "overhead".to_string(),
            value: overhead_money.map(|m| m.amount),
            unit: each_word.to_string(),
            currency: Some(currency),
        },
        RunOutput {
            name: "cost".to_string(),
            value: cost_money.map(|m| m.amount),
            unit: each_word.to_string(),
            currency: Some(currency),
        },
        RunOutput {
            name: "profit".to_string(),
            value: profit_money.map(|m| m.amount),
            unit: each_word.to_string(),
            currency: Some(currency),
        },
        RunOutput {
            name: "price".to_string(),
            value: price_money.map(|m| m.amount),
            unit: each_word.to_string(),
            currency: Some(currency),
        },
    ];

    let complete = run_outputs.iter().all(|o| o.value.is_known());
    let stage = case
        .stages_slice()
        .last()
        .map_or_else(|| price_stage.word().to_string(), |s| s.word().to_string());

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
        budget_outcomes: Vec::new(),
    })
}
