//! Tests for `CivilDate`, `PartialDate`, `EffectivePeriod` and `Duration`.

mod dates {
    use std::cmp::Ordering;

    use fieldbook_calc::{CalcError, CivilDate, Duration, EffectivePeriod, PartialDate, Unit};

    #[test]
    fn valid_leap_day_2024_02_29() {
        let d = CivilDate::parse("2024-02-29").expect("2024 is a leap year");
        assert_eq!(d.year(), 2024);
        assert_eq!(d.month(), 2);
        assert_eq!(d.day(), 29);
        assert_eq!(d.to_string(), "2024-02-29");
    }

    #[test]
    fn valid_leap_day_2000_02_29() {
        let d = CivilDate::parse("2000-02-29").expect("2000 is a 400-year leap year");
        assert_eq!(d.to_string(), "2000-02-29");
    }

    #[test]
    fn invalid_2025_02_30_is_refused() {
        assert_eq!(CivilDate::parse("2025-02-30"), Err(CalcError::Grammar));
    }

    #[test]
    fn invalid_2025_13_01_is_refused() {
        assert_eq!(CivilDate::parse("2025-13-01"), Err(CalcError::Grammar));
    }

    #[test]
    fn invalid_0000_01_01_is_refused() {
        assert_eq!(CivilDate::parse("0000-01-01"), Err(CalcError::Grammar));
    }

    #[test]
    fn invalid_2100_02_29_is_refused() {
        assert_eq!(CivilDate::parse("2100-02-29"), Err(CalcError::Grammar));
    }

    #[test]
    fn invalid_unpadded_2025_1_1_is_refused() {
        assert_eq!(CivilDate::parse("2025-1-1"), Err(CalcError::Grammar));
    }

    #[test]
    fn invalid_leading_space_is_refused() {
        assert_eq!(CivilDate::parse(" 2025-01-01"), Err(CalcError::Grammar));
    }

    #[test]
    fn century_years_follow_the_leap_rule() {
        assert_eq!(
            CivilDate::parse("1900-02-29"),
            Err(CalcError::Grammar),
            "1900 is divisible by 100 but not 400, so it is not a leap year"
        );
        let d2000 = CivilDate::parse("2000-02-29").expect("2000 is divisible by 400");
        assert_eq!(d2000.to_string(), "2000-02-29");
    }

    #[test]
    fn add_days_crosses_a_month_and_a_year() {
        let jan_31 = CivilDate::parse("2024-01-31").expect("valid date");
        assert_eq!(
            jan_31.add_days(1),
            Ok(CivilDate::parse("2024-02-01").expect("valid date"))
        );
        assert_eq!(
            jan_31.add_days(29),
            Ok(CivilDate::parse("2024-02-29").expect("valid date"))
        );
        assert_eq!(
            jan_31.add_days(30),
            Ok(CivilDate::parse("2024-03-01").expect("valid date"))
        );

        let dec_31 = CivilDate::parse("2024-12-31").expect("valid date");
        let jan_01 = CivilDate::parse("2025-01-01").expect("valid date");
        assert_eq!(dec_31.add_days(1), Ok(jan_01));
        assert_eq!(jan_01.add_days(-1), Ok(dec_31));
    }

    #[test]
    fn add_days_past_year_9999_overflows() {
        let max = CivilDate::parse("9999-12-31").expect("valid max date");
        assert_eq!(max.add_days(0), Ok(max));
        assert_eq!(max.add_days(1), Err(CalcError::Overflow));
        assert_eq!(max.add_days(i64::MAX), Err(CalcError::Overflow));

        let min = CivilDate::parse("0001-01-01").expect("valid min date");
        assert_eq!(min.add_days(0), Ok(min));
        assert_eq!(min.add_days(-1), Err(CalcError::Overflow));
        assert_eq!(min.add_days(i64::MIN), Err(CalcError::Overflow));
    }

    #[test]
    fn days_between_is_signed() {
        let start = CivilDate::parse("2025-01-01").expect("valid date");
        let later = CivilDate::parse("2025-01-11").expect("valid date");
        assert_eq!(start.days_between(later), 10);
        assert_eq!(later.days_between(start), -10);
        assert_eq!(start.days_between(start), 0);
    }

    #[test]
    fn epoch_seconds_give_the_date() {
        assert_eq!(
            CivilDate::from_epoch_seconds(0),
            Ok(CivilDate::parse("1970-01-01").expect("valid date"))
        );
        assert_eq!(
            CivilDate::from_epoch_seconds(31_536_000),
            Ok(CivilDate::parse("1971-01-01").expect("valid date"))
        );
        assert_eq!(
            CivilDate::from_epoch_seconds(-1),
            Ok(CivilDate::parse("1969-12-31").expect("valid date"))
        );
        assert_eq!(
            CivilDate::from_epoch_seconds(-86_400),
            Ok(CivilDate::parse("1969-12-31").expect("valid date"))
        );
        assert_eq!(
            CivilDate::from_epoch_seconds(i64::MAX),
            Err(CalcError::Overflow)
        );
    }

    #[test]
    fn partial_date_prints_as_written() {
        for raw in ["2025", "2025-11", "2025-11-01"] {
            let p = PartialDate::parse(raw).expect("valid partial date");
            assert_eq!(p.to_string(), raw);
        }
        assert_eq!(PartialDate::parse("2025-00"), Err(CalcError::Grammar));
        assert_eq!(PartialDate::parse("2025-13"), Err(CalcError::Grammar));
        assert_eq!(PartialDate::parse("0000"), Err(CalcError::Grammar));
    }

