//! `fieldbook status`: the report, the maintenance file and the state lines (`status-command`).

#![expect(
    clippy::expect_used,
    reason = "test helpers build temporary fixtures and inspect their results"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "status tests prepare temporary fixture copies and synthetic maintenance rows"
)]

#[path = "../../../tests/support/rng.rs"]
mod rng;

mod support;

use std::path::{Path, PathBuf};

use fieldbook::World;
use fieldbook_core::CivilDate;
use fieldbook_core::audit::Now;
use fieldbook_core::backup::BACKUP_TARGET_TEMPLATE;
use fieldbook_core::status::{StateLine, StateSummary, iso_week_label, parse_maintenance};
use fieldbook_core::verify::ROLE_LINE;

use support::temp_copy;
use support::world::TestWorld;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_today() -> CivilDate {
    CivilDate::parse(include_str!("../../../tests/fixtures/corpus-mini/TODAY").trim())
        .expect("valid fixture TODAY")
}

/// The fixed date the snapshot uses: the fixture date moved on by whole days until it is past
/// one claim's `review_by` (`CL-EST-0002`, 2026-07-14) and before the others (2026-07-31).
fn snapshot_today() -> CivilDate {
    let today = fixture_today()
        .add_days(136)
        .expect("fixture date plus 136 days");
    assert_eq!(today.to_string(), "2026-07-15");
    today
}

fn status_args(today: CivilDate) -> Vec<String> {
    vec!["status".to_owned(), "--today".to_owned(), today.to_string()]
}

fn write_maintenance(root: &Path, text: &str) {
    let docs = root.join("docs");
    std::fs::create_dir_all(&docs).expect("create docs folder");
    std::fs::write(docs.join("maintenance.csv"), text).expect("write maintenance.csv");
}

/// The synthetic `docs/maintenance.csv` of the snapshot test.
fn synthetic_maintenance(today: CivilDate) -> String {
    let week = iso_week_label(today);
    format!(
        "week,minutes,kind,note\n{week},45,upkeep,Fixed synthetic links\n{week},90,review,Read synthetic claims\n2026-W01,200,review,An old week that does not count\n"
    )
}

/// Writes `settings.toml` with `role` and one folder target at `target`.
fn write_backup_settings(state_dir: &Path, target: &Path, warn_after_days: u32) {
    std::fs::create_dir_all(state_dir).expect("create state folder");
    let text = format!(
        "role = \"lead\"\n\n[backup]\ndefault = \"main\"\nwarn_after_days = {warn_after_days}\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\n",
        target.to_string_lossy().replace('\\', "/")
    );
    std::fs::write(state_dir.join("settings.toml"), text).expect("write settings.toml");
}

#[test]
fn status_snapshot_lists_one_stale_claim_and_copies_its_minutes() {
    let tmp = temp_copy(&repo_root().join("tests/fixtures/corpus-mini"));
    let today = snapshot_today();
    write_maintenance(tmp.path(), &synthetic_maintenance(today));
    let world = TestWorld::new(tmp.path());

    let outcome = fieldbook::run(&status_args(today), &world);
    assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);
    assert!(
        outcome.stdout.contains(
            "CL-EST-0002  consequential claim; unreviewed; review_by 2026-07-14 has passed"
        ),
        "the stale claim is listed: {}",
        outcome.stdout
    );
    assert!(
        !outcome
            .stdout
            .contains("CL-CMP-0002  consequential claim; unreviewed; review_by"),
        "a claim before its review_by is not stale: {}",
        outcome.stdout
    );
    assert!(
        outcome
            .stdout
            .contains("Upkeep this week: 45 min (budget 30 to 60)")
    );
    assert!(
        outcome
            .stdout
            .contains("Review this week: 90 min (budget 60 to 120)")
    );
    insta::assert_snapshot!("status_on_the_fixture", outcome.stdout);
}

