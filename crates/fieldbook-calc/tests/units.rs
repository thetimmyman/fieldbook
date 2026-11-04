//! Drift test keeping `Unit::ALL` in step with `docs/reference/corpus-format.md`.

mod units {
    use fieldbook_calc::{Currency, Unit};

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
}
