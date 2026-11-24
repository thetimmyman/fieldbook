//! Rule-file parsing, the `rules@1` classifier, the source-kind/tier table, and the FB0826 check
//! pass (`pipeline-rules-classify`).

#![expect(clippy::unwrap_used, reason = "integration test helpers")]
#![expect(
    clippy::disallowed_methods,
    reason = "integration tests load repository fixtures"
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use fieldbook_core::check::{self, Code, Host, Options, Report, Tree};
use fieldbook_core::date::CivilDate;
use fieldbook_core::extraction::{Extraction, PageText};
use fieldbook_core::hash::sha256_hex;
use fieldbook_core::labels::LabelSet;
use fieldbook_core::pipeline::rules::{Rules, RulesFile, tier_allowed};
use fieldbook_core::pipeline::{
    Candidate, Classifier, Proposal, ProposalBody, RuleKind, RuleRef, SuggestField, Support, Vocab,
};
use fieldbook_core::schema::{AuthorityTier, Register, SourceKind};

#[path = "../../../tests/support/rng.rs"]
mod rng;

const MINI_CORPUS_TOML: &str =
    include_str!("../../../tests/fixtures/corpus-mini/corpus/corpus.toml");
const MINI_RULES: &str =
    include_str!("../../../tests/fixtures/corpus-mini/corpus/pipeline/rules.toml");

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

fn today() -> CivilDate {
    CivilDate::from_ymd(2026, 3, 1).unwrap()
}

fn vocab() -> Vocab {
    Vocab::from_schema()
}

fn rules_from(text: &str) -> Rules {
    Rules::new(RulesFile::parse(text, &vocab()).unwrap())
}

fn candidate_with_pages(pages: Vec<&str>) -> Candidate {
    let joined = pages.concat();
    Candidate {
        file: "candidate.txt".to_owned(),
        sha256: sha256_hex(joined.as_bytes()),
        bytes: joined.len() as u64,
        ext: "txt".to_owned(),
        labels: LabelSet::public(),
        origin: None,
        extraction: Some(Extraction {
            method: "plain",
            pages: pages
                .into_iter()
                .enumerate()
                .map(|(idx, text)| PageText {
                    number: u32::try_from(idx + 1).unwrap_or(u32::MAX),
                    text: text.to_owned(),
                })
                .collect(),
        }),
    }
}

fn insert_register_headers(tree: &mut Tree) {
    tree.insert_dir("corpus/registers");
    for reg in Register::ALL {
        let mut header = String::new();
        for (idx, col) in reg.columns().iter().enumerate() {
            if idx > 0 {
                header.push(',');
            }
            header.push_str(col.name);
        }
        header.push('\n');
        tree.insert_bytes(format!("corpus/registers/{}", reg.file_name()), header);
    }
}

fn minimal_tree_with_rules(rules_text: &str) -> Tree {
    let mut tree = Tree::new();
    tree.insert_dir("corpus");
    tree.insert_dir("corpus/pipeline");
    tree.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML);
    tree.insert_bytes("corpus/ASSISTANT.md", b"rules_version = 1\n".to_vec());
    insert_register_headers(&mut tree);
    tree.insert_bytes("corpus/pipeline/rules.toml", rules_text);
    tree
}

fn run_check_with_rules(rules_text: &str) -> Report {
    check::run(
        &Host::new(minimal_tree_with_rules(rules_text), today()),
        &Options::default(),
    )
}

fn fb0826(report: &Report) -> Vec<&fieldbook_core::check::Diagnostic> {
    report
        .diagnostics
        .iter()
        .filter(|d| d.code == Code::Fb0826)
        .collect()
}

#[test]
fn rules_file_parses_with_positions() {
    let rules = RulesFile::parse(MINI_RULES, &vocab()).unwrap();
    assert_eq!(rules.rules.len(), 3);
    assert_eq!(rules.rules[0].id, "reg-text");
    assert_eq!(rules.rules[0].line, 1);
    assert_eq!(rules.rules[1].id, "fixture-agency");
    assert_eq!(rules.rules[1].line, 8);
    assert_eq!(rules.rules[2].id, "fixture-licence");
    assert_eq!(rules.rules[2].line, 15);
}

