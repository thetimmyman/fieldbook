//! Feature-enabled (`test-hooks`) integration tests for company policy loading, `policy show`,
//! `policy check`, `Closed` state handling, `policy.change` journaling, session refresh, and
//! Windows/Linux writer checks (`policy-load` steps 5–19).

#![cfg(feature = "test-hooks")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "integration tests copy fixtures into temporary directories, spawn the binary, and inspect files"
)]

#[path = "support/policy.rs"]
mod policy_support;
mod support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use fieldbook::io::journal::read_valid_events_prefix;
use fieldbook::io::policy::resolve_place;
use fieldbook::policy::{
    PolicyCtx, PolicySnapshot, QueuedPolicyProvider, execute_session_operation, policy_line,
};
use fieldbook::{Outcome, World};
use fieldbook_core::CivilDate;
use fieldbook_core::audit::{EventPayload, Kind, Now, Principal, Sha256, ZERO_HASH};
use fieldbook_core::check::Code;
use fieldbook_core::egress::{Delivery, Dest, Operation, Target, Transfer};
use fieldbook_core::labels::{LabelAllow, LabelId, LabelSet, PUBLIC};
use fieldbook_core::policy::{
    ClosedWhy, FileObservation, MAX_POLICY_BYTES, OVER_LIMIT_PREFIX_BYTES, OwnerSettings, Policy,
    Seen, WriterWhy, resolve, writer_check,
};

use policy_support::{WriterVariant, prepare_writer_variant, real_writers, write_policy};
use support::state::{build_state, state_mini_events};
use support::world::TestWorld;

const BASE_FIXTURE: &str = include_str!("../../../tests/fixtures/policy/policy.toml");
const LOWER_FIXTURE: &str = include_str!("../../../tests/fixtures/policy/policy-lower.toml");
const BAD_FIXTURE: &str = include_str!("../../../tests/fixtures/policy/policy-bad.toml");
const NEXT_FIXTURE: &str = include_str!("../../../tests/fixtures/policy/policy-next.toml");
const EXAMPLE_POLICY: &str = include_str!("../../../docs/intake/policy.example.toml");
const FIXTURE_TODAY: &str = include_str!("../../../tests/fixtures/corpus-mini/TODAY");

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn copy_dir_all(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("create dst dir");
    for entry in std::fs::read_dir(src).expect("read src dir") {
        let entry = entry.expect("dir entry");
        let ty = entry.file_type().expect("file type");
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst_path);
        } else {
            std::fs::copy(entry.path(), &dst_path).expect("copy file");
        }
    }
}

fn snapshot_tree_bytes(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(base: &Path, cur: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        let Ok(entries) = std::fs::read_dir(cur) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(base, &path, out);
            } else if path.is_file() {
                let rel = path
                    .strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, std::fs::read(&path).unwrap());
            }
        }
    }
    let mut map = BTreeMap::new();
    walk(dir, dir, &mut map);
    map
}

fn run_cli(world: &dyn World, args: &[&str]) -> Outcome {
    let argv: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();
    fieldbook::run(&argv, world)
}

fn setup_corpus_and_state() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let corpus = tmp.path().join("corpus-copy");
    let state = tmp.path().join("state-copy");
    copy_dir_all(&repo_root().join("tests/fixtures/corpus-mini"), &corpus);
    copy_dir_all(&repo_root().join("tests/fixtures/state-policy"), &state);
    (tmp, corpus, state)
}

fn world_with_support(corpus: &Path, state: &Path) -> TestWorld {
    let world = TestWorld::with_state(corpus, state);
    if let Ok(tree) = fieldbook::io::read_tree(corpus)
        && let Ok(cat) = fieldbook_core::manifest::SupportCatalogue::from_public_source(
            &tree,
            "1111111111111111111111111111111111111111",
        )
    {
        let (_, cat_sha) = cat.to_toml_bytes_and_sha256();
        world.set_support_catalogue(cat, cat_sha);
    }
    world
}

fn policy_change_events(state_dir: &Path) -> Vec<EventPayload> {
    read_valid_events_prefix(state_dir)
        .into_iter()
        .filter(|e| e.kind() == Kind::PolicyChange)
        .map(|e| e.payload)
        .collect()
}

// ---------------------------------------------------------------------------
// Placement & Writer tests (Steps 5, 8, 11, 16, 17)
// ---------------------------------------------------------------------------

// control: PL-01
#[test]
fn policy_flag_into_the_state_folder_is_fb0414() {
    let (_tmp, corpus, state) = setup_corpus_and_state();
    let before = snapshot_tree_bytes(&state);

    let in_state_policy = state.join("policy.toml");
    std::fs::write(&in_state_policy, BASE_FIXTURE.as_bytes()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&in_state_policy, std::fs::Permissions::from_mode(0o000)).unwrap();
    }

    let world = TestWorld::with_state(&corpus, &state);
    let out = run_cli(
        &world,
        &[
            "policy",
            "show",
            "--state",
            state.to_str().unwrap(),
            "--policy",
            in_state_policy.to_str().unwrap(),
        ],
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(&in_state_policy, std::fs::Permissions::from_mode(0o644));
    }
    let _ = std::fs::remove_file(&in_state_policy);

    assert_eq!(out.code, 2, "stderr: {}", out.stderr);
    assert!(out.stderr.contains("error[FB0414]:"), "{}", out.stderr);
    assert!(out.stderr.contains("the data folder"), "{}", out.stderr);
    assert_eq!(snapshot_tree_bytes(&state), before, "no journal line added");
}

#[test]
fn policy_flag_into_the_corpus_is_fb0414() {
    let (_tmp, corpus, state) = setup_corpus_and_state();
    let in_corpus_policy = corpus.join("policy.toml");
    std::fs::write(&in_corpus_policy, BASE_FIXTURE.as_bytes()).unwrap();

    let world = TestWorld::with_state(&corpus, &state);
    let out = run_cli(
        &world,
        &[
            "policy",
            "show",
            "--state",
            state.to_str().unwrap(),
            "--policy",
            in_corpus_policy.to_str().unwrap(),
        ],
    );
    assert_eq!(out.code, 2, "stderr: {}", out.stderr);
    assert!(out.stderr.contains("error[FB0414]:"), "{}", out.stderr);
    assert!(out.stderr.contains("the Corpus"), "{}", out.stderr);
}

#[test]
fn a_link_into_the_state_folder_is_resolved_first() {
    let (tmp, corpus, state) = setup_corpus_and_state();
    let real_in_state = state.join("policy.toml");
    std::fs::write(&real_in_state, BASE_FIXTURE.as_bytes()).unwrap();

    let link_dir = tmp.path().join("link-out");
    std::fs::create_dir_all(&link_dir).unwrap();
    let link_path = link_dir.join("linked-policy.toml");

    #[cfg(unix)]
    std::os::unix::fs::symlink(&real_in_state, &link_path).unwrap();
    #[cfg(windows)]
    {
        if std::os::windows::fs::symlink_file(&real_in_state, &link_path).is_err() {
            std::fs::hard_link(&real_in_state, &link_path).unwrap();
        }
    }

    let world = TestWorld::with_state(&corpus, &state);
    let out = run_cli(
        &world,
        &[
            "policy",
            "show",
            "--state",
            state.to_str().unwrap(),
            "--policy",
            link_path.to_str().unwrap(),
        ],
    );
    assert_eq!(out.code, 2, "stderr: {}", out.stderr);
    assert!(out.stderr.contains("error[FB0414]:"), "{}", out.stderr);
}

#[test]
fn an_absent_policy_path_is_placed_by_its_nearest_folder() {
    let (tmp, corpus, state) = setup_corpus_and_state();
    let link_dir = tmp.path().join("link-to-state");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&state, &link_dir).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&state, &link_dir).unwrap();

    let absent_under_link = link_dir.join("nonexistent-sub").join("policy.toml");
    let world = TestWorld::with_state(&corpus, &state);
    let err = resolve_place(&world, &absent_under_link, &state, &corpus).unwrap_err();
    assert_eq!(err.code(), Code::Fb0414);
}

