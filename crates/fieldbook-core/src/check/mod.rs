//! The check diagnostic model and check passes (`docs/reference/corpus-format.md#check-catalogue`).
//!
//! **Responsibility.** [`Code`], [`Level`], [`Severity`], [`Diagnostic`], [`Report`], [`Tree`],
//! [`Entry`], [`Host`], [`Options`], [`Model`], [`load`] and [`run`].

pub mod claim_cells;
pub mod code;
pub mod corpus_toml;
pub mod diag;
pub mod files;
pub mod model;
pub mod register_cells;
pub mod registers;
pub mod tree;

pub use code::{Code, CodeParseError, Level};
pub use diag::{Diagnostic, Report, Severity};
pub use model::{Located, Model};
pub use tree::{Entry, Tree};

use crate::date::CivilDate;

/// Input state supplied by the binary or a test harness to [`load`] and [`run`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    /// The in-memory Corpus file and directory tree.
    pub tree: Tree,
    /// Calendar date for freshness and `as_of` checks.
    pub today: CivilDate,
}

impl Host {
    /// Create a new [`Host`] with `tree` and `today`.
    #[must_use]
    pub const fn new(tree: Tree, today: CivilDate) -> Self {
        Self { tree, today }
    }
}

/// Options controlling `check` execution.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    /// When `true`, `Strict` codes (`FB0118`, `FB0230`, `FB0320`) are reported as errors instead of warnings.
    pub strict: bool,
}

impl Options {
    /// Create [`Options`] with `strict`.
    #[must_use]
    pub const fn new(strict: bool) -> Self {
        Self { strict }
    }
}

/// Signature of a check pass over [`Model`].
pub type Pass = fn(&Model, &Options, &mut Report);

/// The fixed list of check passes run after `corpus/corpus.toml` passes `FB0100`.
const PASSES: &[Pass] = &[
    files::check,
    registers::check,
    register_cells::check,
    claim_cells::check,
];

fn load_with_passes(host: &Host, options: &Options, passes: &[Pass]) -> (Model, Report) {
    let mut report = Report::new();
    let corpus = corpus_toml::load_corpus_toml(&host.tree, options.strict, &mut report);
    let has_fb0100 = corpus.is_none();
    let model = Model::new(host.tree.clone(), host.today, corpus);
    if !has_fb0100 {
        for pass in passes {
            pass(&model, options, &mut report);
        }
    }
    report.sort();
    (model, report)
}

/// Build the [`Model`] and run all check passes, returning `(Model, Report)`.
///
/// Raises `FB0100` on `corpus/corpus.toml` when the file is absent, undecodable or malformed TOML,
/// fails semantic validation or has an unsupported `schema_version`, and stops before later passes
/// when `FB0100` fires.
#[must_use]
pub fn load(host: &Host, options: &Options) -> (Model, Report) {
    load_with_passes(host, options, PASSES)
}