#[test]
fn unknown_key_is_fb0826() {
    let bad = "[[rule]]\nid = \"a\"\nwhen = { phrase = \"x\" }\nset = { field = \"source_kind\", value = \"primary_law\" }\nexample = \"x\"\nexpect = { field = \"source_kind\", value = \"primary_law\" }\nflavour = \"x\"\n";
    let report = run_check_with_rules(bad);
    let found = fb0826(&report);
    assert_eq!(found.len(), 1, "{:?}", report.diagnostics);
    assert!(found[0].message.contains("flavour"), "{}", found[0].message);
}

#[test]
fn duplicate_id_is_fb0826() {
    let bad = "[[rule]]\nid = \"a\"\nwhen = { phrase = \"x\" }\nset = { field = \"source_kind\", value = \"primary_law\" }\nexample = \"x\"\nexpect = { field = \"source_kind\", value = \"primary_law\" }\n\n[[rule]]\nid = \"a\"\nwhen = { phrase = \"y\" }\nset = { field = \"publisher\", value = \"Y\" }\nexample = \"y\"\nexpect = { field = \"publisher\", value = \"Y\" }\n";
    let report = run_check_with_rules(bad);
    let found = fb0826(&report);
    assert_eq!(found.len(), 1, "{:?}", report.diagnostics);
    assert!(
        found[0].message.contains("duplicate"),
        "{}",
        found[0].message
    );
}

#[test]
fn rule_without_example_is_fb0826() {
    let bad = "[[rule]]\nid = \"a\"\nwhen = { phrase = \"x\" }\nset = { field = \"source_kind\", value = \"primary_law\" }\nexpect = { field = \"source_kind\", value = \"primary_law\" }\n";
    let report = run_check_with_rules(bad);
    let found = fb0826(&report);
    assert_eq!(found.len(), 1, "{:?}", report.diagnostics);
    assert!(found[0].message.contains("example"), "{}", found[0].message);
    assert!(found[0].message.contains('a'), "{}", found[0].message);
}

#[test]
fn rule_whose_example_does_not_match_is_fb0826() {
    let bad = "[[rule]]\nid = \"a\"\nwhen = { phrase = \"Missing Phrase\" }\nset = { field = \"source_kind\", value = \"primary_law\" }\nexample = \"No such phrase here.\"\nexpect = { field = \"source_kind\", value = \"primary_law\" }\n";
    let report = run_check_with_rules(bad);
    let found = fb0826(&report);
    assert_eq!(found.len(), 1, "{:?}", report.diagnostics);
    assert!(found[0].message.contains('a'), "{}", found[0].message);
    assert!(
        found[0].message.contains("does not reproduce"),
        "{}",
        found[0].message
    );
}

#[test]
fn value_outside_the_vocabulary_is_fb0826() {
    let bad = "[[rule]]\nid = \"a\"\nwhen = { phrase = \"x\" }\nset = { field = \"source_kind\", value = \"not_a_kind\" }\nexample = \"x\"\nexpect = { field = \"source_kind\", value = \"primary_law\" }\n";
    let report = run_check_with_rules(bad);
    let found = fb0826(&report);
    assert_eq!(found.len(), 1, "{:?}", report.diagnostics);
    assert!(found[0].message.contains("outside"), "{}", found[0].message);
}

#[test]
fn reserved_field_in_set_is_refused() {
    let bad = "[[rule]]\nid = \"a\"\nwhen = { phrase = \"x\" }\nset = { field = \"lifecycle\", value = \"active\" }\nexample = \"x\"\nexpect = { field = \"source_kind\", value = \"primary_law\" }\n";
    let report = run_check_with_rules(bad);
    let found = fb0826(&report);
    assert_eq!(found.len(), 1, "{:?}", report.diagnostics);
    assert!(
        found[0].message.contains("lifecycle"),
        "{}",
        found[0].message
    );
}

