#![expect(
    clippy::disallowed_methods,
    reason = "chip tests prepare temporary fixture copies for add chip execution"
)]

#[path = "../../../tests/support/chip_cases.rs"]
mod chip_cases;
mod support;

use std::collections::BTreeSet;
use std::path::Path;

use clap::{Command, CommandFactory, Parser};
use fieldbook::{Cli, World};
use fieldbook_core::strings;

use chip_cases::{CASES, ChipCase, ChipContext, ChipOutcome, substitute_chip, terminal_only};
use support::temp_copy;
use support::world::TestWorld;

const NO_CHIP: &[&str] = &[
    "audit export",
    "queue list",
    "queue show",
    "queue ingest",
    "queue pin",
    "ai test",
    "policy check",
    "label add",
    "label reapply",
    "label show",
    "mcp",
    "chat",
];

const PENDING: &str = r#"
fieldbook new page --confirm-public <area>-<slug> --type <type> --subarea <subarea>
fieldbook new question --confirm-public --area <a> "<text>"
fieldbook new claim --confirm-public --area <a> --source <version-id> --at "<locator>" --quote "<text>"
fieldbook new conflict --confirm-public --area <a>
fieldbook new term --confirm-public --area <a>
fieldbook fmt
fieldbook search <query>
fieldbook status
fieldbook status --page
fieldbook verify <claim-id> --on YYYY-MM-DD
fieldbook backup
fieldbook backup verify
fieldbook backup list
fieldbook restore --snapshot <n> --drill
fieldbook context "<question>"
fieldbook cite-check <file>
fieldbook new note --ai-drafted --confirm-public --area <a> --from <file>
fieldbook add <folder> --propose
fieldbook queue review
fieldbook queue accept <proposal-id>
fieldbook queue reject <proposal-id> --reason <word>
fieldbook queue flag
fieldbook ask "<question>"
fieldbook ai status
fieldbook policy show
fieldbook decide <file>
fieldbook build --overlay
fieldbook sync --dry-run
"#;

fn pending_set() -> BTreeSet<&'static str> {
    PENDING
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect()
}

fn collect_leaf_verbs(cmd: &Command, prefix: &str, out: &mut Vec<String>) {
    for sub in cmd.get_subcommands() {
        let name = sub.get_name();
        if name == "help" {
            continue;
        }
        let path = if prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{prefix} {name}")
        };
        if sub.is_subcommand_required_set() || sub.has_subcommands() {
            collect_leaf_verbs(sub, &path, out);
        } else {
            out.push(path);
        }
    }
}

fn verb_has_chip_in_map(verb: &str) -> bool {
    let prefix = format!("{} ", strings::binary());
    strings::chips().iter().any(|row| {
        row.chip.strip_prefix(&prefix).is_some_and(|rest| {
            rest == verb
                || rest
                    .strip_prefix(verb)
                    .is_some_and(|tail| tail.starts_with(' '))
        })
    })
}

fn check_every_verb_has_a_chip(cmd: &Command) -> Result<(), String> {
    let mut leaves = Vec::new();
    collect_leaf_verbs(cmd, "", &mut leaves);
    for leaf in leaves {
        if !NO_CHIP.contains(&leaf.as_str()) && !verb_has_chip_in_map(&leaf) {
            return Err(format!(
                "clap leaf subcommand `{leaf}` has no chip in strings::chips and is not in NO_CHIP"
            ));
        }
    }
    Ok(())
}

fn check_chips_parse(pending: &BTreeSet<&str>) -> Result<(), String> {
    let known: BTreeSet<&str> = strings::chips().iter().map(|r| r.chip).collect();
    for &p in pending {
        if !known.contains(p) {
            return Err(format!("PENDING entry `{p}` is not in strings::chips()"));
        }
    }
    for row in strings::chips() {
        let argv = substitute_chip(row.chip, &[]);
        for token in &argv {
            if (token.contains('<') && token.contains('>')) || token.contains("YYYY-MM-DD") {
                return Err(format!(
                    "unfilled placeholder `{token}` in chip `{}`",
                    row.chip
                ));
            }
        }
        let parsed = Cli::try_parse_from(&argv);
        if pending.contains(row.chip) {
            if parsed.is_ok() {
                return Err(format!(
                    "chip `{}` now parses with Cli; remove it from PENDING",
                    row.chip
                ));
            }
        } else if let Err(err) = parsed {
            return Err(format!(
                "chip `{}` is not in PENDING but failed to parse: {err}",
                row.chip
            ));
        }
    }
    Ok(())
}

fn validate_chip_cases(cases: &[ChipCase], pending: &BTreeSet<&str>) -> Result<(), String> {
    let known: BTreeSet<&str> = strings::chips().iter().map(|r| r.chip).collect();
    let mut seen_keys: BTreeSet<(&str, ChipContext)> = BTreeSet::new();

    for case in cases {
        if !known.contains(case.chip) {
            return Err(format!(
                "chip case `{}` ({:?}) is not in strings::chips()",
                case.chip, case.context
            ));
        }
        if !seen_keys.insert((case.chip, case.context)) {
            return Err(format!(
                "duplicate chip case key `{}` ({:?})",
                case.chip, case.context
            ));
        }
        if !pending.contains(case.chip) {
            let argv = substitute_chip(case.chip, case.samples);
            if let Err(err) = Cli::try_parse_from(&argv) {
                return Err(format!(
                    "setup failure for chip case `{}` ({:?}) on fixture `{}` (setup {:?}, outcome {:?}): {err}",
                    case.chip, case.context, case.fixture, case.setup, case.outcome
                ));
            }
        }
    }

    for row in strings::chips() {
        if !pending.contains(row.chip) {
            let has_case = cases.iter().any(|c| c.chip == row.chip);
            if !has_case {
                return Err(format!(
                    "eligible chip `{}` has no case in tests/support/chip_cases.rs",
                    row.chip
                ));
            }
        }
    }

    for term_chip in terminal_only(cases) {
        if !known.contains(term_chip) {
            return Err(format!(
                "terminal_only entry `{term_chip}` is not an exact chip in strings::chips()"
            ));
        }
    }

    Ok(())
}