#[test]
fn status_lists_held_withdrawn_and_superseded_first() {
    let tmp = temp_copy(&repo_root().join("tests/fixtures/corpus-mini"));
    let registers = tmp.path().join("corpus/registers");

    // CL-CMP-0002 becomes a stale, non-consequential claim with a low id.
    let claims_path = registers.join("claims.csv");
    let claims = std::fs::read_to_string(&claims_path).expect("read claims.csv");
    let original = claims
        .lines()
        .find(|line| line.starts_with("CL-CMP-0002,"))
        .expect("CL-CMP-0002 row");
    let stale = original
        .replace(",2026-02-01,", ",2025-01-01,")
        .replace(",true,codex,unchecked,", ",false,codex,unchecked,");
    assert!(stale.contains(",2025-01-01,"));
    let held_quarantined = stale.replace("CL-CMP-0002,", "CL-CMP-0005,");
    let held_superseded = stale.replace("CL-CMP-0002,", "CL-CMP-0006,").replace(
        "S-0001,1,text_section,sec. 2,",
        "S-0002,1,text_section,sec. 2,",
    );
    let rewritten: String = claims
        .lines()
        .map(|line| {
            if line.starts_with("CL-CMP-0002,") {
                stale.clone()
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(
        &claims_path,
        format!("{rewritten}\n{held_quarantined}\n{held_superseded}\n"),
    )
    .expect("write claims.csv");

    // S-0001 version 1 is quarantined.
    let sources_path = registers.join("sources.csv");
    let sources = std::fs::read_to_string(&sources_path).expect("read sources.csv");
    let quarantined: String = sources
        .lines()
        .map(|line| {
            if line.starts_with("S-0001,1,") {
                line.replace(
                    ",2026-02-01,active,,V2,public,",
                    ",2026-02-01,quarantined,,V2,public,",
                )
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(quarantined.contains(",quarantined,"));
    std::fs::write(&sources_path, format!("{quarantined}\n")).expect("write sources.csv");

    let world = TestWorld::new(tmp.path());
    let outcome = fieldbook::run(&status_args(snapshot_today()), &world);
    assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);

    let held_quarantined = outcome
        .stdout
        .find("CL-CMP-0005  rests on S-0001-v1 (quarantined)")
        .unwrap_or_else(|| panic!("the quarantined claim is held: {}", outcome.stdout));
    let held_superseded = outcome
        .stdout
        .find("CL-CMP-0006  rests on S-0002-v1 (replaced by a newer version)")
        .unwrap_or_else(|| panic!("the superseded claim is held: {}", outcome.stdout));
    let stale = outcome
        .stdout
        .find("CL-CMP-0002  stale claim; review_by 2025-06-30")
        .unwrap_or_else(|| panic!("the stale claim is listed: {}", outcome.stdout));
    assert!(
        held_quarantined < stale,
        "held items come before stale ones"
    );
    assert!(held_superseded < stale, "held items come before stale ones");
}

#[test]
fn no_maintenance_file_prints_unavailable_and_exits_zero() {
    let tmp = temp_copy(&repo_root().join("tests/fixtures/corpus-mini"));
    let world = TestWorld::new(tmp.path());
    let outcome = fieldbook::run(&status_args(fixture_today()), &world);
    assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);
    assert!(outcome.stdout.contains("Upkeep this week: Unavailable"));
    assert!(outcome.stdout.contains("Review this week: Unavailable"));
}

#[test]
fn malformed_maintenance_row_exits_3() {
    let tmp = temp_copy(&repo_root().join("tests/fixtures/corpus-mini"));
    write_maintenance(
        tmp.path(),
        "week,minutes,kind,note\n2026-W09,30,upkeep,Fixed links\n2026-W09,90,chores,Not a kind\n",
    );
    let world = TestWorld::new(tmp.path());
    let outcome = fieldbook::run(&status_args(fixture_today()), &world);
    assert_eq!(outcome.code, 3);
    assert!(outcome.stdout.is_empty());
    assert!(
        outcome.stderr.contains("docs/maintenance.csv:3:"),
        "stderr names the line: {}",
        outcome.stderr
    );
}

#[test]
fn a_bad_today_exits_two() {
    let tmp = temp_copy(&repo_root().join("tests/fixtures/corpus-mini"));
    let world = TestWorld::new(tmp.path());
    let outcome = fieldbook::run(
        &[
            "status".to_owned(),
            "--today".to_owned(),
            "2026-02-30".to_owned(),
        ],
        &world,
    );
    assert_eq!(outcome.code, 2);
}

#[test]
fn parser_never_panics_on_200_seeded_inputs() {
    let base = synthetic_maintenance(snapshot_today());
    let mut rng = rng::Rng::new(1);
    for i in 0..200usize {
        let len = i % 129;
        let bytes = rng.bytes(len);
        let text = String::from_utf8_lossy(&bytes);
        let _ = parse_maintenance(&text);

        let mut flipped = base.clone().into_bytes();
        if !flipped.is_empty() {
            let at = i % flipped.len();
            flipped[at] ^= 1;
        }
        let _ = parse_maintenance(&String::from_utf8_lossy(&flipped));

        let mut deleted = base.clone().into_bytes();
        if !deleted.is_empty() {
            let at = i % deleted.len();
            deleted.remove(at);
        }
        let _ = parse_maintenance(&String::from_utf8_lossy(&deleted));
    }
}

/// Runs one real backup and one real verification under the world's fixed clock.
fn make_verified_backup(world: &TestWorld) {
    let outcome = fieldbook::run(&["backup".to_owned()], world);
    assert_eq!(outcome.code, 0, "backup: {}", outcome.stderr);
    let verified = fieldbook::run(&["backup".to_owned(), "verify".to_owned()], world);
    assert_eq!(verified.code, 0, "backup verify: {}", verified.stderr);
}

fn backup_state() -> (tempfile::TempDir, tempfile::TempDir, TestWorld) {
    let root = temp_copy(&repo_root().join("tests/fixtures/corpus-mini"));
    let target = tempfile::tempdir().expect("create target folder");
    write_backup_settings(&root.path().join(".fieldbook"), target.path(), 7);
    let world = TestWorld::new(root.path());
    let tree = world.read_tree().expect("read fixture tree");
    let catalogue = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    )
    .expect("the fixture has a public support catalogue");
    let (_, sha) = catalogue.to_toml_bytes_and_sha256();
    world.set_support_catalogue(catalogue, sha);
    (root, target, world)
}

#[test]
fn status_prints_the_backup_line() {
    let (_root, _target, world) = backup_state();
    make_verified_backup(&world);

    let later = fixture_today().add_days(3).expect("three days later");
    world.set_now(Now::new(later, 43_200).expect("valid now"));
    let outcome = fieldbook::run(&status_args(later), &world);
    assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);
    assert!(
        outcome
            .stdout
            .contains("Last verified backup: 3 days ago, target main (folder)"),
        "stdout: {}",
        outcome.stdout
    );
}

#[test]
fn status_omits_absent_state_lines() {
    let tmp = temp_copy(&repo_root().join("tests/fixtures/state-mini"));
    let corpus = temp_copy(&repo_root().join("tests/fixtures/corpus-mini"));
    let world = TestWorld::with_state(corpus.path(), tmp.path());
    let outcome = fieldbook::run(&status_args(fixture_today()), &world);
    assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);
    assert!(!outcome.stdout.contains("Last verified backup"));
    assert!(!outcome.stdout.contains(VERIFY_OFF_LINE));
}

const VERIFY_OFF_LINE: &str = "verify is off until a role is set";

#[test]
fn status_without_a_role_says_verify_is_off() {
    let tmp = temp_copy(&repo_root().join("tests/fixtures/state-mini"));
    // Remove the role key; the rest of the settings file stays as it was.
    let settings = tmp.path().join("settings.toml");
    let text = std::fs::read_to_string(&settings).expect("read settings.toml");
    let without: String = text
        .lines()
        .filter(|line| !line.trim_start().starts_with("role"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&settings, format!("{without}\n")).expect("write settings.toml");

    let corpus = temp_copy(&repo_root().join("tests/fixtures/corpus-mini"));
    let world = TestWorld::with_state(corpus.path(), tmp.path());
    let outcome = fieldbook::run(&status_args(fixture_today()), &world);
    assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);
    assert!(
        outcome
            .stdout
            .contains(&format!("{ROLE_LINE}\n{BACKUP_TARGET_TEMPLATE}")),
        "the two lines to add are printed byte for byte: {}",
        outcome.stdout
    );
    assert!(outcome.stdout.contains(VERIFY_OFF_LINE));
}

#[test]
fn status_backup_warning_matches_the_reminder() {
    let (root, _target, world) = backup_state();
    make_verified_backup(&world);

    let later = fixture_today().add_days(9).expect("nine days later");
    world.set_now(Now::new(later, 43_200).expect("valid now"));
    let outcome = fieldbook::run(&status_args(later), &world);
    assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);
    let reminder = fieldbook::backup::reminder(&root.path().join(".fieldbook"), later, 7)
        .expect("the reminder for a stale backup");
    assert!(
        outcome.stdout.lines().any(|line| line == reminder),
        "the warning line equals the reminder `{reminder}`: {}",
        outcome.stdout
    );
}