#[test]
fn doc_kind_in_set_is_refused_for_the_corpus() {
    let bad = "[[rule]]\nid = \"a\"\nwhen = { phrase = \"x\" }\nset = { field = \"doc_kind\", value = \"contract\" }\nexample = \"x\"\nexpect = { field = \"doc_kind\", value = \"contract\" }\n";
    let report = run_check_with_rules(bad);
    let found = fb0826(&report);
    assert_eq!(found.len(), 1, "{:?}", report.diagnostics);
    assert!(
        found[0].message.contains("doc_kind"),
        "{}",
        found[0].message
    );
}

#[test]
fn first_matching_rule_wins_per_field() {
    let text = "[[rule]]\nid = \"kind-a\"\nwhen = { phrase = \"Alpha\" }\nset = { field = \"source_kind\", value = \"primary_law\" }\nexample = \"Alpha\"\nexpect = { field = \"source_kind\", value = \"primary_law\" }\n\n[[rule]]\nid = \"kind-b\"\nwhen = { phrase = \"Beta\" }\nset = { field = \"source_kind\", value = \"trade_press\" }\nexample = \"Beta\"\nexpect = { field = \"source_kind\", value = \"trade_press\" }\n";
    let classifier = rules_from(text);
    let cand = candidate_with_pages(vec!["Alpha and Beta"]);
    let suggestions = classifier.classify(&cand, &vocab()).unwrap();
    let kinds: Vec<&str> = suggestions
        .fields
        .iter()
        .filter(|s| s.field == SuggestField::SourceKind)
        .map(|s| s.value.as_str())
        .collect();
    assert_eq!(kinds, vec!["primary_law"]);
}

#[test]
fn quote_is_text_on_the_page() {
    let text = "[[rule]]\nid = \"reg\"\nwhen = { phrase = \"Synthetic Regulation Text\" }\nset = { field = \"source_kind\", value = \"primary_law\" }\nexample = \"Synthetic Regulation Text, part one.\"\nexpect = { field = \"source_kind\", value = \"primary_law\", quote = \"Synthetic Regulation Text\" }\n";
    let classifier = rules_from(text);
    let cand = candidate_with_pages(vec!["Synthetic Regulation Text, part one."]);
    let suggestions = classifier.classify(&cand, &vocab()).unwrap();
    let s = suggestions
        .fields
        .iter()
        .find(|s| s.field == SuggestField::SourceKind)
        .unwrap();
    assert_eq!(
        s.support,
        vec![Support::Quote {
            page: 1,
            quote: "Synthetic Regulation Text".to_owned()
        }]
    );
}

#[test]
fn tier_the_kind_does_not_allow_is_dropped() {
    let text = "[[rule]]\nid = \"kind\"\nwhen = { phrase = \"Fixture\" }\nset = { field = \"source_kind\", value = \"synthetic_fixture\" }\nexample = \"Fixture\"\nexpect = { field = \"source_kind\", value = \"synthetic_fixture\" }\n\n[[rule]]\nid = \"tier\"\nwhen = { phrase = \"Fixture\" }\nset = { field = \"authority_tier\", value = \"D\" }\nexample = \"Fixture\"\nexpect = { field = \"authority_tier\", value = \"D\" }\n";
    let classifier = rules_from(text);
    let cand = candidate_with_pages(vec!["Fixture"]);
    let suggestions = classifier.classify(&cand, &vocab()).unwrap();
    assert!(
        suggestions
            .fields
            .iter()
            .any(|s| s.field == SuggestField::SourceKind && s.value == "synthetic_fixture")
    );
    assert!(
        !suggestions
            .fields
            .iter()
            .any(|s| s.field == SuggestField::AuthorityTier)
    );
}

