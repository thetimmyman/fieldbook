//! The check diagnostic model and check passes (`docs/reference/corpus-format.md#check-catalogue`).
//!
//! **Responsibility.** [`Code`], [`Level`], [`Severity`], [`Diagnostic`] and [`Report`].

pub mod code;
pub mod diag;

pub use code::{Code, CodeParseError, Level};
pub use diag::{Diagnostic, Report, Severity};

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::str::FromStr;

    use super::{Code, Diagnostic, Level, Report, Severity};

    #[test]
    fn severity_follows_the_level() {
        assert_eq!(Code::Fb0230.level(), Level::Strict);
        assert_eq!(Code::Fb0230.severity(false), Severity::Warning);
        assert_eq!(Code::Fb0230.severity(true), Severity::Error);

        assert_eq!(Code::Fb0118.level(), Level::Strict);
        assert_eq!(Code::Fb0118.severity(false), Severity::Warning);
        assert_eq!(Code::Fb0118.severity(true), Severity::Error);

        assert_eq!(Code::Fb0320.level(), Level::Strict);
        assert_eq!(Code::Fb0320.severity(false), Severity::Warning);
        assert_eq!(Code::Fb0320.severity(true), Severity::Error);

        assert_eq!(Code::Fb0382.level(), Level::Warning);
        assert_eq!(Code::Fb0382.severity(false), Severity::Warning);
        assert_eq!(Code::Fb0382.severity(true), Severity::Warning);

        assert_eq!(Code::Fb0607.level(), Level::Warning);
        assert_eq!(Code::Fb0607.severity(false), Severity::Warning);
        assert_eq!(Code::Fb0607.severity(true), Severity::Warning);

        assert_eq!(Code::Fb0308.level(), Level::Warning);
        assert_eq!(Code::Fb0308.severity(false), Severity::Warning);
        assert_eq!(Code::Fb0308.severity(true), Severity::Warning);

        assert_eq!(Code::Fb0380.level(), Level::Error);
        assert_eq!(Code::Fb0380.severity(false), Severity::Error);
        assert_eq!(Code::Fb0380.severity(true), Severity::Error);

        assert_eq!(Code::Fb0381.level(), Level::Error);
        assert_eq!(Code::Fb0381.severity(false), Severity::Error);
        assert_eq!(Code::Fb0381.severity(true), Severity::Error);
    }

    #[test]
    fn ids_parse_and_print_back() {
        let mut seen = BTreeSet::new();
        for &code in Code::ALL {
            let id = code.id();
            assert!(seen.insert(id), "duplicate code in Code::ALL: {id}");
            assert_eq!(code.to_string(), id);
            assert_eq!(Code::parse(id), Ok(code));
            assert_eq!(Code::from_str(id), Ok(code));
            assert!(!code.help().is_empty(), "empty help on {id}");
        }
        assert!(Code::parse("FB0824").is_err());
        assert!(Code::parse("FB0927").is_err());
        assert!(Code::parse("FB0000").is_err());
    }

    #[test]
    fn rendered_diagnostic_and_report_match_expected_text() {
        let non_ascii_line = "§ 15.404-1 — café rate: bad";
        let byte_idx = non_ascii_line.find("bad").expect("bad in line");
        let col = Diagnostic::col_at_byte(non_ascii_line, byte_idx);
        assert_eq!(col, 25);

        let d1 = Diagnostic::new(
            Code::Fb0102,
            false,
            "corpus/pages/compliance/far/cmp-sample.md",
            4,
            col,
            "unknown field `titel`",
        );
        let expected_d1 = format!(
            "corpus/pages/compliance/far/cmp-sample.md:4:25: error[FB0102]: unknown field `titel`\nhelp: {}",
            Code::Fb0102.help()
        );
        assert_eq!(d1.render_text(), expected_d1);

        let d2 = Diagnostic::new(
            Code::Fb0230,
            false,
            "corpus/registers/questions.csv",
            2,
            1,
            "day-one question Q-001 has empty `default_assumption`",
        );

        let mut report = Report::new();
        assert_eq!(report.render_text(), "clean\n");

        // Push in reverse path order to verify sorting by (path, line, col, code).
        report.push(d2);
        report.push(d1);
        report.push_note("originals check skipped (no corpus/sources/public/)");

        assert_eq!(report.errors(), 1);
        assert_eq!(report.warnings(), 1);

        let expected_report = format!(
            "corpus/pages/compliance/far/cmp-sample.md:4:25: error[FB0102]: unknown field `titel`\nhelp: {}\n\ncorpus/registers/questions.csv:2:1: warning[FB0230]: day-one question Q-001 has empty `default_assumption`\nhelp: {}\nnote: originals check skipped (no corpus/sources/public/)\n1 errors, 1 warnings\n",
            Code::Fb0102.help(),
            Code::Fb0230.help()
        );
        assert_eq!(report.render_text(), expected_report);
    }
}