#[test]
fn a_fixture_in_a_checkout_is_allowed_in_a_test_build() {
    let (tmp, corpus, state) = setup_corpus_and_state();
    let fake_checkout = tmp.path().join("checkout");
    std::fs::create_dir_all(fake_checkout.join(".git")).unwrap();
    let pol_dir = fake_checkout.join("fixtures");
    let pol_path = write_policy(&pol_dir, "policy.toml", BASE_FIXTURE);

    let world = TestWorld::with_state(&corpus, &state);
    let out = run_cli(
        &world,
        &[
            "policy",
            "show",
            "--state",
            state.to_str().unwrap(),
            "--policy",
            pol_path.to_str().unwrap(),
        ],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(out.stdout.starts_with("policy: loaded\n"), "{}", out.stdout);
}

// control: PL-01
#[test]
fn a_writable_policy_file_closes_the_state_fb0418() {
    let (tmp, corpus, state) = setup_corpus_and_state();
    let pol_path = prepare_writer_variant(
        &tmp.path().join("writable-pol"),
        WriterVariant::FileWritable,
        BASE_FIXTURE,
    );

    let world = TestWorld::with_state(&corpus, &state);
    let show_out = run_cli(
        &world,
        &[
            "policy",
            "show",
            "--state",
            state.to_str().unwrap(),
            "--policy",
            pol_path.to_str().unwrap(),
        ],
    );
    assert_eq!(show_out.code, 1);
    assert!(
        show_out.stdout.starts_with("policy: closed (FB0418):"),
        "{}",
        show_out.stdout
    );
    assert!(!show_out.stdout.contains("FB0413"), "{}", show_out.stdout);

    // A non-exempt verb prints the single warning line naming FB0418 and not FB0413.
    let new_out = run_cli(
        &world,
        &[
            "new",
            "question",
            "--confirm-public",
            "--area",
            "cmp",
            "Synthetic question under closed writable policy?",
            "--state",
            state.to_str().unwrap(),
            "--policy",
            pol_path.to_str().unwrap(),
        ],
    );
    let first_err_line = new_out.stderr.lines().next().unwrap_or("");
    assert!(
        first_err_line.starts_with("warning: the policy is closed (FB0418):"),
        "stderr: {}",
        new_out.stderr
    );
    assert!(!first_err_line.contains("FB0413"), "{first_err_line}");
}

#[cfg(unix)]
#[test]
fn a_group_writable_policy_file_is_allowed_on_linux() {
    use std::os::unix::fs::PermissionsExt as _;
    let (tmp, corpus, state) = setup_corpus_and_state();
    let pol_dir = tmp.path().join("group-pol");
    let pol_path = write_policy(&pol_dir, "policy.toml", BASE_FIXTURE);
    std::fs::set_permissions(&pol_path, std::fs::Permissions::from_mode(0o664)).unwrap();

    let world = TestWorld::with_state(&corpus, &state);
    let out = run_cli(
        &world,
        &[
            "policy",
            "show",
            "--state",
            state.to_str().unwrap(),
            "--policy",
            pol_path.to_str().unwrap(),
        ],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(out.stdout.starts_with("policy: loaded\n"), "{}", out.stdout);
}

#[test]
fn windows_policy_fixtures_enforce_file_and_parent_writers() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("variants");

    let normal = prepare_writer_variant(&base, WriterVariant::Normal, BASE_FIXTURE);
    assert_eq!(writer_check(&real_writers(&normal)), Ok(()));

    let file_writable = prepare_writer_variant(&base, WriterVariant::FileWritable, BASE_FIXTURE);
    assert_eq!(
        writer_check(&real_writers(&file_writable)),
        Err(WriterWhy::FileOther)
    );

    let parent_writable =
        prepare_writer_variant(&base, WriterVariant::ParentWritable, BASE_FIXTURE);
    assert_eq!(
        writer_check(&real_writers(&parent_writable)),
        Err(WriterWhy::FolderOther)
    );

    let wrong_owner = prepare_writer_variant(&base, WriterVariant::WrongOwner, BASE_FIXTURE);
    assert_eq!(
        writer_check(&real_writers(&wrong_owner)),
        Err(WriterWhy::FileOther)
    );
}

// ---------------------------------------------------------------------------
// Floor, Loaded, Closed & Journal `policy.change` tests (Steps 6, 8, 9, 14, 15)
// ---------------------------------------------------------------------------

#[test]
fn no_flag_and_no_record_is_the_floor() {
    let tmp = tempfile::tempdir().unwrap();
    let corpus = tmp.path().join("c");
    copy_dir_all(&repo_root().join("tests/fixtures/corpus-mini"), &corpus);
    let state = tmp.path().join("fresh-state");

    let world = TestWorld::with_state(&corpus, &state);
    let out = run_cli(
        &world,
        &["policy", "show", "--state", state.to_str().unwrap()],
    );
    assert_eq!(out.code, 0);
    assert!(
        out.stdout.starts_with("policy: none (floor)\n"),
        "{}",
        out.stdout
    );
    assert!(!state.exists(), "policy show must not create state folder");
}

#[test]
fn a_loaded_policy_appends_policy_change_once() {
    let (tmp, corpus, _state_pol) = setup_corpus_and_state();
    let state = tmp.path().join("state-fresh");
    build_state(&state, &state_mini_events());
    let pol_path = write_policy(&tmp.path().join("pd"), "policy.toml", BASE_FIXTURE);

    let world = TestWorld::with_state(&corpus, &state);
    let out = run_cli(
        &world,
        &[
            "new",
            "question",
            "--confirm-public",
            "--area",
            "cmp",
            "First question with loaded policy?",
            "--state",
            state.to_str().unwrap(),
            "--policy",
            pol_path.to_str().unwrap(),
        ],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);

    let changes = policy_change_events(&state);
    assert_eq!(changes.len(), 1);
    let EventPayload::PolicyChange {
        policy,
        prev,
        revision,
        outcome,
        labels_n,
        rows_n,
    } = &changes[0]
    else {
        panic!("expected PolicyChange");
    };
    assert_eq!(policy, &Sha256::digest(BASE_FIXTURE.as_bytes()));
    assert_eq!(prev.as_str(), ZERO_HASH);
    assert_eq!(revision.value(), 3);
    assert_eq!(outcome.as_str(), "loaded");
    assert_eq!(labels_n.value(), 3);
    assert_eq!(rows_n.value(), 8);
}

#[test]
fn the_second_run_appends_nothing() {
    let (tmp, corpus, state) = setup_corpus_and_state();
    let pol_path = write_policy(&tmp.path().join("pd"), "policy.toml", BASE_FIXTURE);
    let world = TestWorld::with_state(&corpus, &state);

    for q in [
        "First question with base policy?",
        "Second question with same base policy?",
    ] {
        let out = run_cli(
            &world,
            &[
                "new",
                "question",
                "--confirm-public",
                "--area",
                "cmp",
                q,
                "--state",
                state.to_str().unwrap(),
                "--policy",
                pol_path.to_str().unwrap(),
            ],
        );
        assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    }

    assert!(
        policy_change_events(&state).is_empty(),
        "state-policy already recorded BASE_FIXTURE, so neither run appends policy.change"
    );
}

#[test]
fn a_changed_file_appends_with_prev_set() {
    let (tmp, corpus, state) = setup_corpus_and_state();
    let pol_path = write_policy(&tmp.path().join("pd"), "policy.toml", NEXT_FIXTURE);
    let world = TestWorld::with_state(&corpus, &state);

    let out = run_cli(
        &world,
        &[
            "new",
            "question",
            "--confirm-public",
            "--area",
            "cmp",
            "Question under next revision policy?",
            "--state",
            state.to_str().unwrap(),
            "--policy",
            pol_path.to_str().unwrap(),
        ],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);

    let changes = policy_change_events(&state);
    assert_eq!(changes.len(), 1);
    let EventPayload::PolicyChange {
        policy,
        prev,
        revision,
        outcome,
        ..
    } = &changes[0]
    else {
        panic!("expected PolicyChange");
    };
    assert_eq!(policy, &Sha256::digest(NEXT_FIXTURE.as_bytes()));
    assert_eq!(prev, &Sha256::digest(BASE_FIXTURE.as_bytes()));
    assert_eq!(revision.value(), 4);
    assert_eq!(outcome.as_str(), "loaded");
}

// control: PL-01
#[test]
fn a_lower_revision_is_closed_and_check_still_runs() {
    let (tmp, corpus, state) = setup_corpus_and_state();
    let lower_path = write_policy(&tmp.path().join("pd"), "lower.toml", LOWER_FIXTURE);
    let world = TestWorld::with_state(&corpus, &state);

    let show_out = run_cli(
        &world,
        &[
            "policy",
            "show",
            "--state",
            state.to_str().unwrap(),
            "--policy",
            lower_path.to_str().unwrap(),
        ],
    );
    assert_eq!(show_out.code, 1);
    assert!(
        show_out.stdout.starts_with("policy: closed (FB0413):"),
        "{}",
        show_out.stdout
    );

    // check, build, and search still run and exit 0 with no closed warning.
    let check_out = run_cli(
        &world,
        &[
            "check",
            "--strict",
            "--today",
            FIXTURE_TODAY.trim(),
            "--state",
            state.to_str().unwrap(),
            "--policy",
            lower_path.to_str().unwrap(),
        ],
    );
    assert_eq!(check_out.code, 0, "stderr: {}", check_out.stderr);
    assert!(check_out.stderr.is_empty(), "{}", check_out.stderr);

    let search_out = run_cli(
        &world,
        &[
            "search",
            "S-0001",
            "--state",
            state.to_str().unwrap(),
            "--policy",
            lower_path.to_str().unwrap(),
        ],
    );
    assert_eq!(search_out.code, 0, "stderr: {}", search_out.stderr);
    assert!(
        !search_out.stderr.contains("the policy is closed"),
        "{}",
        search_out.stderr
    );

    let out_dir = tmp.path().join("built-out");
    let build_out = run_cli(
        &world,
        &[
            "build",
            "--out",
            out_dir.to_str().unwrap(),
            "--state",
            state.to_str().unwrap(),
            "--policy",
            lower_path.to_str().unwrap(),
        ],
    );
    assert_eq!(build_out.code, 0, "stderr: {}", build_out.stderr);
    assert!(
        !build_out.stderr.contains("the policy is closed"),
        "{}",
        build_out.stderr
    );

    // A send verb (backup) prints the Closed warning on stderr and refuses the send.
    let bkp_dir = tmp.path().join("bkp-out");
    let bkp_out = run_cli(
        &world,
        &[
            "backup",
            "--to",
            bkp_dir.to_str().unwrap(),
            "--state",
            state.to_str().unwrap(),
            "--policy",
            lower_path.to_str().unwrap(),
        ],
    );
    assert_ne!(bkp_out.code, 0);
    assert!(
        bkp_out
            .stderr
            .starts_with("warning: the policy is closed (FB0413):"),
        "{}",
        bkp_out.stderr
    );
}

#[test]
fn a_closed_run_appends_policy_change_once() {
    let (tmp, corpus, state) = setup_corpus_and_state();
    let bad_path = write_policy(&tmp.path().join("pd"), "bad.toml", BAD_FIXTURE);
    let world = TestWorld::with_state(&corpus, &state);

    for q in ["First closed question?", "Second closed question?"] {
        let out = run_cli(
            &world,
            &[
                "new",
                "question",
                "--confirm-public",
                "--area",
                "cmp",
                q,
                "--state",
                state.to_str().unwrap(),
                "--policy",
                bad_path.to_str().unwrap(),
            ],
        );
        assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    }

    let changes = policy_change_events(&state);
    assert_eq!(changes.len(), 1);
    let EventPayload::PolicyChange {
        outcome,
        revision,
        labels_n,
        rows_n,
        ..
    } = &changes[0]
    else {
        panic!("expected PolicyChange");
    };
    assert_eq!(outcome.as_str(), "invalid");
    assert_eq!(revision.value(), 3);
    assert_eq!(labels_n.value(), 0);
    assert_eq!(rows_n.value(), 0);
}

#[test]
fn an_absent_file_after_a_load_is_closed_and_the_record_stays() {
    let (_tmp, corpus, state) = setup_corpus_and_state();
    let seen_before = std::fs::read_to_string(state.join("policy-seen.toml")).unwrap();
    let world = TestWorld::with_state(&corpus, &state);

    let show_out = run_cli(
        &world,
        &["policy", "show", "--state", state.to_str().unwrap()],
    );
    assert_eq!(show_out.code, 1);
    assert!(
        show_out.stdout.starts_with("policy: closed (FB0413):"),
        "{}",
        show_out.stdout
    );

    let new_out = run_cli(
        &world,
        &[
            "new",
            "question",
            "--confirm-public",
            "--area",
            "cmp",
            "Question when policy file is missing after load?",
            "--state",
            state.to_str().unwrap(),
        ],
    );
    assert_eq!(new_out.code, 0);
    let seen_after = std::fs::read_to_string(state.join("policy-seen.toml")).unwrap();
    assert_eq!(seen_after, seen_before);

    let changes = policy_change_events(&state);
    assert_eq!(changes.len(), 1);
    let EventPayload::PolicyChange { outcome, .. } = &changes[0] else {
        panic!("expected PolicyChange");
    };
    assert_eq!(outcome.as_str(), "absent");
}

#[test]
fn deleting_the_record_does_not_open_a_laptop_store() {
    let (tmp, corpus, state) = setup_corpus_and_state();
    std::fs::remove_file(state.join("policy-seen.toml")).unwrap();

    // Create `overlay/store.toml` beside the state folder.
    let overlay_dir = tmp.path().join("overlay");
    std::fs::create_dir_all(&overlay_dir).unwrap();
    std::fs::write(overlay_dir.join("store.toml"), "store_schema = 1\n").unwrap();

    let world = TestWorld::with_state(&corpus, &state);
    let out = run_cli(
        &world,
        &["policy", "show", "--state", state.to_str().unwrap()],
    );
    assert_eq!(out.code, 0);
    assert!(
        out.stdout.starts_with("policy: none (floor)\n"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("destination model: allow=[]"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("destination export: allow=[]"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("company_storage_only=true"),
        "{}",
        out.stdout
    );
}

#[test]
fn read_only_verbs_write_nothing() {
    let (tmp, corpus, state) = setup_corpus_and_state();
    let pol_path = write_policy(&tmp.path().join("pd"), "policy.toml", NEXT_FIXTURE);
    let before = snapshot_tree_bytes(&state);

    let world = TestWorld::with_state(&corpus, &state);
    let s = state.to_str().unwrap();
    let p = pol_path.to_str().unwrap();
    let today = FIXTURE_TODAY.trim();

    for argv in [
        &[
            "check", "--strict", "--today", today, "--state", s, "--policy", p,
        ][..],
        &["status", "--today", today, "--state", s, "--policy", p][..],
        &["policy", "show", "--state", s, "--policy", p][..],
        &["policy", "check", p, "--state", s][..],
        &["audit", "verify", "--state", s, "--policy", p][..],
        &["audit", "show", "--state", s, "--policy", p][..],
    ] {
        let out = run_cli(&world, argv);
        assert_eq!(out.code, 0, "failed on {argv:?}: {}", out.stderr);
        assert_eq!(
            snapshot_tree_bytes(&state),
            before,
            "state folder modified by read-only verb {argv:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// `policy check` and `policy show` tests (Steps 7, 8)
// ---------------------------------------------------------------------------

#[test]
fn policy_check_reads_no_state() {
    let (tmp, corpus, _state) = setup_corpus_and_state();
    let non_existent_state = tmp.path().join("no-such-state");
    let draft = write_policy(&tmp.path().join("drafts"), "draft.toml", BASE_FIXTURE);

    let world = TestWorld::with_state(&corpus, &non_existent_state);
    let out = run_cli(
        &world,
        &[
            "policy",
            "check",
            draft.to_str().unwrap(),
            "--state",
            non_existent_state.to_str().unwrap(),
        ],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        out.stdout
            .starts_with("policy ok: revision 3, 3 labels, 8 rows\n"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("hash: "), "{}", out.stdout);
    assert!(!non_existent_state.exists());
}

#[test]
fn policy_check_reports_would_close() {
    let (tmp, corpus, state) = setup_corpus_and_state();
    let lower = write_policy(&tmp.path().join("drafts"), "lower.toml", LOWER_FIXTURE);

    let world = TestWorld::with_state(&corpus, &state);
    let out = run_cli(
        &world,
        &[
            "policy",
            "check",
            lower.to_str().unwrap(),
            "--state",
            state.to_str().unwrap(),
        ],
    );
    assert_eq!(out.code, 1);
    assert!(
        out.stderr
            .contains("would close: revision 2 is lower than the recorded 3"),
        "{}",
        out.stderr
    );
}

#[test]
fn policy_check_names_rule_and_key_path_and_no_title() {
    let (tmp, corpus, state) = setup_corpus_and_state();
    let bad_with_secret_title = r#"
policy_schema = 1
revision = 4

[[label]]
id = "np-0007"
title = "CONFIDENTIAL_SECRET_TITLE_CANARY"
restricted = true

[[store]]
id = "overlay"
holds = ["public", "np-0007"]
"#;
    let draft = write_policy(
        &tmp.path().join("drafts"),
        "bad.toml",
        bad_with_secret_title,
    );
    let world = TestWorld::with_state(&corpus, &state);
    let out = run_cli(
        &world,
        &[
            "policy",
            "check",
            draft.to_str().unwrap(),
            "--state",
            state.to_str().unwrap(),
        ],
    );
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("error[FB0413]:"), "{}", out.stderr);
    assert!(out.stderr.contains("store[0].holds"), "{}", out.stderr);
    assert!(out.stderr.contains("np-0007"), "{}", out.stderr);
    assert!(
        !out.stderr.contains("CONFIDENTIAL_SECRET_TITLE_CANARY"),
        "{}",
        out.stderr
    );
}

#[test]
fn policy_check_refuses_the_example_and_exits_1() {
    let (tmp, corpus, state) = setup_corpus_and_state();
    let draft = write_policy(&tmp.path().join("drafts"), "example.toml", EXAMPLE_POLICY);
    let world = TestWorld::with_state(&corpus, &state);
    let out = run_cli(
        &world,
        &[
            "policy",
            "check",
            draft.to_str().unwrap(),
            "--state",
            state.to_str().unwrap(),
        ],
    );
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("error[FB0413]:"), "{}", out.stderr);
    assert!(out.stderr.contains("example"), "{}", out.stderr);
    assert!(out.stderr.contains("warning: line "), "{}", out.stderr);
}

#[test]
fn policy_show_json_matches_the_golden() {
    let golden_path = repo_root().join("tests/golden/json/policy-show.json");
    let expected = std::fs::read_to_string(&golden_path).unwrap();
    let world = TestWorld::new(&repo_root().join("tests/fixtures/corpus-mini"));
    let out = run_cli(&world, &["policy", "show", "--json"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, expected);
}

#[test]
fn policy_show_exit_codes() {
    let (tmp, corpus, state) = setup_corpus_and_state();
    let fresh_state = tmp.path().join("fresh");
    let world = TestWorld::with_state(&corpus, &fresh_state);

    // Floor -> exit 0
    let floor_out = run_cli(
        &world,
        &["policy", "show", "--state", fresh_state.to_str().unwrap()],
    );
    assert_eq!(floor_out.code, 0);

    // Loaded -> exit 0
    let valid_pol = write_policy(&tmp.path().join("pd"), "policy.toml", BASE_FIXTURE);
    let loaded_out = run_cli(
        &world,
        &[
            "policy",
            "show",
            "--state",
            state.to_str().unwrap(),
            "--policy",
            valid_pol.to_str().unwrap(),
        ],
    );
    assert_eq!(loaded_out.code, 0);

    // Closed -> exit 1
    let bad_pol = write_policy(&tmp.path().join("pd2"), "bad.toml", BAD_FIXTURE);
    let closed_out = run_cli(
        &world,
        &[
            "policy",
            "show",
            "--state",
            state.to_str().unwrap(),
            "--policy",
            bad_pol.to_str().unwrap(),
        ],
    );
    assert_eq!(closed_out.code, 1);
}

// ---------------------------------------------------------------------------
// Backup, Restore & Add integration under loaded policy (Steps 8, 18)
// ---------------------------------------------------------------------------

#[test]
fn backup_to_a_target_outside_every_row_is_fb0918() {
    let (tmp, corpus, state) = setup_corpus_and_state();
    let pol_path = write_policy(&tmp.path().join("pd"), "policy.toml", BASE_FIXTURE);
    let target_dir = tmp.path().join("unlisted-backup-target");

    let world = world_with_support(&corpus, &state);
    let out = run_cli(
        &world,
        &[
            "backup",
            "--to",
            target_dir.to_str().unwrap(),
            "--state",
            state.to_str().unwrap(),
            "--policy",
            pol_path.to_str().unwrap(),
        ],
    );
    assert_eq!(out.code, 2, "stderr: {}", out.stderr);
    assert!(out.stderr.contains("FB0918"), "{}", out.stderr);
    assert!(!target_dir.exists() || std::fs::read_dir(&target_dir).unwrap().next().is_none());

    let events = read_valid_events_prefix(&state);
    assert_eq!(
        events
            .iter()
            .filter(|e| e.kind() == Kind::BackupBegin)
            .count(),
        1,
        "no new backup.begin appended beyond the 1 in state-mini"
    );
}

#[test]
fn a_backup_of_a_label_no_row_lists_is_fb0901() {
    let (tmp, corpus, state) = setup_corpus_and_state();
    let target_dir = tmp.path().join("approved-backup-target");
    std::fs::create_dir_all(&target_dir).unwrap();
    let canon_target = target_dir.canonicalize().unwrap();
    let target_posix = canon_target.to_string_lossy().replace('\\', "/");

    let custom_policy = BASE_FIXTURE.replace(
        "paths = [\"/srv/company/backups\"]",
        &format!("paths = [{target_posix:?}]"),
    );
    let pol_path = write_policy(&tmp.path().join("pd"), "policy.toml", &custom_policy);

    // Plant a page labelled `ctl-0002` in the Corpus copy.
    let page_path = corpus.join("corpus/pages/compliance/business-systems/cmp-review-steps.md");
    let page_raw = std::fs::read_to_string(&page_path).unwrap();
    let page_ctl = page_raw.replacen("handling = [\"public\"]", "handling = [\"ctl-0002\"]", 1);
    std::fs::write(&page_path, page_ctl).unwrap();

    let world = world_with_support(&corpus, &state);
    let out = run_cli(
        &world,
        &[
            "backup",
            "--to",
            target_dir.to_str().unwrap(),
            "--state",
            state.to_str().unwrap(),
            "--policy",
            pol_path.to_str().unwrap(),
        ],
    );
    assert_eq!(out.code, 2, "stderr: {}", out.stderr);
    assert!(out.stderr.contains("FB0901"), "{}", out.stderr);
    assert!(
        out.stderr
            .contains("1 item carries a label this target does not allow"),
        "{}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("Export-controlled technical data"),
        "{}",
        out.stderr
    );
    assert!(std::fs::read_dir(&target_dir).unwrap().next().is_none());
}

#[test]
fn restore_to_a_path_no_row_allows_is_refused() {
    let (tmp, corpus, state) = setup_corpus_and_state();
    let bkp_dir = tmp.path().join("snapshots");
    std::fs::create_dir_all(&bkp_dir).unwrap();

    let world = world_with_support(&corpus, &state);
    // 1. Create a snapshot at the floor first.
    std::fs::remove_file(state.join("policy-seen.toml")).unwrap();
    let bkp_out = run_cli(
        &world,
        &[
            "backup",
            "--to",
            bkp_dir.to_str().unwrap(),
            "--state",
            state.to_str().unwrap(),
        ],
    );
    assert_eq!(bkp_out.code, 0, "backup failed: {}", bkp_out.stderr);

    // 2. Now run `restore --snapshot 1 --to DIR` under BASE_FIXTURE, whose export row does not cover DIR.
    let pol_path = write_policy(&tmp.path().join("pd"), "policy.toml", BASE_FIXTURE);
    let restore_dst = tmp.path().join("unlisted-restore-dst");
    let res_out = run_cli(
        &world,
        &[
            "restore",
            "--snapshot",
            "1",
            "--from",
            bkp_dir.to_str().unwrap(),
            "--to",
            restore_dst.to_str().unwrap(),
            "--state",
            state.to_str().unwrap(),
            "--policy",
            pol_path.to_str().unwrap(),
        ],
    );
    assert_eq!(res_out.code, 2, "stderr: {}", res_out.stderr);
    assert!(res_out.stderr.contains("FB0918"), "{}", res_out.stderr);
    assert!(!restore_dst.exists());
}

#[test]
fn policy_change_carries_hash_prev_revision_outcome_and_counts() {
    let (tmp, corpus, state) = setup_corpus_and_state();
    let pol_path = write_policy(&tmp.path().join("pd"), "policy.toml", NEXT_FIXTURE);
    let world = TestWorld::with_state(&corpus, &state);

    let out = run_cli(
        &world,
        &[
            "new",
            "question",
            "--confirm-public",
            "--area",
            "cmp",
            "Question verifying policy.change fields?",
            "--state",
            state.to_str().unwrap(),
            "--policy",
            pol_path.to_str().unwrap(),
        ],
    );
    assert_eq!(out.code, 0);

    let changes = policy_change_events(&state);
    assert_eq!(changes.len(), 1);
    let EventPayload::PolicyChange {
        policy,
        prev,
        revision,
        outcome,
        labels_n,
        rows_n,
    } = &changes[0]
    else {
        panic!("expected PolicyChange");
    };
    assert_eq!(policy, &Sha256::digest(NEXT_FIXTURE.as_bytes()));
    assert_eq!(prev, &Sha256::digest(BASE_FIXTURE.as_bytes()));
    assert_eq!(revision.value(), 4);
    assert_eq!(outcome.as_str(), "loaded");
    assert_eq!(labels_n.value(), 3);
    assert_eq!(rows_n.value(), 9);
}

#[test]
fn the_journal_holds_no_title_or_banner() {
    let (tmp, corpus, state) = setup_corpus_and_state();
    let pol_path = write_policy(&tmp.path().join("pd"), "policy.toml", NEXT_FIXTURE);
    let world = TestWorld::with_state(&corpus, &state);

    let _ = run_cli(
        &world,
        &[
            "new",
            "question",
            "--confirm-public",
            "--area",
            "cmp",
            "Question checking journal has no title or banner?",
            "--state",
            state.to_str().unwrap(),
            "--policy",
            pol_path.to_str().unwrap(),
        ],
    );

    let journal_raw = std::fs::read_to_string(state.join("audit/journal.jsonl")).unwrap();
    for forbidden in [
        "Supplier terms under an NDA",
        "Bid cost build",
        "Export-controlled technical data",
        "restricted to the named counterparty",
        "proposal sensitive",
        "controlled, do not move",
    ] {
        assert!(
            !journal_raw.contains(forbidden),
            "journal leaked `{forbidden}`:\n{journal_raw}"
        );
    }
}

#[test]
fn the_recorded_sink_survives_a_closed_run() {
    let (tmp, corpus, state) = setup_corpus_and_state();
    let bad_path = write_policy(&tmp.path().join("pd"), "bad.toml", BAD_FIXTURE);
    let world = TestWorld::with_state(&corpus, &state);

    let _ = run_cli(
        &world,
        &[
            "new",
            "question",
            "--confirm-public",
            "--area",
            "cmp",
            "Question while closed?",
            "--state",
            state.to_str().unwrap(),
            "--policy",
            bad_path.to_str().unwrap(),
        ],
    );

    let seen_raw = std::fs::read_to_string(state.join("policy-seen.toml")).unwrap();
    let seen = Seen::parse(&seen_raw).expect("valid policy-seen.toml");
    assert_eq!(seen.revision, 3);
    assert_eq!(seen.audit_sink, Some(PathBuf::from("/srv/company/audit")));
}

#[test]
fn an_ai_verb_prints_the_policy_line_first() {
    let parsed = Policy::parse(BASE_FIXTURE, None).unwrap();
    let expected_prefix = parsed.hash()[..8].to_owned();
    let ctx = PolicyCtx::loaded(parsed);
    let line = policy_line(&ctx);
    assert_eq!(line, format!("policy: loaded {expected_prefix}"));

    let closed_ctx = PolicyCtx::closed(ClosedWhy::Invalid);
    let closed_line = policy_line(&closed_ctx);
    assert!(closed_line.starts_with("policy: closed "), "{closed_line}");
    assert_eq!(closed_line.len(), "policy: closed ".len() + 8);
}

// control: PL-03
#[test]
fn expiry_ignores_a_today_flag_and_a_today_variable() {
    let (tmp, corpus, state) = setup_corpus_and_state();
    let world = TestWorld::with_state(&corpus, &state);

    // `world.now().date` is 2026-03-01; yesterday is 2026-02-28.
    let expired_toml = NEXT_FIXTURE.replace("expires = 2099-12-31", "expires = 2026-02-28");
    let pol_path = write_policy(&tmp.path().join("pd"), "expired.toml", &expired_toml);

    // 1. `policy show --today 2000-01-01` is a usage error (exit 2).
    let flag_out = run_cli(
        &world,
        &[
            "policy",
            "show",
            "--today",
            "2000-01-01",
            "--state",
            state.to_str().unwrap(),
            "--policy",
            pol_path.to_str().unwrap(),
        ],
    );
    assert_eq!(flag_out.code, 2, "stderr: {}", flag_out.stderr);

    // 2. Running the binary with `FIELDBOOK_TODAY=2000-01-01` in the environment still reports the row as expired.
    let bin = env!("CARGO_BIN_EXE_fieldbook");
    let output = Command::new(bin)
        .current_dir(&corpus)
        .env("FIELDBOOK_TODAY", "2000-01-01")
        .args([
            "policy",
            "show",
            "--state",
            state.to_str().unwrap(),
            "--policy",
            pol_path.to_str().unwrap(),
        ])
        .output()
        .expect("spawn fieldbook policy show");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("expires=2026-02-28 (expired)"),
        "expected expired row in:\n{stdout}"
    );
}

#[test]
fn policy_check_does_not_read_the_clock() {
    let (tmp, corpus, state) = setup_corpus_and_state();
    let expired_toml = NEXT_FIXTURE.replace("expires = 2099-12-31", "expires = 2020-01-01");
    let draft = write_policy(&tmp.path().join("drafts"), "expired.toml", &expired_toml);

    let world = TestWorld::with_state(&corpus, &state);
    let out = run_cli(
        &world,
        &[
            "policy",
            "check",
            draft.to_str().unwrap(),
            "--state",
            state.to_str().unwrap(),
        ],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "unexpected warning: {}", out.stderr);
    assert!(
        out.stdout.starts_with("policy ok: revision 4,"),
        "{}",
        out.stdout
    );
}

#[test]
fn fixture_expiry_dates_are_far_off() {
    let min_date = CivilDate::from_ymd(2090, 1, 1).unwrap();
    let dir = repo_root().join("tests/fixtures/policy");
    for entry in std::fs::read_dir(&dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.file_name().and_then(|s| s.to_str()) == Some("policy.toml") {
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("toml") {
            continue;
        }
        let content = std::fs::read_to_string(&path).unwrap();
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("expires") {
                let rest = rest.trim_start();
                if let Some(val) = rest.strip_prefix('=') {
                    let date_str = val.trim().trim_matches('"');
                    let date = CivilDate::parse(date_str).unwrap();
                    assert!(
                        date >= min_date,
                        "{} has expires {date} before 2090-01-01",
                        path.display()
                    );
                }
            }
        }
    }
}

#[test]
fn state_policy_matches_the_writer() {
    let tmp = tempfile::tempdir().unwrap();
    let built = tmp.path().join("state-policy");
    build_state(&built, &state_mini_events());
    let seen = Seen {
        revision: 3,
        hash: Sha256::digest(BASE_FIXTURE.as_bytes()),
        audit_sink: Some(PathBuf::from("/srv/company/audit")),
    };
    std::fs::write(built.join("policy-seen.toml"), seen.to_text().as_bytes()).unwrap();

    let checked_in = repo_root().join("tests/fixtures/state-policy");
    assert_eq!(
        snapshot_tree_bytes(&built),
        snapshot_tree_bytes(&checked_in),
        "tests/fixtures/state-policy differs from build_state + Seen::to_text"
    );
}

// ---------------------------------------------------------------------------
// Step 13: Session Refresh tests (`PolicyProvider::snapshot`)
// ---------------------------------------------------------------------------

fn sample_model_dest() -> Dest {
    Dest::Model {
        name: "approved-gateway".to_owned(),
        base_url: "https://ai-gateway.synthetic.invalid/v1".to_owned(),
    }
}

fn sample_model_delivery() -> Delivery {
    let body = b"{\"prompt\":\"synthetic\"}";
    Delivery {
        entries: vec![Transfer {
            target: Target::Endpoint("https://ai-gateway.synthetic.invalid/v1/chat".to_owned()),
            operation: Operation::Post,
            sha256: Sha256::digest(body).as_str().to_owned(),
            bytes: body.len() as u64,
        }],
        markings: None,
    }
}

fn bid_label_set() -> LabelSet {
    LabelSet::from_ids([LabelId::parse("bid-0003").unwrap()])
}

fn owner_with_gateway(labels: &[LabelId]) -> OwnerSettings {
    let mut owner = OwnerSettings::unset();
    owner.endpoints.insert(
        "approved-gateway".to_owned(),
        LabelAllow::from_ids(labels.iter().cloned()),
    );
    owner
}

#[test]
fn open_session_revocation_stops_next_reply() {
    let p1 = Policy::parse(NEXT_FIXTURE, None).unwrap();
    let revoked_toml = NEXT_FIXTURE.replace(
        "allow = [\"public\", \"owner-supplied\", \"bid-0003\"]",
        "allow = [\"public\"]",
    );
    let p2 = Policy::parse(&revoked_toml, None).unwrap();

    let now = Now::new(CivilDate::from_ymd(2026, 3, 1).unwrap(), 43_200).unwrap();
    let exp_time = Now::new(CivilDate::from_ymd(2026, 3, 1).unwrap(), 40_000).unwrap();
    let bid = LabelId::parse("bid-0003").unwrap();
    let owner = owner_with_gateway(&[PUBLIC, bid]);

    let snap1 = PolicySnapshot::from_parts(
        PolicyCtx::loaded(p1),
        &owner,
        Principal::local(),
        now,
        Some(exp_time),
    );
    let snap2 = PolicySnapshot::from_parts(
        PolicyCtx::loaded(p2),
        &owner,
        Principal::local(),
        now,
        Some(exp_time),
    );

    let provider = QueuedPolicyProvider::new([snap1, snap2]);
    let dest = sample_model_dest();
    let labels = bid_label_set();
    let delivery = sample_model_delivery();

    let mut sink1 = Vec::new();
    assert!(
        execute_session_operation(&provider, &dest, &labels, &delivery, |_| {
            sink1.extend_from_slice(b"reply-1");
        })
        .is_ok()
    );
    assert_eq!(sink1, b"reply-1");

    let mut sink2 = Vec::new();
    let err = execute_session_operation(&provider, &dest, &labels, &delivery, |_| {
        sink2.extend_from_slice(b"reply-2");
    })
    .unwrap_err();
    assert_eq!(err, Code::Fb0901);
    assert!(
        sink2.is_empty(),
        "second forbidden operation produced sink bytes"
    );
}

#[test]
fn expiry_stops_next_reply() {
    let exp_toml = NEXT_FIXTURE.replace("expires = 2099-12-31", "expires = 2026-03-01");
    let p = Policy::parse(&exp_toml, None).unwrap();
    let owner = owner_with_gateway(&[PUBLIC]);

    let day1 = Now::new(CivilDate::from_ymd(2026, 3, 1).unwrap(), 43_200).unwrap();
    let day2 = Now::new(CivilDate::from_ymd(2026, 3, 2).unwrap(), 100).unwrap();

    let snap1 = PolicySnapshot::from_parts(
        PolicyCtx::loaded(p.clone()),
        &owner,
        Principal::local(),
        day1,
        None,
    );
    let snap2 =
        PolicySnapshot::from_parts(PolicyCtx::loaded(p), &owner, Principal::local(), day2, None);

    let provider = QueuedPolicyProvider::new([snap1, snap2]);
    let dest = sample_model_dest();
    let labels = LabelSet::public();
    let delivery = sample_model_delivery();

    let mut sink1 = Vec::new();
    assert!(
        execute_session_operation(&provider, &dest, &labels, &delivery, |_| {
            sink1.extend_from_slice(b"ok");
        })
        .is_ok()
    );
    assert!(!sink1.is_empty());

    let mut sink2 = Vec::new();
    let err = execute_session_operation(&provider, &dest, &labels, &delivery, |_| {
        sink2.extend_from_slice(b"expired");
    })
    .unwrap_err();
    assert_eq!(err, Code::Fb0918);
    assert!(sink2.is_empty());
}

#[test]
fn freshness_ages_during_session() {
    let p = Policy::parse(NEXT_FIXTURE, None).unwrap();
    let bid = LabelId::parse("bid-0003").unwrap();
    let owner = owner_with_gateway(&[PUBLIC, bid]);

    let export_at = Now::new(CivilDate::from_ymd(2026, 3, 1).unwrap(), 0).unwrap();
    let fresh_now = Now::new(CivilDate::from_ymd(2026, 3, 1).unwrap(), 3600).unwrap();
    // 26 hours later (> max_unshipped_hours = 24)
    let stale_now = Now::new(CivilDate::from_ymd(2026, 3, 2).unwrap(), 7200).unwrap();

    let snap1 = PolicySnapshot::from_parts(
        PolicyCtx::loaded(p.clone()),
        &owner,
        Principal::local(),
        fresh_now,
        Some(export_at),
    );
    let snap2 = PolicySnapshot::from_parts(
        PolicyCtx::loaded(p),
        &owner,
        Principal::local(),
        stale_now,
        Some(export_at),
    );

    let provider = QueuedPolicyProvider::new([snap1, snap2]);
    let dest = sample_model_dest();
    let labels = bid_label_set();
    let delivery = sample_model_delivery();

    let mut sink1 = Vec::new();
    assert!(
        execute_session_operation(&provider, &dest, &labels, &delivery, |_| {
            sink1.extend_from_slice(b"ok");
        })
        .is_ok()
    );
    assert!(!sink1.is_empty());

    let mut sink2 = Vec::new();
    let err = execute_session_operation(&provider, &dest, &labels, &delivery, |_| {
        sink2.extend_from_slice(b"stale");
    })
    .unwrap_err();
    assert_eq!(err, Code::Fb0925);
    assert!(sink2.is_empty());
}

#[test]
fn writer_permission_change_closes_session() {
    let p = Policy::parse(NEXT_FIXTURE, None).unwrap();
    let owner = owner_with_gateway(&[PUBLIC]);
    let now = Now::new(CivilDate::from_ymd(2026, 3, 1).unwrap(), 43_200).unwrap();

    let snap1 =
        PolicySnapshot::from_parts(PolicyCtx::loaded(p), &owner, Principal::local(), now, None);
    let snap2 = PolicySnapshot::from_parts(
        PolicyCtx::closed(ClosedWhy::Writable),
        &owner,
        Principal::local(),
        now,
        None,
    );

    let provider = QueuedPolicyProvider::new([snap1, snap2]);
    let dest = sample_model_dest();
    let labels = LabelSet::public();
    let delivery = sample_model_delivery();

    let mut sink1 = Vec::new();
    assert!(
        execute_session_operation(&provider, &dest, &labels, &delivery, |_| {
            sink1.extend_from_slice(b"ok");
        })
        .is_ok()
    );
    assert!(!sink1.is_empty());

    let mut sink2 = Vec::new();
    let err = execute_session_operation(&provider, &dest, &labels, &delivery, |_| {
        sink2.extend_from_slice(b"closed");
    })
    .unwrap_err();
    assert_eq!(err, Code::Fb0413);
    assert!(sink2.is_empty());
}

#[test]
fn owner_narrowing_is_refreshed() {
    let p = Policy::parse(NEXT_FIXTURE, None).unwrap();
    let bid = LabelId::parse("bid-0003").unwrap();
    let owner1 = owner_with_gateway(&[PUBLIC, bid]);
    let owner2 = owner_with_gateway(&[PUBLIC]);
    let now = Now::new(CivilDate::from_ymd(2026, 3, 1).unwrap(), 43_200).unwrap();
    let export_at = Now::new(CivilDate::from_ymd(2026, 3, 1).unwrap(), 40_000).unwrap();

    let snap1 = PolicySnapshot::from_parts(
        PolicyCtx::loaded(p.clone()),
        &owner1,
        Principal::local(),
        now,
        Some(export_at),
    );
    let snap2 = PolicySnapshot::from_parts(
        PolicyCtx::loaded(p),
        &owner2,
        Principal::local(),
        now,
        Some(export_at),
    );

    let provider = QueuedPolicyProvider::new([snap1, snap2]);
    let dest = sample_model_dest();
    let labels = bid_label_set();
    let delivery = sample_model_delivery();

    let mut sink1 = Vec::new();
    assert!(
        execute_session_operation(&provider, &dest, &labels, &delivery, |_| {
            sink1.extend_from_slice(b"ok");
        })
        .is_ok()
    );
    assert!(!sink1.is_empty());

    let mut sink2 = Vec::new();
    let err = execute_session_operation(&provider, &dest, &labels, &delivery, |_| {
        sink2.extend_from_slice(b"narrowed");
    })
    .unwrap_err();
    assert_eq!(err, Code::Fb0901);
    assert!(sink2.is_empty());
}

// ---------------------------------------------------------------------------
// Step 14 & 15: Observation transition sequence, search mode off, oversized prefix
// ---------------------------------------------------------------------------

#[test]
fn closed_observations_keep_distinct_invalid_bytes() {
    let (tmp, corpus, _state) = setup_corpus_and_state();
    let state = tmp.path().join("state-transitions");
    build_state(&state, &state_mini_events());
    let pd = tmp.path().join("pd");

    let world = TestWorld::with_state(&corpus, &state);
    let s = state.to_str().unwrap();

    // 1. Loaded (BASE_FIXTURE)
    let p_path = write_policy(&pd, "policy.toml", BASE_FIXTURE);
    let p = p_path.to_str().unwrap();
    let _ = run_cli(
        &world,
        &["search", "synthetic", "--state", s, "--policy", p],
    );
    assert_eq!(policy_change_events(&state).len(), 1);

    // 2. Invalid A
    let _ = write_policy(&pd, "policy.toml", BAD_FIXTURE);
    let _ = run_cli(&world, &["search", "review", "--state", s, "--policy", p]);
    assert_eq!(policy_change_events(&state).len(), 2);

    // 3. Identical Invalid A -> not repeated
    let _ = run_cli(&world, &["search", "window", "--state", s, "--policy", p]);
    assert_eq!(policy_change_events(&state).len(), 2);

    // 4. Changed Invalid B -> emitted!
    let _ = write_policy(
        &pd,
        "policy.toml",
        "policy_schema = 1\nrevision = 4\nbad_key_2 = 1\n",
    );
    let _ = run_cli(
        &world,
        &["search", "checklist", "--state", s, "--policy", p],
    );
    assert_eq!(policy_change_events(&state).len(), 3);

    // 5. Restored original Loaded (BASE_FIXTURE) -> emitted again!
    let _ = write_policy(&pd, "policy.toml", BASE_FIXTURE);
    let _ = run_cli(
        &world,
        &["search", "threshold", "--state", s, "--policy", p],
    );
    let changes = policy_change_events(&state);
    assert_eq!(changes.len(), 4);
    let EventPayload::PolicyChange {
        outcome, policy, ..
    } = &changes[3]
    else {
        panic!("expected PolicyChange");
    };
    assert_eq!(outcome.as_str(), "loaded");
    assert_eq!(policy, &Sha256::digest(BASE_FIXTURE.as_bytes()));

    // 6. Absent -> emitted once, repeated absence not emitted
    std::fs::remove_file(&p_path).unwrap();
    let _ = run_cli(
        &world,
        &["search", "onboarding", "--state", s, "--policy", p],
    );
    assert_eq!(policy_change_events(&state).len(), 5);
    let _ = run_cli(
        &world,
        &["search", "escalation", "--state", s, "--policy", p],
    );
    assert_eq!(policy_change_events(&state).len(), 5);

    // Last-valid sink in policy-seen.toml is unchanged while Closed
    let seen =
        Seen::parse(&std::fs::read_to_string(state.join("policy-seen.toml")).unwrap()).unwrap();
    assert_eq!(seen.revision, 3);
    assert_eq!(seen.audit_sink, Some(PathBuf::from("/srv/company/audit")));

    // Search in mode = "off" still records its ask event
    std::fs::write(
        state.join("settings.toml"),
        "role = \"lead\"\n[asked]\nmode = \"off\"\n",
    )
    .unwrap();
    let asks_before = read_valid_events_prefix(&state)
        .iter()
        .filter(|e| e.kind() == Kind::Ask)
        .count();
    let out_off = run_cli(&world, &["search", "pricing", "--state", s]);
    assert_eq!(out_off.code, 0);
    let asks_after = read_valid_events_prefix(&state)
        .iter()
        .filter(|e| e.kind() == Kind::Ask)
        .count();
    assert_eq!(asks_after, asks_before + 1);
}

#[test]
fn identical_closed_observation_is_not_repeated() {
    let obs1 = FileObservation::from_bytes(BAD_FIXTURE.as_bytes(), Ok(()));
    let obs2 = FileObservation::from_bytes(BAD_FIXTURE.as_bytes(), Ok(()));
    let res1 = resolve(obs1, None, false);
    let res2 = resolve(obs2, None, false);
    assert_eq!(res1.observation_hash(), res2.observation_hash());
}

#[test]
fn oversized_observation_hashes_only_the_bounded_prefix() {
    let mut big1 = vec![b'#'; OVER_LIMIT_PREFIX_BYTES + 64];
    let mut big2 = big1.clone();
    big1[MAX_POLICY_BYTES] = b'x';
    big2[MAX_POLICY_BYTES] = b'x';
    big2[OVER_LIMIT_PREFIX_BYTES + 10] = b'y';

    let obs1 = FileObservation::from_bytes(&big1, Ok(()));
    let obs2 = FileObservation::from_bytes(&big2, Ok(()));
    let res1 = resolve(obs1, None, false);
    let res2 = resolve(obs2, None, false);
    assert_eq!(res1.observation_hash(), res2.observation_hash());
}

#[test]
fn restoring_valid_bytes_observes_loaded_again() {
    let (tmp, corpus, _state) = setup_corpus_and_state();
    let state = tmp.path().join("state-restore-obs");
    build_state(&state, &state_mini_events());
    let pd = tmp.path().join("pd");
    let world = TestWorld::with_state(&corpus, &state);
    let s = state.to_str().unwrap();

    let p_path = write_policy(&pd, "policy.toml", BASE_FIXTURE);
    let p = p_path.to_str().unwrap();
    let _ = run_cli(
        &world,
        &["search", "synthetic", "--state", s, "--policy", p],
    );
    let _ = write_policy(&pd, "policy.toml", BAD_FIXTURE);
    let _ = run_cli(&world, &["search", "review", "--state", s, "--policy", p]);
    let _ = write_policy(&pd, "policy.toml", BASE_FIXTURE);
    let _ = run_cli(&world, &["search", "window", "--state", s, "--policy", p]);

    let changes = policy_change_events(&state);
    assert_eq!(changes.len(), 3);
    let EventPayload::PolicyChange { outcome, .. } = &changes[2] else {
        panic!("expected PolicyChange");
    };
    assert_eq!(outcome.as_str(), "loaded");
}

// ---------------------------------------------------------------------------
// Step 18: `add` saved-report classifier uses current state defaults and markings
// ---------------------------------------------------------------------------

#[test]
fn add_report_uses_current_state_defaults_and_markings() {
    let (tmp, corpus, _state) = setup_corpus_and_state();
    let state = tmp.path().join("state-add");
    build_state(&state, &state_mini_events());
    let world = TestWorld::with_state(&corpus, &state);
    let s = state.to_str().unwrap();

    // 1. Policy where State default is `bid-0003`, marking `NP-0007-FILE` maps to `np-0007`,
    //    and State holds both `bid-0003` and `np-0007`.
    let pol_admit = r#"
policy_schema = 1
revision = 4

[[label]]
id = "np-0007"
title = "Supplier terms (fixture)"
restricted = true

[[label]]
id = "bid-0003"
title = "Bid cost build (fixture)"

[[marking]]
match = "NP-0007-FILE"
labels = ["np-0007"]

[[store]]
id = "state"
holds = ["public", "unlabelled", "owner-supplied", "np-0007", "bid-0003"]
default = ["bid-0003"]
readers = 'SYNTH\Fieldbook-Readers'
"#;
    let p_admit = write_policy(&tmp.path().join("pd1"), "policy.toml", pol_admit);
    let inbox = tmp.path().join("inbox");
    std::fs::create_dir_all(&inbox).unwrap();
    let canary_file = inbox.join("NP-0007-FILE-canary.txt");
    std::fs::write(&canary_file, "Synthetic refused input body.\n").unwrap();

    // Without `--confirm-public`, `add` refuses the marked file with FB0400/FB0410 (exit 1) and
    // classifies & saves `reports/last-add.json` under `pol_admit`.
    let out1 = run_cli(
        &world,
        &[
            "add",
            canary_file.to_str().unwrap(),
            "--state",
            s,
            "--policy",
            p_admit.to_str().unwrap(),
        ],
    );
    assert_eq!(out1.code, 1, "stderr: {}", out1.stderr);
    assert!(
        !out1.stderr.contains("report not saved"),
        "expected report saved, got: {}",
        out1.stderr
    );

    let report_path = state.join("reports/last-add.json");
    let saved_json_1 = std::fs::read_to_string(&report_path).unwrap();
    let parsed_1 = fieldbook::report::parse_saved_add_report(&saved_json_1).unwrap();
    assert!(
        parsed_1
            .labels
            .contains(&LabelId::parse("bid-0003").unwrap())
    );
    assert!(
        parsed_1
            .labels
            .contains(&LabelId::parse("np-0007").unwrap())
    );

    // 2. Now load a policy where State does NOT hold `np-0007`. Running `add` on the refused
    //    `NP-0007-FILE` canary fails State admission for `last-add.json`, preserves the previous
    //    `last-add.json`, and keeps the command's original exit 1.
    let pol_deny_state = r#"
policy_schema = 1
revision = 5

[[label]]
id = "np-0007"
title = "Supplier terms (fixture)"
restricted = true

[[label]]
id = "bid-0003"
title = "Bid cost build (fixture)"

[[marking]]
match = "NP-0007-FILE"
labels = ["np-0007"]

[[store]]
id = "state"
holds = ["public", "unlabelled", "owner-supplied", "bid-0003"]
default = ["bid-0003"]
"#;
    let p_deny = write_policy(&tmp.path().join("pd2"), "policy.toml", pol_deny_state);
    let out2 = run_cli(
        &world,
        &[
            "add",
            canary_file.to_str().unwrap(),
            "--state",
            s,
            "--policy",
            p_deny.to_str().unwrap(),
        ],
    );
    assert_eq!(out2.code, 1, "stderr: {}", out2.stderr);
    assert!(out2.stderr.contains("report not saved"), "{}", out2.stderr);
    let saved_json_2 = std::fs::read_to_string(&report_path).unwrap();
    assert_eq!(
        saved_json_2, saved_json_1,
        "previous last-add.json must be preserved on State admission failure"
    );
}

// ---------------------------------------------------------------------------
// Step 19: Cross-platform acceptance smoke verification
// ---------------------------------------------------------------------------

#[test]
fn acceptance_smoke_sequence_succeeds_across_platforms() {
    let (tmp, corpus, state) = setup_corpus_and_state();
    let pd = tmp.path().join("pd");
    let p_next = write_policy(&pd, "policy.toml", NEXT_FIXTURE);
    let p_lower = write_policy(&pd, "lower.toml", LOWER_FIXTURE);

    let world = TestWorld::with_state(&corpus, &state);
    let s = state.to_str().unwrap();
    let p = p_next.to_str().unwrap();

    // 1. policy check
    let chk = run_cli(&world, &["policy", "check", p, "--state", s]);
    assert_eq!(chk.code, 0, "stderr: {}", chk.stderr);
    assert!(
        chk.stdout.starts_with("policy ok: revision 4,"),
        "{}",
        chk.stdout
    );
    assert!(chk.stdout.contains("\nhash: "), "{}", chk.stdout);

    // 2. policy show
    let show_res = run_cli(&world, &["policy", "show", "--state", s, "--policy", p]);
    assert_eq!(show_res.code, 0, "stderr: {}", show_res.stderr);
    assert!(
        show_res.stdout.starts_with("policy: loaded\n"),
        "{}",
        show_res.stdout
    );
    assert!(
        show_res.stdout.contains("writer check: ok\n"),
        "{}",
        show_res.stdout
    );
    assert!(
        show_res.stdout.contains("revision: 4\n"),
        "{}",
        show_res.stdout
    );

    // 3. new question
    let nq = run_cli(
        &world,
        &[
            "new",
            "question",
            "--confirm-public",
            "--area",
            "cmp",
            "Synthetic question, made up for tests?",
            "--state",
            s,
            "--policy",
            p,
        ],
    );
    assert_eq!(nq.code, 0, "stderr: {}", nq.stderr);
    assert!(nq.stdout.starts_with("Q-"), "{}", nq.stdout);

    // 4. audit show --kind policy.change
    let aud_show = run_cli(
        &world,
        &["audit", "show", "--state", s, "--kind", "policy.change"],
    );
    assert_eq!(aud_show.code, 0, "stderr: {}", aud_show.stderr);
    assert!(
        aud_show.stdout.contains("policy.change"),
        "{}",
        aud_show.stdout
    );
    assert!(aud_show.stdout.contains("loaded"), "{}", aud_show.stdout);

    // 5. audit verify
    let aud_ver = run_cli(&world, &["audit", "verify", "--state", s]);
    assert_eq!(aud_ver.code, 0, "stderr: {}", aud_ver.stderr);
    assert!(
        aud_ver.stdout.starts_with("journal ok:"),
        "{}",
        aud_ver.stdout
    );

    // 6. Negative control checks on the same state folder: lower.toml closes and check still runs
    let low_show = run_cli(
        &world,
        &[
            "policy",
            "show",
            "--state",
            s,
            "--policy",
            p_lower.to_str().unwrap(),
        ],
    );
    assert_eq!(low_show.code, 1);
    assert!(
        low_show.stdout.starts_with("policy: closed (FB0413):"),
        "{}",
        low_show.stdout
    );
}