#[test]
fn title_falls_back_to_the_first_line() {
    let text = "[[rule]]\nid = \"kind\"\nwhen = { phrase = \"Primary Law\" }\nset = { field = \"source_kind\", value = \"primary_law\" }\nexample = \"Primary Law\"\nexpect = { field = \"source_kind\", value = \"primary_law\" }\n";
    let classifier = rules_from(text);
    let cand = candidate_with_pages(vec!["Primary Law\nsecond line"]);
    let suggestions = classifier.classify(&cand, &vocab()).unwrap();
    let title = suggestions
        .fields
        .iter()
        .find(|s| s.field == SuggestField::Title)
        .unwrap();
    assert_eq!(title.value, "Primary Law");
    assert!(title.rule.is_none());
}

#[test]
fn no_rules_file_gives_only_a_title() {
    let classifier = Rules::new(RulesFile::default());
    let cand = candidate_with_pages(vec!["Only a title"]);
    let suggestions = classifier.classify(&cand, &vocab()).unwrap();
    assert_eq!(suggestions.fields.len(), 1);
    assert_eq!(suggestions.fields[0].field, SuggestField::Title);
    assert_eq!(suggestions.fields[0].value, "Only a title");
    assert!(suggestions.fields[0].rule.is_none());
}

#[test]
fn same_input_same_output() {
    let classifier = rules_from(MINI_RULES);
    let cand = candidate_with_pages(vec![
        "Synthetic Regulation Text from the Synthetic Standards Board under CC0-1.0.",
    ]);
    let a = classifier.classify(&cand, &vocab()).unwrap();
    let b = classifier.classify(&cand, &vocab()).unwrap();
    assert_eq!(a, b);
}

#[test]
fn layout_allows_the_rules_file_and_nothing_else_under_pipeline() {
    let report = run_check_with_rules(MINI_RULES);
    assert!(
        !report.diagnostics.iter().any(|d| d.code == Code::Fb0121),
        "{:?}",
        report.diagnostics
    );

    let mut tree = minimal_tree_with_rules(MINI_RULES);
    tree.insert_bytes("corpus/pipeline/extra.toml", b"[[rule]]\n".to_vec());
    let report = check::run(&Host::new(tree, today()), &Options::default());
    let fb0121: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.code == Code::Fb0121 && d.path == "corpus/pipeline/extra.toml")
        .collect();
    assert_eq!(fb0121.len(), 1, "{:?}", report.diagnostics);
}

#[test]
fn rule_provenance_is_emitted_with_distinct_ids() {
    let text = "[[rule]]\nid = \"phrase-rule\"\nwhen = { phrase = \"Synthetic Regulation Text\" }\nset = { field = \"source_kind\", value = \"primary_law\" }\nexample = \"Synthetic Regulation Text\"\nexpect = { field = \"source_kind\", value = \"primary_law\" }\n\n[[rule]]\nid = \"publisher-rule\"\nwhen = { publisher = \"Synthetic Standards Board\" }\nset = { field = \"publisher\", value = \"Synthetic Standards Board\" }\nexample = \"Synthetic Standards Board\"\nexpect = { field = \"publisher\", value = \"Synthetic Standards Board\" }\n\n[[rule]]\nid = \"shape-rule\"\nwhen = { id_shape = \"clause_number\" }\nset = { field = \"purpose\", value = \"Clause number found\" }\nexample = \"See 15.404-1 for details\"\nexpect = { field = \"purpose\", value = \"Clause number found\", quote = \"15.404-1\" }\n";
    let classifier = rules_from(text);
    let cand = candidate_with_pages(vec![
        "Synthetic Regulation Text by the Synthetic Standards Board, see 15.404-1.",
    ]);
    let suggestions = classifier.classify(&cand, &vocab()).unwrap();

    let rule_of = |field: SuggestField| {
        suggestions
            .fields
            .iter()
            .find(|s| s.field == field)
            .unwrap()
            .rule
            .clone()
    };

    assert_eq!(
        rule_of(SuggestField::SourceKind),
        Some(RuleRef {
            id: "phrase-rule".to_owned(),
            kind: RuleKind::Phrase
        })
    );
    assert_eq!(
        rule_of(SuggestField::Publisher),
        Some(RuleRef {
            id: "publisher-rule".to_owned(),
            kind: RuleKind::Publisher
        })
    );
    assert_eq!(
        rule_of(SuggestField::Purpose),
        Some(RuleRef {
            id: "shape-rule".to_owned(),
            kind: RuleKind::IdShape
        })
    );

    let purpose = suggestions
        .fields
        .iter()
        .find(|s| s.field == SuggestField::Purpose)
        .unwrap();
    assert!(
        purpose
            .support
            .iter()
            .any(|sup| matches!(sup, Support::Quote { quote, .. } if quote == "15.404-1")),
        "{:?}",
        purpose.support
    );

    let ids: Vec<&str> = suggestions
        .fields
        .iter()
        .filter_map(|s| s.rule.as_ref().map(|r| r.id.as_str()))
        .collect();
    assert_eq!(ids, vec!["phrase-rule", "publisher-rule", "shape-rule"]);
}