    #[test]
    fn partial_date_orders_by_its_first_day() {
        let dec_2023 = PartialDate::parse("2023-12-31").expect("valid full date");
        let y2024 = PartialDate::parse("2024").expect("valid year");
        let ym2024_01 = PartialDate::parse("2024-01").expect("valid year-month");
        let ymd2024_01_01 = PartialDate::parse("2024-01-01").expect("valid full date");
        let ymd2024_01_02 = PartialDate::parse("2024-01-02").expect("valid full date");
        let ym2024_02 = PartialDate::parse("2024-02").expect("valid year-month");
        let y2025 = PartialDate::parse("2025").expect("valid year");

        assert_eq!(y2024.first_day(), ym2024_01.first_day());
        assert_eq!(ym2024_01.first_day(), ymd2024_01_01.first_day());
        assert_ne!(y2024.cmp(&ym2024_01), Ordering::Equal);
        assert_ne!(ym2024_01.cmp(&ymd2024_01_01), Ordering::Equal);
        assert!(dec_2023 < y2024);
        assert!(y2024 < ym2024_01);
        assert!(ym2024_01 < ymd2024_01_01);
        assert!(ymd2024_01_01 < ymd2024_01_02);
        assert!(ymd2024_01_02 < ym2024_02);
        assert!(ym2024_02 < y2025);
    }

    #[test]
    fn partial_date_never_becomes_full() {
        let y = PartialDate::parse("2025").expect("valid year");
        let ym = PartialDate::parse("2025-11").expect("valid month");
        let ymd = PartialDate::parse("2025-11-01").expect("valid day");

        assert_eq!(y.to_civil(), None);
        assert_eq!(ym.to_civil(), None);
        assert_eq!(
            ymd.to_civil(),
            Some(CivilDate::parse("2025-11-01").expect("valid date"))
        );
    }

    #[test]
    fn period_contains_is_inclusive() {
        let period = EffectivePeriod::parse("2024-03-01/2024-03-31").expect("valid closed period");
        let before = CivilDate::parse("2024-02-29").expect("valid date");
        let start = CivilDate::parse("2024-03-01").expect("valid date");
        let mid = CivilDate::parse("2024-03-15").expect("valid date");
        let end = CivilDate::parse("2024-03-31").expect("valid date");
        let after = CivilDate::parse("2024-04-01").expect("valid date");

        assert!(!period.contains(before));
        assert!(period.contains(start));
        assert!(period.contains(mid));
        assert!(period.contains(end));
        assert!(!period.contains(after));
        assert_eq!(period.to_string(), "2024-03-01/2024-03-31");
    }

    #[test]
    fn period_with_an_open_end_contains_later_dates() {
        let period = EffectivePeriod::parse("2024-03-01/").expect("valid open period");
        assert!(period.end.is_none());
        assert!(!period.contains(CivilDate::parse("2024-02-29").expect("valid date")));
        assert!(period.contains(CivilDate::parse("2024-03-01").expect("valid date")));
        assert!(period.contains(CivilDate::parse("9999-12-31").expect("valid date")));
        assert_eq!(period.to_string(), "2024-03-01/");
    }

    #[test]
    fn period_with_an_end_before_the_start_is_refused() {
        assert_eq!(
            EffectivePeriod::parse("2024-03-10/2024-03-09"),
            Err(CalcError::Grammar)
        );
    }

    #[test]
    fn periods_overlap() {
        let q1 = EffectivePeriod::parse("2024-01-01/2024-03-31").expect("valid period");
        let mar_apr = EffectivePeriod::parse("2024-03-31/2024-04-30").expect("valid period");
        let q2 = EffectivePeriod::parse("2024-04-01/2024-06-30").expect("valid period");
        let open_from_may = EffectivePeriod::parse("2024-05-01/").expect("valid period");
        let open_from_jul = EffectivePeriod::parse("2024-07-01/").expect("valid period");

        assert!(q1.overlaps(&mar_apr));
        assert!(mar_apr.overlaps(&q1));
        assert!(!q1.overlaps(&q2));
        assert!(!q2.overlaps(&q1));
        assert!(q2.overlaps(&open_from_may));
        assert!(!q2.overlaps(&open_from_jul));
        assert!(open_from_may.overlaps(&open_from_jul));
    }

    #[test]
    fn duration_refuses_a_non_time_unit() {
        for non_time in [
            Unit::Each,
            Unit::Percent,
            Unit::BasisPoints,
            Unit::Count,
            Unit::Ratio,
            Unit::Per100,
            Unit::Per1000,
            Unit::PerDozen,
        ] {
            let err = Duration::new(10, non_time)
                .expect_err(&format!("expected Duration::new to refuse {non_time}"));
            assert_eq!(err, CalcError::Mismatch);
            assert_eq!(err.code(), "FB0502");
        }
    }

    #[test]
    fn duration_days_checks_overflow() {
        let d_days = Duration::new(14, Unit::Days).expect("days is a time unit");
        assert_eq!(d_days.amount(), 14);
        assert_eq!(d_days.unit(), Unit::Days);
        assert_eq!(d_days.days(), Ok(Some(14)));

        let d_weeks = Duration::new(3, Unit::Weeks).expect("weeks is a time unit");
        assert_eq!(d_weeks.days(), Ok(Some(21)));

        let d_months = Duration::new(6, Unit::Months).expect("months is a time unit");
        assert_eq!(d_months.days(), Ok(None));

        let d_bzn = Duration::new(5, Unit::BusinessDays).expect("business_days is a time unit");
        assert_eq!(d_bzn.days(), Ok(None));

        let d_overflow = Duration::new(i64::MAX, Unit::Weeks).expect("weeks is a time unit");
        assert_eq!(d_overflow.days(), Err(CalcError::Overflow));
    }
}
