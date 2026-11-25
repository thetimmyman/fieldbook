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
use fieldbook_render::chip::VERIFY_LANDED;

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
fieldbook cite-check <file>
fieldbook new note --ai-drafted --confirm-public --area <a> --from <file>
fieldbook add <folder> --propose
fieldbook queue review
fieldbook queue accept <proposal-id>
fieldbook queue reject <proposal-id> --reason <word>
fieldbook queue flag
fieldbook ask "<question>"
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

/// Pages offer the `verify` chip only when the command exists; they show its landing line until then.
fn check_verify_landed(cmd: &Command, landed: bool) -> Result<(), String> {
    let exists = cmd.get_subcommands().any(|sub| sub.get_name() == "verify");
    match (exists, landed) {
        (true, false) => {
            Err("`verify` is in the command line: set VERIFY_LANDED to true".to_owned())
        }
        (false, true) => {
            Err("VERIFY_LANDED is true and `verify` is not in the command line".to_owned())
        }
        _ => Ok(()),
    }
}

#[test]
fn verify_landed_matches_the_command_line() {
    if let Err(msg) = check_verify_landed(&Cli::command(), VERIFY_LANDED) {
        panic!("{msg}");
    }
}

#[test]
fn verify_landed_guard_refuses_both_disagreements() {
    let without = || Command::new(strings::binary()).subcommand(Command::new("check"));
    let with = without().subcommand(Command::new("verify"));
    // `backup verify` and `audit verify` are other commands.
    let nested = without().subcommand(Command::new("backup").subcommand(Command::new("verify")));

    assert_eq!(check_verify_landed(&with, true), Ok(()));
    assert_eq!(check_verify_landed(&without(), false), Ok(()));
    assert_eq!(check_verify_landed(&nested, false), Ok(()));
    assert!(check_verify_landed(&with, false).is_err());
    assert!(check_verify_landed(&without(), true).is_err());
    assert!(check_verify_landed(&nested, true).is_err());
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
        let is_folder_chip = case.chip.contains("<folder>");

        // 1. Literal base chip without --confirm-public exits 1 with FB0410 and unchanged Corpus.
        let unconfirmed_tmp = temp_copy(&fixture_path);
        for &rel in case.setup {
            let p = unconfirmed_tmp.path().join(rel);
            if is_folder_chip {
                std::fs::create_dir_all(&p).expect("create chip setup folder");
                std::fs::write(p.join("note.txt"), b"Synthetic plain text for chip case.\n")
                    .expect("write chip setup folder file");
            } else {
                std::fs::write(p, b"Synthetic plain text for chip case.\n")
                    .expect("write chip setup file");
            }
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
        if is_folder_chip {
            assert!(
                out.stdout.contains("done: 1 refused\n"),
                "unconfirmed folder chip `{}` must print folder summary: {}",
                case.chip,
                out.stdout
            );
        }
        let tree_after = world_before.read_tree().expect("read tree after");
        assert_eq!(
            tree_before, tree_after,
            "unconfirmed chip `{}` must leave Corpus unchanged",
            case.chip
        );

        // 2. Confirmed variant on another fresh copy adds successfully.
        let confirmed_tmp = temp_copy(&fixture_path);
        for &rel in case.setup {
            let p = confirmed_tmp.path().join(rel);
            if is_folder_chip {
                std::fs::create_dir_all(&p).expect("create chip setup folder");
                std::fs::write(p.join("note.txt"), b"Synthetic plain text for chip case.\n")
                    .expect("write chip setup folder file");
            } else {
                std::fs::write(p, b"Synthetic plain text for chip case.\n")
                    .expect("write chip setup file");
            }
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
        if is_folder_chip {
            assert!(
                out_ok.stdout.contains("done: 1 added\n"),
                "confirmed folder chip `{}` must print folder summary: {}",
                case.chip,
                out_ok.stdout
            );
        }
    }
}

/// Every `new` chip runs on a fresh Home copy: exit 0 and one object on standard output.
#[test]
fn new_chip_cases_write_one_object_on_a_fresh_home_copy() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let new_cases: Vec<&ChipCase> = CASES
        .iter()
        .filter(|c| c.chip.starts_with("fieldbook new "))
        .collect();
    assert_eq!(new_cases.len(), 5);

    for case in new_cases {
        let tmp = temp_copy(&repo_root.join(case.fixture));
        let world = TestWorld::new(tmp.path());
        if let Ok(tree) = fieldbook::io::read_tree(tmp.path())
            && let Ok(cat) = fieldbook_core::manifest::SupportCatalogue::from_public_source(
                &tree,
                "1111111111111111111111111111111111111111",
            )
        {
            let (_, cat_sha) = cat.to_toml_bytes_and_sha256();
            world.set_support_catalogue(cat, cat_sha);
        }
        let args: Vec<String> = substitute_chip(case.chip, case.samples)
            .into_iter()
            .skip(1)
            .collect();
        let out = fieldbook::run(&args, &world);
        assert_eq!(out.code, 0, "chip `{}`: {}", case.chip, out.stderr);
        assert_eq!(
            out.stdout.lines().count(),
            1,
            "chip `{}` must print one object: {:?}",
            case.chip,
            out.stdout
        );
        assert!(
            out.stderr
                .lines()
                .all(|l| l.starts_with("fieldbook: no verified backup yet")),
            "chip `{}`: {}",
            case.chip,
            out.stderr
        );
    }
}

