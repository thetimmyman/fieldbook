//! Stored calculation run record (`Run`, `RunRounding`, `RunInput`, `RunOutput`).

use serde::{Deserialize, Serialize};

use crate::decimal::Dec;
use crate::money::Currency;
use crate::value::Value;

/// Rounding metadata recorded in a [`Run`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRounding {
    /// Stable rounding policy identifier (e.g. `"default"`).
    pub policy: String,
    /// Decimal scale (`0..=28`).
    pub scale: u32,
    /// Stable rounding mode identifier (e.g. `"midpoint_away_from_zero"`).
    pub mode: String,
}

/// One ordered input of a [`Run`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunInput {
    /// Input field name.
    pub name: String,
    /// Input value (`Known` or `Unknown`).
    pub value: Value<Dec>,
    /// Input origin (`"synthetic"`, `"quoted"`, `"computed"`, `"judgment"`).
    pub origin: String,
    /// Input pointer or synthetic case identifier.
    pub pointer: String,
    /// Unit word from the calculation vocabulary.
    pub unit: String,
    /// Validated currency when the input is monetary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<Currency>,
}

/// One ordered output of a [`Run`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunOutput {
    /// Output field name.
    pub name: String,
    /// Output value (`Known` or `Unknown`).
    pub value: Value<Dec>,
    /// Unit word from the calculation vocabulary.
    pub unit: String,
    /// Validated currency when the output is monetary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<Currency>,
}

/// Record of one calculation evaluation (AD-14).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Run {
    /// Versioned function identifier (`name@N`).
    pub func: String,
    /// Calculation epoch under which the run was evaluated.
    pub calculation_epoch: u32,
    /// Rounding policy, scale and mode.
    pub rounding: RunRounding,
    /// Ordered inputs.
    pub inputs: Vec<RunInput>,
    /// Ordered outputs.
    pub outputs: Vec<RunOutput>,
    /// Final rounding stage name (`"unit_price"`, `"extension"`, `"total"`, or `""`).
    pub stage: String,
    /// `true` when every output is `Value::Known`.
    pub complete: bool,
}

impl Run {
    /// Formats the run in the framed UTF-8 `canonical-run@1` text format.
    #[must_use]
    pub fn canonical(&self) -> String {
        crate::versions::run_v1::canonical(self)
    }
}
