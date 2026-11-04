//! Corpus date re-exports and review-volatility intervals.
//!
//! **Responsibility.** Re-exports the calendar, partial-date, period and duration
//! types from [`fieldbook_calc`] and computes Corpus review intervals from
//! [`Volatility`].
//!
//! **Rules.** Pure: no clock, no I/O, no environment.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

pub use fieldbook_calc::{CivilDate, Duration, EffectivePeriod, PartialDate};

/// The review volatility class of a page, source version or claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Volatility {
    /// Review every 90 days.
    V1,
    /// Review every 180 days.
    V2,
    /// Review every 365 days.
    V3,
}

impl Volatility {
    /// Every volatility class in order from most to least volatile.
    pub const ALL: [Self; 3] = [Self::V1, Self::V2, Self::V3];

    /// The exact serialized word (`V1`, `V2` or `V3`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::V1 => "V1",
            Self::V2 => "V2",
            Self::V3 => "V3",
        }
    }

    /// Parses one of the exact words `V1`, `V2` or `V3`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "V1" => Some(Self::V1),
            "V2" => Some(Self::V2),
            "V3" => Some(Self::V3),
            _ => None,
        }
    }

    /// The review interval in days for this volatility class.
    #[must_use]
    pub const fn interval_days(self) -> u32 {
        interval_days(self)
    }

    /// Computes the `review_by` date from `as_of`, saturating at `9999-12-31`.
    #[must_use]
    pub fn review_by(self, as_of: CivilDate) -> CivilDate {
        review_by(as_of, self)
    }
}

impl fmt::Display for Volatility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Volatility {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or("expected V1, V2 or V3")
    }
}

/// Returns the review interval in days (`90` for `V1`, `180` for `V2`, `365` for `V3`).
#[must_use]
pub const fn interval_days(v: Volatility) -> u32 {
    match v {
        Volatility::V1 => 90,
        Volatility::V2 => 180,
        Volatility::V3 => 365,
    }
}

/// Adds the volatility interval to `as_of`, saturating at `9999-12-31`.
#[must_use]
pub fn review_by(as_of: CivilDate, v: Volatility) -> CivilDate {
    let days = i64::from(interval_days(v));
    as_of.add_days(days).unwrap_or(CivilDate::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reexports_are_the_calc_types() {
        let calc_date: fieldbook_calc::CivilDate =
            CivilDate::parse("2024-02-29").expect("2024 is a leap year");
        let core_date: crate::date::CivilDate = calc_date;
        assert_eq!(core_date.to_string(), "2024-02-29");
        assert!(CivilDate::parse("2025-02-30").is_err());
    }

    #[test]
    fn volatility_serde_round_trip() {
        for (v, word) in [
            (Volatility::V1, "V1"),
            (Volatility::V2, "V2"),
            (Volatility::V3, "V3"),
        ] {
            assert_eq!(Volatility::parse(word), Some(v));
            assert_eq!(v.as_str(), word);
            let json = serde_json::to_string(&v).expect("serialize volatility");
            assert_eq!(json, format!("\"{word}\""));
            let back: Volatility = serde_json::from_str(&json).expect("deserialize volatility");
            assert_eq!(back, v);
        }
        assert_eq!(Volatility::parse("v1"), None);
        assert_eq!(Volatility::parse("V4"), None);
        assert!(serde_json::from_str::<Volatility>("\"V4\"").is_err());
        assert!(serde_json::from_str::<Volatility>("\"v1\"").is_err());
    }

    #[test]
    fn volatility_intervals_are_90_180_365() {
        assert_eq!(interval_days(Volatility::V1), 90);
        assert_eq!(interval_days(Volatility::V2), 180);
        assert_eq!(interval_days(Volatility::V3), 365);
    }

    #[test]
    fn review_by_adds_the_interval() {
        let base = CivilDate::parse("2025-01-01").expect("valid base date");
        assert_eq!(
            review_by(base, Volatility::V1),
            CivilDate::parse("2025-04-01").expect("valid +90d")
        );
        assert_eq!(
            review_by(base, Volatility::V2),
            CivilDate::parse("2025-06-30").expect("valid +180d")
        );
        assert_eq!(
            review_by(base, Volatility::V3),
            CivilDate::parse("2026-01-01").expect("valid +365d")
        );
    }

    #[test]
    fn review_by_stops_at_the_last_day() {
        let near_end = CivilDate::parse("9999-11-15").expect("valid near-end date");
        let last_day = CivilDate::parse("9999-12-31").expect("valid max date");
        assert_eq!(review_by(near_end, Volatility::V1), last_day);
        assert_eq!(review_by(near_end, Volatility::V2), last_day);
        assert_eq!(review_by(near_end, Volatility::V3), last_day);
        assert_eq!(review_by(last_day, Volatility::V1), last_day);
    }
}