#[test]
fn status_prints_each_summary_line_in_order() {
    let tmp = temp_copy(&repo_root().join("tests/fixtures/corpus-mini"));
    let world = TestWorld::new(tmp.path());
    world.set_state_summary(StateSummary::new(vec![
        StateLine::new("first summary line"),
        StateLine::new("second summary line"),
    ]));
    let outcome = fieldbook::run(&status_args(fixture_today()), &world);
    assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);
    let after_actions = outcome
        .stdout
        .split_once("Next actions")
        .expect("a next actions section")
        .1;
    let first = after_actions
        .find("first summary line")
        .expect("first line");
    let second = after_actions
        .find("second summary line")
        .expect("second line");
    assert!(first < second, "summary lines keep their order");
}

#[test]
fn status_chips_run_on_a_fresh_copy() {
    let tmp = temp_copy(&repo_root().join("tests/fixtures/corpus-mini"));
    let world = TestWorld::new(tmp.path());
    let plain = fieldbook::run(&["status".to_owned()], &world);
    assert_eq!(plain.code, 0, "stderr: {}", plain.stderr);
    assert!(
        world.written().is_empty(),
        "status without --page writes nothing"
    );

    let pages = fieldbook::run(&["status".to_owned(), "--page".to_owned()], &world);
    assert_eq!(pages.code, 0, "stderr: {}", pages.stderr);
    assert_eq!(
        pages
            .stdout
            .lines()
            .filter(|line| line.starts_with("wrote pages/"))
            .count(),
        3
    );
}