#[test]
fn every_verb_has_a_chip() {
    let cmd = Cli::command();
    if let Err(msg) = check_every_verb_has_a_chip(&cmd) {
        panic!("{msg}");
    }
}

#[test]
fn chips_parse_with_clap() {
    let pending = pending_set();
    if let Err(msg) = check_chips_parse(&pending) {
        panic!("{msg}");
    }
}

#[test]
fn every_eligible_chip_has_a_case() {
    let pending = pending_set();
    if let Err(msg) = validate_chip_cases(CASES, &pending) {
        panic!("{msg}");
    }
}

#[test]
fn new_leaf_without_chip_and_stale_pending_and_bad_cases_fail() {
    let cmd = Cli::command().subcommand(Command::new("hello"));
    let err = check_every_verb_has_a_chip(&cmd).unwrap_err();
    assert!(err.contains("hello"), "{err}");

    let check_chip = CASES[0].chip;
    let mut stale_pending = pending_set();
    stale_pending.insert(check_chip);
    let err = check_chips_parse(&stale_pending).unwrap_err();
    assert!(err.contains(check_chip), "{err}");

    let pending = pending_set();
    let bad_sample_cases = [ChipCase {
        chip: check_chip,
        context: ChipContext::Home,
        samples: &[("today", "--today 2025-02-30")],
        fixture: "tests/fixtures/corpus-mini",
        setup: &[],
        outcome: ChipOutcome::FindingsExit1("FB0100"),
    }];
    let err = validate_chip_cases(&bad_sample_cases, &pending).unwrap_err();
    assert!(err.contains(check_chip), "{err}");

    let unknown_key_cases = [
        CASES[0],
        ChipCase {
            chip: "unknown-chip-key",
            context: ChipContext::Laptop,
            samples: &[],
            fixture: "",
            setup: &[],
            outcome: ChipOutcome::RefusalExit2("FB0000"),
        },
    ];
    let err = validate_chip_cases(&unknown_key_cases, &pending).unwrap_err();
    assert!(err.contains("unknown-chip-key"), "{err}");

    let dup_cases = [
        CASES[0],
        ChipCase {
            chip: check_chip,
            context: ChipContext::Home,
            samples: &[],
            fixture: "tests/fixtures/corpus-mini",
            setup: &[],
            outcome: ChipOutcome::Terminal,
        },
    ];
    let err = validate_chip_cases(&dup_cases, &pending).unwrap_err();
    assert!(err.contains(check_chip), "{err}");

    let err = validate_chip_cases(&[], &pending).unwrap_err();
    assert!(
        err.contains("has no case in tests/support/chip_cases.rs"),
        "{err}"
    );
}

#[test]
fn add_chip_cases_refuse_unconfirmed_and_add_when_confirmed() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let add_cases: Vec<&ChipCase> = CASES
        .iter()
        .filter(|c| c.chip.starts_with("fieldbook add "))
        .collect();
    assert_eq!(add_cases.len(), 3);

    for case in add_cases {
        let fixture_path = repo_root.join(case.fixture);

        // 1. Literal base chip without --confirm-public exits 1 with FB0410 and unchanged Corpus.
        let unconfirmed_tmp = temp_copy(&fixture_path);
        for &rel in case.setup {
            std::fs::write(
                unconfirmed_tmp.path().join(rel),
                b"Synthetic plain text for chip case.\n",
            )
            .expect("write chip setup file");
        }
        let world_before = TestWorld::new(unconfirmed_tmp.path());
        let tree_before = world_before.read_tree().expect("read tree before");

        let argv_full = substitute_chip(case.chip, case.samples);
        let args: Vec<String> = argv_full.into_iter().skip(1).collect();
        let out = fieldbook::run(&args, &world_before);
        assert_eq!(out.code, 1, "unconfirmed chip `{}` must exit 1", case.chip);
        assert!(
            out.stderr.contains("FB0410"),
            "unconfirmed chip `{}` must report FB0410: {}",
            case.chip,
            out.stderr
        );
        let tree_after = world_before.read_tree().expect("read tree after");
        assert_eq!(
            tree_before, tree_after,
            "unconfirmed chip `{}` must leave Corpus unchanged",
            case.chip
        );

        // 2. Confirmed variant on another fresh copy adds successfully.
        let confirmed_tmp = temp_copy(&fixture_path);
        for &rel in case.setup {
            std::fs::write(
                confirmed_tmp.path().join(rel),
                b"Synthetic plain text for chip case.\n",
            )
            .expect("write chip setup file");
        }
        let world_confirmed = TestWorld::new(confirmed_tmp.path());
        let mut confirmed_args = args;
        confirmed_args.push("--confirm-public".to_owned());
        let out_ok = fieldbook::run(&confirmed_args, &world_confirmed);
        assert_eq!(
            out_ok.code, 0,
            "confirmed chip `{}` must exit 0: stderr={}",
            case.chip, out_ok.stderr
        );
        assert!(
            out_ok.stdout.starts_with("added "),
            "confirmed chip `{}` must print added: stdout={}",
            case.chip,
            out_ok.stdout
        );
    }
}
