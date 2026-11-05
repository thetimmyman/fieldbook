//! The one table of versioned calculation functions (`CALC_FNS`) and `CALC_EPOCH` (AD-14).

use crate::rounding::Stage;
use crate::unit::Unit;

/// Current calculation epoch, tied to the digest of the synthetic case set (`EPOCH`).
pub const CALC_EPOCH: u32 = 1;

/// Provenance origin of a calculation function's output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Origin {
    /// Figure produced by a named calculation function (`computed`).
    Computed,
}

impl Origin {
    /// Returns the vocabulary word for this origin.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Computed => "computed",
        }
    }
}

/// Dimension slot of an input or output of a calculation function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Slot {
    /// Monetary amount in a currency.
    Money,
    /// Quantity in a specific [`Unit`].
    Qty(Unit),
    /// Percentage (`Unit::Percent`).
    Percent,
    /// Dimensionless number.
    Plain,
}

/// One row of [`CALC_FNS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FnRow {
    /// Versioned function identifier (`name@N`).
    pub id: &'static str,
    /// Input slot dimensions in order.
    pub inputs: &'static [Slot],
    /// Output slot dimensions in order.
    pub outputs: &'static [Slot],
    /// Named rounding stages called by the function.
    pub stages: &'static [Stage],
    /// Output origin (`Origin::Computed`).
    pub origin: Origin,
    /// Whether the function is iterative (and therefore permits `tolerance` on cases).
    pub iterative: bool,
}

const BUILD_UP_V1_INPUTS: &[Slot] = &[
    Slot::Money,
    Slot::Qty(Unit::Each),
    Slot::Money,
    Slot::Money,
    Slot::Qty(Unit::Hours),
    Slot::Money,
    Slot::Percent,
    Slot::Percent,
];

const BUILD_UP_V1_OUTPUTS: &[Slot] = &[
    Slot::Money,
    Slot::Money,
    Slot::Money,
    Slot::Money,
    Slot::Money,
    Slot::Money,
    Slot::Money,
    Slot::Money,
];

const BUILD_UP_V1_STAGES: &[Stage] = &[Stage::UnitPrice, Stage::Extension, Stage::Total];

const CHILDREN_SUM_V1_INPUTS: &[Slot] = &[Slot::Money, Slot::Money];
const CHILDREN_SUM_V1_OUTPUTS: &[Slot] = &[Slot::Money, Slot::Money];
const CHILDREN_SUM_V1_STAGES: &[Stage] = &[];

/// The single table of versioned calculation functions in `fieldbook-calc`.
pub const CALC_FNS: &[FnRow] = &[
    FnRow {
        id: "build_up@1",
        inputs: BUILD_UP_V1_INPUTS,
        outputs: BUILD_UP_V1_OUTPUTS,
        stages: BUILD_UP_V1_STAGES,
        origin: Origin::Computed,
        iterative: false,
    },
    FnRow {
        id: "children_sum@1",
        inputs: CHILDREN_SUM_V1_INPUTS,
        outputs: CHILDREN_SUM_V1_OUTPUTS,
        stages: CHILDREN_SUM_V1_STAGES,
        origin: Origin::Computed,
        iterative: false,
    },
];

/// Looks up a calculation function row by its versioned id (`name@N`).
#[must_use]
pub fn lookup(id: &str) -> Option<&'static FnRow> {
    CALC_FNS.iter().find(|row| row.id == id)
}

/// Checks whether `id` matches `^[a-z][a-z0-9_]*@[0-9]+$`.
#[must_use]
pub fn has_fn_id_shape(id: &str) -> bool {
    let Some((name, ver)) = id.split_once('@') else {
        return false;
    };
    let mut name_bytes = name.bytes();
    let Some(first) = name_bytes.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    if !name_bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
        return false;
    }
    !ver.is_empty() && ver.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::{CALC_FNS, has_fn_id_shape};

    #[test]
    fn fn_ids_have_the_shape() {
        for row in CALC_FNS {
            assert!(
                has_fn_id_shape(row.id),
                "CALC_FNS id `{}` does not match ^[a-z][a-z0-9_]*@[0-9]+$",
                row.id
            );
        }
    }
}