#[test]
fn status_sums_large_minutes_without_wrapping() {
    let tmp = temp_copy(&repo_root().join("tests/fixtures/corpus-mini"));
    let today = fixture_today();
    let week = iso_week_label(today);
    write_maintenance(
        tmp.path(),
        &format!("week,minutes,kind,note\n{week},4294967295,upkeep,One\n{week},1,upkeep,Two\n"),
    );
    let result = fieldbook::run(&status_args(today), &TestWorld::new(tmp.path()));
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(result.stdout.contains("4294967296 min"));
}

#[test]
fn status_rejects_missing_corpus_configuration() {
    let tmp = temp_copy(&repo_root().join("tests/fixtures/corpus-mini"));
    std::fs::remove_file(tmp.path().join("corpus/corpus.toml")).expect("remove configuration");
    let result = fieldbook::run(&status_args(fixture_today()), &TestWorld::new(tmp.path()));
    assert_eq!(result.code, 1);
    assert!(result.stderr.contains("corpus/corpus.toml"));
    assert!(!result.stdout.contains("0 sources"));
}

#[test]
fn report_date_controls_backup_age() {
    let (_root, _target, world) = backup_state();
    make_verified_backup(&world);
    let later = fixture_today().add_days(3).expect("later date");
    let result = fieldbook::run(&status_args(later), &world);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(result.stdout.contains("Last verified backup: 3 days ago"));
}

