//! Child budget reconciliation (`check_children_sum` and `SumMismatch`, R-10).

use std::fmt;

use crate::money::Money;

/// Details of a child budget reconciliation failure (`R-10`, `CALC-04`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SumMismatch {
    /// Declared parent budget amount.
    pub parent: Money,
    /// Actual sum of the child budget amounts.
    pub total: Money,
    /// Absolute difference between `parent` and `total`.
    pub gap: Money,
}

impl fmt::Display for SumMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "children sum {} differs from parent {} by gap {}",
            self.total, self.parent, self.gap
        )
    }
}

impl std::error::Error for SumMismatch {}

/// Checks that `children` sum exactly to `parent` (`children_sum@1`).
///
/// Returns `Ok(())` when the exact sum of `children` equals `parent`, or
/// `Err(SumMismatch { parent, total, gap })` naming the parent amount, the children's
/// total and the absolute gap.
pub fn check_children_sum(parent: Money, children: &[Money]) -> Result<(), SumMismatch> {
    crate::versions::children_sum_v1::check_children_sum(parent, children)
}