/// `new page` into an occupied id exits 2 and leaves the Corpus as it was.
#[test]
fn new_page_chip_into_an_occupied_id_exits_2_and_changes_nothing() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let case = CASES
        .iter()
        .find(|c| c.chip.starts_with("fieldbook new page "))
        .expect("a new page chip case");
    let tmp = temp_copy(&repo_root.join(case.fixture));
    let world = TestWorld::new(tmp.path());
    let args: Vec<String> = substitute_chip(case.chip, case.samples)
        .into_iter()
        .skip(1)
        .collect();
    assert_eq!(fieldbook::run(&args, &world).code, 0);
    let before = world.read_tree().expect("read tree before");

    let again = fieldbook::run(&args, &world);
    assert_eq!(again.code, 2, "{}", again.stderr);
    assert!(again.stderr.contains("already exists"), "{}", again.stderr);
    assert_eq!(world.read_tree().expect("read tree after"), before);
}

/// Every `Terminal` chip runs on a fresh Home copy whose standard input is not a terminal: it
/// prints `needs a terminal` on standard error, never succeeds and writes nothing.
#[test]
fn terminal_chip_cases_need_a_terminal_and_write_nothing() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let terminal_cases: Vec<&ChipCase> = CASES
        .iter()
        .filter(|c| matches!(c.outcome, ChipOutcome::Terminal))
        .collect();
    assert_eq!(terminal_cases.len(), 1);

    for case in terminal_cases {
        let tmp = temp_copy(&repo_root.join(case.fixture));
        let world = TestWorld::new(tmp.path());
        if let Ok(tree) = fieldbook::io::read_tree(tmp.path())
            && let Ok(cat) = fieldbook_core::manifest::SupportCatalogue::from_public_source(
                &tree,
                "1111111111111111111111111111111111111111",
            )
        {
            let (_, cat_sha) = cat.to_toml_bytes_and_sha256();
            world.set_support_catalogue(cat, cat_sha);
        }
        let before = world.read_tree().expect("read tree before");
        let args: Vec<String> = substitute_chip(case.chip, case.samples)
            .into_iter()
            .skip(1)
            .collect();
        let out = fieldbook::run(&args, &world);
        assert_ne!(out.code, 0, "chip `{}` must not succeed", case.chip);
        assert!(
            out.stderr.contains("needs a terminal"),
            "chip `{}`: {}",
            case.chip,
            out.stderr
        );
        assert!(
            out.stdout.is_empty(),
            "chip `{}`: {}",
            case.chip,
            out.stdout
        );
        assert_eq!(
            world.read_tree().expect("read tree after"),
            before,
            "chip `{}` must leave the Corpus unchanged",
            case.chip
        );
    }
}