/// Run all check passes on `host` with `options` and return the [`Report`].
#[must_use]
pub fn run(host: &Host, options: &Options) -> Report {
    load(host, options).1
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::str::FromStr;
    use std::sync::atomic::{AtomicBool, Ordering};

    use crate::date::CivilDate;

    use super::{
        Code, Diagnostic, Entry, Host, Level, Model, Options, Pass, Report, Severity, Tree, load,
        load_with_passes, run,
    };

    const MINI_CORPUS_TOML: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/corpus.toml");

    fn sample_today() -> CivilDate {
        CivilDate::from_ymd(2026, 3, 1).expect("valid date")
    }

    fn insert_empty_registers(tree: &mut Tree) {
        tree.insert_dir("corpus/registers");
        for reg in crate::schema::Register::ALL {
            let mut header = String::new();
            for (idx, col) in reg.columns().iter().enumerate() {
                if idx > 0 {
                    header.push(',');
                }
                header.push_str(col.name);
            }
            header.push('\n');
            tree.insert_bytes(super::registers::register_path(reg), header.into_bytes());
        }
    }

    fn clean_host() -> Host {
        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes());
        tree.insert_bytes("corpus/ASSISTANT.md", b"rules_version = 1\n");
        insert_empty_registers(&mut tree);
        Host::new(tree, sample_today())
    }

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

    #[test]
    fn empty_directory_is_distinct_from_absent() {
        let mut tree = Tree::new();
        assert!(!tree.contains_dir("corpus/sources/public"));

        tree.insert_dir("corpus/sources/public");
        assert!(tree.contains_dir("corpus"));
        assert!(tree.contains_dir("corpus/sources"));
        assert!(tree.contains_dir("corpus/sources/public"));
        assert!(!tree.contains_dir("corpus/sources/text"));
        assert_eq!(tree.paths().count(), 0);

        let sub = tree.with_prefix("corpus/sources");
        assert!(sub.contains_dir("corpus/sources"));
        assert!(sub.contains_dir("corpus/sources/public"));
        assert!(!sub.contains_dir("corpus"));
        assert_eq!(sub.paths().count(), 0);
    }

    #[test]
    fn clean_corpus_toml() {
        let host = clean_host();
        let (model, report) = load(&host, &Options::default());
        assert!(report.diagnostics.is_empty());
        assert_eq!(report.errors(), 0);
        assert_eq!(report.warnings(), 0);
        assert!(model.corpus().is_some());
    }

    #[test]
    fn missing_file_is_fb0100() {
        let host = Host::new(Tree::new(), sample_today());
        let (model, report) = load(&host, &Options::default());
        assert!(model.corpus().is_none());
        assert_eq!(report.diagnostics.len(), 1);
        let d = &report.diagnostics[0];
        assert_eq!(d.code, Code::Fb0100);
        assert_eq!(d.path, "corpus/corpus.toml");
        assert_eq!(d.line, 1);
        assert_eq!(d.col, 1);
    }

    #[test]
    fn syntax_error_has_line_and_column() {
        let mut tree = Tree::new();
        tree.insert_bytes(
            "corpus/corpus.toml",
            b"schema_version = 1\n\n[assistant]\npack_budget_chars = [\n",
        );
        let host = Host::new(tree, sample_today());
        let (_, report) = load(&host, &Options::default());
        assert_eq!(report.diagnostics.len(), 1);
        let d = &report.diagnostics[0];
        assert_eq!(d.code, Code::Fb0100);
        assert_eq!(d.path, "corpus/corpus.toml");
        assert_eq!(d.line, 4);
        assert!(d.col >= 20, "expected column >= 20, got {}", d.col);
    }

    #[test]
    fn unknown_key_in_an_area() {
        let bad = format!(
            "{MINI_CORPUS_TOML}\n[[area]]\ncode = \"fnd\"\nslug = \"foundation\"\nname = \"Foundation\"\nsummary = \"Summary\"\nsubareas = [\"rules\"]\nextra = 1\n"
        );
        let mut tree = Tree::new();
        tree.insert_bytes("corpus/corpus.toml", bad.into_bytes());
        let host = Host::new(tree, sample_today());
        let (model, report) = load(&host, &Options::default());
        assert!(model.corpus().is_none());
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].code, Code::Fb0100);
        assert!(report.diagnostics[0].message.contains("extra"));
    }

    #[test]
    fn repeated_area_code() {
        let bad = MINI_CORPUS_TOML.replace("code = \"est\"", "code = \"cmp\"");
        let mut tree = Tree::new();
        tree.insert_bytes("corpus/corpus.toml", bad.into_bytes());
        let host = Host::new(tree, sample_today());
        let (model, report) = load(&host, &Options::default());
        assert!(model.corpus().is_none());
        assert!(!report.diagnostics.is_empty());
        assert!(report.diagnostics.iter().all(|d| d.code == Code::Fb0100));
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.message.contains("duplicate area code"))
        );
    }

    #[test]
    fn schema_version_above_the_supported() {
        let bad = MINI_CORPUS_TOML.replace("schema_version = 1", "schema_version = 9");
        let mut tree = Tree::new();
        tree.insert_bytes("corpus/corpus.toml", bad.into_bytes());
        let host = Host::new(tree, sample_today());
        let (model, report) = load(&host, &Options::default());
        assert!(model.corpus().is_none());
        assert_eq!(report.diagnostics.len(), 1);
        let d = &report.diagnostics[0];
        assert_eq!(d.code, Code::Fb0100);
        assert_eq!(d.line, 1);
        assert_eq!(d.col, 1);
        assert!(
            d.message.contains("this tool reads schema 1"),
            "unexpected message: {}",
            d.message
        );
    }

    #[test]
    fn two_runs_give_equal_reports() {
        let host = clean_host();
        let r1 = run(&host, &Options::default());
        let r2 = run(&host, &Options::default());
        assert_eq!(r1, r2);
    }

    #[test]
    fn run_equals_load_report() {
        let host = clean_host();
        let (_, from_load) = load(&host, &Options::default());
        let from_run = run(&host, &Options::default());
        assert_eq!(from_load, from_run);
    }

    #[test]
    fn the_model_returns_its_tree_and_today() {
        let mut tree = Tree::new();
        tree.insert_dir("corpus/sources/public");
        tree.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes());
        tree.insert_bytes("corpus/ASSISTANT.md", b"rules_version = 1\n");
        insert_empty_registers(&mut tree);
        tree.insert(
            "corpus/sources/public/S-0001-v1.txt",
            Entry::Original {
                sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                    .to_owned(),
                len: 0,
            },
        );
        let date = CivilDate::from_ymd(2027, 8, 19).expect("valid date");
        let host = Host::new(tree.clone(), date);
        let (model, report) = load(&host, &Options::default());
        assert!(report.diagnostics.is_empty());
        assert_eq!(model.tree(), &tree);
        assert_eq!(model.today(), date);
    }

    #[test]
    fn a_broken_corpus_toml_stops_the_passes() {
        static RAN: AtomicBool = AtomicBool::new(false);
        fn record_pass(_: &Model, _: &Options, _: &mut Report) {
            RAN.store(true, Ordering::SeqCst);
        }

        RAN.store(false, Ordering::SeqCst);
        let broken_host = Host::new(Tree::new(), sample_today());
        let passes: &[Pass] = &[record_pass];
        let (_, report) = load_with_passes(&broken_host, &Options::default(), passes);
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].code, Code::Fb0100);
        assert!(
            !RAN.load(Ordering::SeqCst),
            "passes must not run when FB0100 fires"
        );
    }

    #[test]
    fn pass_notes_survive_load_and_run() {
        fn note_pass(_: &Model, _: &Options, report: &mut Report) {
            report.push_note("originals check skipped (no corpus/sources/public/)");
        }

        let host = clean_host();
        let passes: &[Pass] = &[note_pass];
        let (_, load_report) = load_with_passes(&host, &Options::default(), passes);
        let run_report = load_with_passes(&host, &Options::default(), passes).1;

        assert_eq!(
            load_report.notes,
            vec!["originals check skipped (no corpus/sources/public/)".to_owned()]
        );
        assert_eq!(load_report.notes, run_report.notes);
        assert_eq!(load_report.diagnostics.len(), 0);
        assert_eq!(load_report.errors(), 0);
        assert_eq!(load_report.warnings(), 0);

        let rendered = load_report.render_text();
        assert_eq!(
            rendered,
            "note: originals check skipped (no corpus/sources/public/)\nclean\n"
        );
    }
}
