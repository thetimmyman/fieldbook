//! Dedicated implementation of the `canonical-run@1` framed UTF-8 run format.

use std::fmt::Write as _;

use crate::decimal::Dec;
use crate::money::Currency;
use crate::run::Run;
use crate::value::{Reason, Value};

fn write_framed_str(out: &mut String, s: &str) {
    let _ = write!(out, "{}:{s}", s.len());
}

fn format_value(value: Value<Dec>) -> String {
    match value {
        Value::Known(dec) => format!("known:{dec}"),
        Value::Unknown(Reason::ZeroDenominator) => "unknown:zero_denominator".to_string(),
        Value::Unknown(Reason::Inexact) => "unknown:inexact".to_string(),
    }
}

/// Emits the canonical UTF-8 representation of `run` under `canonical-run@1`.
#[must_use]
pub fn canonical(run: &Run) -> String {
    let mut out = String::new();
    out.push_str("run 1\n");

    out.push_str("fn ");
    write_framed_str(&mut out, &run.func);
    out.push('\n');

    let _ = writeln!(out, "epoch {}", run.calculation_epoch);

    out.push_str("rounding ");
    write_framed_str(&mut out, &run.rounding.policy);
    let _ = write!(out, " {} ", run.rounding.scale);
    write_framed_str(&mut out, &run.rounding.mode);
    out.push('\n');

    out.push_str("stage ");
    write_framed_str(&mut out, &run.stage);
    out.push('\n');

    let _ = writeln!(
        out,
        "complete {}",
        if run.complete { "true" } else { "false" }
    );

    let _ = writeln!(out, "inputs {}", run.inputs.len());
    for input in &run.inputs {
        let val_str = format_value(input.value);
        let curr_str = input.currency.as_ref().map_or("", Currency::as_str);
        out.push_str("in ");
        write_framed_str(&mut out, &input.name);
        out.push(' ');
        write_framed_str(&mut out, &val_str);
        out.push(' ');
        write_framed_str(&mut out, &input.origin);
        out.push(' ');
        write_framed_str(&mut out, &input.pointer);
        out.push(' ');
        write_framed_str(&mut out, &input.unit);
        out.push(' ');
        write_framed_str(&mut out, curr_str);
        out.push('\n');
    }

    let _ = writeln!(out, "outputs {}", run.outputs.len());
    for output in &run.outputs {
        let val_str = format_value(output.value);
        let curr_str = output.currency.as_ref().map_or("", Currency::as_str);
        out.push_str("out ");
        write_framed_str(&mut out, &output.name);
        out.push(' ');
        write_framed_str(&mut out, &val_str);
        out.push(' ');
        write_framed_str(&mut out, &output.unit);
        out.push(' ');
        write_framed_str(&mut out, curr_str);
        out.push('\n');
    }

    out
}