#[test]
fn status_does_not_attribute_old_backup_to_newly_selected_target() {
    let (root, _old_target, world) = backup_state();
    make_verified_backup(&world);

    // Switch `settings.toml` to a brand-new empty folder target `other` that has no verified backup.
    let new_target = tempfile::tempdir().expect("create new empty target");
    let settings = format!(
        "role = \"lead\"\n\n[backup]\ndefault = \"other\"\nwarn_after_days = 7\n\n[[backup.target]]\nid = \"other\"\nkind = \"folder\"\npath = \"{}\"\n",
        new_target.path().to_string_lossy().replace('\\', "/")
    );
    std::fs::write(root.path().join(".fieldbook/settings.toml"), settings)
        .expect("write updated settings.toml");

    let later = fixture_today().add_days(3).expect("three days later");
    let outcome = fieldbook::run(&status_args(later), &world);
    assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);
    assert!(
        outcome.stdout.contains("Last verified backup: none yet"),
        "newly selected unverified target must report none yet, got:\n{}",
        outcome.stdout
    );
    assert!(
        outcome.stdout.contains(
            "fieldbook: no verified backup yet (warn after 7 days). Run: fieldbook backup"
        ),
        "newly selected unverified target must emit the unverified reminder, got:\n{}",
        outcome.stdout
    );
    assert!(
        !outcome.stdout.contains("3 days ago"),
        "must not claim old target's age for newly selected unverified target:\n{}",
        outcome.stdout
    );
    assert!(
        !outcome.stdout.contains("previously verified (unknown)"),
        "must not emit ambiguous placeholder target:\n{}",
        outcome.stdout
    );
}

#[test]
fn status_reports_verified_one_off_backup_without_configured_target() {
    let root = temp_copy(&repo_root().join("tests/fixtures/corpus-mini"));
    let state_dir = root.path().join(".fieldbook");
    std::fs::create_dir_all(&state_dir).expect("create .fieldbook");
    std::fs::write(state_dir.join("settings.toml"), "role = \"lead\"\n")
        .expect("write role-only settings without [[backup.target]]");

    let world = TestWorld::new(root.path());
    let tree = world.read_tree().expect("read fixture tree");
    let catalogue = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    )
    .expect("public support catalogue");
    let (_, sha) = catalogue.to_toml_bytes_and_sha256();
    world.set_support_catalogue(catalogue, sha);

    let one_off = tempfile::tempdir().expect("create one-off folder");
    let one_off_str = one_off.path().to_string_lossy().into_owned();
    let made = fieldbook::run(
        &["backup".to_owned(), "--to".to_owned(), one_off_str.clone()],
        &world,
    );
    assert_eq!(made.code, 0, "backup --to: {}", made.stderr);
    let verified = fieldbook::run(
        &[
            "backup".to_owned(),
            "verify".to_owned(),
            "--in".to_owned(),
            one_off_str,
        ],
        &world,
    );
    assert_eq!(verified.code, 0, "backup verify --in: {}", verified.stderr);

    let later = fixture_today().add_days(3).expect("three days later");
    let outcome = fieldbook::run(&status_args(later), &world);
    assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);
    assert!(
        outcome
            .stdout
            .contains("Last verified backup: 3 days ago, target one-off (folder)"),
        "verified one-off backup without configured target must report one-off (folder), got:\n{}",
        outcome.stdout
    );
}

