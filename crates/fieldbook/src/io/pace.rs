//! Real pacer over `std::thread::sleep` (`model-gate` step 3).

use std::time::Duration;

use crate::adapters::model::Pace;

/// A [`Pace`] that sleeps for real.
#[derive(Debug, Clone, Copy, Default)]
pub struct RealPace;

impl Pace for RealPace {
    fn sleep_ms(&self, ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
    }
}
