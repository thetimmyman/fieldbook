//! Drift test keeping `Unit::ALL` in step with `docs/reference/corpus-format.md`.

mod units {
    use fieldbook_calc::unit::{UnitFamily, convert};
    use fieldbook_calc::{CalcError, Currency, Dec, Reason, Unit, Value};

    fn dec(s: &str) -> Dec {
        Dec::parse(s).unwrap_or_else(|e| panic!("valid Dec `{s}`: {e}"))
    }

    const CORPUS_FORMAT: &str = include_str!("../../../docs/reference/corpus-format.md");

    fn backticked_tokens(text: &str) -> Vec<&str> {
        let mut tokens = Vec::new();
        let mut rest = text;
        while let Some((_, after_open)) = rest.split_once('`') {
            if let Some((token, after_close)) = after_open.split_once('`') {
                tokens.push(token);
                rest = after_close;
            } else {
                break;
            }
        }
        tokens
    }

    #[test]
    fn unit_words_match_the_reference() {
        let unit_row = CORPUS_FORMAT
            .lines()
            .find(|line| line.starts_with("| `unit` |"))
            .expect("corpus-format.md must contain the `| `unit` |` row in Claim vocabularies");

        let mut cells = unit_row.split('|');
        let _leading = cells.next();
        let _field_col = cells.next();
        let value_col = cells
            .next()
            .expect("unit row must have a Value column in Claim vocabularies");

        let ref_unit_words: Vec<&str> = backticked_tokens(value_col)
            .into_iter()
            .filter(|word| Currency::parse(word).is_err())
            .collect();

        let code_unit_words: Vec<&str> = Unit::ALL.iter().map(|u| u.word()).collect();
        assert_eq!(
            code_unit_words, ref_unit_words,
            "Unit::ALL words must match the non-currency words of the `unit` row in corpus-format.md"
        );

        for &unit in Unit::ALL {
            assert_eq!(Unit::from_word(unit.word()), Ok(unit));
        }

        let lint_line = CORPUS_FORMAT
            .lines()
            .find(|line| line.starts_with("- **Number-unit lint.**"))
            .expect("corpus-format.md must contain `- **Number-unit lint.**` in notes on 03 range");

        let (_, after_prefix) = lint_line
            .split_once("a unit word (")
            .expect("number-unit lint sentence must contain `a unit word (`");
        let (unit_word_list, _) = after_prefix
            .split_once(')')
            .expect("number-unit lint unit word list must close with `)`");

        let scale_and_money_words = ["dollars", "million", "billion", "thousand", "percent"];
        let ref_time_words: Vec<&str> = backticked_tokens(unit_word_list)
            .into_iter()
            .filter(|w| !scale_and_money_words.contains(w))
            .collect();

        let code_plain_time_words: Vec<&str> = Unit::ALL
            .iter()
            .copied()
            .filter(|u| u.is_time() && !u.word().contains('_'))
            .map(Unit::word)
            .collect();

        assert_eq!(
            code_plain_time_words,
            vec!["days", "weeks", "months", "years", "hours"]
        );
        assert_eq!(
            code_plain_time_words, ref_time_words,
            "plain time words in Unit::ALL must match the time words of the number-unit lint sentence"
        );
    }

    #[test]
    fn every_unit_has_one_family() {
        let expected = [
            (Unit::Percent, UnitFamily::Percent),
            (Unit::BasisPoints, UnitFamily::Percent),
            (Unit::Days, UnitFamily::Days),
            (Unit::BusinessDays, UnitFamily::BusinessDays),
            (Unit::Weeks, UnitFamily::Days),
            (Unit::Months, UnitFamily::Months),
            (Unit::Years, UnitFamily::Months),
            (Unit::Hours, UnitFamily::Hours),
            (Unit::Count, UnitFamily::Count),
            (Unit::Ratio, UnitFamily::Ratio),
            (Unit::Each, UnitFamily::PriceBasis),
            (Unit::Per100, UnitFamily::PriceBasis),
            (Unit::Per1000, UnitFamily::PriceBasis),
            (Unit::PerDozen, UnitFamily::PriceBasis),
        ];
        assert_eq!(expected.len(), Unit::ALL.len());
        for (unit, fam) in expected {
            assert_eq!(unit.family(), fam);
            assert_eq!(convert(dec("1"), unit, unit), Ok(Value::Known(dec("1"))));
        }
    }

    #[test]
    fn weeks_convert_to_days_exactly() {
        assert_eq!(
            convert(dec("1"), Unit::Weeks, Unit::Days),
            Ok(Value::Known(dec("7")))
        );
        assert_eq!(
            convert(dec("3.5"), Unit::Weeks, Unit::Days),
            Ok(Value::Known(dec("24.5")))
        );
        assert_eq!(
            convert(dec("14"), Unit::Days, Unit::Weeks),
            Ok(Value::Known(dec("2")))
        );
        assert_eq!(
            convert(dec("2"), Unit::Years, Unit::Months),
            Ok(Value::Known(dec("24")))
        );
    }

    #[test]
    fn per_100_converts_to_each() {
        assert_eq!(
            convert(dec("900.00"), Unit::Per100, Unit::Each),
            Ok(Value::Known(dec("9.00")))
        );
        assert_eq!(
            convert(dec("9.00"), Unit::Each, Unit::Per100),
            Ok(Value::Known(dec("900.00")))
        );
        assert_eq!(
            convert(dec("120.00"), Unit::PerDozen, Unit::Each),
            Ok(Value::Known(dec("10.00")))
        );
        assert_eq!(
            convert(dec("250.00"), Unit::Per1000, Unit::Each),
            Ok(Value::Known(dec("0.250")))
        );
    }

    #[test]
    fn basis_points_convert_to_percent() {
        assert_eq!(
            convert(dec("250"), Unit::BasisPoints, Unit::Percent),
            Ok(Value::Known(dec("2.5")))
        );
        assert_eq!(
            convert(dec("2.5"), Unit::Percent, Unit::BasisPoints),
            Ok(Value::Known(dec("250")))
        );
    }

    #[test]
    fn conversion_across_families_is_refused() {
        let err = convert(dec("1"), Unit::Days, Unit::Each).unwrap_err();
        assert_eq!(err, CalcError::Mismatch);
        assert_eq!(err.code(), "FB0502");

        assert_eq!(
            convert(dec("1"), Unit::Days, Unit::Months),
            Err(CalcError::Mismatch)
        );
        assert_eq!(
            convert(dec("1"), Unit::Days, Unit::BusinessDays),
            Err(CalcError::Mismatch)
        );
    }

    #[test]
    fn inexact_conversion_is_unknown() {
        assert_eq!(
            convert(dec("1"), Unit::Days, Unit::Weeks),
            Ok(Value::Unknown(Reason::Inexact))
        );
        assert_eq!(
            convert(dec("10"), Unit::Days, Unit::Weeks),
            Ok(Value::Unknown(Reason::Inexact))
        );
        assert_eq!(
            convert(dec("10.00"), Unit::PerDozen, Unit::Each),
            Ok(Value::Unknown(Reason::Inexact))
        );
    }
}