#[test]
fn rule_ref_survives_the_proposal_round_trip() {
    let raw = "proposal_version = 1\nstate = \"proposed\"\nkind = \"new_source\"\nlabels = [\"public\"]\nwriter = 0\n\n[candidate]\nfile = \"notice.txt\"\nsha256 = \"89418a780df335fdd9b07e3e24271f8485ac83bc488a562d582ea13e9f88fc2a\"\nbytes = 285\n\n[row.purpose]\nvalue = \"Clause number found\"\nby = \"rules@1\"\nbecause = 'p.1: \"15.404-1\"'\nrule = { id = \"shape-rule\", kind = \"id_shape\" }\n";
    let proposal = Proposal::parse("P-0100", raw).unwrap();
    assert_eq!(proposal.render(), raw);
    let ProposalBody::Row(ref row) = proposal.body else {
        panic!("expected RowBody");
    };
    assert_eq!(
        row[&SuggestField::Purpose].rule,
        Some(RuleRef {
            id: "shape-rule".to_owned(),
            kind: RuleKind::IdShape
        })
    );
}

#[test]
fn tier_allowed_matches_register_validation_for_every_row() {
    for kind in SourceKind::ALL {
        for tier in AuthorityTier::ALL {
            assert_eq!(
                tier_allowed(kind, tier),
                reference_tier_allowed(kind, tier),
                "{kind:?} {tier:?}"
            );
        }
    }
    // The three rows the card names directly.
    assert!(tier_allowed(SourceKind::TradePress, AuthorityTier::D));
    assert!(!tier_allowed(
        SourceKind::SyntheticFixture,
        AuthorityTier::D
    ));
    assert!(!tier_allowed(SourceKind::CompanyRecord, AuthorityTier::D));
}

fn reference_tier_allowed(kind: SourceKind, tier: AuthorityTier) -> bool {
    match kind {
        SourceKind::PrimaryLaw
        | SourceKind::OfficialData
        | SourceKind::CompanyFiling
        | SourceKind::OfficialAnnouncement => {
            matches!(tier, AuthorityTier::A | AuthorityTier::D)
        }
        SourceKind::StandardOrGuidance => {
            matches!(tier, AuthorityTier::A | AuthorityTier::B | AuthorityTier::D)
        }
        SourceKind::CompanyStatement => {
            matches!(tier, AuthorityTier::B | AuthorityTier::C | AuthorityTier::D)
        }
        SourceKind::LicensedDatabase | SourceKind::VendorDocumentation => {
            matches!(tier, AuthorityTier::B | AuthorityTier::D)
        }
        SourceKind::StrategicDataset | SourceKind::TradePress => {
            matches!(tier, AuthorityTier::C | AuthorityTier::D)
        }
        SourceKind::SyntheticFixture
        | SourceKind::VendorQuote
        | SourceKind::CompanyRecord
        | SourceKind::CompanyDocument => false,
    }
}