#[test]
fn status_next_actions_preserve_all_finding_group_priority() {
    let tmp = temp_copy(&repo_root().join("tests/fixtures/corpus-mini"));
    let registers = tmp.path().join("corpus/registers");

    // Add both a stale non-consequential claim (`stale`) and a claim on a superseded source (`held`)
    // alongside the fixture's `XC-01` (`conflicts`), `CL-EST-0002` (`unreviewed`), and `Q-001` (`open_questions`).
    let claims_path = registers.join("claims.csv");
    let claims = std::fs::read_to_string(&claims_path).expect("read claims.csv");
    let base = claims
        .lines()
        .find(|line| line.starts_with("CL-CMP-0002,"))
        .expect("CL-CMP-0002 row");
    let stale_row = base
        .replace(",2026-02-01,", ",2025-01-01,")
        .replace(",true,codex,unchecked,", ",false,codex,unchecked,")
        .replace("CL-CMP-0002,", "CL-CMP-0005,");
    let held_row = stale_row.replace("CL-CMP-0005,", "CL-CMP-0006,").replace(
        "S-0001,1,text_section,sec. 2,",
        "S-0002,1,text_section,sec. 2,",
    );
    std::fs::write(&claims_path, format!("{claims}{stale_row}\n{held_row}\n"))
        .expect("append stale and held claims");

    let world = TestWorld::new(tmp.path());
    let all_five = fieldbook::run(&status_args(snapshot_today()), &world);
    assert_eq!(all_five.code, 0, "stderr: {}", all_five.stderr);

    let chip_for = |intent: &str| -> &'static str {
        fieldbook_core::strings::chips()
            .iter()
            .find(|row| row.intent == intent)
            .unwrap_or_else(|| panic!("missing chip for intent `{intent}`"))
            .chip
    };

    let actions_with_conflicts: Vec<&str> = all_five
        .stdout
        .split_once("Next actions\n")
        .expect("Next actions section")
        .1
        .lines()
        .take_while(|line| !line.is_empty())
        .map(str::trim)
        .collect();
    assert_eq!(
        actions_with_conflicts,
        vec![
            chip_for("Add a new version of a source"),
            chip_for("Find an id or a word; list the versions of a source"),
            chip_for("Record that the owner verified a claim"),
            chip_for("Check the Corpus"),
        ],
        "all 5 finding groups must emit exact priority order with deduplicated search"
    );

    // Resolve `XC-01` so `conflicts` is empty while `open_questions` (`Q-001`) remains populated:
    // `search` now moves from position 2 (`conflicts`) to position 4 (`open_questions`).
    let conflicts_path = registers.join("conflicts.csv");
    std::fs::write(
        &conflicts_path,
        "conflict_id,area,topic,positions,resolution,rationale,evidence,resolved_on,status,claim_ids,question_id\nXC-01,cmp,Synthetic review interval versus withholding check timing,Synthetic review runs every 30 days|Synthetic withholding check runs at each billing cycle,Use 30-day cycle,Synthetic rationale,S-0001-v1,2026-02-01,resolved,CL-CMP-0001|CL-CMP-0002,Q-001\n",
    )
    .expect("resolve XC-01");

    let without_conflicts = fieldbook::run(&status_args(snapshot_today()), &world);
    assert_eq!(
        without_conflicts.code, 0,
        "stderr: {}",
        without_conflicts.stderr
    );
    let actions_without_conflicts: Vec<&str> = without_conflicts
        .stdout
        .split_once("Next actions\n")
        .expect("Next actions section")
        .1
        .lines()
        .take_while(|line| !line.is_empty())
        .map(str::trim)
        .collect();
    assert_eq!(
        actions_without_conflicts,
        vec![
            chip_for("Add a new version of a source"),
            chip_for("Record that the owner verified a claim"),
            chip_for("Check the Corpus"),
            chip_for("Find an id or a word; list the versions of a source"),
        ],
        "without conflicts, open_questions places search 4th after check"
    );
}