/// Backup chip cases run on a fresh copy with a target configured outside the copy and state.
#[test]
fn backup_chip_cases_run() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let chips = fieldbook_core::strings::chips();
    let backup_chip = chips
        .iter()
        .find(|c| c.intent == "Back up the working data")
        .expect("backup chip")
        .chip;
    let backup_list_chip = chips
        .iter()
        .find(|c| c.intent == "List backups")
        .expect("backup list chip")
        .chip;
    let backup_cases: Vec<&ChipCase> = CASES
        .iter()
        .filter(|c| c.chip.starts_with(backup_chip))
        .collect();
    assert_eq!(backup_cases.len(), 3);

    for case in backup_cases {
        let tmp = temp_copy(&repo_root.join(case.fixture));
        let target_temp = tempfile::tempdir().expect("create target tempdir");
        let target_str = target_temp.path().to_string_lossy().to_string();

        let state_dir = tmp.path().join(".fieldbook");
        std::fs::create_dir_all(&state_dir).expect("create state dir");
        let settings_path = state_dir.join("settings.toml");
        let settings_content = format!(
            "role = \"lead\"\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\n",
            target_str.replace('\\', "/")
        );
        std::fs::write(&settings_path, settings_content).expect("write settings");

        let world = TestWorld::new(tmp.path());
        if let Ok(tree) = fieldbook::io::read_tree(tmp.path())
            && let Ok(cat) = fieldbook_core::manifest::SupportCatalogue::from_public_source(
                &tree,
                "1111111111111111111111111111111111111111",
            )
        {
            let (_, cat_sha) = cat.to_toml_bytes_and_sha256();
            world.set_support_catalogue(cat, cat_sha);
        }

        // Pre-run a successful backup before verify/list
        if case.chip != backup_chip {
            let backup_args = vec!["backup".to_string()];
            let pre_out = fieldbook::run(&backup_args, &world);
            assert_eq!(pre_out.code, 0, "pre-run backup failed: {}", pre_out.stderr);
        }

        // For backup list, also pre-run backup verify so it doesn't warn FB0814 "none yet"
        if case.chip == backup_list_chip {
            let verify_args = vec!["backup".to_string(), "verify".to_string()];
            let verify_out = fieldbook::run(&verify_args, &world);
            assert_eq!(
                verify_out.code, 0,
                "pre-run verify failed: {}",
                verify_out.stderr
            );
        }

        let args: Vec<String> = substitute_chip(case.chip, case.samples)
            .into_iter()
            .skip(1)
            .collect();
        let out = fieldbook::run(&args, &world);
        assert_eq!(
            out.code, 0,
            "chip `{}` failed: stderr={}",
            case.chip, out.stderr
        );
    }
}

#[expect(clippy::expect_used, reason = "test helper preparing raw copy tempdir")]
fn make_raw_copy(tree_root: &Path, state_dir: &Path) -> tempfile::TempDir {
    let raw = tempfile::tempdir().expect("raw tempdir");
    let corpus_dest = raw.path().join("corpus");
    let state_dest = raw.path().join("state");
    support::copy_dir_recursive(&tree_root.join("corpus"), &corpus_dest).expect("copy corpus");
    support::copy_dir_recursive(state_dir, &state_dest).expect("copy state");
    raw
}

#[test]
fn restore_chip_case_runs() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let restore_chip = fieldbook_core::strings::chips()
        .iter()
        .find(|c| c.intent == "Prove a backup restores")
        .expect("restore chip")
        .chip;
    let case = CASES
        .iter()
        .find(|c| c.chip == restore_chip)
        .expect("restore chip case");

    let tmp = temp_copy(&repo_root.join(case.fixture));
    let target_temp = tempfile::tempdir().expect("create target tempdir");
    let target_str = target_temp.path().to_string_lossy().to_string();

    let state_dir = tmp.path().join(".fieldbook");
    std::fs::create_dir_all(state_dir.join("audit")).expect("create state and audit dir");
    let settings_path = state_dir.join("settings.toml");
    let settings_content = format!(
        "role = \"lead\"\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\n",
        target_str.replace('\\', "/")
    );
    std::fs::write(&settings_path, settings_content).expect("write settings");

    let world = TestWorld::with_state(tmp.path(), &state_dir);
    let tree = fieldbook::io::read_tree(tmp.path()).expect("read tree");
    let cat = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    )
    .expect("from_public_source");
    let (cat_bytes, cat_sha) = cat.to_toml_bytes_and_sha256();
    world.set_support_catalogue(cat, cat_sha.clone());

    let cat_file = tmp.path().join("corpus-support.toml");
    std::fs::write(&cat_file, &cat_bytes).expect("write cat file");

    let raw = make_raw_copy(tmp.path(), &state_dir);
    let verify_args = fieldbook::backup::BackupVerifyArgs {
        target: None,
        in_dir: None,
        snapshot: None,
        raw: Some(raw.path().to_path_buf()),
        support_catalogue: Some(cat_file),
        expect_support_sha256: Some(cat_sha),
    };
    let verify_out = fieldbook::backup::run_verify(&world, &verify_args);
    assert_eq!(
        verify_out.code, 0,
        "raw verify failed: stderr={}",
        verify_out.stderr
    );

    let backup_args = vec!["backup".to_string()];
    let backup_out = fieldbook::run(&backup_args, &world);
    assert_eq!(
        backup_out.code, 0,
        "backup failed: stderr={}",
        backup_out.stderr
    );

    let args: Vec<String> = substitute_chip(case.chip, case.samples)
        .into_iter()
        .skip(1)
        .collect();
    let out = fieldbook::run(&args, &world);
    assert_eq!(out.code, 0, "restore chip failed: stderr={}", out.stderr);
    assert!(
        out.stdout
            .contains("drill passed: check clean, journal ok, fingerprint"),
        "stdout should report drill passed: {}",
        out.stdout
    );

    let drill_dir = state_dir.join("drill");
    assert!(
        !drill_dir.exists(),
        "drill folder must not exist afterwards"
    );
}