#[test]
fn fault_fixture_raises_fb0826() {
    let root = repo_root();
    let fault_dir = root.join("tests/fixtures/corpus-rule-fault/corpus");
    let mut tree = Tree::new();
    insert_dir_recursive(&mut tree, &fault_dir, "corpus");
    let report = check::run(&Host::new(tree, today()), &Options::default());
    let found = fb0826(&report);
    assert_eq!(found.len(), 1, "{:?}", report.diagnostics);
    assert_eq!(found[0].path, "corpus/pipeline/rules.toml");
    assert!(found[0].message.contains("fault-no-example"));
}

#[test]
fn fault_fixture_differs_from_corpus_mini_only_in_the_rules_file() {
    let root = repo_root();
    let mini = collect_fixture_files(&root.join("tests/fixtures/corpus-mini"));
    let fault = collect_fixture_files(&root.join("tests/fixtures/corpus-rule-fault"));
    assert_eq!(
        mini.keys().collect::<Vec<_>>(),
        fault.keys().collect::<Vec<_>>()
    );
    for (path, mini_bytes) in &mini {
        let fault_bytes = fault.get(path).unwrap();
        if path == "corpus/pipeline/rules.toml" {
            assert_ne!(mini_bytes, fault_bytes, "{path} must differ");
        } else {
            assert_eq!(mini_bytes, fault_bytes, "{path} must be identical");
        }
    }
}

#[test]
fn random_and_mutated_bytes_never_panic() {
    let fixture_rules = rules_from(MINI_RULES);
    let mut rng = rng::Rng::new(1);

    for idx in 0..200 {
        let len = match idx {
            0 => 0,
            1 => 512,
            _ => usize::try_from(rng.next_u64() % 513).unwrap_or(0),
        };
        let bytes = rng.bytes(len);
        let text = String::from_utf8_lossy(&bytes);

        match RulesFile::parse(&text, &vocab()) {
            Ok(_) => {}
            Err(errors) => {
                for err in &errors {
                    assert!(err.line >= 1, "line must be at least 1: {err:?}");
                }
            }
        }

        let cand = candidate_with_pages(vec![&text]);
        let suggestions = fixture_rules.classify(&cand, &vocab()).unwrap();
        for s in &suggestions.fields {
            assert!(
                fieldbook_core::pipeline::validate(s, &cand, &vocab()).is_ok(),
                "random candidate suggestion must validate"
            );
        }
    }

    let orig =
        std::fs::read(repo_root().join("tests/fixtures/corpus-mini/corpus/pipeline/rules.toml"))
            .unwrap();
    for pos in 0..orig.len() {
        let mut flipped = orig.clone();
        flipped[pos] ^= 0x01;
        let _ = RulesFile::parse(&String::from_utf8_lossy(&flipped), &vocab());

        let mut deleted = orig.clone();
        deleted.remove(pos);
        let _ = RulesFile::parse(&String::from_utf8_lossy(&deleted), &vocab());
    }
}

fn insert_dir_recursive(tree: &mut Tree, dir: &Path, prefix: &str) {
    tree.insert_dir(prefix);
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = format!("{prefix}/{name}");
        let path = entry.path();
        if path.is_dir() {
            insert_dir_recursive(tree, &path, &rel);
        } else {
            let bytes = std::fs::read(&path).unwrap();
            if rel.starts_with("corpus/sources/public/") {
                tree.insert(
                    &rel,
                    fieldbook_core::check::Entry::Original {
                        len: bytes.len() as u64,
                        sha256: sha256_hex(&bytes),
                    },
                );
            } else {
                tree.insert_bytes(&rel, bytes);
            }
        }
    }
}

fn collect_fixture_files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    collect_files_recursive(root, root, &mut out);
    out
}

fn collect_files_recursive(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_files_recursive(root, &path, out);
        } else {
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.insert(rel, std::fs::read(&path).unwrap());
        }
    }
}
